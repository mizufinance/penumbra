use anyhow::Result;

use penumbra_sdk_app::params::AppParameters;
use penumbra_sdk_governance::{change::ParameterChange, Proposal, ProposalPayload};

use super::FeeTier;

#[derive(Debug, clap::Subcommand)]
pub enum ProposalCmd {
    /// Make a template file for a new proposal.
    Template {
        /// The file to output the template to.
        #[clap(long, global = true)]
        file: Option<camino::Utf8PathBuf>,
        /// The kind of the proposal to template.
        #[clap(subcommand)]
        kind: ProposalKindCmd,
    },
    /// Submit a new governance proposal.
    Submit {
        /// The proposal to vote on, in TOML format.
        #[clap(long)]
        file: camino::Utf8PathBuf,
        /// Only spend funds originally received by the given account.
        #[clap(long, default_value = "0")]
        source: u32,
        /// The selected fee tier to multiply the fee amount by.
        #[clap(short, long, default_value_t)]
        fee_tier: FeeTier,
    },
}

#[derive(Debug, clap::Subcommand)]
pub enum ProposalKindCmd {
    /// Generate a template for a signaling proposal.
    Signaling,
    /// Generate a template for an emergency proposal.
    Emergency,
    /// Generate a template for a parameter change proposal.
    ParameterChange,
    /// Generate a template for a Freeze IBC Client proposal.
    FreezeIbcClient {
        /// The IBC client identifier to freeze.
        #[clap(long, default_value = "07-tendermint-0")]
        client_id: String,
    },
    /// Generate a template for an Unfreeze IBC Client proposal.
    UnfreezeIbcClient {
        /// The IBC client identifier to unfreeze.
        #[clap(long, default_value = "07-tendermint-0")]
        client_id: String,
    },
    /// Generate a template for an upgrade proposal,
    UpgradePlan,
}

impl ProposalKindCmd {
    /// Generate a default proposal of a particular kind.
    pub fn template_proposal(&self, app_params: &AppParameters, id: u64) -> Result<Proposal> {
        let title = "A short title (at most 80 characters)".to_string();
        let description = "A longer description (at most 10,000 characters)".to_string();
        let payload = match self {
            ProposalKindCmd::Signaling => ProposalPayload::Signaling { commit: None },
            ProposalKindCmd::Emergency => ProposalPayload::Emergency { halt_chain: false },
            ProposalKindCmd::ParameterChange => {
                ProposalPayload::ParameterChange(ParameterChange::encode_parameters(
                    serde_json::value::to_value(app_params.clone())?,
                ))
            }
            ProposalKindCmd::FreezeIbcClient { client_id } => ProposalPayload::FreezeIbcClient {
                client_id: client_id.clone(),
            },
            ProposalKindCmd::UnfreezeIbcClient { client_id } => {
                ProposalPayload::UnfreezeIbcClient {
                    client_id: client_id.clone(),
                }
            }
            ProposalKindCmd::UpgradePlan { .. } => ProposalPayload::UpgradePlan { height: 0 },
        };

        Ok(Proposal {
            id,
            title,
            description,
            payload,
        })
    }
}

impl ProposalCmd {
    pub fn offline(&self) -> bool {
        match self {
            ProposalCmd::Template { .. } => false,
            ProposalCmd::Submit { .. } => false,
        }
    }
}
