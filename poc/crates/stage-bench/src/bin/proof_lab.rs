use std::ffi::OsString;

use anyhow::{bail, Result};

mod proof_lab {
    pub mod aggregate_verify;
    pub mod bundle_verify;
    pub mod proof_verify;
    pub mod tx_pool;
}

fn forward_args(binary: &str, rest: impl IntoIterator<Item = OsString>) -> Vec<OsString> {
    let mut args = vec![OsString::from(binary)];
    args.extend(rest);
    args
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let mut args = std::env::args_os();
    let _bin = args.next();
    let Some(subcommand) = args.next() else {
        bail!("expected subcommand: proof-verify|aggregate-verify|bundle-verify|tx-pool");
    };
    let rest = args.collect::<Vec<_>>();
    match subcommand.to_string_lossy().as_ref() {
        "proof-verify" => {
            proof_lab::proof_verify::run_from(forward_args("proof_lab proof-verify", rest))
        }
        "aggregate-verify" => proof_lab::aggregate_verify::run_from(forward_args(
            "proof_lab aggregate-verify",
            rest,
        )),
        "bundle-verify" => {
            proof_lab::bundle_verify::run_from(forward_args("proof_lab bundle-verify", rest))
                .await
        }
        "tx-pool" => {
            proof_lab::tx_pool::run_from(forward_args("proof_lab tx-pool", rest)).await
        }
        other => bail!("unknown proof_lab subcommand: {other}"),
    }
}
