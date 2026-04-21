use anyhow::{Context, Result};
use cnidarium::StateWrite;
use decaf377_rdsa::{Signature, SpendAuth, VerificationKey};
use penumbra_sdk_proto::{DomainType as _, StateWriteProto as _};
use penumbra_sdk_sct::component::{
    source::SourceContext,
    tree::{SctManager, VerificationExt},
};
use penumbra_sdk_sct::Nullifier;
use penumbra_sdk_txhash::TransactionContext;

use crate::{component::NoteManager, event, NotePayload};

pub(crate) trait NoteReshapeInputBody {
    fn nullifier(&self) -> Nullifier;
    fn rk(&self) -> &VerificationKey<SpendAuth>;
}

pub(crate) trait NoteReshapeOutputBody {
    fn note_payload(&self) -> &NotePayload;
}

pub(crate) struct NoteReshapeInputPublicParts {
    pub nullifier: Nullifier,
    pub rk: VerificationKey<SpendAuth>,
}

pub(crate) struct NoteReshapeOutputPublicParts {
    pub note_commitment: penumbra_sdk_tct::StateCommitment,
}

pub(crate) fn verify_auth_sigs<I>(
    action_label: &str,
    inputs: &[I],
    auth_sigs: &[Signature<SpendAuth>],
    context: &TransactionContext,
) -> Result<()>
where
    I: NoteReshapeInputBody,
{
    anyhow::ensure!(
        inputs.len() == auth_sigs.len(),
        "{action_label} expected {} auth sigs, got {}",
        inputs.len(),
        auth_sigs.len()
    );
    for (index, (input, auth_sig)) in inputs.iter().zip(auth_sigs.iter()).enumerate() {
        input
            .rk()
            .verify(context.effect_hash.as_ref(), auth_sig)
            .with_context(|| format!("{action_label} auth signature {index} failed to verify"))?;
    }
    Ok(())
}

pub(crate) fn extract_public_parts<I, O>(
    inputs: &[I],
    outputs: &[O],
) -> (
    Vec<NoteReshapeInputPublicParts>,
    Vec<NoteReshapeOutputPublicParts>,
)
where
    I: NoteReshapeInputBody,
    O: NoteReshapeOutputBody,
{
    let inputs = inputs
        .iter()
        .map(|input| NoteReshapeInputPublicParts {
            nullifier: input.nullifier(),
            rk: *input.rk(),
        })
        .collect();
    let outputs = outputs
        .iter()
        .map(|output| NoteReshapeOutputPublicParts {
            note_commitment: output.note_payload().note_commitment,
        })
        .collect();
    (inputs, outputs)
}

pub(crate) async fn execute<S, I, O>(state: &mut S, inputs: &[I], outputs: &[O]) -> Result<()>
where
    S: StateWrite,
    I: NoteReshapeInputBody,
    O: NoteReshapeOutputBody,
{
    for input in inputs {
        state.check_nullifier_unspent(input.nullifier()).await?;
    }

    let source = state
        .get_current_source()
        .ok_or_else(|| anyhow::anyhow!("source should be set during execution"))?;

    for input in inputs {
        let nullifier = input.nullifier();
        state.nullify(nullifier, source.into()).await;
        state.record_proto(event::EventNullifierSpent { nullifier }.to_proto());
    }
    for output in outputs {
        let note_payload = output.note_payload().clone();
        let note_commitment = note_payload.note_commitment;
        state.add_note_payload(note_payload, source.into()).await;
        state.record_proto(event::EventNoteCreated { note_commitment }.to_proto());
    }

    Ok(())
}
