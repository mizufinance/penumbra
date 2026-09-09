#![cfg(all(feature = "prover", any(unix, windows)))]

use crate::ShieldedWithdrawalFamilyId;
use crate::{
    gnark::{prover_worker::ProverWorker, GnarkShieldedWithdrawalClient},
    shielded_withdrawal::{
        ShieldedWithdrawalProof, ShieldedWithdrawalProofPrivate, ShieldedWithdrawalProofPublic,
    },
    ProofError,
};
use std::collections::BTreeMap;
use std::sync::LazyLock;

static PROVER: LazyLock<
    ProverWorker<
        ShieldedWithdrawalProofPublic,
        ShieldedWithdrawalProofPrivate,
        ShieldedWithdrawalProof,
    >,
> = LazyLock::new(|| {
    ProverWorker::spawn(
        "shielded_withdrawal-prover",
        BTreeMap::<ShieldedWithdrawalFamilyId, GnarkShieldedWithdrawalClient>::new,
        |clients,
         public: ShieldedWithdrawalProofPublic,
         private: ShieldedWithdrawalProofPrivate| {
            (|| -> anyhow::Result<_> {
                let family_id = public.family_id;
                if let std::collections::btree_map::Entry::Vacant(entry) = clients.entry(family_id)
                {
                    entry.insert(GnarkShieldedWithdrawalClient::load(family_id)?);
                }
                clients
                    .get(&family_id)
                    .expect("loaded prover family")
                    .prove(&public, &private)
            })()
            .map_err(|error| {
                ProofError::ProofGenerationFailed(format!("gnark shielded_withdrawal: {error}"))
            })
        },
    )
});

pub(super) fn prove_with_runtime(
    public: ShieldedWithdrawalProofPublic,
    private: ShieldedWithdrawalProofPrivate,
) -> Result<ShieldedWithdrawalProof, ProofError> {
    PROVER.prove(public, private)
}
