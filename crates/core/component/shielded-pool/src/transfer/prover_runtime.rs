#[cfg(any(unix, windows))]
use anyhow::Result;
#[cfg(any(unix, windows))]
use std::{
    sync::{mpsc, LazyLock},
    thread,
};

#[cfg(any(unix, windows))]
use crate::{
    gnark::GnarkTransferClient,
    transfer::{TransferProof, TransferProofPrivate, TransferProofPublic},
};

#[cfg(any(unix, windows))]
enum TransferProverRuntimeRequest {
    Prove {
        public: TransferProofPublic,
        private: TransferProofPrivate,
        response: mpsc::Sender<Result<TransferProof, crate::ProofError>>,
    },
}

#[cfg(any(unix, windows))]
// The gnark transfer transport is owned by a dedicated runtime thread on purpose.
// This gives the process exactly one place that initializes, caches, and tears down
// the native proving clients, and it keeps libtest/Tokio worker threads from
// directly owning the Go `c-shared` transport. Requests are serialized through this
// worker today for correctness and shutdown predictability; callers still use the
// generic `TransferProof::prove` API, so the runtime can later relax this to bounded
// parallelism without changing the proving surface.
static TRANSFER_PROVER_RUNTIME: LazyLock<mpsc::Sender<TransferProverRuntimeRequest>> =
    LazyLock::new(|| {
        let (tx, rx) = mpsc::channel::<TransferProverRuntimeRequest>();
        thread::Builder::new()
            .name("transfer-prover-runtime".to_string())
            .spawn(move || {
                let mut client: Option<GnarkTransferClient> = None;
                while let Ok(request) = rx.recv() {
                    match request {
                        TransferProverRuntimeRequest::Prove {
                            public,
                            private,
                            response,
                        } => {
                            let result = (|| {
                                ensure_client(&mut client)?
                                    .prove(&public, &private)
                                    .map_err(|e| {
                                        crate::ProofError::ProofGenerationFailed(format!(
                                            "gnark transfer prove: {e}",
                                        ))
                                    })
                            })();
                            let _ = response.send(result);
                        }
                    }
                }
            })
            .expect("spawn transfer prover runtime worker");
        tx
    });

#[cfg(any(unix, windows))]
pub(super) fn prove_with_runtime(
    public: TransferProofPublic,
    private: TransferProofPrivate,
) -> Result<TransferProof, crate::ProofError> {
    // All transfer proofs funnel through the runtime owner above so that native
    // gnark client lifetime and teardown are centralized rather than being spread
    // across caller threads.
    call_runtime(public, private)
}

#[cfg(any(unix, windows))]
fn init_gnark_transfer_client() -> Result<GnarkTransferClient, crate::ProofError> {
    if GnarkTransferClient::env_override_configured() {
        return crate::gnark::GnarkTransferClient::from_env().map_err(|e| {
            crate::ProofError::ProofGenerationFailed(format!("gnark transfer init: {e}",))
        });
    }

    let lib_path = crate::gnark::GnarkTransferClient::bundled_lib_path().or_else(|| {
        #[cfg(any(unix, windows))]
        {
            crate::gnark::GnarkTransferClient::auto_lib_path()
        }
        #[cfg(not(any(unix, windows)))]
        {
            None
        }
    }).ok_or_else(|| {
            crate::ProofError::ProofGenerationFailed(
                "gnark transfer library not found (checked bundled path and executable-adjacent locations)".to_string(),
            )
        })?;
    let pk_bytes = penumbra_sdk_proof_params::transfer_proving_key_bytes();
    if pk_bytes.is_empty() {
        return Err(crate::ProofError::ProofGenerationFailed(format!(
            "gnark transfer proving key not bundled (enable bundled-proving-keys feature)",
        )));
    }
    let pvk = penumbra_sdk_proof_params::transfer_proof_verification_key().clone();
    let metadata = penumbra_sdk_proof_params::transfer_circuit_metadata();
    crate::gnark::GnarkTransferClient::from_bundled(&lib_path, pk_bytes, pvk, metadata)
        .map_err(|e| crate::ProofError::ProofGenerationFailed(format!("gnark transfer init: {e}")))
}

#[cfg(any(unix, windows))]
fn ensure_client(
    client: &mut Option<GnarkTransferClient>,
) -> Result<&GnarkTransferClient, crate::ProofError> {
    if client.is_none() {
        *client = Some(init_gnark_transfer_client()?);
    }
    Ok(client
        .as_ref()
        .expect("transfer prover runtime cached client"))
}

#[cfg(any(unix, windows))]
fn call_runtime(
    public: TransferProofPublic,
    private: TransferProofPrivate,
) -> Result<TransferProof, crate::ProofError> {
    let (response_tx, response_rx) = mpsc::channel();
    let request = TransferProverRuntimeRequest::Prove {
        public,
        private,
        response: response_tx,
    };
    TRANSFER_PROVER_RUNTIME.send(request).map_err(|_| {
        crate::ProofError::ProofGenerationFailed("transfer prover runtime channel closed".into())
    })?;
    response_rx.recv().map_err(|_| {
        crate::ProofError::ProofGenerationFailed(
            "transfer prover runtime exited before replying".into(),
        )
    })?
}
