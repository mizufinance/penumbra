use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use cnidarium::StateWrite;
use cnidarium_component::{ActionHandler, Component};
use penumbra_sdk_asset::BASE_ASSET_ID;
use penumbra_sdk_proto::{StateReadProto, StateWriteProto};
use penumbra_sdk_sct::component::clock::EpochRead;
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
                asset_policy: crate::structs::AssetPolicy::default_unregulated(),
            };

            state.emit_asset_registered(event);
        }

        // Register native assets from genesis configuration.
        if let Some(genesis) = app_state {
            for registrar_vk in &genesis.compliance_registrar_vk {
                state.put_compliance_registrar(*registrar_vk);
            }

            for registration in &genesis.native_assets {
                let (policy, is_regulated) = if registration.is_regulated {
                    // Regulated assets MUST have a detection key.
                    let dk_pub_bytes = registration
                        .dk_pub
                        .expect("regulated asset in genesis must have dk_pub");
                    let registration_authority_vk = registration
                        .registration_authority_vk
                        .expect("regulated asset in genesis must have registration_authority_vk");
                    let dk_pub = decaf377::Encoding(dk_pub_bytes)
                        .vartime_decompress()
                        .expect("invalid dk_pub encoding in genesis");

                    (
                        crate::structs::AssetPolicy::new(
                            dk_pub,
                            u128::MAX,
                            vec![],
                            String::new(),
                            decaf377::Element::GENERATOR,
                            String::new(),
                            String::new(),
                            String::new(),
                        )
                        .with_registration_authority(registration_authority_vk),
                        true,
                    )
                } else {
                    (crate::structs::AssetPolicy::default_unregulated(), false)
                };

                let event_policy = policy.clone();
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
                        asset_policy: event_policy,
                    };

                    state.emit_asset_registered(event);
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
        let grant = self
            .grant
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("missing user registration grant"))?;
        anyhow::ensure!(
            grant.body.leaf == self.leaf,
            "user registration grant leaf does not match action leaf"
        );
        Ok(())
    }

    async fn check_and_execute<S: StateWrite>(&self, mut state: S) -> Result<()> {
        anyhow::ensure!(
            state.is_asset_regulated(self.leaf.asset_id).await?,
            "cannot register user for unregulated asset {}",
            self.leaf.asset_id
        );
        let policy = state
            .get_asset_policy(self.leaf.asset_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("missing regulated asset policy"))?;
        let authority_vk = policy.registration_authority_vk.as_ref().ok_or_else(|| {
            anyhow::anyhow!("regulated asset policy missing registration authority")
        })?;
        let grant = self
            .grant
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("missing user registration grant"))?;
        anyhow::ensure!(
            grant.body.policy_id == policy.ring.policy_id,
            "user registration grant policy_id does not match asset policy"
        );
        let current_unix = state.get_current_block_timestamp().await?.unix_timestamp();
        anyhow::ensure!(
            current_unix >= 0,
            "current block timestamp is before Unix epoch"
        );
        anyhow::ensure!(
            (current_unix as u64) <= grant.body.valid_until_unix,
            "user registration grant expired"
        );
        grant.verify(authority_vk)?;

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
        let grant = self
            .asset_registration_grant
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("missing asset registration grant"))?;
        let expected_body = self.registration_grant_body(grant.body.valid_until_unix);
        anyhow::ensure!(
            grant.body == expected_body,
            "asset registration grant body does not match action"
        );
        if self.is_regulated {
            anyhow::ensure!(
                self.registration_authority_vk.is_some(),
                "regulated assets require registration_authority_vk"
            );
        }
        grant.verify()?;
        Ok(())
    }

    async fn check_and_execute<S: StateWrite>(&self, mut state: S) -> Result<()> {
        let grant = self
            .asset_registration_grant
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("missing asset registration grant"))?;
        anyhow::ensure!(
            state.is_compliance_registrar(&grant.registrar_vk).await?,
            "asset registration grant signed by unauthorized registrar"
        );
        let current_unix = state.get_current_block_timestamp().await?.unix_timestamp();
        anyhow::ensure!(
            current_unix >= 0,
            "current block timestamp is before Unix epoch"
        );
        anyhow::ensure!(
            (current_unix as u64) <= grant.body.valid_until_unix,
            "asset registration grant expired"
        );

        let (policy, is_regulated) = if self.is_regulated {
            let dk_pub = self.dk_pub.ok_or_else(|| {
                anyhow::anyhow!("regulated assets require a detection key (dk_pub)")
            })?;
            let registration_authority_vk = self.registration_authority_vk.ok_or_else(|| {
                anyhow::anyhow!("regulated assets require registration_authority_vk")
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
                )
                .with_registration_authority(registration_authority_vk),
                true,
            )
        } else {
            (crate::structs::AssetPolicy::default_unregulated(), false)
        };

        let event_policy = policy.clone();
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
                asset_policy: event_policy,
            };

            state.emit_asset_registered(event);
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
    use decaf377_rdsa::{SigningKey, SpendAuth, VerificationKey};
    use penumbra_sdk_asset::{asset, BASE_ASSET_ID};
    use penumbra_sdk_keys::Address;
    use penumbra_sdk_sct::component::clock::EpochManager;
    use rand_core::OsRng;

    use crate::genesis::NativeAssetRegistration;
    use crate::structs::{
        AssetRegistrationGrant, ComplianceLeaf, MsgRegisterAsset, UserRegistrationGrant,
        UserRegistrationGrantBody,
    };

    const TEST_BLOCK_UNIX: i64 = 1_700_000_000;
    const TEST_VALID_UNTIL_UNIX: u64 = TEST_BLOCK_UNIX as u64 + 300;

    fn set_test_block_time<S: cnidarium::StateWrite>(state: &mut S, unix: i64) {
        let timestamp =
            tendermint::Time::from_unix_timestamp(unix, 0).expect("test timestamp is valid");
        state.put_block_timestamp(1, timestamp);
    }

    fn registrar_genesis(registrar_vk: VerificationKey<SpendAuth>) -> genesis::Content {
        genesis::Content {
            native_assets: vec![],
            compliance_registrar_vk: vec![registrar_vk],
        }
    }

    fn sign_asset_registration(
        mut msg: MsgRegisterAsset,
        registrar_sk: &SigningKey<SpendAuth>,
        valid_until_unix: u64,
    ) -> MsgRegisterAsset {
        let body = msg.registration_grant_body(valid_until_unix);
        msg.asset_registration_grant = Some(AssetRegistrationGrant {
            registrar_vk: VerificationKey::from(registrar_sk),
            signature: registrar_sk.sign(OsRng, &body.signing_bytes()),
            body,
        });
        msg
    }

    fn regulated_asset_msg(
        asset_id: asset::Id,
        authority_vk: VerificationKey<SpendAuth>,
    ) -> MsgRegisterAsset {
        MsgRegisterAsset {
            asset_id,
            is_regulated: true,
            dk_pub: Some(decaf377::Element::GENERATOR),
            threshold: None,
            allowed_channels: vec![],
            ring_pk: None,
            ring_id: String::new(),
            policy_id: "test-policy".to_string(),
            permission: String::new(),
            resource: String::new(),
            registration_authority_vk: Some(authority_vk),
            asset_registration_grant: None,
        }
    }

    fn user_registration_grant(
        leaf: ComplianceLeaf,
        policy_id: String,
        authority_sk: &SigningKey<SpendAuth>,
        valid_until_unix: u64,
    ) -> UserRegistrationGrant {
        let body = UserRegistrationGrantBody {
            leaf,
            policy_id,
            valid_until_unix,
            nonce: vec![1, 2, 3, 4],
        };
        UserRegistrationGrant {
            signature: authority_sk.sign(OsRng, &body.signing_bytes()),
            body,
        }
    }

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
                registration_authority_vk: Some(VerificationKey::from(
                    &SigningKey::<SpendAuth>::new(OsRng),
                )),
            }],
            compliance_registrar_vk: vec![],
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

        let registrar_sk = SigningKey::<SpendAuth>::new(OsRng);
        let authority_sk = SigningKey::<SpendAuth>::new(OsRng);
        let authority_vk = VerificationKey::from(&authority_sk);
        let genesis = registrar_genesis(VerificationKey::from(&registrar_sk));
        Compliance::init_chain(&mut state, Some(&genesis)).await;
        set_test_block_time(&mut state, TEST_BLOCK_UNIX);

        let asset_id = asset::Id(Fq::from(1u64));
        sign_asset_registration(
            regulated_asset_msg(asset_id, authority_vk),
            &registrar_sk,
            TEST_VALID_UNTIL_UNIX,
        )
        .check_and_execute(&mut state)
        .await
        .unwrap();

        let leaf = ComplianceLeaf {
            address: Address::dummy(&mut rand::thread_rng()),
            asset_id,
            d: Fq::from(0u64),
        };
        let msg = MsgRegisterUser {
            leaf: leaf.clone(),
            grant: Some(user_registration_grant(
                leaf,
                "test-policy".to_string(),
                &authority_sk,
                TEST_VALID_UNTIL_UNIX,
            )),
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
            grant: None,
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
            grant: None,
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

        let registrar_sk = SigningKey::<SpendAuth>::new(OsRng);
        let authority_sk = SigningKey::<SpendAuth>::new(OsRng);
        let authority_vk = VerificationKey::from(&authority_sk);
        let genesis = registrar_genesis(VerificationKey::from(&registrar_sk));
        Compliance::init_chain(&mut state, Some(&genesis)).await;
        set_test_block_time(&mut state, TEST_BLOCK_UNIX);

        // Initially the asset is unregulated (not in IMT)
        let asset_id = asset::Id(Fq::from(123u64));
        let proof_before = state.get_asset_proof_data(asset_id).await.unwrap();
        assert!(!proof_before.is_regulated, "asset should start unregulated");

        // Create a register asset message (regulated) - requires dk_pub
        let dk_pub = Some(decaf377::Element::GENERATOR);
        let mut msg = regulated_asset_msg(asset_id, authority_vk);
        msg.dk_pub = dk_pub;
        let msg = sign_asset_registration(msg, &registrar_sk, TEST_VALID_UNTIL_UNIX);

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
    async fn test_asset_registration_rejects_unauthorized_registrar() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);

        let authorized_registrar_sk = SigningKey::<SpendAuth>::new(OsRng);
        let unauthorized_registrar_sk = SigningKey::<SpendAuth>::new(OsRng);
        let authority_sk = SigningKey::<SpendAuth>::new(OsRng);
        Compliance::init_chain(
            &mut state,
            Some(&registrar_genesis(VerificationKey::from(
                &authorized_registrar_sk,
            ))),
        )
        .await;
        set_test_block_time(&mut state, TEST_BLOCK_UNIX);

        let msg = sign_asset_registration(
            regulated_asset_msg(
                asset::Id(Fq::from(321u64)),
                VerificationKey::from(&authority_sk),
            ),
            &unauthorized_registrar_sk,
            TEST_VALID_UNTIL_UNIX,
        );

        let error = msg
            .check_and_execute(&mut state)
            .await
            .expect_err("unauthorized registrar must be rejected");
        assert!(
            error.to_string().contains("unauthorized registrar"),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn test_asset_registration_rejects_forged_grant() {
        let registrar_sk = SigningKey::<SpendAuth>::new(OsRng);
        let forger_sk = SigningKey::<SpendAuth>::new(OsRng);
        let authority_sk = SigningKey::<SpendAuth>::new(OsRng);
        let mut msg = regulated_asset_msg(
            asset::Id(Fq::from(322u64)),
            VerificationKey::from(&authority_sk),
        );
        let body = msg.registration_grant_body(TEST_VALID_UNTIL_UNIX);
        msg.asset_registration_grant = Some(AssetRegistrationGrant {
            registrar_vk: VerificationKey::from(&registrar_sk),
            signature: forger_sk.sign(OsRng, &body.signing_bytes()),
            body,
        });

        let error = msg
            .check_stateless(())
            .await
            .expect_err("forged grant must be rejected");
        assert!(
            error.to_string().contains("signature failed"),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn test_asset_registration_rejects_expired_grant() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);

        let registrar_sk = SigningKey::<SpendAuth>::new(OsRng);
        let authority_sk = SigningKey::<SpendAuth>::new(OsRng);
        Compliance::init_chain(
            &mut state,
            Some(&registrar_genesis(VerificationKey::from(&registrar_sk))),
        )
        .await;
        set_test_block_time(&mut state, TEST_BLOCK_UNIX);

        let msg = sign_asset_registration(
            regulated_asset_msg(
                asset::Id(Fq::from(323u64)),
                VerificationKey::from(&authority_sk),
            ),
            &registrar_sk,
            TEST_BLOCK_UNIX as u64 - 1,
        );

        let error = msg
            .check_and_execute(&mut state)
            .await
            .expect_err("expired grant must be rejected");
        assert!(
            error.to_string().contains("expired"),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn test_user_registration_rejects_missing_wrong_and_expired_grants() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);

        let registrar_sk = SigningKey::<SpendAuth>::new(OsRng);
        let authority_sk = SigningKey::<SpendAuth>::new(OsRng);
        let wrong_authority_sk = SigningKey::<SpendAuth>::new(OsRng);
        let asset_id = asset::Id(Fq::from(324u64));
        Compliance::init_chain(
            &mut state,
            Some(&registrar_genesis(VerificationKey::from(&registrar_sk))),
        )
        .await;
        set_test_block_time(&mut state, TEST_BLOCK_UNIX);

        sign_asset_registration(
            regulated_asset_msg(asset_id, VerificationKey::from(&authority_sk)),
            &registrar_sk,
            TEST_VALID_UNTIL_UNIX,
        )
        .check_and_execute(&mut state)
        .await
        .unwrap();

        let leaf = ComplianceLeaf {
            address: Address::dummy(&mut rand::thread_rng()),
            asset_id,
            d: Fq::from(0u64),
        };

        let missing_grant = MsgRegisterUser {
            leaf: leaf.clone(),
            grant: None,
        };
        assert!(missing_grant.check_stateless(()).await.is_err());

        let wrong_grant = MsgRegisterUser {
            leaf: leaf.clone(),
            grant: Some(user_registration_grant(
                leaf.clone(),
                "test-policy".to_string(),
                &wrong_authority_sk,
                TEST_VALID_UNTIL_UNIX,
            )),
        };
        let error = wrong_grant
            .check_and_execute(&mut state)
            .await
            .expect_err("wrong authority must be rejected");
        assert!(
            error.to_string().contains("signature failed"),
            "unexpected error: {error}"
        );

        let expired_grant = MsgRegisterUser {
            leaf: leaf.clone(),
            grant: Some(user_registration_grant(
                leaf,
                "test-policy".to_string(),
                &authority_sk,
                TEST_BLOCK_UNIX as u64 - 1,
            )),
        };
        let error = expired_grant
            .check_and_execute(&mut state)
            .await
            .expect_err("expired user grant must be rejected");
        assert!(
            error.to_string().contains("expired"),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn test_msg_register_unregulated_asset_is_noop() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);

        let registrar_sk = SigningKey::<SpendAuth>::new(OsRng);
        Compliance::init_chain(
            &mut state,
            Some(&registrar_genesis(VerificationKey::from(&registrar_sk))),
        )
        .await;
        set_test_block_time(&mut state, TEST_BLOCK_UNIX);

        let asset_id = asset::Id(Fq::from(456u64));

        // Create a register asset message (unregulated)
        let msg = sign_asset_registration(
            MsgRegisterAsset {
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
                registration_authority_vk: None,
                asset_registration_grant: None,
            },
            &registrar_sk,
            TEST_VALID_UNTIL_UNIX,
        );

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

        let registrar_sk = SigningKey::<SpendAuth>::new(OsRng);
        let authority_sk = SigningKey::<SpendAuth>::new(OsRng);
        let authority_vk = VerificationKey::from(&authority_sk);
        Compliance::init_chain(
            &mut state,
            Some(&registrar_genesis(VerificationKey::from(&registrar_sk))),
        )
        .await;
        set_test_block_time(&mut state, TEST_BLOCK_UNIX);

        let asset_id = asset::Id(Fq::from(789u64));

        // Create a register asset message (regulated but missing dk_pub)
        let msg = sign_asset_registration(
            MsgRegisterAsset {
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
                registration_authority_vk: Some(authority_vk),
                asset_registration_grant: None,
            },
            &registrar_sk,
            TEST_VALID_UNTIL_UNIX,
        );

        // Execute the action - should fail
        let result = msg.check_and_execute(&mut state).await;
        assert!(
            result.is_err(),
            "registering regulated asset without dk_pub should fail"
        );
    }
}
