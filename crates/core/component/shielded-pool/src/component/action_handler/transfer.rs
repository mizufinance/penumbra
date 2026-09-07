use anyhow::{Context, Result};
use async_trait::async_trait;
use cnidarium::StateWrite;
use cnidarium_component::ActionHandler;
use shieldd_sdk_compliance::registry::ComplianceRegistryRead;
use shieldd_sdk_proof_params::batch::{self, BatchItem, VerifiedBatchItem};
use shieldd_sdk_sct::component::clock::EpochRead;
use shieldd_sdk_sct::component::source::SourceContext as _;
use shieldd_sdk_txhash::TransactionContext;

use crate::transfer::compliance::{
    parse_transfer_output_compliance, transfer_compliance_public_from_parts,
};
use crate::{
    component::action_handler::note_reshape,
    component::{NoteManager as _, StateReadExt as _, StateWriteExt as _},
    Transfer, TransferOutputPublic, TransferProofContext, TransferProofPublic, TransferSpendPublic,
    VolumeAccumulatorPublic,
};

fn transfer_verify_auth_sigs(transfer: &Transfer, context: &TransactionContext) -> Result<()> {
    note_reshape::verify_auth_sigs(
        "transfer",
        &transfer.body.inputs,
        &transfer.auth_sigs,
        context,
        |input| &input.rk,
    )
}

fn transfer_check_lengths(transfer: &Transfer) -> Result<()> {
    for (index, input) in transfer.body.inputs.iter().enumerate() {
        anyhow::ensure!(
            input.compliance_ciphertext.is_empty(),
            "transfer input {} compliance ciphertext must be empty",
            index + 1
        );
    }
    let _ = parse_transfer_output_compliance(&transfer.body.outputs)?;
    Ok(())
}

