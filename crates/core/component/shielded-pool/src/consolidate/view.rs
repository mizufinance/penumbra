use penumbra_sdk_keys::PayloadKey;
use penumbra_sdk_proto::{core::component::shielded_pool::v1 as pb, DomainType};
use serde::{Deserialize, Serialize};

use crate::{Consolidate, NoteView};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "pb::ConsolidateView", into = "pb::ConsolidateView")]
#[allow(clippy::large_enum_variant)]
pub enum ConsolidateView {
    Visible {
        consolidate: Consolidate,
        spent_notes: Vec<NoteView>,
        created_notes: Vec<NoteView>,
        payload_key: PayloadKey,
    },
    Opaque {
        consolidate: Consolidate,
    },
}

impl DomainType for ConsolidateView {
    type Proto = pb::ConsolidateView;
}

impl TryFrom<pb::ConsolidateView> for ConsolidateView {
    type Error = anyhow::Error;

    fn try_from(v: pb::ConsolidateView) -> Result<Self, Self::Error> {
        match v
            .consolidate_view
            .ok_or_else(|| anyhow::anyhow!("missing consolidate_view field"))?
        {
            pb::consolidate_view::ConsolidateView::Visible(x) => Ok(Self::Visible {
                consolidate: x
                    .consolidate
                    .ok_or_else(|| anyhow::anyhow!("missing consolidate field"))?
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
            pb::consolidate_view::ConsolidateView::Opaque(x) => Ok(Self::Opaque {
                consolidate: x
                    .consolidate
                    .ok_or_else(|| anyhow::anyhow!("missing consolidate field"))?
                    .try_into()?,
            }),
        }
    }
}

impl From<ConsolidateView> for pb::ConsolidateView {
    fn from(v: ConsolidateView) -> Self {
        use pb::consolidate_view as cv;

        match v {
            ConsolidateView::Visible {
                consolidate,
                spent_notes,
                created_notes,
                payload_key,
            } => Self {
                consolidate_view: Some(cv::ConsolidateView::Visible(cv::Visible {
                    consolidate: Some(consolidate.into()),
                    spent_notes: spent_notes.into_iter().map(Into::into).collect(),
                    created_notes: created_notes.into_iter().map(Into::into).collect(),
                    payload_key: Some(payload_key.into()),
                })),
            },
            ConsolidateView::Opaque { consolidate } => Self {
                consolidate_view: Some(cv::ConsolidateView::Opaque(cv::Opaque {
                    consolidate: Some(consolidate.into()),
                })),
            },
        }
    }
}

impl From<ConsolidateView> for Consolidate {
    fn from(v: ConsolidateView) -> Self {
        match v {
            ConsolidateView::Visible { consolidate, .. } => consolidate,
            ConsolidateView::Opaque { consolidate } => consolidate,
        }
    }
}
