use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroU64,
    sync::Arc,
    time::Duration,
};

use anyhow::{anyhow, Context};
use camino::Utf8Path;
use decaf377::Fq;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use r2d2_sqlite::{
    rusqlite::{self, OpenFlags, OptionalExtension},
    SqliteConnectionManager,
};
use sha2::{Digest, Sha256};
use tap::{Tap, TapFallible};
use tokio::{
    sync::broadcast::{self, error::RecvError},
    task::spawn_blocking,
};
use tracing::{error_span, Instrument};
use url::Url;

use sct::TreeStore;
use shieldd_sdk_app::params::AppParameters;
use shieldd_sdk_asset::{asset, asset::Id, asset::Metadata, Value};
use shieldd_sdk_compliance::{AssetPolicy, ComplianceLeaf};
use shieldd_sdk_fee::GasPrices;
use shieldd_sdk_keys::{
    keys::{AddressIndex, NullifierKey},
    Address, FullViewingKey,
};
use shieldd_sdk_num::Amount;
use shieldd_sdk_proto::{
    core::app::v1::{
        query_service_client::QueryServiceClient as AppQueryServiceClient, AppParametersRequest,
    },
    core::component::sct::v1 as pb_sct,
    DomainType, Message,
};
use shieldd_sdk_sct::{nullifier_generation::NullifierWindow, CommitmentSource, Nullifier};
use shieldd_sdk_shielded_pool::{
    discovery, note, Note, Rseed, VolumeAccumulatorPayload, VolumeAccumulatorState,
};
use shieldd_sdk_tct::{self as tct, builder::epoch::Root};
use shieldd_sdk_transaction::Transaction;
use tct::StateCommitment;

use crate::{
    historical_proof_cache::{HistoricalProofCache, HistoricalProofCacheState},
    issued_address::{AddressPurpose, IssuedAddress},
    sync::FilteredBlock,
    SpendableNoteRecord,
};

pub(crate) mod compliance;
mod sct;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct BalanceEntry {
    pub id: Id,
    pub amount: u128,
    pub address_index: AddressIndex,
}

#[derive(Debug, Clone)]
pub struct ConfirmedVolumeAccumulator {
    pub state: VolumeAccumulatorState,
    pub commitment: StateCommitment,
    pub position: tct::Position,
}

#[derive(Debug, Clone)]
pub enum VolumeAccumulatorRecovery {
    Absent,
    Complete(ConfirmedVolumeAccumulator),
    Incomplete,
}

#[derive(Debug, Clone)]
pub struct VolumeAccumulatorReservation {
    pub state: VolumeAccumulatorState,
    pub payload: VolumeAccumulatorPayload,
    pub expires_at: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct ComplianceLeafUpdate {
    pub leaf: ComplianceLeaf,
    pub position: u64,
    pub commitment: StateCommitment,
}

#[derive(Debug, Clone)]
pub(crate) struct ComplianceAssetPolicyUpdate {
    pub asset_id: asset::Id,
    pub policy: AssetPolicy,
}

#[derive(Debug, Clone)]
pub(crate) struct ComplianceBlockPlan {
    pub height: u64,
    pub user_tree: crate::compliance_tree::ComplianceUserTreePersistence,
    pub asset_tree: crate::compliance_tree::ComplianceAssetTreePersistence,
    pub user_root: StateCommitment,
    pub asset_root: StateCommitment,
    pub leaf_updates: Vec<ComplianceLeafUpdate>,
    pub asset_policy_updates: Vec<ComplianceAssetPolicyUpdate>,
}

impl ComplianceBlockPlan {
    pub(crate) fn user_leaf(
        &self,
        address: &Address,
        asset_id: asset::Id,
    ) -> Option<&ComplianceLeaf> {
        self.leaf_updates
            .iter()
            .rev()
            .find(|update| &update.leaf.address == address && update.leaf.asset_id == asset_id)
            .map(|update| &update.leaf)
    }

    pub(crate) fn asset_policy(&self, asset_id: asset::Id) -> Option<&AssetPolicy> {
        self.asset_policy_updates
            .iter()
            .rev()
            .find(|update| update.asset_id == asset_id)
            .map(|update| &update.policy)
    }
}

#[cfg(test)]
mod compliance_projection_tests {
    use super::*;
    use shieldd_sdk_keys::test_keys;

    #[tokio::test]
    async fn compliance_block_plan_exposes_same_block_policy_and_user_leaf() {
        let storage = Storage::initialize(
            None::<&Utf8Path>,
            (*test_keys::FULL_VIEWING_KEY).clone(),
            AppParameters::default(),
        )
        .await
        .unwrap();
        let user_tree = storage.compliance_user_tree().await.unwrap();
        let asset_tree = storage.compliance_asset_tree().await.unwrap();
        let asset_id = asset::Id(Fq::from(13u64));
        let leaf = ComplianceLeaf::synthetic_unregulated(test_keys::ADDRESS_0.clone(), asset_id);
        let policy = AssetPolicy::for_test(
            decaf377::Element::GENERATOR,
            u128::MAX,
            decaf377::Element::GENERATOR,
        );
        let plan = ComplianceBlockPlan {
            height: 1,
            user_tree: user_tree.persistence_plan().unwrap(),
            asset_tree: asset_tree.persistence_plan().unwrap(),
            user_root: user_tree.root(),
            asset_root: asset_tree.root(),
            leaf_updates: vec![ComplianceLeafUpdate {
                commitment: leaf.commit(),
                position: 0,
                leaf: leaf.clone(),
            }],
            asset_policy_updates: vec![ComplianceAssetPolicyUpdate {
                asset_id,
                policy: policy.clone(),
            }],
        };

        assert_eq!(plan.user_leaf(&test_keys::ADDRESS_0, asset_id), Some(&leaf));
        assert_eq!(plan.asset_policy(asset_id), Some(&policy));
    }

    #[tokio::test]
    async fn compliance_block_failure_rolls_back_leaf_tree_and_anchor_writes() {
        let storage = Storage::initialize(
            None::<&Utf8Path>,
            (*test_keys::FULL_VIEWING_KEY).clone(),
            AppParameters::default(),
        )
        .await
        .unwrap();
        let mut user_tree = storage.compliance_user_tree().await.unwrap();
        let asset_tree = storage.compliance_asset_tree().await.unwrap();
        let leaf = ComplianceLeaf::synthetic_unregulated(
            test_keys::ADDRESS_0.clone(),
            asset::Id(Fq::from(17u64)),
        );
        let commitment = leaf.commit();
        let position = user_tree.insert(commitment).unwrap();
        let user_root = user_tree.root();
        let asset_root = asset_tree.root();

        let plan = ComplianceBlockPlan {
            height: u64::MAX,
            user_tree: user_tree.persistence_plan().unwrap(),
            asset_tree: asset_tree.persistence_plan().unwrap(),
            user_root,
            asset_root,
            leaf_updates: vec![ComplianceLeafUpdate {
                leaf: leaf.clone(),
                position,
                commitment,
            }],
            asset_policy_updates: Vec::new(),
        };
        let mut conn = storage.pool.get().unwrap();
        let mut tx = conn.transaction().unwrap();
        Storage::record_compliance_plan_inner(&mut tx, plan)
            .expect_err("an unrepresentable anchor height must abort the transaction");
        drop(tx);
        drop(conn);

        assert_eq!(storage.compliance_user_tree().await.unwrap().position(), 0);
        assert!(storage
            .get_compliance_leaf_data(&leaf.address, &leaf.asset_id)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn wallet_block_failure_rolls_back_compliance_projection_and_sync_height() {
        let storage = Storage::initialize(
            None::<&Utf8Path>,
            (*test_keys::FULL_VIEWING_KEY).clone(),
            AppParameters::default(),
        )
        .await
        .unwrap();
        let mut user_tree = storage.compliance_user_tree().await.unwrap();
        let asset_tree = storage.compliance_asset_tree().await.unwrap();
        let leaf = ComplianceLeaf::synthetic_unregulated(
            test_keys::ADDRESS_0.clone(),
            asset::Id(Fq::from(19u64)),
        );
        let commitment = leaf.commit();
        let position = user_tree.insert(commitment).unwrap();
        let user_root = user_tree.root();
        let asset_root = asset_tree.root();
        let plan = ComplianceBlockPlan {
            height: 0,
            user_tree: user_tree.persistence_plan().unwrap(),
            asset_tree: asset_tree.persistence_plan().unwrap(),
            user_root,
            asset_root,
            leaf_updates: vec![ComplianceLeafUpdate {
                leaf: leaf.clone(),
                position,
                commitment,
            }],
            asset_policy_updates: Vec::new(),
        };
        storage
            .pool
            .get()
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER fail_sync_height BEFORE UPDATE ON sync_height BEGIN SELECT RAISE(ABORT, 'injected sync failure'); END;",
            )
            .unwrap();
        let filtered_block = FilteredBlock {
            new_notes: BTreeMap::new(),
            spent_nullifiers: Vec::new(),
            height: 0,
            discovery_parameters: None,
            app_parameters_updated: false,
            gas_prices: None,
            nullifier_window: None,
            volume_accumulators: Vec::new(),
        };
        let mut sct = tct::Tree::new();
        let channel = tonic::transport::Endpoint::from_static("http://127.0.0.1:1").connect_lazy();
        storage
            .record_block(filtered_block, Vec::new(), &mut sct, channel, Some(plan))
            .await
            .expect_err("injected sync-height failure must abort the wallet transaction");

        assert_eq!(storage.last_sync_height().await.unwrap(), None);
        assert_eq!(storage.compliance_user_tree().await.unwrap().position(), 0);
        assert!(storage
            .get_compliance_leaf_data(&leaf.address, &leaf.asset_id)
            .await
            .unwrap()
            .is_none());
    }
}

#[cfg(test)]
mod note_storage_tests {
    use super::*;
    use decaf377::Element;
    use shieldd_sdk_asset::BASE_ASSET_ID;
    use shieldd_sdk_keys::test_keys;

    #[tokio::test]
    async fn note_advice_round_trip_preserves_recovery_commitment() {
        let storage = Storage::initialize(
            None::<&Utf8Path>,
            (*test_keys::FULL_VIEWING_KEY).clone(),
            AppParameters::default(),
        )
        .await
        .unwrap();
        let (note, _) = Note::from_parts_with_recovery(
            test_keys::ADDRESS_0.clone(),
            Value {
                amount: Amount::from(41u64),
                asset_id: *BASE_ASSET_ID,
            },
            Rseed([42u8; 32]),
            Element::GENERATOR,
        )
        .unwrap();
        let commitment = note.commit();

        storage.give_advice(note.clone()).await.unwrap();
        let advice = storage.scan_advice(vec![commitment]).await.unwrap();
        let restored = advice
            .get(&commitment)
            .expect("stored note must be returned");

        assert_eq!(restored, &note);
        assert_eq!(restored.recovery_commitment(), note.recovery_commitment());
    }
}

#[cfg(test)]
mod issued_address_tests {
    use super::*;
    use camino::Utf8Path;
    use shieldd_sdk_app::params::AppParameters;
    use shieldd_sdk_keys::{keys::AddressIndex, test_keys};

    #[tokio::test]
    async fn fresh_storage_distinguishes_an_uninitialized_nullifier_window() {
        let storage = Storage::initialize(
            None::<&Utf8Path>,
            (*test_keys::FULL_VIEWING_KEY).clone(),
            AppParameters::default(),
        )
        .await
        .unwrap();

        assert!(storage
            .nullifier_window_if_initialized()
            .await
            .unwrap()
            .is_none());
        assert!(storage.nullifier_window().await.is_err());
    }

