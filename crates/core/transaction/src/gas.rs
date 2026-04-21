use penumbra_sdk_fee::Gas;
use penumbra_sdk_ibc::IbcRelay;
use penumbra_sdk_shielded_pool::{ShieldedIcs20Withdrawal, ShieldedIcs20WithdrawalPlan};
use penumbra_sdk_validator::validator::Definition as ValidatorDefinition;

use penumbra_sdk_governance::{ProposalSubmit, ValidatorVote};

use crate::{
    plan::{ActionPlan, TransactionPlan},
    Action, Transaction,
};

use penumbra_sdk_proto::DomainType;

const NULLIFIER_SIZE: u64 = 2 + 32;
const NOTEPAYLOAD_SIZE: u64 = 32 + 32 + 176;
const ZKPROOF_SIZE: u64 = 192;

/// Allows [`Action`]s and [`Transaction`]s to statically indicate their relative resource consumption.
pub trait GasCost {
    fn gas_cost(&self) -> Gas;
}

pub fn spend_gas_cost() -> Gas {
    Gas {
        block_space: 160 + ZKPROOF_SIZE,
        compact_block_space: NULLIFIER_SIZE,
        verification: 1000,
        execution: 10,
    }
}

pub fn output_gas_cost() -> Gas {
    Gas {
        block_space: 128 + NOTEPAYLOAD_SIZE + ZKPROOF_SIZE,
        compact_block_space: NOTEPAYLOAD_SIZE,
        verification: 1000,
        execution: 10,
    }
}

pub fn transfer_gas_cost() -> Gas {
    spend_gas_cost() + spend_gas_cost() + output_gas_cost() + output_gas_cost()
}

pub fn consolidate_gas_cost() -> Gas {
    spend_gas_cost() + spend_gas_cost() + output_gas_cost()
}

pub fn split_gas_cost() -> Gas {
    spend_gas_cost() + output_gas_cost() + output_gas_cost()
}

pub fn shielded_ics20_withdrawal_gas_cost() -> Gas {
    spend_gas_cost() + spend_gas_cost() + output_gas_cost()
}

impl GasCost for Transaction {
    fn gas_cost(&self) -> Gas {
        self.actions().map(GasCost::gas_cost).sum()
    }
}

impl GasCost for TransactionPlan {
    fn gas_cost(&self) -> Gas {
        self.actions.iter().map(GasCost::gas_cost).sum()
    }
}

impl GasCost for ActionPlan {
    fn gas_cost(&self) -> Gas {
        match self {
            ActionPlan::Transfer(_) => transfer_gas_cost(),
            ActionPlan::Consolidate(_) => consolidate_gas_cost(),
            ActionPlan::Split(_) => split_gas_cost(),
            ActionPlan::ValidatorDefinition(vd) => vd.gas_cost(),
            ActionPlan::IbcAction(i) => i.gas_cost(),
            ActionPlan::ProposalSubmit(ps) => ps.gas_cost(),
            ActionPlan::ValidatorVote(v) => v.gas_cost(),
            ActionPlan::ShieldedIcs20Withdrawal(w) => w.gas_cost(),
            ActionPlan::ComplianceRegisterAsset(_) | ActionPlan::ComplianceRegisterUser(_) => Gas {
                block_space: 100,
                compact_block_space: 100,
                verification: 0,
                execution: 10,
            },
        }
    }
}

impl GasCost for Action {
    fn gas_cost(&self) -> Gas {
        match self {
            Action::Transfer(_) => transfer_gas_cost(),
            Action::Consolidate(_) => consolidate_gas_cost(),
            Action::Split(_) => split_gas_cost(),
            Action::ProposalSubmit(submit) => submit.gas_cost(),
            Action::ValidatorVote(vote) => vote.gas_cost(),
            Action::ShieldedIcs20Withdrawal(withdrawal) => withdrawal.gas_cost(),
            Action::IbcRelay(x) => x.gas_cost(),
            Action::ValidatorDefinition(x) => x.gas_cost(),
            Action::ComplianceRegisterAsset(_) | Action::ComplianceRegisterUser(_) => Gas {
                block_space: 100,
                compact_block_space: 100,
                verification: 0,
                execution: 10,
            },
            Action::AggregateBundle(_) => Gas {
                block_space: 0,
                compact_block_space: 0,
                verification: 0,
                execution: 0,
            },
        }
    }
}

impl GasCost for penumbra_sdk_shielded_pool::Transfer {
    fn gas_cost(&self) -> Gas {
        transfer_gas_cost()
    }
}

impl GasCost for penumbra_sdk_shielded_pool::Consolidate {
    fn gas_cost(&self) -> Gas {
        consolidate_gas_cost()
    }
}

impl GasCost for penumbra_sdk_shielded_pool::Split {
    fn gas_cost(&self) -> Gas {
        split_gas_cost()
    }
}

impl GasCost for ShieldedIcs20WithdrawalPlan {
    fn gas_cost(&self) -> Gas {
        shielded_ics20_withdrawal_gas_cost()
    }
}

impl GasCost for ShieldedIcs20Withdrawal {
    fn gas_cost(&self) -> Gas {
        shielded_ics20_withdrawal_gas_cost()
    }
}

impl GasCost for ValidatorDefinition {
    fn gas_cost(&self) -> Gas {
        Gas {
            block_space: self.encode_to_vec().len() as u64,
            compact_block_space: 0,
            verification: 200,
            execution: 10,
        }
    }
}

impl GasCost for IbcRelay {
    fn gas_cost(&self) -> Gas {
        Gas {
            block_space: self.encode_to_vec().len() as u64,
            compact_block_space: 0,
            verification: 0,
            execution: 10,
        }
    }
}

impl GasCost for ProposalSubmit {
    fn gas_cost(&self) -> Gas {
        Gas {
            block_space: self.encode_to_vec().len() as u64,
            compact_block_space: 0,
            verification: 200,
            execution: 10,
        }
    }
}

impl GasCost for ValidatorVote {
    fn gas_cost(&self) -> Gas {
        Gas {
            block_space: self.encode_to_vec().len() as u64,
            compact_block_space: 0,
            verification: 200,
            execution: 10,
        }
    }
}
