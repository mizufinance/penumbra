use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use bytes::Bytes;
use clap::Parser;
use penumbra_sdk_app::app::{
    candidate_digest_from_hashes, App, CandidateEnvelope, ExecutionBlockProfile,
    PrepareProposalProfile, ProcessProposalProfile, ProposalArtifactSidecar,
};
use penumbra_sdk_app::block_tx_indexing::BlockTxIndexingMode;
use penumbra_sdk_bench_support::proof_txs::{
    build_proof_tx_pool, build_proof_tx_workload, default_pool_dir, load_proof_tx_pool,
    save_proof_tx_pool, setup_proof_storage, ProofTxPool, ProofTxPoolMetadata,
};
use serde::Serialize;
use sha2::Digest as _;
use tendermint::v0_37::abci::{request, response};
use tendermint::{account, block, Hash, Time};

#[derive(Debug, Parser)]
#[clap(
    name = "ibc-vs-transfer",
    about = "Compare validator-side proof and execution costs for transfer and IBC scenarios"
)]
struct Args {
    #[clap(long, default_value_t = 1_000)]
    tx_count: usize,

    #[clap(long, default_value_t = 3)]
    runs: usize,

    #[clap(long, default_value = "target/bench/ibc-vs-transfer.json")]
    out: PathBuf,

    #[clap(long)]
    corpus_dir: Option<PathBuf>,

    #[clap(long)]
    rebuild_corpus: bool,

    #[clap(long)]
    segment_tx_count: Option<usize>,

    #[clap(
        long,
        use_value_delimiter = true,
        value_delimiter = ',',
        default_value = "regulated_inner_transfer"
    )]
    scenarios: Vec<String>,
}

#[derive(Debug, Serialize)]
struct BenchmarkReport {
    tx_count: usize,
    runs_requested: usize,
    generated_at_unix: u64,
    notes: Vec<String>,
    transfer_corpus: CorpusReport,
    scenarios: Vec<ScenarioReport>,
}

#[derive(Debug, Serialize)]
struct CorpusReport {
    path: PathBuf,
    source: String,
    elapsed_ms: f64,
    rebuild_reason: Option<String>,
    metadata: ProofTxPoolMetadata,
}

#[derive(Debug, Serialize)]
struct ScenarioReport {
    name: String,
    status: String,
    tx_count: usize,
    runs: Vec<RunReport>,
    summary: Option<ScenarioSummary>,
}

#[derive(Debug, Serialize)]
struct RunReport {
    run_index: usize,
    prepare_wall_ms: f64,
    process_wall_ms: f64,
    execute_wall_ms: f64,
    total_wall_ms: f64,
    tps: f64,
    ms_per_tx: f64,
    projected_5000_tx_ms: f64,
    proof_verification_share: f64,
    execution_and_commit_share: f64,
    prepare_profile: PrepareProposalProfile,
    process_profile: ProcessProposalProfile,
    execution_profile: ExecutionBlockProfile,
}

#[derive(Debug, Serialize)]
struct ScenarioSummary {
    mean_total_wall_ms: f64,
    mean_tps: f64,
    mean_ms_per_tx: f64,
    p50_ms_per_tx: f64,
    p95_ms_per_tx: f64,
    p99_ms_per_tx: f64,
    mean_proof_verification_share: f64,
    mean_execution_and_commit_share: f64,
    projected_5000_tx_ms_from_mean: f64,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let args = Args::parse();
    anyhow::ensure!(args.tx_count > 0, "--tx-count must be positive");
    anyhow::ensure!(args.runs > 0, "--runs must be positive");
    if let Some(segment_tx_count) = args.segment_tx_count {
        anyhow::ensure!(segment_tx_count > 0, "--segment-tx-count must be positive");
    }

    let pool_dir = args
        .corpus_dir
        .clone()
        .unwrap_or_else(|| default_pool_dir(args.tx_count));
    let (pool, corpus_report) =
        load_or_build_transfer_corpus(&pool_dir, args.tx_count, args.rebuild_corpus).await?;
    let transfer_txs = build_proof_tx_workload(args.tx_count, &pool);

