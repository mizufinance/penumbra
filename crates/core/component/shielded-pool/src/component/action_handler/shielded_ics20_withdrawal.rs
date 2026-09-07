use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use cnidarium::{StateRead, StateWrite};
use cnidarium_component::ActionHandler;
use shieldd_sdk_compliance::registry::ComplianceRegistryRead;
use shieldd_sdk_ibc::StateReadExt as _;
use shieldd_sdk_proof_params::batch::{self, BatchItem, VerifiedBatchItem};
use shieldd_sdk_sct::component::clock::EpochRead;
use shieldd_sdk_txhash::{EffectingData, TransactionContext};

use crate::{
    component::{
        action_handler::{note_reshape, shielded_withdrawal},
        transfer::{withdrawal_check, withdrawal_execute},
    },
    ShieldedIcs20Withdrawal, ShieldedIcs20WithdrawalChangePublic,
    ShieldedIcs20WithdrawalInputPublic, ShieldedIcs20WithdrawalProofPublic,
};

fn shielded_ics20_withdrawal_verify_auth_sigs(
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

fn shielded_ics20_withdrawal_check_lengths(action: &ShieldedIcs20Withdrawal) -> Result<()> {
    for (index, input) in action.body.inputs.iter().enumerate() {
        anyhow::ensure!(
            input.compliance_ciphertext.is_empty(),
            "shielded ICS-20 withdrawal input {} compliance ciphertext must be empty",
            index + 1
        );
    }
    Ok(())
}

async fn ensure_withdrawals_enabled<S: StateRead + ?Sized>(state: &S) -> Result<()> {
    anyhow::ensure!(
        state
            .get_ibc_params()
            .await?
            .outbound_ics20_transfers_enabled,
        "transaction has a shielded ICS20 withdrawal, but outbound ICS20 withdrawals are not enabled"
    );
    Ok(())
}

fn shielded_ics20_withdrawal_extract_public(
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
                history_required: input.history_required,
            })
            .collect(),
        change_output: ShieldedIcs20WithdrawalChangePublic {
            note_commitment: action.body.change_output.note_payload.note_commitment,
            recovery_commitment: action
                .body
                .change_output
                .note_payload
                .recovery_capsule
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("missing shielded withdrawal recovery capsule"))?
                .commitment(),
        },
        outbound_asset_id: action.body.withdrawal.denom.id().0,
        outbound_amount: decaf377::Fq::from(action.body.withdrawal.amount),
        withdrawal_effect_hash_limbs:
            crate::shielded_ics20_withdrawal::withdrawal_effect_hash_limbs(effect_hash_bytes),
        routing_tag: action.body.routing_tag,
        routing_parameter_set_id: action.body.routing_parameter_set_id,
        withdrawal_compliance_ciphertext: action.body.withdrawal_compliance_ciphertext.clone(),
        recent_position_floor: context.recent_position_floor,
        volume_accumulator: crate::VolumeAccumulatorPublic {
            nullifier: action.body.volume_accumulator.nullifier,
            commitment: action.body.volume_accumulator.commitment,
            day_start: action.body.volume_accumulator.day_start,
        },
    };
    public
        .validate_shape()
        .context("shielded ICS-20 withdrawal proof family shape mismatch")?;
    Ok(public)
}

fn shielded_ics20_withdrawal_to_batch_item(
    action: &ShieldedIcs20Withdrawal,
    public: ShieldedIcs20WithdrawalProofPublic,
) -> Result<BatchItem> {
    action.proof.to_batch_item(&public)
}

pub fn shielded_ics20_withdrawal_check_stateless_and_extract(
    action: &ShieldedIcs20Withdrawal,
    context: &TransactionContext,
) -> Result<BatchItem> {
    note_reshape::validate_action_anchor("shielded_ics20_withdrawal", action.body.anchor, context)?;
    action.body.validate_shape()?;
    action.body.withdrawal.validate()?;
    shielded_ics20_withdrawal_check_lengths(action)?;
    shielded_ics20_withdrawal_verify_auth_sigs(action, context)?;
    let public = shielded_ics20_withdrawal_extract_public(action, context)?;
    shielded_ics20_withdrawal_to_batch_item(action, public)
}

