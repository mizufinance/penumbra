#![deny(clippy::unwrap_used)]

// Route Rust-side allocations through jemalloc. Halves steady-state RSS versus the
// system allocator by returning freed memory promptly instead of holding it in
// per-thread arenas. Note: this only covers Rust allocations, not RocksDB's C++
// nor the embedded gnark Go runtime.
#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

// Bake an aggressive decay config so the e2e/container inherits it without needing
// `_RJEM_MALLOC_CONF` plumbed through the environment. `dirty_decay_ms:0` purges
// freed pages synchronously (works on macOS and Linux, no background thread). The
// env var still overrides this if set. Relax the decay for throughput-sensitive
// production if the synchronous purge shows up in profiles.
#[cfg(not(target_env = "msvc"))]
#[allow(non_upper_case_globals)]
#[export_name = "_rjem_malloc_conf"]
pub static malloc_conf: &[u8] = b"dirty_decay_ms:0,muzzy_decay_ms:0\0";

mod ffi;
mod service;

pub use service::{ErrorKind, ExecutionService, ServiceError};

#[cfg(test)]
mod service_contract_tests;
