use crate::planning_intent::{
    intent_gas, ActionIntent, NoteReshapeIntent, TransactionIntent, TransferIntent,
    WithdrawalIntent,
};
use anyhow::{anyhow, Context, Result};
use decaf377::Fr;
use rand::CryptoRng;
use rand_core::RngCore;
use shieldd_sdk_asset::{asset, Balance, Value, BASE_ASSET_ID};
#[cfg(test)]
use shieldd_sdk_compliance::ComplianceQuery;
use shieldd_sdk_fee::{Fee, FeeTier, GasPrices};
use shieldd_sdk_keys::{keys::AddressIndex, Address};
use shieldd_sdk_num::Amount;
use shieldd_sdk_proto::view::v1::NotesRequest;
use shieldd_sdk_sct::nullifier_generation::NullifierWindow;
use shieldd_sdk_shielded_pool::{
    note, HostWithdrawal, Ics20Withdrawal, NoteReshapeFamilyId, ShieldedInputPlan,
    ShieldedOutputPlan,
};
use shieldd_sdk_transaction::{
    memo::MemoPlaintext,
    plan::{ActionPlan, MemoPlan, TransactionPlan},
    TransactionParameters,
};
use std::collections::BTreeSet;

use crate::{
    client_compliance::complete_plan_with_compliance, planning_io::PlanningIo, SpendableNoteRecord,
};

#[derive(Clone, Debug)]
pub struct TransferResumeToken {
    pub source: AddressIndex,
    pub recipient: Address,
    pub value: Value,
}

#[derive(Clone, Debug)]
pub struct Ics20WithdrawalResumeToken {
    pub source: AddressIndex,
    pub withdrawal: Ics20Withdrawal,
}

#[derive(Clone, Debug)]
pub struct HostWithdrawalResumeToken {
    pub source: AddressIndex,
    pub withdrawal: HostWithdrawal,
}

#[derive(Clone, Debug)]
pub struct ActionFundingResumeToken {
    pub source: AddressIndex,
    pub actions: Vec<ActionPlan>,
}

#[derive(Clone, Debug)]
pub struct NoteReshapeResumeToken {
    pub source: AddressIndex,
    pub asset_id: asset::Id,
    pub family_id: Option<NoteReshapeFamilyId>,
    pub note: Option<SpendableNoteRecord>,
    pub output_amounts: Vec<Amount>,
}

#[derive(Clone, Debug)]
pub enum NoteManagerResumeToken {
    Transfer(TransferResumeToken),
    Ics20Withdrawal(Ics20WithdrawalResumeToken),
    HostWithdrawal(HostWithdrawalResumeToken),
    ActionFunding(ActionFundingResumeToken),
    NoteReshape(NoteReshapeResumeToken),
}

#[derive(Clone, Debug)]
enum WalletWithdrawal {
    Ics20(Ics20Withdrawal),
    Host(HostWithdrawal),
}

impl WalletWithdrawal {
    fn amount(&self) -> Amount {
        match self {
            Self::Ics20(withdrawal) => withdrawal.amount,
            Self::Host(withdrawal) => withdrawal.value.amount,
        }
    }

    fn asset_id(&self) -> asset::Id {
        match self {
            Self::Ics20(withdrawal) => withdrawal.denom.id(),
            Self::Host(withdrawal) => withdrawal.value.asset_id,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Ics20(_) => "ICS-20 withdrawal",
            Self::Host(_) => "host withdrawal",
        }
    }

