//! Compliance extensions for ViewClient.
//!
//! This module provides compliance-related methods that wrap ViewClient calls
//! and provide convenient access to compliance registry state.
//!
//! # Architecture
//!
//! The compliance system has a layered architecture:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    ViewClientComplianceExt                  │
//! │  High-level trait for compliance queries (is_asset_regulated│
//! │  get_compliance_data, etc.)                                 │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                  ComplianceDataProvider                     │
//! │  Trait for fetching compliance data (proofs, anchors, etc.) │
//! │  Two implementations:                                       │
//! │    - GrpcComplianceProvider: Fetches from ViewService gRPC  │
//! │    - MockComplianceProvider: In-memory for testing          │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                      ViewService RPC                        │
//! │  compliance_asset_status, compliance_merkle_proofs, etc.    │
//! │  Uses local trees with gRPC fallback to pd for user proofs  │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    Local Compliance Trees                   │
//! │  - compliance_user_tree (QuadTree): User registrations      │
//! │  - compliance_asset_tree (IMT): Asset registrations         │
//! │  Synced from chain via worker.rs                            │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Data Flow
//!
//! 1. **Asset proofs**: 100% local from `compliance_asset_tree` (IMT)
//!    - Regulated assets: membership proofs
//!    - Unregulated assets: non-membership proofs with protocol sink public keys
//!
//! 2. **User proofs**: Local storage + gRPC fallback
//!    - First checks `get_compliance_leaf_data()` in local storage
//!    - Falls back to pd gRPC if not found locally
//!
//! 3. **Counterparty tracking**: Enables offline user proofs
//!    - Recorded at TX build time (witness_and_build)
//!    - Backfilled during sync from historical TXs (worker.rs)

use anyhow::Result;
use futures::FutureExt;
use penumbra_sdk_asset::asset;
use penumbra_sdk_compliance::ComplianceLeaf;
use penumbra_sdk_keys::Address;
use penumbra_sdk_proto::view::v1 as view_pb;
use penumbra_sdk_tct::StateCommitment;
use std::{future::Future, pin::Pin};

use crate::ViewClient;

/// Convert a proto MerklePath to native MerklePath.
fn parse_proto_merkle_path(
    path: Option<penumbra_sdk_proto::core::component::compliance::v1::MerklePath>,
) -> penumbra_sdk_compliance::structs::MerklePath {
    use penumbra_sdk_compliance::structs::MerklePathLayer;
    match path {
        Some(p) => penumbra_sdk_compliance::structs::MerklePath {
            layers: p
                .layers
                .into_iter()
                .map(|layer| MerklePathLayer {
                    siblings: layer.siblings,
                })
                .collect(),
        },
        None => penumbra_sdk_compliance::structs::MerklePath { layers: vec![] },
    }
}

