use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use cnidarium::StateWrite;
use cnidarium_component::{ActionHandler, Component};
use penumbra_sdk_asset::BASE_ASSET_ID;
use penumbra_sdk_proto::{StateReadProto, StateWriteProto};
use tendermint::v0_37::abci;
use tracing::instrument;

use crate::{
    event, genesis,
    registry::{ComplianceRegistryRead, ComplianceRegistryWrite},
    state_key,
    structs::{MsgRegisterAsset, MsgRegisterUser},
};

// Note: QuadTree is still used for the user tree.
// Asset tree has been migrated to IMT (Indexed Merkle Tree).

/// The Compliance component manages on-chain registries for regulated assets.
///
/// It maintains two Quad Merkle Trees:
/// - User tree: Maps users to their address compliance keys (ACKs) for regulated assets
/// - Asset tree: Tracks which assets are regulated
pub struct Compliance {}

#[async_trait]
impl Component for Compliance {
    type AppState = genesis::Content;

    #[instrument(name = "compliance", skip(state, app_state))]
    async fn init_chain<S: StateWrite>(mut state: S, app_state: Option<&Self::AppState>) {
        // Initialize empty trees if they don't exist
        // This ensures the trees are properly set up at genesis

        // Initialize user tree if not present
        // Note: get_user_tree() returns a new empty tree if nothing is stored,
        // so we just need to handle errors and ensure we persist the initial state.
        match state.get_user_tree().await {
            Ok(tree) => {
                // Persist the tree (may be new or existing)
                let tree_bytes = bincode::serialize(&tree).expect("serialization should not fail");
                state.put_raw(crate::state_key::user_tree().to_string(), tree_bytes);
                state.put(crate::state_key::user_tree_root().to_string(), tree.root());
                state.write_user_tree_cache(tree);
                // Initialize count if not set
                if state
                    .get_proto::<u64>(state_key::user_count())
                    .await
                    .ok()
                    .flatten()
                    .is_none()
                {
                    state.put_proto(crate::state_key::user_count().to_string(), 0u64);
                }
            }
            Err(e) => {
                tracing::error!(?e, "failed to load compliance user tree during init_chain");
                panic!("compliance user tree initialization failed: {}", e);
            }
        }

        // Initialize asset IMT if not present
        // Note: get_asset_imt() returns a new empty tree if nothing is stored.
        match state.get_asset_imt().await {
            Ok(tree) => {
                // Persist the tree (may be new or existing)
                let tree_bytes = bincode::serialize(&tree).expect("serialization should not fail");
                state.put_raw(crate::state_key::asset_imt().to_string(), tree_bytes);
                state.put(crate::state_key::asset_imt_root().to_string(), tree.root());
                state.write_asset_imt_cache(tree);
            }
            Err(e) => {
                tracing::error!(?e, "failed to load compliance asset IMT during init_chain");
                panic!("compliance asset IMT initialization failed: {}", e);
            }
        }

        // Genesis starts clean; modifications during init/register calls will set this.
        state.clear_compliance_trees_modified();

        // Seed the neutral base asset into the IMT as an explicit unregulated asset.
        //
        // This keeps the chain-native asset on a stable membership-proof path
        // without treating it as regulated.
        if let Some(result) = state
            .register_asset_in_imt(
                *BASE_ASSET_ID,
                crate::structs::AssetPolicy::default_unregulated(),
                false,
            )
            .await
            .expect("must be able to register base asset at genesis")
        {
            let event = crate::event::EventAssetRegistered {
                asset_id: *BASE_ASSET_ID,
                is_regulated: false,
                position: result.position,
                indexed_leaf: result.indexed_leaf,
                low_leaf_position: result.low_leaf_position,
                updated_low_leaf: result.updated_low_leaf,
            };

            state.record_proto(event::asset_registered(
                event.asset_id,
                event.is_regulated,
                event.position,
                event.indexed_leaf.clone(),
                event.low_leaf_position,
                event.updated_low_leaf.clone(),
            ));
            state.record_pending_asset_registration(event);
        }

        // Register native assets from genesis configuration.
        if let Some(genesis) = app_state {
            for registration in &genesis.native_assets {
                let (policy, is_regulated) = if registration.is_regulated {
                    // Regulated assets MUST have a detection key.
                    let dk_pub_bytes = registration
                        .dk_pub
                        .expect("regulated asset in genesis must have dk_pub");
                    let dk_pub = decaf377::Encoding(dk_pub_bytes)
                        .vartime_decompress()
                        .expect("invalid dk_pub encoding in genesis");

                    (
                        crate::structs::AssetPolicy::simple(
                            dk_pub,
                            u128::MAX,
                            decaf377::Element::GENERATOR,
                        ),
                        true,
                    )
                } else {
                    (crate::structs::AssetPolicy::default_unregulated(), false)
                };

                if let Some(result) = state
                    .register_asset_in_imt(registration.asset_id, policy, is_regulated)
                    .await
                    .expect("must be able to register native asset at genesis")
                {
                    let event = crate::event::EventAssetRegistered {
                        asset_id: registration.asset_id,
                        is_regulated,
                        position: result.position,
                        indexed_leaf: result.indexed_leaf,
                        low_leaf_position: result.low_leaf_position,
                        updated_low_leaf: result.updated_low_leaf,
                    };

                    state.record_proto(event::asset_registered(
                        event.asset_id,
                        event.is_regulated,
                        event.position,
                        event.indexed_leaf.clone(),
                        event.low_leaf_position,
                        event.updated_low_leaf.clone(),
                    ));
                    state.record_pending_asset_registration(event);
                    tracing::info!(
                        ?registration.asset_id,
                        is_regulated,
                        "registered native asset at genesis"
                    );
                }
            }
        }

        // Record initial anchors at genesis (height 0)
        state
            .record_compliance_anchors(0)
            .await
            .expect("must be able to record initial compliance anchors");
        tracing::info!("recorded initial compliance anchors at genesis");
    }

