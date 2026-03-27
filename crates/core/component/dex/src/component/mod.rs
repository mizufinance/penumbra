//! The dex component contains implementations of the Penumbra dex with token
//! supplies based on liquidity provider interactions.

pub mod metrics;
pub mod rpc;

pub mod router;

mod action_handler;
mod arb;
mod chandelier;
pub(crate) mod circuit_breaker;
mod dex;
mod eviction_manager;
mod flow;
mod lqt;
mod position_manager;
mod swap_manager;

pub use action_handler::swap::swap_check_stateless_and_extract;
pub use action_handler::swap_claim::swap_claim_check_stateless_and_extract;
pub use dex::InternalDexWrite;
pub use dex::{Dex, StateReadExt, StateWriteExt};
pub use position_manager::PositionManager;

// Read data from the Dex component;
pub use lqt::LqtRead;
pub use position_manager::PositionRead;
pub use swap_manager::SwapDataRead;
pub use swap_manager::SwapDataWrite;

pub(crate) use arb::Arbitrage;
pub(crate) use circuit_breaker::ExecutionCircuitBreaker;
pub(crate) use circuit_breaker::ValueCircuitBreaker;
pub use circuit_breaker::ValueCircuitBreakerRead;
pub(crate) use swap_manager::SwapManager;

#[cfg(test)]
pub(crate) mod tests;

pub use self::metrics::register_metrics;
