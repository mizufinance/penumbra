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
//!    - Unregulated assets: non-membership proofs via BLACK_HOLE_ACK
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
use penumbra_sdk_keys::{keys::AddressComplianceKey, Address};
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

            let key_proto = proto_leaf.key.ok_or_else(|| {
                anyhow::anyhow!("compliance leaf proto: missing key field")
            })?;
            let key_bytes: [u8; 32] = key_proto.inner.as_slice().try_into().map_err(|_| {
                anyhow::anyhow!(
                    "compliance leaf proto: key must be 32 bytes, got {}",
                    key_proto.inner.len()
                )
            })?;
            let key_element = decaf377::Encoding(key_bytes).vartime_decompress().map_err(|_| {
                anyhow::anyhow!(
                    "compliance leaf proto: invalid ACK encoding (not a valid curve point)"
                )
            })?;
            let key = AddressComplianceKey::new(key_element);

            let asset_id: penumbra_sdk_asset::asset::Id = proto_leaf.asset_id.ok_or_else(|| {
                anyhow::anyhow!("compliance leaf proto: missing asset_id field")
            })?.try_into()?;

            Ok(ComplianceLeaf {
                address,
                key,
                asset_id,
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

        // For unregistered assets, return default path with unregulated status
        // This allows transactions with new/unregistered assets to proceed
        if !proofs.asset_registered {
            return Ok((
                MerklePath::default(),
                0,
                penumbra_sdk_compliance::IndexedLeaf {
                    value: decaf377::Fq::from(0u64),
                    next_index: 0,
                    next_value: penumbra_sdk_compliance::indexed_tree::FQ_MAX.clone(),
                    policy: penumbra_sdk_compliance::AssetPolicy::default_unregulated(),
                },
                false,
            ));
        }

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
        use penumbra_sdk_compliance::BLACK_HOLE_ACK;
        use penumbra_sdk_keys::keys::AddressComplianceKey;

        let proofs_future = {
            let mut view = self.view.lock().await;
            view.get_compliance_merkle_proofs(address.clone(), asset_id)
        };
        let proofs = proofs_future.await?;

        // For unregulated assets, return synthetic leaf with BLACK_HOLE_ACK
        if !proofs.is_regulated {
            let synthetic_leaf = ComplianceLeaf {
                address: address.clone(),
                key: AddressComplianceKey::new(*BLACK_HOLE_ACK),
                asset_id,
            };
            return Ok((MerklePath::default(), 0, synthetic_leaf));
        }

        // For regulated assets, user must be registered
        if !proofs.user_registered {
            anyhow::bail!(
                "user not registered in compliance tree for address {:?} and asset {:?}",
                address,
                asset_id
            );
        }

        // Get the leaf separately for regulated assets
        let leaf_future = {
            let mut view = self.view.lock().await;
            view.get_compliance_leaf(address.clone(), asset_id)
        };
        let leaf = leaf_future.await?;

        Ok((proofs.compliance_path, proofs.compliance_position, leaf))
    }

    async fn get_asset_policy(
        &self,
        asset_id: asset::Id,
    ) -> Result<Option<penumbra_sdk_compliance::structs::AssetPolicy>> {
        let future = {
            let mut view = self.view.lock().await;
            view.compliance_asset_policy(asset_id)
        };
        let response = future.await?;

        // Parse dk_pub from response
        if response.dk_pub.is_empty() {
            return Ok(None);
        }

        let dk_pub_bytes: [u8; 32] = response
            .dk_pub
            .try_into()
            .map_err(|v: Vec<u8>| anyhow::anyhow!("dk_pub must be 32 bytes, got {}", v.len()))?;
        let dk_pub = decaf377::Encoding(dk_pub_bytes)
            .vartime_decompress()
            .map_err(|_| anyhow::anyhow!("invalid dk_pub encoding in policy response"))?;

        // Parse threshold from bytes (16 bytes little-endian u128)
        let threshold = if response.threshold.is_empty() {
            u128::MAX // Default to "never flag" if empty
        } else {
            let threshold_bytes: [u8; 16] =
                response.threshold.try_into().map_err(|v: Vec<u8>| {
                    anyhow::anyhow!("threshold must be 16 bytes, got {}", v.len())
                })?;
            u128::from_le_bytes(threshold_bytes)
        };

        Ok(Some(penumbra_sdk_compliance::structs::AssetPolicy {
            dk_pub,
            threshold,
        }))
    }

    async fn get_batch_proofs(
        &self,
        queries: &[(Address, asset::Id)],
    ) -> Result<penumbra_sdk_compliance::BatchComplianceData> {
        use penumbra_sdk_compliance::BLACK_HOLE_ACK;
        use penumbra_sdk_keys::keys::AddressComplianceKey;

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

                if result.asset_registered {
                    asset_proofs.insert(
                        *asset_id,
                        (
                            asset_path.clone(),
                            result.asset_position,
                            indexed_leaf,
                            result.is_regulated,
                        ),
                    );
                } else {
                    asset_proofs.insert(*asset_id, (MerklePath::default(), 0, indexed_leaf, false));
                }
            }

            // Build user proof with leaf
            let key = (address.clone(), *asset_id);
            if !user_proofs.contains_key(&key) {
                if result.is_regulated {
                    if !result.user_registered {
                        anyhow::bail!(
                            "user not registered in compliance tree for address {:?} and asset {:?}",
                            address,
                            asset_id
                        );
                    }
                    // Use the compliance_leaf from batch response (avoids N+1 queries)
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
                        // Fallback to separate query if server doesn't include leaf
                        let leaf_future = {
                            let mut view = self.view.lock().await;
                            view.get_compliance_leaf(address.clone(), *asset_id)
                        };
                        leaf_future.await?
                    };
                    user_proofs.insert(key, (compliance_path, result.compliance_position, leaf));
                } else {
                    // For unregulated assets, use synthetic leaf with BLACK_HOLE_ACK
                    let synthetic_leaf = ComplianceLeaf {
                        address: address.clone(),
                        key: AddressComplianceKey::new(*BLACK_HOLE_ACK),
                        asset_id: *asset_id,
                    };
                    user_proofs.insert(key, (MerklePath::default(), 0, synthetic_leaf));
                }
            }
        }

        // Fetch asset policies for regulated assets
        let mut asset_policies = std::collections::BTreeMap::new();
        for (asset_id, (_, _, _, is_regulated)) in &asset_proofs {
            if *is_regulated {
                if let Some(policy) = self.get_asset_policy(*asset_id).await? {
                    asset_policies.insert(*asset_id, policy);
                }
            }
        }

        Ok(penumbra_sdk_compliance::BatchComplianceData {
            compliance_anchor,
            asset_anchor,
            asset_proofs,
            user_proofs,
            asset_policies,
        })
    }
}

