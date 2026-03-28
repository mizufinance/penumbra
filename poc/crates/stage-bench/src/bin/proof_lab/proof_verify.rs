use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use penumbra_sdk_bench::tps::corpus;
use penumbra_sdk_proof_params::{
    batch, OUTPUT_PROOF_VERIFICATION_KEY, SPEND_PROOF_VERIFICATION_KEY,
};
use penumbra_sdk_proto::DomainType;
use penumbra_sdk_shielded_pool::component::{output_extract_public, spend_extract_public};
use penumbra_sdk_transaction::txhash::AuthorizingData;
use penumbra_sdk_transaction::{Action, Transaction};

#[derive(Debug, Parser)]
#[clap(name = "corpus_proof_verify")]
#[clap(
    about = "Verify spend/output proofs in a corpus individually and via single-item batch verify"
)]
struct Cli {
    #[clap(long)]
    corpus: PathBuf,
    #[clap(long, default_value_t = 0)]
    tx_limit: usize,
    #[clap(long, default_value_t = 20)]
    max_failures: usize,
    #[clap(long)]
    family: Option<String>,
    #[clap(long)]
    tx_ordinal: Option<usize>,
    #[clap(long)]
    action_index: Option<usize>,
    #[clap(long, default_value_t = 0)]
    batch_size: usize,
}

#[derive(Default)]
struct Stats {
    tx_count: usize,
    spend_count: usize,
    output_count: usize,
    binding_sig_failures: usize,
    to_batch_item_failures: usize,
    batch_verify_failures: usize,
    direct_verify_failures: usize,
}

struct Failure {
    tx_hash: String,
    action_index: usize,
    action_kind: &'static str,
    stage: &'static str,
    error: String,
}

#[derive(Clone)]
struct ExtractedItem {
    tx_hash: String,
    tx_ordinal: usize,
    action_index: usize,
    action_kind: &'static str,
    item: batch::BatchItem,
    direct_verify_ok: bool,
    a_on_curve: bool,
    a_in_subgroup: bool,
    b_on_curve: bool,
    b_in_subgroup: bool,
    c_on_curve: bool,
    c_in_subgroup: bool,
}

