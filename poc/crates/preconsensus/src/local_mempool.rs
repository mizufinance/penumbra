use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{anyhow, Context, Result};
use penumbra_sdk_asset::{asset, BASE_ASSET_ID};
use penumbra_sdk_num::Amount;
use penumbra_sdk_proto::DomainType;
use penumbra_sdk_sct::Nullifier;
use sha2::Digest as _;

use penumbra_sdk_app::stateless_cache::TxArtifact;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FeeSource {
    RealTx,
    SyntheticBenchmark,
}

#[derive(Clone)]
pub struct AdmittedRecord {
    pub tx_hash: [u8; 32],
    pub admission_seq: u64,
    pub tx_bytes: Arc<Vec<u8>>,
    pub tx_len: usize,
    pub artifact: Arc<TxArtifact>,
    pub snapshot_version: u64,
    pub spend_nullifiers: Vec<Nullifier>,
    pub fee_asset_id: asset::Id,
    pub fee_amount: Amount,
    pub fee_source: FeeSource,
}

impl AdmittedRecord {
    pub fn from_tx_bytes(
        admission_seq: u64,
        tx_bytes: Arc<Vec<u8>>,
        artifact: Arc<TxArtifact>,
        snapshot_version: u64,
    ) -> Self {
        let fee = artifact.tx.transaction_parameters().fee;
        Self {
            tx_hash: sha2::Sha256::digest(tx_bytes.as_slice()).into(),
            admission_seq,
            tx_len: tx_bytes.len(),
            tx_bytes,
            snapshot_version,
            spend_nullifiers: artifact.spend_nullifiers.clone(),
            fee_asset_id: fee.asset_id(),
            fee_amount: fee.amount(),
            fee_source: FeeSource::RealTx,
            artifact,
        }
    }

    pub fn from_artifact(
        admission_seq: u64,
        artifact: Arc<TxArtifact>,
        snapshot_version: u64,
    ) -> Self {
        Self::from_tx_bytes(
            admission_seq,
            Arc::new(artifact.tx.encode_to_vec()),
            artifact,
            snapshot_version,
        )
    }

    pub fn with_fee_metadata(
        mut self,
        fee_asset_id: asset::Id,
        fee_amount: Amount,
        fee_source: FeeSource,
    ) -> Self {
        self.fee_asset_id = fee_asset_id;
        self.fee_amount = fee_amount;
        self.fee_source = fee_source;
        self
    }

    pub fn is_staking_fee(&self) -> bool {
        self.fee_asset_id == *BASE_ASSET_ID
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvictionPolicy {
    OldestUnreservedFirst,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FeeEvictionPolicy {
    Disabled,
    LaunchStakingPriority,
}

#[derive(Clone, Debug)]
pub struct MempoolCoreConfig {
    pub max_store_bytes: usize,
    pub max_store_txs: usize,
    pub ingestion_buffer: usize,
    pub command_buffer: usize,
    pub eviction_policy: EvictionPolicy,
    pub fee_eviction_policy: FeeEvictionPolicy,
}

impl Default for MempoolCoreConfig {
    fn default() -> Self {
        Self {
            max_store_bytes: 1 << 30,
            max_store_txs: usize::MAX,
            ingestion_buffer: 1024,
            command_buffer: 128,
            eviction_policy: EvictionPolicy::OldestUnreservedFirst,
            fee_eviction_policy: FeeEvictionPolicy::Disabled,
        }
    }
}

#[derive(Clone, Debug)]
pub enum AdmitOutcome {
    Admitted {
        evicted_records: usize,
        replaced_records: usize,
    },
    RejectedOversize,
    RejectedAtCapacityNoEvictable,
    RejectedAtCapacityLowFee,
}

impl AdmitOutcome {
    pub fn was_admitted(&self) -> bool {
        matches!(self, Self::Admitted { .. })
    }
}

#[derive(Clone, Debug)]
pub struct ReservationLease {
    pub candidate_id: u64,
    pub target_turn: u64,
    pub record_ids: Vec<u64>,
    pub tx_hashes: Vec<[u8; 32]>,
}

#[derive(Clone)]
pub struct FrozenCandidate {
    pub lease: ReservationLease,
    pub target_turn: u64,
    pub records: Vec<Arc<AdmittedRecord>>,
    pub reserved_bytes: usize,
    pub reserved_tx_count: usize,
    pub frozen_at: Instant,
}

#[derive(Clone, Debug, Default)]
pub struct ExpireSummary {
    pub expired_candidate_count: usize,
    pub requeued_record_count: usize,
}

#[derive(Clone, Debug, Default)]
pub struct CommitPruneSummary {
    pub committed_record_count: usize,
    pub invalidated_free_record_count: usize,
    pub invalidated_active_candidate_count: usize,
    pub requeued_record_count: usize,
}

#[derive(Clone, Debug, Default)]
pub struct MempoolSnapshot {
    pub total_record_count: usize,
    pub free_record_count: usize,
    pub reserved_record_count: usize,
    pub frozen_candidate_count: usize,
    pub current_bytes: usize,
    pub peak_txs: usize,
    pub peak_bytes: usize,
    pub peak_nullifier_entries: usize,
    pub peak_reserved_records: usize,
    pub peak_frozen_candidates: usize,
    pub evicted_total: u64,
    pub committed_total: u64,
    pub invalidated_total: u64,
    pub invalidated_active_candidate_total: u64,
    pub replaced_total: u64,
    pub rejected_oversize_total: u64,
    pub rejected_full_no_evictable_total: u64,
    pub rejected_full_low_fee_total: u64,
    pub evicted_nonstaking_total: u64,
    pub evicted_lowest_staking_total: u64,
}

#[derive(Clone)]
pub struct MempoolHandle {
    ingress_tx: tokio::sync::mpsc::Sender<IngressMessage>,
    command_tx: tokio::sync::mpsc::Sender<Command>,
}

impl MempoolHandle {
    pub fn new(config: MempoolCoreConfig) -> Self {
        let (ingress_tx, ingress_rx) = tokio::sync::mpsc::channel(config.ingestion_buffer.max(1));
        let (command_tx, command_rx) = tokio::sync::mpsc::channel(config.command_buffer.max(1));
        tokio::spawn(run_mempool_task(config, ingress_rx, command_rx));
        Self {
            ingress_tx,
            command_tx,
        }
    }

    pub async fn submit_admitted(&self, record: Arc<AdmittedRecord>) -> Result<AdmitOutcome> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.ingress_tx
            .send(IngressMessage::Single { record, reply_tx })
            .await
            .context("sending admitted record to mempool core")?;
        reply_rx
            .await
            .context("waiting for mempool admit outcome reply")?
    }

    pub async fn submit_admitted_batch(
        &self,
        records: Vec<Arc<AdmittedRecord>>,
    ) -> Result<Vec<AdmitOutcome>> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.ingress_tx
            .send(IngressMessage::Batch { records, reply_tx })
            .await
            .context("sending admitted batch to mempool core")?;
        reply_rx
            .await
            .context("waiting for mempool admit batch reply")?
    }

    pub async fn freeze_next_candidate(
        &self,
        target_turn: u64,
        max_block_txs: usize,
        max_proposal_bytes: usize,
    ) -> Result<Option<FrozenCandidate>> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(Command::FreezeNextCandidate {
                target_turn,
                max_block_txs,
                max_proposal_bytes,
                reply_tx,
            })
            .await
            .context("sending freeze command to mempool core")?;
        reply_rx
            .await
            .context("waiting for freeze_next_candidate reply")?
    }

