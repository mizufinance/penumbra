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
use penumbra_sdk_proof_params::VerifyingKeyExt;

use crate::gnark::artifacts::{
    load_artifact_metadata, load_prepared_vk, validate_artifact_hashes, validate_artifact_metadata,
};
#[cfg(any(unix, windows))]
use crate::gnark::artifacts::{sha256_hex, GnarkArtifactMetadata};
use crate::gnark::runtime::{
    sha256_hex_path, validate_daemon_ready, GnarkDaemonProcess, GNARK_MAX_REQUEST_BYTES,
    GNARK_MAX_RESULT_BYTES,
};

#[repr(C)]
pub(crate) struct PenumbraGnarkInitResult {
    pub handle: u64,
    pub init_ms: f64,
    pub err_ptr: *mut c_void,
    pub err_len: usize,
}

#[repr(C)]
pub(crate) struct PenumbraGnarkBytesResult {
    pub ptr: *mut c_void,
    pub len: usize,
    pub status: u32,
    pub prove_ms: f64,
}

pub(crate) type PenumbraGnarkInit =
    unsafe extern "C" fn(*const c_char, usize, *mut PenumbraGnarkInitResult);
pub(crate) type PenumbraGnarkInitFromBytes =
    unsafe extern "C" fn(*const c_void, usize, *const c_void, usize, *mut PenumbraGnarkInitResult);
pub(crate) type PenumbraGnarkProve =
    unsafe extern "C" fn(u64, *const c_void, usize, *mut PenumbraGnarkBytesResult);
pub(crate) type PenumbraGnarkFree = unsafe extern "C" fn(*mut c_void, usize);
pub(crate) type PenumbraGnarkShutdown = unsafe extern "C" fn(u64);

