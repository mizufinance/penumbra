#![cfg(all(feature = "prover", any(unix, windows)))]

use crate::ShieldedIcs20WithdrawalFamilyId;
use crate::{
    gnark::{prover_worker::ProverWorker, GnarkShieldedIcs20WithdrawalClient},
    shielded_ics20_withdrawal::{
        ShieldedIcs20WithdrawalProof, ShieldedIcs20WithdrawalProofPrivate,
        ShieldedIcs20WithdrawalProofPublic,
    },
    ProofError,
};
use std::collections::BTreeMap;
use std::sync::LazyLock;

static PROVER: LazyLock<
    ProverWorker<
        ShieldedIcs20WithdrawalProofPublic,
        ShieldedIcs20WithdrawalProofPrivate,
        ShieldedIcs20WithdrawalProof,
    >,
> = LazyLock::new(|| {
    ProverWorker::spawn(
        "shielded_ics20_withdrawal-prover",
        BTreeMap::<ShieldedIcs20WithdrawalFamilyId, GnarkShieldedIcs20WithdrawalClient>::new,
        |clients,
         public: ShieldedIcs20WithdrawalProofPublic,
         private: ShieldedIcs20WithdrawalProofPrivate| {
            (|| -> anyhow::Result<_> {
                let family_id = public.family_id;
                if let std::collections::btree_map::Entry::Vacant(entry) = clients.entry(family_id)
                {
                    entry.insert(GnarkShieldedIcs20WithdrawalClient::load(family_id)?);
                }
                clients
                    .get(&family_id)
                    .expect("loaded prover family")
                    .prove(&public, &private)
            })()
            .map_err(|error| {
                ProofError::ProofGenerationFailed(format!(
                    "gnark shielded_ics20_withdrawal: {error}"
                ))
            })
        },
    )
});

pub(super) fn prove_with_runtime(
    public: ShieldedIcs20WithdrawalProofPublic,
    private: ShieldedIcs20WithdrawalProofPrivate,
) -> Result<ShieldedIcs20WithdrawalProof, ProofError> {
    PROVER.prove(public, private)
}
