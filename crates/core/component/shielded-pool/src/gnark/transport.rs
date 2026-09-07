use std::{
    ffi::{c_char, c_void},
    path::{Path, PathBuf},
    slice,
    sync::Mutex,
};

#[cfg(any(unix, windows))]
use std::{ffi::CString, ptr};

#[cfg(any(unix, windows))]
use anyhow::Context;
use anyhow::{anyhow, bail, Result};
use ark_groth16::PreparedVerifyingKey;
use decaf377::Bls12_377;
#[cfg(any(unix, windows))]
use libloading::Library;

#[cfg(any(unix, windows))]
use crate::gnark::artifacts::sha256_hex;
use crate::gnark::artifacts::{
    load_artifact_metadata, load_artifact_metadata_bytes, load_prepared_vk, load_prepared_vk_bytes,
    validate_artifact_hashes, validate_artifact_metadata,
};
use crate::gnark::runtime::{
    sha256_hex_path, validate_daemon_ready, GnarkDaemonProcess, GNARK_MAX_REQUEST_BYTES,
    GNARK_MAX_RESULT_BYTES,
};

#[repr(C)]
pub(crate) struct ShielddGnarkInitResult {
    pub handle: u64,
    pub init_ms: f64,
    pub err_ptr: *mut c_void,
    pub err_len: usize,
}

#[repr(C)]
pub(crate) struct ShielddGnarkBytesResult {
    pub ptr: *mut c_void,
    pub len: usize,
    pub status: u32,
    pub prove_ms: f64,
}

pub(crate) type ShielddGnarkInit =
    unsafe extern "C" fn(*const c_char, usize, *mut ShielddGnarkInitResult);
pub(crate) type ShielddGnarkInitFromBytes =
    unsafe extern "C" fn(*const c_void, usize, *const c_void, usize, *mut ShielddGnarkInitResult);
pub(crate) type ShielddGnarkProve =
    unsafe extern "C" fn(u64, *const c_void, usize, *mut ShielddGnarkBytesResult);
pub(crate) type ShielddGnarkFree = unsafe extern "C" fn(*mut c_void, usize);
pub(crate) type ShielddGnarkShutdown = unsafe extern "C" fn(u64);

pub(crate) enum GnarkTransport {
    #[cfg(any(unix, windows))]
    Library {
        _library: Library,
        prove: ShielddGnarkProve,
        free: ShielddGnarkFree,
        shutdown: ShielddGnarkShutdown,
        handle: u64,
        prove_mutex: Mutex<()>,
    },
    Daemon {
        process: Mutex<GnarkDaemonProcess>,
    },
}

#[derive(Clone, Copy)]
pub(crate) struct GnarkFamilyConfig {
    pub family: &'static str,
    pub lib_basename: &'static str,
    pub bundled_library: Option<&'static str>,
    pub env_artifact_dir: &'static str,
    pub env_lib: &'static str,
    pub env_daemon: &'static str,
    pub init_symbol: &'static [u8],
    pub init_from_bytes_symbol: &'static [u8],
    pub prove_symbol: &'static [u8],
    pub free_symbol: &'static [u8],
    pub shutdown_symbol: &'static [u8],
}

pub(crate) struct GnarkClient {
    transport: GnarkTransport,
    pub verifying_key: PreparedVerifyingKey<Bls12_377>,
    config: &'static GnarkFamilyConfig,
}

pub(crate) struct BundledArtifacts<'a> {
    pub proving_key: &'a [u8],
    pub verifying_key: &'a [u8],
    pub metadata: &'a [u8],
}

impl GnarkFamilyConfig {
    pub fn env_override_configured(&self) -> bool {
        [self.env_lib, self.env_daemon, self.env_artifact_dir]
            .iter()
            .any(|name| std::env::var_os(name).is_some())
    }

    pub fn library_path(&self) -> Option<PathBuf> {
        self.bundled_library
            .map(PathBuf::from)
            .filter(|path| path.is_file())
            .or_else(|| {
                #[cfg(any(unix, windows))]
                {
                    auto_lib_path(self.lib_basename)
                }
                #[cfg(not(any(unix, windows)))]
                {
                    None
                }
            })
    }
}

struct ConfiguredTransport {
    artifact_dir: PathBuf,
    executable: TransportExecutable,
}