    #[tokio::test]
    async fn restore_recovers_standard_and_randomized_issued_addresses() {
        let directory = tempfile::tempdir().unwrap();
        let raw_path = directory.path().join("view.sqlite");
        let path = Utf8Path::from_path(&raw_path).unwrap().to_owned();
        let fvk = (*test_keys::FULL_VIEWING_KEY).clone();
        let storage = Storage::initialize(Some(&path), fvk.clone(), AppParameters::default())
            .await
            .unwrap();
        let general_index = AddressIndex::new(7);
        let regulated_index = AddressIndex {
            account: 9,
            randomizer: [0x5a; 12],
        };
        let regulated_asset = asset::Id(decaf377::Fq::from(42u64));
        let issued = [
            IssuedAddress {
                address_index: general_index,
                address: fvk.payment_address(general_index),
                purpose: AddressPurpose::General,
                birth_height: 11,
                retired_height: None,
            },
            IssuedAddress {
                address_index: regulated_index,
                address: fvk.payment_address(regulated_index),
                purpose: AddressPurpose::Regulated {
                    asset_id: regulated_asset,
                },
                birth_height: 12,
                retired_height: None,
            },
        ];
        for address in issued.clone() {
            storage.record_issued_address(address).await.unwrap();
        }
        drop(storage);

        let restored = Storage::load(&path).await.unwrap();
        assert_eq!(restored.issued_addresses().await.unwrap(), issued);
    }

    #[tokio::test]
    async fn issued_address_birth_height_is_write_once_and_address_can_cover_many_assets() {
        let fvk = (*test_keys::FULL_VIEWING_KEY).clone();
        let storage = Storage::initialize(None::<&Utf8Path>, fvk.clone(), AppParameters::default())
            .await
            .unwrap();
        let index = AddressIndex::new(3);
        let issued = IssuedAddress {
            address_index: index,
            address: fvk.payment_address(index),
            purpose: AddressPurpose::General,
            birth_height: 8,
            retired_height: None,
        };
        storage.record_issued_address(issued.clone()).await.unwrap();
        storage.record_issued_address(issued.clone()).await.unwrap();

        let mut reissued_later = issued.clone();
        reissued_later.birth_height = 42;
        storage.record_issued_address(reissued_later).await.unwrap();
        assert_eq!(
            storage.issued_addresses().await.unwrap(),
            vec![issued.clone()],
            "reissuing an address preserves its original birth height"
        );

        let mut conflicting = issued;
        conflicting.purpose = AddressPurpose::Regulated {
            asset_id: asset::Id(decaf377::Fq::from(9u64)),
        };
        let assigned = storage
            .record_issued_address(conflicting.clone())
            .await
            .unwrap();
        assert_eq!(assigned, conflicting.address);

        let mut second_asset = conflicting.clone();
        second_asset.purpose = AddressPurpose::Regulated {
            asset_id: asset::Id(decaf377::Fq::from(10u64)),
        };
        assert_eq!(
            storage.record_issued_address(second_asset).await.unwrap(),
            conflicting.address
        );

        let other_index = AddressIndex::new(4);
        let other = IssuedAddress {
            address_index: other_index,
            address: fvk.payment_address(other_index),
            purpose: conflicting.purpose,
            birth_height: 9,
            retired_height: None,
        };
        assert_eq!(
            storage.record_issued_address(other).await.unwrap(),
            conflicting.address,
            "an existing regulated-asset assignment is immutable"
        );
        assert!(storage
            .retire_issued_address(index, 100)
            .await
            .unwrap_err()
            .to_string()
            .contains("permanently assigned"));
    }

    #[tokio::test]
    async fn historical_proof_cache_round_trips_and_deletes() {
        let storage = Storage::initialize(
            None::<&Utf8Path>,
            (*test_keys::FULL_VIEWING_KEY).clone(),
            AppParameters::default(),
        )
        .await
        .unwrap();
        let nullifier = Nullifier(Fq::from(77u64));
        let cache = HistoricalProofCache::pending(nullifier);
        storage
            .put_historical_proof_cache(cache.clone())
            .await
            .unwrap();
        assert_eq!(
            storage.historical_proof_cache(nullifier).await.unwrap(),
            Some(cache)
        );
        storage
            .delete_historical_proof_cache(nullifier)
            .await
            .unwrap();
        assert!(storage
            .historical_proof_cache(nullifier)
            .await
            .unwrap()
            .is_none());
    }
}

#[cfg(test)]
mod volume_accumulator_tests {
    use super::*;
    use camino::Utf8Path;
    use shieldd_sdk_app::params::AppParameters;
    use shieldd_sdk_keys::test_keys;

    fn state() -> VolumeAccumulatorState {
        VolumeAccumulatorState {
            subject: VolumeAccumulatorState::subject(
                &test_keys::ADDRESS_0,
                asset::Id(Fq::from(91u64)),
            ),
            day_start: 86_400,
            undisclosed_volume: 50,
            blinding: Fq::from(7u64),
        }
    }

    fn payload(state: &VolumeAccumulatorState) -> VolumeAccumulatorPayload {
        let commitment = state.commitment();
        VolumeAccumulatorPayload::encrypt(
            state,
            true,
            state.origin_nullifier(test_keys::FULL_VIEWING_KEY.nullifier_key()),
            commitment,
            test_keys::FULL_VIEWING_KEY.outgoing(),
        )
    }

    #[tokio::test]
    async fn reservation_is_exclusive_until_release_or_strict_expiry() {
        let storage = Storage::initialize(
            None::<&Utf8Path>,
            (*test_keys::FULL_VIEWING_KEY).clone(),
            AppParameters::default(),
        )
        .await
        .unwrap();
        let state = state();
        let payload = payload(&state);
        let reservation = |expires_at| VolumeAccumulatorReservation {
            state: state.clone(),
            payload: payload.clone(),
            expires_at,
        };
        let nk = *test_keys::FULL_VIEWING_KEY.nullifier_key();

        storage
            .reserve_volume_accumulators(vec![reservation(120)], [1; 32], 100, nk)
            .await
            .unwrap();
        assert!(storage
            .reserve_volume_accumulators(vec![reservation(140)], [2; 32], 120, nk)
            .await
            .is_err());
        storage
            .reserve_volume_accumulators(vec![reservation(140)], [2; 32], 121, nk)
            .await
            .unwrap();
        storage
            .release_volume_reservation(payload.scoped_nullifier())
            .await
            .unwrap();
        assert!(matches!(
            storage
                .volume_accumulator_recovery(state.subject, state.day_start)
                .await
                .unwrap(),
            VolumeAccumulatorRecovery::Absent
        ));
        storage
            .reserve_volume_accumulators(vec![reservation(160)], [3; 32], 140, nk)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn incomplete_recovery_heads_are_explicit() {
        let storage = Storage::initialize(
            None::<&Utf8Path>,
            (*test_keys::FULL_VIEWING_KEY).clone(),
            AppParameters::default(),
        )
        .await
        .unwrap();
        let state = state();
        storage
            .pool
            .get()
            .unwrap()
            .execute(
                "INSERT INTO volume_accumulators
                 (subject, day_start, volume, blinding, commitment, position, recovery_status)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)",
                (
                    state.subject.to_bytes().to_vec(),
                    state.day_start as i64,
                    state.undisclosed_volume.to_le_bytes().to_vec(),
                    state.blinding.to_bytes().to_vec(),
                    state.commitment().0.to_bytes().to_vec(),
                    7i64,
                ),
            )
            .unwrap();

        assert!(matches!(
            storage
                .volume_accumulator_recovery(state.subject, state.day_start)
                .await
                .unwrap(),
            VolumeAccumulatorRecovery::Incomplete
        ));
    }

    #[tokio::test]
    async fn multi_reservation_failure_is_atomic_and_stale_origins_reject() {
        let storage = Storage::initialize(
            None::<&Utf8Path>,
            (*test_keys::FULL_VIEWING_KEY).clone(),
            AppParameters::default(),
        )
        .await
        .unwrap();
        let state = state();
        let payload = payload(&state);
        let reservation = VolumeAccumulatorReservation {
            state: state.clone(),
            payload: payload.clone(),
            expires_at: 200,
        };
        let nk = *test_keys::FULL_VIEWING_KEY.nullifier_key();
        assert!(storage
            .reserve_volume_accumulators(
                vec![reservation.clone(), reservation.clone()],
                [1; 32],
                100,
                nk,
            )
            .await
            .is_err());
        let reservation_count: i64 = storage
            .pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM volume_accumulator_reservations",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(reservation_count, 0);

        storage
            .pool
            .get()
            .unwrap()
            .execute(
                "INSERT INTO volume_accumulators
                 (subject, day_start, volume, blinding, commitment, position, recovery_status)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
                (
                    state.subject.to_bytes().to_vec(),
                    state.day_start as i64,
                    state.undisclosed_volume.to_le_bytes().to_vec(),
                    state.blinding.to_bytes().to_vec(),
                    state.commitment().0.to_bytes().to_vec(),
                    7i64,
                ),
            )
            .unwrap();
        assert!(storage
            .reserve_volume_accumulators(vec![reservation], [2; 32], 100, nk)
            .await
            .unwrap_err()
            .to_string()
            .contains("head changed"));
    }
}

/// The hash of the schema for the database.
static SCHEMA_HASH: Lazy<String> =
    Lazy::new(|| hex::encode(Sha256::digest(include_str!("storage/schema.sql"))));

#[derive(Clone)]
pub struct Storage {
    pool: r2d2::Pool<SqliteConnectionManager>,

    /// This allows an optimization where we only commit to the database after
    /// scanning a nonempty block.
    ///
    /// If this is `Some`, we have uncommitted empty blocks up to the inner height.
    /// If this is `None`, we don't.
    ///
    /// Using a `NonZeroU64` ensures that `Option<NonZeroU64>` fits in 8 bytes.
    uncommitted_height: Arc<Mutex<Option<NonZeroU64>>>,

    scanned_notes_tx: tokio::sync::broadcast::Sender<SpendableNoteRecord>,
    scanned_nullifiers_tx: tokio::sync::broadcast::Sender<Nullifier>,
}

impl Storage {
    pub async fn volume_accumulator_recovery(
        &self,
        subject: Fq,
        day_start: u64,
    ) -> anyhow::Result<VolumeAccumulatorRecovery> {
        let pool = self.pool.clone();
        spawn_blocking(move || {
            let connection = pool.get()?;
            let row = connection
                .query_row(
                    "SELECT volume, blinding, commitment, position, recovery_status
                     FROM volume_accumulators
                     WHERE subject = ?1 AND day_start = ?2",
                    (subject.to_bytes().to_vec(), day_start as i64),
                    |row| {
                        Ok((
                            row.get::<_, Option<Vec<u8>>>(0)?,
                            row.get::<_, Option<Vec<u8>>>(1)?,
                            row.get::<_, Option<Vec<u8>>>(2)?,
                            row.get::<_, Option<i64>>(3)?,
                            row.get::<_, i64>(4)?,
                        ))
                    },
                )
                .optional()?;
            let Some((volume, blinding, commitment, position, recovery_status)) = row else {
                return Ok(VolumeAccumulatorRecovery::Absent);
            };
            if recovery_status != 0 {
                return Ok(VolumeAccumulatorRecovery::Incomplete);
            }
            let volume: [u8; 16] = volume
                .ok_or_else(|| anyhow!("complete volume accumulator is missing its amount"))?
                .try_into()
                .map_err(|_| anyhow!("stored volume accumulator amount is malformed"))?;
            let blinding = Fq::from_bytes_checked(
                &blinding
                    .ok_or_else(|| anyhow!("complete volume accumulator is missing its blinding"))?
                    .try_into()
                    .map_err(|_| anyhow!("stored volume accumulator blinding is malformed"))?,
            )
            .map_err(|_| anyhow!("stored volume accumulator blinding is noncanonical"))?;
            let commitment = commitment
                .ok_or_else(|| anyhow!("complete volume accumulator is missing its commitment"))?;
            let commitment = StateCommitment::try_from(commitment.as_slice())?;
            let position: u64 = position
                .ok_or_else(|| anyhow!("complete volume accumulator is missing its position"))?
                .try_into()
                .map_err(|_| anyhow!("stored volume accumulator position is negative"))?;
            Ok(VolumeAccumulatorRecovery::Complete(
                ConfirmedVolumeAccumulator {
                    state: VolumeAccumulatorState {
                        subject,
                        day_start,
                        undisclosed_volume: u128::from_le_bytes(volume),
                        blinding,
                    },
                    commitment,
                    position: tct::Position::from(position),
                },
            ))
        })
        .await?
    }

