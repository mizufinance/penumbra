use anyhow::{anyhow, ensure, Context, Result};
use decaf377::{Element, Fq, Fr};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use shieldd_sdk_compliance::crypto::compliance_stream_block;
use shieldd_sdk_num::Amount;
use shieldd_sdk_proto::{core::component::shielded_pool::v1 as pb, DomainType};

use crate::Rseed;

pub const RECOVERY_CAPSULE_BYTES: usize = 32 * 6;

static CAPSULE_COMMITMENT_DOMAIN: Lazy<Fq> =
    Lazy::new(|| domain(b"shieldd.recovery_capsule.commitment"));
static CAPSULE_CONFIRMATION_DOMAIN: Lazy<Fq> =
    Lazy::new(|| domain(b"shieldd.recovery_capsule.confirmation"));

fn domain(label: &[u8]) -> Fq {
    Fq::from_le_bytes_mod_order(blake2b_simd::blake2b(label).as_bytes())
}

fn derive_bytes(rseed: Rseed, label: &[u8]) -> [u8; 64] {
    let mut state = blake2b_simd::Params::new().hash_length(64).to_state();
    state.update(label);
    state.update(&rseed.to_bytes());
    state
        .finalize()
        .as_bytes()
        .try_into()
        .expect("configured Blake2b output is 64 bytes")
}

fn derive_opening(rseed: Rseed) -> RecoveryCapsuleOpening {
    let seed = Fq::from_le_bytes_mod_order(&derive_bytes(rseed, b"shieldd.recovery_capsule.seed"));
    let mut r =
        Fr::from_le_bytes_mod_order(&derive_bytes(rseed, b"shieldd.recovery_capsule.randomizer"));
    if r == Fr::from(0u64) {
        r = Fr::from(1u64);
    }
    RecoveryCapsuleOpening { seed, r }
}

fn derive_salt(rseed: Rseed) -> Fq {
    Fq::from_le_bytes_mod_order(&derive_bytes(rseed, b"shieldd.recovery_capsule.salt"))
}

/// The commitment embedded in a note and opened by its public recovery capsule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "[u8; 32]", into = "[u8; 32]")]
pub struct RecoveryCommitment(pub Fq);

impl RecoveryCommitment {
    /// Explicit marker for notes outside the regulated recovery path.
    pub fn unavailable() -> Self {
        static UNAVAILABLE_DOMAIN: Lazy<Fq> =
            Lazy::new(|| domain(b"shieldd.recovery_capsule.unavailable"));
        Self(poseidon377::hash_1(&UNAVAILABLE_DOMAIN, Fq::from(0u64)))
    }
}

impl TryFrom<[u8; 32]> for RecoveryCommitment {
    type Error = anyhow::Error;

    fn try_from(value: [u8; 32]) -> Result<Self> {
        Ok(Self(Fq::from_bytes_checked(&value).map_err(|_| {
            anyhow!("invalid recovery commitment field element")
        })?))
    }
}

impl From<RecoveryCommitment> for [u8; 32] {
    fn from(value: RecoveryCommitment) -> Self {
        value.0.to_bytes()
    }
}

/// Fixed-shape ciphertext under a registered capsule capability.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "pb::RecoveryCapsule", into = "pb::RecoveryCapsule")]
pub struct RecoveryCapsule {
    pub epk: Element,
    pub c2: Fq,
    pub salt: Fq,
    pub key_confirmation: Fq,
    pub encrypted_amount: Fq,
    pub encrypted_note_blinding: Fq,
}

/// Private randomness proving that a recovery capsule is encrypted to a leaf capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecoveryCapsuleOpening {
    pub seed: Fq,
    pub r: Fr,
}

/// Plaintext recovered from a capsule after Orbis releases its verified seed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecoveryPlaintext {
    pub amount: Amount,
    pub note_blinding: Fq,
}

