use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use cnidarium::Storage;
use penumbra_sdk_app::app::{App, CheckTxSharedContext};
use penumbra_sdk_app::stateless_cache::{StatelessCache, TxArtifact};
use penumbra_sdk_app::block_tx_indexing::BlockTxIndexingMode;
use penumbra_sdk_app::SUBSTORE_PREFIXES;
use penumbra_sdk_bench::tps::corpus;
use penumbra_sdk_proof_aggregation::{
    prepare_verify_inputs, verify_family_aggregate_profiled, DevSrs, ProofFamilyId,
};
use penumbra_sdk_proof_params::{OUTPUT_PROOF_VERIFICATION_KEY, SPEND_PROOF_VERIFICATION_KEY};
use penumbra_sdk_proto::DomainType;
use penumbra_sdk_transaction::Transaction;
use sha2::Digest as _;

#[derive(Debug, Parser)]
#[clap(name = "corpus_bundle_verify")]
#[clap(about = "Build and verify a segmented aggregate bundle from a tx corpus")]
struct Cli {
    #[clap(long)]
    corpus: PathBuf,
    #[clap(long, default_value = "direct-extracted")]
    mode: String,
    #[clap(long)]
    rocksdb_home: Option<PathBuf>,
    #[clap(long, default_value_t = 32)]
    segment_tx_count: usize,
    #[clap(long, default_value_t = 0)]
    tx_limit: usize,
    #[clap(long, default_value_t = 1)]
    rayon_threads_per_batch: usize,
    #[clap(long, default_value_t = 0)]
    warmup_blocks: usize,
}

