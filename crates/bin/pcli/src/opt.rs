use crate::{config::PcliConfig, default_home, App, Command};
use anyhow::Result;
use camino::Utf8PathBuf;
use clap::Parser;
use std::io::IsTerminal as _;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[clap(name = "pcli", about = "The Shieldd command-line interface.", version)]
pub struct Opt {
    #[clap(subcommand)]
    pub cmd: Command,
    /// The home directory used to store configuration and data.
    #[clap(long, default_value_t = default_home(), env = "SHIELDD_PCLI_HOME")]
    pub home: Utf8PathBuf,
}

impl Opt {
    pub fn init_tracing(&mut self) {
        tracing_subscriber::fmt()
            .with_ansi(std::io::stdout().is_terminal())
            .with_env_filter(
                EnvFilter::from_default_env()
                    // Without explicitly disabling the `r1cs` target, the ZK proof implementations
                    // will spend an enormous amount of CPU and memory building useless tracing output.
                    .add_directive(
                        "r1cs=off"
                            .parse()
                            .expect("rics=off is a valid filter directive"),
                    ),
            )
            .with_writer(std::io::stderr)
            .init();
    }

    pub fn load_config(&self) -> Result<PcliConfig> {
        let path = self.home.join(crate::CONFIG_FILE_NAME);
        PcliConfig::load(path)
    }

    pub async fn into_app(self) -> Result<(App, Command)> {
        let config = self.load_config()?;
        let app = App { config };
        Ok((app, self.cmd))
    }
}
