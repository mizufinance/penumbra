use std::collections::BTreeMap;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use camino::Utf8PathBuf;
use futures::StreamExt;
use pcli::config::{CustodyConfig, PcliConfig};
use penumbra_sdk_asset::{asset, asset::Id, Value};
use penumbra_sdk_custody::{soft_kms::SoftKms, AuthorizeRequest, CustodyClient};
use penumbra_sdk_fee::{FeeTier, GasPrices};
use penumbra_sdk_keys::{keys::AddressIndex, Address, FullViewingKey};
use penumbra_sdk_proto::box_grpc_svc::{self, BoxGrpcService};
use penumbra_sdk_proto::custody::v1::{
    custody_service_client::CustodyServiceClient, custody_service_server::CustodyServiceServer,
};
use penumbra_sdk_proto::view::v1::{
    view_service_client::ViewServiceClient, view_service_server::ViewServiceServer, NotesRequest,
};
use penumbra_sdk_transaction::plan::ActionPlan;
use penumbra_sdk_transaction::{AuthorizationData, Transaction, TransactionPlan, WitnessData};
use penumbra_sdk_view::{Planner, SpendableNoteRecord, ViewClient, ViewServer};
use rand_core::OsRng;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::tps::corpus;

const VIEW_FILE_NAME: &str = "pcli-view.sqlite";
const REGISTRY_FILE_NAME: &str = "registry.json";

#[derive(Debug, Clone)]
pub struct BuildLocalArgs {
    pub scenario: String,
    pub wallet_home: PathBuf,
    pub asset: String,
    pub asset_kind: Option<String>,
    pub count: usize,
    pub source_start: usize,
    pub to_address: Address,
    pub out: PathBuf,
    pub observer: Option<String>,
    pub fee_tier: FeeTier,
    pub source_label: String,
    pub chain_id: Option<String>,
    pub genesis_hash: String,
    pub notes: String,
    pub concurrency: usize,
    pub sync: bool,
}

#[derive(Debug, Clone)]
pub struct WriteGenesisAllocationsArgs {
    pub wallet_home: PathBuf,
    pub count: usize,
    pub denom: String,
    pub amount: u128,
    pub out: PathBuf,
}

#[derive(Debug, Clone)]
pub struct CountLocalSourcesArgs {
    pub wallet_home: PathBuf,
    pub asset: String,
    pub sync: bool,
}

#[derive(Debug)]
struct PreparedTx {
    ordinal: usize,
    plan: TransactionPlan,
    auth_data: AuthorizationData,
    witness_data: WitnessData,
}

pub async fn build_local(args: BuildLocalArgs) -> Result<()> {
    anyhow::ensure!(args.count > 0, "--count must be greater than zero");
    anyhow::ensure!(
        args.concurrency > 0,
        "--concurrency must be greater than zero"
    );
    anyhow::ensure!(
        args.scenario == "regulated" || args.scenario == "unregulated",
        "--scenario must be regulated or unregulated"
    );
    let wallet_home = utf8_path_buf(&args.wallet_home)?;
    let config_path = wallet_home.join("config.toml");
    let view_path = wallet_home.join(VIEW_FILE_NAME);
    let registry_path = wallet_home.join(REGISTRY_FILE_NAME);

    let config = PcliConfig::load(&config_path)
        .with_context(|| format!("failed to load {}", config_path))?;
    anyhow::ensure!(
        config.view_url.is_none(),
        "build-local only supports local view wallets"
    );

    let fvk = config.full_viewing_key.clone();
    let mut custody = make_soft_kms_custody(&config)?;
    let mut view =
        make_local_view_client(&config, &wallet_home, &view_path, &registry_path).await?;

    if args.sync {
        sync_view(&mut view).await?;
    }

    let gas_prices = ViewClient::gas_prices(&mut view)
        .await
        .context("failed to fetch gas prices")?;
    let app_params = ViewClient::app_params(&mut view)
        .await
        .context("failed to fetch app params")?;
    let asset_cache = ViewClient::assets(&mut view)
        .await
        .context("failed to fetch asset metadata")?;
    let asset_id = resolve_asset_id(&asset_cache, &args.asset)?;
    let manifest_chain_id = args
        .chain_id
        .clone()
        .unwrap_or_else(|| app_params.chain_id.clone());
    let asset_kind = args
        .asset_kind
        .clone()
        .unwrap_or_else(|| args.asset.clone());

    let notes =
        select_distinct_source_notes(&mut view, asset_id, args.source_start, args.count).await?;
    eprintln!(
        "building {} corpus count={} asset={} distinct_sources={} concurrency={}",
        args.scenario,
        args.count,
        args.asset,
        notes.len(),
        args.concurrency
    );

    let mut prepared = Vec::with_capacity(notes.len());
    for (ordinal, note_record) in notes.into_iter().enumerate() {
        let plan = build_plan(
            &mut view,
            &gas_prices,
            args.fee_tier,
            &note_record,
            args.to_address.clone(),
        )
        .await
        .with_context(|| {
            format!(
                "failed building plan for source {:?}",
                note_record.address_index
            )
        })?;
        let mut plan = plan;
        if args.scenario == "unregulated" {
            normalize_unregulated_target_timestamps(&mut plan);
        }
        let auth_data = authorize_plan(&mut custody, &plan)
            .await
            .with_context(|| format!("failed authorizing tx ordinal={ordinal}"))?;
        let witness_data = ViewClient::witness(&mut view, &plan)
            .await
            .with_context(|| format!("failed witnessing tx ordinal={ordinal}"))?;
        prepared.push(PreparedTx {
            ordinal,
            plan,
            auth_data,
            witness_data,
        });
        if (ordinal + 1) % 100 == 0 || ordinal + 1 == args.count {
            eprintln!("prepared {}/{}", ordinal + 1, args.count);
        }
    }

    let txs = build_transactions_parallel(fvk, prepared, args.concurrency).await?;
    let manifest = corpus::Manifest {
        chain_id: manifest_chain_id,
        genesis_hash: args.genesis_hash.clone(),
        scenario: args.scenario.clone(),
        tx_count: txs.len(),
        created_at: unix_ts(),
        source_label: args.source_label.clone(),
        notes: args.notes.clone(),
        ..corpus::Manifest::default()
    };
    corpus::build_corpus_from_manifest(&args.out, &asset_kind, &manifest, &txs)?;

    if let Some(observer) = &args.observer {
        corpus::verify_corpus(
            &args.out,
            observer,
            &crate::tps::config::EndpointKind::TendermintProxy,
        )
        .await
        .with_context(|| format!("failed verifying corpus against {observer}"))?;
    }

    eprintln!("{} corpus ready: {}", args.scenario, args.out.display());
    Ok(())
}