enum TransportExecutable {
    Library(PathBuf),
    Daemon(PathBuf),
}

impl GnarkFamilyConfig {
    fn configured_transport(&self) -> Result<ConfiguredTransport> {
        self.resolve_explicit_paths(
            std::env::var_os(self.env_artifact_dir).map(PathBuf::from),
            std::env::var_os(self.env_lib).map(PathBuf::from),
            std::env::var_os(self.env_daemon).map(PathBuf::from),
        )
    }

    fn resolve_explicit_paths(
        &self,
        artifact_dir: Option<PathBuf>,
        library: Option<PathBuf>,
        daemon: Option<PathBuf>,
    ) -> Result<ConfiguredTransport> {
        let artifact_dir =
            artifact_dir.ok_or_else(|| anyhow!("{} is not set", self.env_artifact_dir))?;
        let executable = match (library, daemon) {
            (Some(path), None) => TransportExecutable::Library(path),
            (None, Some(path)) => TransportExecutable::Daemon(path),
            (Some(_), Some(_)) => bail!(
                "{} and {} are mutually exclusive",
                self.env_lib,
                self.env_daemon
            ),
            (None, None) => bail!("expected {} or {} to be set", self.env_lib, self.env_daemon),
        };
        Ok(ConfiguredTransport {
            artifact_dir,
            executable,
        })
    }

    #[cfg(any(test, feature = "benchmark-helpers"))]
    pub fn require_test_prerequisites(&self, proving_key: &[u8]) -> Result<()> {
        anyhow::ensure!(
            !cfg!(debug_assertions),
            "proof-generation tests require a release build"
        );
        if self.env_override_configured() {
            let configured = self.configured_transport()?;
            let path = match configured.executable {
                TransportExecutable::Library(path) | TransportExecutable::Daemon(path) => path,
            };
            anyhow::ensure!(
                path.is_file(),
                "gnark {} transport not found: {}",
                self.family,
                path.display()
            );
            let metadata = load_artifact_metadata(&configured.artifact_dir)?;
            validate_artifact_metadata(&metadata, self.family)?;
            validate_artifact_hashes(&configured.artifact_dir, &metadata, self.family)?;
            load_prepared_vk(&configured.artifact_dir, self.family)?;
        } else {
            let path = self
                .library_path()
                .ok_or_else(|| anyhow!("gnark {} library not found", self.family))?;
            anyhow::ensure!(
                path.is_file(),
                "gnark {} library not found: {}",
                self.family,
                path.display()
            );
            anyhow::ensure!(
                !proving_key.is_empty(),
                "gnark {} proving key is missing; enable bundled-proving-keys",
                self.family
            );
        }
        Ok(())
    }
}

impl GnarkClient {
    pub fn load(
        config: &'static GnarkFamilyConfig,
        artifacts: BundledArtifacts<'_>,
    ) -> Result<Self> {
        if config.env_override_configured() {
            return Self::from_env(config);
        }
        let library = config
            .library_path()
            .ok_or_else(|| anyhow!("gnark {} library not found", config.family))?;
        if artifacts.proving_key.is_empty() {
            bail!(
                "gnark {} proving key not bundled (enable bundled-proving-keys feature)",
                config.family
            );
        }
        Self::from_bundled(config, &library, artifacts)
    }

    pub fn from_env(config: &'static GnarkFamilyConfig) -> Result<Self> {
        let configured = config.configured_transport()?;
        match configured.executable {
            TransportExecutable::Library(library) => {
                #[cfg(any(unix, windows))]
                {
                    load_library_transport(&library, &configured.artifact_dir, config)
                }
                #[cfg(not(any(unix, windows)))]
                {
                    let _ = library;
                    bail!("gnark library transport is not supported on this platform")
                }
            }
            TransportExecutable::Daemon(binary) => {
                load_daemon_transport(&binary, &configured.artifact_dir, config)
            }
        }
    }

    pub fn from_bundled(
        config: &'static GnarkFamilyConfig,
        library: &Path,
        artifacts: BundledArtifacts<'_>,
    ) -> Result<Self> {
        #[cfg(any(unix, windows))]
        {
            load_bundled_transport(
                library,
                artifacts.proving_key,
                artifacts.verifying_key,
                artifacts.metadata,
                config,
            )
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (config, library, artifacts);
            bail!("gnark bundled library loading is not supported on this platform")
        }
    }

