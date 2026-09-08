#![cfg(all(feature = "prover", any(unix, windows)))]

use crate::{
    gnark::{prover_worker::ProverWorker, GnarkTransferClient},
    transfer::{TransferProof, TransferProofPrivate, TransferProofPublic},
    ProofError,
};
use std::sync::LazyLock;

static PROVER: LazyLock<ProverWorker<TransferProofPublic, TransferProofPrivate, TransferProof>> =
    LazyLock::new(|| {
        ProverWorker::spawn(
            "transfer-prover",
            Option::<GnarkTransferClient>::default,
            |clients, public: TransferProofPublic, private: TransferProofPrivate| {
                (|| -> anyhow::Result<_> {
                    if clients.is_none() {
                        *clients = Some(GnarkTransferClient::load()?);
                    }
                    clients
                        .as_ref()
                        .expect("loaded prover client")
                        .prove(&public, &private)
                })()
                .map_err(|error| {
                    ProofError::ProofGenerationFailed(format!("gnark transfer: {error}"))
                })
            },
        )
    });

pub(super) fn prove_with_runtime(
    public: TransferProofPublic,
    private: TransferProofPrivate,
) -> Result<TransferProof, ProofError> {
    PROVER.prove(public, private)
}
