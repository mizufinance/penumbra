use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use penumbra_sdk_bench::tps::corpus;
use penumbra_sdk_proof_aggregation::{
    aggregate_family, aggregate_family_profiled, pad_items_to_power_of_two, prepare_verify_inputs,
    verify_family_aggregate, verify_family_aggregate_profiled, DevSrs, ProofFamilyId,
};
use penumbra_sdk_proof_params::{OUTPUT_PROOF_VERIFICATION_KEY, SPEND_PROOF_VERIFICATION_KEY};
use penumbra_sdk_proto::DomainType;
use penumbra_sdk_shielded_pool::component::{output_extract_public, spend_extract_public};
use penumbra_sdk_transaction::{Action, Transaction};

#[derive(Debug, Parser)]
#[clap(name = "corpus_aggregate_verify")]
#[clap(about = "Aggregate and verify one proof family from a tx corpus")]
struct Cli {
    #[clap(long)]
    corpus: PathBuf,
    #[clap(long)]
    family: String,
    #[clap(long, default_value_t = 0)]
    count: usize,
    #[clap(long, default_value_t = 0)]
    start: usize,
    #[clap(long, default_value_t = 0)]
    tx_limit: usize,
    #[clap(long, default_value_t = 1)]
    rayon_threads_per_batch: usize,
    #[clap(long, default_value_t = false)]
    profiled: bool,
}

#[derive(Clone)]
struct SelectedItem {
    tx_ordinal: usize,
    tx_hash: String,
    action_index: usize,
    public_inputs_hex: Vec<String>,
    item: penumbra_sdk_proof_params::batch::BatchItem,
}

pub fn run_from<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::parse_from(args);
    penumbra_sdk_proof_aggregation::set_rayon_threads_per_batch_for_bench(
        cli.rayon_threads_per_batch,
    );

    let family = parse_family(&cli.family)?;
    let corpus = corpus::load_corpus(&cli.corpus)
        .with_context(|| format!("loading corpus {}", cli.corpus.display()))?;
    let extracted = extract_family_items(&corpus, family, cli.tx_limit)?;
    let available = extracted.len().saturating_sub(cli.start);
    let count = if cli.count == 0 {
        available
    } else {
        cli.count.min(available)
    };
    anyhow::ensure!(
        count > 0,
        "no items selected for family {:?} start={} count={}",
        family,
        cli.start,
        cli.count
    );

    let selected = extracted[cli.start..cli.start + count].to_vec();
    let raw_items = selected
        .iter()
        .cloned()
        .map(|item| item.item)
        .collect::<Vec<_>>();
    let srs = DevSrs::default();
    let padded_items = pad_items_to_power_of_two(&raw_items, srs.max_padded_count as usize)?;
    let prepared = prepare_verify_inputs(&raw_items, srs.max_padded_count as usize)?;

    println!("corpus={}", cli.corpus.display());
    println!("family={:?}", family);
    println!("start={}", cli.start);
    println!("selected_count={}", count);
    println!("padded_count={}", prepared.padded_count);
    println!("rayon_threads_per_batch={}", cli.rayon_threads_per_batch);
    for (idx, item) in selected.iter().enumerate().take(16) {
        println!(
            "item[{idx}] tx_ordinal={} tx_hash={} action_index={} public_inputs={}",
            item.tx_ordinal,
            item.tx_hash,
            item.action_index,
            item.public_inputs_hex.join(",")
        );
    }

    let pvk = family_pvk(family);
    if cli.profiled {
        let (aggregate, build_profile) =
            aggregate_family_profiled(family, pvk, &padded_items, &srs)?;
        println!("aggregate_bytes_len={}", aggregate.len());
        println!(
            "aggregate_build_profile total_ms={:.3} collect_proofs_ms={:.3} backend_aggregate_ms={:.3} serialize_ms={:.3}",
            build_profile.total_ms,
            build_profile.collect_proofs_ms,
            build_profile.backend_aggregate_ms,
            build_profile.serialize_ms
        );
        let verify_profile = verify_family_aggregate_profiled(
            family,
            pvk,
            &aggregate,
            &prepared.padded_public_inputs,
            &srs,
        )?;
        println!(
            "aggregate_verify_profile accepted={} total_ms={:.3} deserialize_ms={:.3} challenge_ms={:.3} tipa_ab_ms={:.3} tipa_c_ms={:.3} public_input_fold_ms={:.3} ppe_ms={:.3}",
            verify_profile.accepted,
            verify_profile.total_ms,
            verify_profile.deserialize_ms,
            verify_profile.challenge_ms,
            verify_profile.tipa_ab_ms,
            verify_profile.tipa_c_ms,
            verify_profile.public_input_fold_ms,
            verify_profile.ppe_ms
        );
    } else {
        let aggregate = aggregate_family(family, pvk, &padded_items, &srs)?;
        println!("aggregate_bytes_len={}", aggregate.len());
        verify_family_aggregate(
            family,
            pvk,
            &aggregate,
            &prepared.padded_public_inputs,
            &srs,
        )?;
        println!("aggregate_verify_ok=true");
    }

    Ok(())
}

