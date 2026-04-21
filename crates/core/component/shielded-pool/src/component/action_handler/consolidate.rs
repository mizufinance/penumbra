use anyhow::{Context, Result};
use async_trait::async_trait;
use cnidarium::StateWrite;
use cnidarium_component::ActionHandler;
use penumbra_sdk_proof_params::batch::{self, BatchItem};
use penumbra_sdk_txhash::TransactionContext;

use crate::{
    component::action_handler::note_reshape, Consolidate, ConsolidateInputBody,
    ConsolidateInputPublic, ConsolidateOutputBody, ConsolidateOutputPublic, ConsolidateProofPublic,
};

impl note_reshape::NoteReshapeInputBody for ConsolidateInputBody {
    fn nullifier(&self) -> penumbra_sdk_sct::Nullifier {
        self.nullifier
    }

    fn rk(&self) -> &decaf377_rdsa::VerificationKey<decaf377_rdsa::SpendAuth> {
        &self.rk
    }
}

impl note_reshape::NoteReshapeOutputBody for ConsolidateOutputBody {
    fn note_payload(&self) -> &crate::NotePayload {
        &self.note_payload
    }
}

pub fn consolidate_verify_auth_sigs(
    consolidate: &Consolidate,
    context: &TransactionContext,
) -> Result<()> {
    note_reshape::verify_auth_sigs(
        "consolidate",
        &consolidate.body.inputs,
        &consolidate.auth_sigs,
        context,
    )
}

pub fn consolidate_extract_public(
    consolidate: &Consolidate,
    context: &TransactionContext,
) -> Result<ConsolidateProofPublic> {
    let (inputs, outputs) =
        note_reshape::extract_public_parts(&consolidate.body.inputs, &consolidate.body.outputs);

    let public = ConsolidateProofPublic {
        family_id: consolidate.body.family_id,
        anchor: context.anchor,
        balance_commitment: consolidate.body.balance_commitment,
        inputs: inputs
            .into_iter()
            .map(|input| ConsolidateInputPublic {
                nullifier: input.nullifier,
                rk: input.rk,
            })
            .collect(),
        outputs: outputs
            .into_iter()
            .map(|output| ConsolidateOutputPublic {
                note_commitment: output.note_commitment,
            })
            .collect(),
    };
    public
        .validate_shape()
        .context("consolidate proof family shape mismatch")?;
    Ok(public)
}

pub fn consolidate_to_batch_item(
    consolidate: &Consolidate,
    public: ConsolidateProofPublic,
) -> Result<BatchItem> {
    consolidate.proof.to_batch_item(&public)
}

pub fn consolidate_check_stateless_and_extract(
    consolidate: &Consolidate,
    context: &TransactionContext,
) -> Result<BatchItem> {
    consolidate_verify_auth_sigs(consolidate, context)?;
    let public = consolidate_extract_public(consolidate, context)?;
    consolidate_to_batch_item(consolidate, public)
}

#[async_trait]
impl ActionHandler for Consolidate {
    type CheckStatelessContext = TransactionContext;

    async fn check_stateless(&self, context: TransactionContext) -> Result<()> {
        let item = consolidate_check_stateless_and_extract(self, &context)?;
        batch::batch_verify(
            self.body.family_id.proof_verification_key(),
            std::slice::from_ref(&item),
        )
        .map_err(|e| anyhow::anyhow!("consolidate proof did not verify: {e}"))?;
        Ok(())
    }

    async fn check_and_execute<S: StateWrite>(&self, mut state: S) -> Result<()> {
        note_reshape::execute(&mut state, &self.body.inputs, &self.body.outputs).await
    }
}