pub async fn run_from<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::parse_from(args);
    penumbra_sdk_proof_aggregation::set_rayon_threads_per_batch_for_bench(
        cli.rayon_threads_per_batch,
    );

    let corpus = corpus::load_corpus(&cli.corpus)
        .with_context(|| format!("loading corpus {}", cli.corpus.display()))?;
    let limit = if cli.tx_limit == 0 {
        corpus.entries.len()
    } else {
        cli.tx_limit.min(corpus.entries.len())
    };
    let txs = corpus
        .entries
        .iter()
        .take(limit)
        .map(|entry| {
            Transaction::decode(entry.tx_bytes.as_slice())
                .with_context(|| format!("decoding tx ordinal {}", entry.ordinal))
                .map(Arc::new)
        })
        .collect::<Result<Vec<_>>>()?;

    let mode = parse_mode(&cli.mode)?;
    let (artifacts, artifact_profile) = match mode {
        ArtifactSourceMode::DirectExtracted => {
            App::build_tx_artifacts_extracted_profiled_public("corpus_bundle_verify", &txs).await?
        }
        ArtifactSourceMode::StrictMempool | ArtifactSourceMode::OptimisticBuilder => {
            let rocksdb_home = cli.rocksdb_home.unwrap_or_else(default_rocksdb_home);
            let storage = Storage::load(rocksdb_home.clone(), SUBSTORE_PREFIXES.to_vec())
                .await
                .with_context(|| format!("loading RocksDB from {}", rocksdb_home.display()))?;
            build_artifacts_via_admission(mode, &corpus, limit, storage.latest_snapshot()).await?
        }
    };
    let segment_tx_counts = plan_segment_tx_counts(&artifacts, cli.segment_tx_count);

    for w in 0..cli.warmup_blocks {
        let (warmup_bundle, warmup_segment_tx_counts, _) =
            App::build_exact_segmented_aggregate_bundle_for_artifacts_profiled_public(
                &artifacts,
                &segment_tx_counts,
            )
            .await?;
        App::verify_aggregate_bundle_for_artifacts_public(
            &artifacts,
            &warmup_bundle,
            Some(&warmup_segment_tx_counts),
        )
        .await?;
        eprintln!("warmup block {} done", w + 1);
    }

    let build_start = std::time::Instant::now();
    let (bundle, actual_segment_tx_counts, build_profile) =
        App::build_exact_segmented_aggregate_bundle_for_artifacts_profiled_public(
            &artifacts,
            &segment_tx_counts,
        )
        .await?;
    let build_wall_ms = build_start.elapsed().as_secs_f64() * 1000.0;

    println!("corpus={}", cli.corpus.display());
    println!("mode={}", cli.mode);
    println!("tx_count={}", txs.len());
    println!("segment_tx_count={}", cli.segment_tx_count);
    println!("planned_segment_tx_counts={:?}", segment_tx_counts);
    println!("actual_segment_tx_counts={:?}", actual_segment_tx_counts);
    println!("bundle_family_count={}", bundle.families.len());
    println!(
        "artifact_profile precheck_ms={:.3} action_extract_ms={:.3} extract_public_ms={:.3} to_batch_item_ms={:.3} batch_verify_ms={:.3}",
        artifact_profile.precheck_ms,
        artifact_profile.action_extract_ms,
        artifact_profile.action_extract_public_ms,
        artifact_profile.action_to_batch_item_ms,
        artifact_profile.batch_verify_ms
    );
    println!("build_wall_ms={build_wall_ms:.3}");
    println!(
        "aggregate_build_profile merge_items_ms={:.3} setup_ms={:.3} padding_ms={:.3} collect_proofs_ms={:.3} backend_core_ms={:.3} bundle_tx_build_ms={:.3}",
        build_profile.merge_items_ms,
        build_profile.setup_ms,
        build_profile.padding_ms,
        build_profile.collect_proofs_ms,
        build_profile.backend_core_ms,
        build_profile.bundle_tx_build_ms
    );

    let verify_start = std::time::Instant::now();
    let app_verify = App::verify_aggregate_bundle_for_artifacts_public(
        &artifacts,
        &bundle,
        Some(&actual_segment_tx_counts),
    )
    .await;
    let verify_wall_ms = verify_start.elapsed().as_secs_f64() * 1000.0;
    let total_wall_ms = build_wall_ms + verify_wall_ms;
    println!("verify_wall_ms={verify_wall_ms:.3}");
    println!("total_wall_ms={total_wall_ms:.3}");
    println!("app_verify_ok={}", app_verify.is_ok());
    if let Err(err) = &app_verify {
        println!("app_verify_error={err:#}");
    }

    let srs = DevSrs::default();
    let expected_segments = expected_segments(&artifacts, &actual_segment_tx_counts);
    anyhow::ensure!(
        expected_segments.len() == bundle.families.len(),
        "bundle family count mismatch in manual verifier: expected {}, got {}",
        expected_segments.len(),
        bundle.families.len()
    );

    for (idx, ((family_id, items), aggregate)) in expected_segments
        .iter()
        .zip(bundle.families.iter())
        .enumerate()
    {
        let prepared = prepare_verify_inputs(items, srs.max_padded_count as usize)?;
        println!(
            "family_segment[{idx}] family={:?} real_count={} padded_count={} bundle_real_count={} bundle_padded_count={}",
            family_id,
            items.len(),
            prepared.padded_count,
            aggregate.real_count,
            aggregate.padded_count
        );
        let verify_result = verify_family_aggregate_profiled(
            *family_id,
            family_pvk(*family_id),
            &aggregate.aggregate_proof,
            &prepared.padded_public_inputs,
            &srs,
        );
        match verify_result {
            Ok(profile) => {
                println!(
                    "family_segment[{idx}] verify_ok=true accepted={} total_ms={:.3} ppe_ms={:.3} challenge_ms={:.3} public_input_fold_ms={:.3}",
                    profile.accepted,
                    profile.total_ms,
                    profile.ppe_ms,
                    profile.challenge_ms,
                    profile.public_input_fold_ms
                );
            }
            Err(err) => {
                println!("family_segment[{idx}] verify_ok=false error={err:#}");
            }
        }
    }

    app_verify?;
    Ok(())
}

#[derive(Clone, Copy)]
enum ArtifactSourceMode {
    DirectExtracted,
    StrictMempool,
    OptimisticBuilder,
}

fn parse_mode(s: &str) -> Result<ArtifactSourceMode> {
    match s {
        "direct-extracted" => Ok(ArtifactSourceMode::DirectExtracted),
        "strict-mempool" => Ok(ArtifactSourceMode::StrictMempool),
        "optimistic-builder" => Ok(ArtifactSourceMode::OptimisticBuilder),
        other => anyhow::bail!(
            "unsupported --mode value {other}; use direct-extracted, strict-mempool, or optimistic-builder"
        ),
    }
}

