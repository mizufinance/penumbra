// Requires nightly
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![deny(clippy::unwrap_used)]
#![allow(clippy::clone_on_copy)]

mod current_consensus_keys;
pub mod event;
mod governance_key;
mod identity_key;
mod uptime;

pub mod genesis;
pub mod params;
pub mod state_key;
pub mod validator;

#[cfg(feature = "component")]
pub mod component;

#[cfg(feature = "component")]
pub use component::{StateReadExt, StateWriteExt};

pub use self::governance_key::GovernanceKey;
pub use self::identity_key::IdentityKey;
pub use self::identity_key::IDENTITY_KEY_LEN_BYTES;

pub use self::current_consensus_keys::CurrentConsensusKeys;
pub use self::uptime::Uptime;