pub(crate) enum GnarkTransport {
    #[cfg(any(unix, windows))]
    Library {
        _library: Library,
        prove: PenumbraGnarkProve,
        free: PenumbraGnarkFree,
        shutdown: PenumbraGnarkShutdown,
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
    pub env_artifact_dir: &'static str,
    pub env_lib: &'static str,
    pub env_daemon: &'static str,
    pub init_symbol: &'static [u8],
    pub init_from_bytes_symbol: &'static [u8],
    pub prove_symbol: &'static [u8],
    pub free_symbol: &'static [u8],
    pub shutdown_symbol: &'static [u8],
}

#[cfg(any(unix, windows))]
pub(crate) fn load_library_transport(
    lib_path: &Path,
    artifact_dir: &Path,
    config: &'static GnarkFamilyConfig,
) -> Result<(GnarkTransport, PreparedVerifyingKey<Bls12_377>)> {
    let metadata = load_artifact_metadata(artifact_dir)?;
    validate_artifact_metadata(&metadata, config.family)?;
    validate_artifact_hashes(artifact_dir, &metadata, config.family)?;

    let library = unsafe { Library::new(lib_path) }.with_context(|| {
        format!(
            "load gnark {} library {}",
            config.family,
            lib_path.display()
        )
    })?;
    let (init, prove, free, shutdown) = unsafe {
        let init: PenumbraGnarkInit = *library.get(config.init_symbol)?;
        let prove: PenumbraGnarkProve = *library.get(config.prove_symbol)?;
        let free: PenumbraGnarkFree = *library.get(config.free_symbol)?;
        let shutdown: PenumbraGnarkShutdown = *library.get(config.shutdown_symbol)?;
        (init, prove, free, shutdown)
    };

    let mut init_result = PenumbraGnarkInitResult {
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

    let pvk = load_prepared_vk(artifact_dir, &metadata, config.family)?;
    Ok((
        GnarkTransport::Library {
            _library: library,
            prove,
            free,
            shutdown,
            handle: init_result.handle,
            prove_mutex: Mutex::new(()),
        },
        pvk,
    ))
}

pub(crate) fn load_daemon_transport(
    binary: &Path,
    artifact_dir: &Path,
    config: &'static GnarkFamilyConfig,
) -> Result<(GnarkTransport, PreparedVerifyingKey<Bls12_377>)> {
    let metadata = load_artifact_metadata(artifact_dir)?;
    validate_artifact_metadata(&metadata, config.family)?;
    validate_artifact_hashes(artifact_dir, &metadata, config.family)?;

    let metadata_hash = sha256_hex_path(&artifact_dir.join("circuit_metadata.json"))?;
    let (process, ready) = GnarkDaemonProcess::spawn(binary, config.family, artifact_dir)?;
    validate_daemon_ready(
        &ready,
        config.family,
        &metadata_hash,
        metadata.proving_key_sha256_hex.as_deref(),
        metadata.verifying_key_sha256_hex.as_deref(),
        metadata.verifying_key_id.as_deref(),
    )?;

    let pvk = load_prepared_vk(artifact_dir, &metadata, config.family)?;
    Ok((
        GnarkTransport::Daemon {
            process: Mutex::new(process),
        },
        pvk,
    ))
}

#[cfg(any(unix, windows))]
pub(crate) fn load_bundled_transport(
    lib_path: &Path,
    pk_bytes: &[u8],
    pvk: &PreparedVerifyingKey<Bls12_377>,
    metadata_json: &[u8],
    config: &'static GnarkFamilyConfig,
) -> Result<GnarkTransport> {
    let metadata: GnarkArtifactMetadata = serde_json::from_slice(metadata_json)
        .with_context(|| format!("parse bundled {} circuit_metadata.json", config.family))?;
    validate_artifact_metadata(&metadata, config.family)?;
    if let Some(expected_pk_hash) = &metadata.proving_key_sha256_hex {
        let actual = sha256_hex(pk_bytes);
        if &actual != expected_pk_hash {
            bail!(
                "bundled {} proving key hash mismatch: expected {expected_pk_hash}, got {actual}",
                config.family
            );
        }
    }
    if let Some(expected_vk_id) = &metadata.verifying_key_id {
        let actual_id = pvk.debug_id();
        if &actual_id != expected_vk_id {
            bail!(
                "bundled {} verifying key id mismatch: expected {expected_vk_id}, got {actual_id}",
                config.family
            );
        }
    }

    let library = unsafe { Library::new(lib_path) }.with_context(|| {
        format!(
            "load gnark {} library {}",
            config.family,
            lib_path.display()
        )
    })?;
    let (init_from_bytes, prove, free, shutdown) = unsafe {
        let init_from_bytes: PenumbraGnarkInitFromBytes =
            *library.get(config.init_from_bytes_symbol)?;
        let prove: PenumbraGnarkProve = *library.get(config.prove_symbol)?;
        let free: PenumbraGnarkFree = *library.get(config.free_symbol)?;
        let shutdown: PenumbraGnarkShutdown = *library.get(config.shutdown_symbol)?;
        (init_from_bytes, prove, free, shutdown)
    };

    let mut init_result = PenumbraGnarkInitResult {
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

    Ok(GnarkTransport::Library {
        _library: library,
        prove,
        free,
        shutdown,
        handle: init_result.handle,
        prove_mutex: Mutex::new(()),
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
            let mut prove_result = PenumbraGnarkBytesResult {
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

    if let Some(p) = find_in(exe_dir) {
        return Some(p);
    }
    None
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

pub(crate) fn load_from_env_paths(
    config: &'static GnarkFamilyConfig,
) -> Result<(PathBuf, Option<PathBuf>, Option<PathBuf>)> {
    let artifact_dir = std::env::var_os(config.env_artifact_dir)
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("{} is not set", config.env_artifact_dir))?;
    let lib_path = std::env::var_os(config.env_lib).map(PathBuf::from);
    let daemon_path = std::env::var_os(config.env_daemon).map(PathBuf::from);
    Ok((artifact_dir, lib_path, daemon_path))
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