/// Compliance extensions for ViewClient.
///
/// These methods provide convenient access to compliance registry state.
pub trait ViewClientComplianceExt: ViewClient {
    /// Check if an asset is regulated (requires compliance).
    ///
    /// # Implementation
    /// Queries the compliance registry (via ViewClient::compliance_asset_status)
    /// to check if the asset is regulated.
    ///
    /// Returns `true` if the asset is registered and regulated, `false` otherwise.
    fn is_asset_regulated(
        &mut self,
        asset_id: asset::Id,
    ) -> Pin<Box<dyn Future<Output = Result<bool>> + Send + 'static>> {
        let status_future = self.compliance_asset_status(asset_id);
        async move {
            let status = status_future.await?;
            // Return true only if the asset is explicitly regulated
            Ok(status.unwrap_or(false))
        }
        .boxed()
    }

    /// Get the compliance leaf for a specific address and asset.
    ///
    /// Fetches the registered ComplianceLeaf from the chain. This ensures
    /// the leaf used in proofs matches what was actually registered on-chain.
    ///
    /// # Returns
    /// The ComplianceLeaf if the user is registered, or an error if not registered.
    fn get_compliance_leaf(
        &mut self,
        address: Address,
        asset_id: asset::Id,
    ) -> Pin<Box<dyn Future<Output = Result<ComplianceLeaf>> + Send + 'static>> {
        let leaf_future = self.compliance_user_leaf(address.clone(), asset_id);
        async move {
            let response = leaf_future.await?;

            if !response.is_registered {
                anyhow::bail!(
                    "user not registered in compliance registry for asset {}",
                    asset_id
                );
            }

            let proto_leaf = response.leaf.ok_or_else(|| {
                anyhow::anyhow!(
                    "compliance leaf missing from response for asset {} (server returned is_registered=true but no leaf)",
                    asset_id
                )
            })?;

            // Parse the proto leaf into native ComplianceLeaf
            let address: Address = proto_leaf.address.ok_or_else(|| {
                anyhow::anyhow!("compliance leaf proto: missing address field")
            })?.try_into()?;

            let asset_id: penumbra_sdk_asset::asset::Id = proto_leaf.asset_id.ok_or_else(|| {
                anyhow::anyhow!("compliance leaf proto: missing asset_id field")
            })?.try_into()?;

            let d = if proto_leaf.d.is_empty() {
                let b_d_fq = address.diversified_generator().vartime_compress_to_field();
                penumbra_sdk_compliance::derive_compliance_scalar(b_d_fq)
            } else {
                let bytes: [u8; 32] = proto_leaf.d.try_into()
                    .map_err(|_| anyhow::anyhow!("compliance leaf proto: d must be 32 bytes"))?;
                decaf377::Fq::from_bytes_checked(&bytes)
                    .map_err(|_| anyhow::anyhow!("compliance leaf proto: invalid d field element"))?
            };

            Ok(ComplianceLeaf {
                address,
                asset_id,
                d,
            })
        }
        .boxed()
    }

    /// Get the compliance tree anchors from the chain.
    ///
    /// Returns (compliance_anchor, asset_anchor) - the roots of the user tree
    /// and asset tree respectively.
    fn get_compliance_anchors(
        &mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(StateCommitment, StateCommitment)>> + Send + 'static>>
    {
        let anchors_future = self.compliance_anchors();
        async move {
            let (compliance_anchor, asset_anchor) = anchors_future.await?;
            Ok((compliance_anchor, asset_anchor))
        }
        .boxed()
    }

    /// Get the Merkle proofs needed for compliance ZK proofs.
    ///
    /// This method queries the chain for:
    /// - User's Merkle path and position in the compliance tree
    /// - Asset's Merkle path and position in the asset tree
    /// - Both tree anchors (roots)
    ///
    /// Returns a `ComplianceMerkleProofsData` with all the data needed for plans.
    fn get_compliance_merkle_proofs(
        &mut self,
        wallet_id: Address,
        asset_id: asset::Id,
    ) -> Pin<Box<dyn Future<Output = Result<ComplianceMerkleProofsData>> + Send + 'static>> {
        let proofs_future = self.compliance_merkle_proofs(wallet_id, asset_id);
        async move {
            let response = proofs_future.await?;
            ComplianceMerkleProofsData::try_from_proto(response)
        }
        .boxed()
    }
}

/// Data structure containing parsed Merkle proofs for compliance.
/// This is the Rust-native equivalent of ComplianceMerkleProofsResponse.
#[derive(Debug, Clone)]
pub struct ComplianceMerkleProofsData {
    pub user_registered: bool,
    pub asset_registered: bool,
    pub is_regulated: bool,
    pub compliance_path: penumbra_sdk_compliance::structs::MerklePath,
    pub compliance_position: u64,
    pub asset_path: penumbra_sdk_compliance::structs::MerklePath,
    pub asset_position: u64,
    pub asset_indexed_leaf: penumbra_sdk_compliance::IndexedLeaf,
    pub compliance_anchor: StateCommitment,
    pub asset_anchor: StateCommitment,
}

impl ComplianceMerkleProofsData {
    /// Convert from the proto response to native types.
    pub fn try_from_proto(response: view_pb::ComplianceMerkleProofsResponse) -> Result<Self> {
        use decaf377::Fq;

        let compliance_path = parse_proto_merkle_path(response.compliance_path);
        let asset_path = parse_proto_merkle_path(response.asset_path);

        // Parse anchors
        let compliance_anchor_bytes: [u8; 32] =
            response
                .compliance_anchor
                .try_into()
                .map_err(|v: Vec<u8>| {
                    anyhow::anyhow!("compliance_anchor must be 32 bytes, got {}", v.len())
                })?;
        let compliance_anchor = StateCommitment(
            Fq::from_bytes_checked(&compliance_anchor_bytes)
                .map_err(|e| anyhow::anyhow!("invalid compliance_anchor field element: {}", e))?,
        );

        let asset_anchor_bytes: [u8; 32] =
            response.asset_anchor.try_into().map_err(|v: Vec<u8>| {
                anyhow::anyhow!("asset_anchor must be 32 bytes, got {}", v.len())
            })?;
        let asset_anchor = StateCommitment(
            Fq::from_bytes_checked(&asset_anchor_bytes)
                .map_err(|e| anyhow::anyhow!("invalid asset_anchor field element: {}", e))?,
        );

        Ok(Self {
            user_registered: response.user_registered,
            asset_registered: response.asset_registered,
            is_regulated: response.is_regulated,
            compliance_path,
            compliance_position: response.compliance_position,
            asset_path,
            asset_position: response.asset_position,
            asset_indexed_leaf: response
                .asset_indexed_leaf
                .ok_or_else(|| {
                    anyhow::anyhow!("missing asset_indexed_leaf in compliance proofs response")
                })?
                .try_into()?,
            compliance_anchor,
            asset_anchor,
        })
    }
}