pub fn write_genesis_allocations(args: WriteGenesisAllocationsArgs) -> Result<()> {
    anyhow::ensure!(args.count > 0, "--count must be greater than zero");
    anyhow::ensure!(args.amount > 0, "--amount must be greater than zero");

    let wallet_home = utf8_path_buf(&args.wallet_home)?;
    let config_path = wallet_home.join("config.toml");
    let config = PcliConfig::load(&config_path)
        .with_context(|| format!("failed to load {}", config_path))?;

    let parent = args
        .out
        .parent()
        .ok_or_else(|| anyhow::anyhow!("allocation output path must have a parent"))?;
    std::fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create allocation output dir {}",
            parent.display()
        )
    })?;

    let writer = File::create(&args.out)
        .with_context(|| format!("failed to create {}", args.out.display()))?;
    let mut csv_writer = csv::Writer::from_writer(writer);
    csv_writer.write_record(["amount", "denom", "address"])?;

    let ivk = config.full_viewing_key.incoming();
    for index in 0..args.count {
        let (address, _) = ivk.payment_address((index as u32).into());
        csv_writer.write_record([
            args.amount.to_string(),
            args.denom.clone(),
            address.to_string(),
        ])?;
    }
    csv_writer.flush()?;

    eprintln!(
        "wrote {} genesis allocations for denom={} to {}",
        args.count,
        args.denom,
        args.out.display()
    );
    Ok(())
}

pub async fn count_local_sources(args: CountLocalSourcesArgs) -> Result<usize> {
    let wallet_home = utf8_path_buf(&args.wallet_home)?;
    let config_path = wallet_home.join("config.toml");
    let view_path = wallet_home.join(VIEW_FILE_NAME);
    let registry_path = wallet_home.join(REGISTRY_FILE_NAME);

    let config = PcliConfig::load(&config_path)
        .with_context(|| format!("failed to load {}", config_path))?;
    anyhow::ensure!(
        config.view_url.is_none(),
        "count-local-sources only supports local view wallets"
    );

    let mut view =
        make_local_view_client(&config, &wallet_home, &view_path, &registry_path).await?;
    if args.sync {
        sync_view(&mut view).await?;
    }

    let asset_cache = ViewClient::assets(&mut view)
        .await
        .context("failed to fetch asset metadata")?;
    let asset_id = resolve_asset_id(&asset_cache, &args.asset)?;
    let count = count_distinct_source_notes(&mut view, asset_id).await?;
    println!("{count}");
    Ok(count)
}

fn utf8_path_buf(path: &PathBuf) -> Result<Utf8PathBuf> {
    Utf8PathBuf::from_path_buf(path.clone())
        .map_err(|_| anyhow::anyhow!("path is not valid UTF-8: {}", path.display()))
}

