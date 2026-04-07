use anyhow::{anyhow, Context, Result};
use std::env;
use std::path::PathBuf;

use super::TestBinaries;

pub(super) fn resolve_test_binaries() -> Result<TestBinaries> {
    Ok(TestBinaries {
        pd: resolve_binary("pd")?,
        pcli: resolve_binary("pcli")?,
        pmonitor: resolve_binary("pmonitor")?,
    })
}

fn resolve_binary(name: &str) -> Result<PathBuf> {
    if let Some(path) = env::var_os(format!("CARGO_BIN_EXE_{name}")) {
        return Ok(PathBuf::from(path));
    }

    let profile_dir = current_profile_dir()?;
    let candidate = profile_dir.join(format!("{name}{}", env::consts::EXE_SUFFIX));
    if candidate.is_file() {
        return Ok(candidate);
    }

    Err(anyhow!(
        "missing prebuilt binary `{name}` at {}; build the integration binaries first",
        candidate.display()
    ))
}

fn current_profile_dir() -> Result<PathBuf> {
    let current_exe = env::current_exe().context("resolve current test executable")?;
    current_exe
        .parent()
        .and_then(|deps| deps.parent())
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("failed to determine cargo target profile directory"))
}