    let mut scenarios = Vec::new();
    for scenario in &args.scenarios {
        let report = match scenario.as_str() {
            "regulated_inner_transfer" | "inner_transfer" => {
                run_inner_transfer(&args, &transfer_txs)
                    .await
                    .context("running inner_transfer benchmark")?
            }
            "inbound_ics20_recv_preupdated"
            | "inbound_ics20_full_smoke"
            | "outbound_plain_ics20"
            | "outbound_shielded_ics20" => {
                anyhow::bail!(
                    "scenario {scenario} is not supported by this binary because it cannot build \
                     a real reusable IBC corpus from penumbra-sdk-bench. Run the ignored \
                     app-test benchmark ibc_vs_transfer_benchmark for real IBC inbound/outbound \
                     corpora."
                );
            }
            other => anyhow::bail!("unknown scenario name: {other}"),
        };
        scenarios.push(report);
    }

    let report = BenchmarkReport {
        tx_count: args.tx_count,
        runs_requested: args.runs,
        generated_at_unix: unix_ts(),
        notes: vec![
            "transaction/proof generation is excluded from scenario timing".to_string(),
            "regulated_inner_transfer uses current TransferProof proving and verification keys via the corpus cache key".to_string(),
            "unsupported IBC scenarios fail instead of emitting skipped or synthetic rows".to_string(),
        ],
        transfer_corpus: corpus_report,
        scenarios,
    };

    if let Some(parent) = args.out.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating output directory {}", parent.display()))?;
    }
    std::fs::write(&args.out, serde_json::to_vec_pretty(&report)?)
        .with_context(|| format!("writing {}", args.out.display()))?;

    print_summary(&args.out, &report);
    Ok(())
}

async fn load_or_build_transfer_corpus(
    pool_dir: &PathBuf,
    tx_count: usize,
    rebuild_corpus: bool,
) -> Result<(ProofTxPool, CorpusReport)> {
    let start = Instant::now();
    let rebuild_reason;

    if !rebuild_corpus {
        match load_proof_tx_pool(pool_dir) {
            Ok((pool, metadata)) => {
                return Ok((
                    pool,
                    CorpusReport {
                        path: pool_dir.clone(),
                        source: "loaded".to_string(),
                        elapsed_ms: elapsed_ms(start),
                        rebuild_reason: None,
                        metadata,
                    },
                ));
            }
            Err(error) => {
                rebuild_reason = Some(error.to_string());
            }
        }
    } else {
        rebuild_reason = Some("--rebuild-corpus was set".to_string());
    }

    let (storage, _node, client) = setup_proof_storage(tx_count)
        .await
        .context("setting up transfer corpus storage")?;
    let pool = build_proof_tx_pool(client, &storage, tx_count)
        .await
        .context("building transfer proof tx corpus")?;
    let metadata =
        save_proof_tx_pool(pool_dir, &pool).context("saving transfer proof tx corpus")?;

    Ok((
        pool,
        CorpusReport {
            path: pool_dir.clone(),
            source: "rebuilt".to_string(),
            elapsed_ms: elapsed_ms(start),
            rebuild_reason,
            metadata,
        },
    ))
}

