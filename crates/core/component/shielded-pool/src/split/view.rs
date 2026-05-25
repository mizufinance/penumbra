use penumbra_sdk_keys::PayloadKey;
use penumbra_sdk_proto::{core::component::shielded_pool::v1 as pb, DomainType};
use serde::{Deserialize, Serialize};

use crate::{NoteView, Split};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "pb::SplitView", into = "pb::SplitView")]
#[allow(clippy::large_enum_variant)]
pub enum SplitView {
    Visible {
        split: Split,
        spent_notes: Vec<NoteView>,
        created_notes: Vec<NoteView>,
        payload_key: PayloadKey,
    },
    Opaque {
        split: Split,
    },
}

impl DomainType for SplitView {
    type Proto = pb::SplitView;
}

impl TryFrom<pb::SplitView> for SplitView {
    type Error = anyhow::Error;

    fn try_from(v: pb::SplitView) -> Result<Self, Self::Error> {
        match v
            .split_view
            .ok_or_else(|| anyhow::anyhow!("missing split_view field"))?
        {
            pb::split_view::SplitView::Visible(x) => Ok(Self::Visible {
                split: x
                    .split
                    .ok_or_else(|| anyhow::anyhow!("missing split field"))?
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
            pb::split_view::SplitView::Opaque(x) => Ok(Self::Opaque {
                split: x
                    .split
                    .ok_or_else(|| anyhow::anyhow!("missing split field"))?
                    .try_into()?,
            }),
        }
    }
}

impl From<SplitView> for pb::SplitView {
    fn from(v: SplitView) -> Self {
        use pb::split_view as sv;

        match v {
            SplitView::Visible {
                split,
                spent_notes,
                created_notes,
                payload_key,
            } => Self {
                split_view: Some(sv::SplitView::Visible(sv::Visible {
                    split: Some(split.into()),
                    spent_notes: spent_notes.into_iter().map(Into::into).collect(),
                    created_notes: created_notes.into_iter().map(Into::into).collect(),
                    payload_key: Some(payload_key.into()),
                })),
            },
            SplitView::Opaque { split } => Self {
                split_view: Some(sv::SplitView::Opaque(sv::Opaque {
                    split: Some(split.into()),
                })),
            },
        }
    }
}

impl From<SplitView> for Split {
    fn from(v: SplitView) -> Self {
        match v {
            SplitView::Visible { split, .. } => split,
            SplitView::Opaque { split } => split,
        }
    }
}
