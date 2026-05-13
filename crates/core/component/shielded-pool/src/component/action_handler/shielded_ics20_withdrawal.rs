use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use cnidarium::{StateRead, StateWrite};
use cnidarium_component::ActionHandler;
use penumbra_sdk_compliance::registry::ComplianceRegistryRead;
use penumbra_sdk_ibc::StateReadExt as _;
use penumbra_sdk_proof_params::batch::{self, BatchItem};
use penumbra_sdk_sct::component::clock::EpochRead;
use penumbra_sdk_txhash::{EffectingData, TransactionContext};

use crate::{
    component::{
        action_handler::note_reshape,
        transfer::{Ics20TransferExecutionExt as _, Ics20TransferWriteExt as _},
    },
    ShieldedIcs20Withdrawal, ShieldedIcs20WithdrawalChangeBody,
    ShieldedIcs20WithdrawalChangePublic, ShieldedIcs20WithdrawalInputPublic,
    ShieldedIcs20WithdrawalProofPublic, TransferInputBody,
};

pub fn shielded_ics20_withdrawal_verify_auth_sigs(
    action: &ShieldedIcs20Withdrawal,
    context: &TransactionContext,
) -> Result<()> {
    note_reshape::verify_auth_sigs(
        "shielded_ics20_withdrawal",
        &action.body.inputs,
        &action.auth_sigs,
        context,
        |input| &input.rk,
    )
}

pub fn shielded_ics20_withdrawal_extract_public(
    action: &ShieldedIcs20Withdrawal,
    context: &TransactionContext,
) -> Result<ShieldedIcs20WithdrawalProofPublic> {
    let effect_hash = action.body.withdrawal.effect_hash();
    let effect_hash_bytes = effect_hash.as_bytes();
    let public = ShieldedIcs20WithdrawalProofPublic {
        family_id: action.body.family_id,
        anchor: context.anchor,
        balance_commitment: action.body.balance_commitment,
        asset_anchor: action.body.asset_anchor,
        compliance_anchor: action.body.compliance_anchor,
        target_timestamp: decaf377::Fq::from(action.body.target_timestamp),
        inputs: action
            .body
            .inputs
            .iter()
            .map(|input| ShieldedIcs20WithdrawalInputPublic {
                nullifier: input.nullifier,
                rk: input.rk,
            })
            .collect(),
        change_output: ShieldedIcs20WithdrawalChangePublic {
            note_commitment: action.body.change_output.note_payload.note_commitment,
        },
        outbound_asset_id: action.body.withdrawal.denom.id().0,
        outbound_amount: decaf377::Fq::from(action.body.withdrawal.amount),
        withdrawal_effect_hash_lo: decaf377::Fq::from_le_bytes_mod_order(&effect_hash_bytes[..32]),
        withdrawal_effect_hash_hi: decaf377::Fq::from_le_bytes_mod_order(&effect_hash_bytes[32..]),
    };
    public
        .validate_shape()
        .context("shielded ICS-20 withdrawal proof family shape mismatch")?;
    Ok(public)
}

pub fn shielded_ics20_withdrawal_to_batch_item(
    action: &ShieldedIcs20Withdrawal,
    public: ShieldedIcs20WithdrawalProofPublic,
) -> Result<BatchItem> {
    action.proof.to_batch_item(&public)
}

pub fn shielded_ics20_withdrawal_check_stateless_and_extract(
    action: &ShieldedIcs20Withdrawal,
    context: &TransactionContext,
) -> Result<BatchItem> {
    action.body.validate_shape()?;
    action.body.withdrawal.validate()?;
    shielded_ics20_withdrawal_verify_auth_sigs(action, context)?;
    let public = shielded_ics20_withdrawal_extract_public(action, context)?;
    shielded_ics20_withdrawal_to_batch_item(action, public)
}

#[async_trait]
impl ActionHandler for ShieldedIcs20Withdrawal {
    type CheckStatelessContext = TransactionContext;

    async fn check_stateless(&self, context: TransactionContext) -> Result<()> {
        let item = shielded_ics20_withdrawal_check_stateless_and_extract(self, &context)?;
        batch::batch_verify(
            self.body.family_id.proof_verification_key(),
            std::slice::from_ref(&item),
        )
        .map_err(|e| anyhow::anyhow!("shielded ICS-20 withdrawal proof did not verify: {e}"))?;
        Ok(())
    }

    async fn check_historical<S: StateRead + 'static>(&self, state: Arc<S>) -> Result<()> {
        anyhow::ensure!(
            state
                .get_ibc_params()
                .await?
                .outbound_ics20_transfers_enabled,
            "transaction has a shielded ICS20 withdrawal, but outbound ICS20 withdrawals are not enabled"
        );
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

        let current_block_time = block_time;
        state
            .withdrawal_check_cached(&self.body.withdrawal, current_block_time)
            .await?;
        note_reshape::execute(
            &mut state,
            &self.body.inputs,
            std::slice::from_ref(&self.body.change_output),
            |input| input.nullifier,
            TransferInputBody::is_dummy,
            |output| &output.note_payload,
            ShieldedIcs20WithdrawalChangeBody::is_dummy,
        )
        .await?;
        state.withdrawal_execute(&self.body.withdrawal).await
    }
}
