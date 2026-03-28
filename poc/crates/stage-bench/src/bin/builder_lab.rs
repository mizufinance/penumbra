use std::ffi::OsString;

use anyhow::{bail, Result};

mod builder_lab {
    pub mod frontier;
    pub mod lookahead;
    pub mod one_shot;
    pub mod single;
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
        bail!("expected subcommand: single|lookahead|frontier|one-shot");
    };
    let rest = args.collect::<Vec<_>>();
    match subcommand.to_string_lossy().as_ref() {
        "single" => builder_lab::single::run_from(forward_args("builder_lab single", rest)).await,
        "lookahead" => {
            builder_lab::lookahead::run_from(forward_args("builder_lab lookahead", rest)).await
        }
        "frontier" => {
            builder_lab::frontier::run_from(forward_args("builder_lab frontier", rest)).await
        }
        "one-shot" => {
            builder_lab::one_shot::run_from(forward_args("builder_lab one-shot", rest)).await
        }
        other => bail!("unknown builder_lab subcommand: {other}"),
    }
}