    pub async fn release_reservation(&self, candidate_id: u64) -> Result<()> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(Command::ReleaseReservation {
                candidate_id,
                reply_tx,
            })
            .await
            .context("sending release_reservation command to mempool core")?;
        reply_rx
            .await
            .context("waiting for release_reservation reply")?
    }

    pub async fn expire_target_turn(&self, target_turn: u64) -> Result<ExpireSummary> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(Command::ExpireTargetTurn {
                target_turn,
                reply_tx,
            })
            .await
            .context("sending expire_target_turn command to mempool core")?;
        reply_rx
            .await
            .context("waiting for expire_target_turn reply")?
    }

    pub async fn commit_candidate(&self, candidate_id: u64) -> Result<CommitPruneSummary> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(Command::CommitCandidate {
                candidate_id,
                reply_tx,
            })
            .await
            .context("sending commit_candidate command to mempool core")?;
        reply_rx
            .await
            .context("waiting for commit_candidate reply")?
    }

    pub async fn snapshot(&self) -> Result<MempoolSnapshot> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(Command::Snapshot { reply_tx })
            .await
            .context("sending snapshot command to mempool core")?;
        reply_rx.await.context("waiting for snapshot reply")
    }
}

enum IngressMessage {
    Single {
        record: Arc<AdmittedRecord>,
        reply_tx: tokio::sync::oneshot::Sender<Result<AdmitOutcome>>,
    },
    Batch {
        records: Vec<Arc<AdmittedRecord>>,
        reply_tx: tokio::sync::oneshot::Sender<Result<Vec<AdmitOutcome>>>,
    },
}

enum Command {
    FreezeNextCandidate {
        target_turn: u64,
        max_block_txs: usize,
        max_proposal_bytes: usize,
        reply_tx: tokio::sync::oneshot::Sender<Result<Option<FrozenCandidate>>>,
    },
    ReleaseReservation {
        candidate_id: u64,
        reply_tx: tokio::sync::oneshot::Sender<Result<()>>,
    },
    ExpireTargetTurn {
        target_turn: u64,
        reply_tx: tokio::sync::oneshot::Sender<Result<ExpireSummary>>,
    },
    CommitCandidate {
        candidate_id: u64,
        reply_tx: tokio::sync::oneshot::Sender<Result<CommitPruneSummary>>,
    },
    Snapshot {
        reply_tx: tokio::sync::oneshot::Sender<MempoolSnapshot>,
    },
}

