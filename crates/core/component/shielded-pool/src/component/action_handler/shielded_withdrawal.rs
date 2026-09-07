use anyhow::{Context, Result};
use cnidarium::StateRead;
use decaf377_rdsa::{Signature, SpendAuth};
use shieldd_sdk_asset::{balance, Value};
use shieldd_sdk_compliance::registry::ComplianceRegistryRead;
use shieldd_sdk_compliance::WithdrawalComplianceCiphertext;
use shieldd_sdk_sct::component::clock::EpochRead;
use shieldd_sdk_sct::component::source::SourceContext as _;
use shieldd_sdk_tct as tct;
use shieldd_sdk_txhash::{EffectHash, TransactionContext};

use crate::{
    component::action_handler::note_reshape, ShieldedIcs20WithdrawalChangeBody,
    ShieldedIcs20WithdrawalChangePublic, ShieldedIcs20WithdrawalFamilyId,
    ShieldedIcs20WithdrawalInputPublic, ShieldedIcs20WithdrawalProofPublic, TransferInputBody,
};

pub(crate) struct ProofPublicData<'a> {
    pub family_id: ShieldedIcs20WithdrawalFamilyId,
    pub balance_commitment: balance::Commitment,
    pub asset_anchor: tct::StateCommitment,
    pub compliance_anchor: tct::StateCommitment,
    pub target_timestamp: u64,
    pub inputs: &'a [TransferInputBody],
    pub change_output: &'a ShieldedIcs20WithdrawalChangeBody,
    pub outbound_value: Value,
    pub withdrawal_effect_hash: EffectHash,
    pub routing_tag: crate::discovery::RoutingTag,
    pub routing_parameter_set_id: decaf377::Fq,
    pub withdrawal_compliance_ciphertext: &'a WithdrawalComplianceCiphertext,
    pub volume_accumulator: &'a crate::VolumeAccumulatorPayload,
}

pub(crate) fn verify_auth_sigs(
    action_label: &str,
    inputs: &[TransferInputBody],
    auth_sigs: &[Signature<SpendAuth>],
    context: &TransactionContext,
) -> Result<()> {
    note_reshape::verify_auth_sigs(action_label, inputs, auth_sigs, context, |input| &input.rk)
}

pub(crate) fn extract_public(
    data: ProofPublicData<'_>,
    context: &TransactionContext,
) -> Result<ShieldedIcs20WithdrawalProofPublic> {
    let (inputs, change_outputs) = note_reshape::extract_public_parts(
        data.inputs,
        std::slice::from_ref(data.change_output),
        |input| (input.nullifier, &input.rk),
        |output| &output.note_payload,
    );
    let change_output = change_outputs
        .into_iter()
        .next()
        .expect("one change output was supplied");
    let public = ShieldedIcs20WithdrawalProofPublic {
        family_id: data.family_id,
        anchor: context.anchor,
        balance_commitment: data.balance_commitment,
        asset_anchor: data.asset_anchor,
        compliance_anchor: data.compliance_anchor,
        target_timestamp: decaf377::Fq::from(data.target_timestamp),
        inputs: inputs
            .into_iter()
            .zip(data.inputs.iter())
            .map(|(input, body_input)| ShieldedIcs20WithdrawalInputPublic {
                nullifier: input.nullifier,
                rk: input.rk,
                history_required: body_input.history_required,
            })
            .collect(),
        change_output: ShieldedIcs20WithdrawalChangePublic {
            note_commitment: change_output.note_commitment,
            recovery_commitment: data
                .change_output
                .note_payload
                .recovery_capsule
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("missing shielded withdrawal recovery capsule"))?
                .commitment(),
        },
        outbound_asset_id: data.outbound_value.asset_id.0,
        outbound_amount: decaf377::Fq::from(data.outbound_value.amount),
        withdrawal_effect_hash_limbs:
            crate::shielded_ics20_withdrawal::withdrawal_effect_hash_limbs(
                data.withdrawal_effect_hash.as_bytes(),
            ),
        routing_tag: data.routing_tag,
        routing_parameter_set_id: data.routing_parameter_set_id,
        withdrawal_compliance_ciphertext: data.withdrawal_compliance_ciphertext.clone(),
        recent_position_floor: context.recent_position_floor,
        volume_accumulator: crate::VolumeAccumulatorPublic {
            nullifier: data.volume_accumulator.nullifier,
            commitment: data.volume_accumulator.commitment,
            day_start: data.volume_accumulator.day_start,
        },
    };
    public.validate_shape()?;
    Ok(public)
}

pub(crate) async fn validate_compliance<S: StateRead>(
    state: &S,
    compliance_anchor: &tct::StateCommitment,
    asset_anchor: &tct::StateCommitment,
    target_timestamp: u64,
) -> Result<tendermint::Time> {
    state
        .validate_compliance_anchors(compliance_anchor, asset_anchor)
        .await
        .context("invalid compliance anchors")?;

    let block_time = state.get_current_block_timestamp().await?;
    let block_unix = block_time.unix_timestamp();
    anyhow::ensure!(block_unix >= 0, "block timestamp is negative");
    shieldd_sdk_compliance::registry::check_timestamp_freshness(target_timestamp, block_unix)?;

    Ok(block_time)
}

pub(crate) async fn validate_volume<S: StateRead>(
    state: &S,
    payload: &crate::VolumeAccumulatorPayload,
) -> Result<()> {
    use crate::component::StateReadExt as _;
    state
        .check_volume_nullifier_unspent(payload.day_start, payload.nullifier)
        .await
}

pub(crate) async fn execute_volume<S: cnidarium::StateWrite>(
    state: &mut S,
    payload: &crate::VolumeAccumulatorPayload,
) -> Result<()> {
    use crate::component::{NoteManager as _, StateWriteExt as _};
    let source = state
        .get_current_source()
        .ok_or_else(|| anyhow::anyhow!("source should be set during withdrawal execution"))?;
    state
        .record_volume_nullifier(payload.day_start, payload.nullifier)
        .await?;
    state
        .add_volume_accumulator_payload(payload.clone(), source.into())
        .await;
    Ok(())
}