fn expected_segments(
    artifacts: &[Arc<TxArtifact>],
    segment_tx_counts: &[usize],
) -> Vec<(
    ProofFamilyId,
    Vec<penumbra_sdk_proof_params::batch::BatchItem>,
)> {
    let mut expected_segments = Vec::new();
    let mut start = 0usize;

    for &segment_tx_count in segment_tx_counts {
        let end = start + segment_tx_count;
        let segment = &artifacts[start..end];
        let mut spend_items = Vec::new();
        let mut output_items = Vec::new();
        for artifact in segment {
            if let Some(items) = artifact.proof_items.get(&ProofFamilyId::Spend) {
                spend_items.extend(items.iter().cloned());
            }
            if let Some(items) = artifact.proof_items.get(&ProofFamilyId::Output) {
                output_items.extend(items.iter().cloned());
            }
        }
        if !spend_items.is_empty() {
            expected_segments.push((ProofFamilyId::Spend, spend_items));
        }
        if !output_items.is_empty() {
            expected_segments.push((ProofFamilyId::Output, output_items));
        }
        start = end;
    }

    expected_segments
}

fn family_pvk(
    family: ProofFamilyId,
) -> &'static ark_groth16::PreparedVerifyingKey<decaf377::Bls12_377> {
    match family {
        ProofFamilyId::Spend => &SPEND_PROOF_VERIFICATION_KEY,
        ProofFamilyId::Output => &OUTPUT_PROOF_VERIFICATION_KEY,
        other => panic!("unsupported family in corpus_bundle_verify: {other:?}"),
    }
}

fn plan_segment_tx_counts(
    artifacts: &[Arc<TxArtifact>],
    preferred_segment_tx_count: usize,
) -> Vec<usize> {
    let segment_tx_count = preferred_segment_tx_count.max(1);
    artifacts
        .chunks(segment_tx_count)
        .map(|segment| segment.len())
        .collect()
}

async fn build_artifacts_via_admission(
    mode: ArtifactSourceMode,
    corpus: &corpus::Corpus,
    limit: usize,
    snapshot: cnidarium::Snapshot,
) -> Result<(
    Vec<Arc<TxArtifact>>,
    penumbra_sdk_app::app::ArtifactBuildBreakdown,
)> {
    let stateless_cache = Arc::new(StatelessCache::new());
    let shared_context = Arc::new(CheckTxSharedContext::load(&snapshot).await?);
    let mut artifacts = Vec::with_capacity(limit);
    let mut profile_sum = penumbra_sdk_app::app::ArtifactBuildBreakdown::default();

    for entry in corpus.entries.iter().take(limit) {
        let tx_bytes = Arc::new(entry.tx_bytes.clone());
        let tx_hash: [u8; 32] = sha2::Sha256::digest(tx_bytes.as_slice()).into();
        let mut app = App::new(snapshot.clone());
        app.set_block_tx_indexing_mode(BlockTxIndexingMode::NoIndex);
        app.set_checktx_shared_context(shared_context.clone());

        let profile = match mode {
            ArtifactSourceMode::StrictMempool => {
                let (_events, profile) = app
                    .deliver_tx_bytes_v2_profiled(
                        tx_bytes.as_slice(),
                        Some(stateless_cache.as_ref()),
                    )
                    .await?;
                profile
            }
            ArtifactSourceMode::OptimisticBuilder => {
                let (_events, profile) = app
                    .deliver_tx_bytes_v2_extracted_profiled_for_bench(
                        tx_bytes.as_slice(),
                        stateless_cache.as_ref(),
                    )
                    .await?;
                profile
            }
            ArtifactSourceMode::DirectExtracted => unreachable!(),
        };

        let artifact = stateless_cache
            .get(&tx_hash)
            .and_then(|entry| entry.artifact())
            .with_context(|| {
                format!(
                    "missing cached artifact after successful admission for {}",
                    hex::encode(tx_hash)
                )
            })?;
        artifacts.push(artifact);
        profile_sum.precheck_ms += profile.stateless_artifact_precheck_ms;
        profile_sum.action_extract_ms += profile.stateless_artifact_action_extract_ms;
        profile_sum.action_auth_sig_ms += profile.stateless_artifact_action_auth_sig_ms;
        profile_sum.action_extract_public_ms += profile.stateless_artifact_action_extract_public_ms;
        profile_sum.action_to_batch_item_ms += profile.stateless_artifact_action_to_batch_item_ms;
        profile_sum.batch_verify_ms += profile.stateless_artifact_batch_verify_ms;
    }

    Ok((artifacts, profile_sum))
}

fn default_rocksdb_home() -> PathBuf {
    let mut home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.push(".penumbra/network_data/node0/pd/rocksdb");
    home
}
