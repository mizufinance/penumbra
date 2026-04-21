use anyhow::Result;
#[cfg(any(unix, windows))]
use std::{
    collections::BTreeMap,
    sync::{mpsc, LazyLock},
    thread,
};

use crate::{
    gnark::GnarkSplitClient,
    split::{SplitProof, SplitProofPrivate, SplitProofPublic},
    SplitFamilyId,
};

#[cfg(any(unix, windows))]
enum SplitProverRuntimeRequest {
    Prove {
        public: SplitProofPublic,
        private: SplitProofPrivate,
        response: mpsc::Sender<Result<SplitProof, crate::ProofError>>,
    },
}

#[cfg(any(unix, windows))]
static SPLIT_PROVER_RUNTIME: LazyLock<mpsc::Sender<SplitProverRuntimeRequest>> =
    LazyLock::new(|| {
        let (tx, rx) = mpsc::channel::<SplitProverRuntimeRequest>();
        thread::Builder::new()
            .name("split-prover-runtime".to_string())
            .spawn(move || {
                let mut clients = BTreeMap::<SplitFamilyId, GnarkSplitClient>::new();
                while let Ok(request) = rx.recv() {
                    match request {
                        SplitProverRuntimeRequest::Prove {
                            public,
                            private,
                            response,
                        } => {
                            let family_id = public.family_id;
                            let result = (|| {
                                ensure_client(&mut clients, family_id)?
                                    .prove(&public, &private)
                                    .map_err(|e| {
                                        crate::ProofError::ProofGenerationFailed(format!(
                                            "gnark {} prove: {e}",
                                            family_id.label()
                                        ))
                                    })
                            })();
                            let _ = response.send(result);
                        }
                    }
                }
            })
            .expect("spawn split prover runtime worker");
        tx
    });

#[cfg(any(unix, windows))]
pub(super) fn prove_with_runtime(
    public: SplitProofPublic,
    private: SplitProofPrivate,
) -> Result<SplitProof, crate::ProofError> {
    call_runtime(public, private)
}

#[cfg(any(unix, windows))]
fn init_gnark_split_client(
    family_id: SplitFamilyId,
) -> Result<GnarkSplitClient, crate::ProofError> {
    if GnarkSplitClient::env_override_configured() {
        return crate::gnark::GnarkSplitClient::from_env(family_id).map_err(|e| {
            crate::ProofError::ProofGenerationFailed(format!(
                "gnark {} init: {e}",
                family_id.label()
            ))
        });
    }

    let lib_path = crate::gnark::GnarkSplitClient::bundled_lib_path()
        .or_else(crate::gnark::GnarkSplitClient::auto_lib_path)
        .ok_or_else(|| {
        crate::ProofError::ProofGenerationFailed(
            "gnark split library not found (checked bundled path and executable-adjacent locations)".to_string(),
        )
    })?;
    let pk_bytes = family_id.proving_key_bytes();
    if pk_bytes.is_empty() {
        return Err(crate::ProofError::ProofGenerationFailed(format!(
            "gnark {} proving key not bundled (enable bundled-proving-keys feature)",
            family_id.label()
        )));
    }
    let pvk = family_id.proof_verification_key().clone();
    let metadata = family_id.circuit_metadata_bytes();
    crate::gnark::GnarkSplitClient::from_bundled(&lib_path, pk_bytes, pvk, metadata, family_id)
        .map_err(|e| {
            crate::ProofError::ProofGenerationFailed(format!(
                "gnark {} init: {e}",
                family_id.label()
            ))
        })
}

#[cfg(any(unix, windows))]
fn ensure_client<'a>(
    clients: &'a mut BTreeMap<SplitFamilyId, GnarkSplitClient>,
    family_id: SplitFamilyId,
) -> Result<&'a GnarkSplitClient, crate::ProofError> {
    if let std::collections::btree_map::Entry::Vacant(entry) = clients.entry(family_id) {
        entry.insert(init_gnark_split_client(family_id)?);
    }
    Ok(clients
        .get(&family_id)
        .expect("split prover runtime cached client"))
}

#[cfg(any(unix, windows))]
fn call_runtime(
    public: SplitProofPublic,
    private: SplitProofPrivate,
) -> Result<SplitProof, crate::ProofError> {
    let (response_tx, response_rx) = mpsc::channel();
    let request = SplitProverRuntimeRequest::Prove {
        public,
        private,
        response: response_tx,
    };
    SPLIT_PROVER_RUNTIME.send(request).map_err(|_| {
        crate::ProofError::ProofGenerationFailed("split prover runtime channel closed".into())
    })?;
    response_rx.recv().map_err(|_| {
        crate::ProofError::ProofGenerationFailed(
            "split prover runtime exited before replying".into(),
        )
    })?
}
