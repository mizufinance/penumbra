use anyhow::Result;
use ark_ff::Zero;
use decaf377::Fr;
use decaf377_rdsa as rdsa;
#[cfg(all(feature = "parallel", any(unix, windows)))]
use shieldd_sdk_keys::symmetric::PayloadKey;
#[cfg(any(unix, windows))]
use shieldd_sdk_keys::FullViewingKey;
use shieldd_sdk_txhash::{AuthorizingData, EffectingData};
#[cfg(all(feature = "parallel", any(unix, windows)))]
use tokio::sync::oneshot;

use super::TransactionPlan;
use crate::ActionPlan;
use crate::AuthorizationData;
use crate::{action::Action, Transaction, TransactionBody, WitnessData};

impl TransactionPlan {
    pub fn build_unauth_with_actions(
        self,
        actions: Vec<Action>,
        fee_funding: Option<crate::FeeFunding>,
        witness_data: &WitnessData,
    ) -> Result<Transaction> {
        let memo = self
            .memo
            .as_ref()
            .map(|memo_data| memo_data.memo())
            .transpose()?;

        let transaction = Transaction {
            transaction_body: TransactionBody {
                actions,
                transaction_parameters: self.transaction_parameters,
                fee_funding,
                memo,
                nullifier_window: self.nullifier_window,
                historical_nullifier_proofs: witness_data.historical_nullifier_proofs.clone(),
            },
            anchor: witness_data.anchor,
            binding_sig: [0; 64].into(),
        };
        transaction.transaction_body.validate_nullifier_history()?;
        Ok(transaction)
    }

