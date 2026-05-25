use ibc_types::core::client::ClientId;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

use crate::change::ParameterChange;
use penumbra_sdk_compliance::UpdateAssetIbcPolicy;
use penumbra_sdk_proto::{penumbra::core::component::governance::v1 as pb, DomainType};

/// A governance proposal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "pb::Proposal", into = "pb::Proposal")]
pub struct Proposal {
    /// The ID number of the proposal.
    pub id: u64,

    /// A short title describing the intent of the proposal.
    pub title: String,

    /// A natural-language description of the effect of the proposal and its justification.
    pub description: String,

    /// The specific kind and attributes of the proposal.
    pub payload: ProposalPayload,
}

impl From<Proposal> for pb::Proposal {
    fn from(inner: Proposal) -> pb::Proposal {
        let mut proposal = pb::Proposal {
            id: inner.id,
            title: inner.title,
            description: inner.description,
            ..Default::default() // We're about to fill in precisely one of the fields for the payload
        };
        use pb::proposal::Payload;
        let payload = match inner.payload {
            ProposalPayload::Signaling { commit } => {
                Some(Payload::Signaling(pb::proposal::Signaling {
                    commit: if let Some(c) = commit {
                        c
                    } else {
                        String::default()
                    },
                }))
            }
            ProposalPayload::Emergency { halt_chain } => {
                Some(Payload::Emergency(pb::proposal::Emergency { halt_chain }))
            }
            ProposalPayload::ParameterChange(change) => {
                Some(Payload::ParameterChange(change.into()))
            }
            ProposalPayload::UpgradePlan { height } => {
                Some(Payload::UpgradePlan(pb::proposal::UpgradePlan { height }))
            }
            ProposalPayload::FreezeIbcClient { client_id } => {
                Some(Payload::FreezeIbcClient(pb::proposal::FreezeIbcClient {
                    client_id: client_id.into(),
                }))
            }
            ProposalPayload::UnfreezeIbcClient { client_id } => Some(Payload::UnfreezeIbcClient(
                pb::proposal::UnfreezeIbcClient {
                    client_id: client_id.into(),
                },
            )),
            ProposalPayload::UpdateAssetIbcPolicy(update) => {
                Some(Payload::UpdateAssetIbcPolicy(update.into()))
            }
        };
        proposal.payload = payload;
        proposal
    }
}

impl TryFrom<pb::Proposal> for Proposal {
    type Error = anyhow::Error;

    fn try_from(inner: pb::Proposal) -> Result<Proposal, Self::Error> {
        // Validation (matches limits from `impl AppActionHandler for ProposalSubmit`):
        // - Title has a max length of 80 chars
        if inner.title.len() > 80 {
            anyhow::bail!("proposal title field must be less than 80 characters");
        }

        // - Description has a max length of 10_000 chars
        if inner.description.len() > 10_000 {
            anyhow::bail!("proposal description must be less than 10,000 characters");
        }

        use pb::proposal::Payload;
        Ok(Proposal {
            id: inner.id,
            title: inner.title,
            description: inner.description,
            payload: match inner
                .payload
                .ok_or_else(|| anyhow::anyhow!("missing proposal payload"))?
            {
                Payload::Signaling(signaling) => ProposalPayload::Signaling {
                    commit: if signaling.commit.is_empty() {
                        None
                    } else {
                        // Commit hash has max length of 255 bytes:
                        if signaling.commit.len() > 255 {
                            anyhow::bail!("proposal commit hash must be less than 255 bytes");
                        }

                        Some(signaling.commit)
                    },
                },
                Payload::Emergency(emergency) => ProposalPayload::Emergency {
                    halt_chain: emergency.halt_chain,
                },
                Payload::ParameterChange(change) => {
                    ProposalPayload::ParameterChange(change.try_into()?)
                }
                Payload::UpgradePlan(upgrade_plan) => ProposalPayload::UpgradePlan {
                    height: upgrade_plan.height,
                },
                Payload::FreezeIbcClient(freeze_ibc_client) => {
                    // Validation: client ID has a max length of 128 bytes
                    if freeze_ibc_client.client_id.len() > 128 {
                        anyhow::bail!("client ID must be less than 128 bytes");
                    }
                    // Validation: Check the client ID is valid using the validation inside `ClientId::from_str`.
                    ClientId::from_str(&freeze_ibc_client.client_id)
                        .map_err(|e| anyhow::anyhow!("invalid client id: {e}"))?;
                    ProposalPayload::FreezeIbcClient {
                        client_id: freeze_ibc_client.client_id,
                    }
                }
                Payload::UnfreezeIbcClient(unfreeze_ibc_client) => {
                    // Validation: client ID has a max length of 128 bytes
                    if unfreeze_ibc_client.client_id.len() > 128 {
                        anyhow::bail!("client ID must be less than 128 bytes");
                    }
                    // Validation: Check the client ID is valid using the validation inside `ClientId::from_str`.
                    ClientId::from_str(&unfreeze_ibc_client.client_id)
                        .map_err(|e| anyhow::anyhow!("invalid client id: {e}"))?;
                    ProposalPayload::UnfreezeIbcClient {
                        client_id: unfreeze_ibc_client.client_id,
                    }
                }
                Payload::UpdateAssetIbcPolicy(update) => {
                    ProposalPayload::UpdateAssetIbcPolicy(update.try_into()?)
                }
            },
        })
    }
}