// Blanket implementation for all ViewClient implementors
impl<T: ViewClient + ?Sized> ViewClientComplianceExt for T {}

use decaf377::Fr;
use penumbra_sdk_compliance::{ComplianceProofProvider, MerklePath};
use penumbra_sdk_transaction::plan::{ActionPlan, TransactionPlan};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// A compliance proof provider backed by ViewClient.
/// Used by Planner for production transaction enrichment.
///
/// This wraps a ViewClient in a way that implements ComplianceProofProvider,
/// allowing the same enrichment logic to be shared between production (Planner)
/// and tests (mock-client).
pub struct ViewClientComplianceProvider<'a, V: ?Sized> {
    view: Arc<Mutex<&'a mut V>>,
}

impl<'a, V: ?Sized> ViewClientComplianceProvider<'a, V> {
    pub fn new(view: &'a mut V) -> Self {
        Self {
            view: Arc::new(Mutex::new(view)),
        }
    }
}

#[async_trait::async_trait]
impl<'a, V: ViewClient + Send + ?Sized> ComplianceProofProvider
    for ViewClientComplianceProvider<'a, V>
{
    async fn get_compliance_anchor(&self) -> Result<StateCommitment> {
        let future = {
            let mut view = self.view.lock().await;
            view.get_compliance_anchors()
        };
        let (compliance_anchor, _) = future.await?;
        Ok(compliance_anchor)
    }

    async fn get_asset_anchor(&self) -> Result<StateCommitment> {
        let future = {
            let mut view = self.view.lock().await;
            view.get_compliance_anchors()
        };
        let (_, asset_anchor) = future.await?;
        Ok(asset_anchor)
    }

    async fn get_asset_proof(
        &self,
        asset_id: asset::Id,
    ) -> Result<(MerklePath, u64, penumbra_sdk_compliance::IndexedLeaf, bool)> {
        // Use a dummy address - we only need asset info, not user-specific data
        let dummy_address = Address::dummy(&mut rand::thread_rng());
        let future = {
            let mut view = self.view.lock().await;
            view.get_compliance_merkle_proofs(dummy_address, asset_id)
        };
        let proofs = future.await?;

        Ok((
            proofs.asset_path,
            proofs.asset_position,
            proofs.asset_indexed_leaf,
            proofs.is_regulated,
        ))
    }

    async fn get_user_proof(
        &self,
        address: &Address,
        asset_id: asset::Id,
    ) -> Result<(MerklePath, u64, ComplianceLeaf)> {
        let proofs_future = {
            let mut view = self.view.lock().await;
            view.get_compliance_merkle_proofs(address.clone(), asset_id)
        };
        let proofs = proofs_future.await?;

        if proofs.user_registered {
            let leaf_future = {
                let mut view = self.view.lock().await;
                view.get_compliance_leaf(address.clone(), asset_id)
            };
            let leaf = leaf_future.await?;
            return Ok((proofs.compliance_path, proofs.compliance_position, leaf));
        }

        if !proofs.is_regulated {
            let b_d_fq = address.diversified_generator().vartime_compress_to_field();
            let d = penumbra_sdk_compliance::derive_compliance_scalar(b_d_fq);
            let synthetic_leaf = ComplianceLeaf {
                address: address.clone(),
                asset_id,
                d,
            };
            return Ok((MerklePath::default(), 0, synthetic_leaf));
        }

        anyhow::bail!(
            "user not registered in compliance tree for address {:?} and asset {:?}",
            address,
            asset_id
        );
    }

    async fn get_batch_proofs(
        &self,
        queries: &[(Address, asset::Id)],
    ) -> Result<penumbra_sdk_compliance::BatchComplianceData> {
        if queries.is_empty() {
            return Ok(penumbra_sdk_compliance::BatchComplianceData::default());
        }

        // Make a single batch gRPC call
        let batch_future = {
            let mut view = self.view.lock().await;
            view.compliance_batch_merkle_proofs(queries.to_vec())
        };
        let batch_response = batch_future.await?;

        // Parse anchors
        let compliance_anchor_bytes: [u8; 32] = batch_response
            .compliance_anchor
            .try_into()
            .map_err(|v: Vec<u8>| {
                anyhow::anyhow!(
                    "batch response: compliance_anchor must be 32 bytes, got {}",
                    v.len()
                )
            })?;
        let compliance_anchor = StateCommitment(
            decaf377::Fq::from_bytes_checked(&compliance_anchor_bytes)
                .map_err(|e| anyhow::anyhow!("batch response: invalid compliance_anchor: {}", e))?,
        );

        let asset_anchor_bytes: [u8; 32] =
            batch_response
                .asset_anchor
                .try_into()
                .map_err(|v: Vec<u8>| {
                    anyhow::anyhow!(
                        "batch response: asset_anchor must be 32 bytes, got {}",
                        v.len()
                    )
                })?;
        let asset_anchor = StateCommitment(
            decaf377::Fq::from_bytes_checked(&asset_anchor_bytes)
                .map_err(|e| anyhow::anyhow!("batch response: invalid asset_anchor: {}", e))?,
        );

        let mut asset_proofs: BTreeMap<
            asset::Id,
            (MerklePath, u64, penumbra_sdk_compliance::IndexedLeaf, bool),
        > = BTreeMap::new();
        let mut user_proofs: BTreeMap<(Address, asset::Id), (MerklePath, u64, ComplianceLeaf)> =
            BTreeMap::new();

        // Match results with queries - parse directly since individual results don't have anchors
        for (i, result) in batch_response.results.into_iter().enumerate() {
            use penumbra_sdk_compliance::structs::MerklePathLayer;

            let (address, asset_id) = &queries[i];

            // Parse compliance path
            let compliance_path = if let Some(path) = result.compliance_path {
                penumbra_sdk_compliance::structs::MerklePath {
                    layers: path
                        .layers
                        .into_iter()
                        .map(|layer| MerklePathLayer {
                            siblings: layer.siblings,
                        })
                        .collect(),
                }
            } else {
                penumbra_sdk_compliance::structs::MerklePath { layers: vec![] }
            };

            // Parse asset path
            let asset_path = if let Some(path) = result.asset_path {
                penumbra_sdk_compliance::structs::MerklePath {
                    layers: path
                        .layers
                        .into_iter()
                        .map(|layer| MerklePathLayer {
                            siblings: layer.siblings,
                        })
                        .collect(),
                }
            } else {
                penumbra_sdk_compliance::structs::MerklePath { layers: vec![] }
            };

            // Cache asset proof
            if !asset_proofs.contains_key(asset_id) {
                // Parse indexed_leaf from proto response using TryFrom
                let indexed_leaf = if let Some(leaf_data) = result.asset_indexed_leaf {
                    penumbra_sdk_compliance::IndexedLeaf::try_from(leaf_data).map_err(|e| {
                        anyhow::anyhow!("invalid indexed_leaf for asset {}: {}", asset_id, e)
                    })?
                } else {
                    anyhow::bail!(
                        "asset_indexed_leaf missing in batch response for asset {} \
                         (server returned incomplete data)",
                        asset_id
                    );
                };

                asset_proofs.insert(
                    *asset_id,
                    (
                        asset_path.clone(),
                        result.asset_position,
                        indexed_leaf,
                        result.is_regulated,
                    ),
                );
            }

            // Build user proof with leaf
            let key = (address.clone(), *asset_id);
            if !user_proofs.contains_key(&key) {
                if result.user_registered {
                    let leaf = if let Some(leaf_proto) = result.compliance_leaf {
                        ComplianceLeaf::try_from(leaf_proto).map_err(|e| {
                            anyhow::anyhow!(
                                "invalid compliance_leaf for address {:?} asset {}: {}",
                                address,
                                asset_id,
                                e
                            )
                        })?
                    } else {
                        let leaf_future = {
                            let mut view = self.view.lock().await;
                            view.get_compliance_leaf(address.clone(), *asset_id)
                        };
                        leaf_future.await?
                    };
                    user_proofs.insert(key, (compliance_path, result.compliance_position, leaf));
                } else if !result.is_regulated {
                    let b_d_fq = address.diversified_generator().vartime_compress_to_field();
                    let d = penumbra_sdk_compliance::derive_compliance_scalar(b_d_fq);
                    let synthetic_leaf = ComplianceLeaf {
                        address: address.clone(),
                        asset_id: *asset_id,
                        d,
                    };
                    user_proofs.insert(key, (MerklePath::default(), 0, synthetic_leaf));
                } else {
                    anyhow::bail!(
                        "user not registered in compliance tree for address {:?} and asset {:?}",
                        address,
                        asset_id
                    );
                }
            }
        }

        Ok(penumbra_sdk_compliance::BatchComplianceData {
            compliance_anchor,
            asset_anchor,
            asset_proofs,
            user_proofs,
        })
    }
}

