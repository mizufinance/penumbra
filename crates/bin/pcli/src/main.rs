#![deny(clippy::unwrap_used)]
#![allow(clippy::clone_on_copy)]

use std::fs;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;
use rustls::crypto::aws_lc_rs;

use pcli::{command::*, opt::Opt};

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
    // Preserved for posterity and memory
    if std::env::var("PCLI_DISPLAY_WARNING").is_ok() {
        pcli::warning::display();
    }

    let mut opt = Opt::parse();

    // Initialize tracing here, rather than when converting into an `App`, so
    // that tracing is set up even for wallet commands that don't build the `App`.
    opt.init_tracing();

    // Initialize HTTPS support
    // rustls::crypto::aws_lc_rs::default_provider().install_default();
    aws_lc_rs::default_provider()
        .install_default()
        .expect("failed to initialize rustls support, via aws-lc-rs");

    //Ensure that the data_path exists, in case this is a cold start
    fs::create_dir_all(&opt.home)
        .with_context(|| format!("Failed to create home directory {}", opt.home))?;

    // The init command takes the home dir directly, since it may need to
    // create the client state, so handle it specially here so that we can have
    // common code for the other subcommands.
    if let Command::Init(init_cmd) = &opt.cmd {
        init_cmd.exec(opt.home.as_path()).await?;
        return Ok(());
    }

    // The view reset command takes the home dir directly, and should not be invoked when there's a
    // view service running.
    if let Command::View(ViewCmd::Reset(reset)) = &opt.cmd {
        reset.exec(opt.home.as_path())?;
        return Ok(());
    }
    // The debug command takes the home dir directly
    if let Command::Debug(debug_cmd) = &opt.cmd {
        let dd = opt.home.into_std_path_buf();
        debug_cmd.exec(dd)?;
        return Ok(());
    }

    if let Command::Transaction(tx_cmd) = &opt.cmd {
        if let TxCmd::Compliance(compliance_cmd) = &tx_cmd.cmd {
            if compliance_cmd.is_scan() {
                compliance_cmd.exec_scan().await?;
                return Ok(());
            }
            if compliance_cmd.is_generate_dk() {
                compliance_cmd.exec_generate_dk()?;
                return Ok(());
            }
            if compliance_cmd.is_sign_grant() {
                compliance_cmd.exec_sign_grant()?;
                return Ok(());
            }
        }
    }

    let (mut app, cmd) = opt.into_app().await?;

    if !cmd.offline() {
        app.sync().await?;
    }

    // TODO: this is a mess, figure out the right way to bundle up the clients + fvk
    // make sure to be compatible with client for remote view service, with different
    // concrete type

    match &cmd {
        Command::Init(_) => unreachable!("init command already executed"),
        Command::Debug(_) => unreachable!("debug command already executed"),
        Command::Transaction(tx_cmd) => tx_cmd.exec(&mut app).await?,
        Command::View(view_cmd) => view_cmd.exec(&mut app).await?,
        Command::Validator(cmd) => cmd.exec(&mut app).await?,
        Command::Query(cmd) => cmd.exec(&mut app).await?,
        Command::Threshold(cmd) => cmd.exec(&mut app).await?,
    }

    Ok(())
}
