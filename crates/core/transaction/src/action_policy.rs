use crate::{Action, ActionPlan, Transaction, TransactionPlan};

fn disabled_action_name(action: &Action) -> Option<&'static str> {
    match action {
        Action::Spend(_)
        | Action::Output(_)
        | Action::Transfer(_)
        | Action::ValidatorDefinition(_)
        | Action::IbcRelay(_)
        | Action::ProposalSubmit(_)
        | Action::ProposalWithdraw(_)
        | Action::ValidatorVote(_)
        | Action::ProposalDepositClaim(_)
        | Action::Ics20Withdrawal(_)
        | Action::ComplianceRegisterAsset(_)
        | Action::ComplianceRegisterUser(_)
        | Action::AggregateBundle(_) => None,
        Action::Swap(_) => Some("Swap"),
        Action::SwapClaim(_) => Some("SwapClaim"),
        Action::PositionOpen(_) => Some("PositionOpen"),
        Action::PositionClose(_) => Some("PositionClose"),
        Action::PositionWithdraw(_) => Some("PositionWithdraw"),
        Action::Delegate(_) => Some("Delegate"),
        Action::Undelegate(_) => Some("Undelegate"),
        Action::UndelegateClaim(_) => Some("UndelegateClaim"),
        Action::DelegatorVote(_) => Some("DelegatorVote"),
        Action::CommunityPoolSpend(_) => Some("CommunityPoolSpend"),
        Action::CommunityPoolOutput(_) => Some("CommunityPoolOutput"),
        Action::CommunityPoolDeposit(_) => Some("CommunityPoolDeposit"),
        Action::ActionDutchAuctionSchedule(_) => Some("ActionDutchAuctionSchedule"),
        Action::ActionDutchAuctionEnd(_) => Some("ActionDutchAuctionEnd"),
        Action::ActionDutchAuctionWithdraw(_) => Some("ActionDutchAuctionWithdraw"),
        Action::ActionLiquidityTournamentVote(_) => Some("ActionLiquidityTournamentVote"),
    }
}

fn disabled_action_plan_name(action: &ActionPlan) -> Option<&'static str> {
    match action {
        ActionPlan::Spend(_)
        | ActionPlan::Output(_)
        | ActionPlan::Transfer(_)
        | ActionPlan::ValidatorDefinition(_)
        | ActionPlan::IbcAction(_)
        | ActionPlan::ProposalSubmit(_)
        | ActionPlan::ProposalWithdraw(_)
        | ActionPlan::ValidatorVote(_)
        | ActionPlan::ProposalDepositClaim(_)
        | ActionPlan::Ics20Withdrawal(_)
        | ActionPlan::ComplianceRegisterAsset(_)
        | ActionPlan::ComplianceRegisterUser(_) => None,
        ActionPlan::Swap(_) => Some("Swap"),
        ActionPlan::SwapClaim(_) => Some("SwapClaim"),
        ActionPlan::PositionOpen(_) => Some("PositionOpen"),
        ActionPlan::PositionClose(_) => Some("PositionClose"),
        ActionPlan::PositionWithdraw(_) => Some("PositionWithdraw"),
        ActionPlan::Delegate(_) => Some("Delegate"),
        ActionPlan::Undelegate(_) => Some("Undelegate"),
        ActionPlan::UndelegateClaim(_) => Some("UndelegateClaim"),
        ActionPlan::DelegatorVote(_) => Some("DelegatorVote"),
        ActionPlan::CommunityPoolSpend(_) => Some("CommunityPoolSpend"),
        ActionPlan::CommunityPoolOutput(_) => Some("CommunityPoolOutput"),
        ActionPlan::CommunityPoolDeposit(_) => Some("CommunityPoolDeposit"),
        ActionPlan::ActionDutchAuctionSchedule(_) => Some("ActionDutchAuctionSchedule"),
        ActionPlan::ActionDutchAuctionEnd(_) => Some("ActionDutchAuctionEnd"),
        ActionPlan::ActionDutchAuctionWithdraw(_) => Some("ActionDutchAuctionWithdraw"),
        ActionPlan::ActionLiquidityTournamentVote(_) => Some("ActionLiquidityTournamentVote"),
    }
}

fn disabled_action_err(name: &str) -> anyhow::Error {
    anyhow::anyhow!("action disabled in reduced action surface: {name}")
}

pub fn check_action_enabled(action: &Action) -> anyhow::Result<()> {
    if let Some(name) = disabled_action_name(action) {
        anyhow::bail!(disabled_action_err(name));
    }

    Ok(())
}

pub fn check_action_plan_enabled(action: &ActionPlan) -> anyhow::Result<()> {
    if let Some(name) = disabled_action_plan_name(action) {
        anyhow::bail!(disabled_action_err(name));
    }

    Ok(())
}

pub fn check_transaction_enabled(tx: &Transaction) -> anyhow::Result<()> {
    for action in tx.actions() {
        check_action_enabled(action)?;
    }

    Ok(())
}

pub fn check_transaction_plan_enabled(plan: &TransactionPlan) -> anyhow::Result<()> {
    for action in &plan.actions {
        check_action_plan_enabled(action)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use penumbra_sdk_asset::{Value, STAKING_TOKEN_ASSET_ID};
    use penumbra_sdk_community_pool::CommunityPoolDeposit;

    use super::*;

    #[test]
    fn reduced_action_surface_rejects_disabled_action() {
        let err = check_action_enabled(&Action::CommunityPoolDeposit(CommunityPoolDeposit {
            value: Value {
                amount: 1u64.into(),
                asset_id: *STAKING_TOKEN_ASSET_ID,
            },
        }))
        .expect_err("community pool deposits should be disabled");

        assert_eq!(
            err.to_string(),
            "action disabled in reduced action surface: CommunityPoolDeposit"
        );
    }

    #[test]
    fn reduced_action_surface_rejects_disabled_action_plan() {
        let err =
            check_action_plan_enabled(&ActionPlan::CommunityPoolDeposit(CommunityPoolDeposit {
                value: Value {
                    amount: 1u64.into(),
                    asset_id: *STAKING_TOKEN_ASSET_ID,
                },
            }))
            .expect_err("community pool deposit plans should be disabled");

        assert_eq!(
            err.to_string(),
            "action disabled in reduced action surface: CommunityPoolDeposit"
        );
    }
}