    pub fn prove(&self, witness: &[u8]) -> Result<Vec<u8>> {
        prove_with_transport(&self.transport, witness, self.config.family)
    }
}

impl Drop for GnarkTransport {
    fn drop(&mut self) {
        shutdown_transport(self);
    }
}

#[cfg(any(unix, windows))]
pub(crate) fn load_library_transport(
    lib_path: &Path,
    artifact_dir: &Path,
    config: &'static GnarkFamilyConfig,
) -> Result<GnarkClient> {
    let metadata = load_artifact_metadata(artifact_dir)?;
    validate_artifact_metadata(&metadata, config.family)?;
    validate_artifact_hashes(artifact_dir, &metadata, config.family)?;

    let pvk = load_prepared_vk(artifact_dir, config.family)?;
    let library = unsafe { Library::new(lib_path) }.with_context(|| {
        format!(
            "load gnark {} library {}",
            config.family,
            lib_path.display()
        )
    })?;
    let (init, prove, free, shutdown) = unsafe {
        let init: ShielddGnarkInit = *library.get(config.init_symbol)?;
        let prove: ShielddGnarkProve = *library.get(config.prove_symbol)?;
        let free: ShielddGnarkFree = *library.get(config.free_symbol)?;
        let shutdown: ShielddGnarkShutdown = *library.get(config.shutdown_symbol)?;
        (init, prove, free, shutdown)
    };

    let mut init_result = ShielddGnarkInitResult {
        handle: 0,
        init_ms: 0.0,
        err_ptr: ptr::null_mut(),
        err_len: 0,
    };
    let artifact_dir_c = CString::new(artifact_dir.to_string_lossy().as_bytes().to_vec())
        .context("artifact dir path contains interior NUL byte")?;
    unsafe {
        init(
            artifact_dir_c.as_ptr(),
            artifact_dir_c.as_bytes().len(),
            &mut init_result,
        );
    }
    if !init_result.err_ptr.is_null() {
        let err_bytes = take_returned_bytes(init_result.err_ptr, init_result.err_len);
        unsafe { free(init_result.err_ptr, init_result.err_len) };
        let err_bytes = err_bytes?;
        bail!(
            "gnark {} init failed: {}",
            config.family,
            String::from_utf8_lossy(&err_bytes)
        );
    }

    Ok(GnarkClient {
        transport: GnarkTransport::Library {
            _library: library,
            prove,
            free,
            shutdown,
            handle: init_result.handle,
            prove_mutex: Mutex::new(()),
        },
        verifying_key: pvk,
        config,
    })
}

pub(crate) fn load_daemon_transport(
    binary: &Path,
    artifact_dir: &Path,
    config: &'static GnarkFamilyConfig,
) -> Result<GnarkClient> {
    let metadata = load_artifact_metadata(artifact_dir)?;
    validate_artifact_metadata(&metadata, config.family)?;
    validate_artifact_hashes(artifact_dir, &metadata, config.family)?;

    let metadata_hash = sha256_hex_path(&artifact_dir.join("circuit_metadata.json"))?;
    let (process, ready) = GnarkDaemonProcess::spawn(binary, config.family, artifact_dir)?;
    validate_daemon_ready(
        &ready,
        config.family,
        &metadata_hash,
        &metadata.proving_key_sha256_hex,
        &metadata.verifying_key_binary_sha256_hex,
        &metadata.verifying_key_json_sha256_hex,
    )?;

    let pvk = load_prepared_vk(artifact_dir, config.family)?;
    Ok(GnarkClient {
        transport: GnarkTransport::Daemon {
            process: Mutex::new(process),
        },
        verifying_key: pvk,
        config,
    })
}

