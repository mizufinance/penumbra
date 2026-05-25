use parking_lot::RwLock;
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::metrics;
use penumbra_sdk_proof_aggregation::ProofFamilyId;
use penumbra_sdk_proof_params::batch::BatchItem;
use penumbra_sdk_sct::Nullifier;
use penumbra_sdk_tct::{Root, StateCommitment};
use penumbra_sdk_transaction::Transaction;

const MAX_ENTRIES: usize = 65_536;

#[derive(Clone)]
pub enum CacheEntry {
    Extracted(Arc<TxArtifact>),
    FullyVerified(Arc<TxArtifact>),
    Invalid,
}

#[derive(Clone)]
pub struct TxArtifact {
    pub tx: Arc<Transaction>,
    pub proof_items: BTreeMap<ProofFamilyId, Vec<BatchItem>>,
    pub spend_nullifiers: Vec<Nullifier>,
    pub anchor_pairs: Vec<(StateCommitment, StateCommitment)>,
    pub total_proof_count: usize,
    pub historical_validation: Option<HistoricalValidationStamp>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HistoricalValidationStamp {
    pub snapshot_version: u64,
    pub anchor: Root,
}

impl TxArtifact {
    pub fn with_historical_validation(
        &self,
        historical_validation: HistoricalValidationStamp,
    ) -> Arc<Self> {
        if self.historical_validation == Some(historical_validation) {
            return Arc::new(self.clone());
        }

        Arc::new(Self {
            tx: self.tx.clone(),
            proof_items: self.proof_items.clone(),
            spend_nullifiers: self.spend_nullifiers.clone(),
            anchor_pairs: self.anchor_pairs.clone(),
            total_proof_count: self.total_proof_count,
            historical_validation: Some(historical_validation),
        })
    }

    pub fn with_historical_validation_owned(
        mut self: Arc<Self>,
        historical_validation: HistoricalValidationStamp,
    ) -> Arc<Self> {
        if self.historical_validation != Some(historical_validation) {
            Arc::make_mut(&mut self).historical_validation = Some(historical_validation);
        }
        self
    }

    pub fn has_matching_historical_validation(&self, snapshot_version: u64) -> bool {
        self.historical_validation.as_ref().is_some_and(|stamp| {
            stamp.snapshot_version == snapshot_version && stamp.anchor == self.tx.anchor
        })
    }
}

impl CacheEntry {
    /// Returns the artifact if this entry holds one, regardless of verification tier.
    pub fn artifact(&self) -> Option<Arc<TxArtifact>> {
        match self {
            CacheEntry::Extracted(a) | CacheEntry::FullyVerified(a) => Some(a.clone()),
            CacheEntry::Invalid => None,
        }
    }
}

/// Bounded cache for stateless verification results, shared across ABCI passes.
///
/// Keyed by SHA-256 of raw tx bytes. Eviction uses a second-chance (clock)
/// policy to avoid full-cache flushes under bursty load.
pub struct StatelessCache {
    inner: RwLock<CacheInner>,
}

struct CacheValue {
    entry: CacheEntry,
    referenced: AtomicBool,
}

struct CacheInner {
    map: HashMap<[u8; 32], CacheValue>,
    // Keys in clock order. We keep this compact and bounded (MAX_ENTRIES),
    // so O(n) removes are acceptable and keep ordering deterministic.
    clock: Vec<[u8; 32]>,
    hand: usize,
}

impl StatelessCache {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(CacheInner {
                map: HashMap::with_capacity(MAX_ENTRIES / 2),
                clock: Vec::with_capacity(MAX_ENTRIES),
                hand: 0,
            }),
        }
    }

    pub fn get(&self, hash: &[u8; 32]) -> Option<CacheEntry> {
        let inner = self.inner.read();
        let Some(value) = inner.map.get(hash) else {
            metrics::counter!(metrics::STATELESS_CACHE_MISS_TOTAL).increment(1);
            return None;
        };
        // Mark as recently referenced on hit so clock eviction gives it a second chance.
        value.referenced.store(true, Ordering::Relaxed);
        let entry = value.entry.clone();
        drop(inner);
        match entry {
            CacheEntry::Extracted(_) | CacheEntry::FullyVerified(_) => {
                metrics::counter!(metrics::STATELESS_CACHE_HIT_VALID_TOTAL).increment(1)
            }
            CacheEntry::Invalid => {
                metrics::counter!(metrics::STATELESS_CACHE_HIT_INVALID_TOTAL).increment(1)
            }
        }
        Some(entry)
    }

    pub fn insert(&self, hash: [u8; 32], entry: CacheEntry) {
        let mut inner = self.inner.write();

        if let Some(value) = inner.map.get_mut(&hash) {
            value.entry = entry;
            value.referenced.store(true, Ordering::Relaxed);
            return;
        }

        if inner.map.len() >= MAX_ENTRIES {
            evict_one_clock(&mut inner);
        }

        inner.map.insert(
            hash,
            CacheValue {
                entry,
                referenced: AtomicBool::new(true),
            },
        );
        inner.clock.push(hash);
    }

    pub fn insert_extracted(&self, hash: [u8; 32], artifact: Arc<TxArtifact>) {
        self.insert(hash, CacheEntry::Extracted(artifact));
    }

    pub fn insert_fully_verified(&self, hash: [u8; 32], artifact: Arc<TxArtifact>) {
        self.insert(hash, CacheEntry::FullyVerified(artifact));
    }

    pub fn insert_invalid(&self, hash: [u8; 32]) {
        self.insert(hash, CacheEntry::Invalid);
    }
}

fn evict_one_clock(inner: &mut CacheInner) {
    while !inner.clock.is_empty() {
        if inner.hand >= inner.clock.len() {
            inner.hand = 0;
        }

        let key = inner.clock[inner.hand];
        match inner.map.get_mut(&key) {
            Some(value) if value.referenced.swap(false, Ordering::Relaxed) => {
                inner.hand = (inner.hand + 1) % inner.clock.len();
            }
            Some(_) => {
                inner.map.remove(&key);
                inner.clock.remove(inner.hand);
                if inner.hand >= inner.clock.len() && !inner.clock.is_empty() {
                    inner.hand = 0;
                }
                metrics::counter!(metrics::STATELESS_CACHE_EVICTION_TOTAL).increment(1);
                return;
            }
            None => {
                // Clock contains stale key (should be rare); compact and continue.
                inner.clock.remove(inner.hand);
                if inner.hand >= inner.clock.len() && !inner.clock.is_empty() {
                    inner.hand = 0;
                }
            }
        }
    }
}
