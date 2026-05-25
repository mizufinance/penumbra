use penumbra_sdk_proto::{
    core::component::validator::v1::ValidatorInfoResponse,
    penumbra::core::component::validator::v1 as pb, DomainType,
};
use serde::{Deserialize, Serialize};

use super::{Status, Validator};

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(try_from = "pb::ValidatorInfo", into = "pb::ValidatorInfo")]
pub struct Info {
    pub validator: Validator,
    pub status: Status,
}

impl DomainType for Info {
    type Proto = pb::ValidatorInfo;
}

impl From<Info> for pb::ValidatorInfo {
    fn from(v: Info) -> Self {
        pb::ValidatorInfo {
            validator: Some(v.validator.into()),
            status: Some(v.status.into()),
        }
    }
}

impl From<Info> for ValidatorInfoResponse {
    fn from(v: Info) -> Self {
        ValidatorInfoResponse {
            validator_info: Some(v.into()),
        }
    }
}

impl TryFrom<pb::ValidatorInfo> for Info {
    type Error = anyhow::Error;
    fn try_from(v: pb::ValidatorInfo) -> Result<Self, Self::Error> {
        Ok(Info {
            validator: v
                .validator
                .ok_or_else(|| anyhow::anyhow!("missing validator field in proto"))?
                .try_into()?,
            status: v
                .status
                .ok_or_else(|| anyhow::anyhow!("missing status field in proto"))?
                .try_into()?,
        })
    }
}

impl TryFrom<ValidatorInfoResponse> for Info {
    type Error = anyhow::Error;

    fn try_from(info_resp: ValidatorInfoResponse) -> Result<Self, Self::Error> {
        info_resp
            .validator_info
            .ok_or_else(|| anyhow::anyhow!("empty ValidatorInfoResponse message"))?
            .try_into()
    }
}