/// Enriches a transaction plan with compliance data using a ComplianceProofProvider.
///
/// This is the canonical implementation for multi-asset transaction enrichment.
/// It handles cross-asset binding correctly by using "canonical" binding assets.
///
/// For multi-asset transactions (e.g., delegation where spend is staking token
/// and output is delegation token), the binding check requires:
/// - spend.counterparty_leaf_hash == output.receiver_leaf_hash
/// - output.counterparty_leaf_hash == spend.sender_leaf_hash
///
/// Since ComplianceLeaf includes asset_id, we use canonical binding assets:
/// - Spend uses first OUTPUT's asset for counterparty lookup
/// - Output uses first SPEND's asset for counterparty lookup
pub async fn enrich_plan_with_compliance<P: ComplianceProofProvider>(
    plan: &mut TransactionPlan,
    provider: &P,
    rng: &mut (impl rand_core::RngCore + rand_core::CryptoRng),
) -> Result<()> {
    use std::collections::BTreeSet;

    // Collect spend and output indices
    let mut all_spend_indices = Vec::new();
    let mut all_output_indices = Vec::new();

    for (i, action) in plan.actions.iter().enumerate() {
        match action {
            ActionPlan::Spend(_) => all_spend_indices.push(i),
            ActionPlan::Output(_) => all_output_indices.push(i),
            _ => {}
        }
    }

    // Need at least one spend or output for compliance
    if all_spend_indices.is_empty() && all_output_indices.is_empty() {
        return Ok(());
    }

    // Get sender address from first spend, or first output's destination if no spends
    let sender_address = if !all_spend_indices.is_empty() {
        let ActionPlan::Spend(spend) = &plan.actions[all_spend_indices[0]] else {
            unreachable!()
        };
        spend.note.address()
    } else {
        let ActionPlan::Output(output) = &plan.actions[all_output_indices[0]] else {
            unreachable!()
        };
        output.dest_address.clone()
    };

    // For cross-action binding in multi-asset transactions, we use "canonical" binding assets.
    // This ensures spend.counterparty_leaf_hash == output.receiver_leaf_hash when assets differ.
    let binding_asset_id = if !all_output_indices.is_empty() {
        let ActionPlan::Output(output) = &plan.actions[all_output_indices[0]] else {
            unreachable!()
        };
        output.value.asset_id
    } else {
        let ActionPlan::Spend(spend) = &plan.actions[all_spend_indices[0]] else {
            unreachable!()
        };
        spend.note.asset_id()
    };

    let binding_recipient_address = if !all_output_indices.is_empty() {
        let ActionPlan::Output(output) = &plan.actions[all_output_indices[0]] else {
            unreachable!()
        };
        output.dest_address.clone()
    } else {
        sender_address.clone()
    };

    // Determine the spend's binding asset for output counterparty lookups
    let spend_binding_asset_id = if !all_spend_indices.is_empty() {
        let ActionPlan::Spend(spend) = &plan.actions[all_spend_indices[0]] else {
            unreachable!()
        };
        spend.note.asset_id()
    } else {
        binding_asset_id
    };

    // PHASE 1: Collect all unique (address, asset) pairs needed for the transaction
    let mut queries: BTreeSet<(Address, asset::Id)> = BTreeSet::new();

    // For each spend: own (address, asset) + counterparty binding
    for &spend_idx in &all_spend_indices {
        let ActionPlan::Spend(spend) = &plan.actions[spend_idx] else {
            unreachable!()
        };
        queries.insert((spend.note.address(), spend.note.asset_id()));
        queries.insert((binding_recipient_address.clone(), binding_asset_id));
    }

    // For each output: recipient (address, asset) + sender binding
    for &output_idx in &all_output_indices {
        let ActionPlan::Output(output) = &plan.actions[output_idx] else {
            continue;
        };
        queries.insert((output.dest_address.clone(), output.value.asset_id));
        queries.insert((sender_address.clone(), spend_binding_asset_id));
    }

    // PHASE 2: Batch fetch all compliance data in a single call
    let query_vec: Vec<(Address, asset::Id)> = queries.into_iter().collect();
    let batch_data = provider.get_batch_proofs(&query_vec).await?;

    // Extract anchors from batch data
    let compliance_anchor = batch_data.compliance_anchor;
    let asset_anchor = batch_data.asset_anchor;

    // PHASE 3: Apply the cached data to each action

    // Process all spends
    let mut tx_blinding_nonce = None;

    for &spend_idx in &all_spend_indices {
        // Get this spend's own address and asset - each spend may have a different diversifier
        let (spend_asset_id, spend_address) = {
            let ActionPlan::Spend(spend) = &plan.actions[spend_idx] else {
                unreachable!()
            };
            (spend.note.asset_id(), spend.note.address())
        };

        let (asset_path, asset_position, asset_indexed_leaf, is_regulated) = batch_data
            .asset_proofs
            .get(&spend_asset_id)
            .cloned()
            .unwrap_or_else(|| {
                let default_leaf = penumbra_sdk_compliance::IndexedLeaf {
                    value: decaf377::Fq::from(0u64),
                    next_index: 0,
                    next_value: penumbra_sdk_compliance::indexed_tree::FQ_MAX.clone(),
                    policy: penumbra_sdk_compliance::AssetPolicy::default_unregulated(),
                };
                (MerklePath::default(), 0, default_leaf, false)
            });

        let (sender_compliance_path, sender_compliance_position, sender_leaf) = batch_data
            .user_proofs
            .get(&(spend_address.clone(), spend_asset_id))
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "missing user proof for spend at index {}: \
                     user may not be registered for asset {} \
                     (check compliance registration status)",
                    spend_idx,
                    spend_asset_id
                )
            })?;

        let (_, _, counterparty_leaf) = batch_data
            .user_proofs
            .get(&(binding_recipient_address.clone(), binding_asset_id))
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "missing user proof for counterparty in spend binding: \
                     counterparty may not be registered for asset {} \
                     (recipient must be registered for regulated assets)",
                    binding_asset_id
                )
            })?;

        {
            let ActionPlan::Spend(spend) = &mut plan.actions[spend_idx] else {
                unreachable!()
            };
            // Set asset_indexed_leaf BEFORE set_compliance_details since encryption uses it
            spend.asset_indexed_leaf = asset_indexed_leaf.clone();
            spend.asset_path = asset_path;
            spend.asset_position = asset_position;
            spend.asset_anchor = asset_anchor;
            spend.compliance_anchor = compliance_anchor;
            spend.compliance_path = sender_compliance_path;
            spend.compliance_position = sender_compliance_position;
            spend.is_regulated = is_regulated;

            // Use this spend's own address to ensure diversifier matches
            spend.set_compliance_details(
                rng,
                &sender_leaf.key,
                &spend_address,
                &binding_recipient_address,
                counterparty_leaf,
            )?;

            // Unify tx_blinding_nonce across all spends for cross-action binding
            if let Some(nonce) = tx_blinding_nonce {
                // Apply the first spend's nonce to subsequent spends
                spend.tx_blinding_nonce = nonce;
            } else {
                // Capture the first spend's nonce
                tx_blinding_nonce = Some(spend.tx_blinding_nonce);
            }
        }
    }

    // Process all outputs
    if !all_output_indices.is_empty() {
        let tx_blinding_nonce = tx_blinding_nonce.unwrap_or_else(|| Fr::rand(rng));

        for &output_idx in &all_output_indices {
            let (output_asset_id, recipient_address) = {
                let ActionPlan::Output(output) = &plan.actions[output_idx] else {
                    continue;
                };
                (output.value.asset_id, output.dest_address.clone())
            };

            let (asset_path, asset_position, asset_indexed_leaf, is_regulated) = batch_data
                .asset_proofs
                .get(&output_asset_id)
                .cloned()
                .unwrap_or_else(|| {
                    let default_leaf = penumbra_sdk_compliance::IndexedLeaf {
                        value: decaf377::Fq::from(0u64),
                        next_index: 0,
                        next_value: penumbra_sdk_compliance::indexed_tree::FQ_MAX.clone(),
                        policy: penumbra_sdk_compliance::AssetPolicy::default_unregulated(),
                    };
                    (MerklePath::default(), 0, default_leaf, false)
                });

            let (recipient_compliance_path, recipient_compliance_position, recipient_leaf) =
                batch_data
                    .user_proofs
                    .get(&(recipient_address.clone(), output_asset_id))
                    .cloned()
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing user proof for output at index {}: \
                             recipient may not be registered for asset {} \
                             (recipient must be registered for regulated assets)",
                            output_idx,
                            output_asset_id
                        )
                    })?;

            let (_, _, sender_leaf_for_output) = batch_data
                .user_proofs
                .get(&(sender_address.clone(), spend_binding_asset_id))
                .cloned()
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "missing user proof for sender in output binding: \
                         sender may not be registered for asset {} \
                         (sender must be registered for regulated assets)",
                        spend_binding_asset_id
                    )
                })?;

            {
                let ActionPlan::Output(output) = &mut plan.actions[output_idx] else {
                    continue;
                };
                // Set asset_indexed_leaf BEFORE set_compliance_details since encryption uses it
                output.asset_indexed_leaf = asset_indexed_leaf.clone();
                output.asset_path = asset_path;
                output.asset_position = asset_position;
                output.asset_anchor = asset_anchor;
                output.compliance_anchor = compliance_anchor;
                output.compliance_path = recipient_compliance_path;
                output.compliance_position = recipient_compliance_position;
                output.is_regulated = is_regulated;

                output.set_compliance_details(
                    rng,
                    &recipient_leaf.key,
                    &sender_address,
                    sender_leaf_for_output,
                    tx_blinding_nonce,
                )?;
            }
        }
    }

    Ok(())
}
