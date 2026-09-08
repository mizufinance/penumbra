use anyhow::{anyhow, Error};
use shieldd_sdk_keys::symmetric::PayloadKey;
use shieldd_sdk_proto::{core::component::shielded_pool::v1 as pb, DomainType};

use crate::NoteView;

use super::ShieldedHostWithdrawal;

#[derive(Clone, Debug)]
pub enum ShieldedHostWithdrawalView {
    Visible {
        withdrawal: ShieldedHostWithdrawal,
        spent_notes: Vec<NoteView>,
        change_note: Option<NoteView>,
        payload_key: PayloadKey,
    },
    Opaque {
        withdrawal: ShieldedHostWithdrawal,
    },
}

impl DomainType for ShieldedHostWithdrawalView {
    type Proto = pb::ShieldedHostWithdrawalView;
}

impl From<ShieldedHostWithdrawalView> for pb::ShieldedHostWithdrawalView {
    fn from(value: ShieldedHostWithdrawalView) -> Self {
        use pb::shielded_host_withdrawal_view::ShieldedHostWithdrawalView as View;

        match value {
            ShieldedHostWithdrawalView::Visible {
                withdrawal,
                spent_notes,
                change_note,
                payload_key,
            } => Self {
                shielded_host_withdrawal_view: Some(View::Visible(
                    pb::shielded_host_withdrawal_view::Visible {
                        withdrawal: Some(withdrawal.into()),
                        spent_notes: spent_notes.into_iter().map(Into::into).collect(),
                        change_note: change_note.map(Into::into),
                        payload_key: Some(payload_key.into()),
                    },
                )),
            },
            ShieldedHostWithdrawalView::Opaque { withdrawal } => Self {
                shielded_host_withdrawal_view: Some(View::Opaque(
                    pb::shielded_host_withdrawal_view::Opaque {
                        withdrawal: Some(withdrawal.into()),
                    },
                )),
            },
        }
    }
}

impl TryFrom<pb::ShieldedHostWithdrawalView> for ShieldedHostWithdrawalView {
    type Error = Error;

    fn try_from(value: pb::ShieldedHostWithdrawalView) -> Result<Self, Self::Error> {
        use pb::shielded_host_withdrawal_view::ShieldedHostWithdrawalView as View;

        match value
            .shielded_host_withdrawal_view
            .ok_or_else(|| anyhow!("missing shielded host withdrawal view"))?
        {
            View::Visible(visible) => Ok(Self::Visible {
                withdrawal: visible
                    .withdrawal
                    .ok_or_else(|| anyhow!("missing visible shielded host withdrawal"))?
                    .try_into()?,
                spent_notes: visible
                    .spent_notes
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<_>, _>>()?,
                change_note: visible.change_note.map(TryInto::try_into).transpose()?,
                payload_key: visible
                    .payload_key
                    .ok_or_else(|| anyhow!("missing visible shielded host withdrawal payload key"))?
                    .try_into()?,
            }),
            View::Opaque(opaque) => Ok(Self::Opaque {
                withdrawal: opaque
                    .withdrawal
                    .ok_or_else(|| anyhow!("missing opaque shielded host withdrawal"))?
                    .try_into()?,
            }),
        }
    }
}
