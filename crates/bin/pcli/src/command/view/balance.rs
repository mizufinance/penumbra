use anyhow::{Context, Result};
use comfy_table::{presets, Table};
use futures::StreamExt;

use penumbra_sdk_asset::Value;
use penumbra_sdk_keys::{AddressView, AssetViewingKey};
use penumbra_sdk_proto::core::component::compact_block::v1::{
    query_service_client::QueryServiceClient as CompactBlockQueryServiceClient,
    CompactBlockRangeRequest,
};
use penumbra_sdk_proto::core::component::shielded_pool::v1::{
    query_service_client::QueryServiceClient as ShieldedPoolQueryServiceClient,
    AssetMetadataByIdRequest,
};
use penumbra_sdk_proto::util::tendermint_proxy::v1::{
    tendermint_proxy_service_client::TendermintProxyServiceClient,
    GetStatusRequest,
};
use penumbra_sdk_sct::CommitmentSource;
use penumbra_sdk_shielded_pool::{Note, NotePayload};
use penumbra_sdk_view::ViewClient;

use crate::App;

#[derive(Debug, clap::Args)]
pub struct BalanceCmd {
    #[clap(long)]
    /// If set, prints the value of each note individually.
    pub by_note: bool,

    #[clap(long)]
    /// If set, query balances using an asset viewing key instead of the wallet's full viewing key.
    /// This allows querying balances for a specific asset only.
    /// When using this flag without a configured wallet, you must also provide --grpc-url.
    pub asset_viewing_key: Option<String>,
}

impl BalanceCmd {
    pub fn offline(&self) -> bool {
        false
    }

    /// Execute with just a GRPC URL and asset viewing key (no wallet required)
    pub async fn exec_standalone(&self, avk_str: &str, grpc_url: url::Url) -> Result<()> {
        self.exec_with_asset_viewing_key_standalone(avk_str, grpc_url).await
    }

    pub async fn exec(&self, app: &mut App) -> Result<()> {
        // If an asset viewing key is provided, we need to scan the chain directly
        if let Some(avk_str) = &self.asset_viewing_key {
            let grpc_url = app.config.grpc_url.clone();
            return self.exec_with_asset_viewing_key_standalone(avk_str, grpc_url).await;
        }

        // Otherwise, use the normal flow with the wallet's view service
        let view = app.view();
        let asset_cache = view.assets().await?;

        // Initialize the table
        let mut table = Table::new();
        table.load_preset(presets::NOTHING);

        let notes = view.unspent_notes_by_account_and_asset().await?;

        if self.by_note {
            table.set_header(vec!["Account", "Value", "Source", "Sender"]);

            let rows = notes
                .iter()
                .flat_map(|(index, notes_by_asset)| {
                    // Include each note individually:
                    notes_by_asset.iter().flat_map(|(asset, notes)| {
                        notes.iter().map(|record| {
                            (
                                *index,
                                asset.value(record.note.amount()),
                                record.source.clone(),
                                record.return_address.clone(),
                            )
                        })
                    })
                });

            for (index, value, source, return_address) in rows {
                table.add_row(vec![
                    format!("# {}", index),
                    value.format(&asset_cache),
                    format_source(&source),
                    format_return_address(&return_address),
                ]);
            }

            println!("{table}");

            return Ok(());
        } else {
            table.set_header(vec!["Account", "Amount"]);

            let rows = notes
                .iter()
                .flat_map(|(index, notes_by_asset)| {
                    // Sum the notes for each asset:
                    notes_by_asset.iter().map(|(asset, notes)| {
                        let sum: u128 = notes
                            .iter()
                            .map(|record| u128::from(record.note.amount()))
                            .sum();
                        (*index, asset.value(sum.into()))
                    })
                })
                // Exclude withdrawn LPNFTs and withdrawn auction NFTs.
                .filter(|(_, value)| match asset_cache.get(&value.asset_id) {
                    None => true,
                    Some(denom) => {
                        !denom.is_withdrawn_position_nft() && !denom.is_withdrawn_auction_nft()
                    }
                });

            for (index, value) in rows {
                table.add_row(vec![format!("# {}", index), value.format(&asset_cache)]);
            }

            println!("{table}");

            return Ok(());
        }
    }