#[cfg(any(unix, windows))]
pub(crate) fn load_bundled_transport(
    lib_path: &Path,
    pk_bytes: &[u8],
    vk_json_bytes: &[u8],
    metadata_json: &[u8],
    config: &'static GnarkFamilyConfig,
) -> Result<GnarkClient> {
    let metadata = load_artifact_metadata_bytes(
        metadata_json,
        &format!("bundled {} circuit_metadata.json", config.family),
    )?;
    validate_artifact_metadata(&metadata, config.family)?;
    if i64::try_from(pk_bytes.len()).ok() != Some(metadata.proving_key_size_bytes) {
        bail!(
            "bundled {} proving key size mismatch: expected {}, got {}",
            config.family,
            metadata.proving_key_size_bytes,
            pk_bytes.len()
        );
    }
    let actual = sha256_hex(pk_bytes);
    if actual != metadata.proving_key_sha256_hex {
        bail!(
            "bundled {} proving key hash mismatch: expected {}, got {actual}",
            config.family,
            metadata.proving_key_sha256_hex
        );
    }
    let actual = sha256_hex(vk_json_bytes);
    if actual != metadata.verifying_key_json_sha256_hex {
        bail!(
            "bundled {} JSON verifying key hash mismatch: expected {}, got {actual}",
            config.family,
            metadata.verifying_key_json_sha256_hex
        );
    }
    let pvk = load_prepared_vk_bytes(vk_json_bytes, config.family)?;
    let library = unsafe { Library::new(lib_path) }.with_context(|| {
        format!(
            "load gnark {} library {}",
            config.family,
            lib_path.display()
        )
    })?;
    let (init_from_bytes, prove, free, shutdown) = unsafe {
        let init_from_bytes: ShielddGnarkInitFromBytes =
            *library.get(config.init_from_bytes_symbol)?;
        let prove: ShielddGnarkProve = *library.get(config.prove_symbol)?;
        let free: ShielddGnarkFree = *library.get(config.free_symbol)?;
        let shutdown: ShielddGnarkShutdown = *library.get(config.shutdown_symbol)?;
        (init_from_bytes, prove, free, shutdown)
    };

    let mut init_result = ShielddGnarkInitResult {
        handle: 0,
        init_ms: 0.0,
        err_ptr: ptr::null_mut(),
        err_len: 0,
    };
    unsafe {
        init_from_bytes(
            pk_bytes.as_ptr() as *const c_void,
            pk_bytes.len(),
            metadata_json.as_ptr() as *const c_void,
            metadata_json.len(),
            &mut init_result,
        );
    }
    if !init_result.err_ptr.is_null() {
        let err_bytes = take_returned_bytes(init_result.err_ptr, init_result.err_len);
        unsafe { free(init_result.err_ptr, init_result.err_len) };
        let err_bytes = err_bytes?;
        bail!(
            "gnark {} init_from_bytes failed: {}",
            config.family,
            String::from_utf8_lossy(&err_bytes)
        );
    }

    Ok(GnarkClient {
        transport: GnarkTransport::Library {
            _library: library,
            prove,
            free,
            shutdown,
            handle: init_result.handle,
            prove_mutex: Mutex::new(()),
        },
        verifying_key: pvk,
        config,
    })
}

pub(crate) fn prove_with_transport(
    transport: &GnarkTransport,
    witness: &[u8],
    family: &str,
) -> Result<Vec<u8>> {
    validate_prove_request_len(family, witness)?;
    match transport {
        #[cfg(any(unix, windows))]
        GnarkTransport::Library {
            prove,
            free,
            handle,
            prove_mutex,
            ..
        } => {
            let _guard = prove_mutex
                .lock()
                .map_err(|_| anyhow!("gnark {family} library mutex poisoned"))?;
            let mut prove_result = ShielddGnarkBytesResult {
                ptr: ptr::null_mut(),
                len: 0,
                status: 0,
                prove_ms: 0.0,
            };
            unsafe {
                (prove)(
                    *handle,
                    witness.as_ptr() as *const c_void,
                    witness.len(),
                    &mut prove_result,
                );
            }
            let payload = take_returned_bytes(prove_result.ptr, prove_result.len);
            if !prove_result.ptr.is_null() {
                unsafe { (free)(prove_result.ptr, prove_result.len) };
            }
            let payload = payload?;
            if prove_result.status != 0 {
                bail!(
                    "gnark {family} prove failed: {}",
                    String::from_utf8_lossy(&payload)
                );
            }
            Ok(payload)
        }
        GnarkTransport::Daemon { process } => process
            .lock()
            .map_err(|_| anyhow!("gnark {family} daemon mutex poisoned"))?
            .prove(witness),
    }
}

