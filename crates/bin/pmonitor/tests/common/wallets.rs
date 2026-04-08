use anyhow::Result;
use pcli::config::PcliConfig;
use penumbra_sdk_keys::address::Address;
use std::fs::{create_dir_all, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use super::PmonitorTestRunner;
use crate::common::pcli_helpers::{pcli_init_softkms, pcli_view_address};

impl PmonitorTestRunner {
    /// Create directory and return path for storing client wallets.
    pub fn wallets_dir(&self) -> Result<PathBuf> {
        let path = self.pmonitor_integration_test_dir.join("wallets");
        create_dir_all(&path)?;
        Ok(path)
    }

    /// Initialize local pcli configs for all wallets specified in config.
    pub fn create_pcli_wallets(&self) -> Result<()> {
        for index in 0..self.num_wallets {
            let pcli_home = self.wallets_dir()?.join(format!("wallet-{index}"));
            pcli_init_softkms(&self.binaries.pcli, &pcli_home)?;
        }
        Ok(())
    }

    /// Iterate over all client wallets and return a `PcliConfig` for each.
    pub fn get_pcli_wallet_configs(&self) -> Result<Vec<PcliConfig>> {
        (0..self.num_wallets)
            .map(|index| {
                let pcli_home = self.wallets_dir()?.join(format!("wallet-{index}"));
                let pcli_config_path = pcli_home.join("config.toml");
                PcliConfig::load(
                    pcli_config_path
                        .to_str()
                        .expect("failed to convert pcli wallet path to str"),
                )
            })
            .collect()
    }

    /// Iterate over all client wallets and return address 0 for each.
    pub fn get_pcli_wallet_addresses(&self) -> Result<Vec<Address>> {
        (0..self.num_wallets)
            .map(|index| {
                let pcli_home = self.wallets_dir()?.join(format!("wallet-{index}"));
                pcli_view_address(&self.binaries.pcli, &pcli_home)
            })
            .collect()
    }

    /// Iterate over all client wallets, grab an FVK for each, write those
    /// FVKs to a local JSON file, and return the path to that file.
    pub fn get_pcli_wallet_fvks_filepath(&self) -> Result<PathBuf> {
        let path = self.pmonitor_integration_test_dir.join("fvks.json");
        if !path.exists() {
            let fvks: Vec<String> = self
                .get_pcli_wallet_configs()?
                .into_iter()
                .map(|config| config.full_viewing_key.to_string())
                .collect();
            let mut writer = BufWriter::new(File::create(&path)?);
            serde_json::to_writer(&mut writer, &fvks)?;
            writer.flush()?;
        }
        Ok(path)
    }

    /// Create a CSV file of genesis allocations for all pcli test wallets.
    pub fn generate_genesis_allocations(&self) -> Result<PathBuf> {
        let allocations_filepath = self.pmonitor_integration_test_dir.join("allocations.csv");
        if !allocations_filepath.exists() {
            let mut writer = BufWriter::new(File::create(&allocations_filepath)?);
            writer.write_all(b"amount,denom,address\n")?;
            for address in self.get_pcli_wallet_addresses()? {
                let allocation =
                    format!("1_000_000__000_000,upenumbra,{address}\n1000,test_usd,{address}\n");
                writer.write_all(allocation.as_bytes())?;
            }
            writer.flush()?;
        }
        Ok(allocations_filepath)
    }
}
