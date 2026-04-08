//! Integration test helpers for `pmonitor`.
//! The runner coordinates four concerns:
//! - resolving prebuilt test binaries
//! - creating wallet fixtures and genesis allocations
//! - starting and stopping the local devnet
//! - driving `pmonitor audit` until it reaches the expected state

use once_cell::sync::Lazy;
use std::fs::{create_dir_all, remove_dir_all};
use std::path::{Path, PathBuf};

mod audit;
mod binaries;
mod devnet;
pub mod pcli_helpers;
mod wallets;

pub use audit::ExpectedAudit;

/// The TCP port for the process-compose API, used to start/stop devnet.
const PROCESS_COMPOSE_PORT: u16 = 8888;

/// The path to the root of the git repo, used for setting the working directory
/// when running `process-compose`.
static REPO_ROOT: Lazy<PathBuf> = Lazy::new(|| {
    [env!("CARGO_MANIFEST_DIR"), "../", "../", "../"]
        .iter()
        .collect()
});

/// Manager for running suites of integration tests for `pmonitor`.
/// Only one instance should exist at a time. The test suites assume access
/// to global resources such as 8080/TCP for `pd`, and a fixed directory in `/tmp/`.
pub struct PmonitorTestRunner {
    pub(super) pmonitor_integration_test_dir: PathBuf,
    pub(super) num_wallets: u16,
    pub(super) binaries: TestBinaries,
}

#[derive(Clone)]
pub struct TestBinaries {
    pub pd: PathBuf,
    pub pcli: PathBuf,
    pub pmonitor: PathBuf,
}

impl Drop for PmonitorTestRunner {
    fn drop(&mut self) {
        let _ = self.stop_devnet();
    }
}

impl PmonitorTestRunner {
    /// Create a new test runner environment.
    /// Caller must ensure no other instances exist, because this method
    /// will destroy existing test data directories.
    pub fn new() -> Self {
        let binaries = binaries::resolve_test_binaries()
            .expect("failed to resolve prebuilt pmonitor test binaries");
        let root: PathBuf = ["/tmp", "pmonitor-integration-test"].iter().collect();
        if root.exists() {
            remove_dir_all(&root)
                .expect("failed to remove directory for pmonitor integration tests");
        }
        create_dir_all(&root).expect("failed to create directory for pmonitor integration tests");
        Self {
            pmonitor_integration_test_dir: root,
            num_wallets: 10,
            binaries,
        }
    }

    /// Return path for pmonitor home directory.
    /// Does not create the path, because `pmonitor` will fail if its home already exists.
    pub fn pmonitor_home(&self) -> PathBuf {
        self.pmonitor_integration_test_dir.join("pmonitor")
    }

    pub fn pcli_binary(&self) -> &Path {
        &self.binaries.pcli
    }
}