    pub fn apply_auth_data(
        &self,
        auth_data: &AuthorizationData,
        mut transaction: Transaction,
    ) -> Result<Transaction> {
        let spend_count = self.num_spends();

        if auth_data.spend_auths.len() != spend_count {
            anyhow::bail!(
                "expected {} spend auths but got {}",
                spend_count,
                auth_data.spend_auths.len()
            );
        }

        let mut synthetic_blinding_factor = Fr::zero();
        for action_plan in &self.actions {
            synthetic_blinding_factor += action_plan.value_blinding();
        }
        if let Some(fee_funding) = &self.fee_funding {
            synthetic_blinding_factor += fee_funding.value_blinding();
        }
        anyhow::ensure!(
            self.num_proofs() == 0 || !synthetic_blinding_factor.is_zero(),
            "proof-bearing transaction plan has an identity aggregate binding key; rebuild with fresh balance blindings"
        );
        anyhow::ensure!(
            (self.num_proofs() == 0 && spend_count == 0) || auth_data.effect_hash.is_some(),
            "proof- or spend-bearing transaction authorization data must include the approved effect hash"
        );

        anyhow::ensure!(
            self.actions.len() == transaction.transaction_body.actions.len(),
            "transaction action count does not match plan: expected {}, got {}",
            self.actions.len(),
            transaction.transaction_body.actions.len()
        );
        anyhow::ensure!(
            self.fee_funding.is_some() == transaction.transaction_body.fee_funding.is_some(),
            "transaction fee-funding presence does not match plan"
        );

        let transaction_effect_hash = transaction.effect_hash();
        if let Some(supplied_effect_hash) = auth_data.effect_hash {
            anyhow::ensure!(
                supplied_effect_hash == transaction_effect_hash,
                "authorization-data effect hash does not match transaction effect hash"
            );
        }

        let mut spend_auths = auth_data.spend_auths.clone().into_iter();
        let effect_hash = transaction_effect_hash;

        for (action_index, (action_plan, action)) in self
            .actions
            .iter()
            .zip(transaction.transaction_body.actions.iter_mut())
            .enumerate()
        {
            match (action_plan, action) {
                (ActionPlan::Transfer(plan), Action::Transfer(transfer)) => {
                    plan.validate().map_err(|error| {
                        anyhow::anyhow!(
                            "invalid transfer plan at action {action_index}: {error}"
                        )
                    })?;
                    transfer.body.validate_shape().map_err(|error| {
                        anyhow::anyhow!(
                            "invalid transfer shape at action {action_index}: {error}"
                        )
                    })?;
                    anyhow::ensure!(
                        transfer.auth_sigs.len()
                            == shieldd_sdk_shielded_pool::transfer_auth_sig_count(),
                        "transfer action {action_index} expected {} authorization signature slots, got {}",
                        shieldd_sdk_shielded_pool::transfer_auth_sig_count(),
                        transfer.auth_sigs.len()
                    );
                    anyhow::ensure!(
                        plan.spends.len() <= transfer.auth_sigs.len(),
                        "transfer action {action_index} has fewer authorization signature slots than real spends"
                    );
                    for (index, auth_sig) in transfer.auth_sigs.iter_mut().enumerate() {
                        if index < plan.spends.len() {
                            *auth_sig = spend_auths.next().ok_or_else(|| {
                                anyhow::anyhow!(
                                    "missing spend authorization for transfer action {action_index} slot {index}"
                                )
                            })?;
                        } else {
                            *auth_sig = plan.synthetic_dummy_auth_sig(index, effect_hash.as_ref());
                        }
                    }
                }
                (ActionPlan::NoteReshape(plan), Action::NoteReshape(note_reshape)) => {
                    plan.validate().map_err(|error| {
                        anyhow::anyhow!(
                            "invalid NoteReshape plan at action {action_index}: {error}"
                        )
                    })?;
                    anyhow::ensure!(
                        plan.family_id() == note_reshape.body.family_id,
                        "NoteReshape family at action {action_index} does not match plan"
                    );
                    note_reshape.body.validate_shape().map_err(|error| {
                        anyhow::anyhow!(
                            "invalid NoteReshape shape at action {action_index}: {error}"
                        )
                    })?;
                    anyhow::ensure!(
                        note_reshape.auth_sigs.len()
                            == note_reshape.body.family_id.auth_sig_count(),
                        "NoteReshape action {action_index} expected {} authorization signature slots, got {}",
                        note_reshape.body.family_id.auth_sig_count(),
                        note_reshape.auth_sigs.len()
                    );
                    anyhow::ensure!(
                        plan.spends.len() <= note_reshape.auth_sigs.len(),
                        "NoteReshape action {action_index} has fewer authorization signature slots than real spends"
                    );
                    for (index, auth_sig) in note_reshape.auth_sigs.iter_mut().enumerate() {
                        if index < plan.spends.len() {
                            *auth_sig = spend_auths.next().ok_or_else(|| {
                                anyhow::anyhow!(
                                    "missing spend authorization for NoteReshape action {action_index} slot {index}"
                                )
                            })?;
                        } else {
                            *auth_sig = plan.synthetic_dummy_auth_sig(index, effect_hash.as_ref());
                        }
                    }
                }
                (
                    ActionPlan::ShieldedIcs20Withdrawal(plan),
                    Action::ShieldedIcs20Withdrawal(withdrawal),
                ) => {
                    plan.validate().map_err(|error| {
                        anyhow::anyhow!(
                            "invalid shielded ICS-20 withdrawal plan at action {action_index}: {error}"
                        )
                    })?;
                    withdrawal.body.validate_shape().map_err(|error| {
                        anyhow::anyhow!(
                            "invalid shielded ICS-20 withdrawal shape at action {action_index}: {error}"
                        )
                    })?;
                    anyhow::ensure!(
                        plan.withdrawal.effect_hash() == withdrawal.body.withdrawal.effect_hash(),
                        "shielded ICS-20 withdrawal payload at action {action_index} does not match plan"
                    );
                    anyhow::ensure!(
                        withdrawal.auth_sigs.len()
                            == withdrawal.body.family_id.auth_sig_count(),
                        "shielded ICS-20 withdrawal action {action_index} expected {} authorization signature slots, got {}",
                        withdrawal.body.family_id.auth_sig_count(),
                        withdrawal.auth_sigs.len()
                    );
                    anyhow::ensure!(
                        plan.spends.len() <= withdrawal.auth_sigs.len(),
                        "shielded ICS-20 withdrawal action {action_index} has fewer authorization signature slots than real spends"
                    );
                    for (index, auth_sig) in withdrawal.auth_sigs.iter_mut().enumerate() {
                        if index < plan.spends.len() {
                            *auth_sig = spend_auths.next().ok_or_else(|| {
                                anyhow::anyhow!(
                                    "missing spend authorization for shielded ICS-20 withdrawal action {action_index} slot {index}"
                                )
                            })?;
                        } else {
                            *auth_sig = plan.synthetic_dummy_auth_sig(index, effect_hash.as_ref());
                        }
                    }
                }
                (
                    ActionPlan::ShieldedHostWithdrawal(plan),
                    Action::ShieldedHostWithdrawal(withdrawal),
                ) => {
                    plan.validate().map_err(|error| {
                        anyhow::anyhow!(
                            "invalid shielded host withdrawal plan at action {action_index}: {error}"
                        )
                    })?;
                    withdrawal.body.validate_shape().map_err(|error| {
                        anyhow::anyhow!(
                            "invalid shielded host withdrawal shape at action {action_index}: {error}"
                        )
                    })?;
                    anyhow::ensure!(
                        plan.withdrawal.effect_hash() == withdrawal.body.withdrawal.effect_hash(),
                        "shielded host withdrawal payload at action {action_index} does not match plan"
                    );
                    anyhow::ensure!(
                        withdrawal.auth_sigs.len()
                            == withdrawal.body.family_id.auth_sig_count(),
                        "shielded host withdrawal action {action_index} expected {} authorization signature slots, got {}",
                        withdrawal.body.family_id.auth_sig_count(),
                        withdrawal.auth_sigs.len()
                    );
                    anyhow::ensure!(
                        plan.spends.len() <= withdrawal.auth_sigs.len(),
                        "shielded host withdrawal action {action_index} has fewer authorization signature slots than real spends"
                    );
                    for (index, auth_sig) in withdrawal.auth_sigs.iter_mut().enumerate() {
                        if index < plan.spends.len() {
                            *auth_sig = spend_auths.next().ok_or_else(|| {
                                anyhow::anyhow!(
                                    "missing spend authorization for shielded host withdrawal action {action_index} slot {index}"
                                )
                            })?;
                        } else {
                            *auth_sig =
                                plan.synthetic_dummy_auth_sig(index, effect_hash.as_ref());
                        }
                    }
                }
                (ActionPlan::IbcAction(plan), Action::IbcRelay(action)) => anyhow::ensure!(
                    plan.effect_hash() == action.effect_hash(),
                    "IBC relay action {action_index} does not match plan"
                ),
                (
                    ActionPlan::ComplianceRegisterAsset(plan),
                    Action::ComplianceRegisterAsset(action),
                ) => anyhow::ensure!(
                    plan.effect_hash() == action.effect_hash(),
                    "compliance asset-registration action {action_index} does not match plan"
                ),
                (
                    ActionPlan::ComplianceRegisterUser(plan),
                    Action::ComplianceRegisterUser(action),
                ) => anyhow::ensure!(
                    plan.effect_hash() == action.effect_hash(),
                    "compliance user-registration action {action_index} does not match plan"
                ),
                _ => anyhow::bail!(
                    "transaction action at index {action_index} does not match its ActionPlan variant"
                ),
            }
        }

        match (
            self.fee_funding.as_ref(),
            transaction.transaction_body.fee_funding.as_mut(),
        ) {
            (Some(fee_funding_plan), Some(fee_funding)) => {
                fee_funding_plan.transfer.validate().map_err(|error| {
                    anyhow::anyhow!("invalid fee-funding transfer plan: {error}")
                })?;
                fee_funding
                    .transfer
                    .body
                    .validate_shape()
                    .map_err(|error| {
                        anyhow::anyhow!("invalid fee-funding transfer shape: {error}")
                    })?;
                anyhow::ensure!(
                    fee_funding.transfer.auth_sigs.len()
                        == shieldd_sdk_shielded_pool::transfer_auth_sig_count(),
                    "fee-funding transfer expected {} authorization signature slots, got {}",
                    shieldd_sdk_shielded_pool::transfer_auth_sig_count(),
                    fee_funding.transfer.auth_sigs.len()
                );
                anyhow::ensure!(
                    fee_funding_plan.transfer.spends.len() <= fee_funding.transfer.auth_sigs.len(),
                    "fee-funding transfer has fewer authorization signature slots than real spends"
                );
                for (index, auth_sig) in fee_funding.transfer.auth_sigs.iter_mut().enumerate() {
                    if index < fee_funding_plan.transfer.spends.len() {
                        *auth_sig = spend_auths.next().ok_or_else(|| {
                            anyhow::anyhow!(
                                "missing spend authorization for fee-funding slot {index}"
                            )
                        })?;
                    } else {
                        *auth_sig = fee_funding_plan
                            .transfer
                            .synthetic_dummy_auth_sig(index, effect_hash.as_ref());
                    }
                }
            }
            (None, None) => {}
            _ => anyhow::bail!("transaction fee-funding presence does not match plan"),
        }
        anyhow::ensure!(
            spend_auths.next().is_none(),
            "authorization data contains unconsumed spend signatures"
        );
        anyhow::ensure!(
            self.num_proofs() == transaction.num_proofs(),
            "transaction proof count does not match plan"
        );

        transaction.binding_sig = if synthetic_blinding_factor.is_zero() {
            crate::no_binding_signature()
        } else {
            let binding_signing_key = rdsa::SigningKey::from(synthetic_blinding_factor);
            let auth_hash = transaction.transaction_body.auth_hash();
            binding_signing_key.sign_deterministic(auth_hash.as_bytes())
        };

        Ok(transaction)
    }