impl DomainType for Proposal {
    type Proto = pb::Proposal;
}

/// A human-readable TOML-serializable version of a proposal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalToml {
    pub id: u64,
    pub title: String,
    pub description: String,
    #[serde(flatten)]
    pub payload: ProposalPayloadToml,
}

impl From<Proposal> for ProposalToml {
    fn from(proposal: Proposal) -> ProposalToml {
        ProposalToml {
            id: proposal.id,
            title: proposal.title,
            description: proposal.description,
            payload: proposal.payload.into(),
        }
    }
}

impl TryFrom<ProposalToml> for Proposal {
    type Error = anyhow::Error;

    fn try_from(proposal: ProposalToml) -> Result<Proposal, Self::Error> {
        Ok(Proposal {
            id: proposal.id,
            title: proposal.title,
            description: proposal.description,
            payload: proposal.payload.try_into()?,
        })
    }
}

/// The specific kind of a proposal.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "pb::ProposalKind", into = "pb::ProposalKind")]
pub enum ProposalKind {
    /// A signaling proposal.
    Signaling,
    /// An emergency proposal.
    Emergency,
    /// A parameter change proposal.
    ParameterChange,
    /// An upgrade proposal.
    UpgradePlan,
    /// A proposal to freeze an IBC client.
    FreezeIbcClient,
    /// A proposal to unfreeze an IBC client.
    UnfreezeIbcClient,
    /// A proposal to replace a regulated asset's direct IBC routes.
    UpdateAssetIbcPolicy,
}

impl From<ProposalKind> for pb::ProposalKind {
    fn from(kind: ProposalKind) -> pb::ProposalKind {
        match kind {
            ProposalKind::Signaling => pb::ProposalKind::Signaling,
            ProposalKind::Emergency => pb::ProposalKind::Emergency,
            ProposalKind::ParameterChange => pb::ProposalKind::ParameterChange,
            ProposalKind::UpgradePlan => pb::ProposalKind::UpgradePlan,
            ProposalKind::FreezeIbcClient => pb::ProposalKind::FreezeIbcClient,
            ProposalKind::UnfreezeIbcClient => pb::ProposalKind::UnfreezeIbcClient,
            ProposalKind::UpdateAssetIbcPolicy => pb::ProposalKind::UpdateAssetIbcPolicy,
        }
    }
}

impl TryFrom<pb::ProposalKind> for ProposalKind {
    type Error = anyhow::Error;

    fn try_from(kind: pb::ProposalKind) -> anyhow::Result<ProposalKind> {
        let kind = match kind {
            pb::ProposalKind::Unspecified => anyhow::bail!("unspecified proposal kind"),
            pb::ProposalKind::Signaling => ProposalKind::Signaling,
            pb::ProposalKind::Emergency => ProposalKind::Emergency,
            pb::ProposalKind::ParameterChange => ProposalKind::ParameterChange,
            pb::ProposalKind::UpgradePlan => ProposalKind::UpgradePlan,
            pb::ProposalKind::FreezeIbcClient => ProposalKind::FreezeIbcClient,
            pb::ProposalKind::UnfreezeIbcClient => ProposalKind::UnfreezeIbcClient,
            pb::ProposalKind::UpdateAssetIbcPolicy => ProposalKind::UpdateAssetIbcPolicy,
        };
        Ok(kind)
    }
}

impl FromStr for ProposalKind {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "signaling" => Ok(ProposalKind::Signaling),
            "emergency" => Ok(ProposalKind::Emergency),
            "parameter_change" => Ok(ProposalKind::ParameterChange),
            "upgrade_plan" => Ok(ProposalKind::UpgradePlan),
            "freeze_ibc_client" => Ok(ProposalKind::FreezeIbcClient),
            "unfreeze_ibc_client" => Ok(ProposalKind::UnfreezeIbcClient),
            "update_asset_ibc_policy" => Ok(ProposalKind::UpdateAssetIbcPolicy),
            _ => Err(anyhow::anyhow!("invalid proposal kind: {}", s)),
        }
    }
}

impl Proposal {
    /// Get the kind of a proposal.
    pub fn kind(&self) -> ProposalKind {
        match self.payload {
            ProposalPayload::Signaling { .. } => ProposalKind::Signaling,
            ProposalPayload::Emergency { .. } => ProposalKind::Emergency,
            ProposalPayload::ParameterChange { .. } => ProposalKind::ParameterChange,
            ProposalPayload::UpgradePlan { .. } => ProposalKind::UpgradePlan,
            ProposalPayload::FreezeIbcClient { .. } => ProposalKind::FreezeIbcClient,
            ProposalPayload::UnfreezeIbcClient { .. } => ProposalKind::UnfreezeIbcClient,
            ProposalPayload::UpdateAssetIbcPolicy(_) => ProposalKind::UpdateAssetIbcPolicy,
        }
    }
}

