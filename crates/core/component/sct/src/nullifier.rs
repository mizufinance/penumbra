use decaf377::Fq;
use poseidon377::hash_3;
use shieldd_sdk_tct::StateCommitment;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use shieldd_sdk_keys::keys::NullifierKey;
use shieldd_sdk_proto::{core::component::sct::v1 as pb, DomainType};

#[derive(PartialEq, Eq, Clone, Copy, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "pb::Nullifier", into = "pb::Nullifier")]
pub struct Nullifier(pub Fq);

impl Nullifier {
    pub fn parse_hex(str: &str) -> anyhow::Result<Nullifier> {
        let bytes = hex::decode(str)?;
        Nullifier::try_from(&bytes[..])
    }
}

impl DomainType for Nullifier {
    type Proto = pb::Nullifier;
}

impl From<Nullifier> for pb::Nullifier {
    fn from(n: Nullifier) -> Self {
        pb::Nullifier {
            inner: n.0.to_bytes().to_vec(),
        }
    }
}

impl TryFrom<pb::Nullifier> for Nullifier {
    type Error = anyhow::Error;
    fn try_from(n: pb::Nullifier) -> Result<Self, Self::Error> {
        n.inner.as_slice().try_into()
    }
}

/// The domain separator used to derive nullifiers.
pub static NULLIFIER_DOMAIN_SEP: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(blake2b_simd::blake2b(b"shieldd.nullifier").as_bytes())
});

impl std::fmt::Display for Nullifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&hex::encode(self.to_bytes()))
    }
}

impl std::fmt::Debug for Nullifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Nullifier")
            .field(&hex::encode(self.to_bytes()))
            .finish()
    }
}

impl Nullifier {
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }

    /// Derive the [`Nullifier`] for a positioned note or swap given its [`merkle::Position`]
    /// and [`Commitment`].
    pub fn derive(
        nk: &NullifierKey,
        pos: shieldd_sdk_tct::Position,
        state_commitment: &StateCommitment,
    ) -> Nullifier {
        Nullifier(hash_3(
            &NULLIFIER_DOMAIN_SEP,
            (nk.0, state_commitment.0, (u64::from(pos)).into()),
        ))
    }
}

impl From<Nullifier> for [u8; 32] {
    fn from(nullifier: Nullifier) -> [u8; 32] {
        nullifier.0.to_bytes()
    }
}

impl TryFrom<&[u8]> for Nullifier {
    type Error = anyhow::Error;

    fn try_from(slice: &[u8]) -> Result<Nullifier, Self::Error> {
        let bytes: [u8; 32] = slice[..].try_into()?;
        let inner = Fq::from_bytes_checked(&bytes)
            .map_err(|_| anyhow::anyhow!("invalid nullifier field encoding"))?;
        Ok(Nullifier(inner))
    }
}

impl TryFrom<Vec<u8>> for Nullifier {
    type Error = anyhow::Error;

    fn try_from(vec: Vec<u8>) -> Result<Nullifier, Self::Error> {
        Self::try_from(&vec[..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_field_encoding_is_rejected_without_panicking() {
        let invalid = [u8::MAX; 32];
        assert!(Nullifier::try_from(invalid.as_slice()).is_err());
    }
}
