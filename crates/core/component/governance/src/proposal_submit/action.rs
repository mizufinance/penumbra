use decaf377_rdsa::{Signature, SpendAuth};
use serde::{Deserialize, Serialize};

use penumbra_sdk_asset::Balance;
use penumbra_sdk_proto::{penumbra::core::component::governance::v1 as pb, DomainType};
use penumbra_sdk_txhash::{EffectHash, EffectingData};
use penumbra_sdk_validator::{GovernanceKey, IdentityKey};

use crate::proposal::Proposal;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "pb::ProposalSubmitBody", into = "pb::ProposalSubmitBody")]
pub struct ProposalSubmitBody {
    /// The proposal to propose.
    pub proposal: Proposal,
    /// The active validator submitting the proposal.
    pub proposer: IdentityKey,
    /// The governance key authorizing the proposal.
    pub governance_key: GovernanceKey,
}

impl DomainType for ProposalSubmitBody {
    type Proto = pb::ProposalSubmitBody;
}

impl From<ProposalSubmitBody> for pb::ProposalSubmitBody {
    fn from(value: ProposalSubmitBody) -> pb::ProposalSubmitBody {
        pb::ProposalSubmitBody {
            proposal: Some(value.proposal.into()),
            proposer: Some(value.proposer.into()),
            governance_key: Some(value.governance_key.into()),
        }
    }
}

impl TryFrom<pb::ProposalSubmitBody> for ProposalSubmitBody {
    type Error = anyhow::Error;

    fn try_from(msg: pb::ProposalSubmitBody) -> Result<Self, Self::Error> {
        Ok(Self {
            proposal: msg
                .proposal
                .ok_or_else(|| anyhow::anyhow!("missing proposal in `ProposalSubmitBody`"))?
                .try_into()?,
            proposer: msg
                .proposer
                .ok_or_else(|| anyhow::anyhow!("missing proposer in `ProposalSubmitBody`"))?
                .try_into()?,
            governance_key: msg
                .governance_key
                .ok_or_else(|| anyhow::anyhow!("missing governance key in `ProposalSubmitBody`"))?
                .try_into()?,
        })
    }
}

/// A POA proposal submission authorized by an active validator governance key.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "pb::ProposalSubmit", into = "pb::ProposalSubmit")]
pub struct ProposalSubmit {
    /// The effecting proposal submission body.
    pub body: ProposalSubmitBody,
    /// The proposal submission authorization signature.
    pub auth_sig: Signature<SpendAuth>,
}

impl EffectingData for ProposalSubmit {
    fn effect_hash(&self) -> EffectHash {
        EffectHash::from_proto_effecting_data(&self.to_proto())
    }
}

impl ProposalSubmit {
    pub fn proposal(&self) -> &Proposal {
        &self.body.proposal
    }

    /// Compute a commitment to the value contributed to a transaction by this proposal submission.
    pub fn balance(&self) -> Balance {
        Balance::default()
    }
}

impl From<ProposalSubmit> for pb::ProposalSubmit {
    fn from(value: ProposalSubmit) -> pb::ProposalSubmit {
        pb::ProposalSubmit {
            proposal: Some(value.body.proposal.into()),
            proposer: Some(value.body.proposer.into()),
            governance_key: Some(value.body.governance_key.into()),
            auth_sig: Some(value.auth_sig.into()),
        }
    }
}

impl TryFrom<pb::ProposalSubmit> for ProposalSubmit {
    type Error = anyhow::Error;

    fn try_from(msg: pb::ProposalSubmit) -> Result<Self, Self::Error> {
        Ok(Self {
            body: ProposalSubmitBody {
                proposal: msg
                    .proposal
                    .ok_or_else(|| anyhow::anyhow!("missing proposal in `ProposalSubmit`"))?
                    .try_into()?,
                proposer: msg
                    .proposer
                    .ok_or_else(|| anyhow::anyhow!("missing proposer in `ProposalSubmit`"))?
                    .try_into()?,
                governance_key: msg
                    .governance_key
                    .ok_or_else(|| anyhow::anyhow!("missing governance key in `ProposalSubmit`"))?
                    .try_into()?,
            },
            auth_sig: msg
                .auth_sig
                .ok_or_else(|| anyhow::anyhow!("missing auth sig in `ProposalSubmit`"))?
                .try_into()?,
        })
    }
}

impl DomainType for ProposalSubmit {
    type Proto = pb::ProposalSubmit;
}
