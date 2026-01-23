use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use cnidarium::StateWrite;
use cnidarium_component::{ActionHandler, Component};
use penumbra_sdk_proto::StateWriteProto;
use tendermint::v0_37::abci;
use tracing::instrument;

use crate::{
    event, genesis,
    indexed_tree::IndexedMerkleTree,
    registry::{ComplianceRegistryRead, ComplianceRegistryWrite},
    structs::{MsgRegisterAsset, MsgRegisterUser},
    tree::QuadTree,
};

// Note: QuadTree is still used for the user tree.
// Asset tree has been migrated to IMT (Indexed Merkle Tree).

/// The Compliance component manages on-chain registries for regulated assets.
///
/// It maintains two Quad Merkle Trees:
/// - User tree: Maps users to their compliance viewing keys for regulated assets
/// - Asset tree: Tracks which assets are regulated
pub struct Compliance {}

#[async_trait]
impl Component for Compliance {
    type AppState = genesis::Content;

    #[instrument(name = "compliance", skip(state, app_state))]
    async fn init_chain<S: StateWrite>(mut state: S, app_state: Option<&Self::AppState>) {
        // Initialize empty trees if they don't exist
        // This ensures the trees are properly set up at genesis

        // Check and initialize user tree
        if state.get_user_tree().await.ok().is_none() {
            let user_tree = QuadTree::new();
            let tree_bytes = bincode::serialize(&user_tree).expect("serialization should not fail");
            state.put_raw(crate::state_key::user_tree().to_string(), tree_bytes);
            state.put_proto(crate::state_key::user_count().to_string(), 0u64);
        }

        // Check and initialize asset IMT (Indexed Merkle Tree for regulated assets)
        if state.get_asset_imt().await.ok().is_none() {
            let asset_imt = IndexedMerkleTree::new();
            let tree_bytes = bincode::serialize(&asset_imt).expect("serialization should not fail");
            state.put_raw(crate::state_key::asset_imt().to_string(), tree_bytes);
        }

        // Register regulated native assets from genesis configuration.
        // Unregulated assets are NOT stored - they're proven via IMT non-membership.
        if let Some(genesis) = app_state {
            for registration in &genesis.native_assets {
                if registration.is_regulated {
                    state
                        .register_regulated_asset(registration.asset_id)
                        .await
                        .expect("must be able to register regulated asset at genesis");
                    tracing::info!(
                        ?registration.asset_id,
                        "registered regulated asset at genesis"
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
/// This handler registers a user's compliance viewing key for a regulated asset.
#[async_trait]
impl ActionHandler for MsgRegisterUser {
    type CheckStatelessContext = ();

    async fn check_stateless(&self, _context: ()) -> Result<()> {
        // TODO(compliance): Verify signature proving ownership of address address.
        // The signature should be over a canonical message including the leaf commitment.
        Ok(())
    }

    async fn check_and_execute<S: StateWrite>(&self, mut state: S) -> Result<()> {
        // TODO(compliance): Check if user is already registered (idempotent but wasteful).
        // Could add check_stateful() to detect and skip duplicate registrations.
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
        // Only regulated assets are stored in the IMT.
        // Unregulated status is proven via non-membership proofs.
        if self.is_regulated {
            if let Some(result) = state.register_regulated_asset(self.asset_id).await? {
                // Create the event
                let event = crate::event::EventAssetRegistered {
                    asset_id: self.asset_id,
                    is_regulated: self.is_regulated,
                    position: result.position,
                    indexed_leaf: result.indexed_leaf,
                    low_leaf_position: result.low_leaf_position,
                    updated_low_leaf: result.updated_low_leaf,
                };

                // Also emit as ABCI event (for existing event listeners)
                // We need to convert from the domain type to the proto type
                state.record_proto(event::asset_registered(
                    event.asset_id,
                    event.is_regulated,
                    event.position,
                    event.indexed_leaf.clone(),
                    event.low_leaf_position,
                    event.updated_low_leaf.clone(),
                ));

                // Buffer the event for CompactBlock inclusion
                state.record_pending_asset_registration(event);
            }
            // If None, asset was already registered - skip event
        }
        // Unregulated assets don't emit events (no state change)

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cnidarium::TempStorage;
    use decaf377::Fq;
    use penumbra_sdk_asset::asset;
    use penumbra_sdk_keys::{keys::AddressComplianceKey, Address};

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
        // IMT should only have sentinel leaf
        assert_eq!(asset_imt.leaf_count(), 1);
    }

    #[tokio::test]
    async fn test_init_chain_with_custom_genesis() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);

        let custom_asset = asset::Id(Fq::from(999u64));

        // Custom genesis with a regulated asset
        let genesis = genesis::Content {
            native_assets: vec![NativeAssetRegistration {
                asset_id: custom_asset,
                is_regulated: true,
            }],
        };

        Compliance::init_chain(&mut state, Some(&genesis)).await;

        // Custom asset should be in IMT (regulated)
        let proof_data = state.get_asset_proof_data(custom_asset).await.unwrap();
        assert!(proof_data.is_regulated, "custom asset should be regulated");
    }

    #[tokio::test]
    async fn test_msg_register_user() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);

        // Initialize component with defaults
        let genesis = genesis::Content::default();
        Compliance::init_chain(&mut state, Some(&genesis)).await;

        // Create a register user message
        let msg = MsgRegisterUser {
            leaf: ComplianceLeaf {
                address: Address::dummy(&mut rand::thread_rng()),
                key: AddressComplianceKey::new(decaf377::Element::GENERATOR),
                asset_id: asset::Id(Fq::from(1u64)),
            },
            signature: vec![0u8; 64], // Dummy signature
        };

        // Execute the action directly on state
        msg.check_and_execute(&mut state).await.unwrap();

        // Verify user was registered
        let user_count = state.get_user_count().await.unwrap();
        assert_eq!(user_count, 1);
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

        // Create a register asset message (regulated)
        let msg = MsgRegisterAsset {
            asset_id,
            is_regulated: true,
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
}