pub(crate) fn shutdown_transport(transport: &mut GnarkTransport) {
    #[cfg(any(unix, windows))]
    if let GnarkTransport::Library {
        shutdown, handle, ..
    } = transport
    {
        if *handle != 0 {
            unsafe { (shutdown)(*handle) };
            *handle = 0;
        }
    }
    #[cfg(not(any(unix, windows)))]
    let _ = transport;
}

#[cfg(any(unix, windows))]
pub(crate) fn auto_lib_path(lib_basename: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    let exts = ["so", "dylib", "dll"];
    let find_in = |dir: &Path| -> Option<PathBuf> {
        exts.iter()
            .map(|e| dir.join(format!("{lib_basename}.{e}")))
            .find(|p| p.exists())
    };

    find_in(exe_dir).or_else(|| find_in(&exe_dir.join("../lib/shieldd")))
}

pub(crate) fn validate_prove_request_len(family: &str, witness: &[u8]) -> Result<()> {
    if witness.len() > GNARK_MAX_REQUEST_BYTES {
        bail!(
            "gnark {family} prove request {} bytes exceeds limit {GNARK_MAX_REQUEST_BYTES}",
            witness.len()
        );
    }
    Ok(())
}

fn take_returned_bytes(ptr: *mut c_void, len: usize) -> Result<Vec<u8>> {
    if len > GNARK_MAX_RESULT_BYTES {
        bail!("gnark library returned {len} bytes, limit {GNARK_MAX_RESULT_BYTES}");
    }
    if len == 0 {
        return Ok(Vec::new());
    }
    if ptr.is_null() {
        bail!("gnark library returned null pointer with {len} bytes");
    }
    Ok(unsafe { slice::from_raw_parts(ptr as *const u8, len) }.to_vec())
}

#[cfg(test)]
mod tests {
    use std::ptr::NonNull;

    use super::*;

    #[test]
    fn explicit_configuration_requires_one_transport_and_artifacts() {
        let config = &crate::gnark::transfer::TRANSFER_FAMILY_CONFIG;
        let artifacts = Some(PathBuf::from("artifacts"));
        let library = Some(PathBuf::from("library"));
        let daemon = Some(PathBuf::from("daemon"));
        assert!(config
            .resolve_explicit_paths(None, library.clone(), None)
            .is_err());
        assert!(config
            .resolve_explicit_paths(artifacts.clone(), None, None)
            .is_err());
        assert!(config
            .resolve_explicit_paths(artifacts.clone(), library.clone(), daemon.clone())
            .is_err());
        let selected = config
            .resolve_explicit_paths(artifacts.clone(), library, None)
            .expect("library config");
        assert!(matches!(
            selected.executable,
            TransportExecutable::Library(_)
        ));
        let selected = config
            .resolve_explicit_paths(artifacts, None, daemon)
            .expect("daemon config");
        assert!(matches!(
            selected.executable,
            TransportExecutable::Daemon(_)
        ));
    }

    #[test]
    fn validate_prove_request_len_rejects_oversized_request() {
        let witness = vec![0u8; GNARK_MAX_REQUEST_BYTES + 1];
        let err = validate_prove_request_len("transfer", &witness)
            .expect_err("oversized request should fail");

        assert!(
            err.to_string().contains("prove request"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn take_returned_bytes_rejects_oversized_result_before_copying() {
        let ptr = NonNull::<u8>::dangling().as_ptr() as *mut c_void;
        let err = take_returned_bytes(ptr, GNARK_MAX_RESULT_BYTES + 1)
            .expect_err("oversized result should fail");

        assert!(
            err.to_string().contains("gnark library returned"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn take_returned_bytes_rejects_null_pointer_with_nonzero_length() {
        let err = take_returned_bytes(std::ptr::null_mut(), 1)
            .expect_err("null pointer with data length should fail");

        assert!(
            err.to_string().contains("null pointer"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn take_returned_bytes_copies_bounded_bytes() {
        let bytes = [1u8, 2, 3];
        let copied = take_returned_bytes(bytes.as_ptr() as *mut c_void, bytes.len())
            .expect("bounded bytes should copy");

        assert_eq!(copied, bytes);
    }
}
