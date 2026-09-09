use anyhow::{Context, Error};
use shieldd_sdk_keys::symmetric::{OvkWrappedKey, WrappedMemoKey};
use shieldd_sdk_proto::{core::component::shielded_pool::v1 as pb, DomainType};

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(
    try_from = "pb::ShieldedWithdrawalChangeBody",
    into = "pb::ShieldedWithdrawalChangeBody"
)]
pub struct ShieldedWithdrawalChangeBody {
    pub note_payload: crate::NotePayload,
    pub wrapped_memo_key: WrappedMemoKey,
    pub ovk_wrapped_key: OvkWrappedKey,
}

impl DomainType for ShieldedWithdrawalChangeBody {
    type Proto = pb::ShieldedWithdrawalChangeBody;
}

impl From<ShieldedWithdrawalChangeBody> for pb::ShieldedWithdrawalChangeBody {
    fn from(value: ShieldedWithdrawalChangeBody) -> Self {
        Self {
            note_payload: Some(value.note_payload.into()),
            wrapped_memo_key: value.wrapped_memo_key.0.to_vec(),
            ovk_wrapped_key: value.ovk_wrapped_key.0.to_vec(),
        }
    }
}

impl TryFrom<pb::ShieldedWithdrawalChangeBody> for ShieldedWithdrawalChangeBody {
    type Error = Error;

    fn try_from(value: pb::ShieldedWithdrawalChangeBody) -> Result<Self, Self::Error> {
        Ok(Self {
            note_payload: value
                .note_payload
                .ok_or_else(|| anyhow::anyhow!("missing change note payload"))?
                .try_into()
                .context("malformed change note payload")?,
            wrapped_memo_key: value.wrapped_memo_key[..]
                .try_into()
                .context("malformed change wrapped memo key")?,
            ovk_wrapped_key: value.ovk_wrapped_key[..]
                .try_into()
                .context("malformed change ovk wrapped key")?,
        })
    }
}