fn make_soft_kms_custody(config: &PcliConfig) -> Result<CustodyServiceClient<BoxGrpcService>> {
    let CustodyConfig::SoftKms(soft_kms_config) = &config.custody else {
        bail!("build-local only supports SoftKms wallets");
    };
    let soft_kms = SoftKms::new(soft_kms_config.clone());
    let custody_svc = CustodyServiceServer::new(soft_kms);
    Ok(CustodyServiceClient::new(box_grpc_svc::local(custody_svc)))
}

async fn make_local_view_client(
    config: &PcliConfig,
    wallet_home: &Utf8PathBuf,
    view_path: &Utf8PathBuf,
    registry_path: &Utf8PathBuf,
) -> Result<ViewServiceClient<BoxGrpcService>> {
    anyhow::ensure!(
        view_path.exists(),
        "missing local view database at {}; run prepare first",
        view_path
    );
    let registry_path = if registry_path.exists() {
        Some(registry_path.clone())
    } else {
        None
    };
    let view_server = ViewServer::load_or_initialize(
        Some(view_path.clone()),
        registry_path,
        &config.full_viewing_key,
        config.grpc_url.clone(),
    )
    .await
    .with_context(|| {
        format!(
            "failed to initialize local view service under {}",
            wallet_home
        )
    })?;
    let svc = ViewServiceServer::new(view_server);
    Ok(ViewServiceClient::new(box_grpc_svc::local(svc)))
}

async fn sync_view(view: &mut impl ViewClient) -> Result<()> {
    let mut status_stream = view.status_stream().await?;
    let initial_status = status_stream
        .next()
        .await
        .transpose()?
        .ok_or_else(|| anyhow::anyhow!("view service did not report sync status"))?;
    eprintln!(
        "syncing local view from height {} to {}",
        initial_status.full_sync_height, initial_status.latest_known_block_height
    );
    while let Some(status) = status_stream.next().await.transpose()? {
        if status.full_sync_height >= status.latest_known_block_height {
            break;
        }
    }
    Ok(())
}

fn resolve_asset_id(asset_cache: &asset::Cache, asset: &str) -> Result<Id> {
    if let Some(unit) = asset_cache.get_unit(asset) {
        return Ok(unit.id());
    }
    if let Ok(asset_id) = asset.parse::<Id>() {
        return Ok(asset_id);
    }
    bail!("failed to resolve asset '{asset}' as a known denom or asset id")
}

async fn select_distinct_source_notes(
    view: &mut (impl ViewClient + Send),
    asset_id: Id,
    source_start: usize,
    count: usize,
) -> Result<Vec<SpendableNoteRecord>> {
    let by_source = distinct_source_notes(view, asset_id).await?;

    anyhow::ensure!(
        by_source.len() >= source_start + count,
        "need {} distinct funded source indexes for this asset starting at offset {}, found {}",
        source_start + count,
        source_start,
        by_source.len()
    );

    Ok(by_source
        .into_values()
        .skip(source_start)
        .take(count)
        .collect())
}

async fn count_distinct_source_notes(
    view: &mut (impl ViewClient + Send),
    asset_id: Id,
) -> Result<usize> {
    Ok(distinct_source_notes(view, asset_id).await?.len())
}

async fn distinct_source_notes(
    view: &mut (impl ViewClient + Send),
    asset_id: Id,
) -> Result<BTreeMap<AddressIndex, SpendableNoteRecord>> {
    let mut notes = view
        .notes(NotesRequest {
            include_spent: false,
            asset_id: Some(asset_id.into()),
            ..Default::default()
        })
        .await
        .context("failed querying spendable notes")?;
    notes.sort_by_key(|record| (record.address_index, record.position));

    let mut by_source = BTreeMap::new();
    for record in notes {
        by_source.entry(record.address_index).or_insert(record);
    }
    Ok(by_source)
}

async fn build_plan(
    view: &mut (impl ViewClient + Send),
    gas_prices: &GasPrices,
    fee_tier: FeeTier,
    note_record: &SpendableNoteRecord,
    to_address: Address,
) -> Result<TransactionPlan> {
    let mut planner = Planner::new(OsRng);
    planner.set_gas_prices(gas_prices.clone());
    planner.set_fee_tier(fee_tier);
    planner.spend(note_record.note.clone(), note_record.position);
    planner.output(
        Value {
            amount: note_record.note.amount(),
            asset_id: note_record.note.asset_id(),
        },
        to_address,
    );
    planner
        .plan(view, note_record.address_index)
        .await
        .context("planner failed")
}

