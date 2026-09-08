use std::{
    io::{stdin, IsTerminal as _, Read, Write},
    str::FromStr,
};

use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use rand_core::OsRng;
use shieldd_sdk_custody::threshold;
#[cfg(feature = "ledger")]
use shieldd_sdk_custody_ledger_usb as ledger;
use shieldd_sdk_keys::keys::{Bip44Path, SeedPhrase, SpendKey};
use termion::screen::IntoAlternateScreen;

use crate::{
    config::{CustodyConfig, PcliConfig},
    terminal::ActualTerminal,
};

#[derive(Debug, clap::Parser)]
pub struct InitCmd {
    #[clap(subcommand)]
    pub subcmd: InitTopSubCmd,
    /// For configs with spend authority, this will enable password encryption.
    ///
    /// This has no effect on a view only service.
    #[clap(long, action)]
    encrypted: bool,
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum InitTopSubCmd {
    #[clap(flatten)]
    Wallet(InitSubCmd),
    /// Initialize `pcli` in view-only mode, without spending keys.
    #[clap(display_order = 200)]
    ViewOnly {},
    /// Wipe all `pcli` configuration and data, INCLUDING KEYS.
    #[clap(display_order = 900)]
    UnsafeWipe {},
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum InitSubCmd {
    /// Initialize using a basic, file-based custody backend.
    #[clap(subcommand, display_order = 100)]
    SoftKms(SoftKmsInitCmd),
    /// Initialize using a manual threshold signing backend.
    #[clap(subcommand, display_order = 150)]
    Threshold(ThresholdInitCmd),
    // This is selected by the top-level view-only command.
    #[clap(skip, display_order = 200)]
    ViewOnly,
    /// Initialize using a ledger hardware wallet.
    #[cfg(feature = "ledger")]
    #[clap(display_order = 250)]
    Ledger,
    /// If relevant, change the current config to an encrypted config, with a password.
    #[clap(display_order = 800)]
    ReEncrypt,
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum SoftKmsInitCmd {
    /// Generate a new seed phrase and import its corresponding key.
    #[clap(display_order = 100)]
    Generate {
        /// If set, will write the seed phrase to stdout.
        #[clap(long, action)]
        stdout: bool,
    },
    /// Import a spend key from an existing seed phrase.
    #[clap(display_order = 200)]
    ImportPhrase {
        /// Use raw BIP39 derivation instead of BIP44.
        #[clap(long, action)]
        raw_bip39_derivation: bool,
    },
}

// Reusable function for prompting interactively for key material.
fn prompt_for_password(msg: &str) -> Result<String> {
    let mut password = String::new();
    // The `rpassword` crate doesn't support reading from stdin, so we check
    // for an interactive session. We must support non-interactive use cases,
    // for integration with other tooling.
    if std::io::stdin().is_terminal() {
        password = rpassword::prompt_password(msg)?;
    } else {
        while let Ok(n_bytes) = std::io::stdin().lock().read_to_string(&mut password) {
            if n_bytes == 0 {
                break;
            }
            password = password.trim().to_string();
        }
    }
    Ok(password)
}

impl SoftKmsInitCmd {
    fn spend_key(&self) -> Result<SpendKey> {
        match self {
            SoftKmsInitCmd::Generate { stdout } => {
                let seed_phrase = SeedPhrase::generate(OsRng);
                let seed_msg = format!(
                    "YOUR PRIVATE SEED PHRASE:\n\n\
                   {seed_phrase}\n\n\
                   Save this in a safe place!\n\
                   DO NOT SHARE WITH ANYONE!\n"
                );

                let mut output = std::io::stdout();

                if *stdout {
                    output.write_all(seed_msg.as_bytes())?;
                    output.flush()?;
                } else {
                    let mut screen = output.into_alternate_screen()?;
                    writeln!(screen, "{seed_msg}")?;
                    screen.flush()?;
                    println!("Press enter to proceed.");
                    let _ = stdin().bytes().next();
                }

                let path = Bip44Path::new(0);
                SpendKey::from_seed_phrase_bip44(seed_phrase, &path)
                    .context("generated seed phrase produced an invalid spend key")
            }
            SoftKmsInitCmd::ImportPhrase {
                raw_bip39_derivation,
            } => {
                let seed_phrase = prompt_for_password("Enter seed phrase: ")?;
                let seed_phrase = SeedPhrase::from_str(&seed_phrase)
                    .context("failed to parse input as seed phrase")?;

                if *raw_bip39_derivation {
                    SpendKey::from_seed_phrase_bip39(seed_phrase, 0)
                        .context("imported seed phrase produced an invalid raw BIP39 spend key")
                } else {
                    let path = Bip44Path::new(0);
                    SpendKey::from_seed_phrase_bip44(seed_phrase, &path)
                        .context("imported seed phrase produced an invalid spend key")
                }
            }
        }
    }
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum ThresholdInitCmd {
    /// Use a centralized dealer to create config files for each signer.
    ///
    /// Unlike the other `pcli init` commands, this one ignores the global
    /// `--home` argument, since it generates one config for each signer.
    Deal {
        /// The minimum number of signers required to make a signature (>= 2).
        #[clap(short, long)]
        threshold: u16,
        /// A path to the home directory for each signer.
        ///
        /// Each directory will be configured to be used as the --home parameter
        /// for that signer's pcli instance.  This implicitly specifies the
        /// total number of signers (one for each --home).
        #[clap(long, value_delimiter = ' ', multiple_values = true)]
        home: Vec<Utf8PathBuf>,
    },
    /// Generate a config file without using a trusted dealer.
    Dkg {
        /// The minimum number of signers required to make a signature (>= 2).
        #[clap(short, long)]
        threshold: u16,
        /// The maximum number of signers that can make a signature
        #[clap(short, long)]
        num_participants: u16,
    },
}

fn exec_deal(threshold: u16, home: Vec<Utf8PathBuf>) -> Result<()> {
    if threshold < 2 {
        anyhow::bail!("threshold must be >= 2");
    }
    let n = home.len() as u16;

    // Check before doing anything so we don't write partial results if a config exists.
    for config_path in home.iter() {
        let config_path = config_path.join(crate::CONFIG_FILE_NAME);
        if config_path.exists() {
            anyhow::bail!(
                "config file already exists at {:?}; refusing to overwrite it",
                config_path
            );
        }
    }

    println!("Generating {}-of-{} threshold config.", threshold, n);
    let configs = threshold::Config::deal(&mut OsRng, threshold, n)?;
    println!("Writing dealt config files...");
    for (i, (config, config_path)) in configs.into_iter().zip(home.iter()).enumerate() {
        let full_viewing_key = config.fvk().clone();

        let config = PcliConfig {
            custody: CustodyConfig::Threshold(config),
            full_viewing_key,
            disable_warning: false,
        };

        println!("  Writing signer {} config to {}", i, config_path);
        std::fs::create_dir_all(config_path)?;
        config.save(config_path.join(crate::CONFIG_FILE_NAME))?;
    }
    Ok(())
}

impl InitCmd {
    pub async fn exec(&self, home_dir: impl AsRef<camino::Utf8Path>) -> Result<()> {
        let subcmd = match self.subcmd.clone() {
            InitTopSubCmd::Wallet(subcmd) => subcmd,
            InitTopSubCmd::ViewOnly {} => InitSubCmd::ViewOnly,
            InitTopSubCmd::UnsafeWipe {} => {
                println!("Deleting all data in {}...", home_dir.as_ref());
                std::fs::remove_dir_all(home_dir.as_ref())?;
                return Ok(());
            }
        };

        if let InitSubCmd::Threshold(ThresholdInitCmd::Deal { threshold, home }) = &subcmd {
            exec_deal(threshold.clone(), home.clone())?;
            return Ok(());
        }
        let home_dir = home_dir.as_ref();

        let existing_config = {
            let config_path = home_dir.join(crate::CONFIG_FILE_NAME);
            if config_path.exists() {
                Some(PcliConfig::load(config_path)?)
            } else {
                None
            }
        };
        let relevant_config_exists = existing_config.is_some();

        let (full_viewing_key, custody) = match (&subcmd, relevant_config_exists) {
            (InitSubCmd::SoftKms(cmd), false) => {
                let spend_key = cmd.spend_key()?;
                (
                    spend_key.full_viewing_key().clone(),
                    if self.encrypted {
                        let password = ActualTerminal::get_confirmed_password().await?;
                        CustodyConfig::Encrypted(shieldd_sdk_custody::encrypted::Config::create(
                            &password,
                            shieldd_sdk_custody::encrypted::InnerConfig::SoftKms(spend_key.into()),
                        )?)
                    } else {
                        CustodyConfig::SoftKms(spend_key.into())
                    },
                )
            }
            (
                InitSubCmd::Threshold(ThresholdInitCmd::Dkg {
                    threshold,
                    num_participants,
                }),
                false,
            ) => {
                let config =
                    threshold::dkg(*threshold, *num_participants, &ActualTerminal::default())
                        .await?;
                let fvk = config.fvk().clone();
                let custody_config = if self.encrypted {
                    let password = ActualTerminal::get_confirmed_password().await?;
                    CustodyConfig::Encrypted(shieldd_sdk_custody::encrypted::Config::create(
                        &password,
                        shieldd_sdk_custody::encrypted::InnerConfig::Threshold(config),
                    )?)
                } else {
                    CustodyConfig::Threshold(config)
                };
                (fvk, custody_config)
            }
            (InitSubCmd::Threshold(ThresholdInitCmd::Deal { .. }), _) => {
                unreachable!("this should already have been handled above")
            }
            (InitSubCmd::ViewOnly {}, false) => {
                let full_viewing_key = prompt_for_password("Enter full viewing key: ")?
                    .parse()
                    .context("failed to parse input as FullViewingKey")?;
                (full_viewing_key, CustodyConfig::ViewOnly)
            }
            (InitSubCmd::ReEncrypt, true) => {
                let config = existing_config.expect("the config should exist in this branch");
                let fvk = config.full_viewing_key;
                let custody = config.custody;
                let custody = match custody {
                    x @ CustodyConfig::ViewOnly => x,
                    x @ CustodyConfig::Encrypted(_) => x,
                    CustodyConfig::SoftKms(spend_key) => {
                        let password = ActualTerminal::get_confirmed_password().await?;
                        CustodyConfig::Encrypted(shieldd_sdk_custody::encrypted::Config::create(
                            &password,
                            shieldd_sdk_custody::encrypted::InnerConfig::SoftKms(spend_key),
                        )?)
                    }
                    CustodyConfig::Threshold(c) => {
                        let password = ActualTerminal::get_confirmed_password().await?;
                        CustodyConfig::Encrypted(shieldd_sdk_custody::encrypted::Config::create(
                            &password,
                            shieldd_sdk_custody::encrypted::InnerConfig::Threshold(c),
                        )?)
                    }
                    #[cfg(feature = "ledger")]
                    CustodyConfig::Ledger(_config) => {
                        anyhow::bail!("An additional layer of password encryption is not (currently) possible for hardware wallets.");
                    }
                };
                (fvk, custody)
            }
            (InitSubCmd::ReEncrypt, false) => {
                anyhow::bail!("re-encrypt requires existing config to exist",);
            }
            #[cfg(feature = "ledger")]
            (InitSubCmd::Ledger, false) => {
                let config = ledger::Config::initialize(ledger::InitOptions::default()).await?;
                let service = ledger::Service::new(config.clone());
                let fvk = service.impl_export_full_viewing_key().await?;
                (fvk, CustodyConfig::Ledger(config))
            }
            (_, true) => {
                anyhow::bail!(
                    "home directory {:?} is not empty; refusing to initialize",
                    home_dir
                );
            }
        };

        let config = PcliConfig {
            custody,
            full_viewing_key,
            disable_warning: false,
        };

        let config_path = home_dir.join(crate::CONFIG_FILE_NAME);
        println!("Writing generated config to {}", config_path);
        config.save(config_path)?;

        Ok(())
    }
}
