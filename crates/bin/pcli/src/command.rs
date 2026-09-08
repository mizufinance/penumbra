pub use compliance::ComplianceCmd;
pub use init::InitCmd;
pub use threshold::ThresholdCmd;
pub use view::ViewCmd;

mod compliance;
mod init;
mod threshold;
mod view;

#[derive(Debug, clap::Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum Command {
    /// Initialize or reset wallet keys and custody configuration.
    Init(InitCmd),
    /// Derive wallet identifiers and addresses locally.
    #[clap(subcommand)]
    View(ViewCmd),
    /// Generate compliance keys or sign registration grants locally.
    #[clap(subcommand)]
    Compliance(ComplianceCmd),
    /// Follow the threshold signing protocol.
    #[clap(subcommand)]
    Threshold(ThresholdCmd),
}