/// Execute a Withdrawal whose exact proof item has already verified.
pub async fn shielded_ics20_withdrawal_execute_verified<S: StateWrite>(
    action: &ShieldedIcs20Withdrawal,
    context: &TransactionContext,
    verified_proof: &VerifiedBatchItem,
    mut state: S,
) -> Result<()> {
    let item = shielded_ics20_withdrawal_check_stateless_and_extract(action, context)?;
    verified_proof
        .ensure_binds(action.body.family_id.deployed_proof_key(), &item)
        .context("shielded ICS-20 withdrawal verified proof capability mismatch")?;

    ensure_withdrawals_enabled(&state).await?;

    state
        .validate_compliance_anchors(&action.body.compliance_anchor, &action.body.asset_anchor)
        .await
        .context("invalid compliance anchors")?;

    let block_time = state.get_current_block_timestamp().await?;
    shieldd_sdk_compliance::registry::check_timestamp_freshness(
        action.body.target_timestamp,
        block_time.unix_timestamp(),
    )?;

    let current_block_time = block_time;
    shielded_withdrawal::validate_volume(&state, &action.body.volume_accumulator).await?;
    let checked = withdrawal_check(&mut state, &action.body.withdrawal, current_block_time).await?;
    note_reshape::execute_proof_bound_effects(
        &mut state,
        &action.body.inputs,
        std::slice::from_ref(&action.body.change_output),
        |input| input.nullifier,
        |output| &output.note_payload,
    )
    .await?;
    shielded_withdrawal::execute_volume(&mut state, &action.body.volume_accumulator).await?;
    withdrawal_execute(&mut state, checked).await
}

#[async_trait]
impl ActionHandler for ShieldedIcs20Withdrawal {
    type CheckStatelessContext = TransactionContext;

    async fn check_stateless(&self, context: TransactionContext) -> Result<()> {
        let item = shielded_ics20_withdrawal_check_stateless_and_extract(self, &context)?;
        batch::verify_each(
            self.body.family_id.proof_verification_key(),
            std::slice::from_ref(&item),
        )
        .map_err(|e| anyhow::anyhow!("shielded ICS-20 withdrawal proof did not verify: {e}"))?;
        Ok(())
    }

    async fn check_historical<S: StateRead + 'static>(&self, state: Arc<S>) -> Result<()> {
        ensure_withdrawals_enabled(state.as_ref()).await
    }

    async fn check_and_execute<S: StateWrite>(&self, _state: S) -> Result<()> {
        anyhow::bail!(
            "shielded ICS-20 withdrawal execution requires an exact verified proof capability"
        )
    }
}

#[cfg(test)]
mod tests {
    use std::{ops::Deref, str::FromStr};

    use decaf377::Fr;
    use decaf377_rdsa::{SigningKey, SpendAuth, VerificationKey};
    use ibc_types::core::{channel::ChannelId, client::Height as IbcHeight};
    use rand_core::OsRng;
    use shieldd_sdk_asset::{Value, BASE_ASSET_DENOM};
    use shieldd_sdk_ibc::{params::IBCParameters, StateWriteExt as _};
    use shieldd_sdk_keys::test_keys;
    use shieldd_sdk_sct::component::tree::SctRead as _;

    use super::*;
    use crate::{
        component::NoteManager as _,
        note_reshape_padding::{dummy_spend_auth_sig, dummy_state_commitment_proof},
        Ics20Withdrawal, Note, ShieldedIcs20WithdrawalFamilyId, ShieldedInputPlan,
    };

