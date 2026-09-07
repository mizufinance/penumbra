use anyhow::{anyhow, Context as _, Result};
use decaf377::Fq;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use shieldd_sdk_asset::asset;
use shieldd_sdk_keys::{
    keys::{NullifierKey, OutgoingViewingKey},
    symmetric::PayloadKey,
    Address,
};
use shieldd_sdk_proto::{core::component::shielded_pool::v1 as pb, DomainType};
use shieldd_sdk_sct::Nullifier;
use shieldd_sdk_tct::{Position, Proof, StateCommitment};

pub const UTC_DAY_SECS: u64 = 86_400;
/// Post-midnight retention for proofs whose exact timestamp is still fresh.
pub const VOLUME_ACCUMULATOR_RETENTION_GRACE_SECS: u64 = 1_800;
pub const VOLUME_ACCUMULATOR_RETENTION_SECS: u64 =
    UTC_DAY_SECS + VOLUME_ACCUMULATOR_RETENTION_GRACE_SECS;
pub const VOLUME_ACCUMULATOR_PLAINTEXT_BYTES: usize = 92;
pub const VOLUME_ACCUMULATOR_CIPHERTEXT_BYTES: usize = VOLUME_ACCUMULATOR_PLAINTEXT_BYTES + 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VolumeNullifier {
    pub day_start: u64,
    pub nullifier: Nullifier,
}

static SUBJECT_DOMAIN: Lazy<Fq> = Lazy::new(|| domain(b"shieldd.volume.subject"));
static STATE_DOMAIN: Lazy<Fq> = Lazy::new(|| domain(b"shieldd.volume.state"));
static ORIGIN_NULLIFIER_DOMAIN: Lazy<Fq> = Lazy::new(|| domain(b"shieldd.volume.origin_nullifier"));
static PADDING_COMMITMENT_DOMAIN: Lazy<Fq> =
    Lazy::new(|| domain(b"shieldd.volume.padding_commitment"));
static PADDING_NULLIFIER_DOMAIN: Lazy<Fq> =
    Lazy::new(|| domain(b"shieldd.volume.padding_nullifier"));

fn domain(label: &[u8]) -> Fq {
    Fq::from_le_bytes_mod_order(blake2b_simd::blake2b(label).as_bytes())
}

/// Selects the UTC accounting day containing `target_timestamp`.
pub fn select_accumulator_day(target_timestamp: u64) -> u64 {
    target_timestamp - target_timestamp % UTC_DAY_SECS
}

