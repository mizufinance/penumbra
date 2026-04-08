use std::{ffi::c_void, fs, path::Path, ptr, time::Instant};

use anyhow::{anyhow, Context, Result};
use libloading::Library;

#[repr(C)]
struct PenumbraGnarkInitResult {
    handle: u64,
    init_ms: f64,
    err_ptr: *mut c_void,
    err_len: usize,
}

#[repr(C)]
struct PenumbraGnarkBytesResult {
    ptr: *mut c_void,
    len: usize,
    status: u32,
    prove_ms: f64,
}

type PenumbraGnarkTransferInitFromBytes =
    unsafe extern "C" fn(*const c_void, usize, *const c_void, usize, *mut PenumbraGnarkInitResult);
type PenumbraGnarkTransferProve =
    unsafe extern "C" fn(u64, *const c_void, usize, *mut PenumbraGnarkBytesResult);
type PenumbraGnarkTransferFree = unsafe extern "C" fn(*mut c_void, usize);
type PenumbraGnarkTransferShutdown = unsafe extern "C" fn(u64);

pub struct GnarkTransferClient {
    _library: Library,
    prove: PenumbraGnarkTransferProve,
    free: PenumbraGnarkTransferFree,
    shutdown: PenumbraGnarkTransferShutdown,
    handle: u64,
    lib_load_ms: f64,
    init_ms: f64,
}

pub struct GnarkTransferProofCall {
    pub payload: Vec<u8>,
}

impl GnarkTransferClient {
    pub fn load(lib_path: &Path, artifact_dir: &Path) -> Result<Self> {
        let pk_bytes = fs::read(artifact_dir.join("proving_key.bin"))
            .with_context(|| format!("read {}", artifact_dir.join("proving_key.bin").display()))?;
        let metadata_bytes =
            fs::read(artifact_dir.join("circuit_metadata.json")).with_context(|| {
                format!(
                    "read {}",
                    artifact_dir.join("circuit_metadata.json").display()
                )
            })?;
        let lib_start = Instant::now();
        let library = unsafe { Library::new(lib_path) }
            .with_context(|| format!("load gnark transfer library {}", lib_path.display()))?;
        let lib_load_ms = lib_start.elapsed().as_secs_f64() * 1000.0;

        let (init_from_bytes, prove, free, shutdown) = unsafe {
            let init_from_bytes: PenumbraGnarkTransferInitFromBytes =
                *library.get(b"penumbra_gnark_transfer_init_from_bytes")?;
            let prove: PenumbraGnarkTransferProve =
                *library.get(b"penumbra_gnark_transfer_prove")?;
            let free: PenumbraGnarkTransferFree = *library.get(b"penumbra_gnark_transfer_free")?;
            let shutdown: PenumbraGnarkTransferShutdown =
                *library.get(b"penumbra_gnark_transfer_shutdown")?;
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
                metadata_bytes.as_ptr() as *const c_void,
                metadata_bytes.len(),
                &mut init_result,
            );
        }
        if !init_result.err_ptr.is_null() {
            let err_bytes = take_returned_bytes(init_result.err_ptr, init_result.err_len);
            unsafe { free(init_result.err_ptr, init_result.err_len) };
            return Err(anyhow!(
                "gnark transfer init failed: {}",
                String::from_utf8_lossy(&err_bytes)
            ));
        }

        Ok(Self {
            _library: library,
            prove,
            free,
            shutdown,
            handle: init_result.handle,
            lib_load_ms,
            init_ms: init_result.init_ms,
        })
    }

    pub fn lib_load_ms(&self) -> f64 {
        self.lib_load_ms
    }

    pub fn init_ms(&self) -> f64 {
        self.init_ms
    }

    pub fn prove_raw(&self, witness: &[u8]) -> Result<GnarkTransferProofCall> {
        let mut prove_result = PenumbraGnarkBytesResult {
            ptr: ptr::null_mut(),
            len: 0,
            status: 0,
            prove_ms: 0.0,
        };
        unsafe {
            (self.prove)(
                self.handle,
                witness.as_ptr() as *const c_void,
                witness.len(),
                &mut prove_result,
            );
        }

        let payload = take_returned_bytes(prove_result.ptr, prove_result.len);
        if !prove_result.ptr.is_null() {
            unsafe { (self.free)(prove_result.ptr, prove_result.len) };
        }
        if prove_result.status != 0 {
            return Err(anyhow!(
                "gnark transfer prove failed: {}",
                String::from_utf8_lossy(&payload)
            ));
        }

        Ok(GnarkTransferProofCall { payload })
    }
}

impl Drop for GnarkTransferClient {
    fn drop(&mut self) {
        if self.handle != 0 {
            unsafe { (self.shutdown)(self.handle) };
            self.handle = 0;
        }
    }
}

fn take_returned_bytes(ptr: *mut c_void, len: usize) -> Vec<u8> {
    if ptr.is_null() || len == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(ptr as *const u8, len) }.to_vec()
    }
}