    pub async fn reserve_volume_accumulators(
        &self,
        reservations: Vec<VolumeAccumulatorReservation>,
        tx_id: [u8; 32],
        chain_time: u64,
        nk: NullifierKey,
    ) -> anyhow::Result<()> {
        let pool = self.pool.clone();
        spawn_blocking(move || {
            let mut connection = pool.get()?;
            let transaction = connection.transaction()?;
            transaction.execute(
                "DELETE FROM volume_accumulator_reservations WHERE expires_at < ?1",
                [chain_time as i64],
            )?;

            let mut subjects = BTreeSet::new();
            for reservation in &reservations {
                let state = &reservation.state;
                let payload = &reservation.payload;
                anyhow::ensure!(
                    subjects.insert((state.subject.to_bytes(), state.day_start)),
                    "transaction contains more than one real accumulator transition for the same subject and day"
                );
                anyhow::ensure!(
                    payload.day_start == state.day_start && payload.commitment == state.commitment(),
                    "volume accumulator reservation does not match its decrypted successor"
                );

                let prior: Option<(Vec<u8>, i64, i64)> = transaction
                    .query_row(
                        "SELECT commitment, position, recovery_status
                         FROM volume_accumulators WHERE subject = ?1 AND day_start = ?2",
                        (state.subject.to_bytes().to_vec(), state.day_start as i64),
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .optional()?;
                let expected = match prior {
                    Some((commitment, position, recovery_status)) => {
                        anyhow::ensure!(
                            recovery_status == 0,
                            "daily volume accumulator head is incomplete; rebuild with issuer disclosure"
                        );
                        let commitment = StateCommitment::try_from(commitment.as_slice())?;
                        let position: u64 = position
                            .try_into()
                            .context("stored volume accumulator position is negative")?;
                        Nullifier::derive(&nk, tct::Position::from(position), &commitment)
                    }
                    None => state.origin_nullifier(&nk),
                };
                anyhow::ensure!(
                    payload.nullifier == expected,
                    "daily volume accumulator head changed after planning; rebuild the transaction"
                );

                let busy: bool = transaction.query_row(
                    "SELECT EXISTS(SELECT 1 FROM volume_accumulator_reservations
                     WHERE subject = ?1 AND day_start = ?2)",
                    (state.subject.to_bytes().to_vec(), state.day_start as i64),
                    |row| row.get(0),
                )?;
                anyhow::ensure!(
                    !busy,
                    "daily volume accumulator head is reserved by an in-flight transaction; request issuer disclosure or wait"
                );
            }

            for reservation in reservations {
                transaction.execute(
                    "INSERT INTO volume_accumulator_reservations
                     (subject, day_start, nullifier, tx_id, expires_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    (
                        reservation.state.subject.to_bytes().to_vec(),
                        reservation.state.day_start as i64,
                        reservation.payload.nullifier.to_bytes().to_vec(),
                        tx_id.to_vec(),
                        reservation.expires_at as i64,
                    ),
                )?;
            }
            transaction.commit()?;
            anyhow::Ok(())
        })
        .await?
    }

    pub async fn release_volume_reservation(
        &self,
        scoped: shieldd_sdk_shielded_pool::VolumeNullifier,
    ) -> anyhow::Result<()> {
        let pool = self.pool.clone();
        spawn_blocking(move || {
            pool.get()?.execute(
                "DELETE FROM volume_accumulator_reservations
                 WHERE day_start = ?1 AND nullifier = ?2",
                (
                    scoped.day_start as i64,
                    scoped.nullifier.to_bytes().to_vec(),
                ),
            )?;
            anyhow::Ok(())
        })
        .await?
    }

    fn put_historical_proof_cache_inner(
        connection: &rusqlite::Connection,
        cache: &HistoricalProofCache,
    ) -> anyhow::Result<()> {
        cache.validate()?;
        let proof: pb_sct::HistoricalNullifierProof = cache.proof.clone().into();
        connection.execute(
            "INSERT INTO historical_proof_cache
             (nullifier, protocol_version, covered_generation_count, terminal_history_head, proof_bundle, cache_state, last_error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(nullifier) DO UPDATE SET
               protocol_version = excluded.protocol_version,
               covered_generation_count = excluded.covered_generation_count,
               terminal_history_head = excluded.terminal_history_head,
               proof_bundle = excluded.proof_bundle,
               cache_state = excluded.cache_state,
               last_error = excluded.last_error",
            rusqlite::params![
                cache.nullifier.to_bytes().to_vec(),
                cache.protocol_version,
                cache.covered_generation_count,
                cache.terminal_history_head.to_vec(),
                proof.encode_to_vec(),
                cache.state.storage_id(),
                cache.last_error.as_deref(),
            ],
        )?;
        Ok(())
    }

    pub async fn put_historical_proof_cache(
        &self,
        cache: HistoricalProofCache,
    ) -> anyhow::Result<()> {
        let pool = self.pool.clone();
        spawn_blocking(move || {
            let connection = pool.get()?;
            Self::put_historical_proof_cache_inner(&connection, &cache)
        })
        .await?
    }

    pub async fn historical_proof_cache(
        &self,
        nullifier: Nullifier,
    ) -> anyhow::Result<Option<HistoricalProofCache>> {
        let pool = self.pool.clone();
        spawn_blocking(move || {
            let row = pool
                .get()?
                .prepare_cached(
                    "SELECT protocol_version, covered_generation_count, terminal_history_head,
                            proof_bundle, cache_state, last_error
                     FROM historical_proof_cache WHERE nullifier = ?1",
                )?
                .query_row([nullifier.to_bytes().to_vec()], |row| {
                    Ok((
                        row.get::<_, u32>(0)?,
                        row.get::<_, u64>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                })
                .optional()?;
            let Some((
                protocol_version,
                covered_generation_count,
                terminal_history_head,
                proof_bundle,
                cache_state,
                last_error,
            )) = row
            else {
                return Ok(None);
            };
            let terminal_history_head: [u8; 32] =
                terminal_history_head.try_into().map_err(|bytes: Vec<u8>| {
                    anyhow!("history head must be 32 bytes, got {}", bytes.len())
                })?;
            let proof =
                pb_sct::HistoricalNullifierProof::decode(proof_bundle.as_slice())?.try_into()?;
            let cache = HistoricalProofCache {
                protocol_version,
                nullifier,
                covered_generation_count,
                terminal_history_head,
                proof,
                state: HistoricalProofCacheState::from_storage_id(cache_state)?,
                last_error,
            };
            cache.validate()?;
            Ok(Some(cache))
        })
        .await?
    }

    pub async fn historical_proof_caches_for_unspent_notes(
        &self,
    ) -> anyhow::Result<Vec<HistoricalProofCache>> {
        let pool = self.pool.clone();
        spawn_blocking(move || {
            let connection = pool.get()?;
            let mut statement = connection.prepare_cached(
                "SELECT c.nullifier, c.protocol_version, c.covered_generation_count,
                        c.terminal_history_head, c.proof_bundle, c.cache_state, c.last_error
                 FROM historical_proof_cache c
                 JOIN spendable_notes n ON n.nullifier = c.nullifier
                 WHERE n.height_spent IS NULL
                 ORDER BY n.position ASC",
            )?;
            let caches = statement
                .query_and_then([], |row| {
                    let nullifier_bytes: Vec<u8> = row.get(0)?;
                    let terminal_history_head: Vec<u8> = row.get(3)?;
                    let proof_bundle: Vec<u8> = row.get(4)?;
                    let cache = HistoricalProofCache {
                        protocol_version: row.get(1)?,
                        nullifier: Nullifier::try_from(nullifier_bytes)?,
                        covered_generation_count: row.get(2)?,
                        terminal_history_head: terminal_history_head.try_into().map_err(
                            |bytes: Vec<u8>| {
                                anyhow!("history head must be 32 bytes, got {}", bytes.len())
                            },
                        )?,
                        proof: pb_sct::HistoricalNullifierProof::decode(proof_bundle.as_slice())?
                            .try_into()?,
                        state: HistoricalProofCacheState::from_storage_id(row.get(5)?)?,
                        last_error: row.get(6)?,
                    };
                    cache.validate()?;
                    anyhow::Ok(cache)
                })?
                .collect::<anyhow::Result<Vec<_>>>()?;
            Ok(caches)
        })
        .await?
    }

    pub async fn delete_historical_proof_cache(&self, nullifier: Nullifier) -> anyhow::Result<()> {
        let pool = self.pool.clone();
        spawn_blocking(move || {
            pool.get()?.execute(
                "DELETE FROM historical_proof_cache WHERE nullifier = ?1",
                [nullifier.to_bytes().to_vec()],
            )?;
            anyhow::Ok(())
        })
        .await?
    }

    /// If the database at `storage_path` exists, [`Self::load`] it, otherwise, [`Self::initialize`] it.
    #[tracing::instrument(
        skip_all,
        fields(
            path = ?storage_path.as_ref().map(|p| p.as_ref().as_str()),
            url = %node,
        )
    )]
    pub async fn load_or_initialize(
        storage_path: Option<impl AsRef<Utf8Path>>,
        fvk: &FullViewingKey,
        node: Url,
    ) -> anyhow::Result<Self> {
        if let Some(path) = storage_path.as_ref().map(AsRef::as_ref) {
            if path.exists() {
                tracing::debug!(?path, "database exists");
                return Self::load(path).await;
            } else {
                tracing::debug!(?path, "database does not exist");
            }
        };

        let mut client = AppQueryServiceClient::connect(node.to_string())
            .instrument(error_span!("connecting_to_endpoint"))
            .await
            .tap_err(|error| {
                tracing::error!(?error, "failed to connect to app query service endpoint")
            })?
            .tap(|_| tracing::debug!("connected to app query service endpoint"));
        let params = client
            .app_parameters(tonic::Request::new(AppParametersRequest {}))
            .instrument(error_span!("getting_app_parameters"))
            .await?
            .into_inner()
            .try_into()?;

        Self::initialize(storage_path, fvk.clone(), params).await
    }

    fn connect(
        path: Option<impl AsRef<Utf8Path>>,
    ) -> anyhow::Result<r2d2::Pool<SqliteConnectionManager>> {
        if let Some(path) = path {
            let manager = SqliteConnectionManager::file(path.as_ref())
                .with_flags(
                    // Don't allow opening URIs, because they can change the behavior of the database; we
                    // just want to open normal filepaths.
                    OpenFlags::default() & !OpenFlags::SQLITE_OPEN_URI,
                )
                .with_init(|conn| {
                    // "NORMAL" will be consistent, but maybe not durable -- this is fine,
                    // since all our data is being synced from the chain, so if we lose a dbtx,
                    // it's like we're resuming sync from a previous height.
                    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
                    // We use `prepare_cached` a fair amount: this is an overestimate of the number
                    // of cached prepared statements likely to be used.
                    conn.set_prepared_statement_cache_capacity(32);
                    Ok(())
                });
            Ok(r2d2::Pool::builder()
                // We set max_size=1 to avoid "database is locked" sqlite errors,
                // when accessing across multiple threads.
                .max_size(1)
                .build(manager)?)
        } else {
            let manager = SqliteConnectionManager::memory();
            // Max size needs to be set to 1, otherwise a new in-memory database is created for each
            // connection to the pool, which results in very confusing errors.
            //
            // Lifetimes and timeouts are likewise configured to their maximum values, since
            // the in-memory database will disappear on connection close.
            Ok(r2d2::Pool::builder()
                .max_size(1)
                .min_idle(Some(1))
                .max_lifetime(Some(Duration::MAX))
                .idle_timeout(Some(Duration::MAX))
                .build(manager)?)
        }
    }

    pub async fn load(path: impl AsRef<Utf8Path>) -> anyhow::Result<Self> {
        let storage = Self {
            pool: Self::connect(Some(path))?,
            uncommitted_height: Arc::new(Mutex::new(None)),
            scanned_notes_tx: broadcast::channel(128).0,
            scanned_nullifiers_tx: broadcast::channel(512).0,
        };

        spawn_blocking(move || {
            // Check the version of the software used when first initializing this database.
            // If it doesn't match the current version, we should report the error to the user.
            let actual_schema_hash: String = storage
                .pool
                .get()?
                .query_row("SELECT schema_hash FROM schema_hash", (), |row| {
                    row.get("schema_hash")
                })
                .context("failed to query database schema version: the database was probably created by an old client version, and needs to be reset and resynchronized")?;

            if actual_schema_hash != *SCHEMA_HASH {
                let database_client_version: String = storage
                    .pool
                    .get()?
                    .query_row("SELECT client_version FROM client_version", (), |row| {
                        row.get("client_version")
                    })
                    .context("failed to query client version: the database was probably created by an old client version, and needs to be reset and resynchronized")?;

                anyhow::bail!(
                    "can't load view database created by client version {} using client version {}: they have different schemata, so you need to reset your view database and resynchronize by running pcli view reset",
                    database_client_version,
                    env!("CARGO_PKG_VERSION"),
                );
            }

            Ok(storage)
        })
            .await?
    }

    pub async fn initialize(
        storage_path: Option<impl AsRef<Utf8Path>>,
        fvk: FullViewingKey,
        params: AppParameters,
    ) -> anyhow::Result<Self> {
        tracing::debug!(storage_path = ?storage_path.as_ref().map(AsRef::as_ref), ?fvk, ?params);

        // Connect to the database (or create it)
        let pool = Self::connect(storage_path)?;

        let out = spawn_blocking(move || {
            // In one database transaction, populate everything
            let mut conn = pool.get()?;
            let tx = conn.transaction()?;

            // Create the tables
            tx.execute_batch(include_str!("storage/schema.sql"))?;

            let params_bytes = params.encode_to_vec();
            tx.execute(
                "INSERT INTO kv (k, v) VALUES ('app_params', ?1)",
                [&params_bytes[..]],
            )?;

            let fvk_bytes = fvk.encode_to_vec();
            tx.execute("INSERT INTO kv (k, v) VALUES ('fvk', ?1)", [&fvk_bytes[..]])?;

            // Insert -1 as a signaling value for pre-genesis.
            // We just have to be careful to treat negative values as None
            // in last_sync_height.
            tx.execute("INSERT INTO sync_height (height) VALUES (-1)", ())?;

            // Insert the schema hash into the database
            tx.execute(
                "INSERT INTO schema_hash (schema_hash) VALUES (?1)",
                [&*SCHEMA_HASH],
            )?;

            // Insert the client version into the database
            tx.execute(
                "INSERT INTO client_version (client_version) VALUES (?1)",
                [env!("CARGO_PKG_VERSION")],
            )?;

            tx.commit()?;
            drop(conn);

            anyhow::Ok(Storage {
                pool,
                uncommitted_height: Arc::new(Mutex::new(None)),
                scanned_notes_tx: broadcast::channel(128).0,
                scanned_nullifiers_tx: broadcast::channel(512).0,
            })
        })
        .await??;

        out.update_epoch(0, None, Some(0)).await?;

        Ok(out)
    }

    /// Loads asset metadata from a JSON file and use to update the database.
    pub async fn load_asset_metadata(
        &self,
        registry_path: impl AsRef<Utf8Path>,
    ) -> anyhow::Result<()> {
        tracing::debug!(registry_path = ?registry_path.as_ref(), "loading asset metadata");
        let registry_path = registry_path.as_ref();
        // Parse into a serde_json::Value first so we can get the bits we care about
        let mut registry_json: serde_json::Value = serde_json::from_str(
            std::fs::read_to_string(registry_path)
                .context("failed to read file")?
                .as_str(),
        )
        .context("failed to parse JSON")?;

        let registry: BTreeMap<String, Metadata> = serde_json::value::from_value(
            registry_json
                .get_mut("assetById")
                .ok_or_else(|| anyhow::anyhow!("missing assetById"))?
                .take(),
        )
        .context("could not parse asset registry")?;

        for metadata in registry.into_values() {
            self.record_asset(metadata).await?;
        }

        Ok(())
    }

    /// Query for account balance by address
    pub async fn balances(
        &self,
        address_index: Option<AddressIndex>,
        asset_id: Option<asset::Id>,
    ) -> anyhow::Result<Vec<BalanceEntry>> {
        let pool = self.pool.clone();

        spawn_blocking(move || {
            let query = "SELECT notes.asset_id, notes.amount, spendable_notes.address_index
                FROM    notes
                JOIN    spendable_notes ON notes.note_commitment = spendable_notes.note_commitment
                WHERE   spendable_notes.height_spent IS NULL";

            tracing::debug!(?query);

            // Combine notes of the same asset/address index together
            let mut balances: BTreeMap<AddressIndex, BTreeMap<asset::Id, Amount>> = BTreeMap::new();

            for result in pool.get()?.prepare_cached(query)?.query_map([], |row| {
                let asset_id = row.get::<&str, Vec<u8>>("asset_id")?;
                let amount = row.get::<&str, Vec<u8>>("amount")?;
                let address_index = row.get::<&str, Vec<u8>>("address_index")?;

                Ok((asset_id, amount, address_index))
            })? {
                let (id, amount, index) = result?;

                let id = Id::try_from(id.as_slice())?;

                let amount: Amount = Amount::from_be_bytes(
                    amount
                        .as_slice()
                        .try_into()
                        .expect("amount slice of incorrect length"),
                );

                let index = AddressIndex::try_from(index.as_slice())?;

                // Skip this entry if not captured by address index filter
                if let Some(address_index) = address_index {
                    if address_index != index {
                        continue;
                    }
                }
                if let Some(asset_id) = asset_id {
                    if asset_id != id {
                        continue;
                    }
                }

                balances
                    .entry(index)
                    .or_insert_with(BTreeMap::new)
                    .entry(id)
                    .and_modify(|e| *e += amount)
                    .or_insert(amount);
            }

            let entries = balances
                .into_iter()
                .flat_map(|(index, assets)| {
                    assets.into_iter().map(move |(id, amount)| BalanceEntry {
                        id,
                        amount: amount.into(),
                        address_index: index,
                    })
                })
                .collect::<Vec<_>>();
            Ok(entries)
        })
        .await?
    }

    /// Query for a note by its note commitment, optionally waiting until the note is detected.
    pub async fn note_by_commitment(
        &self,
        note_commitment: tct::StateCommitment,
        await_detection: bool,
    ) -> anyhow::Result<SpendableNoteRecord> {
        // Start subscribing now, before querying for whether we already
        // have the record, so that we can't miss it if we race a write.
        let mut rx = self.scanned_notes_tx.subscribe();

        let pool = self.pool.clone();

        if let Some(record) = spawn_blocking(move || {
            // Check if we already have the record
            pool.get()?
                .prepare(&format!(
                    "SELECT
                        notes.note_commitment,
                        spendable_notes.height_created,
                        notes.address,
                        notes.amount,
                        notes.asset_id,
                        notes.rseed,
                        notes.recovery_commitment,
                        spendable_notes.address_index,
                        spendable_notes.source,
                        spendable_notes.height_spent,
                        spendable_notes.nullifier,
                        spendable_notes.position,
                        tx.return_address
                    FROM notes
                    JOIN spendable_notes ON notes.note_commitment = spendable_notes.note_commitment
                    LEFT JOIN tx ON spendable_notes.tx_hash = tx.tx_hash
                    WHERE notes.note_commitment = x'{}'",
                    hex::encode(note_commitment.0.to_bytes())
                ))?
                .query_and_then((), |record| record.try_into())?
                .next()
                .transpose()
        })
        .await??
        {
            return Ok(record);
        }

        if !await_detection {
            anyhow::bail!("Note commitment {} not found", note_commitment);
        }

        // Otherwise, wait for newly detected notes and check whether they're
        // the requested one.

        loop {
            match rx.recv().await {
                Ok(record) => {
                    if record.note_commitment == note_commitment {
                        return Ok(record);
                    }
                }

                Err(e) => match e {
                    RecvError::Closed => {
                        anyhow::bail!(
                            "Receiver error during note detection: closed (no more active senders)"
                        );
                    }
                    RecvError::Lagged(count) => {
                        anyhow::bail!(
                            "Receiver error during note detection: lagged (by {:?} messages)",
                            count
                        );
                    }
                },
            };
        }
    }

    /// Query for a nullifier's status, optionally waiting until the nullifier is detected.
    pub async fn nullifier_status(
        &self,
        nullifier: Nullifier,
        await_detection: bool,
    ) -> anyhow::Result<bool> {
        // Start subscribing now, before querying for whether we already have the nullifier, so we
        // can't miss it if we race a write.
        let mut rx = self.scanned_nullifiers_tx.subscribe();

        // Clone the pool handle so that the returned future is 'static
        let pool = self.pool.clone();

        let nullifier_bytes = nullifier.0.to_bytes().to_vec();

        // Check if we already have the nullifier in the set of spent notes
        if let Some(height_spent) = spawn_blocking(move || {
            pool.get()?
                .prepare_cached("SELECT height_spent FROM spendable_notes WHERE nullifier = ?1")?
                .query_and_then([nullifier_bytes], |row| {
                    let height_spent: Option<u64> = row.get("height_spent")?;
                    anyhow::Ok(height_spent)
                })?
                .next()
                .transpose()
        })
        .await??
        {
            let spent = height_spent.is_some();

            // If we're awaiting detection and the nullifier isn't yet spent, don't return just yet
            if !await_detection || spent {
                return Ok(spent);
            }
        }

        // After checking the database, if we didn't find it, return `false` unless we are to
        // await detection
        if !await_detection {
            return Ok(false);
        }

        // Otherwise, wait for newly detected nullifiers and check whether they're the requested
        // one.
        loop {
            let new_nullifier = rx.recv().await.context("change subscriber failed")?;

            if new_nullifier == nullifier {
                return Ok(true);
            }
        }
    }

    /// The last block height we've scanned to, if any.
    pub async fn last_sync_height(&self) -> anyhow::Result<Option<u64>> {
        // Check if we have uncommitted blocks beyond the database height.
        if let Some(height) = *self.uncommitted_height.lock() {
            return Ok(Some(height.get()));
        }

        let pool = self.pool.clone();

        spawn_blocking(move || {
            let height: Option<i64> = pool
                .get()?
                .prepare_cached("SELECT height FROM sync_height ORDER BY height DESC LIMIT 1")?
                .query_row([], |row| row.get::<_, Option<i64>>(0))?;

            anyhow::Ok(u64::try_from(height.ok_or_else(|| anyhow!("missing sync height"))?).ok())
        })
        .await?
    }

    /// Persist an issued address before it is returned to a caller.
    pub async fn record_issued_address(&self, issued: IssuedAddress) -> anyhow::Result<Address> {
        let pool = self.pool.clone();
        spawn_blocking(move || {
            let address_index = issued.address_index.to_bytes();
            let address = issued.address.to_vec();
            let birth_height = i64::try_from(issued.birth_height)
                .context("issued-address birth height exceeds SQLite i64")?;
            let retired_height = issued
                .retired_height
                .map(i64::try_from)
                .transpose()
                .context("issued-address retirement height exceeds SQLite i64")?;
            let mut conn = pool.get()?;
            let tx = conn.transaction()?;
            if let AddressPurpose::Regulated { asset_id } = issued.purpose {
                let assigned: Option<Vec<u8>> = tx
                    .query_row(
                        "SELECT address FROM regulated_address_assignments WHERE asset_id = ?1",
                        [asset_id.to_bytes().to_vec()],
                        |row| row.get(0),
                    )
                    .optional()?;
                if let Some(assigned) = assigned {
                    let assigned = Address::try_from(assigned.as_slice())?;
                    tx.commit()?;
                    return Ok(assigned);
                }
            }

            tx.execute(
                "INSERT INTO issued_addresses
                 (address_index, address, birth_height, retired_height)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(address) DO NOTHING",
                rusqlite::params![&address_index[..], &address, birth_height, retired_height,],
            )?;

            let existing: (Vec<u8>, Vec<u8>, Option<i64>) = tx.query_row(
                "SELECT address_index, address, retired_height
                 FROM issued_addresses WHERE address = ?1",
                [&address[..]],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            anyhow::ensure!(
                existing == (address_index.to_vec(), address.clone(), retired_height),
                "address was already issued with different identity or retirement metadata"
            );
            if let AddressPurpose::Regulated { asset_id } = issued.purpose {
                tx.execute(
                    "INSERT INTO regulated_address_assignments (asset_id, address) VALUES (?1, ?2)",
                    (asset_id.to_bytes().to_vec(), &address),
                )?;
            }
            tx.commit()?;
            Ok(issued.address)
        })
        .await?
    }

    /// Load every issued address, including randomized indices needed by backups.
    pub async fn issued_addresses(&self) -> anyhow::Result<Vec<IssuedAddress>> {
        let pool = self.pool.clone();
        spawn_blocking(move || {
            let conn = pool.get()?;
            let mut statement = conn.prepare_cached(
                "SELECT i.address_index, i.address, r.asset_id, i.birth_height, i.retired_height
                 FROM issued_addresses i
                 LEFT JOIN regulated_address_assignments r ON r.address = i.address
                 ORDER BY i.birth_height, i.address_index, r.asset_id",
            )?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Option<Vec<u8>>>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, Option<i64>>(4)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;

            rows.into_iter()
                .map(|(index, address, asset_id, birth, retired)| {
                    let purpose = match asset_id {
                        None => AddressPurpose::General,
                        Some(asset_id) => AddressPurpose::Regulated {
                            asset_id: asset::Id::try_from(asset_id.as_slice())?,
                        },
                    };
                    Ok(IssuedAddress {
                        address_index: AddressIndex::try_from(index.as_slice())?,
                        address: Address::try_from(address.as_slice())?,
                        purpose,
                        birth_height: u64::try_from(birth)
                            .context("negative issued-address birth height")?,
                        retired_height: retired
                            .map(u64::try_from)
                            .transpose()
                            .context("negative issued-address retirement height")?,
                    })
                })
                .collect()
        })
        .await?
    }

    pub async fn retire_issued_address(
        &self,
        address_index: AddressIndex,
        retired_height: u64,
    ) -> anyhow::Result<()> {
        let pool = self.pool.clone();
        spawn_blocking(move || {
            let changed = pool.get()?.execute(
                "UPDATE issued_addresses SET retired_height = ?2
                 WHERE address_index = ?1 AND retired_height IS NULL
                   AND NOT EXISTS (
                     SELECT 1 FROM regulated_address_assignments r
                     WHERE r.address = issued_addresses.address
                   )",
                rusqlite::params![
                    &address_index.to_bytes()[..],
                    i64::try_from(retired_height)
                        .context("retirement height exceeds SQLite i64")?,
                ],
            )?;
            anyhow::ensure!(
                changed == 1,
                "issued address is unknown, already retired, or permanently assigned to a regulated asset"
            );
            Ok(())
        })
        .await?
    }

    pub async fn app_params(&self) -> anyhow::Result<AppParameters> {
        let pool = self.pool.clone();

        spawn_blocking(move || {
            let params_bytes = pool
                .get()?
                .prepare_cached("SELECT v FROM kv WHERE k IS 'app_params' LIMIT 1")?
                .query_row([], |row| row.get::<_, Option<Vec<u8>>>("v"))?
                .ok_or_else(|| anyhow!("missing app_params in kv table"))?;

            AppParameters::decode(params_bytes.as_slice())
        })
        .await?
    }

    pub async fn gas_prices(&self) -> anyhow::Result<GasPrices> {
        let pool = self.pool.clone();

        spawn_blocking(move || {
            let bytes = pool
                .get()?
                .prepare_cached("SELECT v FROM kv WHERE k IS 'gas_prices' LIMIT 1")?
                .query_row([], |row| row.get::<_, Option<Vec<u8>>>("v"))?
                .ok_or_else(|| anyhow!("missing gas_prices in kv table"))?;

            GasPrices::decode(bytes.as_slice())
        })
        .await?
    }

    pub async fn nullifier_window(&self) -> anyhow::Result<NullifierWindow> {
        self.nullifier_window_if_initialized()
            .await?
            .context("missing nullifier_window in kv table")
    }

    pub(crate) async fn nullifier_window_if_initialized(
        &self,
    ) -> anyhow::Result<Option<NullifierWindow>> {
        let pool = self.pool.clone();
        spawn_blocking(move || {
            let bytes = pool
                .get()?
                .prepare_cached("SELECT v FROM kv WHERE k IS 'nullifier_window' LIMIT 1")?
                .query_row([], |row| row.get::<_, Vec<u8>>("v"))
                .optional()?;
            bytes
                .map(|bytes| pb_sct::NullifierWindow::decode(bytes.as_slice())?.try_into())
                .transpose()
        })
        .await?
    }

    pub async fn discovery_parameters(&self) -> anyhow::Result<discovery::Parameters> {
        let pool = self.pool.clone();

        spawn_blocking(move || {
            let bytes = pool
                .get()?
                .prepare_cached("SELECT v FROM kv WHERE k IS 'discovery_params' LIMIT 1")?
                .query_row([], |row| row.get::<_, Option<Vec<u8>>>("v"))?
                .ok_or_else(|| anyhow!("missing discovery_params in kv table"))?;

            discovery::Parameters::decode(bytes.as_slice())
        })
        .await?
    }

    pub async fn full_viewing_key(&self) -> anyhow::Result<FullViewingKey> {
        let pool = self.pool.clone();

        spawn_blocking(move || {
            let bytes = pool
                .get()?
                .prepare_cached("SELECT v FROM kv WHERE k is 'fvk' LIMIT 1")?
                .query_row([], |row| row.get::<_, Option<Vec<u8>>>("v"))?
                .ok_or_else(|| anyhow!("missing fvk in kv table"))?;

            FullViewingKey::decode(bytes.as_slice())
        })
        .await?
    }

    pub async fn state_commitment_tree(&self) -> anyhow::Result<tct::Tree> {
        let pool = self.pool.clone();
        spawn_blocking(move || {
            tct::Tree::from_reader(&mut TreeStore(&mut pool.get()?.transaction()?))
        })
        .await?
    }

    /// Returns a tuple of (block height, transaction hash) for all transactions in a given range of block heights.
    pub async fn transaction_hashes(
        &self,
        start_height: Option<u64>,
        end_height: Option<u64>,
    ) -> anyhow::Result<Vec<(u64, Vec<u8>)>> {
        let starting_block = start_height.unwrap_or(0) as i64;
        let ending_block = end_height.unwrap_or(self.last_sync_height().await?.unwrap_or(0)) as i64;

        let pool = self.pool.clone();

        spawn_blocking(move || {
            pool.get()?
                .prepare_cached(
                    "SELECT block_height, tx_hash
                    FROM tx
                    WHERE block_height BETWEEN ?1 AND ?2",
                )?
                .query_and_then([starting_block, ending_block], |row| {
                    let block_height: u64 = row.get("block_height")?;
                    let tx_hash: Vec<u8> = row.get("tx_hash")?;
                    anyhow::Ok((block_height, tx_hash))
                })?
                .collect()
        })
        .await?
    }

    /// Returns a tuple of (block height, transaction hash, transaction) for all transactions in a given range of block heights.
    pub async fn transactions(
        &self,
        start_height: Option<u64>,
        end_height: Option<u64>,
    ) -> anyhow::Result<Vec<(u64, Vec<u8>, Transaction)>> {
        let starting_block = start_height.unwrap_or(0) as i64;
        let ending_block = end_height.unwrap_or(self.last_sync_height().await?.unwrap_or(0)) as i64;

        let pool = self.pool.clone();

        spawn_blocking(move || {
            pool.get()?
                .prepare_cached(
                    "SELECT block_height, tx_hash, tx_bytes
                    FROM tx
                    WHERE block_height BETWEEN ?1 AND ?2",
                )?
                .query_and_then([starting_block, ending_block], |row| {
                    let block_height: u64 = row.get("block_height")?;
                    let tx_hash: Vec<u8> = row.get("tx_hash")?;
                    let tx_bytes: Vec<u8> = row.get("tx_bytes")?;
                    let tx = Transaction::decode(tx_bytes.as_slice())?;
                    anyhow::Ok((block_height, tx_hash, tx))
                })?
                .collect()
        })
        .await?
    }

    pub async fn transaction_by_hash(
        &self,
        tx_hash: &[u8],
    ) -> anyhow::Result<Option<(u64, Transaction)>> {
        let pool = self.pool.clone();
        let tx_hash = tx_hash.to_vec();

        spawn_blocking(move || {
            if let Some((block_height, tx_bytes)) = pool
                .get()?
                .prepare_cached("SELECT block_height, tx_bytes FROM tx WHERE tx_hash = ?1")?
                .query_row([tx_hash], |row| {
                    let block_height: u64 = row.get("block_height")?;
                    let tx_bytes: Vec<u8> = row.get("tx_bytes")?;
                    Ok((block_height, tx_bytes))
                })
                .optional()?
            {
                let tx = Transaction::decode(tx_bytes.as_slice())?;
                Ok(Some((block_height, tx)))
            } else {
                Ok(None)
            }
        })
        .await?
    }

    // Query for a note by its note commitment, optionally waiting until the note is detected.
    pub async fn note_by_nullifier(
        &self,
        nullifier: Nullifier,
        await_detection: bool,
    ) -> anyhow::Result<SpendableNoteRecord> {
        // Start subscribing now, before querying for whether we already
        // have the record, so that we can't miss it if we race a write.
        let mut rx = self.scanned_notes_tx.subscribe();

        // Clone the pool handle so that the returned future is 'static
        let pool = self.pool.clone();

        let nullifier_bytes = nullifier.to_bytes().to_vec();

        if let Some(record) = spawn_blocking(move || {
            let record = pool
                .get()?
                .prepare(&format!(
                    "SELECT
                        notes.note_commitment,
                        spendable_notes.height_created,
                        notes.address,
                        notes.amount,
                        notes.asset_id,
                        notes.rseed,
                        notes.recovery_commitment,
                        spendable_notes.address_index,
                        spendable_notes.source,
                        spendable_notes.height_spent,
                        spendable_notes.nullifier,
                        spendable_notes.position,
                        tx.return_address
                    FROM notes
                    JOIN spendable_notes ON notes.note_commitment = spendable_notes.note_commitment
                    LEFT JOIN tx ON spendable_notes.tx_hash = tx.tx_hash
                    WHERE hex(spendable_notes.nullifier) = \"{}\"",
                    hex::encode_upper(nullifier_bytes)
                ))?
                .query_and_then((), |row| SpendableNoteRecord::try_from(row))?
                .next()
                .transpose()?;

            anyhow::Ok(record)
        })
        .await??
        {
            return Ok(record);
        }

        if !await_detection {
            anyhow::bail!("Note commitment for nullifier {:?} not found", nullifier);
        }

        // Otherwise, wait for newly detected notes and check whether they're
        // the requested one.

        loop {
            match rx.recv().await {
                Ok(record) => {
                    if record.nullifier == nullifier {
                        return Ok(record);
                    }
                }

                Err(e) => match e {
                    RecvError::Closed => {
                        anyhow::bail!(
                            "Receiver error during note detection: closed (no more active senders)"
                        );
                    }
                    RecvError::Lagged(count) => {
                        anyhow::bail!(
                            "Receiver error during note detection: lagged (by {:?} messages)",
                            count
                        );
                    }
                },
            };
        }
    }

    pub async fn all_assets(&self) -> anyhow::Result<Vec<Metadata>> {
        let pool = self.pool.clone();

        spawn_blocking(move || {
            pool.get()?
                .prepare_cached("SELECT metadata FROM assets")?
                .query_and_then([], |row| {
                    let metadata_json = row.get::<_, String>("metadata")?;
                    let denom_metadata = serde_json::from_str(&metadata_json)?;

                    anyhow::Ok(denom_metadata)
                })?
                .collect()
        })
        .await?
    }

    pub async fn asset_by_id(&self, id: &Id) -> anyhow::Result<Option<Metadata>> {
        let id = id.to_bytes().to_vec();

        let pool = self.pool.clone();

        spawn_blocking(move || {
            pool.get()?
                .prepare_cached("SELECT metadata FROM assets WHERE asset_id = ?1")?
                .query_and_then([id], |row| {
                    let metadata_json = row.get::<_, String>("metadata")?;
                    let denom_metadata = serde_json::from_str(&metadata_json)?;
                    anyhow::Ok(denom_metadata)
                })?
                .next()
                .transpose()
        })
        .await?
    }

    // Get assets whose denoms match the given SQL LIKE pattern, with the `_` and `%` wildcards,
    // where `\` is the escape character.
    pub async fn assets_matching(&self, pattern: String) -> anyhow::Result<Vec<Metadata>> {
        let pattern = pattern.to_owned();

        let pool = self.pool.clone();

        spawn_blocking(move || {
            pool.get()?
                .prepare_cached("SELECT metadata FROM assets WHERE denom LIKE ?1 ESCAPE '\\'")?
                .query_and_then([pattern], |row| {
                    let metadata_json = row.get::<_, String>("metadata")?;
                    let denom_metadata = serde_json::from_str(&metadata_json)?;
                    anyhow::Ok(denom_metadata)
                })?
                .collect()
        })
        .await?
    }

    pub async fn notes(
        &self,
        include_spent: bool,
        asset_id: Option<asset::Id>,
        address_index: Option<shieldd_sdk_keys::keys::AddressIndex>,
        amount_to_spend: Option<Amount>,
    ) -> anyhow::Result<Vec<SpendableNoteRecord>> {
        // If set, return spent notes as well as unspent notes.
        // bool include_spent = 2;
        let spent_clause = match include_spent {
            false => "NULL",
            true => "height_spent",
        };

        // If set, only return notes with the specified asset id.
        // core.crypto.v1.AssetId asset_id = 3;
        let asset_clause = asset_id
            .map(|id| format!("x'{}'", hex::encode(id.to_bytes())))
            .unwrap_or_else(|| "asset_id".to_string());

        // If set, only return notes with the specified address index.
        // crypto.AddressIndex address_index = 4;
        // This isn't what we want any more, we need to be indexing notes
        // by *account*, not just by address index.
        // For now, just do filtering in software.
        /*
        let address_clause = address_index
            .map(|d| format!("x'{}'", hex::encode(d.to_bytes())))
            .unwrap_or_else(|| "address_index".to_string());
         */
        let address_clause = "address_index".to_string();

        // If set, stop returning notes once the total exceeds this amount.
        //
        // Ignored if `asset_id` is unset or if `include_spent` is set.
        // uint64 amount_to_spend = 5;
        //TODO: figure out a clever way to only return notes up to the sum using SQL
        let amount_cutoff = (amount_to_spend.is_some()) && !(include_spent || asset_id.is_none());
        let mut amount_total = Amount::zero();

        let pool = self.pool.clone();

        spawn_blocking(move || {
            let mut output: Vec<SpendableNoteRecord> = Vec::new();

            for result in pool
                .get()?
                .prepare(&format!(
                    "SELECT notes.note_commitment,
                        spendable_notes.height_created,
                        notes.address,
                        notes.amount,
                        notes.asset_id,
                        notes.rseed,
                        notes.recovery_commitment,
                        spendable_notes.address_index,
                        spendable_notes.source,
                        spendable_notes.height_spent,
                        spendable_notes.nullifier,
                        spendable_notes.position,
                        tx.return_address
                FROM notes
                JOIN spendable_notes ON notes.note_commitment = spendable_notes.note_commitment
                LEFT JOIN tx ON spendable_notes.tx_hash = tx.tx_hash
                WHERE spendable_notes.height_spent IS {spent_clause}
                AND notes.asset_id IS {asset_clause}
                AND spendable_notes.address_index IS {address_clause}"
                ))?
                .query_and_then((), |row| SpendableNoteRecord::try_from(row))?
            {
                let record = result?;

                // Skip notes that don't match the account, since we're
                // not doing account filtering in SQL as a temporary hack (see above)
                if let Some(address_index) = address_index {
                    if record.address_index.account != address_index.account {
                        continue;
                    }
                }
                let amount = record.note.amount();

                // Only display notes of value > 0

                if amount.value() > 0 {
                    output.push(record);
                }

                // If we're tracking amounts, accumulate the value of the note
                // and check if we should break out of the loop.
                if amount_cutoff {
                    // We know all the notes are of the same type, so adding raw quantities makes sense.
                    amount_total += amount;
                    if amount_total >= amount_to_spend.unwrap_or_default() {
                        break;
                    }
                }
            }

            if amount_total < amount_to_spend.unwrap_or_default() {
                anyhow::bail!(
                    "requested amount of {} exceeds total of {}",
                    amount_to_spend.unwrap_or_default(),
                    amount_total
                );
            }

            anyhow::Ok(output)
        })
        .await?
    }

    #[tracing::instrument(skip(self))]
    pub async fn record_asset(&self, asset: Metadata) -> anyhow::Result<()> {
        tracing::debug!(?asset);

        let asset_id = asset.id().to_bytes().to_vec();
        let denom = asset.base_denom().denom;
        let metadata_json = serde_json::to_string(&asset)?;

        let pool = self.pool.clone();

        spawn_blocking(move || {
            pool.get()?
                .execute(
                    "INSERT OR REPLACE INTO assets (asset_id, denom, metadata) VALUES (?1, ?2, ?3)",
                    (asset_id, denom, metadata_json),
                )
                .map_err(anyhow::Error::from)
        })
        .await??;

        Ok(())
    }

    pub async fn record_empty_block(&self, height: u64) -> anyhow::Result<()> {
        // Check that the incoming block height follows the latest recorded height
        let last_sync_height = self.last_sync_height().await?.ok_or_else(|| {
            anyhow::anyhow!("invalid: tried to record empty block as genesis block")
        })?;

        if height != last_sync_height + 1 {
            anyhow::bail!(
                "Wrong block height {} for latest sync height {}",
                height,
                last_sync_height
            );
        }

        *self.uncommitted_height.lock() = Some(height.try_into()?);
        Ok(())
    }

    fn record_note_inner(
        dbtx: &r2d2_sqlite::rusqlite::Transaction<'_>,
        note: &Note,
    ) -> anyhow::Result<()> {
        let note_commitment = note.commit().0.to_bytes().to_vec();
        let address = note.address().to_vec();
        let amount = u128::from(note.amount()).to_be_bytes().to_vec();
        let asset_id = note.asset_id().to_bytes().to_vec();
        let rseed = note.rseed().to_bytes().to_vec();
        let recovery_commitment: [u8; 32] = note.recovery_commitment().into();

        dbtx.execute(
            "INSERT INTO notes (note_commitment, address, amount, asset_id, rseed, recovery_commitment)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ON CONFLICT (note_commitment)
                DO UPDATE SET
                address = excluded.address,
                amount = excluded.amount,
                asset_id = excluded.asset_id,
                rseed = excluded.rseed,
                recovery_commitment = excluded.recovery_commitment",
            (
                note_commitment,
                address,
                amount,
                asset_id,
                rseed,
                recovery_commitment,
            ),
        )?;

        Ok(())
    }

    pub async fn give_advice(&self, note: Note) -> anyhow::Result<()> {
        let pool = self.pool.clone();
        let mut lock = pool.get()?;
        let dbtx = lock.transaction()?;

        Storage::record_note_inner(&dbtx, &note)?;

        dbtx.commit()?;

        Ok(())
    }

    /// Return advice about note contents for use in scanning.
    ///
    /// Given a list of note commitments, this method checks whether any of them
    /// correspond to notes that have been recorded in the database but not yet
    /// observed during scanning.
    pub async fn scan_advice(
        &self,
        note_commitments: Vec<note::StateCommitment>,
    ) -> anyhow::Result<BTreeMap<note::StateCommitment, Note>> {
        if note_commitments.is_empty() {
            return Ok(BTreeMap::new());
        }

        let pool = self.pool.clone();

        // This query gives advice about notes which are known but which have not already been recorded as spendable,
        // in part to avoid revealing information about which notes have been spent.

        spawn_blocking(move || {
            pool.get()?
                .prepare(&format!(
                    "SELECT notes.note_commitment,
                        notes.address,
                        notes.amount,
                        notes.asset_id,
                        notes.rseed,
                        notes.recovery_commitment
                    FROM notes
                    LEFT OUTER JOIN spendable_notes ON notes.note_commitment = spendable_notes.note_commitment
                    WHERE (spendable_notes.note_commitment IS NULL) AND (notes.note_commitment IN ({}))",
                    note_commitments
                        .iter()
                        .map(|cm| format!("x'{}'", hex::encode(cm.0.to_bytes())))
                        .collect::<Vec<_>>()
                        .join(", ")
                ))?
                .query_and_then((), |row| {
                    let address = Address::try_from(row.get::<_, Vec<u8>>("address")?)?;
                    let amount = row.get::<_, [u8; 16]>("amount")?;
                    let amount_u128: u128 = u128::from_be_bytes(amount);
                    let asset_id = asset::Id(Fq::from_bytes_checked(&row.get::<_, [u8; 32]>("asset_id")?).expect("asset id malformed"));
                    let rseed = Rseed(row.get::<_, [u8; 32]>("rseed")?);
                    let recovery_commitment = row
                        .get::<_, [u8; 32]>("recovery_commitment")?
                        .try_into()?;
                    let note = Note::from_parts(
                        address,
                        Value {
                            amount: amount_u128.into(),
                            asset_id,
                        },
                        rseed,
                        recovery_commitment,
                    )?;
                    anyhow::Ok((note.commit(), note))
                })?
                .collect::<anyhow::Result<BTreeMap<_, _>>>()
        }).await?
    }

    /// Filters for nullifiers whose notes we control
    pub async fn filter_nullifiers(
        &self,
        nullifiers: Vec<Nullifier>,
    ) -> anyhow::Result<Vec<Nullifier>> {
        if nullifiers.is_empty() {
            return Ok(Vec::new());
        }

        let pool = self.pool.clone();

        spawn_blocking(move || {
            pool.get()?
                .prepare(&format!(
                    "SELECT nullifier FROM (SELECT nullifier FROM spendable_notes UNION SELECT nullifier FROM tx_by_nullifier) WHERE nullifier IN ({})",
                    nullifiers
                        .iter()
                        .map(|x| format!("x'{}'", hex::encode(x.0.to_bytes())))
                        .collect::<Vec<String>>()
                        .join(",")
                ))?
                .query_and_then((), |row| {
                    let nullifier: Vec<u8> = row.get("nullifier")?;
                    nullifier.as_slice().try_into()
                })?
                .collect()
        })
            .await?
    }

    pub(crate) async fn record_block(
        &self,
        filtered_block: FilteredBlock,
        transactions: Vec<Transaction>,
        sct: &mut tct::Tree,
        channel: tonic::transport::Channel,
        compliance_plan: Option<ComplianceBlockPlan>,
    ) -> anyhow::Result<()> {
        //Check that the incoming block height follows the latest recorded height
        let last_sync_height = self.last_sync_height().await?;

        let correct_height = match last_sync_height {
            // Require that the new block follows the last one we scanned.
            Some(cur_height) => filtered_block.height == cur_height + 1,
            // Require that the new block represents the initial chain state.
            None => filtered_block.height == 0,
        };

        if !correct_height {
            anyhow::bail!(
                "Wrong block height {} for latest sync height {:?}",
                filtered_block.height,
                last_sync_height
            );
        }

        let pool = self.pool.clone();
        let uncommitted_height = self.uncommitted_height.clone();
        let scanned_notes_tx = self.scanned_notes_tx.clone();
        let scanned_nullifiers_tx = self.scanned_nullifiers_tx.clone();

        let fvk = self.full_viewing_key().await?;

        // If the app parameters have changed, update them.
        let new_app_parameters: Option<AppParameters> = if filtered_block.app_parameters_updated {
            // Fetch the latest parameters
            let mut client = AppQueryServiceClient::new(channel);
            Some(
                client
                    .app_parameters(tonic::Request::new(AppParametersRequest {}))
                    .await?
                    .into_inner()
                    .try_into()?,
            )
        } else {
            None
        };

        // Cloning the SCT is cheap because it's a copy-on-write structure, so we move an owned copy
        // into the spawned thread. This means that if for any reason the thread panics or throws an
        // error, the changes to the SCT will be discarded, just like any changes to the database,
        // so the two stay transactionally in sync, even in the case of errors. This would not be
        // the case if we `std::mem::take` the SCT and move it into the spawned thread, because then
        // an error would mean the updated version would never be put back, and the outcome would be
        // a cleared SCT but a non-empty database.
        let mut new_sct = sct.clone();

        *sct = spawn_blocking(move || {
            let mut lock = pool.get()?;
            let mut dbtx = lock.transaction()?;

            if let Some(params) = new_app_parameters {
                let params_bytes = params.encode_to_vec();
                // We expect app_params to be present already but may as well use an upsert
                dbtx.execute(
                    "INSERT INTO kv (k, v) VALUES ('app_params', ?1)
                    ON CONFLICT(k) DO UPDATE SET v = excluded.v",
                    [&params_bytes[..]],
                )?;
            }

            // Insert new note records into storage
            for note_record in filtered_block.new_notes.values() {
                let note_commitment = note_record.note_commitment.0.to_bytes().to_vec();
                let height_created = filtered_block.height as i64;
                let address_index = note_record.address_index.to_bytes().to_vec();
                let nullifier = note_record.nullifier.to_bytes().to_vec();
                let position = (u64::from(note_record.position)) as i64;
                let source = note_record.source.encode_to_vec();
                // Check if the note is from a transaction, if so, include the tx hash (id)
                let tx_hash = match note_record.source {
                    CommitmentSource::Transaction { id } => id,
                    _ => None,
                };

                // Record the inner note data in the notes table
                Storage::record_note_inner(&dbtx, &note_record.note)?;

                dbtx.execute(
                    "INSERT INTO spendable_notes
                    (note_commitment, nullifier, position, height_created, address_index, source, height_spent, tx_hash)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7)
                    ON CONFLICT (note_commitment)
                    DO UPDATE SET nullifier = excluded.nullifier,
                    position = excluded.position,
                    height_created = excluded.height_created,
                    address_index = excluded.address_index,
                    source = excluded.source,
                    height_spent = excluded.height_spent,
                    tx_hash = excluded.tx_hash",
                    (
                        &note_commitment,
                        &nullifier,
                        &position,
                        &height_created,
                        &address_index,
                        &source,
                        // height_spent is NULL because the note is newly discovered
                        &tx_hash,
                    ),
                )?;
                Storage::put_historical_proof_cache_inner(
                    &dbtx,
                    &HistoricalProofCache::pending(note_record.nullifier),
                )?;
            }

            // Update any rows of the table with matching nullifiers to have height_spent
            for nullifier in &filtered_block.spent_nullifiers {
                let height_spent = filtered_block.height as i64;
                let nullifier_bytes = nullifier.to_bytes().to_vec();

                dbtx.execute(
                    "DELETE FROM historical_proof_cache WHERE nullifier = ?1",
                    [&nullifier_bytes],
                )?;

                let spent_commitment: Option<StateCommitment> = dbtx.prepare_cached(
                    "UPDATE spendable_notes SET height_spent = ?1 WHERE nullifier = ?2 RETURNING note_commitment"
                )?
                    .query_and_then(
                        (height_spent, &nullifier_bytes),
                        |row| {
                            let bytes: Vec<u8> = row.get("note_commitment")?;
                            StateCommitment::try_from(&bytes[..]).context("invalid commitment bytes")
                        },
                    )?
                    .next()
                    .transpose()?;

                // Mark spent notes as spent
                if let Some(spent_commitment) = spent_commitment {
                    tracing::debug!(?nullifier, ?spent_commitment, "detected spent note commitment");
                    tracing::debug!(?nullifier, ?spent_commitment, "forgetting spent note commitment");
                    new_sct.forget(spent_commitment);
                };
            }

            for recovered in &filtered_block.volume_accumulators {
                let subject = recovered.state.subject.to_bytes().to_vec();
                let day_start = recovered.state.day_start as i64;
                let volume = recovered.state.undisclosed_volume.to_le_bytes().to_vec();
                let blinding = recovered.state.blinding.to_bytes().to_vec();
                let commitment = recovered.payload.commitment.0.to_bytes().to_vec();
                let position = u64::from(recovered.position) as i64;
                let prior: Option<(Vec<u8>, i64, i64)> = dbtx
                    .query_row(
                        "SELECT commitment, position, recovery_status
                         FROM volume_accumulators
                         WHERE subject = ?1 AND day_start = ?2",
                        (&subject, day_start),
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .optional()?;
                let (expected_nullifier, prior_is_complete) = match &prior {
                    Some((commitment, position, recovery_status)) => {
                        let commitment = StateCommitment::try_from(commitment.as_slice())?;
                        let position: u64 = (*position)
                            .try_into()
                            .context("stored volume accumulator position is negative")?;
                        (
                            Nullifier::derive(
                                fvk.nullifier_key(),
                                tct::Position::from(position),
                                &commitment,
                            ),
                            *recovery_status == 0,
                        )
                    }
                    None => (
                        recovered.state.origin_nullifier(fvk.nullifier_key()),
                        true,
                    ),
                };
                let recovery_status = i64::from(
                    !prior_is_complete || expected_nullifier != recovered.payload.nullifier,
                );
                if let Some((prior_commitment, _, _)) = &prior {
                    let prior_commitment = StateCommitment::try_from(prior_commitment.as_slice())?;
                    anyhow::ensure!(
                        new_sct.forget(prior_commitment),
                        "stored volume accumulator commitment is not retained in the wallet SCT"
                    );
                }
                dbtx.execute(
                    "INSERT INTO volume_accumulators
                     (subject, day_start, volume, blinding, commitment, position, recovery_status)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                     ON CONFLICT(subject, day_start) DO UPDATE SET
                       volume = excluded.volume,
                       blinding = excluded.blinding,
                       commitment = excluded.commitment,
                       position = excluded.position,
                       recovery_status = excluded.recovery_status",
                    (
                        &subject,
                        day_start,
                        &volume,
                        &blinding,
                        &commitment,
                        position,
                        recovery_status,
                    ),
                )?;
                dbtx.execute(
                    "DELETE FROM volume_accumulator_reservations
                     WHERE subject = ?1 AND day_start = ?2 AND nullifier = ?3",
                    (&subject, day_start, recovered.payload.nullifier.to_bytes().to_vec()),
                )?;
            }

            // Update SCT table with current SCT state
            new_sct.to_writer(&mut TreeStore(&mut dbtx))?;

            // Record all transactions
            for transaction in transactions {
                let tx_bytes = transaction.encode_to_vec();
                // We have to create an explicit temporary borrow, because the sqlx api is bad (see above)
                let tx_hash_owned = sha2::Sha256::digest(&tx_bytes);
                let tx_hash = tx_hash_owned.as_slice();
                let tx_block_height = filtered_block.height as i64;
                let decrypted_memo = transaction.decrypt_memo(&fvk).ok();
                let memo_text = decrypted_memo.clone().map_or(None,|x| Some(x.text().to_string()));
                let return_address = decrypted_memo.map_or(None, |x| Some(x.return_address().to_vec()));

                tracing::debug!(tx_hash = ?hex::encode(tx_hash), "recording extended transaction");

                dbtx.execute(
                    "INSERT OR IGNORE INTO tx (tx_hash, tx_bytes, block_height, return_address, memo_text) VALUES (?1, ?2, ?3, ?4, ?5)",
                    (&tx_hash, &tx_bytes, tx_block_height, return_address, memo_text),
                )?;

                // Associate all of the spent nullifiers with the transaction by hash.
                for nf in transaction.spent_nullifiers() {
                    let nf_bytes = nf.0.to_bytes().to_vec();
                    dbtx.execute(
                        "INSERT OR IGNORE INTO tx_by_nullifier (nullifier, tx_hash) VALUES (?1, ?2)",
                        (&nf_bytes, &tx_hash),
                    )?;
                }
            }

            // Update discovery parameters if they've changed.
            if filtered_block.discovery_parameters.is_some() {
                let discovery_parameters_bytes =
                    &discovery::Parameters::encode_to_vec(&filtered_block.discovery_parameters.ok_or_else(|| anyhow::anyhow!("missing discovery parameters in filtered block"))?)[..];

                dbtx.execute(
                    "INSERT INTO kv (k, v) VALUES ('discovery_params', ?1)
                    ON CONFLICT(k) DO UPDATE SET v = excluded.v",
                    [&discovery_parameters_bytes],
                )?;
            }

            // Update gas prices if they've changed.
            if filtered_block.gas_prices.is_some() {
                let gas_prices_bytes =
                    &GasPrices::encode_to_vec(&filtered_block.gas_prices.ok_or_else(|| anyhow::anyhow!("missing gas prices in filtered block"))?)[..];

                dbtx.execute(
                    "INSERT INTO kv (k, v) VALUES ('gas_prices', ?1)
                    ON CONFLICT(k) DO UPDATE SET v = excluded.v",
                    [&gas_prices_bytes],
                )?;
            }

            if let Some(window) = filtered_block.nullifier_window {
                let bytes = pb_sct::NullifierWindow::from(window).encode_to_vec();
                dbtx.execute(
                    "INSERT INTO kv (k, v) VALUES ('nullifier_window', ?1)
                    ON CONFLICT(k) DO UPDATE SET v = excluded.v",
                    [&bytes],
                )?;
            }

            if let Some(plan) = compliance_plan {
                anyhow::ensure!(
                    plan.height == filtered_block.height,
                    "compliance plan height does not match wallet block height"
                );
                Storage::record_compliance_plan_inner(&mut dbtx, plan)?;
            }

            // Record block height as latest synced height
            let latest_sync_height = filtered_block.height as i64;
            dbtx.execute("UPDATE sync_height SET height = ?1", [latest_sync_height])?;

            // Commit the changes to the database
            dbtx.commit()?;

            // IMPORTANT: NO PANICS OR ERRORS PAST THIS POINT
            // If there is a panic or error past this point, the database will be left in out of
            // sync with the in-memory copy of the SCT, which means that it will become corrupted as
            // synchronization continues.

            // It's critical to reset the uncommitted height here, since we've just
            // invalidated it by committing.
            uncommitted_height.lock().take();

            // Broadcast all committed note records to channel
            // Done following tx.commit() to avoid notifying of a new SpendableNoteRecord before it is actually committed to the database

            for note_record in filtered_block.new_notes.values() {
                // This will fail to be broadcast if there is no active receiver (such as on initial
                // sync) The error is ignored, as this isn't a problem, because if there is no
                // active receiver there is nothing to do
                let _ = scanned_notes_tx.send(note_record.clone());
            }

            for nullifier in filtered_block.spent_nullifiers.iter() {
                // This will fail to be broadcast if there is no active receiver (such as on initial
                // sync) The error is ignored, as this isn't a problem, because if there is no
                // active receiver there is nothing to do
                let _ = scanned_nullifiers_tx.send(*nullifier);
            }

            anyhow::Ok(new_sct)
        })
            .await??;

        Ok(())
    }

    pub async fn notes_by_sender(
        &self,
        return_address: &Address,
    ) -> anyhow::Result<Vec<SpendableNoteRecord>> {
        let pool = self.pool.clone();

        let query = "SELECT notes.note_commitment,
            spendable_notes.height_created,
            notes.address,
            notes.amount,
            notes.asset_id,
            notes.rseed,
            notes.recovery_commitment,
            spendable_notes.address_index,
            spendable_notes.source,
            spendable_notes.height_spent,
            spendable_notes.nullifier,
            spendable_notes.position
            FROM notes
            JOIN spendable_notes ON notes.note_commitment = spendable_notes.note_commitment
            JOIN tx ON spendable_notes.tx_hash = tx.tx_hash
            WHERE tx.return_address = ?1";

        let return_address = return_address.to_vec();

        let records = spawn_blocking(move || {
            pool.get()?
                .prepare(query)?
                .query_and_then([return_address], |record| record.try_into())?
                .collect::<anyhow::Result<Vec<_>>>()
        })
        .await??;

        Ok(records)
    }

    /// Get all transactions with a matching memo text. The `pattern` argument
    /// should include SQL wildcards, such as `%` and `_`, to match substrings,
    /// e.g. `%foo%`.
    pub async fn transactions_matching_memo(
        &self,
        pattern: String,
    ) -> anyhow::Result<Vec<(u64, Vec<u8>, Transaction, String)>> {
        let pattern = pattern.to_owned();
        tracing::trace!(?pattern, "searching for memos matching");
        let pool = self.pool.clone();

        spawn_blocking(move || {
            pool.get()?
                .prepare_cached("SELECT block_height, tx_hash, tx_bytes, memo_text FROM tx WHERE memo_text LIKE ?1 ESCAPE '\\'")?
                .query_and_then([pattern], |row| {
                    let block_height: u64 = row.get("block_height")?;
                    let tx_hash: Vec<u8> = row.get("tx_hash")?;
                    let tx_bytes: Vec<u8> = row.get("tx_bytes")?;
                    let tx = Transaction::decode(tx_bytes.as_slice())?;
                    let memo_text: String = row.get("memo_text")?;
                    anyhow::Ok((block_height, tx_hash, tx, memo_text))
                })?
                .collect()
        })
        .await?
    }

    /// Update information about an epoch.
    pub async fn update_epoch(
        &self,
        epoch: u64,
        root: Option<Root>,
        start_height: Option<u64>,
    ) -> anyhow::Result<()> {
        let pool = self.pool.clone();

        spawn_blocking(move || {
            pool.get()?
                .execute(
                    r#"
                    INSERT INTO epochs(epoch_index, root, start_height)
                    VALUES (?1, ?2, ?3)
                    ON CONFLICT(epoch_index)
                    DO UPDATE SET
                        root = COALESCE(?2, root),
                        start_height = COALESCE(?3, start_height)
                    "#,
                    (epoch, root.map(|x| x.encode_to_vec()), start_height),
                )
                .map_err(anyhow::Error::from)
        })
        .await??;

        Ok(())
    }

    /// Fetch information about the current epoch.
    ///
    /// This will return the root of the epoch, if present,
    /// and the start height of the epoch, if present.
    pub async fn get_epoch(&self, epoch: u64) -> anyhow::Result<(Option<Root>, Option<u64>)> {
        let pool = self.pool.clone();

        spawn_blocking(move || {
            pool.get()?
                .query_row_and_then(
                    r#"
                    SELECT root, start_height
                    FROM epochs
                    WHERE epoch_index = ?1
                    "#,
                    (epoch,),
                    |row| {
                        let root_raw: Option<Vec<u8>> = row.get("root")?;
                        let start_height: Option<u64> = row.get("start_height")?;
                        let root = root_raw.map(|x| Root::decode(x.as_slice())).transpose()?;
                        anyhow::Ok((root, start_height))
                    },
                )
                .map_err(anyhow::Error::from)
        })
        .await?
    }

    /// Load the compliance user tree from storage.
    pub async fn compliance_user_tree(
        &self,
    ) -> anyhow::Result<crate::compliance_tree::ComplianceUserTree> {
        let pool = self.pool.clone();

        spawn_blocking(move || {
            let mut conn = pool.get()?;
            let mut tx = conn.transaction()?;
            let mut store = compliance::ComplianceTreeStore(&mut tx);
            crate::compliance_tree::ComplianceUserTree::from_store(&mut store)
        })
        .await?
    }

    /// Load the compliance asset tree from storage.
    pub async fn compliance_asset_tree(
        &self,
    ) -> anyhow::Result<crate::compliance_tree::ComplianceAssetTree> {
        let pool = self.pool.clone();

        spawn_blocking(move || {
            let mut conn = pool.get()?;
            let mut tx = conn.transaction()?;
            let mut store = compliance::ComplianceTreeStore(&mut tx);
            crate::compliance_tree::ComplianceAssetTree::from_store(&mut store)
        })
        .await?
    }

    fn record_compliance_plan_inner(
        dbtx: &mut r2d2_sqlite::rusqlite::Transaction<'_>,
        plan: ComplianceBlockPlan,
    ) -> anyhow::Result<()> {
        let mut store = compliance::ComplianceTreeStore(dbtx);
        for write in plan.user_tree.leaves {
            store.add_user_position(write.position, write.commitment)?;
        }
        store.set_user_tree_position(plan.user_tree.next_position)?;
        for write in plan.asset_tree.leaves {
            store.add_asset_leaf(write.position, write.leaf)?;
        }
        store.set_asset_tree_leaf_count(plan.asset_tree.leaf_count)?;
        for update in plan.leaf_updates {
            store.add_leaf_data(
                &update.leaf.address.to_vec(),
                &update.leaf.asset_id.to_bytes(),
                update.position,
                &update.leaf.capk.vartime_compress().0,
                &update.leaf.rnk_dh_pk.vartime_compress().0,
                &update.leaf.rnk_commitment.to_bytes(),
                update.leaf.status,
                update.leaf.freeze_generation,
                update.leaf.frozen_since_height,
                update.commitment,
            )?;
        }
        for update in plan.asset_policy_updates {
            store.add_asset_policy(&update.asset_id.to_bytes(), &update.policy.to_bytes()?)?;
        }
        store.add_anchor(plan.height, plan.user_root, plan.asset_root)
    }

    /// Record a counterparty address for tracking.
    pub async fn record_counterparty(
        &self,
        address: &shieldd_sdk_keys::Address,
        height: u64,
    ) -> anyhow::Result<()> {
        let pool = self.pool.clone();
        let address_bytes = address.to_vec();

        spawn_blocking(move || {
            let mut conn = pool.get()?;
            let mut tx = conn.transaction()?;
            {
                let mut store = compliance::ComplianceTreeStore(&mut tx);
                store.add_counterparty(&address_bytes, height)?;
            }
            tx.commit()?;
            Ok::<(), anyhow::Error>(())
        })
        .await??;

        Ok(())
    }

    /// Check if an address is in the compliance sync scope (own or counterparty).
    pub async fn is_address_in_compliance_scope(
        &self,
        fvk: &FullViewingKey,
        address: &shieldd_sdk_keys::Address,
    ) -> anyhow::Result<bool> {
        // First check if it's one of our own addresses
        if fvk.address_index(address).is_some() {
            return Ok(true);
        }

        // Otherwise check if it's a tracked counterparty
        let pool = self.pool.clone();
        let address_bytes = address.to_vec();

        spawn_blocking(move || {
            let mut conn = pool.get()?;
            let mut tx = conn.transaction()?;
            let result = {
                let mut store = compliance::ComplianceTreeStore(&mut tx);
                store.is_counterparty(&address_bytes)?
            };
            Ok::<bool, anyhow::Error>(result)
        })
        .await?
    }

    /// Get compliance leaf data for an address and asset from local storage.
    ///
    /// Returns full leaf slot data if available, None if not in scope.
    pub async fn get_compliance_leaf_data(
        &self,
        address: &shieldd_sdk_keys::Address,
        asset_id: &asset::Id,
    ) -> anyhow::Result<Option<compliance::UserLeafData>> {
        let pool = self.pool.clone();
        let address_bytes = address.to_vec();
        let asset_bytes = asset_id.to_bytes().to_vec();

        spawn_blocking(move || {
            let mut conn = pool.get()?;
            let mut tx = conn.transaction()?;
            let result = {
                let mut store = compliance::ComplianceTreeStore(&mut tx);
                store.get_leaf_data(&address_bytes, &asset_bytes)?
            };
            Ok::<Option<compliance::UserLeafData>, anyhow::Error>(result)
        })
        .await?
    }

    /// Get an asset policy (daily_volume_limit and DK_pub) if one exists.
    pub async fn get_asset_policy(
        &self,
        asset_id: &asset::Id,
    ) -> anyhow::Result<Option<shieldd_sdk_compliance::structs::AssetPolicy>> {
        let pool = self.pool.clone();
        let asset_bytes = asset_id.to_bytes().to_vec();

        spawn_blocking(move || {
            let conn = pool.get()?;
            let result: Option<Vec<u8>> = conn
                .query_row(
                    "SELECT policy FROM compliance_asset_policies WHERE asset_id = ?1",
                    [asset_bytes.as_slice()],
                    |row| row.get(0),
                )
                .optional()?;

            match result {
                Some(policy_bytes) => Ok(Some(
                    shieldd_sdk_compliance::structs::AssetPolicy::from_bytes(&policy_bytes)?,
                )),
                None => Ok(None),
            }
        })
        .await?
    }
}
