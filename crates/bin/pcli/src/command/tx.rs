use std::{
    fs::{self, File},
    io::{Read, Write},
    path::PathBuf,
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use ibc_proto::ibc::core::client::v1::{
    query_client::QueryClient as IbcClientQueryClient, QueryClientStateRequest,
};
use ibc_proto::ibc::core::connection::v1::query_client::QueryClient as IbcConnectionQueryClient;
use ibc_proto::ibc::core::{
    channel::v1::{query_client::QueryClient as IbcChannelQueryClient, QueryChannelRequest},
    connection::v1::QueryConnectionRequest,
};
use ibc_types::core::{
    channel::{ChannelId, PortId},
    client::Height as IbcHeight,
};
use ibc_types::lightclients::tendermint::client_state::ClientState as TendermintClientState;
use rand_core::OsRng;
use regex::Regex;

use compliance::ComplianceCmd;
use penumbra_sdk_asset::{asset, asset::Metadata, Value};
use penumbra_sdk_fee::FeeTier;
use penumbra_sdk_governance::{proposal::ProposalToml, ProposalSubmit, ProposalSubmitBody};
use penumbra_sdk_keys::{keys::AddressIndex, Address};
use penumbra_sdk_num::Amount;
use penumbra_sdk_proto::{
    core::component::governance::v1::{
        query_service_client::QueryServiceClient as GovernanceQueryServiceClient,
        NextProposalIdRequest,
    },
    cosmos::tx::v1beta1::{
        mode_info::{Single, Sum},
        service_client::ServiceClient as CosmosServiceClient,
        AuthInfo as CosmosAuthInfo, BroadcastTxRequest as CosmosBroadcastTxRequest,
        Fee as CosmosFee, ModeInfo, SignerInfo as CosmosSignerInfo, Tx as CosmosTx,
        TxBody as CosmosTxBody,
    },
    noble::forwarding::v1::{ForwardingPubKey, MsgRegisterAccount},
    view::v1::GasPricesRequest,
    Message, Name as _,
};
use penumbra_sdk_shielded_pool::{ConsolidateFamilyId, Ics20Withdrawal};
use penumbra_sdk_transaction::Transaction;
use penumbra_sdk_validator::{GovernanceKey, IdentityKey};
use penumbra_sdk_view::{NoteManager, TransferPlanningResult, ViewClient};
use proposal::ProposalCmd;
use tonic::transport::{Channel, ClientTlsConfig};
use url::Url;

use crate::App;
use clap::Parser;

mod compliance;
mod proposal;

#[derive(Debug, Parser)]
pub struct TxCmdWithOptions {
    /// If present, a file to save the transaction to instead of broadcasting it
    #[clap(long)]
    pub offline: Option<PathBuf>,
    #[clap(subcommand)]
    pub cmd: TxCmd,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum ConsolidateFamilyArg {
    #[clap(name = "2x1")]
    TwoByOne,
    #[clap(name = "4x1")]
    FourByOne,
    #[clap(name = "8x1")]
    EightByOne,
}

impl From<ConsolidateFamilyArg> for ConsolidateFamilyId {
    fn from(value: ConsolidateFamilyArg) -> Self {
        match value {
            ConsolidateFamilyArg::TwoByOne => ConsolidateFamilyId::TwoByOne,
            ConsolidateFamilyArg::FourByOne => ConsolidateFamilyId::FourByOne,
            ConsolidateFamilyArg::EightByOne => ConsolidateFamilyId::EightByOne,
        }
    }
}

impl TxCmdWithOptions {
    /// Determine if this command requires a network sync before it executes.
    pub fn offline(&self) -> bool {
        self.cmd.offline()
    }

    pub async fn exec(&self, app: &mut App) -> Result<()> {
        app.save_transaction_here_instead = self.offline.clone();
        self.cmd.exec(app).await
    }
}

#[derive(Debug, clap::Subcommand)]
pub enum TxCmd {
    /// Transfer funds to a Penumbra address.
    #[clap(name = "transfer", display_order = 100)]
    Transfer {
        /// The destination address to transfer funds to.
        #[clap(long, display_order = 100)]
        to: String,
        /// The amounts to transfer, written as typed values 1.87penumbra, 12cubes, etc.
        values: Vec<String>,
        /// Only spend funds originally received by the given account.
        #[clap(long, default_value = "0", display_order = 300)]
        source: u32,
        /// Optional. Set the transaction's memo field to the provided text.
        #[clap(long)]
        memo: Option<String>,
        /// The selected fee tier to multiply the fee amount by.
        #[clap(short, long, default_value_t)]
        fee_tier: FeeTier,
    },
    /// Consolidate many notes of one asset into a larger note.
    #[clap(display_order = 101)]
    Consolidate {
        /// The asset to consolidate, expressed as a display denom.
        asset: String,
        /// Only spend funds originally received by the given account.
        #[clap(long, default_value = "0", display_order = 300)]
        source: u32,
        /// Optional. Select a specific consolidate family.
        #[clap(long, value_enum)]
        family: Option<ConsolidateFamilyArg>,
        /// The selected fee tier to multiply the fee amount by.
        #[clap(short, long, default_value_t)]
        fee_tier: FeeTier,
    },
    /// Split a spendable note into exactly 4 or 8 notes of the same asset.
    #[clap(display_order = 102)]
    Split {
        /// The note commitment of the spendable note to split, as hex.
        #[clap(long)]
        note_commitment: String,
        /// The output values to create, written as typed values using the note's asset.
        #[clap(min_values = 4, required = true)]
        values: Vec<String>,
        /// Only spend funds originally received by the given account.
        #[clap(long, default_value = "0", display_order = 300)]
        source: u32,
        /// The selected fee tier to multiply the fee amount by.
        #[clap(short, long, default_value_t)]
        fee_tier: FeeTier,
    },
    /// Submit or vote on a governance proposal.
    #[clap(display_order = 500, subcommand)]
    Proposal(ProposalCmd),
    /// Compliance-related transactions (asset and user registration).
    #[clap(display_order = 550, subcommand)]
    Compliance(ComplianceCmd),
    /// Perform a shielded ICS-20 withdrawal, moving funds from the Penumbra chain
    /// to a counterparty chain.
    ///
    /// For a withdrawal to be processed on the counterparty, IBC packets must be relayed between
    /// the two chains. Relaying is out of scope for the `pcli` tool.
    #[clap(name = "withdraw", display_order = 250)]
    ShieldedIcs20Withdrawal {
        /// Address on the receiving chain,
        /// e.g. cosmos1grgelyng2v6v3t8z87wu3sxgt9m5s03xvslewd. The chain_id for the counterparty
        /// chain will be discovered automatically, based on the `--channel` setting.
        #[clap(long)]
        to: String,
        /// The value to withdraw, eg "1000upenumbra"
        value: String,
        /// The IBC channel on the primary Penumbra chain to use for performing the withdrawal.
        /// This channel must already exist, as configured by a relayer client.
        /// You can search for channels via e.g. `pcli query ibc channel transfer 0`.
        #[clap(long)]
        channel: u64,
        /// Block height on the counterparty chain, after which the withdrawal will be considered
        /// invalid if not already relayed. Must be specified as a tuple of revision number and block
        /// height, e.g. `5-1000000` means "chain revision 5, block height of 1000000".
        /// You must know the chain id of the counterparty chain beforehand, e.g. `osmosis-testnet-5`,
        /// to know the revision number.
        #[clap(long, display_order = 100)]
        timeout_height: Option<IbcHeight>,
        /// Timestamp, specified in epoch time, after which the withdrawal will be considered
        /// invalid if not already relayed.
        #[clap(long, default_value = "0", display_order = 150)]
        timeout_timestamp: u64,
        /// Only withdraw funds from the specified wallet id within Penumbra.
        #[clap(long, default_value = "0", display_order = 200)]
        source: u32,
        /// Optional. Set the IBC ICS-20 packet memo field to the provided text.
        #[clap(long)]
        memo: Option<String>,
        /// The selected fee tier to multiply the fee amount by.
        #[clap(short, long, default_value_t)]
        fee_tier: FeeTier,
        /// Whether to use a transparent address (bech32, 32-byte) for
        /// the return address in the withdrawal.
        /// Required for some chains for a successful acknowledgement.
        #[clap(long)]
        use_transparent_address: bool,
    },
    #[clap(display_order = 970)]
    /// Register a Noble forwarding account.
    RegisterForwardingAccount {
        /// The Noble node to submit the registration transaction to.
        #[clap(long)]
        noble_node: Url,
        /// The Noble IBC channel to use for forwarding.
        #[clap(long)]
        channel: String,
        /// The Penumbra address or address index to receive forwarded funds.
        #[clap(long)]
        address_or_index: String,
        /// Whether or not to use an ephemeral address.
        #[clap(long)]
        ephemeral: bool,
    },
    /// Broadcast a saved transaction to the network
    #[clap(display_order = 1000)]
    Broadcast {
        /// The transaction to be broadcast
        transaction: PathBuf,
    },
}

/// Vote on a governance proposal.
#[derive(Debug, Clone, Copy, clap::Subcommand)]
pub enum VoteCmd {
    /// Vote in favor of a proposal.
    #[clap(display_order = 100)]
    Yes {
        /// The proposal ID to vote on.
        #[clap(long = "on")]
        proposal_id: u64,
    },
    /// Vote against a proposal.
    #[clap(display_order = 200)]
    No {
        /// The proposal ID to vote on.
        #[clap(long = "on")]
        proposal_id: u64,
    },
    /// Abstain from voting on a proposal.
    #[clap(display_order = 300)]
    Abstain {
        /// The proposal ID to vote on.
        #[clap(long = "on")]
        proposal_id: u64,
    },
}

impl From<VoteCmd> for (u64, penumbra_sdk_governance::Vote) {
    fn from(cmd: VoteCmd) -> (u64, penumbra_sdk_governance::Vote) {
        match cmd {
            VoteCmd::Yes { proposal_id } => (proposal_id, penumbra_sdk_governance::Vote::Yes),
            VoteCmd::No { proposal_id } => (proposal_id, penumbra_sdk_governance::Vote::No),
            VoteCmd::Abstain { proposal_id } => {
                (proposal_id, penumbra_sdk_governance::Vote::Abstain)
            }
        }
    }
}

impl TxCmd {
    /// Determine if this command requires a network sync before it executes.
    pub fn offline(&self) -> bool {
        match self {
            TxCmd::Transfer { .. } => false,
            TxCmd::Consolidate { .. } => false,
            TxCmd::Split { .. } => false,
            TxCmd::Proposal(proposal_cmd) => proposal_cmd.offline(),
            TxCmd::Compliance(compliance_cmd) => compliance_cmd.offline(),
            TxCmd::ShieldedIcs20Withdrawal { .. } => false,
            TxCmd::Broadcast { .. } => false,
            TxCmd::RegisterForwardingAccount { .. } => false,
        }
    }

    pub async fn exec(&self, app: &mut App) -> Result<()> {
        // Handle compliance commands that don't need wallet/view service early
        if let TxCmd::Compliance(compliance_cmd) = self {
            if compliance_cmd.is_scan() {
                return compliance_cmd.exec_scan().await;
            }
            if compliance_cmd.is_decrypt() {
                return compliance_cmd.exec_decrypt().await;
            }
            if compliance_cmd.is_generate_dk() {
                return compliance_cmd.exec_generate_dk();
            }
            if compliance_cmd.is_issuer_db() {
                return compliance_cmd.exec_issuer_db().await;
            }
        }

        // The reduced chain prices gas only in the base asset.
        // TODO: fetching this here means that no tx commands
        // can be run in offline mode, which is a bit annoying
        let gas_prices = app
            .view
            .as_mut()
            .context("view service must be initialized")?
            .gas_prices(GasPricesRequest {})
            .await?
            .into_inner()
            .gas_prices
            .expect("gas prices must be available")
            .try_into()?;

        match self {
            TxCmd::Transfer {
                values,
                to,
                source: from,
                memo,
                fee_tier,
            } => {
                // Parse all of the values provided.
                let values = values
                    .iter()
                    .map(|v| v.parse())
                    .collect::<Result<Vec<Value>, _>>()?;
                let to = to
                    .parse::<Address>()
                    .map_err(|_| anyhow::anyhow!("address is invalid"))?;

                let mut note_manager = NoteManager::new(OsRng);
                note_manager
                    .set_gas_prices(gas_prices)
                    .set_fee_tier((*fee_tier).into());
                note_manager.memo(memo.clone().unwrap_or_default());
                match note_manager
                    .plan_transfer_values(
                        app.view
                            .as_mut()
                            .context("view service must be initialized")?,
                        AddressIndex::new(*from),
                        values,
                        to,
                    )
                    .await
                    .context("can't build transfer transaction")?
                {
                    TransferPlanningResult::Ready { transaction_plan } => {
                        app.build_and_submit_transaction(transaction_plan).await?;
                    }
                    TransferPlanningResult::NeedsMaintenance {
                        maintenance_plan, ..
                    } => {
                        anyhow::bail!(
                            "transfer requires note maintenance first; submit the suggested consolidate transaction and retry after finality: {:?}",
                            maintenance_plan
                        );
                    }
                    TransferPlanningResult::InsufficientBalance => {
                        anyhow::bail!("insufficient balance for requested transfer");
                    }
                    TransferPlanningResult::UnsupportedIntent { reason } => {
                        anyhow::bail!("{reason}");
                    }
                }
            }
            TxCmd::Consolidate {
                asset,
                source,
                family,
                fee_tier,
            } => {
                let asset_id = asset::REGISTRY.parse_unit(asset.as_str()).id();
                let mut note_manager = NoteManager::new(OsRng);
                note_manager
                    .set_gas_prices(gas_prices)
                    .set_fee_tier((*fee_tier).into());
                match note_manager
                    .plan_consolidate(
                        app.view
                            .as_mut()
                            .context("view service must be initialized")?,
                        AddressIndex::new(*source),
                        asset_id,
                        family.map(Into::into),
                    )
                    .await
                    .context("can't build consolidate transaction")?
                {
                    TransferPlanningResult::Ready { transaction_plan } => {
                        app.build_and_submit_transaction(transaction_plan).await?;
                    }
                    TransferPlanningResult::NeedsMaintenance { .. } => {
                        anyhow::bail!("consolidate planning unexpectedly requested maintenance");
                    }
                    TransferPlanningResult::InsufficientBalance => {
                        anyhow::bail!("insufficient balance for requested consolidate");
                    }
                    TransferPlanningResult::UnsupportedIntent { reason } => {
                        anyhow::bail!("{reason}");
                    }
                }
            }
            TxCmd::Split {
                note_commitment,
                values,
                source,
                fee_tier,
            } => {
                let note_commitment =
                    penumbra_sdk_shielded_pool::note::StateCommitment::parse_hex(note_commitment)
                        .map_err(|e| anyhow::anyhow!("invalid note commitment: {e}"))?;
                let note_record =
                    ViewClient::note_by_commitment(app.view(), note_commitment).await?;
                anyhow::ensure!(
                    note_record.address_index.account == *source,
                    "selected note is not controlled by source account {}",
                    source
                );

                let output_values = values
                    .iter()
                    .map(|value| value.parse::<Value>())
                    .collect::<Result<Vec<_>, _>>()?;
                let output_amounts = output_values
                    .iter()
                    .map(|value| {
                        anyhow::ensure!(
                            value.asset_id == note_record.note.asset_id(),
                            "split output {:?} must use the same asset as the selected note",
                            value
                        );
                        Ok(value.amount)
                    })
                    .collect::<Result<Vec<_>>>()?;

                let mut note_manager = NoteManager::new(OsRng);
                note_manager
                    .set_gas_prices(gas_prices)
                    .set_fee_tier((*fee_tier).into());
                match note_manager
                    .plan_split(
                        app.view
                            .as_mut()
                            .context("view service must be initialized")?,
                        AddressIndex::new(*source),
                        note_record,
                        output_amounts,
                    )
                    .await
                    .context("can't build split transaction")?
                {
                    TransferPlanningResult::Ready { transaction_plan } => {
                        app.build_and_submit_transaction(transaction_plan).await?;
                    }
                    TransferPlanningResult::NeedsMaintenance { .. } => {
                        anyhow::bail!("split planning unexpectedly requested maintenance");
                    }
                    TransferPlanningResult::InsufficientBalance => {
                        anyhow::bail!(
                            "selected note does not cover requested split outputs and fee"
                        );
                    }
                    TransferPlanningResult::UnsupportedIntent { reason } => {
                        anyhow::bail!("{reason}");
                    }
                }
            }
            TxCmd::Compliance(compliance_cmd) => {
                // Scan command is handled early in exec() before gas_prices fetch.
                // This branch only handles register-asset and register-user.
                let plan = compliance_cmd.plan(app, gas_prices).await?;
                app.build_and_submit_transaction(plan).await?;
            }
            TxCmd::Proposal(ProposalCmd::Submit {
                file,
                source,
                fee_tier,
            }) => {
                let mut proposal_file = File::open(file).context("can't open proposal file")?;
                let mut proposal_string = String::new();
                proposal_file
                    .read_to_string(&mut proposal_string)
                    .context("can't read proposal file")?;
                let proposal_toml: ProposalToml =
                    toml::from_str(&proposal_string).context("can't parse proposal file")?;
                let proposal: penumbra_sdk_governance::Proposal = proposal_toml
                    .try_into()
                    .context("can't parse proposal file")?;

                let fvk = app.config.full_viewing_key.clone();
                let proposer = IdentityKey(fvk.spend_verification_key().clone().into());
                let governance_key: GovernanceKey = app.config.governance_key();
                let body = ProposalSubmitBody {
                    proposal,
                    proposer,
                    governance_key,
                };
                let auth_sig = app.sign_proposal_submit(body.clone()).await?;
                let proposal_submit = ProposalSubmit { body, auth_sig };

                let mut note_manager = NoteManager::new(OsRng);
                note_manager
                    .set_gas_prices(gas_prices)
                    .set_fee_tier((*fee_tier).into());
                match note_manager
                    .plan_actions_with_transfer_funding(
                        app.view
                            .as_mut()
                            .context("view service must be initialized")?,
                        AddressIndex::new(*source),
                        vec![proposal_submit.into()],
                    )
                    .await
                    .context("can't build proposal submit transaction")?
                {
                    TransferPlanningResult::Ready { transaction_plan } => {
                        app.build_and_submit_transaction(transaction_plan).await?;
                    }
                    TransferPlanningResult::NeedsMaintenance {
                        maintenance_plan, ..
                    } => {
                        anyhow::bail!(
                            "proposal submission requires note maintenance first; submit the suggested consolidate transaction and retry after finality: {:?}",
                            maintenance_plan
                        );
                    }
                    TransferPlanningResult::InsufficientBalance => {
                        anyhow::bail!("insufficient balance for proposal submission fees");
                    }
                    TransferPlanningResult::UnsupportedIntent { reason } => {
                        anyhow::bail!("{reason}");
                    }
                }
            }
            TxCmd::Proposal(ProposalCmd::Template { file, kind }) => {
                let app_params = app.view().app_params().await?;

                // Find out what the latest proposal ID is so we can include the next ID in the template:
                let mut client = GovernanceQueryServiceClient::new(app.pd_channel().await?);
                let next_proposal_id: u64 = client
                    .next_proposal_id(NextProposalIdRequest {})
                    .await?
                    .into_inner()
                    .next_proposal_id;

                let toml_template: ProposalToml = kind
                    .template_proposal(&app_params, next_proposal_id)?
                    .into();

                if let Some(file) = file {
                    File::create(file)
                        .with_context(|| format!("cannot create file {file:?}"))?
                        .write_all(toml::to_string_pretty(&toml_template)?.as_bytes())
                        .context("could not write file")?;
                } else {
                    println!("{}", toml::to_string_pretty(&toml_template)?);
                }
            }
            TxCmd::ShieldedIcs20Withdrawal {
                to,
                value,
                timeout_height,
                timeout_timestamp,
                channel,
                source,
                memo,
                fee_tier,
                use_transparent_address,
            } => {
                let destination_chain_address = to;

                let ephemeral_return_address = if *use_transparent_address {
                    let ivk = app.config.full_viewing_key.incoming();

                    ivk.transparent_address()
                        .parse::<Address>()
                        .expect("we round-trip from a valid transparent address")
                } else {
                    app.config
                        .full_viewing_key
                        .ephemeral_address(OsRng, AddressIndex::from(*source))
                        .0
                };

                let timeout_height = match timeout_height {
                    Some(h) => h.clone(),
                    None => {
                        // look up the height for the counterparty and add 2 days of block time
                        // (assuming 10 seconds per block) to it

                        // look up the client state from the channel by looking up channel id -> connection id -> client state
                        let mut ibc_channel_client =
                            IbcChannelQueryClient::new(app.pd_channel().await?);

                        let req = QueryChannelRequest {
                            port_id: PortId::transfer().to_string(),
                            channel_id: format!("channel-{}", channel),
                        };

                        let channel = ibc_channel_client
                            .channel(req)
                            .await?
                            .into_inner()
                            .channel
                            .ok_or_else(|| anyhow::anyhow!("channel not found"))?;

                        let connection_id = channel.connection_hops[0].clone();

                        let mut ibc_connection_client =
                            IbcConnectionQueryClient::new(app.pd_channel().await?);

                        let req = QueryConnectionRequest {
                            connection_id: connection_id.clone(),
                        };
                        let connection = ibc_connection_client
                            .connection(req)
                            .await?
                            .into_inner()
                            .connection
                            .ok_or_else(|| anyhow::anyhow!("connection not found"))?;

                        let mut ibc_client_client =
                            IbcClientQueryClient::new(app.pd_channel().await?);
                        let req = QueryClientStateRequest {
                            client_id: connection.client_id,
                        };
                        let client_state = ibc_client_client
                            .client_state(req)
                            .await?
                            .into_inner()
                            .client_state
                            .ok_or_else(|| anyhow::anyhow!("client state not found"))?;

                        let tm_client_state = TendermintClientState::try_from(client_state)?;

                        let last_update_height = tm_client_state.latest_height;

                        // 10 seconds per block, 2 days
                        let timeout_n_blocks = ((24 * 60 * 60) / 10) * 2;

                        IbcHeight {
                            revision_number: last_update_height.revision_number,
                            revision_height: last_update_height.revision_height + timeout_n_blocks,
                        }
                    }
                };

                // get the current time on the local machine
                let current_time_ns = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("Time went backwards")
                    .as_nanos() as u64;

                let mut timeout_timestamp = *timeout_timestamp;
                if timeout_timestamp == 0u64 {
                    // add 2 days to current time
                    timeout_timestamp = current_time_ns + 1.728e14 as u64;
                }

                // round to the nearest 10 minutes
                timeout_timestamp += 600_000_000_000 - (timeout_timestamp % 600_000_000_000);

                fn parse_denom_and_amount(value_str: &str) -> anyhow::Result<(Amount, Metadata)> {
                    let denom_re = Regex::new(r"^([0-9.]+)(.+)$").context("denom regex invalid")?;
                    if let Some(captures) = denom_re.captures(value_str) {
                        let numeric_str = captures.get(1).expect("matched regex").as_str();
                        let denom_str = captures.get(2).expect("matched regex").as_str();

                        let display_denom = asset::REGISTRY.parse_unit(denom_str);
                        let amount = display_denom.parse_value(numeric_str)?;
                        let denom = display_denom.base();

                        Ok((amount, denom))
                    } else {
                        Err(anyhow::anyhow!("could not parse value"))
                    }
                }

                let (amount, denom) = parse_denom_and_amount(value)?;

                let withdrawal = Ics20Withdrawal {
                    destination_chain_address: destination_chain_address.to_string(),
                    denom,
                    amount,
                    timeout_height,
                    timeout_time: timeout_timestamp,
                    return_address: ephemeral_return_address,
                    // TODO: impl From<u64> for ChannelId
                    source_channel: ChannelId::from_str(format!("channel-{}", channel).as_ref())?,
                    use_compat_address: false,
                    ics20_memo: memo.clone().unwrap_or_default(),
                    use_transparent_address: *use_transparent_address,
                };

                let mut note_manager = NoteManager::new(OsRng);
                note_manager
                    .set_gas_prices(gas_prices)
                    .set_fee_tier((*fee_tier).into());
                match note_manager
                    .plan_ics20_withdrawal(
                        app.view
                            .as_mut()
                            .context("view service must be initialized")?,
                        AddressIndex::new(*source),
                        withdrawal,
                    )
                    .await
                    .context("can't build ICS-20 withdrawal transaction")?
                {
                    TransferPlanningResult::Ready { transaction_plan } => {
                        app.build_and_submit_transaction(transaction_plan).await?;
                    }
                    TransferPlanningResult::NeedsMaintenance {
                        maintenance_plan, ..
                    } => {
                        anyhow::bail!(
                            "ICS-20 withdrawal requires note maintenance first; submit the suggested consolidate transaction and retry after finality: {:?}",
                            maintenance_plan
                        );
                    }
                    TransferPlanningResult::InsufficientBalance => {
                        anyhow::bail!("insufficient balance for requested ICS-20 withdrawal");
                    }
                    TransferPlanningResult::UnsupportedIntent { reason } => {
                        anyhow::bail!("{reason}");
                    }
                }
            }
            TxCmd::Broadcast { transaction } => {
                let transaction: Transaction = serde_json::from_slice(&fs::read(transaction)?)?;
                app.submit_transaction(transaction).await?;
            }
            TxCmd::RegisterForwardingAccount {
                noble_node,
                channel,
                address_or_index,
                ephemeral,
            } => {
                let index: Result<u32, _> = address_or_index.parse();
                let fvk = app.config.full_viewing_key.clone();

                let address = if let Ok(index) = index {
                    // address index provided
                    let (address, _dtk) = match ephemeral {
                        false => fvk.incoming().payment_address(index.into()),
                        true => fvk.incoming().ephemeral_address(OsRng, index.into()),
                    };

                    address
                } else {
                    // address or nothing provided
                    let address: Address = address_or_index
                        .parse()
                        .map_err(|_| anyhow::anyhow!("Provided address is invalid."))?;

                    address
                };

                let noble_address = address.noble_forwarding_address(channel);

                println!(
                    "registering Noble forwarding account with address {} to forward to Penumbra address {}...",
                    noble_address, address
                );

                let mut noble_client = CosmosServiceClient::new(
                    Channel::from_shared(noble_node.to_string())?
                        .tls_config(ClientTlsConfig::new().with_webpki_roots())?
                        .connect()
                        .await?,
                );

                let tx = CosmosTx {
                    body: Some(CosmosTxBody {
                        messages: vec![pbjson_types::Any {
                            type_url: MsgRegisterAccount::type_url(),
                            value: MsgRegisterAccount {
                                signer: noble_address.to_string(),
                                recipient: address.to_string(),
                                channel: channel.to_string(),
                            }
                            .encode_to_vec()
                            .into(),
                        }],
                        memo: "".to_string(),
                        timeout_height: 0,
                        extension_options: vec![],
                        non_critical_extension_options: vec![],
                    }),
                    auth_info: Some(CosmosAuthInfo {
                        signer_infos: vec![CosmosSignerInfo {
                            public_key: Some(pbjson_types::Any {
                                type_url: ForwardingPubKey::type_url(),
                                value: ForwardingPubKey {
                                    key: noble_address.bytes(),
                                }
                                .encode_to_vec()
                                .into(),
                            }),
                            mode_info: Some(ModeInfo {
                                // SIGN_MODE_DIRECT
                                sum: Some(Sum::Single(Single { mode: 1 })),
                            }),
                            sequence: 0,
                        }],
                        fee: Some(CosmosFee {
                            amount: vec![],
                            gas_limit: 200000u64,
                            payer: "".to_string(),
                            granter: "".to_string(),
                        }),
                        tip: None,
                    }),
                    signatures: vec![vec![]],
                };
                let r = noble_client
                    .broadcast_tx(CosmosBroadcastTxRequest {
                        tx_bytes: tx.encode_to_vec().into(),
                        // sync
                        mode: 2,
                    })
                    .await?;

                // let r = noble_client
                //     .register_account(MsgRegisterAccount {
                //         signer: noble_address,
                //         recipient: address.to_string(),
                //         channel: channel.to_string(),
                //     })
                //     .await?;

                println!("Noble response: {:?}", r);
            }
        }

        Ok(())
    }
}