    fn test_action() -> ShieldedIcs20Withdrawal {
        let value = Value {
            amount: 40u64.into(),
            asset_id: BASE_ASSET_DENOM.id(),
        };
        let note = Note::generate(&mut OsRng, &test_keys::ADDRESS_0, value);
        let spend = ShieldedInputPlan::new(&mut OsRng, note, 0u64.into());
        let withdrawal = Ics20Withdrawal {
            amount: 40u64.into(),
            denom: BASE_ASSET_DENOM.clone(),
            destination_chain_address: "cosmos1destination".to_string(),
            return_address: test_keys::ADDRESS_0.deref().clone(),
            timeout_height: IbcHeight::new(1, 10).expect("valid timeout height"),
            timeout_time: 60_000_000_000,
            source_channel: ChannelId::from_str("channel-0").expect("valid channel id"),
            ics20_memo: String::new(),
            use_transparent_address: false,
        };
        let plan = crate::test_plan_helpers::ics20_withdrawal(
            vec![spend],
            None,
            withdrawal,
            Fr::from(7u64),
        )
        .expect("one-spend withdrawal plan should be valid");
        let anchor = shieldd_sdk_tct::Tree::default().root();
        ShieldedIcs20Withdrawal {
            body: plan
                .action_body(&test_keys::FULL_VIEWING_KEY, &[7u8; 32].into(), anchor, 0)
                .expect("derive action body"),
            auth_sigs: vec![
                dummy_spend_auth_sig();
                ShieldedIcs20WithdrawalFamilyId::Canonical.auth_sig_count()
            ],
            proof: Default::default(),
        }
    }

    #[test]
    fn stateless_rejects_nonempty_input_compliance_ciphertext() {
        let value = Value {
            amount: 40u64.into(),
            asset_id: BASE_ASSET_DENOM.id(),
        };
        let note = Note::generate(&mut OsRng, &test_keys::ADDRESS_0, value);
        let spend = ShieldedInputPlan::new(&mut OsRng, note, 0u64.into());
        let withdrawal = Ics20Withdrawal {
            amount: 40u64.into(),
            denom: BASE_ASSET_DENOM.clone(),
            destination_chain_address: "cosmos1destination".to_string(),
            return_address: test_keys::ADDRESS_0.deref().clone(),
            timeout_height: IbcHeight::new(1, 10).expect("valid timeout height"),
            timeout_time: 60_000_000_000,
            source_channel: ChannelId::from_str("channel-0").expect("valid channel id"),
            ics20_memo: String::new(),
            use_transparent_address: false,
        };
        let plan = crate::test_plan_helpers::ics20_withdrawal(
            vec![spend],
            None,
            withdrawal,
            Fr::from(7u64),
        )
        .expect("withdrawal plan should be valid");
        let anchor = shieldd_sdk_tct::Tree::default().root();
        let mut body = plan
            .action_body(&test_keys::FULL_VIEWING_KEY, &[7u8; 32].into(), anchor, 0)
            .expect("withdrawal body should build");
        body.inputs[0].compliance_ciphertext.push(1);
        let auth_sigs = vec![dummy_spend_auth_sig(); body.inputs.len()];
        let action = ShieldedIcs20Withdrawal {
            body,
            auth_sigs,
            proof: Default::default(),
        };
        let context = TransactionContext {
            anchor,
            effect_hash: Default::default(),
            recent_position_floor: 0,
        };

        let err = shielded_ics20_withdrawal_check_stateless_and_extract(&action, &context)
            .err()
            .expect("stateless validation must reject unused input compliance ciphertext");
        assert!(err
            .to_string()
            .contains("withdrawal input 1 compliance ciphertext must be empty"));
    }