impl RecoveryCapsule {
    pub fn encrypt(
        amount: Amount,
        note_blinding: Fq,
        capk: Element,
        rseed: Rseed,
    ) -> Result<(Self, RecoveryCapsuleOpening)> {
        ensure!(
            capk != Element::IDENTITY,
            "recovery capk must be nonidentity"
        );
        let opening = derive_opening(rseed);
        let epk = Element::GENERATOR * opening.r;
        let shared = capk * opening.r;
        let salt = derive_salt(rseed);
        let epk_fq = epk.vartime_compress_to_field();
        let capsule = Self {
            epk,
            c2: opening.seed + shared.vartime_compress_to_field(),
            salt,
            key_confirmation: poseidon377::hash_3(
                &CAPSULE_CONFIRMATION_DOMAIN,
                (opening.seed, epk_fq, salt),
            ),
            encrypted_amount: Fq::from(amount) + compliance_stream_block(opening.seed, 0),
            encrypted_note_blinding: note_blinding + compliance_stream_block(opening.seed, 1),
        };
        capsule.validate()?;
        Ok((capsule, opening))
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.epk != Element::IDENTITY,
            "recovery capsule epk must be nonidentity"
        );
        Ok(())
    }

    pub fn commitment(&self) -> RecoveryCommitment {
        RecoveryCommitment(poseidon377::hash_6(
            &CAPSULE_COMMITMENT_DOMAIN,
            (
                self.epk.vartime_compress_to_field(),
                self.c2,
                self.salt,
                self.key_confirmation,
                self.encrypted_amount,
                self.encrypted_note_blinding,
            ),
        ))
    }

    pub fn verify_opening(
        &self,
        amount: Amount,
        note_blinding: Fq,
        capk: Element,
        opening: RecoveryCapsuleOpening,
    ) -> Result<()> {
        ensure!(
            capk != Element::IDENTITY,
            "recovery capk must be nonidentity"
        );
        let epk = Element::GENERATOR * opening.r;
        let shared = capk * opening.r;
        ensure!(self.epk == epk, "recovery capsule epk opening mismatch");
        ensure!(
            self.c2 == opening.seed + shared.vartime_compress_to_field(),
            "recovery capsule seed envelope mismatch"
        );
        ensure!(
            self.key_confirmation
                == poseidon377::hash_3(
                    &CAPSULE_CONFIRMATION_DOMAIN,
                    (opening.seed, epk.vartime_compress_to_field(), self.salt),
                ),
            "recovery capsule key confirmation mismatch"
        );
        ensure!(
            self.encrypted_amount == Fq::from(amount) + compliance_stream_block(opening.seed, 0),
            "recovery capsule amount mismatch"
        );
        ensure!(
            self.encrypted_note_blinding
                == note_blinding + compliance_stream_block(opening.seed, 1),
            "recovery capsule note blinding mismatch"
        );
        Ok(())
    }

    pub fn decrypt_with_seed(&self, seed: Fq) -> Result<RecoveryPlaintext> {
        ensure!(
            self.key_confirmation
                == poseidon377::hash_3(
                    &CAPSULE_CONFIRMATION_DOMAIN,
                    (seed, self.epk.vartime_compress_to_field(), self.salt),
                ),
            "recovery capsule seed does not match key confirmation"
        );
        let amount_fq = self.encrypted_amount - compliance_stream_block(seed, 0);
        let amount_bytes = amount_fq.to_bytes();
        ensure!(
            amount_bytes[16..].iter().all(|byte| *byte == 0),
            "recovery capsule amount is outside the u128 range"
        );
        let amount = Amount::from_le_bytes(
            amount_bytes[..16]
                .try_into()
                .expect("the checked prefix is 16 bytes"),
        );
        let note_blinding = self.encrypted_note_blinding - compliance_stream_block(seed, 1);
        Ok(RecoveryPlaintext {
            amount,
            note_blinding,
        })
    }

    pub fn to_bytes(&self) -> [u8; RECOVERY_CAPSULE_BYTES] {
        let mut bytes = [0u8; RECOVERY_CAPSULE_BYTES];
        bytes[..32].copy_from_slice(&self.epk.vartime_compress().0);
        for (index, field) in [
            self.c2,
            self.salt,
            self.key_confirmation,
            self.encrypted_amount,
            self.encrypted_note_blinding,
        ]
        .into_iter()
        .enumerate()
        {
            let start = 32 * (index + 1);
            bytes[start..start + 32].copy_from_slice(&field.to_bytes());
        }
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() == RECOVERY_CAPSULE_BYTES,
            "recovery capsule must be {RECOVERY_CAPSULE_BYTES} bytes"
        );
        let epk = decaf377::Encoding(bytes[..32].try_into().context("read recovery epk")?)
            .vartime_decompress()
            .map_err(|error| anyhow!("invalid recovery capsule epk: {error:?}"))?;
        let read_fq = |index: usize| -> Result<Fq> {
            let start = 32 * index;
            let raw: [u8; 32] = bytes[start..start + 32]
                .try_into()
                .context("read recovery capsule field")?;
            Fq::from_bytes_checked(&raw)
                .map_err(|_| anyhow!("invalid canonical recovery capsule field {index}"))
        };
        let capsule = Self {
            epk,
            c2: read_fq(1)?,
            salt: read_fq(2)?,
            key_confirmation: read_fq(3)?,
            encrypted_amount: read_fq(4)?,
            encrypted_note_blinding: read_fq(5)?,
        };
        capsule.validate()?;
        Ok(capsule)
    }
}

impl DomainType for RecoveryCapsule {
    type Proto = pb::RecoveryCapsule;
}

impl TryFrom<pb::RecoveryCapsule> for RecoveryCapsule {
    type Error = anyhow::Error;

    fn try_from(value: pb::RecoveryCapsule) -> Result<Self> {
        Self::from_bytes(&value.inner)
    }
}

impl From<RecoveryCapsule> for pb::RecoveryCapsule {
    fn from(value: RecoveryCapsule) -> Self {
        Self {
            inner: value.to_bytes().to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_core::OsRng;

    #[test]
    fn capsule_roundtrip_and_opening() {
        let rseed = Rseed::generate(&mut OsRng);
        let capk = Element::GENERATOR * Fr::from(19u64);
        let amount = Amount::from(42u64);
        let blinding = Fq::from(77u64);
        let (capsule, opening) = RecoveryCapsule::encrypt(amount, blinding, capk, rseed).unwrap();

        capsule
            .verify_opening(amount, blinding, capk, opening)
            .unwrap();
        assert_eq!(
            capsule.decrypt_with_seed(opening.seed).unwrap(),
            RecoveryPlaintext {
                amount,
                note_blinding: blinding,
            }
        );
        assert_eq!(
            RecoveryCapsule::from_bytes(&capsule.to_bytes()).unwrap(),
            capsule
        );
    }

    #[test]
    fn capsule_rejects_wrong_seed_and_noncanonical_wire_fields() {
        let capk = Element::GENERATOR * Fr::from(5u64);
        let (capsule, _) =
            RecoveryCapsule::encrypt(Amount::from(9u64), Fq::from(10u64), capk, Rseed([11; 32]))
                .unwrap();
        assert!(capsule.decrypt_with_seed(Fq::from(12u64)).is_err());

        let mut bytes = capsule.to_bytes();
        bytes[32..64].fill(0xff);
        assert!(RecoveryCapsule::from_bytes(&bytes).is_err());
    }
}
