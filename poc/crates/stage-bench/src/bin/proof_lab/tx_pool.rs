use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use penumbra_sdk_bench::proof_txs;

#[derive(Debug, Parser)]
#[clap(name = "proof_tx_pool")]
#[clap(about = "Generate and verify persistent synthetic proof-tx pools")]
struct Cli {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Generate {
        #[clap(long)]
        count: usize,
        #[clap(long)]
        out: Option<PathBuf>,
    },
    Verify {
        #[clap(long)]
        pool: PathBuf,
    },
}

pub async fn run_from<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::parse_from(args);
    match cli.command {
        Command::Generate { count, out } => generate(count, out).await,
        Command::Verify { pool } => verify(pool),
    }
}

async fn generate(count: usize, out: Option<PathBuf>) -> Result<()> {
    let out_dir = out.unwrap_or_else(|| proof_txs::default_pool_dir(count));
    if out_dir.exists() {
        std::fs::remove_dir_all(&out_dir)?;
    }
    std::fs::create_dir_all(&out_dir)?;

    let (storage, _node, client) = proof_txs::setup_proof_storage(count).await?;
    let pool = proof_txs::build_proof_tx_pool(client, &storage, count).await?;
    let metadata = proof_txs::save_proof_tx_pool(&out_dir, &pool)?;
    let verified = proof_txs::verify_proof_tx_pool(&out_dir)?;

    println!(
        "Generated proof tx pool at {} (tx_count={}, shard_count={}, raw_bytes={}, compressed_bytes={})",
        out_dir.display(),
        verified.tx_count,
        verified.shard_count,
        verified.raw_bytes,
        verified.compressed_bytes
    );
    println!(
        "Compatibility fingerprint: {}",
        metadata.compatibility_fingerprint
    );

    Ok(())
}

fn verify(pool: PathBuf) -> Result<()> {
    let metadata = proof_txs::verify_proof_tx_pool(&pool)?;
    println!(
        "Verified proof tx pool {} (tx_count={}, shard_count={}, fingerprint={})",
        pool.display(),
        metadata.tx_count,
        metadata.shard_count,
        metadata.compatibility_fingerprint
    );
    Ok(())
}