    #[test]
    fn auth_verification_rejects_invalid_optional_dummy_signature() {
        let value = Value {
            amount: 40u64.into(),
            asset_id: BASE_ASSET_DENOM.id(),
        };
        let note = Note::generate(&mut OsRng, &test_keys::ADDRESS_0, value);
        let spend = ShieldedInputPlan::new(&mut OsRng, note, 0u64.into());
        let withdrawal = Ics20Withdrawal {
            amount: 40u64.into(),
            denom: BASE_ASSET_DENOM.clone(),
            destination_chain_address: "cosmos1destination".to_string(),
            return_address: test_keys::ADDRESS_0.deref().clone(),
            timeout_height: IbcHeight::new(1, 10).expect("valid timeout height"),
            timeout_time: 60_000_000_000,
            source_channel: ChannelId::from_str("channel-0").expect("valid channel id"),
            ics20_memo: String::new(),
            use_transparent_address: false,
        };
        let plan = crate::test_plan_helpers::ics20_withdrawal(
            vec![spend],
            None,
            withdrawal,
            Fr::from(7u64),
        )
        .expect("one-spend withdrawal plan should be valid");
        let anchor = shieldd_sdk_tct::Tree::default().root();
        let body = plan
            .action_body(&test_keys::FULL_VIEWING_KEY, &[7u8; 32].into(), anchor, 0)
            .expect("withdrawal body should build");
        let context = TransactionContext {
            anchor,
            effect_hash: Default::default(),
            recent_position_floor: 0,
        };
        let real_rsk = test_keys::SPEND_KEY
            .spend_auth_key()
            .randomize(&plan.spends[0].randomizer);
        let real_sig = real_rsk.sign(&mut OsRng, context.effect_hash.as_ref());
        let dummy_sig = plan.synthetic_dummy_auth_sig(1, context.effect_hash.as_ref());
        let mut action = ShieldedIcs20Withdrawal {
            body,
            auth_sigs: vec![real_sig, dummy_sig],
            proof: Default::default(),
        };
        shielded_ics20_withdrawal_verify_auth_sigs(&action, &context)
            .expect("real and synthetic dummy signatures should verify");

        let wrong_dummy = SigningKey::<SpendAuth>::from(Fr::from(999u64));
        action.auth_sigs[1] = wrong_dummy.sign(&mut OsRng, context.effect_hash.as_ref());
        let error = shielded_ics20_withdrawal_verify_auth_sigs(&action, &context)
            .expect_err("the serialized optional dummy RK must authenticate its signature");
        assert!(
            error.to_string().contains("auth signature 1 failed"),
            "unexpected rejection reason: {error:#}"
        );
    }

    #[test]
    fn withdrawal_auth_verification_rejects_identity_randomized_key() {
        let mut action = test_action();
        let context = TransactionContext {
            anchor: action.body.anchor,
            effect_hash: Default::default(),
            recent_position_floor: 0,
        };
        let identity_sk = SigningKey::<SpendAuth>::from(Fr::from(0u64));
        action.body.inputs[0].rk = VerificationKey::from(identity_sk.clone());
        let different_message = b"different withdrawal authorization hash";
        assert_ne!(&different_message[..], context.effect_hash.as_ref());
        action.auth_sigs[0] = identity_sk.sign_deterministic(different_message);

        action.body.inputs[0]
            .rk
            .verify(context.effect_hash.as_ref(), &action.auth_sigs[0])
            .expect("the pinned RDSA primitive admits identity keys across messages");
        let error = shielded_ics20_withdrawal_verify_auth_sigs(&action, &context)
            .expect_err("Withdrawal must reject an identity randomized spend key");
        assert!(
            error
                .to_string()
                .contains("randomized spend key 0 must not be identity"),
            "unexpected rejection reason: {error:#}"
        );
    }