    async fn exec_with_asset_viewing_key_standalone(&self, avk_str: &str, grpc_url: url::Url) -> Result<()> {
        // Parse the asset viewing key
        let avk: AssetViewingKey = avk_str
            .parse()
            .context("Failed to parse asset viewing key")?;

        let asset_id = avk.asset_id();
        let ivk = avk.incoming_viewing_key();

        // Connect to the chain to fetch compact blocks
        let channel = penumbra_sdk_proto::box_grpc_svc::connect(
            tonic::transport::Endpoint::new(grpc_url.to_string())?
        ).await?;
        let mut tm_client = TendermintProxyServiceClient::new(channel.clone());
        let mut cb_client = CompactBlockQueryServiceClient::new(channel.clone());

        // Get the current block height
        let status = tm_client
            .get_status(GetStatusRequest {})
            .await?
            .into_inner();
        let latest_height = status
            .sync_info
            .ok_or_else(|| anyhow::anyhow!("missing sync info"))?
            .latest_block_height;

        eprintln!("Scanning blocks from height 0 to {}", latest_height);

        // Fetch compact blocks and scan them
        let request = CompactBlockRangeRequest {
            start_height: 0,
            end_height: latest_height,
            keep_alive: false,
        };

        let mut stream = cb_client.compact_block_range(request).await?.into_inner();

        // Track discovered notes - store amount as u128 for each note
        let mut notes: Vec<(u128, u64, CommitmentSource)> = Vec::new();
        let mut blocks_scanned = 0u64;

        while let Some(compact_block_response) = stream.next().await {
            let compact_block = compact_block_response?
                .compact_block
                .ok_or_else(|| anyhow::anyhow!("missing compact block"))?;

            let height = compact_block.height;

            // Scan all state payloads in this block
            for state_payload_wrapper in compact_block.state_payloads {
                use penumbra_sdk_proto::core::component::compact_block::v1::state_payload::StatePayload as StatePayloadEnum;

                if let Some(StatePayloadEnum::Note(note_wrapper)) = state_payload_wrapper.state_payload {
                    if let Some(note_payload_proto) = note_wrapper.note {
                        // Try to deserialize and decrypt the note
                        if let Ok(note_payload) = NotePayload::try_from(note_payload_proto) {
                            // Try to decrypt with the IVK from the asset viewing key
                            if let Ok(note) = Note::decrypt(&note_payload.encrypted_note, ivk, &note_payload.ephemeral_key) {
                                // Check if this note matches the asset ID we're looking for
                                if note.asset_id() == asset_id {
                                    // Verify the commitment matches
                                    if note.commit() == note_payload.note_commitment {
                                        let amount = u128::from(note.amount());

                                        let source = state_payload_wrapper
                                            .source
                                            .and_then(|s| s.try_into().ok())
                                            .unwrap_or(CommitmentSource::Genesis);

                                        notes.push((amount, height, source));
                                    }
                                }
                            }
                        }
                    }
                }
            }

            blocks_scanned += 1;
            if blocks_scanned % 1000 == 0 {
                eprint!("\rScanned {} blocks...", blocks_scanned);
            }
        }

        if blocks_scanned > 0 {
            eprintln!("\rScanned {} blocks total.", blocks_scanned);
        }

        // Query asset metadata from the chain
        let mut sp_client = ShieldedPoolQueryServiceClient::new(channel.clone());
        let metadata_response = sp_client
            .asset_metadata_by_id(AssetMetadataByIdRequest {
                asset_id: Some(asset_id.into()),
            })
            .await?;

        // Build an asset cache with the metadata
        let asset_cache = if let Some(denom_metadata) = metadata_response.into_inner().denom_metadata {
            let metadata: penumbra_sdk_asset::asset::Metadata = denom_metadata.try_into()?;
            vec![metadata].into_iter().collect()
        } else {
            // If we can't find the metadata, create an empty cache
            penumbra_sdk_asset::asset::Cache::default()
        };

        // Display results
        let mut table = Table::new();
        table.load_preset(presets::NOTHING);

        if self.by_note {
            table.set_header(vec!["Amount", "Height", "Source"]);

            for (amount, height, source) in notes.iter() {
                let value = Value {
                    amount: (*amount).into(),
                    asset_id,
                };
                table.add_row(vec![
                    value.format(&asset_cache),
                    height.to_string(),
                    format_source(source),
                ]);
            }
        } else {
            table.set_header(vec!["Total Amount"]);

            // Sum all notes
            let total: u128 = notes.iter().map(|(amount, _, _)| amount).sum();

            let value = Value {
                amount: total.into(),
                asset_id,
            };

            table.add_row(vec![value.format(&asset_cache)]);
        }

        println!("{table}");

        Ok(())
    }
}

fn format_source(source: &CommitmentSource) -> String {
    match source {
        CommitmentSource::Genesis => "Genesis".to_owned(),
        CommitmentSource::Transaction { id: None } => "Tx (Unknown)".to_owned(),
        CommitmentSource::Transaction { id: Some(id) } => format!("Tx {}", hex::encode(&id[..])),
        CommitmentSource::FundingStreamReward { epoch_index } => {
            format!("Funding Stream (Epoch {})", epoch_index)
        }
        CommitmentSource::CommunityPoolOutput => format!("CommunityPoolOutput"),
        CommitmentSource::Ics20Transfer {
            packet_seq,
            channel_id,
            sender,
        } => format!(
            "ICS20 packet {} via {} from {}",
            packet_seq, channel_id, sender
        ),
        CommitmentSource::LiquidityTournamentReward { epoch, tx_hash } => {
            format!(
                "Liquidity tournament reward (Epoch {}, Tx {})",
                epoch, tx_hash
            )
        }
    }
}

fn format_return_address(return_address: &Option<penumbra_sdk_keys::AddressView>) -> String {
    match return_address {
        None => "Unknown".to_owned(),
        Some(AddressView::Opaque { address }) => address.display_short_form(),
        Some(AddressView::Decoded { index, .. }) => {
            if index.is_ephemeral() {
                format!("[account {} (IBC deposit address)]", index.account)
            } else {
                format!("[account {}]", index.account)
            }
        }
    }
}
