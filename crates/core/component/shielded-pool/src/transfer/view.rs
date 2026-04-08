use penumbra_sdk_keys::PayloadKey;
use penumbra_sdk_proto::{core::component::shielded_pool::v1 as pbt, DomainType};
use serde::{Deserialize, Serialize};

use crate::{NoteView, Transfer};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "pbt::TransferView", into = "pbt::TransferView")]
#[allow(clippy::large_enum_variant)]
pub enum TransferView {
    Visible {
        transfer: Transfer,
        spent_notes: Vec<NoteView>,
        created_notes: Vec<NoteView>,
        payload_key: PayloadKey,
    },
    Opaque {
        transfer: Transfer,
    },
}

impl DomainType for TransferView {
    type Proto = pbt::TransferView;
}

impl TryFrom<pbt::TransferView> for TransferView {
    type Error = anyhow::Error;

    fn try_from(v: pbt::TransferView) -> Result<Self, Self::Error> {
        match v
            .transfer_view
            .ok_or_else(|| anyhow::anyhow!("missing transfer_view field"))?
        {
            pbt::transfer_view::TransferView::Visible(x) => Ok(Self::Visible {
                transfer: x
                    .transfer
                    .ok_or_else(|| anyhow::anyhow!("missing transfer field"))?
                    .try_into()?,
                spent_notes: x
                    .spent_notes
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<_>, _>>()?,
                created_notes: x
                    .created_notes
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<_>, _>>()?,
                payload_key: x
                    .payload_key
                    .ok_or_else(|| anyhow::anyhow!("missing payload key"))?
                    .inner
                    .as_slice()
                    .try_into()?,
            }),
            pbt::transfer_view::TransferView::Opaque(x) => Ok(Self::Opaque {
                transfer: x
                    .transfer
                    .ok_or_else(|| anyhow::anyhow!("missing transfer field"))?
                    .try_into()?,
            }),
        }
    }
}

impl From<TransferView> for pbt::TransferView {
    fn from(v: TransferView) -> Self {
        use pbt::transfer_view as tv;

        match v {
            TransferView::Visible {
                transfer,
                spent_notes,
                created_notes,
                payload_key,
            } => Self {
                transfer_view: Some(tv::TransferView::Visible(tv::Visible {
                    transfer: Some(transfer.into()),
                    spent_notes: spent_notes.into_iter().map(Into::into).collect(),
                    created_notes: created_notes.into_iter().map(Into::into).collect(),
                    payload_key: Some(payload_key.into()),
                })),
            },
            TransferView::Opaque { transfer } => Self {
                transfer_view: Some(tv::TransferView::Opaque(tv::Opaque {
                    transfer: Some(transfer.into()),
                })),
            },
        }
    }
}

impl From<TransferView> for Transfer {
    fn from(v: TransferView) -> Self {
        match v {
            TransferView::Visible { transfer, .. } => transfer,
            TransferView::Opaque { transfer } => transfer,
        }
    }
}