pub(crate) fn transfer_extract_public(
    transfer: &Transfer,
    context: &TransactionContext,
) -> Result<TransferProofPublic> {
    let inputs = transfer
        .body
        .inputs
        .iter()
        .map(|input| {
            Ok(TransferSpendPublic {
                nullifier: input.nullifier,
                rk: input.rk,
                history_required: input.history_required,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let (ciphertext, metadata) = parse_transfer_output_compliance(&transfer.body.outputs)?;
    let outputs = transfer
        .body
        .outputs
        .iter()
        .map(|output| {
            Ok(TransferOutputPublic {
                note_commitment: output.note_payload.note_commitment,
                recovery_commitment: output
                    .note_payload
                    .recovery_capsule
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("missing transfer recovery capsule"))?
                    .commitment(),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let public = TransferProofPublic {
        anchor: context.anchor,
        balance_commitment: transfer.body.balance_commitment,
        asset_anchor: transfer.body.asset_anchor,
        compliance_anchor: transfer.body.compliance_anchor,
        target_timestamp: decaf377::Fq::from(transfer.body.target_timestamp),
        inputs,
        outputs,
        compliance: transfer_compliance_public_from_parts(&ciphertext, &metadata)?,
        routing: transfer.body.routing,
        routing_parameter_set_id: transfer.body.routing_parameter_set_id,
        recent_position_floor: context.recent_position_floor,
        volume_accumulator: VolumeAccumulatorPublic {
            nullifier: transfer.body.volume_accumulator.nullifier,
            commitment: transfer.body.volume_accumulator.commitment,
            day_start: transfer.body.volume_accumulator.day_start,
        },
        proof_context: transfer.body.proof_context,
    };
    public
        .validate_shape()
        .context("transfer proof shape mismatch")?;
    Ok(public)
}

fn transfer_to_batch_item(transfer: &Transfer, public: TransferProofPublic) -> Result<BatchItem> {
    transfer.proof.to_batch_item(&public)
}

pub fn transfer_check_stateless_and_extract(
    transfer: &Transfer,
    context: &TransactionContext,
    expected_context: TransferProofContext,
) -> Result<BatchItem> {
    note_reshape::validate_action_anchor("transfer", transfer.body.anchor, context)?;
    transfer
        .body
        .validate_shape()
        .context("transfer body shape mismatch")?;
    anyhow::ensure!(
        transfer.body.proof_context == expected_context,
        "transfer proof context does not match its transaction location"
    );
    transfer_verify_auth_sigs(transfer, context)?;
    transfer_check_lengths(transfer)?;
    let public = transfer_extract_public(transfer, context)?;
    transfer_to_batch_item(transfer, public)
}

/// Evidence that an exact verified Transfer passed its state preconditions.
pub struct ValidatedTransferExecution {
    item: BatchItem,
    proof_context: TransferProofContext,
}

/// Validate an exact verified Transfer against the current state.
pub async fn transfer_validate_verified<S: StateWrite>(
    transfer: &Transfer,
    context: &TransactionContext,
    verified_proof: &VerifiedBatchItem,
    expected_context: TransferProofContext,
    state: S,
) -> Result<ValidatedTransferExecution> {
    let item = transfer_check_stateless_and_extract(transfer, context, expected_context)?;
    verified_proof
        .ensure_binds(shieldd_sdk_proof_params::DeployedProofKey::Transfer, &item)
        .context("transfer verified proof capability mismatch")?;

    state
        .validate_compliance_anchors(
            &transfer.body.compliance_anchor,
            &transfer.body.asset_anchor,
        )
        .await
        .context("invalid compliance anchors")?;

    let block_time = state.get_current_block_timestamp().await?;
    shieldd_sdk_compliance::registry::check_timestamp_freshness(
        transfer.body.target_timestamp,
        block_time.unix_timestamp(),
    )?;
    if expected_context == TransferProofContext::Ordinary {
        state
            .check_volume_nullifier_unspent(
                transfer.body.volume_accumulator.day_start,
                transfer.body.volume_accumulator.nullifier,
            )
            .await?;
    }

    Ok(ValidatedTransferExecution {
        item,
        proof_context: expected_context,
    })
}

/// Apply proof-bound effects after exact Transfer preconditions were validated.
pub async fn transfer_execute_validated<S: StateWrite>(
    transfer: &Transfer,
    context: &TransactionContext,
    validated: ValidatedTransferExecution,
    mut state: S,
) -> Result<()> {
    let item = transfer_check_stateless_and_extract(transfer, context, validated.proof_context)?;
    anyhow::ensure!(
        validated.item == item,
        "validated transfer capability does not bind this exact action"
    );

    note_reshape::execute_proof_bound_effects(
        &mut state,
        &transfer.body.inputs,
        &transfer.body.outputs,
        |input| input.nullifier,
        |output| &output.note_payload,
    )
    .await?;

    if validated.proof_context == TransferProofContext::Ordinary {
        let source = state
            .get_current_source()
            .ok_or_else(|| anyhow::anyhow!("source should be set during execution"))?;
        state
            .record_volume_nullifier(
                transfer.body.volume_accumulator.day_start,
                transfer.body.volume_accumulator.nullifier,
            )
            .await?;
        state
            .add_volume_accumulator_payload(transfer.body.volume_accumulator.clone(), source.into())
            .await;
    }
    Ok(())
}

/// Execute a Transfer whose exact proof item has already verified.
pub async fn transfer_execute_verified<S: StateWrite>(
    transfer: &Transfer,
    context: &TransactionContext,
    verified_proof: &VerifiedBatchItem,
    expected_context: TransferProofContext,
    mut state: S,
) -> Result<()> {
    let validated = transfer_validate_verified(
        transfer,
        context,
        verified_proof,
        expected_context,
        &mut state,
    )
    .await?;
    transfer_execute_validated(transfer, context, validated, &mut state).await
}

#[async_trait]
impl ActionHandler for Transfer {
    type CheckStatelessContext = TransactionContext;

    async fn check_stateless(&self, context: TransactionContext) -> Result<()> {
        let item =
            transfer_check_stateless_and_extract(self, &context, TransferProofContext::Ordinary)?;
        batch::verify_each(
            shieldd_sdk_proof_params::transfer_proof_verification_key(),
            std::slice::from_ref(&item),
        )
        .map_err(|e| anyhow::anyhow!("transfer proof did not verify: {e}"))?;
        Ok(())
    }

    async fn check_and_execute<S: StateWrite>(&self, _state: S) -> Result<()> {
        anyhow::bail!("transfer execution requires an exact verified proof capability")
    }
}

#[cfg(test)]
mod tests {
    use decaf377::Fr;
    use decaf377_rdsa::{SigningKey, SpendAuth, VerificationKey};
    use rand_core::OsRng;
    use shieldd_sdk_sct::component::tree::SctRead as _;

    use super::*;
    use crate::test_proof_helpers::proof_test_helpers::build_transfer_action_and_public_without_proof;

    #[test]
    fn auth_verification_rejects_invalid_fixed_slot_signature() {
        let (mut transfer, _, context) = build_transfer_action_and_public_without_proof(true);
        transfer_verify_auth_sigs(&transfer, &context)
            .expect("fixture signatures must authenticate every fixed slot");

        let wrong_key = SigningKey::<SpendAuth>::from(Fr::from(999u64));
        transfer.auth_sigs[1] = wrong_key.sign(&mut OsRng, context.effect_hash.as_ref());
        let error = transfer_verify_auth_sigs(&transfer, &context)
            .expect_err("an unrelated fixed-slot signature must be rejected");
        assert!(
            error
                .to_string()
                .contains("transfer auth signature 1 failed"),
            "unexpected rejection reason: {error:#}"
        );
    }

    #[test]
    fn transfer_auth_verification_rejects_identity_randomized_key() {
        let (mut transfer, _, context) = build_transfer_action_and_public_without_proof(true);
        let identity_sk = SigningKey::<SpendAuth>::from(Fr::from(0u64));
        transfer.body.inputs[0].rk = VerificationKey::from(identity_sk.clone());
        let different_message = b"different transfer authorization hash";
        assert_ne!(&different_message[..], context.effect_hash.as_ref());
        transfer.auth_sigs[0] = identity_sk.sign_deterministic(different_message);

        transfer.body.inputs[0]
            .rk
            .verify(context.effect_hash.as_ref(), &transfer.auth_sigs[0])
            .expect("the pinned RDSA primitive admits identity keys across messages");
        let error = transfer_verify_auth_sigs(&transfer, &context)
            .expect_err("Transfer must reject an identity randomized spend key");
        assert!(
            error
                .to_string()
                .contains("randomized spend key 0 must not be identity"),
            "unexpected rejection reason: {error:#}"
        );
    }

    #[test]
    fn stateless_rejects_nonempty_input_compliance_ciphertext() {
        let (mut transfer, _, _) = build_transfer_action_and_public_without_proof(true);
        transfer.body.inputs[0].compliance_ciphertext.push(1);

        let error = transfer_check_lengths(&transfer)
            .expect_err("input compliance bytes are not part of the proof statement");
        assert!(
            error
                .to_string()
                .contains("transfer input 1 compliance ciphertext must be empty"),
            "unexpected rejection reason: {error:#}"
        );
    }

    #[tokio::test]
    async fn raw_transfer_execution_fails_closed_without_verified_capability() {
        let (transfer, _, _) = build_transfer_action_and_public_without_proof(true);
        let storage = cnidarium::TempStorage::new()
            .await
            .expect("temporary storage");
        let mut state = cnidarium::StateDelta::new(storage.latest_snapshot());

        let error = transfer
            .check_and_execute(&mut state)
            .await
            .expect_err("raw Transfer execution must not mutate state");
        assert!(
            error
                .to_string()
                .contains("requires an exact verified proof capability"),
            "unexpected rejection reason: {error:#}"
        );
        assert!(state.pending_nullifiers().is_empty());
        assert!(state.pending_note_payloads().is_empty());
    }
}
