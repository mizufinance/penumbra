//! Crate-specific metrics functionality.
//!
//! This module re-exports the contents of the `metrics` crate.  This is
//! effectively a way to monkey-patch the functions in this module into the
//! `metrics` crate, at least from the point of view of the other code in this
//! crate.
//!
//! Code in this crate that wants to use metrics should `use crate::metrics;`,
//! so that this module shadows the `metrics` crate.
//!
//! This trick is probably good to avoid in general, because it could be
//! confusing, but in this limited case, it seems like a clean option.

pub use metrics::*;

/// Registers all metrics used by this crate.
pub fn register_metrics() {
    cnidarium::register_metrics();
    penumbra_sdk_validator::component::register_metrics();
    // TODO: this should be under component::
    penumbra_sdk_governance::register_metrics();
    penumbra_sdk_ibc::component::register_metrics();
    penumbra_sdk_shielded_pool::component::register_metrics();

    describe_counter!(
        MEMPOOL_CHECKTX_TOTAL,
        Unit::Count,
        "The total number of checktx requests made to the mempool"
    );
    describe_histogram!(
        MEMPOOL_CHECKTX_DURATION,
        Unit::Seconds,
        "End-to-end checktx duration"
    );
    describe_gauge!(
        MEMPOOL_CHECKTX_PENDING,
        Unit::Count,
        "Number of checktx requests accepted by the mempool service and not yet completed"
    );
    describe_gauge!(
        MEMPOOL_CHECKTX_IN_FLIGHT,
        Unit::Count,
        "Number of checktx requests currently holding a worker permit"
    );
    describe_counter!(
        STATELESS_CACHE_HIT_VALID_TOTAL,
        Unit::Count,
        "Total number of stateless cache hits for valid entries"
    );
    describe_counter!(
        STATELESS_CACHE_HIT_INVALID_TOTAL,
        Unit::Count,
        "Total number of stateless cache hits for invalid entries"
    );
    describe_counter!(
        STATELESS_CACHE_MISS_TOTAL,
        Unit::Count,
        "Total number of stateless cache misses"
    );
    describe_counter!(
        STATELESS_CACHE_EVICTION_TOTAL,
        Unit::Count,
        "Total number of stateless cache evictions"
    );
    describe_counter!(
        TX_ARTIFACT_BUILD_TOTAL,
        Unit::Count,
        "Total number of transaction artifacts built"
    );
    describe_histogram!(
        TX_ARTIFACT_BUILD_DURATION,
        Unit::Seconds,
        "Wall-clock duration for transaction artifact production"
    );
    describe_counter!(
        TX_ARTIFACT_REUSE_TOTAL,
        Unit::Count,
        "Total number of transaction artifacts reused from cache"
    );
    describe_histogram!(
        CONSENSUS_PHASE_DURATION,
        Unit::Seconds,
        "Wall-clock duration spent handling a consensus phase in pd"
    );
    describe_histogram!(
        CONSENSUS_BLOCK_TX_COUNT,
        Unit::Count,
        "Per-block transaction counts observed at consensus boundaries"
    );
    describe_histogram!(
        CONSENSUS_BLOCK_IDLE_GAP,
        Unit::Seconds,
        "Wall-clock idle gap between the previous commit completion and the next proposal start"
    );
}

pub const MEMPOOL_CHECKTX_TOTAL: &str = "penumbra_pd_mempool_checktx_total";
pub const MEMPOOL_CHECKTX_DURATION: &str = "penumbra_pd_mempool_checktx_duration_seconds";
pub const MEMPOOL_CHECKTX_PENDING: &str = "penumbra_pd_mempool_checktx_pending";
pub const MEMPOOL_CHECKTX_IN_FLIGHT: &str = "penumbra_pd_mempool_checktx_in_flight";
pub const STATELESS_CACHE_HIT_VALID_TOTAL: &str = "penumbra_pd_stateless_cache_hit_valid_total";
pub const STATELESS_CACHE_HIT_INVALID_TOTAL: &str = "penumbra_pd_stateless_cache_hit_invalid_total";
pub const STATELESS_CACHE_MISS_TOTAL: &str = "penumbra_pd_stateless_cache_miss_total";
pub const STATELESS_CACHE_EVICTION_TOTAL: &str = "penumbra_pd_stateless_cache_eviction_total";
pub const TX_ARTIFACT_BUILD_TOTAL: &str = "penumbra_pd_tx_artifact_build_total";
pub const TX_ARTIFACT_BUILD_DURATION: &str = "penumbra_pd_tx_artifact_build_duration_seconds";
pub const TX_ARTIFACT_REUSE_TOTAL: &str = "penumbra_pd_tx_artifact_reuse_total";
pub const CONSENSUS_PHASE_DURATION: &str = "penumbra_pd_consensus_phase_duration_seconds";
pub const CONSENSUS_BLOCK_TX_COUNT: &str = "penumbra_pd_consensus_block_tx_count";
pub const CONSENSUS_BLOCK_IDLE_GAP: &str = "penumbra_pd_consensus_block_idle_gap_seconds";