    #[test]
    fn withdrawal_action_public_matches_proving_public() {
        let value = Value {
            amount: 40u64.into(),
            asset_id: BASE_ASSET_DENOM.id(),
        };
        let note = Note::generate(&mut OsRng, &test_keys::ADDRESS_0, value);
        let note_commitment = note.commit();
        let spend = ShieldedInputPlan::new(&mut OsRng, note, 0u64.into());
        let withdrawal = Ics20Withdrawal {
            amount: 40u64.into(),
            denom: BASE_ASSET_DENOM.clone(),
            destination_chain_address: "cosmos1destination".to_string(),
            return_address: test_keys::ADDRESS_0.deref().clone(),
            timeout_height: IbcHeight::new(1, 10).expect("valid timeout height"),
            timeout_time: 60_000_000_000,
            source_channel: ChannelId::from_str("channel-0").expect("valid channel id"),
            ics20_memo: String::new(),
            use_transparent_address: false,
        };
        let plan = crate::test_plan_helpers::ics20_withdrawal(
            vec![spend],
            None,
            withdrawal,
            Fr::from(7u64),
        )
        .expect("one-spend withdrawal plan should be valid");
        let anchor = shieldd_sdk_tct::Tree::default().root();
        let proofs = vec![dummy_state_commitment_proof(note_commitment)];
        let (proving_public, _) = plan
            .shielded_ics20_withdrawal_public_private(
                &test_keys::FULL_VIEWING_KEY,
                &proofs,
                anchor,
                0,
            )
            .expect("derive proving public");
        let action = ShieldedIcs20Withdrawal {
            body: plan
                .action_body(&test_keys::FULL_VIEWING_KEY, &[7u8; 32].into(), anchor, 0)
                .expect("derive action body"),
            auth_sigs: vec![
                dummy_spend_auth_sig();
                ShieldedIcs20WithdrawalFamilyId::Canonical.auth_sig_count()
            ],
            proof: Default::default(),
        };
        let context = TransactionContext {
            anchor,
            effect_hash: Default::default(),
            recent_position_floor: 0,
        };
        let extracted = shielded_ics20_withdrawal_extract_public(&action, &context)
            .expect("extract verifier public");

        assert_eq!(
            proving_public
                .statement_hash()
                .expect("proving statement hash"),
            extracted
                .statement_hash()
                .expect("extracted statement hash"),
            "withdrawal action projection must equal the proving statement"
        );
    }

    #[tokio::test]
    async fn historical_check_accepts_enabled_withdrawals() {
        let action = test_action();
        let storage = cnidarium::TempStorage::new()
            .await
            .expect("temporary storage");
        let mut state = cnidarium::StateDelta::new(storage.latest_snapshot());
        state.put_ibc_params(IBCParameters {
            outbound_ics20_transfers_enabled: true,
            ..Default::default()
        });

        action
            .check_historical(Arc::new(state))
            .await
            .expect("enabled outbound withdrawals must be accepted");
    }

    #[tokio::test]
    async fn historical_check_rejects_disabled_withdrawals() {
        let action = test_action();
        let storage = cnidarium::TempStorage::new()
            .await
            .expect("temporary storage");
        let mut state = cnidarium::StateDelta::new(storage.latest_snapshot());
        state.put_ibc_params(IBCParameters {
            outbound_ics20_transfers_enabled: false,
            ..Default::default()
        });

        let error = action
            .check_historical(Arc::new(state))
            .await
            .expect_err("disabled outbound withdrawals must be rejected");
        assert!(
            error
                .to_string()
                .contains("outbound ICS20 withdrawals are not enabled"),
            "unexpected rejection reason: {error:#}"
        );
    }

    #[tokio::test]
    async fn raw_withdrawal_execution_fails_closed_without_verified_capability() {
        let action = test_action();
        let storage = cnidarium::TempStorage::new()
            .await
            .expect("temporary storage");
        let mut state = cnidarium::StateDelta::new(storage.latest_snapshot());
        state.put_ibc_params(IBCParameters {
            outbound_ics20_transfers_enabled: false,
            ..Default::default()
        });

        let error = action
            .check_and_execute(&mut state)
            .await
            .expect_err("raw withdrawal execution must not mutate state");
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
