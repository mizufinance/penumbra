use crate::{
    config::{CustodyConfig, PcliConfig},
    default_home,
    terminal::ActualTerminal,
    App, Command,
};
use anyhow::Result;
use camino::Utf8PathBuf;
use clap::Parser;
use shieldd_sdk_custody::{null_kms::NullKms, soft_kms::SoftKms};
use shieldd_sdk_proto::box_grpc_svc;
use shieldd_sdk_proto::custody::v1::{
    custody_service_client::CustodyServiceClient, custody_service_server::CustodyServiceServer,
};
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
        let fvk = config.full_viewing_key.clone();

        // Build the custody service...
        let custody = match &config.custody {
            CustodyConfig::ViewOnly => {
                tracing::info!("using view-only custody service");
                let null_kms = NullKms::default();
                let custody_svc = CustodyServiceServer::new(null_kms);
                CustodyServiceClient::new(box_grpc_svc::local(custody_svc))
            }
            CustodyConfig::SoftKms(config) => {
                tracing::info!("using software KMS custody service");
                let soft_kms = SoftKms::new(config.clone());
                let custody_svc = CustodyServiceServer::new(soft_kms);
                CustodyServiceClient::new(box_grpc_svc::local(custody_svc))
            }
            CustodyConfig::Threshold(config) => {
                tracing::info!("using manual threshold custody service");
                let threshold_kms = shieldd_sdk_custody::threshold::Threshold::new(
                    config.clone(),
                    ActualTerminal {
                        fvk: Some(fvk.clone()),
                    },
                );
                let custody_svc = CustodyServiceServer::new(threshold_kms);
                CustodyServiceClient::new(box_grpc_svc::local(custody_svc))
            }
            CustodyConfig::Encrypted(config) => {
                tracing::info!("using encrypted custody service");
                let encrypted_kms = shieldd_sdk_custody::encrypted::Encrypted::new(
                    config.clone(),
                    ActualTerminal {
                        fvk: Some(fvk.clone()),
                    },
                );
                let custody_svc = CustodyServiceServer::new(encrypted_kms);
                CustodyServiceClient::new(box_grpc_svc::local(custody_svc))
            }
            #[cfg(feature = "ledger")]
            CustodyConfig::Ledger(config) => {
                tracing::info!("using ledger custody service");
                let service = shieldd_sdk_custody_ledger_usb::Service::new(config.clone());
                let custody_svc = CustodyServiceServer::new(service);
                CustodyServiceClient::new(box_grpc_svc::local(custody_svc))
            }
        };

        let app = App { custody, config };
        Ok((app, self.cmd))
    }
}
