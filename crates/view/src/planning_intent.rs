use decaf377::Fr;
use shieldd_sdk_fee::Gas;
use shieldd_sdk_sct::nullifier_generation::NullifierWindow;
use shieldd_sdk_shielded_pool::{
    HostWithdrawal, NoteReshapeFamilyId, ShieldedInputPlan, ShieldedOutputPlan,
};
use shieldd_sdk_transaction::{
    gas::{self, GasCost},
    plan::{ActionPlan, MemoPlan},
    TransactionParameters,
};

#[derive(Clone, Debug)]
pub struct TransferIntent {
    pub spends: Vec<ShieldedInputPlan>,
    pub outputs: Vec<ShieldedOutputPlan>,
    pub value_blinding: Fr,
}

#[derive(Clone, Debug)]
pub struct NoteReshapeIntent {
    pub family_id: NoteReshapeFamilyId,
    pub spends: Vec<ShieldedInputPlan>,
    pub outputs: Vec<ShieldedOutputPlan>,
    pub value_blinding: Fr,
}

#[derive(Clone, Debug)]
pub struct WithdrawalIntent {
    pub spends: Vec<ShieldedInputPlan>,
    pub change_output: Option<ShieldedOutputPlan>,
    pub withdrawal: HostWithdrawal,
    pub value_blinding: Fr,
}

#[derive(Clone, Debug)]
pub enum ActionIntent {
    Transfer(TransferIntent),
    NoteReshape(NoteReshapeIntent),
    HostWithdrawal(WithdrawalIntent),
    Complete(ActionPlan),
}

impl ActionIntent {
    pub fn spends(&self) -> &[ShieldedInputPlan] {
        match self {
            Self::Transfer(intent) => &intent.spends,
            Self::NoteReshape(intent) => &intent.spends,
            Self::HostWithdrawal(intent) => &intent.spends,
            Self::Complete(action) => action.spends(),
        }
    }

    pub fn outputs(&self) -> &[ShieldedOutputPlan] {
        match self {
            Self::Transfer(intent) => &intent.outputs,
            Self::NoteReshape(intent) => &intent.outputs,
            Self::HostWithdrawal(intent) => intent.change_output.as_slice(),
            Self::Complete(_) => &[],
        }
    }
}

impl GasCost for ActionIntent {
    fn gas_cost(&self) -> Gas {
        match self {
            Self::Transfer(_) => gas::transfer_gas_cost(),
            Self::NoteReshape(intent) => gas::note_reshape_gas_cost(
                intent.family_id.input_count(),
                intent.family_id.output_count(),
            ),
            Self::HostWithdrawal(intent) => gas::host_withdrawal_gas_cost(&intent.withdrawal),
            Self::Complete(action) => action.gas_cost(),
        }
    }
}

/// Wallet selections before compliance reads and final action construction.
#[derive(Clone, Debug)]
pub struct TransactionIntent {
    pub actions: Vec<ActionIntent>,
    pub fee_funding: Option<TransferIntent>,
    pub transaction_parameters: TransactionParameters,
    pub memo: Option<MemoPlan>,
    pub nullifier_window: Option<NullifierWindow>,
}

pub(crate) fn intent_gas(
    actions: &[ActionIntent],
    fee: Option<&TransferIntent>,
    window: NullifierWindow,
) -> Gas {
    gas::planned_gas(
        actions
            .iter()
            .map(GasCost::gas_cost)
            .chain(fee.map(|_| gas::transfer_gas_cost())),
        actions
            .iter()
            .flat_map(ActionIntent::spends)
            .chain(fee.into_iter().flat_map(|fee| &fee.spends)),
        Some(window),
    )
}

impl TransactionIntent {
    pub fn has_outputs(&self) -> bool {
        self.fee_funding.is_some()
            || self.actions.iter().any(|action| match action {
                ActionIntent::Complete(action) => matches!(
                    action,
                    ActionPlan::Transfer(_)
                        | ActionPlan::NoteReshape(_)
                        | ActionPlan::ShieldedHostWithdrawal(_)
                ),
                _ => true,
            })
    }
}

impl From<TransferIntent> for ActionIntent {
    fn from(intent: TransferIntent) -> Self {
        Self::Transfer(intent)
    }
}
impl From<NoteReshapeIntent> for ActionIntent {
    fn from(intent: NoteReshapeIntent) -> Self {
        Self::NoteReshape(intent)
    }
}

impl From<WithdrawalIntent> for ActionIntent {
    fn from(intent: WithdrawalIntent) -> Self {
        Self::HostWithdrawal(intent)
    }
}