/// Returns the next undisclosed volume, or `None` when this transfer must disclose.
pub fn accumulated_volume(prior: u128, outbound: u128, limit: u128) -> Option<u128> {
    prior
        .checked_add(outbound)
        .filter(|candidate| *candidate <= limit)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransferProofContext {
    Ordinary,
    FeeFunding,
}

impl TransferProofContext {
    pub fn as_field(self) -> Fq {
        Fq::from(match self {
            Self::Ordinary => 1u64,
            Self::FeeFunding => 2u64,
        })
    }
}

impl From<TransferProofContext> for i32 {
    fn from(value: TransferProofContext) -> Self {
        match value {
            TransferProofContext::Ordinary => pb::TransferProofContext::Ordinary as i32,
            TransferProofContext::FeeFunding => pb::TransferProofContext::FeeFunding as i32,
        }
    }
}

impl TryFrom<i32> for TransferProofContext {
    type Error = anyhow::Error;

    fn try_from(value: i32) -> Result<Self> {
        match pb::TransferProofContext::try_from(value) {
            Ok(pb::TransferProofContext::Ordinary) => Ok(Self::Ordinary),
            Ok(pb::TransferProofContext::FeeFunding) => Ok(Self::FeeFunding),
            _ => Err(anyhow!("missing or invalid transfer proof context")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VolumeAccumulatorState {
    pub subject: Fq,
    pub day_start: u64,
    pub undisclosed_volume: u128,
    pub blinding: Fq,
}

impl VolumeAccumulatorState {
    pub fn subject(address: &Address, asset_id: asset::Id) -> Fq {
        poseidon377::hash_3(
            &SUBJECT_DOMAIN,
            (
                address.diversified_generator().vartime_compress_to_field(),
                *address.transmission_key_s(),
                asset_id.0,
            ),
        )
    }

    pub fn commitment(&self) -> StateCommitment {
        StateCommitment(poseidon377::hash_4(
            &STATE_DOMAIN,
            (
                self.subject,
                Fq::from(self.day_start),
                Fq::from(self.undisclosed_volume),
                self.blinding,
            ),
        ))
    }

    pub fn origin_nullifier(&self, nk: &NullifierKey) -> Nullifier {
        Nullifier(poseidon377::hash_3(
            &ORIGIN_NULLIFIER_DOMAIN,
            (nk.0, self.subject, Fq::from(self.day_start)),
        ))
    }

    fn encode(&self, is_real: bool) -> [u8; VOLUME_ACCUMULATOR_PLAINTEXT_BYTES] {
        let mut out = [0u8; VOLUME_ACCUMULATOR_PLAINTEXT_BYTES];
        out[..4].copy_from_slice(&(is_real as u32).to_le_bytes());
        out[4..36].copy_from_slice(&self.subject.to_bytes());
        out[36..44].copy_from_slice(&self.day_start.to_le_bytes());
        out[44..60].copy_from_slice(&self.undisclosed_volume.to_le_bytes());
        out[60..92].copy_from_slice(&self.blinding.to_bytes());
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<(Self, bool)> {
        anyhow::ensure!(
            bytes.len() == VOLUME_ACCUMULATOR_PLAINTEXT_BYTES,
            "volume accumulator plaintext must be {VOLUME_ACCUMULATOR_PLAINTEXT_BYTES} bytes"
        );
        let marker = u32::from_le_bytes(bytes[..4].try_into()?);
        anyhow::ensure!(marker <= 1, "invalid volume accumulator payload marker");
        let subject = Fq::from_bytes_checked(&bytes[4..36].try_into()?)
            .map_err(|_| anyhow!("invalid volume accumulator subject"))?;
        let day_start = u64::from_le_bytes(bytes[36..44].try_into()?);
        let undisclosed_volume = u128::from_le_bytes(bytes[44..60].try_into()?);
        let blinding = Fq::from_bytes_checked(&bytes[60..92].try_into()?)
            .map_err(|_| anyhow!("invalid volume accumulator blinding"))?;
        Ok((
            Self {
                subject,
                day_start,
                undisclosed_volume,
                blinding,
            },
            marker == 1,
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    try_from = "pb::VolumeAccumulatorPayload",
    into = "pb::VolumeAccumulatorPayload"
)]
pub struct VolumeAccumulatorPayload {
    pub nullifier: Nullifier,
    pub commitment: StateCommitment,
    pub encrypted_state: Vec<u8>,
    pub day_start: u64,
}

impl VolumeAccumulatorPayload {
    pub fn scoped_nullifier(&self) -> VolumeNullifier {
        VolumeNullifier {
            day_start: self.day_start,
            nullifier: self.nullifier,
        }
    }
}

impl VolumeAccumulatorPayload {
    pub fn canonical_fee_funding() -> Self {
        Self {
            nullifier: Nullifier(Fq::from(0u64)),
            commitment: StateCommitment(Fq::from(0u64)),
            encrypted_state: vec![0; VOLUME_ACCUMULATOR_CIPHERTEXT_BYTES],
            day_start: 0,
        }
    }

    pub fn encrypt(
        state: &VolumeAccumulatorState,
        is_real: bool,
        nullifier: Nullifier,
        commitment: StateCommitment,
        ovk: &OutgoingViewingKey,
    ) -> Self {
        let key = PayloadKey::derive_volume_accumulator(ovk, commitment);
        Self {
            nullifier,
            commitment,
            encrypted_state: key.encrypt_volume_accumulator(state.encode(is_real).to_vec()),
            day_start: state.day_start,
        }
    }

    pub fn trial_decrypt(
        &self,
        ovk: &OutgoingViewingKey,
    ) -> Option<(VolumeAccumulatorState, bool)> {
        if self.encrypted_state.len() != VOLUME_ACCUMULATOR_CIPHERTEXT_BYTES {
            return None;
        }
        let key = PayloadKey::derive_volume_accumulator(ovk, self.commitment);
        let plaintext = key
            .decrypt_volume_accumulator(self.encrypted_state.clone())
            .ok()?;
        let (state, is_real) = VolumeAccumulatorState::decode(&plaintext).ok()?;
        if state.day_start != self.day_start {
            return None;
        }
        if is_real && state.commitment() != self.commitment {
            return None;
        }
        Some((state, is_real))
    }

    pub fn validate(&self, context: TransferProofContext) -> Result<()> {
        anyhow::ensure!(
            self.encrypted_state.len() == VOLUME_ACCUMULATOR_CIPHERTEXT_BYTES,
            "volume accumulator ciphertext must be {VOLUME_ACCUMULATOR_CIPHERTEXT_BYTES} bytes"
        );
        match context {
            TransferProofContext::FeeFunding => anyhow::ensure!(
                self == &Self::canonical_fee_funding(),
                "fee funding must use the canonical disabled volume accumulator slot"
            ),
            TransferProofContext::Ordinary => anyhow::ensure!(
                self.day_start % UTC_DAY_SECS == 0,
                "volume accumulator day must be UTC aligned"
            ),
        }
        Ok(())
    }
}

impl DomainType for VolumeAccumulatorPayload {
    type Proto = pb::VolumeAccumulatorPayload;
}

impl From<VolumeAccumulatorPayload> for pb::VolumeAccumulatorPayload {
    fn from(value: VolumeAccumulatorPayload) -> Self {
        Self {
            nullifier: Some(value.nullifier.into()),
            commitment: Some(value.commitment.into()),
            encrypted_state: value.encrypted_state,
            day_start: value.day_start,
        }
    }
}

impl TryFrom<pb::VolumeAccumulatorPayload> for VolumeAccumulatorPayload {
    type Error = anyhow::Error;

    fn try_from(value: pb::VolumeAccumulatorPayload) -> Result<Self> {
        let payload = Self {
            nullifier: value
                .nullifier
                .ok_or_else(|| anyhow!("missing volume accumulator nullifier"))?
                .try_into()?,
            commitment: value
                .commitment
                .ok_or_else(|| anyhow!("missing volume accumulator commitment"))?
                .try_into()?,
            encrypted_state: value.encrypted_state,
            day_start: value.day_start,
        };
        anyhow::ensure!(
            payload.encrypted_state.len() == VOLUME_ACCUMULATOR_CIPHERTEXT_BYTES,
            "invalid volume accumulator ciphertext length"
        );
        Ok(payload)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    try_from = "pb::VolumeAccumulatorPlan",
    into = "pb::VolumeAccumulatorPlan"
)]
pub enum VolumeAccumulatorPlan {
    Padding {
        day_start: u64,
    },
    Origin {
        successor: VolumeAccumulatorState,
    },
    Continuation {
        prior: VolumeAccumulatorState,
        prior_commitment: StateCommitment,
        prior_position: u64,
        successor_volume: u128,
        successor_blinding: Fq,
    },
}

impl VolumeAccumulatorPlan {
    pub fn padding(target_timestamp: u64) -> Self {
        Self::Padding {
            day_start: select_accumulator_day(target_timestamp),
        }
    }

    pub fn origin(successor: VolumeAccumulatorState) -> Self {
        Self::Origin { successor }
    }

    pub fn continuation(
        prior: VolumeAccumulatorState,
        prior_commitment: StateCommitment,
        prior_position: u64,
        successor_volume: u128,
        successor_blinding: Fq,
    ) -> Result<Self> {
        anyhow::ensure!(
            prior.commitment() == prior_commitment,
            "prior volume accumulator state does not match commitment"
        );
        anyhow::ensure!(
            successor_volume >= prior.undisclosed_volume,
            "volume accumulator cannot move backward"
        );
        Ok(Self::Continuation {
            prior,
            prior_commitment,
            prior_position,
            successor_volume,
            successor_blinding,
        })
    }

    pub fn is_real(&self) -> bool {
        !matches!(self, Self::Padding { .. })
    }

    pub fn starts_new_day(&self) -> bool {
        matches!(self, Self::Origin { .. })
    }

    pub fn day_start(&self) -> u64 {
        match self {
            Self::Padding { day_start } => *day_start,
            Self::Origin { successor } => successor.day_start,
            Self::Continuation { prior, .. } => prior.day_start,
        }
    }

    pub fn prior_state(&self) -> Option<&VolumeAccumulatorState> {
        match self {
            Self::Continuation { prior, .. } => Some(prior),
            Self::Padding { .. } | Self::Origin { .. } => None,
        }
    }

    pub fn prior_commitment(&self) -> StateCommitment {
        match self {
            Self::Continuation {
                prior_commitment, ..
            } => *prior_commitment,
            Self::Padding { .. } | Self::Origin { .. } => StateCommitment(Fq::from(0u64)),
        }
    }

    pub fn prior_position(&self) -> u64 {
        match self {
            Self::Continuation { prior_position, .. } => *prior_position,
            Self::Padding { .. } | Self::Origin { .. } => 0,
        }
    }

    pub fn successor_state(&self) -> Option<VolumeAccumulatorState> {
        match self {
            Self::Padding { .. } => None,
            Self::Origin { successor } => Some(successor.clone()),
            Self::Continuation {
                prior,
                successor_volume,
                successor_blinding,
                ..
            } => Some(VolumeAccumulatorState {
                subject: prior.subject,
                day_start: prior.day_start,
                undisclosed_volume: *successor_volume,
                blinding: *successor_blinding,
            }),
        }
    }

    pub fn selected_payload(
        &self,
        nk: &NullifierKey,
        ovk: &OutgoingViewingKey,
        padding_seed: Fq,
        context: TransferProofContext,
    ) -> VolumeAccumulatorPayload {
        if context == TransferProofContext::FeeFunding {
            return VolumeAccumulatorPayload::canonical_fee_funding();
        }
        if let Some(state) = self.successor_state() {
            let commitment = state.commitment();
            let nullifier = match self {
                Self::Origin { .. } => state.origin_nullifier(nk),
                Self::Continuation {
                    prior_commitment,
                    prior_position,
                    ..
                } => Nullifier::derive(nk, Position::from(*prior_position), prior_commitment),
                Self::Padding { .. } => unreachable!("real accumulator state excludes padding"),
            };
            VolumeAccumulatorPayload::encrypt(&state, true, nullifier, commitment, ovk)
        } else {
            let day_start = self.day_start();
            let commitment = StateCommitment(poseidon377::hash_3(
                &PADDING_COMMITMENT_DOMAIN,
                (nk.0, padding_seed, Fq::from(day_start)),
            ));
            let nullifier = Nullifier(poseidon377::hash_3(
                &PADDING_NULLIFIER_DOMAIN,
                (nk.0, padding_seed, Fq::from(day_start)),
            ));
            let state = VolumeAccumulatorState {
                subject: Fq::from(0u64),
                day_start,
                undisclosed_volume: 0,
                blinding: padding_seed,
            };
            VolumeAccumulatorPayload::encrypt(&state, false, nullifier, commitment, ovk)
        }
    }
}

impl DomainType for VolumeAccumulatorPlan {
    type Proto = pb::VolumeAccumulatorPlan;
}

impl From<VolumeAccumulatorPlan> for pb::VolumeAccumulatorPlan {
    fn from(value: VolumeAccumulatorPlan) -> Self {
        let use_real = value.is_real();
        let starts_new_day = value.starts_new_day();
        let day_start = value.day_start();
        let prior = value
            .prior_state()
            .cloned()
            .unwrap_or(VolumeAccumulatorState {
                subject: Fq::from(0u64),
                day_start,
                undisclosed_volume: 0,
                blinding: Fq::from(0u64),
            });
        let successor = value.successor_state().unwrap_or(VolumeAccumulatorState {
            subject: Fq::from(0u64),
            day_start,
            undisclosed_volume: 0,
            blinding: Fq::from(0u64),
        });
        Self {
            use_real,
            starts_new_day,
            day_start,
            subject: successor.subject.to_bytes().to_vec(),
            prior_volume: prior.undisclosed_volume.to_le_bytes().to_vec(),
            prior_blinding: prior.blinding.to_bytes().to_vec(),
            prior_commitment: Some(value.prior_commitment().into()),
            prior_position: value.prior_position(),
            successor_volume: successor.undisclosed_volume.to_le_bytes().to_vec(),
            successor_blinding: successor.blinding.to_bytes().to_vec(),
        }
    }
}

fn parse_fq(bytes: Vec<u8>, label: &str) -> Result<Fq> {
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow!("{label} must be 32 bytes"))?;
    Fq::from_bytes_checked(&bytes).map_err(|_| anyhow!("{label} must be canonical"))
}

fn parse_u128(bytes: Vec<u8>, label: &str) -> Result<u128> {
    Ok(u128::from_le_bytes(
        bytes
            .try_into()
            .map_err(|_| anyhow!("{label} must be 16 bytes"))?,
    ))
}

impl TryFrom<pb::VolumeAccumulatorPlan> for VolumeAccumulatorPlan {
    type Error = anyhow::Error;

    fn try_from(value: pb::VolumeAccumulatorPlan) -> Result<Self> {
        let subject = parse_fq(value.subject, "volume accumulator subject")?;
        let prior_volume = parse_u128(value.prior_volume, "prior volume")?;
        let prior_blinding = parse_fq(value.prior_blinding, "prior blinding")?;
        let prior_commitment = value
            .prior_commitment
            .ok_or_else(|| anyhow!("missing prior commitment"))?
            .try_into()
            .context("invalid prior commitment")?;
        let successor_volume = parse_u128(value.successor_volume, "successor volume")?;
        let successor_blinding = parse_fq(value.successor_blinding, "successor blinding")?;
        anyhow::ensure!(
            value.day_start % UTC_DAY_SECS == 0,
            "volume accumulator day must be UTC aligned"
        );
        if !value.use_real {
            anyhow::ensure!(
                !value.starts_new_day
                    && subject == Fq::from(0u64)
                    && prior_volume == 0
                    && prior_blinding == Fq::from(0u64)
                    && prior_commitment == StateCommitment(Fq::from(0u64))
                    && value.prior_position == 0
                    && successor_volume == 0
                    && successor_blinding == Fq::from(0u64),
                "padding accumulator plan must use canonical empty state"
            );
            return Ok(Self::Padding {
                day_start: value.day_start,
            });
        }
        anyhow::ensure!(
            successor_volume >= prior_volume,
            "volume accumulator cannot move backward"
        );
        if value.starts_new_day {
            anyhow::ensure!(
                prior_volume == 0
                    && prior_blinding == Fq::from(0u64)
                    && prior_commitment == StateCommitment(Fq::from(0u64))
                    && value.prior_position == 0,
                "origin accumulator plan must not carry prior state"
            );
            return Ok(Self::Origin {
                successor: VolumeAccumulatorState {
                    subject,
                    day_start: value.day_start,
                    undisclosed_volume: successor_volume,
                    blinding: successor_blinding,
                },
            });
        }
        Self::continuation(
            VolumeAccumulatorState {
                subject,
                day_start: value.day_start,
                undisclosed_volume: prior_volume,
                blinding: prior_blinding,
            },
            prior_commitment,
            value.prior_position,
            successor_volume,
            successor_blinding,
        )
    }
}

#[derive(Clone, Debug)]
pub struct VolumeAccumulatorPrivate {
    pub plan: VolumeAccumulatorPlan,
    pub prior_proof: Proof,
}

#[derive(Clone, Debug)]
pub struct VolumeAccumulatorPublic {
    pub nullifier: Nullifier,
    pub commitment: StateCommitment,
    pub day_start: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use shieldd_sdk_keys::test_keys;

    #[test]
    fn accounting_days_change_only_at_utc_midnight() {
        assert_eq!(select_accumulator_day(0), 0);
        assert_eq!(select_accumulator_day(86_399), 0);
        assert_eq!(select_accumulator_day(86_400), 86_400);
        assert_eq!(select_accumulator_day(172_799), 86_400);
    }

    #[test]
    fn accumulation_is_inclusive_at_the_limit_and_discloses_overflow() {
        assert_eq!(accumulated_volume(40, 60, 100), Some(100));
        assert_eq!(accumulated_volume(100, 1, 100), None);
        assert_eq!(accumulated_volume(u128::MAX, 1, u128::MAX), None);

        // A disclosed over-limit action does not consume the confirmed head,
        // so a later smaller action can still continue from the same volume.
        assert_eq!(accumulated_volume(80, 30, 100), None);
        assert_eq!(accumulated_volume(80, 20, 100), Some(100));
    }

    #[test]
    fn encrypted_real_state_round_trips_and_binds_commitment() {
        let state = VolumeAccumulatorState {
            subject: Fq::from(7u64),
            day_start: 86_400,
            undisclosed_volume: 42,
            blinding: Fq::from(9u64),
        };
        let payload = VolumeAccumulatorPayload::encrypt(
            &state,
            true,
            Nullifier(Fq::from(11u64)),
            state.commitment(),
            test_keys::FULL_VIEWING_KEY.outgoing(),
        );
        assert_eq!(
            payload.trial_decrypt(test_keys::FULL_VIEWING_KEY.outgoing()),
            Some((state, true))
        );
    }

    #[test]
    fn accumulator_plan_proto_round_trips_each_canonical_state() {
        let padding = VolumeAccumulatorPlan::padding(86_400);
        assert_eq!(
            VolumeAccumulatorPlan::try_from(pb::VolumeAccumulatorPlan::from(padding.clone()))
                .expect("padding plan should roundtrip"),
            padding
        );

        let origin_state = VolumeAccumulatorState {
            subject: Fq::from(7u64),
            day_start: 86_400,
            undisclosed_volume: 42,
            blinding: Fq::from(9u64),
        };
        let origin = VolumeAccumulatorPlan::origin(origin_state.clone());
        assert_eq!(
            VolumeAccumulatorPlan::try_from(pb::VolumeAccumulatorPlan::from(origin.clone()))
                .expect("origin plan should roundtrip"),
            origin
        );

        let continuation = VolumeAccumulatorPlan::continuation(
            origin_state.clone(),
            origin_state.commitment(),
            11,
            50,
            Fq::from(12u64),
        )
        .expect("continuation plan should be valid");
        assert_eq!(
            VolumeAccumulatorPlan::try_from(pb::VolumeAccumulatorPlan::from(continuation.clone()))
                .expect("continuation plan should roundtrip"),
            continuation
        );
    }

    #[test]
    fn accumulator_plan_proto_rejects_noncanonical_padding_flags() {
        let mut proto = pb::VolumeAccumulatorPlan::from(VolumeAccumulatorPlan::padding(86_400));
        proto.starts_new_day = true;
        let error = VolumeAccumulatorPlan::try_from(proto)
            .expect_err("padding must not claim to start a real accumulator");
        assert!(error
            .to_string()
            .contains("padding accumulator plan must use canonical empty state"));
    }
}