struct ReservationState {
    target_turn: u64,
    record_ids: Vec<u64>,
    reserved_bytes: usize,
    frozen_at: Instant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StakingFeeOrderKey {
    fee_amount: Amount,
    tx_len: usize,
    admission_seq: u64,
    record_id: u64,
}

impl Ord for StakingFeeOrderKey {
    fn cmp(&self, other: &Self) -> Ordering {
        let lhs = self.fee_amount.value() * other.tx_len as u128;
        let rhs = other.fee_amount.value() * self.tx_len as u128;
        match lhs.cmp(&rhs) {
            Ordering::Equal => match other.admission_seq.cmp(&self.admission_seq) {
                Ordering::Equal => self.record_id.cmp(&other.record_id),
                ordering => ordering,
            },
            ordering => ordering,
        }
    }
}

impl PartialOrd for StakingFeeOrderKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct MempoolState {
    config: MempoolCoreConfig,
    free_queue: VecDeque<u64>,
    free_nonstaking_queue: VecDeque<u64>,
    free_staking_index: BTreeSet<StakingFeeOrderKey>,
    record_fee_order: HashMap<u64, StakingFeeOrderKey>,
    free_tombstones: HashSet<u64>,
    records: HashMap<u64, Arc<AdmittedRecord>>,
    nullifier_index: HashMap<Nullifier, HashSet<u64>>,
    record_reservation: HashMap<u64, u64>,
    reservations: HashMap<u64, ReservationState>,
    current_bytes: usize,
    peak_txs: usize,
    peak_bytes: usize,
    peak_nullifier_entries: usize,
    peak_reserved_records: usize,
    peak_frozen_candidates: usize,
    evicted_total: u64,
    committed_total: u64,
    invalidated_total: u64,
    invalidated_active_candidate_total: u64,
    replaced_total: u64,
    rejected_oversize_total: u64,
    rejected_full_no_evictable_total: u64,
    rejected_full_low_fee_total: u64,
    evicted_nonstaking_total: u64,
    evicted_lowest_staking_total: u64,
    next_candidate_id: u64,
}

impl MempoolState {
    fn new(config: MempoolCoreConfig) -> Self {
        Self {
            config,
            free_queue: VecDeque::new(),
            free_nonstaking_queue: VecDeque::new(),
            free_staking_index: BTreeSet::new(),
            record_fee_order: HashMap::new(),
            free_tombstones: HashSet::new(),
            records: HashMap::new(),
            nullifier_index: HashMap::new(),
            record_reservation: HashMap::new(),
            reservations: HashMap::new(),
            current_bytes: 0,
            peak_txs: 0,
            peak_bytes: 0,
            peak_nullifier_entries: 0,
            peak_reserved_records: 0,
            peak_frozen_candidates: 0,
            evicted_total: 0,
            committed_total: 0,
            invalidated_total: 0,
            invalidated_active_candidate_total: 0,
            replaced_total: 0,
            rejected_oversize_total: 0,
            rejected_full_no_evictable_total: 0,
            rejected_full_low_fee_total: 0,
            evicted_nonstaking_total: 0,
            evicted_lowest_staking_total: 0,
            next_candidate_id: 1,
        }
    }

    fn snapshot(&self) -> MempoolSnapshot {
        MempoolSnapshot {
            total_record_count: self.records.len(),
            free_record_count: self.free_queue.len(),
            reserved_record_count: self.record_reservation.len(),
            frozen_candidate_count: self.reservations.len(),
            current_bytes: self.current_bytes,
            peak_txs: self.peak_txs,
            peak_bytes: self.peak_bytes,
            peak_nullifier_entries: self.peak_nullifier_entries,
            peak_reserved_records: self.peak_reserved_records,
            peak_frozen_candidates: self.peak_frozen_candidates,
            evicted_total: self.evicted_total,
            committed_total: self.committed_total,
            invalidated_total: self.invalidated_total,
            invalidated_active_candidate_total: self.invalidated_active_candidate_total,
            replaced_total: self.replaced_total,
            rejected_oversize_total: self.rejected_oversize_total,
            rejected_full_no_evictable_total: self.rejected_full_no_evictable_total,
            rejected_full_low_fee_total: self.rejected_full_low_fee_total,
            evicted_nonstaking_total: self.evicted_nonstaking_total,
            evicted_lowest_staking_total: self.evicted_lowest_staking_total,
        }
    }

    fn submit_admitted(&mut self, record: Arc<AdmittedRecord>) -> AdmitOutcome {
        if record.tx_len > self.config.max_store_bytes {
            self.rejected_oversize_total += 1;
            return AdmitOutcome::RejectedOversize;
        }

        let mut evicted_records = 0usize;
        let mut replaced_records = 0usize;

        let t0 = std::time::Instant::now();
        while self.would_exceed_capacity(record.tx_len, 1) {
            match self.config.fee_eviction_policy {
                FeeEvictionPolicy::Disabled => {
                    if self.free_queue.is_empty() {
                        self.rejected_full_no_evictable_total += 1;
                        return AdmitOutcome::RejectedAtCapacityNoEvictable;
                    }
                    if self.evict_oldest_free().is_some() {
                        evicted_records += 1;
                    } else {
                        self.rejected_full_no_evictable_total += 1;
                        return AdmitOutcome::RejectedAtCapacityNoEvictable;
                    }
                }
                FeeEvictionPolicy::LaunchStakingPriority => {
                    match self.make_room_with_fee_policy(record.as_ref()) {
                        CapacityResolution::Evicted => {
                            evicted_records += 1;
                        }
                        CapacityResolution::Replaced => {
                            evicted_records += 1;
                            replaced_records += 1;
                        }
                        CapacityResolution::RejectNoEvictable => {
                            self.rejected_full_no_evictable_total += 1;
                            return AdmitOutcome::RejectedAtCapacityNoEvictable;
                        }
                        CapacityResolution::RejectLowFee => {
                            self.rejected_full_low_fee_total += 1;
                            return AdmitOutcome::RejectedAtCapacityLowFee;
                        }
                    }
                }
            }
        }
        let eviction_ns = t0.elapsed().as_nanos();

        let t1 = std::time::Instant::now();
        let record_id = record.admission_seq;
        for nullifier in &record.spend_nullifiers {
            self.nullifier_index
                .entry(*nullifier)
                .or_default()
                .insert(record_id);
        }
        let nullifier_ns = t1.elapsed().as_nanos();

        let t2 = std::time::Instant::now();
        self.current_bytes += record.tx_len;
        self.records.insert(record_id, record);
        let records_ns = t2.elapsed().as_nanos();

        let t3 = std::time::Instant::now();
        self.insert_free_record(record_id);
        let free_queue_ns = t3.elapsed().as_nanos();

        self.peak_txs = self.peak_txs.max(self.records.len());
        self.peak_bytes = self.peak_bytes.max(self.current_bytes);
        self.peak_nullifier_entries = self.peak_nullifier_entries.max(self.nullifier_index.len());

        if self.records.len() % 1000 == 0 {
            eprintln!(
                "admit_profile records={} eviction_ns={} nullifier_ns={} records_insert_ns={} free_queue_ns={}",
                self.records.len(), eviction_ns, nullifier_ns, records_ns, free_queue_ns
            );
        }

        AdmitOutcome::Admitted {
            evicted_records,
            replaced_records,
        }
    }

