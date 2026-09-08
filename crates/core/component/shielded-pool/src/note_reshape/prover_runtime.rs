#![cfg(all(feature = "prover", any(unix, windows)))]

use crate::NoteReshapeFamilyId;
use crate::{
    gnark::{prover_worker::ProverWorker, GnarkNoteReshapeClient},
    note_reshape::{NoteReshapeProof, NoteReshapeProofPrivate, NoteReshapeProofPublic},
    ProofError,
};
use std::collections::BTreeMap;
use std::sync::LazyLock;

static PROVER: LazyLock<
    ProverWorker<NoteReshapeProofPublic, NoteReshapeProofPrivate, NoteReshapeProof>,
> = LazyLock::new(|| {
    ProverWorker::spawn(
        "note_reshape-prover",
        BTreeMap::<NoteReshapeFamilyId, GnarkNoteReshapeClient>::new,
        |clients, public: NoteReshapeProofPublic, private: NoteReshapeProofPrivate| {
            (|| -> anyhow::Result<_> {
                let family_id = public.family_id;
                if let std::collections::btree_map::Entry::Vacant(entry) = clients.entry(family_id)
                {
                    entry.insert(GnarkNoteReshapeClient::load(family_id)?);
                }
                clients
                    .get(&family_id)
                    .expect("loaded prover family")
                    .prove(&public, &private)
            })()
            .map_err(|error| {
                ProofError::ProofGenerationFailed(format!("gnark note_reshape: {error}"))
            })
        },
    )
});

pub(super) fn prove_with_runtime(
    public: NoteReshapeProofPublic,
    private: NoteReshapeProofPrivate,
) -> Result<NoteReshapeProof, ProofError> {
    PROVER.prove(public, private)
}