async fn run_inner_transfer(args: &Args, txs: &[Vec<u8>]) -> Result<ScenarioReport> {
    let mut runs = Vec::with_capacity(args.runs);

    for run_index in 0..args.runs {
        let (storage, _node, _client) = setup_proof_storage(args.tx_count)
            .await
            .with_context(|| format!("setting up fresh storage for run {run_index}"))?;

        let mut proposer = App::new(storage.latest_snapshot());
        proposer.set_block_tx_indexing_mode(BlockTxIndexingMode::DeferredBatch);
        proposer.set_proposal_segment_tx_count(args.segment_tx_count);
        let prepare_request = prepare_request(txs);
        let prepare_start = Instant::now();
        let (prepared, prepare_profile, sidecar) = proposer
            .prepare_proposal_v2_profiled(prepare_request, None, true)
            .await;
        let prepare_wall_ms = elapsed_ms(prepare_start);
        let sidecar = sidecar.context("prepare proposal did not return an artifact sidecar")?;
        let envelope = envelope_from_prepare_response(txs, &prepared, &sidecar)?;

        let process_request = process_request_from_envelope(&envelope);
        let mut validator = App::new(storage.latest_snapshot());
        let process_start = Instant::now();
        let (process_verdict, process_profile) = validator
            .process_proposal_v2_profiled(process_request, None, Some(&sidecar), true)
            .await;
        let process_wall_ms = elapsed_ms(process_start);
        anyhow::ensure!(
            matches!(process_verdict, response::ProcessProposal::Accept),
            "process proposal rejected run {run_index}: {process_verdict:?}"
        );

        let execution_envelope = execution_only_envelope(&envelope);
        let mut executor = App::new(storage.latest_snapshot());
        executor.set_block_tx_indexing_mode(BlockTxIndexingMode::DeferredBatch);
        let execute_start = Instant::now();
        let execution_profile = executor
            .execute_validated_candidate_envelope_profiled(
                &execution_envelope,
                storage.as_ref().clone(),
            )
            .await
            .with_context(|| format!("executing run {run_index}"))?;
        let execute_wall_ms = elapsed_ms(execute_start);

        let total_wall_ms = prepare_wall_ms + process_wall_ms + execute_wall_ms;
        let tx_count = args.tx_count as f64;
        let tps = tx_count / (total_wall_ms / 1000.0);
        let ms_per_tx = total_wall_ms / tx_count;
        let proof_verification_ms =
            prepare_profile.artifact_fill_batch_verify_ms + process_profile.aggregate_verify_ms;
        let execution_and_commit_ms = execution_profile.deliver_txs_wall_ms
            + execution_profile.end_block_ms
            + execution_profile.commit_ms;

        runs.push(RunReport {
            run_index,
            prepare_wall_ms,
            process_wall_ms,
            execute_wall_ms,
            total_wall_ms,
            tps,
            ms_per_tx,
            projected_5000_tx_ms: ms_per_tx * 5_000.0,
            proof_verification_share: share(proof_verification_ms, total_wall_ms),
            execution_and_commit_share: share(execution_and_commit_ms, total_wall_ms),
            prepare_profile,
            process_profile,
            execution_profile,
        });
    }

    Ok(ScenarioReport {
        name: "regulated_inner_transfer".to_string(),
        status: "completed".to_string(),
        tx_count: args.tx_count,
        summary: Some(summarize(&runs)),
        runs,
    })
}

fn prepare_request(txs: &[Vec<u8>]) -> request::PrepareProposal {
    request::PrepareProposal {
        txs: txs.iter().cloned().map(Bytes::from).collect(),
        max_tx_bytes: i64::MAX / 4,
        local_last_commit: None,
        misbehavior: Vec::new(),
        height: block::Height::from(2u32),
        time: Time::unix_epoch(),
        next_validators_hash: Hash::None,
        proposer_address: account::Id::new([0u8; 20]),
    }
}

fn process_request_from_envelope(envelope: &CandidateEnvelope) -> request::ProcessProposal {
    let mut txs = envelope
        .txs
        .iter()
        .cloned()
        .map(Bytes::from)
        .collect::<Vec<_>>();
    if let Some(bundle_tx_bytes) = &envelope.aggregate_bundle_tx_bytes {
        txs.push(Bytes::from(bundle_tx_bytes.clone()));
    }

    request::ProcessProposal {
        txs,
        proposed_last_commit: None,
        misbehavior: Vec::new(),
        hash: Hash::None,
        height: block::Height::from(2u32),
        time: Time::unix_epoch(),
        next_validators_hash: Hash::None,
        proposer_address: account::Id::new([0u8; 20]),
    }
}