    fn freeze_next_candidate(
        &mut self,
        target_turn: u64,
        max_block_txs: usize,
        max_proposal_bytes: usize,
    ) -> Option<FrozenCandidate> {
        const MAX_LOOKAHEAD: usize = 64;
        let mut selected_ids = Vec::new();
        let mut selected_records = Vec::new();
        let mut selected_bytes = 0usize;
        let mut lookahead_remaining = MAX_LOOKAHEAD;

        for record_id in &self.free_queue {
            if self.free_tombstones.contains(record_id) {
                continue;
            }
            let Some(record) = self.records.get(record_id) else {
                continue;
            };
            if selected_ids.len() >= max_block_txs {
                break;
            }
            if !selected_ids.is_empty() && selected_bytes + record.tx_len > max_proposal_bytes {
                lookahead_remaining -= 1;
                if lookahead_remaining == 0 {
                    break;
                }
                continue;
            }
            if selected_ids.is_empty() && record.tx_len > max_proposal_bytes {
                continue;
            }
            selected_ids.push(*record_id);
            selected_records.push(record.clone());
            selected_bytes += record.tx_len;
        }

        if selected_ids.is_empty() {
            return None;
        }

        for record_id in &selected_ids {
            self.tombstone_free_record(*record_id);
        }
        self.gc_free_tombstones();

        let candidate_id = self.next_candidate_id;
        self.next_candidate_id += 1;
        for record_id in &selected_ids {
            self.record_reservation.insert(*record_id, candidate_id);
        }
        self.update_peak_reservation_state();

        let tx_hashes = selected_records
            .iter()
            .map(|record| record.tx_hash)
            .collect();
        let frozen_at = Instant::now();
        self.reservations.insert(
            candidate_id,
            ReservationState {
                target_turn,
                record_ids: selected_ids.clone(),
                reserved_bytes: selected_bytes,
                frozen_at,
            },
        );
        self.update_peak_reservation_state();

        Some(FrozenCandidate {
            lease: ReservationLease {
                candidate_id,
                target_turn,
                record_ids: selected_ids,
                tx_hashes,
            },
            target_turn,
            records: selected_records,
            reserved_bytes: selected_bytes,
            reserved_tx_count: self
                .reservations
                .get(&candidate_id)
                .map(|reservation| reservation.record_ids.len())
                .unwrap_or_default(),
            frozen_at,
        })
    }

    fn release_reservation(&mut self, candidate_id: u64) {
        let Some(reservation) = self.reservations.remove(&candidate_id) else {
            return;
        };
        self.requeue_reservation_records(&reservation.record_ids);
    }

    fn expire_target_turn(&mut self, target_turn: u64) -> ExpireSummary {
        let expired = self
            .reservations
            .iter()
            .filter_map(|(candidate_id, reservation)| {
                (reservation.target_turn == target_turn).then_some(*candidate_id)
            })
            .collect::<Vec<_>>();

        let mut summary = ExpireSummary::default();
        for candidate_id in expired {
            if let Some(reservation) = self.reservations.remove(&candidate_id) {
                summary.expired_candidate_count += 1;
                summary.requeued_record_count +=
                    self.requeue_reservation_records(&reservation.record_ids);
            }
        }
        summary
    }

    fn commit_candidate(&mut self, candidate_id: u64) -> Result<CommitPruneSummary> {
        let reservation = self
            .reservations
            .remove(&candidate_id)
            .ok_or_else(|| anyhow!("unknown candidate_id {}", candidate_id))?;

        let mut summary = CommitPruneSummary::default();
        let committed_ids = reservation
            .record_ids
            .iter()
            .copied()
            .collect::<HashSet<_>>();
        let mut committed_nullifiers = Vec::new();

        for record_id in &reservation.record_ids {
            if let Some(record) = self.records.get(record_id) {
                committed_nullifiers.extend(record.spend_nullifiers.iter().copied());
            }
        }
        for record_id in &reservation.record_ids {
            self.record_reservation.remove(record_id);
            if self.remove_record(*record_id, RemovalReason::Committed) {
                summary.committed_record_count += 1;
            }
        }

        let mut conflicting_ids = HashSet::new();
        for nullifier in committed_nullifiers {
            if let Some(ids) = self.nullifier_index.get(&nullifier) {
                conflicting_ids.extend(ids.iter().copied());
            }
        }
        conflicting_ids.retain(|record_id| !committed_ids.contains(record_id));

        let mut invalidated_candidate_ids = conflicting_ids
            .iter()
            .filter_map(|record_id| self.record_reservation.get(record_id).copied())
            .collect::<HashSet<_>>();
        invalidated_candidate_ids.remove(&candidate_id);

        for invalidated_candidate_id in invalidated_candidate_ids {
            if let Some(invalidated) = self.reservations.remove(&invalidated_candidate_id) {
                summary.invalidated_active_candidate_count += 1;
                self.invalidated_active_candidate_total += 1;
                for record_id in invalidated.record_ids {
                    self.record_reservation.remove(&record_id);
                    if conflicting_ids.contains(&record_id) {
                        if self.remove_record(record_id, RemovalReason::Invalidated) {
                            summary.invalidated_free_record_count += 1;
                        }
                    } else if self.records.contains_key(&record_id) {
                        self.insert_free_record(record_id);
                        summary.requeued_record_count += 1;
                    }
                }
            }
        }

        let reserved_conflicts = self
            .record_reservation
            .keys()
            .copied()
            .collect::<HashSet<_>>();
        let free_conflicts = conflicting_ids
            .into_iter()
            .filter(|record_id| !reserved_conflicts.contains(record_id))
            .collect::<Vec<_>>();
        for record_id in free_conflicts {
            if self.remove_record(record_id, RemovalReason::Invalidated) {
                summary.invalidated_free_record_count += 1;
            }
        }

        Ok(summary)
    }

