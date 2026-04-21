use anyhow::{anyhow, Error};
use penumbra_sdk_keys::symmetric::PayloadKey;
use penumbra_sdk_proto::{core::component::shielded_pool::v1 as pb, DomainType};

use crate::NoteView;

use super::ShieldedIcs20Withdrawal;

#[derive(Clone, Debug)]
pub enum ShieldedIcs20WithdrawalView {
    Visible {
        withdrawal: ShieldedIcs20Withdrawal,
        spent_notes: Vec<NoteView>,
        change_note: NoteView,
        payload_key: PayloadKey,
    },
    Opaque {
        withdrawal: ShieldedIcs20Withdrawal,
    },
}

impl DomainType for ShieldedIcs20WithdrawalView {
    type Proto = pb::ShieldedIcs20WithdrawalView;
}

impl From<ShieldedIcs20WithdrawalView> for pb::ShieldedIcs20WithdrawalView {
    fn from(value: ShieldedIcs20WithdrawalView) -> Self {
        use pb::shielded_ics20_withdrawal_view::ShieldedIcs20WithdrawalView as View;

        match value {
            ShieldedIcs20WithdrawalView::Visible {
                withdrawal,
                spent_notes,
                change_note,
                payload_key,
            } => Self {
                shielded_ics20_withdrawal_view: Some(View::Visible(
                    pb::shielded_ics20_withdrawal_view::Visible {
                        withdrawal: Some(withdrawal.into()),
                        spent_notes: spent_notes.into_iter().map(Into::into).collect(),
                        change_note: Some(change_note.into()),
                        payload_key: Some(payload_key.into()),
                    },
                )),
            },
            ShieldedIcs20WithdrawalView::Opaque { withdrawal } => Self {
                shielded_ics20_withdrawal_view: Some(View::Opaque(
                    pb::shielded_ics20_withdrawal_view::Opaque {
                        withdrawal: Some(withdrawal.into()),
                    },
                )),
            },
        }
    }
}

impl TryFrom<pb::ShieldedIcs20WithdrawalView> for ShieldedIcs20WithdrawalView {
    type Error = Error;

    fn try_from(value: pb::ShieldedIcs20WithdrawalView) -> Result<Self, Self::Error> {
        use pb::shielded_ics20_withdrawal_view::ShieldedIcs20WithdrawalView as View;

        match value
            .shielded_ics20_withdrawal_view
            .ok_or_else(|| anyhow!("missing shielded ICS-20 withdrawal view"))?
        {
            View::Visible(visible) => Ok(Self::Visible {
                withdrawal: visible
                    .withdrawal
                    .ok_or_else(|| anyhow!("missing visible shielded ICS-20 withdrawal"))?
                    .try_into()?,
                spent_notes: visible
                    .spent_notes
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<_>, _>>()?,
                change_note: visible
                    .change_note
                    .ok_or_else(|| {
                        anyhow!("missing visible shielded ICS-20 withdrawal change note")
                    })?
                    .try_into()?,
                payload_key: visible
                    .payload_key
                    .ok_or_else(|| {
                        anyhow!("missing visible shielded ICS-20 withdrawal payload key")
                    })?
                    .try_into()?,
            }),
            View::Opaque(opaque) => Ok(Self::Opaque {
                withdrawal: opaque
                    .withdrawal
                    .ok_or_else(|| anyhow!("missing opaque shielded ICS-20 withdrawal"))?
                    .try_into()?,
            }),
        }
    }
}
