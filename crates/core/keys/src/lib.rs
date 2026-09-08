#![deny(clippy::unwrap_used)]
// Requires nightly.
#![cfg_attr(docsrs, feature(doc_cfg))]

use decaf377_ka as ka;
use decaf377_rdsa as rdsa;

pub mod address;
pub mod keys;
pub mod prf;
pub mod symmetric;
pub mod test_keys;

pub use address::{Address, AddressError, AddressView};
pub use keys::{FullViewingKey, FullViewingKeyError, SpendKeyError};
pub use symmetric::{BackreferenceKey, PayloadKey, PositionMetadataKey};

/// Rejects the identity point for keys used to authorize protocol actions.
pub fn ensure_nonidentity_spend_auth_key(
    key: &rdsa::VerificationKey<rdsa::SpendAuth>,
    role: &str,
) -> anyhow::Result<()> {
    anyhow::ensure!(!key.is_identity(), "{role} must not be identity");
    Ok(())
}

fn fmt_hex<T: AsRef<[u8]>>(data: T, f: &mut std::fmt::Formatter) -> std::fmt::Result {
    write!(f, "{}", hex::encode(data))
}