/// Enriches a transaction plan with compliance data using a ComplianceProofProvider.
///
/// This is the canonical implementation for multi-asset transaction enrichment.
/// It handles cross-asset binding correctly by using "canonical" binding assets.
///
/// For multi-asset transactions, the binding check requires:
/// - output.counterparty_leaf_hash == spend.sender_leaf_hash
///
/// Since ComplianceLeaf includes asset_id, we use canonical binding assets:
/// - Output uses first SPEND's asset for counterparty lookup
///
/// # Parameters
/// - `plan`: The transaction plan to enrich
/// - `provider`: The compliance proof provider
/// - `rng`: Random number generator
pub async fn enrich_plan_with_compliance<P: ComplianceProofProvider>(
    plan: &mut TransactionPlan,
    provider: &P,
    rng: &mut (impl rand_core::RngCore + rand_core::CryptoRng),
    target_timestamp_override: Option<u64>,
) -> Result<()> {
    // Unix UTC timestamp for DLEQ metadata binding.
    // In production, use SystemTime::now(). Tests with fake chain times pass an override.
    let target_timestamp = match target_timestamp_override {
        Some(ts) => ts,
        None => std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| anyhow::anyhow!("system clock before Unix epoch: {}", e))?
            .as_secs(),
    };

    let mut tx_blinding_nonce = None;
    enrich_transfer_family_with_compliance(
        plan,
        provider,
        rng,
        target_timestamp,
        &mut tx_blinding_nonce,
    )
    .await?;
    enrich_shielded_ics20_withdrawals_with_compliance(
        plan,
        provider,
        rng,
        target_timestamp,
        &mut tx_blinding_nonce,
    )
    .await?;
    enrich_internal_funding_with_compliance(
        plan,
        provider,
        rng,
        target_timestamp,
        &mut tx_blinding_nonce,
    )
    .await?;

    Ok(())
}

