use anyhow::Result;
use shieldd_sdk_custody::threshold::Terminal;

use crate::{config::CustodyConfig, terminal::ActualTerminal, App};

#[derive(Debug, clap::Subcommand)]
pub enum ThresholdCmd {
    /// Contribute to signing a transaction with threshold custody
    Sign,
}

impl ThresholdCmd {
    #[tracing::instrument(skip(self, app))]
    pub async fn exec(&self, app: &mut App) -> Result<()> {
        let config = match app.config.custody.clone() {
            CustodyConfig::Threshold(config) => Some(config),
            CustodyConfig::Encrypted(config) => {
                let password = ActualTerminal::default().get_password().await?;
                config.convert_to_threshold(&password)?
            }
            _ => None, // If not threshold, we can't sign using threshold config
        };
        match self {
            ThresholdCmd::Sign => {
                shieldd_sdk_custody::threshold::follow(config.as_ref(), &ActualTerminal::default())
                    .await
            }
        }
    }
}