    #[instrument(name = "compliance", skip(_state, _begin_block))]
    async fn begin_block<S: StateWrite + 'static>(
        _state: &mut Arc<S>,
        _begin_block: &abci::request::BeginBlock,
    ) {
        // No-op for compliance component
    }

    #[instrument(name = "compliance", skip(state, end_block))]
    async fn end_block<S: StateWrite + 'static>(
        state: &mut Arc<S>,
        end_block: &abci::request::EndBlock,
    ) {
        // Record compliance tree anchors at this block height.
        // This enables historical anchor validation for proofs generated at past blocks.
        let height = end_block.height as u64;
        let state = Arc::get_mut(state).expect("state should be unique");
        state
            .record_compliance_anchors(height)
            .await
            .expect("must be able to record compliance anchors");
    }

    async fn end_epoch<S: StateWrite + 'static>(_state: &mut Arc<S>) -> Result<()> {
        // No-op for compliance component
        Ok(())
    }
}

/// ActionHandler implementation for MsgRegisterUser.
///
/// This handler registers a user's address compliance key (ACK) for a regulated asset.
#[async_trait]
impl ActionHandler for MsgRegisterUser {
    type CheckStatelessContext = ();

    async fn check_stateless(&self, _context: ()) -> Result<()> {
        // TODO(compliance): Verify signature proving ownership of address address.
        // The signature should be over a canonical message including the leaf commitment.
        Ok(())
    }