#[derive(Clone, Copy, Debug)]
enum TransferSpendLocation {
    Transfer {
        action_index: usize,
        spend_index: usize,
    },
}

#[derive(Clone, Copy, Debug)]
enum TransferOutputLocation {
    Transfer {
        action_index: usize,
        output_index: usize,
    },
}

#[derive(Clone, Copy, Debug)]
enum ShieldedIcs20WithdrawalSpendLocation {
    ShieldedIcs20Withdrawal {
        action_index: usize,
        spend_index: usize,
    },
}

async fn fetch_batch_compliance_data<P: ComplianceProofProvider>(
    provider: &P,
    spend_identities: &[(asset::Id, Address)],
    output_identities: &[(asset::Id, Address)],
) -> Result<
    Option<(
        penumbra_sdk_compliance::BatchComplianceData,
        Address,
        asset::Id,
    )>,
> {
    use std::collections::BTreeSet;

    if spend_identities.is_empty() && output_identities.is_empty() {
        return Ok(None);
    }

    let sender_address = spend_identities
        .first()
        .map(|(_, address)| address.clone())
        .or_else(|| {
            output_identities
                .first()
                .map(|(_, address)| address.clone())
        })
        .expect("at least one spend or output identity must exist");
    let spend_binding_asset_id = spend_identities
        .first()
        .map(|(asset_id, _)| *asset_id)
        .or_else(|| output_identities.first().map(|(asset_id, _)| *asset_id))
        .expect("at least one spend or output identity must exist");

    let mut queries: BTreeSet<(Address, asset::Id)> = BTreeSet::new();
    for (asset_id, address) in spend_identities {
        queries.insert((address.clone(), *asset_id));
    }
    for (asset_id, address) in output_identities {
        queries.insert((address.clone(), *asset_id));
        queries.insert((sender_address.clone(), spend_binding_asset_id));
    }

    let query_vec: Vec<_> = queries.into_iter().collect();
    let batch_data = provider.get_batch_proofs(&query_vec).await?;
    Ok(Some((batch_data, sender_address, spend_binding_asset_id)))
}

