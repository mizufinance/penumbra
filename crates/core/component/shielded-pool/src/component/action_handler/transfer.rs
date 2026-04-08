use anyhow::{Context, Result};
use async_trait::async_trait;
use cnidarium::StateWrite;
use cnidarium_component::ActionHandler;
use penumbra_sdk_compliance::registry::ComplianceRegistryRead;
use penumbra_sdk_compliance::structs::{
    ComplianceCiphertext, OUTPUT_DLEQ_BYTES, OUTPUT_WIRE_BYTES, SPEND_DLEQ_BYTES, SPEND_WIRE_BYTES,
};
use penumbra_sdk_proof_params::batch::{self, BatchItem};
use penumbra_sdk_proto::{DomainType as _, StateWriteProto as _};
use penumbra_sdk_sct::component::{
    clock::EpochRead,
    source::SourceContext,
    tree::{SctManager, VerificationExt},
};
use penumbra_sdk_txhash::TransactionContext;

use crate::{
    component::NoteManager, event, Transfer, TransferOutputPublic, TransferProofPublic,
    TransferSpendPublic,
};

fn parse_spend_ciphertext_fields(
    input: &crate::TransferInputBody,
) -> Result<(decaf377::Element, decaf377::Fq, Vec<decaf377::Fq>)> {
    anyhow::ensure!(
        input.compliance_ciphertext.len() == SPEND_WIRE_BYTES,
        "transfer spend compliance ciphertext must be {SPEND_WIRE_BYTES} bytes, got {}",
        input.compliance_ciphertext.len()
    );
    let ct = ComplianceCiphertext::from_bytes(&input.compliance_ciphertext)
        .context("failed to deserialize transfer spend compliance ciphertext")?;
    Ok(ct.to_spend_circuit_public_inputs())
}

fn parse_output_ciphertext_fields(
    output: &crate::TransferOutputBody,
) -> Result<(
    decaf377::Element,
    decaf377::Element,
    decaf377::Element,
    decaf377::Fq,
    decaf377::Fq,
    decaf377::Fq,
    Vec<decaf377::Fq>,
)> {
    anyhow::ensure!(
        output.compliance_ciphertext.len() == OUTPUT_WIRE_BYTES,
        "transfer output compliance ciphertext must be {OUTPUT_WIRE_BYTES} bytes, got {}",
        output.compliance_ciphertext.len()
    );
    let ct = ComplianceCiphertext::from_bytes(&output.compliance_ciphertext)
        .context("failed to deserialize transfer output compliance ciphertext")?;
    Ok(ct.to_output_circuit_public_inputs())
}

fn parse_spend_dleq_fields(
    input: &crate::TransferInputBody,
    target_timestamp: u64,
) -> Result<(decaf377::Fq, decaf377::Fq, decaf377::Fq)> {
    anyhow::ensure!(
        input.dleq_proof.len() == SPEND_DLEQ_BYTES,
        "transfer spend dleq_proof must be {SPEND_DLEQ_BYTES} bytes, got {}",
        input.dleq_proof.len()
    );
    let c_bytes: [u8; 32] = input.dleq_proof[..32]
        .try_into()
        .context("transfer spend dleq_c must be 32 bytes")?;
    let s_bytes: [u8; 32] = input.dleq_proof[32..64]
        .try_into()
        .context("transfer spend dleq_s must be 32 bytes")?;
    Ok((
        decaf377::Fq::from(target_timestamp),
        decaf377::Fq::from_bytes_checked(&c_bytes)
            .map_err(|_| anyhow::anyhow!("invalid transfer spend dleq_c field element"))?,
        decaf377::Fq::from_bytes_checked(&s_bytes)
            .map_err(|_| anyhow::anyhow!("invalid transfer spend dleq_s field element"))?,
    ))
}

fn parse_output_dleq_fields(
    output: &crate::TransferOutputBody,
) -> Result<(
    decaf377::Fq,
    decaf377::Fq,
    decaf377::Fq,
    decaf377::Fq,
    decaf377::Fq,
    decaf377::Fq,
)> {
    anyhow::ensure!(
        output.dleq_proofs.len() == OUTPUT_DLEQ_BYTES,
        "transfer output dleq_proofs must be {OUTPUT_DLEQ_BYTES} bytes, got {}",
        output.dleq_proofs.len()
    );
    let parse = |offset: usize| -> anyhow::Result<decaf377::Fq> {
        let bytes: [u8; 32] = output.dleq_proofs[offset..offset + 32]
            .try_into()
            .context("transfer output dleq field must be 32 bytes")?;
        decaf377::Fq::from_bytes_checked(&bytes)
            .map_err(|_| anyhow::anyhow!("invalid transfer output dleq field element"))
    };
    Ok((
        parse(0)?,
        parse(32)?,
        parse(64)?,
        parse(96)?,
        parse(128)?,
        parse(160)?,
    ))
}

pub fn transfer_verify_auth_sigs(transfer: &Transfer, context: &TransactionContext) -> Result<()> {
    anyhow::ensure!(
        transfer.body.inputs.len() == transfer.auth_sigs.len(),
        "transfer expected {} auth sigs, got {}",
        transfer.body.inputs.len(),
        transfer.auth_sigs.len()
    );
    for (index, (input, auth_sig)) in transfer
        .body
        .inputs
        .iter()
        .zip(transfer.auth_sigs.iter())
        .enumerate()
    {
        input
            .rk
            .verify(context.effect_hash.as_ref(), auth_sig)
            .with_context(|| format!("transfer auth signature {index} failed to verify"))?;
    }
    Ok(())
}

