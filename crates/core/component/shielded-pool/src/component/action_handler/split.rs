use anyhow::{Context, Result};
use async_trait::async_trait;
use cnidarium::StateWrite;
use cnidarium_component::ActionHandler;
use penumbra_sdk_proof_params::batch::{self, BatchItem};
use penumbra_sdk_txhash::TransactionContext;

use crate::{
    component::action_handler::note_reshape, Split, SplitInputBody, SplitInputPublic,
    SplitOutputBody, SplitOutputPublic, SplitProofPublic,
};

pub fn split_verify_auth_sigs(split: &Split, context: &TransactionContext) -> Result<()> {
    note_reshape::verify_auth_sigs(
        "split",
        &split.body.inputs,
        &split.auth_sigs,
        context,
        |input| &input.rk,
    )
}

pub fn split_extract_public(
    split: &Split,
    context: &TransactionContext,
) -> Result<SplitProofPublic> {
    let (inputs, outputs) = note_reshape::extract_public_parts(
        &split.body.inputs,
        &split.body.outputs,
        |input| (input.nullifier, &input.rk),
        |output| &output.note_payload,
    );

    let public = SplitProofPublic {
        family_id: split.body.family_id,
        anchor: context.anchor,
        balance_commitment: split.body.balance_commitment,
        inputs: inputs
            .into_iter()
            .map(|input| SplitInputPublic {
                nullifier: input.nullifier,
                rk: input.rk,
            })
            .collect(),
        outputs: outputs
            .into_iter()
            .map(|output| SplitOutputPublic {
                note_commitment: output.note_commitment,
            })
            .collect(),
    };
    public
        .validate_shape()
        .context("split proof family shape mismatch")?;
    Ok(public)
}

pub fn split_to_batch_item(split: &Split, public: SplitProofPublic) -> Result<BatchItem> {
    split.proof.to_batch_item(&public)
}

pub fn split_check_stateless_and_extract(
    split: &Split,
    context: &TransactionContext,
) -> Result<BatchItem> {
    split_verify_auth_sigs(split, context)?;
    let public = split_extract_public(split, context)?;
    split_to_batch_item(split, public)
}

#[async_trait]
impl ActionHandler for Split {
    type CheckStatelessContext = TransactionContext;

    async fn check_stateless(&self, context: TransactionContext) -> Result<()> {
        let item = split_check_stateless_and_extract(self, &context)?;
        batch::batch_verify(
            self.body.family_id.proof_verification_key(),
            std::slice::from_ref(&item),
        )
        .map_err(|e| anyhow::anyhow!("split proof did not verify: {e}"))?;
        Ok(())
    }

    async fn check_and_execute<S: StateWrite>(&self, mut state: S) -> Result<()> {
        note_reshape::execute(
            &mut state,
            &self.body.inputs,
            &self.body.outputs,
            |input| input.nullifier,
            SplitInputBody::is_dummy,
            |output| &output.note_payload,
            SplitOutputBody::is_dummy,
        )
        .await
    }
}