    fn would_exceed_capacity(&self, added_bytes: usize, added_records: usize) -> bool {
        self.current_bytes.saturating_add(added_bytes) > self.config.max_store_bytes
            || self.records.len().saturating_add(added_records) > self.config.max_store_txs
    }

    fn make_room_with_fee_policy(&mut self, incoming: &AdmittedRecord) -> CapacityResolution {
        if incoming.is_staking_fee() {
            if self.evict_oldest_free_nonstaking().is_some() {
                return CapacityResolution::Replaced;
            }
            let Some(key) = self.free_staking_index.iter().next().copied() else {
                return CapacityResolution::RejectNoEvictable;
            };
            let incoming_key = StakingFeeOrderKey {
                fee_amount: incoming.fee_amount,
                tx_len: incoming.tx_len,
                admission_seq: incoming.admission_seq,
                record_id: incoming.admission_seq,
            };
            if incoming_key <= key {
                return CapacityResolution::RejectLowFee;
            }
            let _ = self.evict_lowest_staking(key.record_id);
            CapacityResolution::Replaced
        } else {
            if self.evict_oldest_free_nonstaking().is_some() {
                CapacityResolution::Replaced
            } else {
                CapacityResolution::RejectNoEvictable
            }
        }
    }

    fn evict_oldest_free(&mut self) -> Option<u64> {
        match self.config.eviction_policy {
            EvictionPolicy::OldestUnreservedFirst => loop {
                let record_id = self.free_queue.pop_front()?;
                if self.free_tombstones.remove(&record_id) {
                    continue;
                }
                if self.record_reservation.contains_key(&record_id)
                    || !self.records.contains_key(&record_id)
                {
                    continue;
                }
                self.remove_record(record_id, RemovalReason::EvictedOldest);
                return Some(record_id);
            },
        }
    }

    fn evict_oldest_free_nonstaking(&mut self) -> Option<u64> {
        loop {
            let record_id = self.free_nonstaking_queue.pop_front()?;
            if self.free_tombstones.remove(&record_id) {
                continue;
            }
            let Some(record) = self.records.get(&record_id) else {
                continue;
            };
            if self.record_reservation.contains_key(&record_id) || record.is_staking_fee() {
                continue;
            }
            self.remove_record(record_id, RemovalReason::EvictedNonStakingReplacement);
            return Some(record_id);
        }
    }

    fn evict_lowest_staking(&mut self, record_id: u64) -> Option<u64> {
        if self.record_reservation.contains_key(&record_id) {
            return None;
        }
        self.remove_record(record_id, RemovalReason::EvictedLowestStakingReplacement)
            .then_some(record_id)
    }

    fn insert_free_record(&mut self, record_id: u64) {
        let Some(record) = self.records.get(&record_id) else {
            return;
        };
        self.free_queue.push_back(record_id);
        if record.is_staking_fee() {
            let key = StakingFeeOrderKey {
                fee_amount: record.fee_amount,
                tx_len: record.tx_len,
                admission_seq: record.admission_seq,
                record_id,
            };
            self.record_fee_order.insert(record_id, key);
            self.free_staking_index.insert(key);
        } else {
            self.free_nonstaking_queue.push_back(record_id);
        }
    }

    fn remove_from_free_indexes(&mut self, record_id: u64) {
        if let Some(key) = self.record_fee_order.remove(&record_id) {
            self.free_staking_index.remove(&key);
        }
    }

    fn tombstone_free_record(&mut self, record_id: u64) {
        self.free_tombstones.insert(record_id);
        self.remove_from_free_indexes(record_id);
    }

    fn gc_free_tombstones(&mut self) {
        const GC_THRESHOLD: usize = 10_000;
        if self.free_tombstones.len() > GC_THRESHOLD {
            self.free_queue
                .retain(|id| !self.free_tombstones.contains(id));
            self.free_nonstaking_queue
                .retain(|id| !self.free_tombstones.contains(id));
            self.free_tombstones.clear();
        }
    }

    fn requeue_reservation_records(&mut self, record_ids: &[u64]) -> usize {
        let mut requeued = 0usize;
        for record_id in record_ids {
            self.record_reservation.remove(record_id);
            self.free_tombstones.remove(record_id);
            if self.records.contains_key(record_id) {
                self.insert_free_record(*record_id);
                requeued += 1;
            }
        }
        requeued
    }

    fn remove_record(&mut self, record_id: u64, reason: RemovalReason) -> bool {
        let Some(record) = self.records.remove(&record_id) else {
            return false;
        };

        self.current_bytes = self.current_bytes.saturating_sub(record.tx_len);
        self.free_tombstones.remove(&record_id);
        self.remove_from_free_indexes(record_id);
        self.record_reservation.remove(&record_id);

        for nullifier in &record.spend_nullifiers {
            let mut remove_nullifier = false;
            if let Some(ids) = self.nullifier_index.get_mut(nullifier) {
                ids.remove(&record_id);
                remove_nullifier = ids.is_empty();
            }
            if remove_nullifier {
                self.nullifier_index.remove(nullifier);
            }
        }

        match reason {
            RemovalReason::EvictedOldest => self.evicted_total += 1,
            RemovalReason::Committed => self.committed_total += 1,
            RemovalReason::Invalidated => self.invalidated_total += 1,
            RemovalReason::EvictedNonStakingReplacement => {
                self.evicted_total += 1;
                self.replaced_total += 1;
                self.evicted_nonstaking_total += 1;
            }
            RemovalReason::EvictedLowestStakingReplacement => {
                self.evicted_total += 1;
                self.replaced_total += 1;
                self.evicted_lowest_staking_total += 1;
            }
        }
        true
    }

