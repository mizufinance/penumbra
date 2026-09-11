//! Registered LaKey public keys, separate from note recovery capabilities.
use anyhow::{ensure, Result};
use decaf377::{Element, Fq};
use once_cell::sync::Lazy;

static DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(blake2b_simd::blake2b(b"shieldd.audit.keys.v1").as_bytes())
});

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "Vec<u8>", into = "Vec<u8>")]
pub struct AuditKeys {
    pub epoch: u64,
    pub amount: Element,
    pub sender: Element,
    pub receiver: Element,
}

impl AuditKeys {
    pub fn validate(&self) -> Result<()> {
        for point in [self.amount, self.sender, self.receiver] {
            ensure!(!point.is_identity(), "identity audit key");
        }
        Ok(())
    }

    pub fn validate_registered(&self) -> Result<()> {
        self.validate()?;
        ensure!(
            self.epoch > 0,
            "registered audit keys require a nonzero epoch"
        );
        let points = [self.amount, self.sender, self.receiver];
        for (i, point) in points.iter().enumerate() {
            ensure!(
                *point != *crate::UNREGULATED_SINK_RING_PK,
                "registered audit key cannot be the unregulated sink"
            );
            for other in &points[i + 1..] {
                ensure!(point != other, "audit fields require distinct keys");
            }
        }
        Ok(())
    }

    #[cfg(any(test, feature = "test-helpers"))]
    pub fn test_keys() -> Self {
        Self {
            epoch: 1,
            amount: Element::GENERATOR * decaf377::Fr::from(201u64),
            sender: Element::GENERATOR * decaf377::Fr::from(202u64),
            receiver: Element::GENERATOR * decaf377::Fr::from(203u64),
        }
    }

    pub fn commitment(&self) -> Fq {
        poseidon377::hash_4(
            &DOMAIN,
            (
                Fq::from(self.epoch),
                self.amount.vartime_compress_to_field(),
                self.sender.vartime_compress_to_field(),
                self.receiver.vartime_compress_to_field(),
            ),
        )
    }

    pub fn to_bytes(&self) -> [u8; 104] {
        let mut bytes = [0; 104];
        bytes[..8].copy_from_slice(&self.epoch.to_le_bytes());
        for (i, point) in [self.amount, self.sender, self.receiver].iter().enumerate() {
            bytes[8 + i * 32..40 + i * 32].copy_from_slice(&point.vartime_compress().0);
        }
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() == 104, "invalid audit key bundle length");
        let point = |offset: usize| -> Result<Element> {
            let encoded: [u8; 32] = bytes[offset..offset + 32].try_into()?;
            let point = decaf377::Encoding(encoded)
                .vartime_decompress()
                .map_err(|_| anyhow::anyhow!("invalid audit key"))?;
            ensure!(
                point.vartime_compress().0 == encoded,
                "noncanonical audit key"
            );
            Ok(point)
        };
        let keys = Self {
            epoch: u64::from_le_bytes(bytes[..8].try_into()?),
            amount: point(8)?,
            sender: point(40)?,
            receiver: point(72)?,
        };
        keys.validate()?;
        Ok(keys)
    }

    pub fn unregulated() -> Self {
        let sink = *crate::UNREGULATED_SINK_RING_PK;
        Self {
            epoch: 0,
            amount: sink,
            sender: sink,
            receiver: sink,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn key_bundle_binds_epoch_and_every_key() {
        let keys = AuditKeys::unregulated();
        assert_eq!(AuditKeys::from_bytes(&keys.to_bytes()).unwrap(), keys);
        for offset in [0, 8, 40, 72] {
            let mut changed = keys.clone();
            match offset {
                0 => changed.epoch += 1,
                8 => changed.amount += Element::GENERATOR,
                40 => changed.sender += Element::GENERATOR,
                _ => changed.receiver += Element::GENERATOR,
            }
            assert_ne!(changed.commitment(), keys.commitment());
        }
        assert!(AuditKeys::from_bytes(&[0; 104]).is_err());
        assert!(AuditKeys::from_bytes(&[0; 103]).is_err());
    }
}

impl From<AuditKeys> for Vec<u8> {
    fn from(keys: AuditKeys) -> Self {
        keys.to_bytes().to_vec()
    }
}
impl TryFrom<Vec<u8>> for AuditKeys {
    type Error = anyhow::Error;
    fn try_from(bytes: Vec<u8>) -> Result<Self> {
        Self::from_bytes(&bytes)
    }
}