async fn enrich_transfer_family_with_compliance<P: ComplianceProofProvider>(
    plan: &mut TransactionPlan,
    provider: &P,
    rng: &mut (impl rand_core::RngCore + rand_core::CryptoRng),
    target_timestamp: u64,
    tx_blinding_nonce: &mut Option<Fr>,
) -> Result<()> {
    let mut spend_locations = Vec::new();
    let mut output_locations = Vec::new();

    for (action_index, action) in plan.actions.iter().enumerate() {
        match action {
            ActionPlan::Transfer(transfer) => {
                for spend_index in 0..transfer.spends.len() {
                    spend_locations.push(TransferSpendLocation::Transfer {
                        action_index,
                        spend_index,
                    });
                }
                for output_index in 0..transfer.outputs.len() {
                    output_locations.push(TransferOutputLocation::Transfer {
                        action_index,
                        output_index,
                    });
                }
            }
            ActionPlan::Consolidate(_) | ActionPlan::Split(_) => {}
            _ => {}
        }
    }

    let spend_identities = spend_locations
        .iter()
        .map(|location| match *location {
            TransferSpendLocation::Transfer {
                action_index,
                spend_index,
            } => {
                let ActionPlan::Transfer(transfer) = &plan.actions[action_index] else {
                    unreachable!()
                };
                let spend = &transfer.spends[spend_index];
                (spend.note.asset_id(), spend.note.address())
            }
        })
        .collect::<Vec<_>>();
    let output_identities = output_locations
        .iter()
        .map(|location| match *location {
            TransferOutputLocation::Transfer {
                action_index,
                output_index,
            } => {
                let ActionPlan::Transfer(transfer) = &plan.actions[action_index] else {
                    unreachable!()
                };
                let output = &transfer.outputs[output_index];
                (output.value.asset_id, output.dest_address.clone())
            }
        })
        .collect::<Vec<_>>();

    let Some((batch_data, sender_address, spend_binding_asset_id)) =
        fetch_batch_compliance_data(provider, &spend_identities, &output_identities).await?
    else {
        return Ok(());
    };
    let compliance_anchor = batch_data.compliance_anchor;
    let asset_anchor = batch_data.asset_anchor;

    for (spend_location, (spend_asset_id, spend_address)) in spend_locations
        .iter()
        .copied()
        .zip(spend_identities.iter().cloned())
    {
        let TransferSpendLocation::Transfer {
            action_index,
            spend_index,
        } = spend_location;

        let (asset_path, asset_position, asset_indexed_leaf, is_regulated) = batch_data
            .asset_proofs
            .get(&spend_asset_id)
            .cloned()
            .unwrap_or_else(default_unregulated_asset_proof);

        let (sender_compliance_path, sender_compliance_position, _) = batch_data
            .user_proofs
            .get(&(spend_address.clone(), spend_asset_id))
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "missing user proof for transfer spend at action {} input {}: \
                     user may not be registered for asset {} \
                     (check compliance registration status)",
                    action_index,
                    spend_index,
                    spend_asset_id
                )
            })?;

        let ActionPlan::Transfer(transfer) = &mut plan.actions[action_index] else {
            unreachable!()
        };
        let spend = &mut transfer.spends[spend_index];
        spend.asset_indexed_leaf = asset_indexed_leaf;
        spend.asset_path = asset_path;
        spend.asset_position = asset_position;
        spend.asset_anchor = asset_anchor;
        spend.compliance_anchor = compliance_anchor;
        spend.compliance_path = sender_compliance_path;
        spend.compliance_position = sender_compliance_position;
        spend.is_regulated = is_regulated;
        spend.target_timestamp = target_timestamp;
        spend.set_compliance_details(rng)?;
        if let Some(nonce) = *tx_blinding_nonce {
            spend.tx_blinding_nonce = nonce;
        } else {
            *tx_blinding_nonce = Some(spend.tx_blinding_nonce);
        }
    }

    if !output_locations.is_empty() {
        let nonce = tx_blinding_nonce.unwrap_or_else(|| Fr::rand(rng));
        *tx_blinding_nonce = Some(nonce);

        for (output_location, (output_asset_id, recipient_address)) in output_locations
            .iter()
            .copied()
            .zip(output_identities.iter().cloned())
        {
            let TransferOutputLocation::Transfer {
                action_index,
                output_index,
            } = output_location;

            let (asset_path, asset_position, asset_indexed_leaf, is_regulated) = batch_data
                .asset_proofs
                .get(&output_asset_id)
                .cloned()
                .unwrap_or_else(default_unregulated_asset_proof);

            let (recipient_compliance_path, recipient_compliance_position, recipient_leaf) =
                batch_data
                    .user_proofs
                    .get(&(recipient_address.clone(), output_asset_id))
                    .cloned()
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing user proof for transfer output at action {} output {}: \
                             recipient may not be registered for asset {} \
                             (recipient must be registered for regulated assets)",
                            action_index,
                            output_index,
                            output_asset_id
                        )
                    })?;

            let (_, _, sender_leaf_for_output) = batch_data
                .user_proofs
                .get(&(sender_address.clone(), spend_binding_asset_id))
                .cloned()
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "missing user proof for transfer sender binding: \
                         sender may not be registered for asset {} \
                         (sender must be registered for regulated assets)",
                        spend_binding_asset_id
                    )
                })?;

            let ActionPlan::Transfer(transfer) = &mut plan.actions[action_index] else {
                unreachable!()
            };
            let output = &mut transfer.outputs[output_index];
            output.asset_indexed_leaf = asset_indexed_leaf;
            output.asset_path = asset_path;
            output.asset_position = asset_position;
            output.asset_anchor = asset_anchor;
            output.compliance_anchor = compliance_anchor;
            output.compliance_path = recipient_compliance_path;
            output.compliance_position = recipient_compliance_position;
            output.is_regulated = is_regulated;
            output.target_timestamp = target_timestamp;
            output.set_compliance_details(rng, &recipient_leaf, sender_leaf_for_output, nonce)?;
        }
    }

    Ok(())
}