pub fn run_from<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::parse_from(args);
    let corpus = corpus::load_corpus(&cli.corpus)
        .with_context(|| format!("loading corpus {}", cli.corpus.display()))?;

    let mut stats = Stats::default();
    let mut failures = Vec::new();
    let limit = if cli.tx_limit == 0 {
        corpus.entries.len()
    } else {
        cli.tx_limit.min(corpus.entries.len())
    };

    let mut spend_items = Vec::new();
    let mut output_items = Vec::new();

    for entry in corpus.entries.iter().take(limit) {
        if let Some(tx_ordinal) = cli.tx_ordinal {
            if entry.ordinal != tx_ordinal {
                continue;
            }
        }
        stats.tx_count += 1;

        let tx = Transaction::decode(entry.tx_bytes.as_slice())
            .with_context(|| format!("decoding tx ordinal {}", entry.ordinal))?;

        if let Err(err) = verify_binding_sig(&tx) {
            stats.binding_sig_failures += 1;
            maybe_push_failure(
                &mut failures,
                cli.max_failures,
                Failure {
                    tx_hash: entry.tx_hash_hex.clone(),
                    action_index: usize::MAX,
                    action_kind: "tx",
                    stage: "binding_sig",
                    error: format!("{err:#}"),
                },
            );
            continue;
        }

        let context = tx.context();
        for (action_index, action) in tx.actions().enumerate() {
            if let Some(filter_action_index) = cli.action_index {
                if action_index != filter_action_index {
                    continue;
                }
            }
            match action {
                Action::Spend(spend) => {
                    stats.spend_count += 1;
                    if family_matches(cli.family.as_deref(), "spend") {
                        match extract_spend(
                            entry.ordinal,
                            entry.tx_hash_hex.as_str(),
                            action_index,
                            spend,
                            &context,
                        ) {
                            Ok(extracted) => spend_items.push(extracted),
                            Err(err) => {
                                classify_failure(&mut stats, err.stage);
                                maybe_push_failure(&mut failures, cli.max_failures, err);
                            }
                        }
                    }
                }
                Action::Output(output) => {
                    stats.output_count += 1;
                    if family_matches(cli.family.as_deref(), "output") {
                        match extract_output(
                            entry.ordinal,
                            entry.tx_hash_hex.as_str(),
                            action_index,
                            output,
                        ) {
                            Ok(extracted) => output_items.push(extracted),
                            Err(err) => {
                                classify_failure(&mut stats, err.stage);
                                maybe_push_failure(&mut failures, cli.max_failures, err);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    if let Some(family) = cli.family.as_deref() {
        match family {
            "spend" => run_batch_checks(
                "spend",
                &spend_items,
                cli.batch_size,
                &mut stats,
                &mut failures,
                cli.max_failures,
                &SPEND_PROOF_VERIFICATION_KEY,
            ),
            "output" => run_batch_checks(
                "output",
                &output_items,
                cli.batch_size,
                &mut stats,
                &mut failures,
                cli.max_failures,
                &OUTPUT_PROOF_VERIFICATION_KEY,
            ),
            other => anyhow::bail!("unsupported --family value {other}; use spend or output"),
        }
    } else {
        run_batch_checks(
            "spend",
            &spend_items,
            cli.batch_size,
            &mut stats,
            &mut failures,
            cli.max_failures,
            &SPEND_PROOF_VERIFICATION_KEY,
        );
        run_batch_checks(
            "output",
            &output_items,
            cli.batch_size,
            &mut stats,
            &mut failures,
            cli.max_failures,
            &OUTPUT_PROOF_VERIFICATION_KEY,
        );
    }

    println!("corpus={}", cli.corpus.display());
    println!("tx_count={}", stats.tx_count);
    println!("spend_count={}", stats.spend_count);
    println!("output_count={}", stats.output_count);
    println!("binding_sig_failures={}", stats.binding_sig_failures);
    println!("to_batch_item_failures={}", stats.to_batch_item_failures);
    println!("batch_verify_failures={}", stats.batch_verify_failures);
    println!("direct_verify_failures={}", stats.direct_verify_failures);
    println!("success={}", total_failures(&stats) == 0);

    if !failures.is_empty() {
        println!("failures:");
        for failure in failures {
            println!(
                "  tx={} action_index={} kind={} stage={} error={}",
                failure.tx_hash,
                failure.action_index,
                failure.action_kind,
                failure.stage,
                failure.error
            );
        }
    }

    if total_failures(&stats) > 0 {
        anyhow::bail!("corpus proof verification found failures");
    }

    Ok(())
}

fn family_matches(filter: Option<&str>, family: &str) -> bool {
    match filter {
        None => true,
        Some(value) => value == family,
    }
}

fn verify_binding_sig(tx: &Transaction) -> Result<()> {
    let auth_hash = tx.auth_hash();
    tx.binding_verification_key()
        .verify(auth_hash.as_bytes(), tx.binding_sig())
        .map_err(|e| anyhow::anyhow!("binding signature should verify: {e}"))
}

fn extract_spend(
    tx_ordinal: usize,
    tx_hash: &str,
    action_index: usize,
    spend: &penumbra_sdk_shielded_pool::Spend,
    context: &penumbra_sdk_transaction::txhash::TransactionContext,
) -> std::result::Result<ExtractedItem, Failure> {
    let public = spend_extract_public(spend, context).map_err(|err| Failure {
        tx_hash: tx_hash.to_string(),
        action_index,
        action_kind: "spend",
        stage: "extract_public",
        error: format!("{err:#}"),
    })?;

    let item = spend
        .proof
        .to_batch_item(public.clone())
        .map_err(|err| Failure {
            tx_hash: tx_hash.to_string(),
            action_index,
            action_kind: "spend",
            stage: "to_batch_item",
            error: format!("{err:#}"),
        })?;

    let direct_verify_ok = spend
        .proof
        .verify(&SPEND_PROOF_VERIFICATION_KEY, public)
        .map(|_| true)
        .map_err(|err| Failure {
            tx_hash: tx_hash.to_string(),
            action_index,
            action_kind: "spend",
            stage: "direct_verify",
            error: format!("{err:#}"),
        })?;

    Ok(ExtractedItem {
        tx_hash: tx_hash.to_string(),
        tx_ordinal,
        action_index,
        action_kind: "spend",
        a_on_curve: item.proof.a.is_on_curve(),
        a_in_subgroup: item.proof.a.is_in_correct_subgroup_assuming_on_curve(),
        b_on_curve: item.proof.b.is_on_curve(),
        b_in_subgroup: item.proof.b.is_in_correct_subgroup_assuming_on_curve(),
        c_on_curve: item.proof.c.is_on_curve(),
        c_in_subgroup: item.proof.c.is_in_correct_subgroup_assuming_on_curve(),
        item,
        direct_verify_ok,
    })
}

fn extract_output(
    tx_ordinal: usize,
    tx_hash: &str,
    action_index: usize,
    output: &penumbra_sdk_shielded_pool::Output,
) -> std::result::Result<ExtractedItem, Failure> {
    let public = output_extract_public(output).map_err(|err| Failure {
        tx_hash: tx_hash.to_string(),
        action_index,
        action_kind: "output",
        stage: "extract_public",
        error: format!("{err:#}"),
    })?;

    let item = output
        .proof
        .to_batch_item(public.clone())
        .map_err(|err| Failure {
            tx_hash: tx_hash.to_string(),
            action_index,
            action_kind: "output",
            stage: "to_batch_item",
            error: format!("{err:#}"),
        })?;

    let direct_verify_ok = output
        .proof
        .verify(&OUTPUT_PROOF_VERIFICATION_KEY, public)
        .map(|_| true)
        .map_err(|err| Failure {
            tx_hash: tx_hash.to_string(),
            action_index,
            action_kind: "output",
            stage: "direct_verify",
            error: format!("{err:#}"),
        })?;

    Ok(ExtractedItem {
        tx_hash: tx_hash.to_string(),
        tx_ordinal,
        action_index,
        action_kind: "output",
        a_on_curve: item.proof.a.is_on_curve(),
        a_in_subgroup: item.proof.a.is_in_correct_subgroup_assuming_on_curve(),
        b_on_curve: item.proof.b.is_on_curve(),
        b_in_subgroup: item.proof.b.is_in_correct_subgroup_assuming_on_curve(),
        c_on_curve: item.proof.c.is_on_curve(),
        c_in_subgroup: item.proof.c.is_in_correct_subgroup_assuming_on_curve(),
        item,
        direct_verify_ok,
    })
}

fn run_batch_checks(
    family: &str,
    extracted: &[ExtractedItem],
    batch_size: usize,
    stats: &mut Stats,
    failures: &mut Vec<Failure>,
    max_failures: usize,
    pvk: &ark_groth16::PreparedVerifyingKey<decaf377::Bls12_377>,
) {
    let take = if batch_size == 0 {
        extracted.len()
    } else {
        batch_size.min(extracted.len())
    };
    let selected = &extracted[..take];
    if selected.is_empty() {
        println!("family={family} selected_count=0");
        return;
    }

    let bad_subgroup = selected
        .iter()
        .filter(|item| {
            !(item.a_on_curve
                && item.a_in_subgroup
                && item.b_on_curve
                && item.b_in_subgroup
                && item.c_on_curve
                && item.c_in_subgroup)
        })
        .count();
    println!(
        "family={family} selected_count={} bad_subgroup_count={}",
        selected.len(),
        bad_subgroup
    );
    for item in selected.iter().take(10) {
        println!(
            "  item tx_ordinal={} tx_hash={} action_index={} kind={} direct_verify_ok={} a=({}, {}) b=({}, {}) c=({}, {})",
            item.tx_ordinal,
            item.tx_hash,
            item.action_index,
            item.action_kind,
            item.direct_verify_ok,
            item.a_on_curve,
            item.a_in_subgroup,
            item.b_on_curve,
            item.b_in_subgroup,
            item.c_on_curve,
            item.c_in_subgroup,
        );
    }

    let batch_items = selected
        .iter()
        .cloned()
        .map(|item| item.item)
        .collect::<Vec<_>>();
    if let Err(err) = batch::batch_verify(pvk, &batch_items) {
        stats.batch_verify_failures += batch_items.len();
        for item in selected
            .iter()
            .take(max_failures.saturating_sub(failures.len()))
        {
            failures.push(Failure {
                tx_hash: item.tx_hash.clone(),
                action_index: item.action_index,
                action_kind: item.action_kind,
                stage: "batch_verify_selected",
                error: format!(
                    "{err}; direct_verify_ok={} a=({}, {}) b=({}, {}) c=({}, {})",
                    item.direct_verify_ok,
                    item.a_on_curve,
                    item.a_in_subgroup,
                    item.b_on_curve,
                    item.b_in_subgroup,
                    item.c_on_curve,
                    item.c_in_subgroup
                ),
            });
        }
    }
}

fn maybe_push_failure(failures: &mut Vec<Failure>, max_failures: usize, failure: Failure) {
    if failures.len() < max_failures {
        failures.push(failure);
    }
}

fn classify_failure(stats: &mut Stats, stage: &str) {
    match stage {
        "to_batch_item" => stats.to_batch_item_failures += 1,
        "batch_verify" => {}
        "direct_verify" => stats.direct_verify_failures += 1,
        _ => {}
    }
}

fn total_failures(stats: &Stats) -> usize {
    stats.binding_sig_failures
        + stats.to_batch_item_failures
        + stats.batch_verify_failures
        + stats.direct_verify_failures
}