    async fn check_and_execute<S: StateWrite>(&self, mut state: S) -> Result<()> {
        anyhow::ensure!(
            state.is_asset_regulated(self.leaf.asset_id).await?,
            "cannot register user for unregulated asset {}",
            self.leaf.asset_id
        );

        // Check if user is already registered for this asset (idempotent)
        if let Some(existing_position) = state
            .get_user_leaf_position(&self.leaf.address, self.leaf.asset_id)
            .await?
        {
            tracing::debug!(
                position = existing_position,
                address = ?self.leaf.address,
                asset_id = ?self.leaf.asset_id,
                "user already registered for asset, skipping duplicate registration"
            );
            // Return success without modifying state - idempotent behavior
            return Ok(());
        }

        // User not registered, proceed with registration
        let position = state.add_compliance_leaf(self.leaf.clone()).await?;
        let commitment = self.leaf.commit();

        // Create the event
        let event = crate::event::EventUserRegistered {
            position,
            commitment,
            leaf: self.leaf.clone(),
        };

        // Buffer the event for CompactBlock inclusion
        state.record_pending_user_registration(event.clone());

        // Also emit as ABCI event (for existing event listeners)
        state.record_proto(event::user_registered(
            position,
            commitment,
            self.leaf.clone(),
        ));

        Ok(())
    }
}

/// ActionHandler implementation for MsgRegisterAsset.
///
/// This handler registers an asset's regulation status in the compliance registry.
#[async_trait]
impl ActionHandler for MsgRegisterAsset {
    type CheckStatelessContext = ();

    async fn check_stateless(&self, _context: ()) -> Result<()> {
        // TODO(compliance): Add governance authorization check.
        // Only authorized parties (e.g., asset issuer) should be able to register assets.
        Ok(())
    }

    async fn check_and_execute<S: StateWrite>(&self, mut state: S) -> Result<()> {
        let (policy, is_regulated) = if self.is_regulated {
            let dk_pub = self.dk_pub.ok_or_else(|| {
                anyhow::anyhow!("regulated assets require a detection key (dk_pub)")
            })?;

            let threshold = self.threshold.unwrap_or(u128::MAX);
            let ring_pk = self.ring_pk.unwrap_or(decaf377::Element::GENERATOR);
            (
                crate::structs::AssetPolicy::new(
                    dk_pub,
                    threshold,
                    self.allowed_channels.clone(),
                    self.ring_id.clone(),
                    ring_pk,
                    self.policy_id.clone(),
                    self.permission.clone(),
                    self.resource.clone(),
                ),
                true,
            )
        } else {
            (crate::structs::AssetPolicy::default_unregulated(), false)
        };

        if let Some(result) = state
            .register_asset_in_imt(self.asset_id, policy, is_regulated)
            .await?
        {
            let event = crate::event::EventAssetRegistered {
                asset_id: self.asset_id,
                is_regulated,
                position: result.position,
                indexed_leaf: result.indexed_leaf,
                low_leaf_position: result.low_leaf_position,
                updated_low_leaf: result.updated_low_leaf,
            };

            state.record_proto(event::asset_registered(
                event.asset_id,
                event.is_regulated,
                event.position,
                event.indexed_leaf.clone(),
                event.low_leaf_position,
                event.updated_low_leaf.clone(),
            ));

            state.record_pending_asset_registration(event);
        }
        // If None, asset was already registered — policy is immutable, skip.

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cnidarium::{StateRead, TempStorage};
    use decaf377::Fq;
    use penumbra_sdk_asset::{asset, BASE_ASSET_ID};
    use penumbra_sdk_keys::Address;

    use crate::genesis::NativeAssetRegistration;
    use crate::structs::ComplianceLeaf;

    #[tokio::test]
    async fn test_init_chain() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);

        // Initialize the component with default genesis
        let genesis = genesis::Content::default();
        Compliance::init_chain(&mut state, Some(&genesis)).await;

        // Verify trees were initialized
        let user_tree = state.get_user_tree().await.unwrap();
        let asset_imt = state.get_asset_imt().await.unwrap();