async fn enrich_internal_funding_with_compliance<P: ComplianceProofProvider>(
    plan: &mut TransactionPlan,
    provider: &P,
    rng: &mut (impl rand_core::RngCore + rand_core::CryptoRng),
    target_timestamp: u64,
    tx_blinding_nonce: &mut Option<Fr>,
) -> Result<()> {
    let Some(fee_funding) = plan.fee_funding.as_mut() else {
        return Ok(());
    };

    let spend_identities = fee_funding
        .transfer
        .spends
        .iter()
        .map(|spend| (spend.note.asset_id(), spend.note.address()))
        .collect::<Vec<_>>();
    let output_identities = fee_funding
        .transfer
        .outputs
        .iter()
        .map(|output| (output.value.asset_id, output.dest_address.clone()))
        .collect::<Vec<_>>();

    let Some((batch_data, sender_address, spend_binding_asset_id)) =
        fetch_batch_compliance_data(provider, &spend_identities, &output_identities).await?
    else {
        return Ok(());
    };
    let compliance_anchor = batch_data.compliance_anchor;
    let asset_anchor = batch_data.asset_anchor;

    for (spend, (spend_asset_id, spend_address)) in fee_funding
        .transfer
        .spends
        .iter_mut()
        .zip(spend_identities.iter().cloned())
    {
        let (asset_path, asset_position, asset_indexed_leaf, is_regulated) = batch_data
            .asset_proofs
            .get(&spend_asset_id)
            .cloned()
            .unwrap_or_else(default_unregulated_asset_proof);

        let (sender_compliance_path, sender_compliance_position, _) = batch_data
            .user_proofs
            .get(&(spend_address.clone(), spend_asset_id))
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "missing user proof for fee funding spend: \
                     user may not be registered for asset {}",
                    spend_asset_id
                )
            })?;

        spend.asset_indexed_leaf = asset_indexed_leaf;
        spend.asset_path = asset_path;
        spend.asset_position = asset_position;
        spend.asset_anchor = asset_anchor;
        spend.compliance_anchor = compliance_anchor;
        spend.compliance_path = sender_compliance_path;
        spend.compliance_position = sender_compliance_position;
        spend.is_regulated = is_regulated;
        spend.target_timestamp = target_timestamp;
        spend.set_compliance_details(rng)?;
        if let Some(nonce) = *tx_blinding_nonce {
            spend.tx_blinding_nonce = nonce;
        } else {
            *tx_blinding_nonce = Some(spend.tx_blinding_nonce);
        }
    }

    if !fee_funding.transfer.outputs.is_empty() {
        let nonce = tx_blinding_nonce.unwrap_or_else(|| Fr::rand(rng));
        *tx_blinding_nonce = Some(nonce);

        for (output, (output_asset_id, recipient_address)) in fee_funding
            .transfer
            .outputs
            .iter_mut()
            .zip(output_identities.iter().cloned())
        {
            let (asset_path, asset_position, asset_indexed_leaf, is_regulated) = batch_data
                .asset_proofs
                .get(&output_asset_id)
                .cloned()
                .unwrap_or_else(default_unregulated_asset_proof);

            let (recipient_compliance_path, recipient_compliance_position, recipient_leaf) =
                batch_data
                    .user_proofs
                    .get(&(recipient_address.clone(), output_asset_id))
                    .cloned()
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing user proof for fee funding output: \
                             recipient may not be registered for asset {}",
                            output_asset_id
                        )
                    })?;

            let (_, _, sender_leaf_for_output) = batch_data
                .user_proofs
                .get(&(sender_address.clone(), spend_binding_asset_id))
                .cloned()
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "missing user proof for fee funding sender binding: \
                         sender may not be registered for asset {}",
                        spend_binding_asset_id
                    )
                })?;

            output.asset_indexed_leaf = asset_indexed_leaf;
            output.asset_path = asset_path;
            output.asset_position = asset_position;
            output.asset_anchor = asset_anchor;
            output.compliance_anchor = compliance_anchor;
            output.compliance_path = recipient_compliance_path;
            output.compliance_position = recipient_compliance_position;
            output.is_regulated = is_regulated;
            output.target_timestamp = target_timestamp;
            output.set_compliance_details(rng, &recipient_leaf, sender_leaf_for_output, nonce)?;
        }
    }

    Ok(())
}