/// The machine-interpretable body of a proposal.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ProposalPayload {
    /// A signaling proposal is merely for coordination; it does not enact anything automatically by
    /// itself.
    Signaling {
        /// An optional commit hash for code that this proposal refers to.
        commit: Option<String>,
    },
    /// An emergency proposal is immediately passed when 1/3 of all validators approve it, without
    /// waiting for the voting period to conclude.
    Emergency {
        /// If `halt_chain == true`, then the chain will immediately halt when the proposal is
        /// passed.
        halt_chain: bool,
    },
    /// A parameter change proposal describes a change to the app parameters, which should
    /// take effect when the proposal is passed.
    ParameterChange(ParameterChange),
    /// An upgrade plan proposal describes a planned upgrade to the chain. If ratified, the chain
    /// will halt at the specified height, trigger an epoch transition, and halt the chain.
    UpgradePlan { height: u64 },
    /// A proposal to freeze a specific IBC client.
    FreezeIbcClient {
        /// The identifier of the client to freeze.
        client_id: String,
    },
    /// A proposal to unfreeze a specific IBC client.
    UnfreezeIbcClient {
        /// The identifier of the client to unfreeze.
        client_id: String,
    },
    /// Replace direct IBC routes for an existing regulated asset.
    UpdateAssetIbcPolicy(UpdateAssetIbcPolicy),
}

/// A TOML-serializable version of `ProposalPayload`, meant for human consumption.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProposalPayloadToml {
    Signaling { commit: Option<String> },
    Emergency { halt_chain: bool },
    ParameterChange(ParameterChange),
    UpgradePlan { height: u64 },
    FreezeIbcClient { client_id: String },
    UnfreezeIbcClient { client_id: String },
    UpdateAssetIbcPolicy(UpdateAssetIbcPolicy),
}

impl TryFrom<ProposalPayloadToml> for ProposalPayload {
    type Error = anyhow::Error;

    fn try_from(toml: ProposalPayloadToml) -> Result<Self, Self::Error> {
        Ok(match toml {
            ProposalPayloadToml::Signaling { commit } => ProposalPayload::Signaling { commit },
            ProposalPayloadToml::Emergency { halt_chain } => {
                ProposalPayload::Emergency { halt_chain }
            }
            ProposalPayloadToml::ParameterChange(change) => {
                ProposalPayload::ParameterChange(change)
            }
            ProposalPayloadToml::UpgradePlan { height } => ProposalPayload::UpgradePlan { height },
            ProposalPayloadToml::FreezeIbcClient { client_id } => {
                ProposalPayload::FreezeIbcClient { client_id }
            }
            ProposalPayloadToml::UnfreezeIbcClient { client_id } => {
                ProposalPayload::UnfreezeIbcClient { client_id }
            }
            ProposalPayloadToml::UpdateAssetIbcPolicy(update) => {
                ProposalPayload::UpdateAssetIbcPolicy(update)
            }
        })
    }
}

impl From<ProposalPayload> for ProposalPayloadToml {
    fn from(payload: ProposalPayload) -> Self {
        match payload {
            ProposalPayload::Signaling { commit } => ProposalPayloadToml::Signaling { commit },
            ProposalPayload::Emergency { halt_chain } => {
                ProposalPayloadToml::Emergency { halt_chain }
            }
            ProposalPayload::ParameterChange(change) => {
                ProposalPayloadToml::ParameterChange(change)
            }
            ProposalPayload::UpgradePlan { height } => ProposalPayloadToml::UpgradePlan { height },
            ProposalPayload::FreezeIbcClient { client_id } => {
                ProposalPayloadToml::FreezeIbcClient { client_id }
            }
            ProposalPayload::UnfreezeIbcClient { client_id } => {
                ProposalPayloadToml::UnfreezeIbcClient { client_id }
            }
            ProposalPayload::UpdateAssetIbcPolicy(update) => {
                ProposalPayloadToml::UpdateAssetIbcPolicy(update)
            }
        }
    }
}

impl ProposalPayload {
    pub fn is_signaling(&self) -> bool {
        matches!(self, ProposalPayload::Signaling { .. })
    }

    pub fn is_emergency(&self) -> bool {
        matches!(self, ProposalPayload::Emergency { .. })
    }

    pub fn is_ibc_freeze(&self) -> bool {
        matches!(self, ProposalPayload::FreezeIbcClient { .. })
            || matches!(self, ProposalPayload::UnfreezeIbcClient { .. })
    }

    pub fn is_parameter_change(&self) -> bool {
        matches!(self, ProposalPayload::ParameterChange { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::ProposalKind;
    use std::str::FromStr;

    #[test]
    fn proposal_kind_parses_ibc_freeze_variants() {
        assert_eq!(
            ProposalKind::from_str("freeze_ibc_client").expect("freeze kind should parse"),
            ProposalKind::FreezeIbcClient
        );
        assert_eq!(
            ProposalKind::from_str("unfreeze_ibc_client").expect("unfreeze kind should parse"),
            ProposalKind::UnfreezeIbcClient
        );
    }
}
