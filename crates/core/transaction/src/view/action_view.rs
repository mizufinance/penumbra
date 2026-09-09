use serde::{Deserialize, Serialize};
use shieldd_sdk_compliance::structs::{MsgRegisterAsset, MsgRegisterUser};
use shieldd_sdk_proof_aggregation::AggregateBundle;
use shieldd_sdk_proto::{core::transaction::v1 as pbt, DomainType};
use shieldd_sdk_shielded_pool::ShieldedHostWithdrawalView;

pub use shieldd_sdk_shielded_pool::NoteReshapeView;
pub use shieldd_sdk_shielded_pool::TransferView;

use crate::Action;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "pbt::ActionView", into = "pbt::ActionView")]
#[allow(clippy::large_enum_variant)]
pub enum ActionView {
    Transfer(TransferView),
    NoteReshape(NoteReshapeView),
    ShieldedHostWithdrawal(ShieldedHostWithdrawalView),
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
                AV::NoteReshape(x) => ActionView::NoteReshape(x.try_into()?),

                AV::ShieldedHostWithdrawal(x) => ActionView::ShieldedHostWithdrawal(x.try_into()?),
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
                ActionView::NoteReshape(x) => AV::NoteReshape(x.into()),

                ActionView::ShieldedHostWithdrawal(x) => AV::ShieldedHostWithdrawal(x.into()),
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
            ActionView::NoteReshape(x) => Action::NoteReshape(x.into()),

            ActionView::ShieldedHostWithdrawal(x) => match x {
                ShieldedHostWithdrawalView::Visible { withdrawal, .. } => {
                    Action::ShieldedHostWithdrawal(withdrawal)
                }
                ShieldedHostWithdrawalView::Opaque { withdrawal } => {
                    Action::ShieldedHostWithdrawal(withdrawal)
                }
            },
            ActionView::ComplianceRegisterAsset(x) => Action::ComplianceRegisterAsset(x),
            ActionView::ComplianceRegisterUser(x) => Action::ComplianceRegisterUser(x),
            ActionView::AggregateBundle(x) => Action::AggregateBundle(x),
        }
    }
}