async fn enrich_shielded_ics20_withdrawals_with_compliance<P: ComplianceProofProvider>(
    plan: &mut TransactionPlan,
    provider: &P,
    rng: &mut (impl rand_core::RngCore + rand_core::CryptoRng),
    target_timestamp: u64,
    tx_blinding_nonce: &mut Option<Fr>,
) -> Result<()> {
    let mut spend_locations = Vec::new();
    let mut action_indices = Vec::new();

    for (action_index, action) in plan.actions.iter().enumerate() {
        if let ActionPlan::ShieldedIcs20Withdrawal(withdrawal) = action {
            action_indices.push(action_index);
            for spend_index in 0..withdrawal.spends.len() {
                spend_locations.push(
                    ShieldedIcs20WithdrawalSpendLocation::ShieldedIcs20Withdrawal {
                        action_index,
                        spend_index,
                    },
                );
            }
        }
    }

    let spend_identities = spend_locations
        .iter()
        .map(|location| match *location {
            ShieldedIcs20WithdrawalSpendLocation::ShieldedIcs20Withdrawal {
                action_index,
                spend_index,
            } => {
                let ActionPlan::ShieldedIcs20Withdrawal(withdrawal) = &plan.actions[action_index]
                else {
                    unreachable!()
                };
                let spend = &withdrawal.spends[spend_index];
                (spend.note.asset_id(), spend.note.address())
            }
        })
        .collect::<Vec<_>>();

    let Some((batch_data, _, _)) =
        fetch_batch_compliance_data(provider, &spend_identities, &[]).await?
    else {
        return Ok(());
    };
    let compliance_anchor = batch_data.compliance_anchor;
    let asset_anchor = batch_data.asset_anchor;

    for (spend_location, (spend_asset_id, spend_address)) in spend_locations
        .iter()
        .copied()
        .zip(spend_identities.iter().cloned())
    {
        let ShieldedIcs20WithdrawalSpendLocation::ShieldedIcs20Withdrawal {
            action_index,
            spend_index,
        } = spend_location;

        let (asset_path, asset_position, asset_indexed_leaf, is_regulated) = batch_data
            .asset_proofs
            .get(&spend_asset_id)
            .cloned()
            .unwrap_or_else(default_unregulated_asset_proof);

        let (sender_compliance_path, sender_compliance_position, _) = batch_data
            .user_proofs
            .get(&(spend_address.clone(), spend_asset_id))
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "missing user proof for shielded ICS-20 withdrawal spend at action {} input {}: \
                     user may not be registered for asset {} \
                     (check compliance registration status)",
                    action_index,
                    spend_index,
                    spend_asset_id
                )
            })?;

        let ActionPlan::ShieldedIcs20Withdrawal(withdrawal) = &mut plan.actions[action_index]
        else {
            unreachable!()
        };
        let spend = &mut withdrawal.spends[spend_index];
        spend.asset_indexed_leaf = asset_indexed_leaf;
        spend.asset_path = asset_path;
        spend.asset_position = asset_position;
        spend.asset_anchor = asset_anchor;
        spend.compliance_anchor = compliance_anchor;
        spend.compliance_path = sender_compliance_path;
        spend.compliance_position = sender_compliance_position;
        spend.is_regulated = is_regulated;
        spend.target_timestamp = target_timestamp;
        spend.set_compliance_details(rng)?;
        if let Some(nonce) = *tx_blinding_nonce {
            spend.tx_blinding_nonce = nonce;
        } else {
            *tx_blinding_nonce = Some(spend.tx_blinding_nonce);
        }
    }

    for action_index in action_indices {
        let ActionPlan::ShieldedIcs20Withdrawal(withdrawal) = &mut plan.actions[action_index]
        else {
            unreachable!()
        };
        let Some(first_spend) = withdrawal.spends.first() else {
            continue;
        };

        withdrawal.body.target_timestamp = first_spend.target_timestamp;
        withdrawal.body.compliance_anchor = first_spend.compliance_anchor;
        withdrawal.body.asset_anchor = first_spend.asset_anchor;

        if first_spend.is_regulated
            && !first_spend.compliance_ciphertext.is_empty()
            && !penumbra_sdk_compliance::IbcComplianceMetadata::is_compliance_memo(
                &withdrawal.withdrawal.ics20_memo,
            )
        {
            let metadata = penumbra_sdk_compliance::IbcComplianceMetadata {
                compliance_ciphertext: first_spend.compliance_ciphertext.clone(),
                asset_id: first_spend.note.asset_id(),
            };
            withdrawal.withdrawal.ics20_memo =
                metadata.encode_to_memo(&withdrawal.withdrawal.ics20_memo)?;
        }

        withdrawal.body.withdrawal = withdrawal.withdrawal.clone();
    }

    Ok(())
}

fn default_unregulated_asset_proof() -> (MerklePath, u64, penumbra_sdk_compliance::IndexedLeaf, bool)
{
    let default_leaf = penumbra_sdk_compliance::IndexedLeaf::with_default_policy(
        decaf377::Fq::from(0u64),
        0,
        penumbra_sdk_compliance::indexed_tree::FQ_MAX.clone(),
    );
    (MerklePath::default(), 0, default_leaf, false)
}
