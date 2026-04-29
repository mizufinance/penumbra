use anyhow::Result;
use ark_ff::Zero;
use decaf377::Fr;
use decaf377_rdsa as rdsa;
#[cfg(all(feature = "parallel", any(unix, windows)))]
use penumbra_sdk_keys::symmetric::PayloadKey;
#[cfg(any(unix, windows))]
use penumbra_sdk_keys::FullViewingKey;
use penumbra_sdk_txhash::{AuthorizingData, EffectingData};
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

        let detection_data = self.detection_data.as_ref().map(|x| x.detection_data());

        Ok(Transaction {
            transaction_body: TransactionBody {
                actions,
                transaction_parameters: self.transaction_parameters,
                fee_funding,
                detection_data,
                memo,
            },
            anchor: witness_data.anchor,
            binding_sig: [0; 64].into(),
        })
    }

    pub fn apply_auth_data(
        &self,
        auth_data: &AuthorizationData,
        mut transaction: Transaction,
    ) -> Result<Transaction> {
        let spend_count = self
            .actions
            .iter()
            .map(|action| match action {
                ActionPlan::Transfer(plan) => plan.spends.len(),
                ActionPlan::Consolidate(plan) => plan.spends.len(),
                ActionPlan::Split(plan) => plan.spends.len(),
                ActionPlan::ShieldedIcs20Withdrawal(plan) => plan.spends.len(),
                _ => 0,
            })
            .sum::<usize>()
            + self
                .fee_funding
                .as_ref()
                .map(|fee_funding| fee_funding.transfer.spends.len())
                .unwrap_or_default();

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

        let mut spend_auths = auth_data.spend_auths.clone().into_iter();
        let effect_hash = auth_data
            .effect_hash
            .unwrap_or_else(|| transaction.effect_hash());

        for (action_plan, action) in self
            .actions
            .iter()
            .zip(transaction.transaction_body.actions.iter_mut())
        {
            match (action_plan, action) {
                (ActionPlan::Transfer(plan), Action::Transfer(transfer)) => {
                    for (index, auth_sig) in transfer.auth_sigs.iter_mut().enumerate() {
                        if index < plan.spends.len() {
                            *auth_sig = spend_auths.next().expect("checked spend auth count");
                        } else {
                            *auth_sig = plan.synthetic_dummy_auth_sig(index, effect_hash.as_ref());
                        }
                    }
                }
                (ActionPlan::Consolidate(plan), Action::Consolidate(consolidate)) => {
                    for auth_sig in consolidate.auth_sigs.iter_mut().take(plan.spends.len()) {
                        *auth_sig = spend_auths.next().expect("checked spend auth count");
                    }
                }
                (ActionPlan::Split(plan), Action::Split(split)) => {
                    for auth_sig in split.auth_sigs.iter_mut().take(plan.spends.len()) {
                        *auth_sig = spend_auths.next().expect("checked spend auth count");
                    }
                }
                (
                    ActionPlan::ShieldedIcs20Withdrawal(plan),
                    Action::ShieldedIcs20Withdrawal(withdrawal),
                ) => {
                    for (index, auth_sig) in withdrawal.auth_sigs.iter_mut().enumerate() {
                        if index < plan.spends.len() {
                            *auth_sig = spend_auths.next().expect("checked spend auth count");
                        } else {
                            *auth_sig = plan.synthetic_dummy_auth_sig(index, effect_hash.as_ref());
                        }
                    }
                }
                _ => {}
            }
        }

        if let (Some(fee_funding_plan), Some(fee_funding)) = (
            self.fee_funding.as_ref(),
            transaction.transaction_body.fee_funding.as_mut(),
        ) {
            for (index, auth_sig) in fee_funding.transfer.auth_sigs.iter_mut().enumerate() {
                if index < fee_funding_plan.transfer.spends.len() {
                    *auth_sig = spend_auths.next().expect("checked spend auth count");
                } else {
                    *auth_sig = fee_funding_plan
                        .transfer
                        .synthetic_dummy_auth_sig(index, effect_hash.as_ref());
                }
            }
        }

        let binding_signing_key = rdsa::SigningKey::from(synthetic_blinding_factor);
        let auth_hash = transaction.transaction_body.auth_hash();
        transaction.binding_sig = binding_signing_key.sign_deterministic(auth_hash.as_bytes());

        Ok(transaction)
    }

    #[cfg(any(unix, windows))]
    pub fn build(
        self,
        full_viewing_key: &FullViewingKey,
        witness_data: &WitnessData,
        auth_data: &AuthorizationData,
    ) -> Result<Transaction> {
        let actions = self
            .actions
            .iter()
            .map(|action_plan| {
                ActionPlan::build_unauth(
                    action_plan.clone(),
                    full_viewing_key,
                    witness_data,
                    self.memo_key(),
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let memo_key = self.memo_key().unwrap_or([0u8; 32].into());
        let fee_funding = self
            .fee_funding
            .as_ref()
            .map(|fee_funding| fee_funding.build_unauth(full_viewing_key, witness_data, &memo_key))
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
        let witness_data = std::sync::Arc::new(witness_data.clone());

        let scheduler =
            ActionBuildScheduler::new(self.memo_key(), full_viewing_key, witness_data.clone());
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
            .map(|fee_funding| fee_funding.build_unauth(full_viewing_key, &witness_data, &memo_key))
            .transpose()?;

        let tx = self
            .clone()
            .build_unauth_with_actions(actions, fee_funding, &*witness_data)?;
        self.apply_auth_data(auth_data, tx)
    }

    pub fn witness_data(&self, sct: &penumbra_sdk_tct::Tree) -> Result<WitnessData, anyhow::Error> {
        let anchor = sct.root();

        let witness_note = |spend: &penumbra_sdk_shielded_pool::ShieldedInputPlan| {
            let commitment = spend.note.commit();
            sct.witness(commitment)
                .ok_or_else(|| anyhow::anyhow!("commitment should exist in tree"))
                .map(|proof| (commitment, proof))
        };

        let mut state_commitment_proofs = std::collections::BTreeMap::new();
        for action in &self.actions {
            match action {
                ActionPlan::Transfer(plan) => {
                    for spend in &plan.spends {
                        let (commitment, proof) = witness_note(spend)?;
                        state_commitment_proofs.insert(commitment, proof);
                    }
                }
                ActionPlan::Consolidate(plan) => {
                    for spend in &plan.spends {
                        let (commitment, proof) = witness_note(spend)?;
                        state_commitment_proofs.insert(commitment, proof);
                    }
                }
                ActionPlan::Split(plan) => {
                    for spend in &plan.spends {
                        let (commitment, proof) = witness_note(spend)?;
                        state_commitment_proofs.insert(commitment, proof);
                    }
                }
                ActionPlan::ShieldedIcs20Withdrawal(plan) => {
                    for spend in &plan.spends {
                        let (commitment, proof) = witness_note(spend)?;
                        state_commitment_proofs.insert(commitment, proof);
                    }
                }
                _ => {}
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
        })
    }
}

#[cfg(all(feature = "parallel", any(unix, windows)))]
struct ActionBuildScheduler {
    memo_key: Option<PayloadKey>,
    full_viewing_key: FullViewingKey,
    witness_data: std::sync::Arc<WitnessData>,
}

#[cfg(all(feature = "parallel", any(unix, windows)))]
impl ActionBuildScheduler {
    fn new(
        memo_key: Option<PayloadKey>,
        full_viewing_key: &FullViewingKey,
        witness_data: std::sync::Arc<WitnessData>,
    ) -> Self {
        Self {
            memo_key,
            full_viewing_key: full_viewing_key.clone(),
            witness_data,
        }
    }

    fn spawn(&self, action_plan: ActionPlan) -> Result<PendingActionTask> {
        let fvk = self.full_viewing_key.clone();
        let witness_data = self.witness_data.clone();
        let memo_key = self.memo_key;

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
                        ));
                    })
                    .map_err(|e| anyhow::anyhow!("spawn transfer action build thread: {e}"))?;
                Ok(PendingActionTask::Thread(rx))
            }
            other => Ok(PendingActionTask::Tokio(tokio::task::spawn_blocking(
                move || ActionPlan::build_unauth(other, &fvk, &witness_data, memo_key),
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