fn parse_family(s: &str) -> Result<ProofFamilyId> {
    match s {
        "spend" => Ok(ProofFamilyId::Spend),
        "output" => Ok(ProofFamilyId::Output),
        other => anyhow::bail!("unsupported --family value {other}; use spend or output"),
    }
}

fn family_pvk(
    family: ProofFamilyId,
) -> &'static ark_groth16::PreparedVerifyingKey<decaf377::Bls12_377> {
    match family {
        ProofFamilyId::Spend => &SPEND_PROOF_VERIFICATION_KEY,
        ProofFamilyId::Output => &OUTPUT_PROOF_VERIFICATION_KEY,
        other => panic!("unsupported family in corpus_aggregate_verify: {other:?}"),
    }
}

fn extract_family_items(
    corpus: &corpus::Corpus,
    family: ProofFamilyId,
    tx_limit: usize,
) -> Result<Vec<SelectedItem>> {
    let limit = if tx_limit == 0 {
        corpus.entries.len()
    } else {
        tx_limit.min(corpus.entries.len())
    };

    let mut items = Vec::new();
    for entry in corpus.entries.iter().take(limit) {
        let tx = Transaction::decode(entry.tx_bytes.as_slice())
            .with_context(|| format!("decoding tx ordinal {}", entry.ordinal))?;
        let context = tx.context();

        for (action_index, action) in tx.actions().enumerate() {
            match (family, action) {
                (ProofFamilyId::Spend, Action::Spend(spend)) => {
                    let public = spend_extract_public(&spend, &context).with_context(|| {
                        format!(
                            "extracting spend public inputs tx_ordinal={} action_index={}",
                            entry.ordinal, action_index
                        )
                    })?;
                    let item = spend.proof.to_batch_item(public).with_context(|| {
                        format!(
                            "building spend batch item tx_ordinal={} action_index={}",
                            entry.ordinal, action_index
                        )
                    })?;
                    let public_inputs_hex = item
                        .public_inputs
                        .iter()
                        .map(|value| format!("{value:?}"))
                        .collect::<Vec<_>>();
                    items.push(SelectedItem {
                        tx_ordinal: entry.ordinal,
                        tx_hash: entry.tx_hash_hex.clone(),
                        action_index,
                        public_inputs_hex,
                        item,
                    });
                }
                (ProofFamilyId::Output, Action::Output(output)) => {
                    let public = output_extract_public(&output).with_context(|| {
                        format!(
                            "extracting output public inputs tx_ordinal={} action_index={}",
                            entry.ordinal, action_index
                        )
                    })?;
                    let item = output.proof.to_batch_item(public).with_context(|| {
                        format!(
                            "building output batch item tx_ordinal={} action_index={}",
                            entry.ordinal, action_index
                        )
                    })?;
                    let public_inputs_hex = item
                        .public_inputs
                        .iter()
                        .map(|value| format!("{value:?}"))
                        .collect::<Vec<_>>();
                    items.push(SelectedItem {
                        tx_ordinal: entry.ordinal,
                        tx_hash: entry.tx_hash_hex.clone(),
                        action_index,
                        public_inputs_hex,
                        item,
                    });
                }
                _ => {}
            }
        }
    }

    Ok(items)
}