fn envelope_from_prepare_response(
    input_txs: &[Vec<u8>],
    prepared: &response::PrepareProposal,
    sidecar: &ProposalArtifactSidecar,
) -> Result<CandidateEnvelope> {
    anyhow::ensure!(
        prepared.txs.len() == input_txs.len() + 1,
        "prepared proposal must contain {} user txs plus one aggregate bundle tx, got {} txs",
        input_txs.len(),
        prepared.txs.len()
    );
    for (index, expected) in input_txs.iter().enumerate() {
        anyhow::ensure!(
            prepared.txs[index].as_ref() == expected.as_slice(),
            "prepare proposal changed or skipped user tx ordinal {index}"
        );
    }

    let aggregate_bundle_tx_bytes = prepared
        .txs
        .last()
        .map(|tx| tx.to_vec())
        .context("missing aggregate bundle tx")?;
    let tx_hashes = input_txs
        .iter()
        .map(|tx_bytes| sha2::Sha256::digest(tx_bytes).into())
        .collect::<Vec<[u8; 32]>>();

    Ok(CandidateEnvelope {
        txs: input_txs.to_vec(),
        tx_hashes: tx_hashes.clone(),
        aggregate_bundle_tx_bytes: Some(aggregate_bundle_tx_bytes),
        sidecar: sidecar.to_record(),
        segment_tx_counts: sidecar.segment_tx_counts.clone(),
        block_tx_count: input_txs.len(),
        total_payload_bytes: input_txs.iter().map(Vec::len).sum(),
        candidate_digest: candidate_digest_from_hashes(&tx_hashes),
        source_builder_label: "ibc_vs_transfer_prepare".to_string(),
    })
}

fn execution_only_envelope(envelope: &CandidateEnvelope) -> CandidateEnvelope {
    let mut execution_only = envelope.clone();
    execution_only.aggregate_bundle_tx_bytes = None;
    execution_only.segment_tx_counts.clear();
    execution_only.sidecar =
        ProposalArtifactSidecar::from_record(envelope.sidecar.clone()).to_record();
    execution_only
}

fn summarize(runs: &[RunReport]) -> ScenarioSummary {
    let mean_total_wall_ms = mean(runs.iter().map(|run| run.total_wall_ms));
    let mean_ms_per_tx = mean(runs.iter().map(|run| run.ms_per_tx));
    let mut ms_per_tx = runs.iter().map(|run| run.ms_per_tx).collect::<Vec<_>>();

    ScenarioSummary {
        mean_total_wall_ms,
        mean_tps: mean(runs.iter().map(|run| run.tps)),
        mean_ms_per_tx,
        p50_ms_per_tx: percentile(&mut ms_per_tx, 0.50),
        p95_ms_per_tx: percentile(&mut ms_per_tx, 0.95),
        p99_ms_per_tx: percentile(&mut ms_per_tx, 0.99),
        mean_proof_verification_share: mean(runs.iter().map(|run| run.proof_verification_share)),
        mean_execution_and_commit_share: mean(
            runs.iter().map(|run| run.execution_and_commit_share),
        ),
        projected_5000_tx_ms_from_mean: mean_ms_per_tx * 5_000.0,
    }
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let mut count = 0usize;
    let mut sum = 0.0;
    for value in values {
        count += 1;
        sum += value;
    }
    if count == 0 {
        0.0
    } else {
        sum / count as f64
    }
}

fn percentile(values: &mut [f64], percentile: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    let index = ((values.len() - 1) as f64 * percentile).ceil() as usize;
    values[index]
}

fn share(part_ms: f64, total_ms: f64) -> f64 {
    if total_ms <= 0.0 {
        0.0
    } else {
        part_ms / total_ms
    }
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn unix_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn print_summary(path: &PathBuf, report: &BenchmarkReport) {
    println!("wrote {}", path.display());
    for scenario in &report.scenarios {
        match &scenario.summary {
            Some(summary) => {
                println!(
                    "{}: {:.2} TPS, {:.3} ms/tx, projected 5k {:.1} ms",
                    scenario.name,
                    summary.mean_tps,
                    summary.mean_ms_per_tx,
                    summary.projected_5000_tx_ms_from_mean,
                );
            }
            None => unreachable!("completed scenario must include a summary"),
        }
    }
}
