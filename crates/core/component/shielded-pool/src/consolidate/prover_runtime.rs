use anyhow::Result;
#[cfg(any(unix, windows))]
use std::{
    collections::BTreeMap,
    sync::{mpsc, LazyLock},
    thread,
};

use crate::{
    consolidate::{ConsolidateProof, ConsolidateProofPrivate, ConsolidateProofPublic},
    gnark::GnarkConsolidateClient,
    ConsolidateFamilyId,
};

#[cfg(any(unix, windows))]
enum ConsolidateProverRuntimeRequest {
    Prove {
        public: ConsolidateProofPublic,
        private: ConsolidateProofPrivate,
        response: mpsc::Sender<Result<ConsolidateProof, crate::ProofError>>,
    },
}

#[cfg(any(unix, windows))]
static CONSOLIDATE_PROVER_RUNTIME: LazyLock<mpsc::Sender<ConsolidateProverRuntimeRequest>> =
    LazyLock::new(|| {
        let (tx, rx) = mpsc::channel::<ConsolidateProverRuntimeRequest>();
        thread::Builder::new()
            .name("consolidate-prover-runtime".to_string())
            .spawn(move || {
                let mut clients = BTreeMap::<ConsolidateFamilyId, GnarkConsolidateClient>::new();
                while let Ok(request) = rx.recv() {
                    match request {
                        ConsolidateProverRuntimeRequest::Prove {
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
            .expect("spawn consolidate prover runtime worker");
        tx
    });

#[cfg(any(unix, windows))]
pub(super) fn prove_with_runtime(
    public: ConsolidateProofPublic,
    private: ConsolidateProofPrivate,
) -> Result<ConsolidateProof, crate::ProofError> {
    call_runtime(public, private)
}

#[cfg(any(unix, windows))]
fn init_gnark_consolidate_client(
    family_id: ConsolidateFamilyId,
) -> Result<GnarkConsolidateClient, crate::ProofError> {
    if GnarkConsolidateClient::env_override_configured() {
        return crate::gnark::GnarkConsolidateClient::from_env(family_id).map_err(|e| {
            crate::ProofError::ProofGenerationFailed(format!(
                "gnark {} init: {e}",
                family_id.label()
            ))
        });
    }

    let lib_path = crate::gnark::GnarkConsolidateClient::bundled_lib_path().or_else(|| {
        #[cfg(any(unix, windows))]
        {
            crate::gnark::GnarkConsolidateClient::auto_lib_path()
        }
        #[cfg(not(any(unix, windows)))]
        {
            None
        }
    }).ok_or_else(|| {
        crate::ProofError::ProofGenerationFailed(
            "gnark consolidate library not found (checked bundled path and executable-adjacent locations)".to_string(),
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
    crate::gnark::GnarkConsolidateClient::from_bundled(
        &lib_path, pk_bytes, pvk, metadata, family_id,
    )
    .map_err(|e| {
        crate::ProofError::ProofGenerationFailed(format!("gnark {} init: {e}", family_id.label()))
    })
}

#[cfg(any(unix, windows))]
fn ensure_client<'a>(
    clients: &'a mut BTreeMap<ConsolidateFamilyId, GnarkConsolidateClient>,
    family_id: ConsolidateFamilyId,
) -> Result<&'a GnarkConsolidateClient, crate::ProofError> {
    if let std::collections::btree_map::Entry::Vacant(entry) = clients.entry(family_id) {
        entry.insert(init_gnark_consolidate_client(family_id)?);
    }
    Ok(clients
        .get(&family_id)
        .expect("consolidate prover runtime cached client"))
}

#[cfg(any(unix, windows))]
fn call_runtime(
    public: ConsolidateProofPublic,
    private: ConsolidateProofPrivate,
) -> Result<ConsolidateProof, crate::ProofError> {
    let (response_tx, response_rx) = mpsc::channel();
    let request = ConsolidateProverRuntimeRequest::Prove {
        public,
        private,
        response: response_tx,
    };
    CONSOLIDATE_PROVER_RUNTIME.send(request).map_err(|_| {
        crate::ProofError::ProofGenerationFailed("consolidate prover runtime channel closed".into())
    })?;
    response_rx.recv().map_err(|_| {
        crate::ProofError::ProofGenerationFailed(
            "consolidate prover runtime exited before replying".into(),
        )
    })?
}