        assert_eq!(user_tree.depth(), 16);
        assert_eq!(asset_imt.depth(), 16);
    }

    #[tokio::test]
    async fn test_init_chain_without_genesis() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);

        // Initialize without genesis content
        Compliance::init_chain(&mut state, None).await;

        // Trees should be initialized
        let user_tree = state.get_user_tree().await.unwrap();
        let asset_imt = state.get_asset_imt().await.unwrap();

        assert_eq!(user_tree.depth(), 16);
        // IMT contains the sentinel plus the neutral base asset registration.
        assert_eq!(asset_imt.leaf_count(), 2);

        let proof_data = state.get_asset_proof_data(*BASE_ASSET_ID).await.unwrap();
        assert_eq!(proof_data.indexed_leaf.value, BASE_ASSET_ID.0);
        assert!(
            !proof_data.is_regulated,
            "the seeded base asset must stay explicitly unregulated"
        );
    }

    #[tokio::test]
    async fn test_init_chain_with_custom_genesis() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);

        let custom_asset = asset::Id(Fq::from(999u64));

        // Custom genesis with a regulated asset (requires dk_pub)
        let dk_pub_bytes = decaf377::Element::GENERATOR.vartime_compress().0;
        let genesis = genesis::Content {
            native_assets: vec![NativeAssetRegistration {
                asset_id: custom_asset,
                is_regulated: true,
                dk_pub: Some(dk_pub_bytes),
            }],
        };

        Compliance::init_chain(&mut state, Some(&genesis)).await;

        // Custom asset should be in IMT (regulated)
        let proof_data = state.get_asset_proof_data(custom_asset).await.unwrap();
        assert!(proof_data.is_regulated, "custom asset should be regulated");
    }

    #[tokio::test]
    async fn test_msg_register_user_for_regulated_asset() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);

        // Initialize component with defaults
        let genesis = genesis::Content::default();
        Compliance::init_chain(&mut state, Some(&genesis)).await;

        let asset_id = asset::Id(Fq::from(1u64));
        MsgRegisterAsset {
            asset_id,
            is_regulated: true,
            dk_pub: Some(decaf377::Element::GENERATOR),
            threshold: None,
            allowed_channels: vec![],
            ring_pk: None,
            ring_id: String::new(),
            policy_id: String::new(),
            permission: String::new(),
            resource: String::new(),
        }
        .check_and_execute(&mut state)
        .await
        .unwrap();

        let msg = MsgRegisterUser {
            leaf: ComplianceLeaf {
                address: Address::dummy(&mut rand::thread_rng()),
                asset_id,
                d: Fq::from(0u64),
            },
            signature: vec![0u8; 64], // Dummy signature
        };

        // Execute the action directly on state
        msg.check_and_execute(&mut state).await.unwrap();

        // Duplicate registration stays idempotent for regulated assets.
        msg.check_and_execute(&mut state).await.unwrap();

        // Verify user was registered once.
        let user_count = state.get_user_count().await.unwrap();
        assert_eq!(user_count, 1);
    }

    #[tokio::test]
    async fn test_msg_register_user_for_unregulated_asset_fails_without_mutating_state() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);

        Compliance::init_chain(&mut state, Some(&genesis::Content::default())).await;

        let msg = MsgRegisterUser {
            leaf: ComplianceLeaf {
                address: Address::dummy(&mut rand::thread_rng()),
                asset_id: *BASE_ASSET_ID,
                d: Fq::from(0u64),
            },
            signature: vec![0u8; 64],
        };

        let error = msg.check_and_execute(&mut state).await.expect_err(
            "unregulated assets, including the base asset, must reject user registration",
        );
        assert!(
            error
                .to_string()
                .contains("cannot register user for unregulated asset"),
            "unexpected error: {error}"
        );

        assert_eq!(state.get_user_count().await.unwrap(), 0);
        assert!(state
            .object_get::<Vec<crate::event::EventUserRegistered>>(
                crate::state_key::pending_user_registrations()
            )
            .is_none());
    }

    #[tokio::test]
    async fn test_msg_register_user_for_absent_asset_fails_without_mutating_state() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);

        Compliance::init_chain(&mut state, Some(&genesis::Content::default())).await;

        let msg = MsgRegisterUser {
            leaf: ComplianceLeaf {
                address: Address::dummy(&mut rand::thread_rng()),
                asset_id: asset::Id(Fq::from(999_999u64)),
                d: Fq::from(0u64),
            },
            signature: vec![0u8; 64],
        };

        let error = msg
            .check_and_execute(&mut state)
            .await
            .expect_err("absent assets must reject user registration");
        assert!(
            error
                .to_string()
                .contains("cannot register user for unregulated asset"),
            "unexpected error: {error}"
        );

        assert_eq!(state.get_user_count().await.unwrap(), 0);
        assert!(state
            .object_get::<Vec<crate::event::EventUserRegistered>>(
                crate::state_key::pending_user_registrations()
            )
            .is_none());
    }

    #[tokio::test]
    async fn test_msg_register_asset() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);

        // Initialize component
        let genesis = genesis::Content::default();
        Compliance::init_chain(&mut state, Some(&genesis)).await;

        // Initially the asset is unregulated (not in IMT)
        let asset_id = asset::Id(Fq::from(123u64));
        let proof_before = state.get_asset_proof_data(asset_id).await.unwrap();
        assert!(!proof_before.is_regulated, "asset should start unregulated");

        // Create a register asset message (regulated) - requires dk_pub
        let dk_pub = Some(decaf377::Element::GENERATOR);
        let msg = MsgRegisterAsset {
            asset_id,
            is_regulated: true,
            dk_pub,
            threshold: None,
            allowed_channels: vec![],
            ring_pk: None,
            ring_id: String::new(),
            policy_id: String::new(),
            permission: String::new(),
            resource: String::new(),
        };

        // Execute the action
        msg.check_and_execute(&mut state).await.unwrap();

        // Verify asset is now regulated
        let proof_after = state.get_asset_proof_data(asset_id).await.unwrap();
        assert!(
            proof_after.is_regulated,
            "asset should be regulated after registration"
        );
    }

    #[tokio::test]
    async fn test_msg_register_unregulated_asset_is_noop() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);

        // Initialize component
        Compliance::init_chain(&mut state, None).await;

        let asset_id = asset::Id(Fq::from(456u64));

        // Create a register asset message (unregulated)
        let msg = MsgRegisterAsset {
            asset_id,
            is_regulated: false,
            dk_pub: None,
            threshold: None,
            allowed_channels: vec![],
            ring_pk: None,
            ring_id: String::new(),
            policy_id: String::new(),
            permission: String::new(),
            resource: String::new(),
        };

        // Execute the action - should be a no-op
        msg.check_and_execute(&mut state).await.unwrap();

        // Asset should still be unregulated (not in IMT)
        let proof = state.get_asset_proof_data(asset_id).await.unwrap();
        assert!(
            !proof.is_regulated,
            "unregulated asset should not be added to IMT"
        );
    }

    #[tokio::test]
    async fn test_msg_register_regulated_without_dk_pub_fails() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);

        // Initialize component
        Compliance::init_chain(&mut state, None).await;

        let asset_id = asset::Id(Fq::from(789u64));

        // Create a register asset message (regulated but missing dk_pub)
        let msg = MsgRegisterAsset {
            asset_id,
            is_regulated: true,
            dk_pub: None, // Missing!
            threshold: None,
            allowed_channels: vec![],
            ring_pk: None,
            ring_id: String::new(),
            policy_id: String::new(),
            permission: String::new(),
            resource: String::new(),
        };

        // Execute the action - should fail
        let result = msg.check_and_execute(&mut state).await;
        assert!(
            result.is_err(),
            "registering regulated asset without dk_pub should fail"
        );
    }
}
