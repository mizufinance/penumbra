use penumbra_sdk_compliance::structs::{MsgRegisterAsset, MsgRegisterUser};
use penumbra_sdk_governance::{ProposalSubmit, ValidatorVote};
use penumbra_sdk_ibc::IbcRelay;
use penumbra_sdk_proof_aggregation::AggregateBundle;
use penumbra_sdk_proto::{core::transaction::v1 as pbt, DomainType};
use penumbra_sdk_shielded_pool::ShieldedIcs20WithdrawalView;
use serde::{Deserialize, Serialize};

pub use penumbra_sdk_shielded_pool::ConsolidateView;
pub use penumbra_sdk_shielded_pool::SplitView;
pub use penumbra_sdk_shielded_pool::TransferView;

use crate::Action;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "pbt::ActionView", into = "pbt::ActionView")]
#[allow(clippy::large_enum_variant)]
pub enum ActionView {
    Transfer(TransferView),
    Consolidate(ConsolidateView),
    Split(SplitView),
    ValidatorDefinition(penumbra_sdk_validator::validator::Definition),
    IbcRelay(IbcRelay),
    ProposalSubmit(ProposalSubmit),
    ValidatorVote(ValidatorVote),
    ShieldedIcs20Withdrawal(ShieldedIcs20WithdrawalView),
    ComplianceRegisterAsset(MsgRegisterAsset),
    ComplianceRegisterUser(MsgRegisterUser),
    AggregateBundle(AggregateBundle),
}

impl DomainType for ActionView {
    type Proto = pbt::ActionView;
}

impl TryFrom<pbt::ActionView> for ActionView {
    type Error = anyhow::Error;

    fn try_from(v: pbt::ActionView) -> Result<Self, Self::Error> {
        use pbt::action_view::ActionView as AV;
        Ok(
            match v
                .action_view
                .ok_or_else(|| anyhow::anyhow!("missing action_view"))?
            {
                AV::Transfer(x) => ActionView::Transfer(x.try_into()?),
                AV::Consolidate(x) => ActionView::Consolidate(x.try_into()?),
                AV::Split(x) => ActionView::Split(x.try_into()?),
                AV::ValidatorDefinition(x) => ActionView::ValidatorDefinition(x.try_into()?),
                AV::IbcRelayAction(x) => ActionView::IbcRelay(x.try_into()?),
                AV::ProposalSubmit(x) => ActionView::ProposalSubmit(x.try_into()?),
                AV::ValidatorVote(x) => ActionView::ValidatorVote(x.try_into()?),
                AV::ShieldedIcs20Withdrawal(x) => {
                    ActionView::ShieldedIcs20Withdrawal(x.try_into()?)
                }
                AV::ComplianceRegisterAsset(x) => {
                    ActionView::ComplianceRegisterAsset(x.try_into()?)
                }
                AV::ComplianceRegisterUser(x) => ActionView::ComplianceRegisterUser(x.try_into()?),
                AV::AggregateBundle(x) => ActionView::AggregateBundle(x.try_into()?),
            },
        )
    }
}

impl From<ActionView> for pbt::ActionView {
    fn from(v: ActionView) -> Self {
        use pbt::action_view::ActionView as AV;
        Self {
            action_view: Some(match v {
                ActionView::Transfer(x) => AV::Transfer(x.into()),
                ActionView::Consolidate(x) => AV::Consolidate(x.into()),
                ActionView::Split(x) => AV::Split(x.into()),
                ActionView::ValidatorDefinition(x) => AV::ValidatorDefinition(x.into()),
                ActionView::IbcRelay(x) => AV::IbcRelayAction(x.into()),
                ActionView::ProposalSubmit(x) => AV::ProposalSubmit(x.into()),
                ActionView::ValidatorVote(x) => AV::ValidatorVote(x.into()),
                ActionView::ShieldedIcs20Withdrawal(x) => AV::ShieldedIcs20Withdrawal(x.into()),
                ActionView::ComplianceRegisterAsset(x) => AV::ComplianceRegisterAsset(x.into()),
                ActionView::ComplianceRegisterUser(x) => AV::ComplianceRegisterUser(x.into()),
                ActionView::AggregateBundle(x) => AV::AggregateBundle(x.into()),
            }),
        }
    }
}

impl From<ActionView> for Action {
    fn from(action_view: ActionView) -> Action {
        match action_view {
            ActionView::Transfer(x) => Action::Transfer(x.into()),
            ActionView::Consolidate(x) => Action::Consolidate(x.into()),
            ActionView::Split(x) => Action::Split(x.into()),
            ActionView::ValidatorDefinition(x) => Action::ValidatorDefinition(x),
            ActionView::IbcRelay(x) => Action::IbcRelay(x),
            ActionView::ProposalSubmit(x) => Action::ProposalSubmit(x),
            ActionView::ValidatorVote(x) => Action::ValidatorVote(x),
            ActionView::ShieldedIcs20Withdrawal(x) => match x {
                ShieldedIcs20WithdrawalView::Visible { withdrawal, .. } => {
                    Action::ShieldedIcs20Withdrawal(withdrawal)
                }
                ShieldedIcs20WithdrawalView::Opaque { withdrawal } => {
                    Action::ShieldedIcs20Withdrawal(withdrawal)
                }
            },
            ActionView::ComplianceRegisterAsset(x) => Action::ComplianceRegisterAsset(x),
            ActionView::ComplianceRegisterUser(x) => Action::ComplianceRegisterUser(x),
            ActionView::AggregateBundle(x) => Action::AggregateBundle(x),
        }
    }
}
