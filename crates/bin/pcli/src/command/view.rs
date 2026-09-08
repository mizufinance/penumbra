use crate::App;
use address::AddressCmd;
use anyhow::Result;
use wallet_id::WalletIdCmd;
mod address;
mod wallet_id;

#[derive(Debug, clap::Subcommand)]
pub enum ViewCmd {
    /// View your wallet identifier.
    WalletId(WalletIdCmd),
    /// Derive an indexed or ephemeral address.
    Address(AddressCmd),
}

impl ViewCmd {
    pub fn exec(&self, app: &App) -> Result<()> {
        match self {
            Self::WalletId(cmd) => cmd.exec(&app.config.full_viewing_key),
            Self::Address(cmd) => cmd.exec(&app.config.full_viewing_key),
        }
    }
}
