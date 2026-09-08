use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
#[cfg(feature = "ledger")]
use shieldd_sdk_custody_ledger_usb::Config as LedgerConfig;

use shieldd_sdk_custody::{
    encrypted::Config as EncryptedConfig, soft_kms::Config as SoftKmsConfig,
    threshold::Config as ThresholdConfig,
};
use shieldd_sdk_keys::FullViewingKey;

/// Configuration data for `pcli`.
#[serde_as]
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct PcliConfig {
    /// Disable the scary "you will lose all your money" warning.
    #[serde(default, skip_serializing_if = "is_default")]
    pub disable_warning: bool,
    /// The FVK used for viewing chain data.
    #[serde_as(as = "DisplayFromStr")]
    pub full_viewing_key: FullViewingKey,
    /// The custody backend to use.
    pub custody: CustodyConfig,
}

impl PcliConfig {
    pub fn load<P: AsRef<Path> + std::fmt::Display>(path: P) -> Result<Self> {
        let contents = std::fs::read_to_string(&path).context(format!(
            "pcli config file not found: {}. hint: run 'pcli init' to create new keys",
            &path
        ))?;
        Ok(toml::from_str(&contents)?)
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let contents = toml::to_string_pretty(&self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }
}

/// The custody backend to use.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
#[serde(tag = "backend")]
#[allow(clippy::large_enum_variant)]
pub enum CustodyConfig {
    /// A view-only client that can't sign transactions.
    ViewOnly,
    /// A software key management service.
    SoftKms(SoftKmsConfig),
    /// A manual threshold custody service.
    Threshold(ThresholdConfig),
    /// An encrypted custody service.
    Encrypted(EncryptedConfig),
    /// A custody service using an external ledger device.
    #[cfg(feature = "ledger")]
    Ledger(LedgerConfig),
}

impl Default for CustodyConfig {
    fn default() -> Self {
        Self::ViewOnly
    }
}

/// Helper function for Serde serialization, allowing us to skip serialization
/// of default config values.  Rationale: if we don't skip serialization of
/// defaults, if someone serializes a config with some default values, they're
/// "pinning" the current defaults as their choices for all time, and we have no
/// way to distinguish between fields they configured explicitly and ones they
/// passed through from the defaults. If we skip serializing default values,
/// then we know every value in the config was explicitly set.
fn is_default<T: Default + Eq>(value: &T) -> bool {
    *value == T::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_config() {
        let config = PcliConfig {
            disable_warning: false,
            full_viewing_key: shieldd_sdk_keys::test_keys::FULL_VIEWING_KEY.clone(),
            custody: CustodyConfig::SoftKms(SoftKmsConfig::from(
                shieldd_sdk_keys::test_keys::SPEND_KEY.clone(),
            )),
        };

        let mut config2 = config.clone();
        config2.custody = CustodyConfig::ViewOnly;
        config2.disable_warning = true;

        for original in [config, config2] {
            let encoded = toml::to_string_pretty(&original).unwrap();
            let decoded: PcliConfig = toml::from_str(&encoded).unwrap();
            assert_eq!(decoded.disable_warning, original.disable_warning);
            assert_eq!(decoded.full_viewing_key, original.full_viewing_key);
            assert_eq!(
                serde_json::to_value(&decoded.custody).unwrap(),
                serde_json::to_value(&original.custody).unwrap(),
            );
        }
    }
}