async fn authorize_plan(
    custody: &mut impl CustodyClient,
    plan: &TransactionPlan,
) -> Result<AuthorizationData> {
    let response = custody
        .authorize(AuthorizeRequest {
            plan: plan.clone(),
            pre_authorizations: Vec::new(),
        })
        .await
        .context("custody authorize failed")?;
    response
        .data
        .ok_or_else(|| anyhow::anyhow!("authorize response was missing authorization data"))?
        .try_into()
        .context("failed to decode authorization data")
}

async fn build_transactions_parallel(
    fvk: FullViewingKey,
    prepared: Vec<PreparedTx>,
    concurrency: usize,
) -> Result<Vec<Transaction>> {
    let permits = Arc::new(Semaphore::new(concurrency));
    let fvk = Arc::new(fvk);
    let total = prepared.len();
    let mut tasks = JoinSet::new();
    for prepared_tx in prepared {
        let permits = permits.clone();
        let fvk = fvk.clone();
        tasks.spawn(async move {
            let _permit = permits.acquire_owned().await?;
            let tx = prepared_tx
                .plan
                .build_concurrent(&fvk, &prepared_tx.witness_data, &prepared_tx.auth_data)
                .await
                .map_err(anyhow::Error::from)?;
            Ok::<_, anyhow::Error>((prepared_tx.ordinal, tx))
        });
    }

    let mut out = Vec::with_capacity(total);
    while let Some(result) = tasks.join_next().await {
        let (ordinal, tx) = result.context("join error in parallel build")??;
        out.push((ordinal, tx));
        if out.len() % 100 == 0 || out.len() == total {
            eprintln!("built {}/{}", out.len(), total);
        }
    }
    out.sort_by_key(|(ordinal, _)| *ordinal);
    Ok(out.into_iter().map(|(_, tx)| tx).collect())
}

fn unix_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn normalize_unregulated_target_timestamps(plan: &mut TransactionPlan) {
    for action in &mut plan.actions {
        match action {
            ActionPlan::Spend(spend) if !spend.is_regulated => {
                spend.target_timestamp = 0;
            }
            ActionPlan::Output(output) if !output.is_regulated => {
                output.target_timestamp = 0;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_unregulated_target_timestamps;
    use penumbra_sdk_asset::Value;
    use penumbra_sdk_keys::Address;
    use penumbra_sdk_shielded_pool::{Note, OutputPlan, Rseed, SpendPlan};
    use penumbra_sdk_transaction::{plan::ActionPlan, TransactionParameters, TransactionPlan};
    use rand_core::OsRng;

    #[test]
    fn normalize_unregulated_target_timestamps_only_zeroes_unregulated_actions() {
        let mut rng = OsRng;
        let sender = Address::dummy(&mut rng);
        let recipient = Address::dummy(&mut rng);
        let value = Value {
            amount: 1u64.into(),
            asset_id: penumbra_sdk_asset::asset::Id(decaf377::Fq::from(7u64)),
        };
        let note =
            Note::from_parts(sender.clone(), value, Rseed::generate(&mut rng)).expect("valid note");

        let mut unregulated_spend = SpendPlan::new(&mut rng, note.clone(), 0u64.into());
        unregulated_spend.is_regulated = false;
        unregulated_spend.target_timestamp = 123;

        let mut regulated_spend = SpendPlan::new(&mut rng, note, 1u64.into());
        regulated_spend.is_regulated = true;
        regulated_spend.target_timestamp = 456;

        let mut unregulated_output = OutputPlan::new(&mut rng, value, recipient.clone());
        unregulated_output.is_regulated = false;
        unregulated_output.target_timestamp = 789;

        let mut regulated_output = OutputPlan::new(&mut rng, value, recipient);
        regulated_output.is_regulated = true;
        regulated_output.target_timestamp = 987;

        let mut plan = TransactionPlan {
            transaction_parameters: TransactionParameters::default(),
            actions: vec![
                ActionPlan::Spend(unregulated_spend),
                ActionPlan::Spend(regulated_spend),
                ActionPlan::Output(unregulated_output),
                ActionPlan::Output(regulated_output),
            ],
            detection_data: None,
            memo: None,
        };

        normalize_unregulated_target_timestamps(&mut plan);

        let mut actions = plan.actions.iter();
        match actions.next().expect("unregulated spend") {
            ActionPlan::Spend(spend) => assert_eq!(spend.target_timestamp, 0),
            _ => panic!("expected spend"),
        }
        match actions.next().expect("regulated spend") {
            ActionPlan::Spend(spend) => assert_eq!(spend.target_timestamp, 456),
            _ => panic!("expected spend"),
        }
        match actions.next().expect("unregulated output") {
            ActionPlan::Output(output) => assert_eq!(output.target_timestamp, 0),
            _ => panic!("expected output"),
        }
        match actions.next().expect("regulated output") {
            ActionPlan::Output(output) => assert_eq!(output.target_timestamp, 987),
            _ => panic!("expected output"),
        }
    }
}