    fn resume_token(&self, source: AddressIndex) -> NoteManagerResumeToken {
        match self {
            Self::Ics20(withdrawal) => {
                NoteManagerResumeToken::Ics20Withdrawal(Ics20WithdrawalResumeToken {
                    source,
                    withdrawal: withdrawal.clone(),
                })
            }
            Self::Host(withdrawal) => {
                NoteManagerResumeToken::HostWithdrawal(HostWithdrawalResumeToken {
                    source,
                    withdrawal: withdrawal.clone(),
                })
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum NoteManagerPlanningResult {
    Ready {
        transaction_plan: TransactionPlan,
    },
    NeedsMaintenance {
        maintenance_plan: TransactionPlan,
        resume_token: NoteManagerResumeToken,
    },
    InsufficientBalance,
    UnsupportedIntent {
        reason: String,
    },
}

enum BaseFeeFundingSelection {
    Ready { selected: Vec<SpendableNoteRecord> },
    NeedsMaintenance { maintenance_plan: TransactionPlan },
    InsufficientBalance,
    UnsupportedIntent { reason: String },
}

fn price_transaction_plan(
    gas_prices: GasPrices,
    fee_tier: FeeTier,
    actions: &[ActionIntent],
    fee_funding: Option<&TransferIntent>,
    nullifier_window: NullifierWindow,
) -> Fee {
    gas_prices
        .fee(&intent_gas(actions, fee_funding, nullifier_window))
        .apply_tier(fee_tier)
}

pub struct NoteManager<R: RngCore + CryptoRng> {
    rng: R,
    fee_tier: FeeTier,
    gas_prices: Option<GasPrices>,
    transaction_parameters: TransactionParameters,
    memo_text: Option<String>,
    memo_return_address: Option<Address>,
}

impl<R: RngCore + CryptoRng> NoteManager<R> {
    pub fn new(rng: R) -> Self {
        Self {
            rng,
            fee_tier: FeeTier::default(),
            gas_prices: None,
            transaction_parameters: TransactionParameters::default(),
            memo_text: None,
            memo_return_address: None,
        }
    }

    pub fn set_gas_prices(&mut self, gas_prices: GasPrices) -> &mut Self {
        self.gas_prices = Some(gas_prices);
        self
    }

    pub fn set_fee_tier(&mut self, fee_tier: FeeTier) -> &mut Self {
        self.fee_tier = fee_tier;
        self
    }

    pub fn expiry_height(&mut self, expiry_height: u64) -> &mut Self {
        self.transaction_parameters.expiry_height = expiry_height;
        self
    }

    pub fn memo(&mut self, text: String) -> &mut Self {
        self.memo_text = Some(text);
        self
    }

    pub fn memo_return_address(&mut self, address: Address) -> &mut Self {
        self.memo_return_address = Some(address);
        self
    }

    pub async fn plan_transfer<V: PlanningIo + Send + ?Sized>(
        &mut self,
        view: &mut V,
        source: AddressIndex,
        value: Value,
        recipient: Address,
    ) -> Result<NoteManagerPlanningResult> {
        self.plan_transfer_values(view, source, vec![value], recipient)
            .await
    }

    pub async fn plan_transfer_values<V: PlanningIo + Send + ?Sized>(
        &mut self,
        view: &mut V,
        source: AddressIndex,
        values: Vec<Value>,
        recipient: Address,
    ) -> Result<NoteManagerPlanningResult> {
        let Some(value) = collapse_transfer_values(values)? else {
            return Ok(NoteManagerPlanningResult::UnsupportedIntent {
                reason: "wallet-facing transfers require at least one output value".to_string(),
            });
        };

        let gas_prices = self
            .gas_prices
            .context("note manager instances must call set_gas_prices prior to planning")?;
        if let Some(result) = ensure_base_gas_prices(gas_prices) {
            return Ok(result);
        }
        let nullifier_window = view.nullifier_window().await?;

        let mut notes = self
            .load_notes_for_asset(view, source, value.asset_id)
            .await?;
        let total_available = notes
            .iter()
            .map(|record| record.note.amount())
            .sum::<Amount>();

        let mut fee = zero_base_fee();

        for _ in 0..4 {
            let required_amount = if gas_prices_are_zero(gas_prices) {
                value
                    .amount
                    .checked_add(&fee.amount())
                    .ok_or_else(|| anyhow!("transfer amount overflow while planning"))?
            } else {
                value.amount
            };
            let selected = select_notes_covering(&mut notes, required_amount);
            let selected_total = selected
                .iter()
                .map(|record| record.note.amount())
                .sum::<Amount>();

            if selected_total < required_amount {
                return Ok(NoteManagerPlanningResult::InsufficientBalance);
            }

            let resume_token = NoteManagerResumeToken::Transfer(TransferResumeToken {
                source,
                recipient: recipient.clone(),
                value,
            });

            let action_needs_maintenance = selected.len() > 2;
            let excluded_fee_notes = selected_note_commitments(&selected);
            let fee_funding_selection = if gas_prices_are_zero(gas_prices) {
                None
            } else {
                Some(
                    self.select_base_fee_funding(view, source, fee, &excluded_fee_notes)
                        .await?,
                )
            };

            if let Some(fee_funding_selection) = fee_funding_selection {
                match fee_funding_selection {
                    BaseFeeFundingSelection::Ready {
                        selected: fee_notes,
                    } => {
                        if action_needs_maintenance {
                            let Some(maintenance_plan) = self
                                .plan_auto_note_reshape_step(
                                    view,
                                    source,
                                    value.asset_id,
                                    selected.len(),
                                )
                                .await?
                            else {
                                return Ok(NoteManagerPlanningResult::UnsupportedIntent {
                                    reason: format!(
                                        "transfer requires note maintenance for asset {}, but no supported note reshape family is currently applicable",
                                        value.asset_id
                                    ),
                                });
                            };

                            return Ok(NoteManagerPlanningResult::NeedsMaintenance {
                                maintenance_plan,
                                resume_token,
                            });
                        }

                        let transfer = self.build_transfer_plan(
                            &selected,
                            recipient.clone(),
                            value,
                            zero_base_fee(),
                        )?;
                        let fee_funding = self.build_fee_funding_plan(&fee_notes, fee)?;
                        let actions = vec![ActionIntent::Transfer(transfer.clone())];
                        let new_fee = price_transaction_plan(
                            gas_prices,
                            self.fee_tier,
                            &actions,
                            Some(&fee_funding),
                            nullifier_window,
                        );

                        if new_fee == fee {
                            let plan = self
                                .finalize_wallet_plan(
                                    view,
                                    source,
                                    actions,
                                    Some(fee_funding),
                                    new_fee,
                                    nullifier_window,
                                )
                                .await?;
                            return Ok(NoteManagerPlanningResult::Ready {
                                transaction_plan: plan,
                            });
                        }

                        fee = new_fee;
                        notes = self
                            .load_notes_for_asset(view, source, value.asset_id)
                            .await?;
                        continue;
                    }
                    BaseFeeFundingSelection::NeedsMaintenance { maintenance_plan } => {
                        return Ok(NoteManagerPlanningResult::NeedsMaintenance {
                            maintenance_plan,
                            resume_token,
                        });
                    }
                    BaseFeeFundingSelection::InsufficientBalance => {
                        return Ok(NoteManagerPlanningResult::InsufficientBalance);
                    }
                    BaseFeeFundingSelection::UnsupportedIntent { reason } => {
                        return Ok(NoteManagerPlanningResult::UnsupportedIntent { reason });
                    }
                }
            }

            if action_needs_maintenance {
                if total_available < required_amount {
                    return Ok(NoteManagerPlanningResult::InsufficientBalance);
                }

                let Some(maintenance_plan) = self
                    .plan_auto_note_reshape_step(view, source, value.asset_id, selected.len())
                    .await?
                else {
                    return Ok(NoteManagerPlanningResult::UnsupportedIntent {
                        reason: format!(
                            "transfer requires note maintenance for asset {}, but no supported note reshape family is currently applicable",
                            value.asset_id
                        ),
                    });
                };

                return Ok(NoteManagerPlanningResult::NeedsMaintenance {
                    maintenance_plan,
                    resume_token,
                });
            }

            let transfer = self.build_transfer_plan(&selected, recipient.clone(), value, fee)?;
            let actions = vec![ActionIntent::Transfer(transfer.clone())];
            let new_fee =
                price_transaction_plan(gas_prices, self.fee_tier, &actions, None, nullifier_window);

            if new_fee == fee {
                let plan = self
                    .finalize_wallet_plan(
                        view,
                        source,
                        vec![ActionIntent::Transfer(transfer)],
                        None,
                        new_fee,
                        nullifier_window,
                    )
                    .await?;
                return Ok(NoteManagerPlanningResult::Ready {
                    transaction_plan: plan,
                });
            }

            fee = new_fee;
            notes = self
                .load_notes_for_asset(view, source, value.asset_id)
                .await?;
        }

        Err(anyhow!("transfer planning did not converge"))
    }

    pub async fn resume_transfer<V: PlanningIo + Send + ?Sized>(
        &mut self,
        view: &mut V,
        resume_token: TransferResumeToken,
    ) -> Result<NoteManagerPlanningResult> {
        self.plan_transfer(
            view,
            resume_token.source,
            resume_token.value,
            resume_token.recipient,
        )
        .await
    }

    pub async fn resume<V: PlanningIo + Send + ?Sized>(
        &mut self,
        view: &mut V,
        resume_token: NoteManagerResumeToken,
    ) -> Result<NoteManagerPlanningResult> {
        match resume_token {
            NoteManagerResumeToken::Transfer(token) => self.resume_transfer(view, token).await,
            NoteManagerResumeToken::Ics20Withdrawal(token) => {
                self.resume_ics20_withdrawal(view, token).await
            }
            NoteManagerResumeToken::HostWithdrawal(token) => {
                self.resume_host_withdrawal(view, token).await
            }
            NoteManagerResumeToken::ActionFunding(token) => {
                self.resume_action_funding(view, token).await
            }
            NoteManagerResumeToken::NoteReshape(token) => {
                self.resume_note_reshape(view, token).await
            }
        }
    }

    pub async fn plan_actions_with_transfer_funding<V: PlanningIo + Send + ?Sized>(
        &mut self,
        view: &mut V,
        source: AddressIndex,
        actions: Vec<ActionPlan>,
    ) -> Result<NoteManagerPlanningResult> {
        if actions.is_empty() {
            return Ok(NoteManagerPlanningResult::UnsupportedIntent {
                reason: "transfer-funded action planning requires at least one action".to_string(),
            });
        }

        if let Some(action) = actions
            .iter()
            .find(|action| action.balance() != Balance::default())
        {
            return Ok(NoteManagerPlanningResult::UnsupportedIntent {
                reason: format!(
                    "transfer-funded action planning only supports balance-neutral actions, got {:?}",
                    action
                ),
            });
        }

        let gas_prices = self
            .gas_prices
            .context("note manager instances must call set_gas_prices prior to planning")?;
        if let Some(result) = ensure_base_gas_prices(gas_prices) {
            return Ok(result);
        }
        let intents = actions
            .iter()
            .cloned()
            .map(ActionIntent::Complete)
            .collect::<Vec<_>>();
        if gas_prices_are_zero(gas_prices) {
            let nullifier_window = view.nullifier_window().await?;
            let plan = self
                .finalize_wallet_plan(
                    view,
                    source,
                    intents,
                    None,
                    zero_base_fee(),
                    nullifier_window,
                )
                .await?;
            return Ok(NoteManagerPlanningResult::Ready {
                transaction_plan: plan,
            });
        }
        self.plan_actions_with_base_fee_funding(
            view,
            source,
            intents,
            NoteManagerResumeToken::ActionFunding(ActionFundingResumeToken { source, actions }),
        )
        .await
    }

    pub async fn resume_action_funding<V: PlanningIo + Send + ?Sized>(
        &mut self,
        view: &mut V,
        resume_token: ActionFundingResumeToken,
    ) -> Result<NoteManagerPlanningResult> {
        self.plan_actions_with_transfer_funding(view, resume_token.source, resume_token.actions)
            .await
    }

    pub async fn plan_ics20_withdrawal<V: PlanningIo + Send + ?Sized>(
        &mut self,
        view: &mut V,
        source: AddressIndex,
        withdrawal: Ics20Withdrawal,
    ) -> Result<NoteManagerPlanningResult> {
        self.plan_wallet_withdrawal(view, source, WalletWithdrawal::Ics20(withdrawal))
            .await
    }

    pub async fn plan_host_withdrawal<V: PlanningIo + Send + ?Sized>(
        &mut self,
        view: &mut V,
        source: AddressIndex,
        withdrawal: HostWithdrawal,
    ) -> Result<NoteManagerPlanningResult> {
        self.plan_wallet_withdrawal(view, source, WalletWithdrawal::Host(withdrawal))
            .await
    }

    async fn plan_wallet_withdrawal<V: PlanningIo + Send + ?Sized>(
        &mut self,
        view: &mut V,
        source: AddressIndex,
        withdrawal: WalletWithdrawal,
    ) -> Result<NoteManagerPlanningResult> {
        let gas_prices = self
            .gas_prices
            .context("note manager instances must call set_gas_prices prior to planning")?;
        let asset_id = withdrawal.asset_id();
        let withdrawal_amount = withdrawal.amount();
        let label = withdrawal.label();

        if let Some(result) = ensure_base_gas_prices(gas_prices) {
            return Ok(result);
        }
        let nullifier_window = view.nullifier_window().await?;

        let mut notes = self.load_notes_for_asset(view, source, asset_id).await?;
        let total_available = notes
            .iter()
            .map(|record| record.note.amount())
            .sum::<Amount>();

        let mut fee = zero_base_fee();

        for _ in 0..4 {
            let required_amount = if gas_prices_are_zero(gas_prices) {
                withdrawal_amount
                    .checked_add(&fee.amount())
                    .ok_or_else(|| anyhow!("{label} amount overflow while planning"))?
            } else {
                withdrawal_amount
            };
            let selected = select_notes_covering(&mut notes, required_amount);
            let selected_total = selected
                .iter()
                .map(|record| record.note.amount())
                .sum::<Amount>();

            if selected_total < required_amount {
                return Ok(NoteManagerPlanningResult::InsufficientBalance);
            }

            let resume_token = withdrawal.resume_token(source);
            let action_needs_maintenance = selected.len() > 2;
            let excluded_fee_notes = selected_note_commitments(&selected);
            let fee_funding_selection = if gas_prices_are_zero(gas_prices) {
                None
            } else {
                Some(
                    self.select_base_fee_funding(view, source, fee, &excluded_fee_notes)
                        .await?,
                )
            };

            if let Some(fee_funding_selection) = fee_funding_selection {
                match fee_funding_selection {
                    BaseFeeFundingSelection::Ready {
                        selected: fee_notes,
                    } => {
                        if action_needs_maintenance {
                            let Some(maintenance_plan) = self
                                .plan_auto_note_reshape_step(view, source, asset_id, selected.len())
                                .await?
                            else {
                                return Ok(NoteManagerPlanningResult::UnsupportedIntent {
                                    reason: format!(
                                        "{label} requires note maintenance for asset {}, but no supported note reshape family is currently applicable",
                                        asset_id
                                    ),
                                });
                            };

                            return Ok(NoteManagerPlanningResult::NeedsMaintenance {
                                maintenance_plan,
                                resume_token,
                            });
                        }

                        let action = self.build_wallet_withdrawal_action(
                            &selected,
                            withdrawal.clone(),
                            zero_base_fee(),
                        )?;
                        let fee_funding = self.build_fee_funding_plan(&fee_notes, fee)?;
                        let actions = vec![action];
                        let new_fee = price_transaction_plan(
                            gas_prices,
                            self.fee_tier,
                            &actions,
                            Some(&fee_funding),
                            nullifier_window,
                        );

                        if new_fee == fee {
                            let plan = self
                                .finalize_wallet_plan(
                                    view,
                                    source,
                                    actions,
                                    Some(fee_funding),
                                    new_fee,
                                    nullifier_window,
                                )
                                .await?;
                            return Ok(NoteManagerPlanningResult::Ready {
                                transaction_plan: plan,
                            });
                        }

                        fee = new_fee;
                        notes = self.load_notes_for_asset(view, source, asset_id).await?;
                        continue;
                    }
                    BaseFeeFundingSelection::NeedsMaintenance { maintenance_plan } => {
                        return Ok(NoteManagerPlanningResult::NeedsMaintenance {
                            maintenance_plan,
                            resume_token,
                        });
                    }
                    BaseFeeFundingSelection::InsufficientBalance => {
                        return Ok(NoteManagerPlanningResult::InsufficientBalance);
                    }
                    BaseFeeFundingSelection::UnsupportedIntent { reason } => {
                        return Ok(NoteManagerPlanningResult::UnsupportedIntent { reason });
                    }
                }
            }

            if action_needs_maintenance {
                if total_available < required_amount {
                    return Ok(NoteManagerPlanningResult::InsufficientBalance);
                }

                let Some(maintenance_plan) = self
                    .plan_auto_note_reshape_step(view, source, asset_id, selected.len())
                    .await?
                else {
                    return Ok(NoteManagerPlanningResult::UnsupportedIntent {
                        reason: format!(
                            "{label} requires note maintenance for asset {}, but no supported note reshape family is currently applicable",
                            asset_id
                        ),
                    });
                };

                return Ok(NoteManagerPlanningResult::NeedsMaintenance {
                    maintenance_plan,
                    resume_token,
                });
            }

            let action = self.build_wallet_withdrawal_action(&selected, withdrawal.clone(), fee)?;
            let actions = vec![action];
            let new_fee =
                price_transaction_plan(gas_prices, self.fee_tier, &actions, None, nullifier_window);

            if new_fee == fee {
                let plan = self
                    .finalize_wallet_plan(view, source, actions, None, new_fee, nullifier_window)
                    .await?;
                return Ok(NoteManagerPlanningResult::Ready {
                    transaction_plan: plan,
                });
            }

            fee = new_fee;
            notes = self.load_notes_for_asset(view, source, asset_id).await?;
        }

        Err(anyhow!("{label} planning did not converge"))
    }

    pub async fn resume_ics20_withdrawal<V: PlanningIo + Send + ?Sized>(
        &mut self,
        view: &mut V,
        resume_token: Ics20WithdrawalResumeToken,
    ) -> Result<NoteManagerPlanningResult> {
        self.plan_ics20_withdrawal(view, resume_token.source, resume_token.withdrawal)
            .await
    }

    pub async fn resume_host_withdrawal<V: PlanningIo + Send + ?Sized>(
        &mut self,
        view: &mut V,
        resume_token: HostWithdrawalResumeToken,
    ) -> Result<NoteManagerPlanningResult> {
        self.plan_host_withdrawal(view, resume_token.source, resume_token.withdrawal)
            .await
    }

    pub async fn resume_note_reshape<V: PlanningIo + Send + ?Sized>(
        &mut self,
        view: &mut V,
        resume_token: NoteReshapeResumeToken,
    ) -> Result<NoteManagerPlanningResult> {
        if let Some(note) = resume_token.note {
            self.plan_note_reshape_from_note(
                view,
                resume_token.source,
                note,
                resume_token.output_amounts,
            )
            .await
        } else {
            self.plan_note_reshape_from_notes(
                view,
                resume_token.source,
                resume_token.asset_id,
                resume_token.family_id,
            )
            .await
        }
    }

    pub async fn plan_note_reshape_from_notes<V: PlanningIo + Send + ?Sized>(
        &mut self,
        view: &mut V,
        source: AddressIndex,
        asset_id: asset::Id,
        family_id: Option<NoteReshapeFamilyId>,
    ) -> Result<NoteManagerPlanningResult> {
        let gas_prices = self
            .gas_prices
            .context("note manager instances must call set_gas_prices prior to planning")?;
        if let Some(result) = ensure_base_gas_prices(gas_prices) {
            return Ok(result);
        }
        let mut notes = self.load_notes_for_asset(view, source, asset_id).await?;
        let family_id = if let Some(family_id) = family_id {
            family_id
        } else if let Some(family_id) = NoteReshapeFamilyId::smallest_covering(
            notes
                .len()
                .min(NoteReshapeFamilyId::EightByOne.max_real_inputs()),
            1,
        ) {
            family_id
        } else {
            return Ok(NoteManagerPlanningResult::UnsupportedIntent {
                reason: format!(
                    "no active note reshape family is applicable for asset {}",
                    asset_id
                ),
            });
        };

        if !family_id.is_many_to_one() {
            return Ok(NoteManagerPlanningResult::UnsupportedIntent {
                reason: format!(
                    "note reshape family {} is not a many-to-one family",
                    family_id.label(),
                ),
            });
        }
        let real_input_count = notes.len().min(family_id.max_real_inputs());
        if real_input_count < family_id.min_real_inputs() {
            return Ok(NoteManagerPlanningResult::UnsupportedIntent {
                reason: format!(
                    "note reshape family {} requires between {} and {} spendable notes, found {}",
                    family_id.label(),
                    family_id.min_real_inputs(),
                    family_id.max_real_inputs(),
                    notes.len()
                ),
            });
        }

        let selected: Vec<_> = (0..real_input_count).filter_map(|_| notes.pop()).collect();
        if gas_prices_are_zero(gas_prices) {
            let Some(plan) = self
                .build_note_reshape_transaction(view, source, asset_id, family_id, selected)
                .await?
            else {
                return Ok(NoteManagerPlanningResult::InsufficientBalance);
            };

            Ok(NoteManagerPlanningResult::Ready {
                transaction_plan: plan,
            })
        } else {
            let sender_address = selected
                .first()
                .map(|record| record.note.address())
                .ok_or_else(|| anyhow!("note reshape requires at least one selected note"))?;
            let total_input = selected
                .iter()
                .map(|record| record.note.amount())
                .sum::<Amount>();
            let spends = selected
                .iter()
                .map(|record| {
                    ShieldedInputPlan::new(&mut self.rng, record.note.clone(), record.position)
                })
                .collect::<Vec<_>>();
            let outputs = vec![ShieldedOutputPlan::new(
                &mut self.rng,
                Value {
                    amount: total_input,
                    asset_id,
                },
                sender_address,
            )];
            let note_reshape = NoteReshapeIntent {
                family_id,
                spends,
                outputs,
                value_blinding: Fr::rand(&mut self.rng),
            };
            let actions = vec![ActionIntent::NoteReshape(note_reshape.clone())];
            self.plan_actions_with_base_fee_funding(
                view,
                source,
                actions.clone(),
                NoteManagerResumeToken::NoteReshape(NoteReshapeResumeToken {
                    source,
                    asset_id,
                    family_id: Some(family_id),
                    note: None,
                    output_amounts: Vec::new(),
                }),
            )
            .await
        }
    }

    pub async fn plan_note_reshape_from_note<V: PlanningIo + Send + ?Sized>(
        &mut self,
        view: &mut V,
        source: AddressIndex,
        note: SpendableNoteRecord,
        output_amounts: Vec<Amount>,
    ) -> Result<NoteManagerPlanningResult> {
        let gas_prices = self
            .gas_prices
            .context("note manager instances must call set_gas_prices prior to planning")?;
        if let Some(result) = ensure_base_gas_prices(gas_prices) {
            return Ok(result);
        }
        let nullifier_window = view.nullifier_window().await?;

        let family_id = NoteReshapeFamilyId::smallest_covering(1, output_amounts.len())
            .ok_or_else(|| {
                anyhow!(
                    "no active note reshape family supports {} outputs",
                    output_amounts.len()
                )
            })?;
        let sender_address = note.note.address();
        let asset_id = note.note.asset_id();
        let outputs_total = output_amounts
            .iter()
            .copied()
            .fold(Amount::zero(), |acc, amount| acc + amount);
        let mut fee = zero_base_fee();

        for _ in 0..4 {
            let required_total = if gas_prices_are_zero(gas_prices) {
                outputs_total
                    .checked_add(&fee.amount())
                    .ok_or_else(|| anyhow!("note reshape amount overflow while planning"))?
            } else {
                outputs_total
            };
            if note.note.amount() < required_total {
                return Ok(NoteManagerPlanningResult::InsufficientBalance);
            }
            if note.note.amount() > required_total {
                return Ok(NoteManagerPlanningResult::UnsupportedIntent {
                    reason: if gas_prices_are_zero(gas_prices) {
                        format!(
                            "note reshape output amounts plus fee must exactly consume the selected note"
                        )
                    } else {
                        format!(
                            "note reshape output amounts must exactly consume the selected note when fees are paid in the base asset"
                        )
                    },
                });
            }

            let spends = vec![ShieldedInputPlan::new(
                &mut self.rng,
                note.note.clone(),
                note.position,
            )];
            let outputs = output_amounts
                .iter()
                .map(|amount| {
                    ShieldedOutputPlan::new(
                        &mut self.rng,
                        Value {
                            amount: *amount,
                            asset_id,
                        },
                        sender_address.clone(),
                    )
                })
                .collect::<Vec<_>>();
            let note_reshape = NoteReshapeIntent {
                family_id,
                spends,
                outputs,
                value_blinding: Fr::rand(&mut self.rng),
            };
            if gas_prices_are_zero(gas_prices) {
                let actions = vec![ActionIntent::NoteReshape(note_reshape.clone())];
                let new_fee = price_transaction_plan(
                    gas_prices,
                    self.fee_tier,
                    &actions,
                    None,
                    nullifier_window,
                );
                if new_fee == fee {
                    let transaction_plan = self
                        .finalize_wallet_plan(
                            view,
                            source,
                            vec![ActionIntent::NoteReshape(note_reshape)],
                            None,
                            new_fee,
                            nullifier_window,
                        )
                        .await?;
                    return Ok(NoteManagerPlanningResult::Ready { transaction_plan });
                }
                fee = new_fee;
            } else {
                let actions = vec![ActionIntent::NoteReshape(note_reshape.clone())];
                return self
                    .plan_actions_with_base_fee_funding(
                        view,
                        source,
                        actions.clone(),
                        NoteManagerResumeToken::NoteReshape(NoteReshapeResumeToken {
                            source,
                            asset_id,
                            family_id: Some(family_id),
                            note: Some(note.clone()),
                            output_amounts: output_amounts.clone(),
                        }),
                    )
                    .await;
            }
        }

        Err(anyhow!("note reshape planning did not converge"))
    }

    async fn plan_auto_note_reshape_step<V: PlanningIo + Send + ?Sized>(
        &mut self,
        view: &mut V,
        source: AddressIndex,
        asset_id: asset::Id,
        selected_note_count: usize,
    ) -> Result<Option<TransactionPlan>> {
        let available_notes = self.load_notes_for_asset(view, source, asset_id).await?;
        let Some(family_id) =
            select_auto_note_reshape_family(selected_note_count, available_notes.len())
        else {
            return Ok(None);
        };

        let mut notes = available_notes;
        let real_input_count = notes.len().min(family_id.max_real_inputs());
        if real_input_count < family_id.min_real_inputs() {
            return Ok(None);
        }
        let selected: Vec<_> = (0..real_input_count).filter_map(|_| notes.pop()).collect();
        self.build_note_reshape_transaction(view, source, asset_id, family_id, selected)
            .await
    }

    async fn build_note_reshape_transaction<V: PlanningIo + Send + ?Sized>(
        &mut self,
        view: &mut V,
        source: AddressIndex,
        asset_id: asset::Id,
        family_id: NoteReshapeFamilyId,
        selected: Vec<SpendableNoteRecord>,
    ) -> Result<Option<TransactionPlan>> {
        let gas_prices = self
            .gas_prices
            .context("note manager instances must call set_gas_prices prior to planning")?;
        let nullifier_window = view.nullifier_window().await?;
        let sender_address = selected
            .first()
            .map(|record| record.note.address())
            .ok_or_else(|| anyhow!("note reshape requires at least one selected note"))?;
        let mut fee = zero_base_fee();

        for _ in 0..4 {
            let total_input = selected
                .iter()
                .map(|record| record.note.amount())
                .sum::<Amount>();
            if total_input <= fee.amount() {
                return Ok(None);
            }
            let output_value = Value {
                amount: total_input - fee.amount(),
                asset_id,
            };
            let spends = selected
                .iter()
                .map(|record| {
                    ShieldedInputPlan::new(&mut self.rng, record.note.clone(), record.position)
                })
                .collect::<Vec<_>>();
            let outputs = vec![ShieldedOutputPlan::new(
                &mut self.rng,
                output_value,
                sender_address.clone(),
            )];
            let note_reshape = NoteReshapeIntent {
                family_id,
                spends,
                outputs,
                value_blinding: Fr::rand(&mut self.rng),
            };
            let actions = vec![ActionIntent::NoteReshape(note_reshape.clone())];
            let new_fee =
                price_transaction_plan(gas_prices, self.fee_tier, &actions, None, nullifier_window);
            if new_fee == fee {
                let plan = self
                    .finalize_wallet_plan(
                        view,
                        source,
                        vec![ActionIntent::NoteReshape(note_reshape)],
                        None,
                        new_fee,
                        nullifier_window,
                    )
                    .await?;
                return Ok(Some(plan));
            }
            fee = new_fee;
        }

        Err(anyhow!("note reshape planning did not converge"))
    }

    fn build_transfer_plan(
        &mut self,
        selected: &[SpendableNoteRecord],
        recipient: Address,
        value: Value,
        fee: Fee,
    ) -> Result<TransferIntent> {
        let sender_address = selected
            .first()
            .map(|record| record.note.address())
            .ok_or_else(|| anyhow!("transfer requires at least one selected note"))?;
        let total_input = selected
            .iter()
            .map(|record| record.note.amount())
            .sum::<Amount>();
        let total_required = value
            .amount
            .checked_add(&fee.amount())
            .ok_or_else(|| anyhow!("transfer amount overflow while building"))?;
        if total_input < total_required {
            anyhow::bail!("selected notes do not cover transfer amount and fee");
        }

        let change_amount = total_input - total_required;
        let spends = selected
            .iter()
            .map(|record| {
                ShieldedInputPlan::new(&mut self.rng, record.note.clone(), record.position)
            })
            .collect::<Vec<_>>();

        let mut outputs = vec![ShieldedOutputPlan::new(&mut self.rng, value, recipient)];
        if change_amount > Amount::zero() {
            outputs.push(ShieldedOutputPlan::new(
                &mut self.rng,
                Value {
                    amount: change_amount,
                    asset_id: value.asset_id,
                },
                sender_address,
            ));
        }

        Ok(TransferIntent {
            spends,
            outputs,
            value_blinding: Fr::rand(&mut self.rng),
        })
    }

    fn build_self_funded_transfer_plan(
        &mut self,
        selected: &[SpendableNoteRecord],
        asset_id: asset::Id,
        fee: Fee,
    ) -> Result<TransferIntent> {
        let sender_address = selected
            .first()
            .map(|record| record.note.address())
            .ok_or_else(|| anyhow!("transfer-funded action planning requires at least one note"))?;

        let total_input = selected
            .iter()
            .map(|record| record.note.amount())
            .sum::<Amount>();

        anyhow::ensure!(
            total_input >= fee.amount(),
            "transfer-funded action planning requires notes covering the fee"
        );

        let change_amount = total_input - fee.amount();
        let spends = selected
            .iter()
            .map(|record| {
                ShieldedInputPlan::new(&mut self.rng, record.note.clone(), record.position)
            })
            .collect::<Vec<_>>();
        let outputs = vec![ShieldedOutputPlan::new(
            &mut self.rng,
            Value {
                amount: change_amount,
                asset_id,
            },
            sender_address,
        )];

        Ok(TransferIntent {
            spends,
            outputs,
            value_blinding: Fr::rand(&mut self.rng),
        })
    }

    fn build_fee_funding_plan(
        &mut self,
        selected: &[SpendableNoteRecord],
        fee: Fee,
    ) -> Result<TransferIntent> {
        self.build_self_funded_transfer_plan(selected, *BASE_ASSET_ID, fee)
    }

    async fn select_base_fee_funding<V: PlanningIo + Send + ?Sized>(
        &mut self,
        view: &mut V,
        source: AddressIndex,
        fee: Fee,
        excluded_note_commitments: &BTreeSet<note::StateCommitment>,
    ) -> Result<BaseFeeFundingSelection> {
        anyhow::ensure!(
            fee.asset_id() == *BASE_ASSET_ID,
            "fee funding requires base-asset fees, found {}",
            fee.asset_id(),
        );

        let minimum_total = if fee.amount() == Amount::zero() {
            Amount::from(1u64)
        } else {
            fee.amount()
        };

        let mut notes = self
            .load_notes_for_asset(view, source, *BASE_ASSET_ID)
            .await?
            .into_iter()
            .filter(|record| !excluded_note_commitments.contains(&record.note_commitment))
            .collect::<Vec<_>>();
        let selected = select_notes_covering(&mut notes, minimum_total);
        let selected_total = selected
            .iter()
            .map(|record| record.note.amount())
            .sum::<Amount>();

        if selected_total < minimum_total {
            return Ok(BaseFeeFundingSelection::InsufficientBalance);
        }

        if selected.len() > 2 {
            let Some(maintenance_plan) = self
                .plan_auto_note_reshape_step(view, source, *BASE_ASSET_ID, selected.len())
                .await?
            else {
                return Ok(BaseFeeFundingSelection::UnsupportedIntent {
                    reason: format!(
                        "base-asset fee funding requires note maintenance, but no supported note reshape family is currently applicable"
                    ),
                });
            };

            return Ok(BaseFeeFundingSelection::NeedsMaintenance { maintenance_plan });
        }

        Ok(BaseFeeFundingSelection::Ready { selected })
    }

    async fn plan_actions_with_base_fee_funding<V: PlanningIo + Send + ?Sized>(
        &mut self,
        view: &mut V,
        source: AddressIndex,
        primary_actions: Vec<ActionIntent>,
        resume_token: NoteManagerResumeToken,
    ) -> Result<NoteManagerPlanningResult> {
        let gas_prices = self
            .gas_prices
            .context("note manager instances must call set_gas_prices prior to planning")?;
        if let Some(result) = ensure_base_gas_prices(gas_prices) {
            return Ok(result);
        }
        let nullifier_window = view.nullifier_window().await?;

        if gas_prices_are_zero(gas_prices) {
            let plan = self
                .finalize_wallet_plan(
                    view,
                    source,
                    primary_actions,
                    None,
                    zero_base_fee(),
                    nullifier_window,
                )
                .await?;
            return Ok(NoteManagerPlanningResult::Ready {
                transaction_plan: plan,
            });
        }

        let excluded_fee_notes = fee_funding_excluded_note_commitments(&primary_actions);
        let mut fee = zero_base_fee();

        for _ in 0..4 {
            let fee_funding = self
                .select_base_fee_funding(view, source, fee, &excluded_fee_notes)
                .await?;
            let selected_fee_notes = match fee_funding {
                BaseFeeFundingSelection::Ready { selected } => selected,
                BaseFeeFundingSelection::NeedsMaintenance { maintenance_plan } => {
                    return Ok(NoteManagerPlanningResult::NeedsMaintenance {
                        maintenance_plan,
                        resume_token,
                    });
                }
                BaseFeeFundingSelection::InsufficientBalance => {
                    return Ok(NoteManagerPlanningResult::InsufficientBalance);
                }
                BaseFeeFundingSelection::UnsupportedIntent { reason } => {
                    return Ok(NoteManagerPlanningResult::UnsupportedIntent { reason });
                }
            };

            let fee_funding_plan = self.build_fee_funding_plan(&selected_fee_notes, fee)?;
            let new_fee = price_transaction_plan(
                gas_prices,
                self.fee_tier,
                &primary_actions,
                Some(&fee_funding_plan),
                nullifier_window,
            );
            if new_fee == fee {
                let plan = self
                    .finalize_wallet_plan(
                        view,
                        source,
                        primary_actions,
                        Some(fee_funding_plan),
                        new_fee,
                        nullifier_window,
                    )
                    .await?;
                return Ok(NoteManagerPlanningResult::Ready {
                    transaction_plan: plan,
                });
            }

            fee = new_fee;
        }

        Err(anyhow!("fee-funded action planning did not converge"))
    }

    fn build_wallet_withdrawal_action(
        &mut self,
        selected: &[SpendableNoteRecord],
        withdrawal: WalletWithdrawal,
        fee: Fee,
    ) -> Result<ActionIntent> {
        let label = withdrawal.label();
        let withdrawal_amount = withdrawal.amount();
        let asset_id = withdrawal.asset_id();
        let sender_address = selected
            .first()
            .map(|record| record.note.address())
            .ok_or_else(|| anyhow!("{label} requires at least one selected note"))?;
        let total_input = selected
            .iter()
            .map(|record| record.note.amount())
            .sum::<Amount>();
        let total_required = withdrawal_amount
            .checked_add(&fee.amount())
            .ok_or_else(|| anyhow!("{label} amount overflow while building"))?;
        if total_input < total_required {
            anyhow::bail!("selected notes do not cover {label} amount and fee");
        }

        let change_amount = total_input - total_required;
        let spends = selected
            .iter()
            .map(|record| {
                ShieldedInputPlan::new(&mut self.rng, record.note.clone(), record.position)
            })
            .collect::<Vec<_>>();

        let change_output = if change_amount > Amount::zero() {
            Some(ShieldedOutputPlan::new(
                &mut self.rng,
                Value {
                    amount: change_amount,
                    asset_id,
                },
                sender_address,
            ))
        } else {
            None
        };
        let value_blinding = Fr::rand(&mut self.rng);
        Ok(match withdrawal {
            WalletWithdrawal::Ics20(withdrawal) => {
                ActionIntent::Ics20Withdrawal(WithdrawalIntent {
                    spends,
                    change_output,
                    withdrawal,
                    value_blinding,
                })
            }
            WalletWithdrawal::Host(withdrawal) => ActionIntent::HostWithdrawal(WithdrawalIntent {
                spends,
                change_output,
                withdrawal,
                value_blinding,
            }),
        })
    }

    async fn load_notes_for_asset<V: PlanningIo + Send + ?Sized>(
        &mut self,
        view: &mut V,
        source: AddressIndex,
        asset_id: asset::Id,
    ) -> Result<Vec<SpendableNoteRecord>> {
        let records = view
            .notes(NotesRequest {
                include_spent: false,
                asset_id: Some(asset_id.into()),
                address_index: Some(source.into()),
                amount_to_spend: None,
            })
            .await?;
        Ok(prioritize_and_filter_spendable_notes(records))
    }

    async fn finalize_wallet_plan<V: PlanningIo + Send + ?Sized>(
        &mut self,
        view: &mut V,
        source: AddressIndex,
        actions: Vec<ActionIntent>,
        fee_funding: Option<TransferIntent>,
        fee: Fee,
        nullifier_window: NullifierWindow,
    ) -> Result<TransactionPlan> {
        let mut transaction_parameters = self.transaction_parameters.clone();
        transaction_parameters.fee = fee;
        transaction_parameters.chain_id = view.chain_id().await?;

        let mut intent = TransactionIntent {
            actions,
            transaction_parameters,
            fee_funding,
            memo: None,
            nullifier_window: Some(nullifier_window),
        };

        if intent.has_outputs() {
            let return_address = if let Some(ref address) = self.memo_return_address {
                anyhow::ensure!(
                    view.index_by_address(address.clone()).await?.is_some(),
                    "return address for memo is not controlled by the user",
                );
                address.clone()
            } else {
                view.address_by_index(source).await?
            };

            intent.memo = Some(MemoPlan::new(
                &mut self.rng,
                MemoPlaintext::new(return_address, self.memo_text.clone().unwrap_or_default())
                    .context("could not create memo plaintext")?,
            ));
        }

        let discovery_params = view.discovery_parameters().await?;
        complete_plan_with_compliance(
            intent,
            |queries| view.compliance_data(queries),
            &mut self.rng,
            discovery_params,
            None,
        )
        .await
    }
}

fn gas_prices_are_zero(gas_prices: GasPrices) -> bool {
    gas_prices.block_space_price == 0
        && gas_prices.compact_block_space_price == 0
        && gas_prices.verification_price == 0
        && gas_prices.execution_price == 0
}

fn zero_base_fee() -> Fee {
    Fee::from_staking_token_amount(Amount::zero())
}

fn ensure_base_gas_prices(gas_prices: GasPrices) -> Option<NoteManagerPlanningResult> {
    if gas_prices.asset_id == *BASE_ASSET_ID {
        None
    } else {
        Some(NoteManagerPlanningResult::UnsupportedIntent {
            reason: format!(
                "wallet-facing planning only supports base-asset gas, got fee asset {}",
                gas_prices.asset_id
            ),
        })
    }
}

fn collapse_transfer_values(values: Vec<Value>) -> Result<Option<Value>> {
    let mut iter = values.into_iter();
    let Some(first) = iter.next() else {
        return Ok(None);
    };

    let mut total_amount = first.amount;
    for value in iter {
        if value.asset_id != first.asset_id {
            return Ok(None);
        }
        total_amount = total_amount
            .checked_add(&value.amount)
            .ok_or_else(|| anyhow!("transfer amount overflow while combining values"))?;
    }

    Ok(Some(Value {
        amount: total_amount,
        asset_id: first.asset_id,
    }))
}

fn selected_note_commitments(selected: &[SpendableNoteRecord]) -> BTreeSet<note::StateCommitment> {
    selected
        .iter()
        .map(|record| record.note_commitment)
        .collect()
}

fn fee_funding_excluded_note_commitments(
    actions: &[ActionIntent],
) -> BTreeSet<note::StateCommitment> {
    let mut commitments = BTreeSet::new();
    for action in actions {
        commitments.extend(
            action
                .spends()
                .iter()
                .filter(|spend| spend.note.asset_id() == *BASE_ASSET_ID)
                .map(|spend| spend.note.commit()),
        );
    }
    commitments
}

fn select_auto_note_reshape_family(
    selected_note_count: usize,
    available_note_count: usize,
) -> Option<NoteReshapeFamilyId> {
    let family = NoteReshapeFamilyId::EightByOne;
    (selected_note_count > 2 && available_note_count >= family.min_real_inputs()).then_some(family)
}

fn prioritize_and_filter_spendable_notes(
    records: Vec<SpendableNoteRecord>,
) -> Vec<SpendableNoteRecord> {
    let mut filtered = records
        .into_iter()
        .filter(|record| record.note.amount() > Amount::zero())
        .collect::<Vec<_>>();
    filtered.sort_by(|a, b| {
        match (
            a.address_index.is_ephemeral(),
            b.address_index.is_ephemeral(),
        ) {
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            _ => a.note.amount().cmp(&b.note.amount()),
        }
    });
    filtered
}

fn select_notes_covering(
    notes: &mut Vec<SpendableNoteRecord>,
    required_amount: Amount,
) -> Vec<SpendableNoteRecord> {
    let mut selected = Vec::new();
    let mut total = Amount::zero();

    while total < required_amount {
        let Some(note) = notes.pop() else {
            break;
        };
        total += note.note.amount();
        selected.push(note);
    }

    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    use decaf377::Fq;
    use ibc_types::core::{channel::ChannelId, client::Height as IbcHeight};
    use rand_core::OsRng;
    use shieldd_sdk_asset::BASE_ASSET_ID;
    use shieldd_sdk_fee::GasPrices;
    use shieldd_sdk_ibc::IbcRelay;
    use shieldd_sdk_keys::keys::SeedPhrase;
    use shieldd_sdk_keys::keys::{AddressIndex, Bip44Path, SpendKey};
    use shieldd_sdk_keys::symmetric::PayloadKey;
    use shieldd_sdk_proto::view::v1 as pb;
    use shieldd_sdk_sct::{CommitmentSource, Nullifier};
    use shieldd_sdk_shielded_pool::{
        discovery, HostTransfer, HostWithdrawalDestination, Note, Rseed,
    };
    use shieldd_sdk_transaction::gas::GasCost;
    use shieldd_sdk_transaction::plan::ActionPlan;
    use std::collections::BTreeMap;
    use std::str::FromStr;
    use std::sync::{Arc, Mutex};

    fn test_address(index: u32) -> Address {
        test_spend_key(index)
            .full_viewing_key()
            .incoming()
            .payment_address(index.into())
    }

    fn test_spend_key(index: u32) -> SpendKey {
        let seed = SeedPhrase::from_randomness(&[index as u8; 32]);
        SpendKey::from_seed_phrase_bip44(seed, &Bip44Path::new(0))
            .expect("test spend key should satisfy key refinements")
    }

    fn spendable_note_record(
        rng: &mut OsRng,
        amount: u64,
        address_index: AddressIndex,
        address: Address,
        position: u64,
    ) -> SpendableNoteRecord {
        spendable_note_record_with_asset(
            rng,
            amount,
            *BASE_ASSET_ID,
            address_index,
            address,
            position,
        )
    }

    fn spendable_note_record_with_asset(
        rng: &mut OsRng,
        amount: u64,
        asset_id: asset::Id,
        address_index: AddressIndex,
        address: Address,
        position: u64,
    ) -> SpendableNoteRecord {
        let note = Note::from_parts(
            address,
            Value {
                amount: amount.into(),
                asset_id,
            },
            Rseed::generate(rng),
        )
        .expect("valid test note");

        SpendableNoteRecord {
            note_commitment: note.commit(),
            note,
            address_index,
            nullifier: Nullifier(Fq::from(position)),
            height_created: 0,
            height_spent: None,
            position: position.into(),
            source: CommitmentSource::Genesis,
            return_address: None,
        }
    }

    fn test_ics20_withdrawal(amount: u64, return_address: Address) -> Ics20Withdrawal {
        Ics20Withdrawal {
            amount: amount.into(),
            denom: shieldd_sdk_asset::asset::Metadata::try_from("ushieldd")
                .expect("valid base-asset metadata"),
            destination_chain_address: "cosmos1deadbeefdeadbeefdeadbeefdeadbeef7a8n3x".to_string(),
            return_address,
            timeout_height: IbcHeight {
                revision_number: 0,
                revision_height: 10,
            },
            timeout_time: 60_000_000_000,
            source_channel: ChannelId::from_str("channel-0").expect("valid channel id"),
            ics20_memo: String::new(),
            use_transparent_address: false,
        }
    }

    fn test_host_withdrawal(amount: u64, recipient: &str) -> HostWithdrawal {
        HostWithdrawal {
            value: Value {
                amount: amount.into(),
                asset_id: *BASE_ASSET_ID,
            },
            destination: HostWithdrawalDestination::Transfer(HostTransfer {
                recipient: recipient.to_owned(),
            }),
        }
    }

    fn test_ibc_action() -> IbcRelay {
        IbcRelay::Unknown(pbjson_types::Any {
            type_url: "/shieldd.test.ibc".to_owned(),
            value: vec![1, 2, 3].into(),
        })
    }

    fn assert_action_only_plan(
        transaction_plan: TransactionPlan,
        expected_action: fn(&ActionPlan) -> bool,
    ) {
        assert_eq!(transaction_plan.actions.len(), 1);
        assert!(expected_action(transaction_plan.actions.first().expect(
            "wallet-facing action planned without funding transfer"
        )));
    }

    struct MockNoteManagerView {
        notes: Arc<Mutex<Vec<SpendableNoteRecord>>>,
        addresses: BTreeMap<AddressIndex, Address>,
        nullifier_window: NullifierWindow,
    }

    impl MockNoteManagerView {
        fn new(
            notes: Vec<SpendableNoteRecord>,
            addresses: BTreeMap<AddressIndex, Address>,
        ) -> Self {
            Self {
                notes: Arc::new(Mutex::new(notes)),
                addresses,
                nullifier_window: NullifierWindow {
                    protocol_version: shieldd_sdk_sct::nullifier_generation::PROTOCOL_VERSION,
                    current_generation: 0,
                    recent_position_floor: 0,
                    archived_generation_count: 0,
                    archived_history_head:
                        shieldd_sdk_sct::nullifier_generation::empty_history_head(),
                },
            }
        }

        fn with_nullifier_window(mut self, nullifier_window: NullifierWindow) -> Self {
            self.nullifier_window = nullifier_window;
            self
        }

        fn replace_notes(&self, notes: Vec<SpendableNoteRecord>) {
            *self.notes.lock().expect("notes mutex") = notes;
        }
    }

    #[async_trait::async_trait]
    impl PlanningIo for MockNoteManagerView {
        async fn chain_id(&mut self) -> Result<String> {
            Ok("test-chain".to_owned())
        }
        async fn nullifier_window(&mut self) -> Result<NullifierWindow> {
            Ok(self.nullifier_window)
        }
        async fn discovery_parameters(&mut self) -> Result<discovery::Parameters> {
            Ok(Default::default())
        }
        async fn notes(&mut self, request: pb::NotesRequest) -> Result<Vec<SpendableNoteRecord>> {
            let asset: Option<asset::Id> = request.asset_id.map(TryInto::try_into).transpose()?;
            let index: Option<AddressIndex> =
                request.address_index.map(TryInto::try_into).transpose()?;
            Ok(self
                .notes
                .lock()
                .expect("notes mutex")
                .iter()
                .filter(|record| {
                    asset.map_or(true, |asset| record.note.asset_id() == asset)
                        && index.map_or(true, |index| record.address_index == index)
                })
                .cloned()
                .collect())
        }
        async fn address_by_index(&mut self, index: AddressIndex) -> Result<Address> {
            self.addresses
                .get(&index)
                .cloned()
                .context("unknown fixture address")
        }
        async fn index_by_address(&mut self, address: Address) -> Result<Option<AddressIndex>> {
            Ok(self
                .addresses
                .iter()
                .find_map(|(index, known)| (*known == address).then_some(*index)))
        }
        async fn compliance_data(
            &mut self,
            queries: Vec<ComplianceQuery>,
        ) -> Result<shieldd_sdk_compliance::BatchComplianceData> {
            use shieldd_sdk_compliance::{
                AssetProofData, BatchComplianceData, ComplianceLeaf, MerklePath, UserProofData,
            };
            let (root, _, _, _) = shieldd_sdk_compliance::create_default_imt_proof(BASE_ASSET_ID.0);
            let mut batch = BatchComplianceData {
                asset_anchor: root,
                ..Default::default()
            };
            for ComplianceQuery { address, asset_id } in queries {
                let (asset_root, indexed_leaf, auth_path, position) =
                    shieldd_sdk_compliance::create_default_imt_proof(asset_id.0);
                assert_eq!(asset_root, root);
                batch.asset_proofs.insert(
                    asset_id,
                    AssetProofData {
                        auth_path,
                        position,
                        indexed_leaf,
                        is_regulated: false,
                    },
                );
                batch.user_proofs.insert(
                    (address.clone(), asset_id),
                    UserProofData {
                        auth_path: MerklePath::default(),
                        position: 0,
                        leaf: ComplianceLeaf::synthetic_unregulated(address, asset_id),
                    },
                );
            }
            Ok(batch)
        }
    }

    #[tokio::test]
    async fn transfer_ready_produces_wallet_facing_transfer_only() {
        let mut rng = OsRng;
        let source = AddressIndex::new(0);
        let sender = test_address(0);
        let recipient = test_address(1);
        let view_addresses = BTreeMap::from([(source, sender.clone())]);
        let notes = vec![spendable_note_record(&mut rng, 10, source, sender, 1)];
        let mut view = MockNoteManagerView::new(notes, view_addresses);

        let mut note_manager = NoteManager::new(OsRng);
        note_manager.set_gas_prices(GasPrices::zero());

        let result = note_manager
            .plan_transfer(
                &mut view,
                source,
                Value {
                    amount: 5u64.into(),
                    asset_id: *BASE_ASSET_ID,
                },
                recipient,
            )
            .await
            .expect("transfer planning succeeds");

        let NoteManagerPlanningResult::Ready { transaction_plan } = result else {
            panic!("expected ready transfer plan");
        };
        assert_eq!(transaction_plan.actions.len(), 1);
        assert!(matches!(
            transaction_plan.actions.first(),
            Some(ActionPlan::Transfer(_))
        ));
    }

    #[tokio::test]
    async fn fragmented_transfer_requests_note_reshape_then_resume_builds_transfer() {
        let mut rng = OsRng;
        let source = AddressIndex::new(0);
        let sender = test_address(2);
        let recipient = test_address(3);
        let view_addresses = BTreeMap::from([(source, sender.clone())]);
        let notes = vec![
            spendable_note_record(&mut rng, 4, source, sender.clone(), 1),
            spendable_note_record(&mut rng, 3, source, sender.clone(), 2),
            spendable_note_record(&mut rng, 2, source, sender.clone(), 3),
            spendable_note_record(&mut rng, 1, source, sender.clone(), 4),
        ];
        let mut view = MockNoteManagerView::new(notes, view_addresses);
        let mut note_manager = NoteManager::new(OsRng);
        note_manager.set_gas_prices(GasPrices::zero());

        let result = note_manager
            .plan_transfer(
                &mut view,
                source,
                Value {
                    amount: 10u64.into(),
                    asset_id: *BASE_ASSET_ID,
                },
                recipient.clone(),
            )
            .await
            .expect("transfer planning succeeds");

        let (maintenance_plan, resume_token) = match result {
            NoteManagerPlanningResult::NeedsMaintenance {
                maintenance_plan,
                resume_token,
            } => (maintenance_plan, resume_token),
            _ => panic!("expected maintenance result"),
        };
        assert!(matches!(
            maintenance_plan.actions.first(),
            Some(ActionPlan::NoteReshape(note_reshape))
                if note_reshape.family_id() == NoteReshapeFamilyId::EightByOne
        ));

        view.replace_notes(vec![spendable_note_record(
            &mut rng,
            10,
            source,
            test_address(2),
            5,
        )]);

        let resumed = note_manager
            .resume(&mut view, resume_token)
            .await
            .expect("resume succeeds");

        let NoteManagerPlanningResult::Ready { transaction_plan } = resumed else {
            panic!("expected resumed transfer to be ready");
        };
        assert!(matches!(
            transaction_plan.actions.first(),
            Some(ActionPlan::Transfer(_))
        ));
    }

    #[tokio::test]
    async fn ics20_withdrawal_ready_produces_wallet_facing_action_only() {
        let mut rng = OsRng;
        let source = AddressIndex::new(0);
        let sender = test_address(20);
        let return_address = test_address(21);
        let view_addresses = BTreeMap::from([(source, sender.clone())]);
        let notes = vec![
            spendable_note_record(&mut rng, 7, source, sender.clone(), 1),
            spendable_note_record(&mut rng, 5, source, sender, 2),
        ];
        let mut view = MockNoteManagerView::new(notes, view_addresses);

        let mut note_manager = NoteManager::new(OsRng);
        note_manager.set_gas_prices(GasPrices::zero());

        let result = note_manager
            .plan_ics20_withdrawal(&mut view, source, test_ics20_withdrawal(10, return_address))
            .await
            .expect("ICS-20 withdrawal planning succeeds");

        let NoteManagerPlanningResult::Ready { transaction_plan } = result else {
            panic!("expected ready ICS-20 withdrawal plan");
        };
        assert_eq!(transaction_plan.actions.len(), 1);
        assert!(matches!(
            transaction_plan.actions.first(),
            Some(ActionPlan::ShieldedIcs20Withdrawal(_))
        ));
    }

    #[tokio::test]
    async fn fragmented_ics20_withdrawal_requests_note_reshape_then_resume_builds_action() {
        let mut rng = OsRng;
        let source = AddressIndex::new(0);
        let sender = test_address(22);
        let return_address = test_address(23);
        let view_addresses = BTreeMap::from([(source, sender.clone())]);
        let notes = vec![
            spendable_note_record(&mut rng, 4, source, sender.clone(), 1),
            spendable_note_record(&mut rng, 3, source, sender.clone(), 2),
            spendable_note_record(&mut rng, 2, source, sender.clone(), 3),
            spendable_note_record(&mut rng, 1, source, sender.clone(), 4),
        ];
        let mut view = MockNoteManagerView::new(notes, view_addresses);
        let mut note_manager = NoteManager::new(OsRng);
        note_manager.set_gas_prices(GasPrices::zero());

        let result = note_manager
            .plan_ics20_withdrawal(&mut view, source, test_ics20_withdrawal(10, return_address))
            .await
            .expect("ICS-20 withdrawal planning succeeds");

        let (maintenance_plan, resume_token) = match result {
            NoteManagerPlanningResult::NeedsMaintenance {
                maintenance_plan,
                resume_token,
            } => (maintenance_plan, resume_token),
            _ => panic!("expected maintenance result"),
        };
        assert!(matches!(
            maintenance_plan.actions.first(),
            Some(ActionPlan::NoteReshape(note_reshape))
                if note_reshape.family_id() == NoteReshapeFamilyId::EightByOne
        ));

        view.replace_notes(vec![spendable_note_record(
            &mut rng,
            10,
            source,
            test_address(22),
            5,
        )]);

        let resumed = note_manager
            .resume(&mut view, resume_token)
            .await
            .expect("resume succeeds");

        let NoteManagerPlanningResult::Ready { transaction_plan } = resumed else {
            panic!("expected resumed ICS-20 withdrawal to be ready");
        };
        assert!(matches!(
            transaction_plan.actions.first(),
            Some(ActionPlan::ShieldedIcs20Withdrawal(_))
        ));
    }

    #[tokio::test]
    async fn host_withdrawal_ready_produces_wallet_facing_action_only() {
        let mut rng = OsRng;
        let source = AddressIndex::new(0);
        let sender = test_address(24);
        let view_addresses = BTreeMap::from([(source, sender.clone())]);
        let notes = vec![
            spendable_note_record(&mut rng, 7, source, sender.clone(), 1),
            spendable_note_record(&mut rng, 5, source, sender, 2),
        ];
        let mut view = MockNoteManagerView::new(notes, view_addresses);
        let withdrawal = test_host_withdrawal(10, "bankd1recipient");

        let mut note_manager = NoteManager::new(OsRng);
        note_manager.set_gas_prices(GasPrices::zero());

        let result = note_manager
            .plan_host_withdrawal(&mut view, source, withdrawal.clone())
            .await
            .expect("host withdrawal planning succeeds");

        let NoteManagerPlanningResult::Ready { transaction_plan } = result else {
            panic!("expected ready host withdrawal plan");
        };
        assert_eq!(transaction_plan.actions.len(), 1);
        let Some(ActionPlan::ShieldedHostWithdrawal(plan)) = transaction_plan.actions.first()
        else {
            panic!("expected shielded host withdrawal action");
        };
        assert_eq!(plan.withdrawal, withdrawal);
        assert_eq!(plan.spends.len(), 2);
        assert_eq!(
            plan.change_output
                .as_ref()
                .expect("withdrawal should create change")
                .value
                .amount,
            2u64.into()
        );
    }

    #[tokio::test]
    async fn fragmented_host_withdrawal_requests_note_reshape_then_resume_builds_action() {
        let mut rng = OsRng;
        let source = AddressIndex::new(0);
        let sender = test_address(25);
        let view_addresses = BTreeMap::from([(source, sender.clone())]);
        let notes = vec![
            spendable_note_record(&mut rng, 4, source, sender.clone(), 1),
            spendable_note_record(&mut rng, 3, source, sender.clone(), 2),
            spendable_note_record(&mut rng, 2, source, sender.clone(), 3),
            spendable_note_record(&mut rng, 1, source, sender.clone(), 4),
        ];
        let mut view = MockNoteManagerView::new(notes, view_addresses);
        let withdrawal = test_host_withdrawal(10, "bankd1recipient");
        let mut note_manager = NoteManager::new(OsRng);
        note_manager.set_gas_prices(GasPrices::zero());

        let result = note_manager
            .plan_host_withdrawal(&mut view, source, withdrawal.clone())
            .await
            .expect("host withdrawal planning succeeds");

        let (maintenance_plan, resume_token) = match result {
            NoteManagerPlanningResult::NeedsMaintenance {
                maintenance_plan,
                resume_token,
            } => (maintenance_plan, resume_token),
            _ => panic!("expected maintenance result"),
        };
        assert!(matches!(
            maintenance_plan.actions.first(),
            Some(ActionPlan::NoteReshape(note_reshape))
                if note_reshape.family_id() == NoteReshapeFamilyId::EightByOne
        ));
        assert!(matches!(
            &resume_token,
            NoteManagerResumeToken::HostWithdrawal(_)
        ));

        view.replace_notes(vec![spendable_note_record(
            &mut rng,
            10,
            source,
            test_address(25),
            5,
        )]);

        let resumed = note_manager
            .resume(&mut view, resume_token)
            .await
            .expect("resume succeeds");

        let NoteManagerPlanningResult::Ready { transaction_plan } = resumed else {
            panic!("expected resumed host withdrawal to be ready");
        };
        let Some(ActionPlan::ShieldedHostWithdrawal(plan)) = transaction_plan.actions.first()
        else {
            panic!("expected shielded host withdrawal action");
        };
        assert_eq!(plan.withdrawal, withdrawal);
    }

    #[tokio::test]
    async fn zero_fee_ibc_action_plans_without_funding_transfer() {
        let source = AddressIndex::new(0);
        let sender = test_address(32);
        let view_addresses = BTreeMap::from([(source, sender.clone())]);
        let mut view = MockNoteManagerView::new(vec![], view_addresses);

        let mut note_manager = NoteManager::new(OsRng);
        note_manager.set_gas_prices(GasPrices::zero());

        let result = note_manager
            .plan_actions_with_transfer_funding(
                &mut view,
                source,
                vec![ActionPlan::IbcAction(test_ibc_action())],
            )
            .await
            .expect("IBC relay planning succeeds");

        let NoteManagerPlanningResult::Ready { transaction_plan } = result else {
            panic!("expected ready IBC relay plan");
        };
        assert_action_only_plan(transaction_plan, |action| {
            matches!(action, ActionPlan::IbcAction(_))
        });
    }

    #[tokio::test]
    async fn zero_fee_compliance_register_user_plans_without_funding_transfer() {
        let source = AddressIndex::new(1);
        let address = test_address(34);
        let view_addresses = BTreeMap::from([(source, address.clone())]);
        let mut view = MockNoteManagerView::new(vec![], view_addresses);

        let leaf = shieldd_sdk_compliance::ComplianceLeaf::new(address, *BASE_ASSET_ID);
        let msg = shieldd_sdk_compliance::structs::MsgRegisterUser { leaf, grant: None };

        let mut note_manager = NoteManager::new(OsRng);
        note_manager.set_gas_prices(GasPrices::zero());

        let result = note_manager
            .plan_actions_with_transfer_funding(
                &mut view,
                source,
                vec![ActionPlan::ComplianceRegisterUser(msg)],
            )
            .await
            .expect("compliance register-user planning succeeds");

        let NoteManagerPlanningResult::Ready { transaction_plan } = result else {
            panic!("expected ready compliance register-user plan");
        };
        assert_action_only_plan(transaction_plan, |action| {
            matches!(action, ActionPlan::ComplianceRegisterUser(_))
        });
    }

    #[tokio::test]
    async fn exact_fee_generic_funding_builds_zero_value_fee_change() {
        let source = AddressIndex::new(0);
        let sender = test_address(34);
        let gas_prices = GasPrices {
            block_space_price: 0,
            compact_block_space_price: 0,
            verification_price: 0,
            execution_price: 1_000,
            asset_id: *BASE_ASSET_ID,
        };
        let action = ActionPlan::IbcAction(test_ibc_action());
        let exact_fee = gas_prices
            .fee(&(action.gas_cost() + shieldd_sdk_transaction::gas::transfer_gas_cost()))
            .apply_tier(FeeTier::default());
        let amount = exact_fee.amount();

        let notes = vec![spendable_note_record(
            &mut OsRng,
            u128::from(amount) as u64,
            source,
            sender.clone(),
            u128::from(amount) as u64,
        )];
        let view_addresses = BTreeMap::from([(source, sender)]);
        let mut view = MockNoteManagerView::new(notes, view_addresses);
        let mut note_manager = NoteManager::new(OsRng);
        note_manager.set_gas_prices(gas_prices);

        let result = note_manager
            .plan_actions_with_transfer_funding(&mut view, source, vec![action.clone()])
            .await
            .expect("planning should not error");

        let NoteManagerPlanningResult::Ready { transaction_plan } = result else {
            panic!("expected exact-fee funding plan to succeed");
        };
        assert_eq!(transaction_plan.transaction_parameters.fee, exact_fee);
        assert_eq!(
            gas_prices
                .fee(&transaction_plan.gas_cost())
                .apply_tier(FeeTier::default()),
            exact_fee,
            "wallet and consensus fee inputs must cover the same complete plan",
        );
        assert_action_only_plan(transaction_plan.clone(), |planned_action| {
            matches!(planned_action, ActionPlan::IbcAction(_))
        });
        let fee_funding = transaction_plan
            .fee_funding
            .expect("nonzero-fee transaction should include fee funding");
        assert_eq!(fee_funding.transfer.outputs.len(), 1);
        assert_eq!(fee_funding.transfer.outputs[0].value.amount, Amount::zero());
    }

    #[tokio::test]
    async fn historical_fee_funding_prices_complete_final_plan() {
        let source = AddressIndex::new(0);
        let sender = test_address(35);
        let gas_prices = GasPrices {
            block_space_price: 1_000,
            compact_block_space_price: 1_000,
            verification_price: 1_000,
            execution_price: 1_000,
            asset_id: *BASE_ASSET_ID,
        };
        let action = ActionPlan::IbcAction(test_ibc_action());
        let baseline_fee = gas_prices
            .fee(&(action.gas_cost() + shieldd_sdk_transaction::gas::transfer_gas_cost()))
            .apply_tier(FeeTier::default());
        let notes = vec![spendable_note_record(
            &mut OsRng,
            1_000_000,
            source,
            sender.clone(),
            1,
        )];
        let view_addresses = BTreeMap::from([(source, sender)]);
        let mut view = MockNoteManagerView::new(notes, view_addresses).with_nullifier_window(
            NullifierWindow {
                protocol_version: shieldd_sdk_sct::nullifier_generation::PROTOCOL_VERSION,
                current_generation: 2,
                recent_position_floor: 100,
                archived_generation_count: 1,
                archived_history_head: [1; 32],
            },
        );
        let mut note_manager = NoteManager::new(OsRng);
        note_manager.set_gas_prices(gas_prices);

        let result = note_manager
            .plan_actions_with_transfer_funding(&mut view, source, vec![action])
            .await
            .expect("historical fee-funding planning should not error");
        let NoteManagerPlanningResult::Ready { transaction_plan } = result else {
            panic!("expected ready historical fee-funding plan");
        };
        let final_fee = gas_prices
            .fee(&transaction_plan.gas_cost())
            .apply_tier(FeeTier::default());

        assert_eq!(transaction_plan.transaction_parameters.fee, final_fee);
        assert!(
            final_fee.amount() > baseline_fee.amount(),
            "an old fee input must include its historical proof and verification cost",
        );
    }

    #[tokio::test]
    async fn explicit_many_to_one_returns_single_note_reshape_transaction() {
        let mut rng = OsRng;
        let source = AddressIndex::new(0);
        let sender = test_address(4);
        let view_addresses = BTreeMap::from([(source, sender.clone())]);
        let notes = vec![
            spendable_note_record(&mut rng, 7, source, sender.clone(), 1),
            spendable_note_record(&mut rng, 5, source, sender, 2),
        ];
        let mut view = MockNoteManagerView::new(notes, view_addresses);
        let mut note_manager = NoteManager::new(OsRng);
        note_manager.set_gas_prices(GasPrices::zero());

        let result = note_manager
            .plan_note_reshape_from_notes(
                &mut view,
                source,
                *BASE_ASSET_ID,
                Some(NoteReshapeFamilyId::EightByOne),
            )
            .await
            .expect("note reshape planning succeeds");

        let NoteManagerPlanningResult::Ready { transaction_plan } = result else {
            panic!("expected ready note reshape plan");
        };
        assert!(matches!(
            transaction_plan.actions.first(),
            Some(ActionPlan::NoteReshape(note_reshape))
                if note_reshape.family_id() == NoteReshapeFamilyId::EightByOne
        ));
    }

    #[tokio::test]
    async fn many_to_one_uses_padded_canonical_family() {
        let mut rng = OsRng;
        let source = AddressIndex::new(0);
        let sender = test_address(6);
        let view_addresses = BTreeMap::from([(source, sender.clone())]);
        let notes = vec![
            spendable_note_record(&mut rng, 7, source, sender.clone(), 1),
            spendable_note_record(&mut rng, 5, source, sender.clone(), 2),
            spendable_note_record(&mut rng, 3, source, sender, 3),
        ];
        let mut view = MockNoteManagerView::new(notes, view_addresses);
        let mut note_manager = NoteManager::new(OsRng);
        note_manager.set_gas_prices(GasPrices::zero());

        let result = note_manager
            .plan_note_reshape_from_notes(&mut view, source, *BASE_ASSET_ID, None)
            .await
            .expect("note reshape planning succeeds");

        let NoteManagerPlanningResult::Ready { transaction_plan } = result else {
            panic!("expected ready note reshape plan");
        };
        let Some(ActionPlan::NoteReshape(note_reshape)) = transaction_plan.actions.first() else {
            panic!("expected note reshape action");
        };
        assert_eq!(note_reshape.family_id(), NoteReshapeFamilyId::EightByOne);
        assert_eq!(note_reshape.spends.len(), 3);
        assert_eq!(note_reshape.family_id().input_count(), 8);
        let spend_key = test_spend_key(6);
        let fvk = spend_key.full_viewing_key();
        let body = note_reshape
            .note_reshape_body(
                fvk,
                &PayloadKey::from([0u8; 32]),
                shieldd_sdk_tct::Tree::default().root(),
                0,
            )
            .expect("note reshape body materialization succeeds");
        assert!(body.inputs.iter().all(|input| input.encrypted_backref.len()
            == shieldd_sdk_shielded_pool::backref::ENCRYPTED_BACKREF_LEN));
        let padded_input = &body.inputs[3];
        assert!(padded_input
            .encrypted_backref
            .decrypt(&fvk.backref_key(), &padded_input.nullifier)
            .expect("padded backref is a valid ciphertext")
            .is_some());
    }

    #[tokio::test]
    async fn one_to_many_pads_requested_outputs_to_family_capacity() {
        let mut rng = OsRng;
        let source = AddressIndex::new(0);
        let sender = test_address(5);
        let view_addresses = BTreeMap::from([(source, sender.clone())]);
        let note_record = spendable_note_record(&mut rng, 40, source, sender, 1);
        let mut view = MockNoteManagerView::new(vec![note_record.clone()], view_addresses);
        let mut note_manager = NoteManager::new(OsRng);
        note_manager.set_gas_prices(GasPrices::zero());

        let result = note_manager
            .plan_note_reshape_from_note(
                &mut view,
                source,
                note_record,
                vec![10u64.into(), 15u64.into(), 15u64.into()],
            )
            .await
            .expect("note reshape planning succeeds");

        let NoteManagerPlanningResult::Ready { transaction_plan } = result else {
            panic!("expected ready note reshape plan");
        };
        let Some(ActionPlan::NoteReshape(note_reshape)) = transaction_plan.actions.first() else {
            panic!("expected note reshape action");
        };
        assert_eq!(note_reshape.family_id(), NoteReshapeFamilyId::OneByEight);
        assert_eq!(note_reshape.outputs.len(), 3);
        assert_eq!(note_reshape.family_id().output_count(), 8);
        let spend_key = test_spend_key(5);
        let fvk = spend_key.full_viewing_key();
        let body = note_reshape
            .note_reshape_body(
                fvk,
                &PayloadKey::from([0u8; 32]),
                shieldd_sdk_tct::Tree::default().root(),
                0,
            )
            .expect("note reshape body materialization succeeds");
        assert!(body.outputs.iter().all(|output| {
            output.wrapped_memo_key.0 != [0u8; 48] && output.ovk_wrapped_key.0 != [0u8; 48]
        }));
        for (index, output) in body.outputs.iter().enumerate() {
            let shared_secret = Note::decrypt_key(
                output.ovk_wrapped_key.clone(),
                output.note_payload.note_commitment,
                body.balance_commitment,
                fvk.outgoing(),
                &output.note_payload.ephemeral_key,
            )
            .expect("every fixed output uses the serialized action commitment");
            let payload_key =
                PayloadKey::derive(&shared_secret, &output.note_payload.ephemeral_key);
            let note = Note::decrypt_with_payload_key(
                &output.note_payload.encrypted_note,
                &payload_key,
                &output.note_payload.ephemeral_key,
            )
            .expect("fixed output is an ordinary encrypted note");
            if index >= 3 {
                assert_eq!(note.amount(), Amount::zero());
            }
        }
    }
}