    #[cfg(any(unix, windows))]
    pub fn build(
        self,
        full_viewing_key: &FullViewingKey,
        witness_data: &WitnessData,
        auth_data: &AuthorizationData,
    ) -> Result<Transaction> {
        let recent_position_floor = self.recent_position_floor()?;
        let actions = self
            .actions
            .iter()
            .map(|action_plan| {
                ActionPlan::build_unauth(
                    action_plan.clone(),
                    full_viewing_key,
                    witness_data,
                    self.memo_key(),
                    recent_position_floor,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let memo_key = self.memo_key().unwrap_or([0u8; 32].into());
        let fee_funding = self
            .fee_funding
            .as_ref()
            .map(|fee_funding| {
                fee_funding.build_unauth(
                    full_viewing_key,
                    witness_data,
                    &memo_key,
                    recent_position_floor,
                )
            })
            .transpose()?;

        let tx = self
            .clone()
            .build_unauth_with_actions(actions, fee_funding, witness_data)?;
        self.apply_auth_data(auth_data, tx)
    }

    #[cfg(all(feature = "parallel", any(unix, windows)))]
    pub async fn build_concurrent(
        self,
        full_viewing_key: &FullViewingKey,
        witness_data: &WitnessData,
        auth_data: &AuthorizationData,
    ) -> Result<Transaction> {
        let recent_position_floor = self.recent_position_floor()?;
        let witness_data = std::sync::Arc::new(witness_data.clone());

        let scheduler = ActionBuildScheduler::new(
            self.memo_key(),
            full_viewing_key,
            witness_data.clone(),
            recent_position_floor,
        );
        let action_tasks = self
            .actions
            .iter()
            .cloned()
            .map(|action_plan| scheduler.spawn(action_plan))
            .collect::<Result<Vec<_>>>()?;
        let actions = scheduler.collect(action_tasks).await?;
        let memo_key = self.memo_key().unwrap_or([0u8; 32].into());
        let fee_funding = self
            .fee_funding
            .as_ref()
            .map(|fee_funding| {
                fee_funding.build_unauth(
                    full_viewing_key,
                    &witness_data,
                    &memo_key,
                    recent_position_floor,
                )
            })
            .transpose()?;

        let tx = self
            .clone()
            .build_unauth_with_actions(actions, fee_funding, &*witness_data)?;
        self.apply_auth_data(auth_data, tx)
    }

    pub fn witness_data(&self, sct: &shieldd_sdk_tct::Tree) -> Result<WitnessData, anyhow::Error> {
        let anchor = sct.root();

        let witness_note = |spend: &shieldd_sdk_shielded_pool::ShieldedInputPlan| {
            let commitment = spend.note.commit();
            sct.witness(commitment)
                .ok_or_else(|| anyhow::anyhow!("commitment should exist in tree"))
                .map(|proof| (commitment, proof))
        };

        let mut state_commitment_proofs = std::collections::BTreeMap::new();
        for action in &self.actions {
            for spend in action.spends() {
                let (commitment, proof) = witness_note(spend)?;
                state_commitment_proofs.insert(commitment, proof);
            }
            let accumulator_commitment = match action {
                ActionPlan::Transfer(plan) => plan.accumulator_prior_commitment(),
                ActionPlan::ShieldedHostWithdrawal(plan) => plan.accumulator_prior_commitment(),
                ActionPlan::ShieldedIcs20Withdrawal(plan) => plan.accumulator_prior_commitment(),
                _ => None,
            };
            if let Some(commitment) = accumulator_commitment {
                let proof = sct.witness(commitment).ok_or_else(|| {
                    anyhow::anyhow!("volume accumulator commitment should exist in tree")
                })?;
                state_commitment_proofs.insert(commitment, proof);
            }
        }
        if let Some(fee_funding) = &self.fee_funding {
            for spend in &fee_funding.transfer.spends {
                let (commitment, proof) = witness_note(spend)?;
                state_commitment_proofs.insert(commitment, proof);
            }
        }

        Ok(WitnessData {
            anchor,
            state_commitment_proofs,
            historical_nullifier_proofs: Vec::new(),
        })
    }
}

#[cfg(all(feature = "parallel", any(unix, windows)))]
struct ActionBuildScheduler {
    memo_key: Option<PayloadKey>,
    full_viewing_key: FullViewingKey,
    witness_data: std::sync::Arc<WitnessData>,
    recent_position_floor: u64,
}

#[cfg(all(feature = "parallel", any(unix, windows)))]
impl ActionBuildScheduler {
    fn new(
        memo_key: Option<PayloadKey>,
        full_viewing_key: &FullViewingKey,
        witness_data: std::sync::Arc<WitnessData>,
        recent_position_floor: u64,
    ) -> Self {
        Self {
            memo_key,
            full_viewing_key: full_viewing_key.clone(),
            witness_data,
            recent_position_floor,
        }
    }

    fn spawn(&self, action_plan: ActionPlan) -> Result<PendingActionTask> {
        let fvk = self.full_viewing_key.clone();
        let witness_data = self.witness_data.clone();
        let memo_key = self.memo_key;
        let recent_position_floor = self.recent_position_floor;

        match action_plan {
            transfer @ ActionPlan::Transfer(_) => {
                let (tx, rx) = oneshot::channel();
                std::thread::Builder::new()
                    .name("transfer-action-build".to_string())
                    .spawn(move || {
                        let _ = tx.send(ActionPlan::build_unauth(
                            transfer,
                            &fvk,
                            &witness_data,
                            memo_key,
                            recent_position_floor,
                        ));
                    })
                    .map_err(|e| anyhow::anyhow!("spawn transfer action build thread: {e}"))?;
                Ok(PendingActionTask::Thread(rx))
            }
            other => Ok(PendingActionTask::Tokio(tokio::task::spawn_blocking(
                move || {
                    ActionPlan::build_unauth(
                        other,
                        &fvk,
                        &witness_data,
                        memo_key,
                        recent_position_floor,
                    )
                },
            ))),
        }
    }

    async fn collect(&self, tasks: Vec<PendingActionTask>) -> Result<Vec<Action>> {
        let mut actions = Vec::with_capacity(tasks.len());
        for task in tasks {
            actions.push(match task {
                PendingActionTask::Tokio(handle) => handle.await??,
                PendingActionTask::Thread(receiver) => receiver.await.map_err(|_| {
                    anyhow::anyhow!("transfer action build thread exited before replying")
                })??,
            });
        }
        Ok(actions)
    }
}

#[cfg(all(feature = "parallel", any(unix, windows)))]
enum PendingActionTask {
    Tokio(tokio::task::JoinHandle<Result<Action>>),
    Thread(oneshot::Receiver<Result<Action>>),
}
