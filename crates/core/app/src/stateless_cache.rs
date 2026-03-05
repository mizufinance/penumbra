use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

const MAX_ENTRIES: usize = 4096;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CacheEntry {
    Valid,
    Invalid,
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
        let value = inner.map.get(hash)?;
        // Mark as recently referenced on hit so clock eviction gives it a second chance.
        value.referenced.store(true, Ordering::Relaxed);
        let entry = value.entry;
        drop(inner);
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
