#![deny(clippy::unwrap_used)]
use anyhow::{Context, Result};
use clap::Parser;
use pcli::{command::*, opt::Opt};
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<()> {
    let mut opt = Opt::parse();
    opt.init_tracing();
    match &opt.cmd {
        Command::Init(cmd) => {
            std::fs::create_dir_all(&opt.home).with_context(|| format!("creating {}", opt.home))?;
            return cmd.exec(opt.home.as_path()).await;
        }
        Command::Compliance(cmd) => return cmd.exec(),
        #[cfg(feature = "disclosure")]
        Command::Disclosure(cmd) => return cmd.exec(opt.home.as_path()).await,
        _ => {}
    }
    let (mut app, cmd) = opt.into_app().await?;
    match cmd {
        Command::View(cmd) => cmd.exec(&app),
        Command::Threshold(cmd) => cmd.exec(&mut app).await,
        _ => unreachable!("handled before loading custody"),
    }
}