    fn update_peak_reservation_state(&mut self) {
        self.peak_reserved_records = self
            .peak_reserved_records
            .max(self.record_reservation.len());
        self.peak_frozen_candidates = self.peak_frozen_candidates.max(self.reservations.len());
    }
}

enum RemovalReason {
    EvictedOldest,
    Committed,
    Invalidated,
    EvictedNonStakingReplacement,
    EvictedLowestStakingReplacement,
}

enum CapacityResolution {
    Evicted,
    Replaced,
    RejectNoEvictable,
    RejectLowFee,
}

const INGRESS_BATCH_CAP: usize = 32;

async fn run_mempool_task(
    config: MempoolCoreConfig,
    mut ingress_rx: tokio::sync::mpsc::Receiver<IngressMessage>,
    mut command_rx: tokio::sync::mpsc::Receiver<Command>,
) {
    let mut state = MempoolState::new(config);
    let mut ingress_batch: Vec<IngressMessage> = Vec::with_capacity(INGRESS_BATCH_CAP);

    loop {
        tokio::select! {
            biased;
            Some(command) = command_rx.recv() => {
                match command {
                    Command::FreezeNextCandidate { target_turn, max_block_txs, max_proposal_bytes, reply_tx } => {
                        let _ = reply_tx.send(Ok(state.freeze_next_candidate(target_turn, max_block_txs, max_proposal_bytes)));
                    }
                    Command::ReleaseReservation { candidate_id, reply_tx } => {
                        state.release_reservation(candidate_id);
                        let _ = reply_tx.send(Ok(()));
                    }
                    Command::ExpireTargetTurn { target_turn, reply_tx } => {
                        let _ = reply_tx.send(Ok(state.expire_target_turn(target_turn)));
                    }
                    Command::CommitCandidate { candidate_id, reply_tx } => {
                        let _ = reply_tx.send(state.commit_candidate(candidate_id));
                    }
                    Command::Snapshot { reply_tx } => {
                        let _ = reply_tx.send(state.snapshot());
                    }
                }
            }
            count = ingress_rx.recv_many(&mut ingress_batch, INGRESS_BATCH_CAP),
                if ingress_batch.is_empty() => {
                if count == 0 {
                    break;
                }
                for msg in ingress_batch.drain(..) {
                    match msg {
                        IngressMessage::Single { record, reply_tx } => {
                            let _ = reply_tx.send(Ok(state.submit_admitted(record)));
                        }
                        IngressMessage::Batch { records, reply_tx } => {
                            let outcomes = records
                                .into_iter()
                                .map(|r| state.submit_admitted(r))
                                .collect();
                            let _ = reply_tx.send(Ok(outcomes));
                        }
                    }
                }
            }
            else => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use decaf377::Fq;
    use penumbra_sdk_fee::Fee;

    fn dummy_nonstaking_asset(seed: u64) -> asset::Id {
        asset::Id(Fq::from(seed))
    }

    fn dummy_record_with_fee(
        admission_seq: u64,
        tx_len: usize,
        spend_nullifiers: Vec<Nullifier>,
        fee_asset_id: asset::Id,
        fee_amount: Amount,
    ) -> Arc<AdmittedRecord> {
        Arc::new(AdmittedRecord {
            tx_hash: [admission_seq as u8; 32],
            admission_seq,
            tx_bytes: Arc::new(vec![0u8; tx_len]),
            tx_len,
            artifact: Arc::new(TxArtifact {
                tx: Arc::new(penumbra_sdk_transaction::Transaction {
                    transaction_body: penumbra_sdk_transaction::TransactionBody {
                        transaction_parameters: penumbra_sdk_transaction::TransactionParameters {
                            fee: Fee(penumbra_sdk_asset::Value {
                                amount: fee_amount,
                                asset_id: fee_asset_id,
                            }),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    ..Default::default()
                }),
                proof_items: Default::default(),
                spend_nullifiers: spend_nullifiers.clone(),
                anchor_pairs: Default::default(),
                total_proof_count: 0,
                historical_validation: None,
            }),
            snapshot_version: 0,
            spend_nullifiers,
            fee_asset_id,
            fee_amount,
            fee_source: FeeSource::RealTx,
        })
    }

    fn dummy_record(
        admission_seq: u64,
        tx_len: usize,
        spend_nullifiers: Vec<Nullifier>,
    ) -> Arc<AdmittedRecord> {
        dummy_record_with_fee(
            admission_seq,
            tx_len,
            spend_nullifiers,
            *BASE_ASSET_ID,
            Amount::from(1u64),
        )
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn tail_requeue_on_expiry() -> Result<()> {
        let handle = MempoolHandle::new(MempoolCoreConfig {
            max_store_bytes: 1024,
            max_store_txs: 16,
            ingestion_buffer: 16,
            command_buffer: 16,
            eviction_policy: EvictionPolicy::OldestUnreservedFirst,
            fee_eviction_policy: FeeEvictionPolicy::Disabled,
        });
        for admission_seq in 0..4 {
            handle
                .submit_admitted(dummy_record(admission_seq, 16, Vec::new()))
                .await?;
        }
        let frozen = handle
            .freeze_next_candidate(1, 2, 128)
            .await?
            .expect("frozen");
        assert_eq!(frozen.lease.record_ids, vec![0, 1]);

        let expired = handle.expire_target_turn(1).await?;
        assert_eq!(expired.expired_candidate_count, 1);

        let next = handle
            .freeze_next_candidate(2, 4, 512)
            .await?
            .expect("next frozen");
        assert_eq!(next.lease.record_ids, vec![2, 3, 0, 1]);
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn commit_cascades_into_active_candidates() -> Result<()> {
        let shared = Nullifier::try_from([9u8; 32].as_slice()).expect("nullifier");
        let other = Nullifier::try_from([7u8; 32].as_slice()).expect("nullifier");
        let handle = MempoolHandle::new(MempoolCoreConfig {
            max_store_bytes: 1024,
            max_store_txs: 16,
            ingestion_buffer: 16,
            command_buffer: 16,
            eviction_policy: EvictionPolicy::OldestUnreservedFirst,
            fee_eviction_policy: FeeEvictionPolicy::Disabled,
        });
        handle
            .submit_admitted(dummy_record(0, 16, vec![shared]))
            .await?;
        handle
            .submit_admitted(dummy_record(1, 16, vec![other]))
            .await?;
        handle
            .submit_admitted(dummy_record(2, 16, vec![shared]))
            .await?;
        handle
            .submit_admitted(dummy_record(3, 16, vec![other]))
            .await?;

        let first = handle
            .freeze_next_candidate(1, 2, 128)
            .await?
            .expect("first");
        let second = handle
            .freeze_next_candidate(2, 2, 128)
            .await?
            .expect("second");
        assert_eq!(first.lease.record_ids, vec![0, 1]);
        assert_eq!(second.lease.record_ids, vec![2, 3]);

        let summary = handle.commit_candidate(first.lease.candidate_id).await?;
        assert_eq!(summary.committed_record_count, 2);
        assert_eq!(summary.invalidated_active_candidate_count, 1);
        assert_eq!(summary.invalidated_free_record_count, 2);
        assert_eq!(summary.requeued_record_count, 0);

        let next = handle.freeze_next_candidate(3, 4, 512).await?;
        assert!(next.is_none());
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn eviction_skips_reserved_records() -> Result<()> {
        let handle = MempoolHandle::new(MempoolCoreConfig {
            max_store_bytes: 64,
            max_store_txs: 16,
            ingestion_buffer: 16,
            command_buffer: 16,
            eviction_policy: EvictionPolicy::OldestUnreservedFirst,
            fee_eviction_policy: FeeEvictionPolicy::Disabled,
        });
        handle
            .submit_admitted(dummy_record(0, 16, Vec::new()))
            .await?;
        handle
            .submit_admitted(dummy_record(1, 16, Vec::new()))
            .await?;
        let frozen = handle
            .freeze_next_candidate(1, 1, 32)
            .await?
            .expect("frozen");
        assert_eq!(frozen.lease.record_ids, vec![0]);

        handle
            .submit_admitted(dummy_record(2, 16, Vec::new()))
            .await?;
        handle
            .submit_admitted(dummy_record(3, 16, Vec::new()))
            .await?;
        handle
            .submit_admitted(dummy_record(4, 16, Vec::new()))
            .await?;

        let snapshot = handle.snapshot().await?;
        assert_eq!(snapshot.evicted_total, 1);
        assert_eq!(snapshot.reserved_record_count, 1);
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn higher_fee_staking_replaces_lower_fee_staking_when_full() -> Result<()> {
        let handle = MempoolHandle::new(MempoolCoreConfig {
            max_store_bytes: 128,
            max_store_txs: 2,
            ingestion_buffer: 16,
            command_buffer: 16,
            eviction_policy: EvictionPolicy::OldestUnreservedFirst,
            fee_eviction_policy: FeeEvictionPolicy::LaunchStakingPriority,
        });
        handle
            .submit_admitted(dummy_record_with_fee(
                0,
                16,
                Vec::new(),
                *BASE_ASSET_ID,
                Amount::from(1u64),
            ))
            .await?;
        handle
            .submit_admitted(dummy_record_with_fee(
                1,
                16,
                Vec::new(),
                *BASE_ASSET_ID,
                Amount::from(2u64),
            ))
            .await?;

        let outcome = handle
            .submit_admitted(dummy_record_with_fee(
                2,
                16,
                Vec::new(),
                *BASE_ASSET_ID,
                Amount::from(10u64),
            ))
            .await?;
        assert!(matches!(
            outcome,
            AdmitOutcome::Admitted {
                replaced_records: 1,
                ..
            }
        ));
        let snapshot = handle.snapshot().await?;
        assert_eq!(snapshot.evicted_lowest_staking_total, 1);
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn equal_fee_staking_does_not_replace_existing() -> Result<()> {
        let handle = MempoolHandle::new(MempoolCoreConfig {
            max_store_bytes: 64,
            max_store_txs: 1,
            ingestion_buffer: 16,
            command_buffer: 16,
            eviction_policy: EvictionPolicy::OldestUnreservedFirst,
            fee_eviction_policy: FeeEvictionPolicy::LaunchStakingPriority,
        });
        handle
            .submit_admitted(dummy_record_with_fee(
                0,
                16,
                Vec::new(),
                *BASE_ASSET_ID,
                Amount::from(5u64),
            ))
            .await?;
        let outcome = handle
            .submit_admitted(dummy_record_with_fee(
                1,
                16,
                Vec::new(),
                *BASE_ASSET_ID,
                Amount::from(5u64),
            ))
            .await?;
        assert!(matches!(outcome, AdmitOutcome::RejectedAtCapacityLowFee));
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn equal_fee_evicts_newest_first() -> Result<()> {
        let handle = MempoolHandle::new(MempoolCoreConfig {
            max_store_bytes: 128,
            max_store_txs: 2,
            ingestion_buffer: 16,
            command_buffer: 16,
            eviction_policy: EvictionPolicy::OldestUnreservedFirst,
            fee_eviction_policy: FeeEvictionPolicy::LaunchStakingPriority,
        });
        handle
            .submit_admitted(dummy_record_with_fee(
                0,
                16,
                Vec::new(),
                *BASE_ASSET_ID,
                Amount::from(1u64),
            ))
            .await?;
        handle
            .submit_admitted(dummy_record_with_fee(
                1,
                16,
                Vec::new(),
                *BASE_ASSET_ID,
                Amount::from(1u64),
            ))
            .await?;
        handle
            .submit_admitted(dummy_record_with_fee(
                2,
                16,
                Vec::new(),
                *BASE_ASSET_ID,
                Amount::from(2u64),
            ))
            .await?;

        let next = handle
            .freeze_next_candidate(1, 4, 512)
            .await?
            .expect("freeze");
        assert_eq!(next.lease.record_ids, vec![0, 2]);
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn staking_evicts_oldest_free_nonstaking_first() -> Result<()> {
        let nonstaking = dummy_nonstaking_asset(7);
        let handle = MempoolHandle::new(MempoolCoreConfig {
            max_store_bytes: 128,
            max_store_txs: 2,
            ingestion_buffer: 16,
            command_buffer: 16,
            eviction_policy: EvictionPolicy::OldestUnreservedFirst,
            fee_eviction_policy: FeeEvictionPolicy::LaunchStakingPriority,
        });
        handle
            .submit_admitted(dummy_record_with_fee(
                0,
                16,
                Vec::new(),
                nonstaking,
                Amount::from(50u64),
            ))
            .await?;
        handle
            .submit_admitted(dummy_record_with_fee(
                1,
                16,
                Vec::new(),
                *BASE_ASSET_ID,
                Amount::from(1u64),
            ))
            .await?;

        handle
            .submit_admitted(dummy_record_with_fee(
                2,
                16,
                Vec::new(),
                *BASE_ASSET_ID,
                Amount::from(2u64),
            ))
            .await?;

        let next = handle
            .freeze_next_candidate(1, 4, 512)
            .await?
            .expect("freeze");
        assert_eq!(next.lease.record_ids, vec![1, 2]);
        let snapshot = handle.snapshot().await?;
        assert_eq!(snapshot.evicted_nonstaking_total, 1);
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn nonstaking_incoming_evicts_oldest_nonstaking_only() -> Result<()> {
        let nonstaking = dummy_nonstaking_asset(8);
        let handle = MempoolHandle::new(MempoolCoreConfig {
            max_store_bytes: 128,
            max_store_txs: 2,
            ingestion_buffer: 16,
            command_buffer: 16,
            eviction_policy: EvictionPolicy::OldestUnreservedFirst,
            fee_eviction_policy: FeeEvictionPolicy::LaunchStakingPriority,
        });
        handle
            .submit_admitted(dummy_record_with_fee(
                0,
                16,
                Vec::new(),
                nonstaking,
                Amount::from(1u64),
            ))
            .await?;
        handle
            .submit_admitted(dummy_record_with_fee(
                1,
                16,
                Vec::new(),
                nonstaking,
                Amount::from(2u64),
            ))
            .await?;
        handle
            .submit_admitted(dummy_record_with_fee(
                2,
                16,
                Vec::new(),
                nonstaking,
                Amount::from(3u64),
            ))
            .await?;
        let next = handle
            .freeze_next_candidate(1, 4, 512)
            .await?
            .expect("freeze");
        assert_eq!(next.lease.record_ids, vec![1, 2]);
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn nonstaking_never_evicts_staking() -> Result<()> {
        let nonstaking = dummy_nonstaking_asset(9);
        let handle = MempoolHandle::new(MempoolCoreConfig {
            max_store_bytes: 64,
            max_store_txs: 1,
            ingestion_buffer: 16,
            command_buffer: 16,
            eviction_policy: EvictionPolicy::OldestUnreservedFirst,
            fee_eviction_policy: FeeEvictionPolicy::LaunchStakingPriority,
        });
        handle
            .submit_admitted(dummy_record_with_fee(
                0,
                16,
                Vec::new(),
                *BASE_ASSET_ID,
                Amount::from(1u64),
            ))
            .await?;
        let outcome = handle
            .submit_admitted(dummy_record_with_fee(
                1,
                16,
                Vec::new(),
                nonstaking,
                Amount::from(100u64),
            ))
            .await?;
        assert!(matches!(
            outcome,
            AdmitOutcome::RejectedAtCapacityNoEvictable
        ));
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn reserved_records_are_never_evicted() -> Result<()> {
        let handle = MempoolHandle::new(MempoolCoreConfig {
            max_store_bytes: 64,
            max_store_txs: 2,
            ingestion_buffer: 16,
            command_buffer: 16,
            eviction_policy: EvictionPolicy::OldestUnreservedFirst,
            fee_eviction_policy: FeeEvictionPolicy::LaunchStakingPriority,
        });
        handle
            .submit_admitted(dummy_record(0, 16, Vec::new()))
            .await?;
        handle
            .submit_admitted(dummy_record(1, 16, Vec::new()))
            .await?;
        let frozen = handle
            .freeze_next_candidate(1, 1, 32)
            .await?
            .expect("frozen");
        assert_eq!(frozen.lease.record_ids, vec![0]);
        let outcome = handle
            .submit_admitted(dummy_record_with_fee(
                2,
                16,
                Vec::new(),
                *BASE_ASSET_ID,
                Amount::from(10u64),
            ))
            .await?;
        assert!(matches!(outcome, AdmitOutcome::Admitted { .. }));
        let next = handle
            .freeze_next_candidate(2, 4, 512)
            .await?
            .expect("next");
        assert_eq!(next.lease.record_ids, vec![2]);
        Ok(())
    }
}
