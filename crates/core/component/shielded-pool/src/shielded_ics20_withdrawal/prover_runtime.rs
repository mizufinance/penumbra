use anyhow::Result;
use std::{
    collections::BTreeMap,
    sync::{mpsc, LazyLock},
    thread,
};

use crate::{
    gnark::GnarkShieldedIcs20WithdrawalClient,
    shielded_ics20_withdrawal::{
        ShieldedIcs20WithdrawalProof, ShieldedIcs20WithdrawalProofPrivate,
        ShieldedIcs20WithdrawalProofPublic,
    },
    ShieldedIcs20WithdrawalFamilyId,
};

enum ShieldedIcs20WithdrawalProverRuntimeRequest {
    Prove {
        public: ShieldedIcs20WithdrawalProofPublic,
        private: ShieldedIcs20WithdrawalProofPrivate,
        response: mpsc::Sender<Result<ShieldedIcs20WithdrawalProof, crate::ProofError>>,
    },
}

static SHIELDED_ICS20_WITHDRAWAL_PROVER_RUNTIME: LazyLock<
    mpsc::Sender<ShieldedIcs20WithdrawalProverRuntimeRequest>,
> = LazyLock::new(|| {
    let (tx, rx) = mpsc::channel::<ShieldedIcs20WithdrawalProverRuntimeRequest>();
    thread::Builder::new()
        .name("shielded-ics20-withdrawal-prover-runtime".to_string())
        .spawn(move || {
            let mut clients = BTreeMap::<
                ShieldedIcs20WithdrawalFamilyId,
                GnarkShieldedIcs20WithdrawalClient,
            >::new();
            while let Ok(request) = rx.recv() {
                match request {
                    ShieldedIcs20WithdrawalProverRuntimeRequest::Prove {
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
        .expect("spawn shielded ICS-20 withdrawal prover runtime worker");
    tx
});

pub(super) fn prove_with_runtime(
    public: ShieldedIcs20WithdrawalProofPublic,
    private: ShieldedIcs20WithdrawalProofPrivate,
) -> Result<ShieldedIcs20WithdrawalProof, crate::ProofError> {
    call_runtime(public, private)
}

fn init_gnark_shielded_ics20_withdrawal_client(
    family_id: ShieldedIcs20WithdrawalFamilyId,
) -> Result<GnarkShieldedIcs20WithdrawalClient, crate::ProofError> {
    if GnarkShieldedIcs20WithdrawalClient::env_override_configured() {
        return GnarkShieldedIcs20WithdrawalClient::from_env(family_id).map_err(|e| {
            crate::ProofError::ProofGenerationFailed(format!(
                "gnark {} init: {e}",
                family_id.label()
            ))
        });
    }

    let lib_path = GnarkShieldedIcs20WithdrawalClient::bundled_lib_path()
        .or_else(GnarkShieldedIcs20WithdrawalClient::auto_lib_path)
        .ok_or_else(|| {
            crate::ProofError::ProofGenerationFailed(
                "gnark shielded ICS-20 withdrawal library not found (checked bundled path and executable-adjacent locations)".to_string(),
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
    GnarkShieldedIcs20WithdrawalClient::from_bundled(&lib_path, pk_bytes, pvk, metadata, family_id)
        .map_err(|e| {
            crate::ProofError::ProofGenerationFailed(format!(
                "gnark {} init: {e}",
                family_id.label()
            ))
        })
}

fn ensure_client<'a>(
    clients: &'a mut BTreeMap<ShieldedIcs20WithdrawalFamilyId, GnarkShieldedIcs20WithdrawalClient>,
    family_id: ShieldedIcs20WithdrawalFamilyId,
) -> Result<&'a GnarkShieldedIcs20WithdrawalClient, crate::ProofError> {
    if let std::collections::btree_map::Entry::Vacant(entry) = clients.entry(family_id) {
        entry.insert(init_gnark_shielded_ics20_withdrawal_client(family_id)?);
    }
    Ok(clients
        .get(&family_id)
        .expect("shielded ICS-20 withdrawal prover runtime cached client"))
}

fn call_runtime(
    public: ShieldedIcs20WithdrawalProofPublic,
    private: ShieldedIcs20WithdrawalProofPrivate,
) -> Result<ShieldedIcs20WithdrawalProof, crate::ProofError> {
    let (response_tx, response_rx) = mpsc::channel();
    let request = ShieldedIcs20WithdrawalProverRuntimeRequest::Prove {
        public,
        private,
        response: response_tx,
    };
    SHIELDED_ICS20_WITHDRAWAL_PROVER_RUNTIME
        .send(request)
        .map_err(|_| {
            crate::ProofError::ProofGenerationFailed(
                "shielded ICS-20 withdrawal prover runtime channel closed".into(),
            )
        })?;
    response_rx.recv().map_err(|_| {
        crate::ProofError::ProofGenerationFailed(
            "shielded ICS-20 withdrawal prover runtime exited before replying".into(),
        )
    })?
}