fn transfer_check_lengths(transfer: &Transfer) -> Result<()> {
    for input in &transfer.body.inputs {
        anyhow::ensure!(
            input.compliance_ciphertext.len() == SPEND_WIRE_BYTES,
            "transfer spend compliance ciphertext must be {SPEND_WIRE_BYTES} bytes, got {}",
            input.compliance_ciphertext.len()
        );
        anyhow::ensure!(
            input.dleq_proof.len() == SPEND_DLEQ_BYTES,
            "transfer spend dleq_proof must be {SPEND_DLEQ_BYTES} bytes, got {}",
            input.dleq_proof.len()
        );
    }
    for output in &transfer.body.outputs {
        anyhow::ensure!(
            output.compliance_ciphertext.len() == OUTPUT_WIRE_BYTES,
            "transfer output compliance ciphertext must be {OUTPUT_WIRE_BYTES} bytes, got {}",
            output.compliance_ciphertext.len()
        );
        anyhow::ensure!(
            output.dleq_proofs.len() == OUTPUT_DLEQ_BYTES,
            "transfer output dleq_proofs must be {OUTPUT_DLEQ_BYTES} bytes, got {}",
            output.dleq_proofs.len()
        );
    }
    Ok(())
}

pub fn transfer_extract_public(
    transfer: &Transfer,
    context: &TransactionContext,
) -> Result<TransferProofPublic> {
    let inputs = transfer
        .body
        .inputs
        .iter()
        .map(|input| {
            let (epk, c2_core, compliance_ciphertext) = parse_spend_ciphertext_fields(input)?;
            let (_, dleq_c, dleq_s) =
                parse_spend_dleq_fields(input, transfer.body.target_timestamp)?;
            Ok(TransferSpendPublic {
                nullifier: input.nullifier,
                rk: input.rk,
                epk,
                c2_core,
                compliance_ciphertext,
                dleq_c,
                dleq_s,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let outputs = transfer
        .body
        .outputs
        .iter()
        .map(|output| {
            let (epk_1, epk_2, epk_3, c2_core, c2_ext, c2_sext, compliance_ciphertext) =
                parse_output_ciphertext_fields(output)?;
            let (dleq_c_1, dleq_s_1, dleq_c_2, dleq_s_2, dleq_c_3, dleq_s_3) =
                parse_output_dleq_fields(output)?;
            Ok(TransferOutputPublic {
                note_commitment: output.note_payload.note_commitment,
                epk_1,
                epk_2,
                epk_3,
                c2_core,
                c2_ext,
                c2_sext,
                compliance_ciphertext,
                dleq_c_1,
                dleq_s_1,
                dleq_c_2,
                dleq_s_2,
                dleq_c_3,
                dleq_s_3,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let public = TransferProofPublic {
        family_id: transfer.body.family_id,
        anchor: context.anchor,
        balance_commitment: transfer.body.balance_commitment,
        asset_anchor: transfer.body.asset_anchor,
        compliance_anchor: transfer.body.compliance_anchor,
        target_timestamp: decaf377::Fq::from(transfer.body.target_timestamp),
        inputs,
        outputs,
    };
    public
        .validate_shape()
        .context("transfer proof family shape mismatch")?;
    Ok(public)
}

pub fn transfer_to_batch_item(
    transfer: &Transfer,
    public: TransferProofPublic,
) -> Result<BatchItem> {
    transfer.proof.to_batch_item(&public)
}

pub fn transfer_check_stateless_and_extract(
    transfer: &Transfer,
    context: &TransactionContext,
) -> Result<BatchItem> {
    transfer_verify_auth_sigs(transfer, context)?;
    transfer_check_lengths(transfer)?;
    let public = transfer_extract_public(transfer, context)?;
    transfer_to_batch_item(transfer, public)
}

#[async_trait]
impl ActionHandler for Transfer {
    type CheckStatelessContext = TransactionContext;

    async fn check_stateless(&self, context: TransactionContext) -> Result<()> {
        let item = transfer_check_stateless_and_extract(self, &context)?;
        batch::batch_verify(
            self.body.family_id.proof_verification_key(),
            std::slice::from_ref(&item),
        )
        .map_err(|e| anyhow::anyhow!("transfer proof did not verify: {e}"))?;
        Ok(())
    }

    async fn check_and_execute<S: StateWrite>(&self, mut state: S) -> Result<()> {
        state
            .validate_compliance_anchors(&self.body.compliance_anchor, &self.body.asset_anchor)
            .await
            .context("invalid compliance anchors")?;

        let block_time = state.get_current_block_timestamp().await?;
        let block_unix = block_time.unix_timestamp();
        anyhow::ensure!(block_unix >= 0, "block timestamp is negative");
        let block_timestamp = block_unix as u64;
        penumbra_sdk_compliance::registry::check_timestamp_freshness(
            self.body.target_timestamp,
            block_timestamp,
        )?;

        for input in &self.body.inputs {
            state.check_nullifier_unspent(input.nullifier).await?;
        }

        let source = state
            .get_current_source()
            .ok_or_else(|| anyhow::anyhow!("source should be set during execution"))?;

        for input in &self.body.inputs {
            state.nullify(input.nullifier, source.into()).await;
            state.record_proto(
                event::EventSpend {
                    nullifier: input.nullifier,
                }
                .to_proto(),
            );
        }
        for output in &self.body.outputs {
            state
                .add_note_payload(output.note_payload.clone(), source.into())
                .await;
            state.record_proto(
                event::EventOutput {
                    note_commitment: output.note_payload.note_commitment,
                }
                .to_proto(),
            );
        }

        Ok(())
    }
}
