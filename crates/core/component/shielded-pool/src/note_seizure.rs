use anyhow::{ensure, Context, Result};
use ark_groth16::{r1cs_to_qap::LibsnarkReduction, Groth16, PreparedVerifyingKey, Proof};
use ark_serialize::CanonicalSerialize;
use ark_snark::SNARK;
use decaf377::{Bls12_377, Element, Fq, Fr};
use decaf377_rdsa::{Signature, SpendAuth, VerificationKey};
use once_cell::sync::Lazy;
use shieldd_sdk_asset::{asset, Value};
use shieldd_sdk_compliance::structs::MAX_CERTIFICATE_TEXT_BYTES;
use shieldd_sdk_compliance::{fq_to_challenge_scalar, verify_dleq, DleqProof};
use shieldd_sdk_keys::Address;
use shieldd_sdk_num::Amount;
use shieldd_sdk_proto::{core::component::shielded_pool::v1 as pb, DomainType};
use shieldd_sdk_sct::{
    nullifier_generation::{HistoricalNullifierProof, NullifierWindow},
    Nullifier,
};
use shieldd_sdk_tct as tct;

use crate::{
    public_input_hash::{note_seizure_statement_hash_from_public, StatementHashError},
    HostWithdrawal, RecoveryCapsule, RecoveryCommitment,
};

pub const NOTE_SEIZURE_PROOF_LABEL: &str = "note_seizure";
pub const NOTE_SEIZURE_STATEMENT_FIELD_COUNT: usize = 19;
pub const MAX_NOTE_SEIZURE_CHAIN_ID_BYTES: usize = 128;
const NOTE_SEIZURE_AUTHORIZATION_DOMAIN: &[u8] = b"shieldd.note_seizure.authorization";
const NOTE_SEIZURE_AUTHORIZATION_COMMITMENT_DOMAIN: &[u8] =
    b"shieldd.note_seizure.authorization_commitment";
const CAPSULE_RELEASE_ID_DOMAIN: &[u8] = b"shieldd.capsule_release.request.v1";
static CAPSULE_RELEASE_DLEQ_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"shieldd.capsule_release.dleq.v1").as_bytes(),
    )
});

/// Immutable facts approved by the asset's seizure authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteSeizureAuthorizationBody {
    pub chain_id: String,
    pub note_commitment: tct::StateCommitment,
    pub nullifier: Nullifier,
    pub address: Address,
    pub asset_id: asset::Id,
    pub amount: Amount,
    pub freeze_generation: u64,
    pub frozen_since_height: u64,
    pub withdrawal: HostWithdrawal,
    pub expiry_height: u64,
}

impl NoteSeizureAuthorizationBody {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.chain_id.is_empty() && self.chain_id.len() <= MAX_NOTE_SEIZURE_CHAIN_ID_BYTES,
            "invalid note seizure chain_id length"
        );
        ensure!(
            self.amount != Amount::zero(),
            "note seizure amount must be nonzero"
        );
        ensure!(
            self.freeze_generation > 0,
            "note seizure freeze generation must be nonzero"
        );
        ensure!(
            self.frozen_since_height > 0,
            "note seizure frozen-since height must be nonzero"
        );
        ensure!(
            self.expiry_height >= self.frozen_since_height,
            "note seizure authorization expires before the freeze"
        );
        self.withdrawal.validate()?;
        ensure!(
            self.withdrawal.value
                == Value {
                    amount: self.amount,
                    asset_id: self.asset_id,
                },
            "note seizure withdrawal must exactly match the seized note value"
        );
        Ok(())
    }

    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let body = self.encode_to_vec();
        let mut bytes =
            Vec::with_capacity(NOTE_SEIZURE_AUTHORIZATION_DOMAIN.len() + 1 + body.len());
        bytes.extend_from_slice(NOTE_SEIZURE_AUTHORIZATION_DOMAIN);
        bytes.push(0);
        bytes.extend_from_slice(&body);
        Ok(bytes)
    }

    pub fn verify_signature(
        &self,
        authority: &VerificationKey<SpendAuth>,
        signature: &Signature<SpendAuth>,
    ) -> Result<()> {
        authority
            .verify(&self.signing_bytes()?, signature)
            .context("invalid note seizure authority signature")
    }

    pub fn commitment(&self) -> Result<Fq> {
        self.validate()?;
        let mut state = blake2b_simd::Params::new().hash_length(64).to_state();
        state.update(NOTE_SEIZURE_AUTHORIZATION_COMMITMENT_DOMAIN);
        state.update(&[0]);
        state.update(&self.encode_to_vec());
        Ok(Fq::from_le_bytes_mod_order(state.finalize().as_bytes()))
    }
}

impl DomainType for NoteSeizureAuthorizationBody {
    type Proto = pb::NoteSeizureAuthorizationBody;
}

impl TryFrom<pb::NoteSeizureAuthorizationBody> for NoteSeizureAuthorizationBody {
    type Error = anyhow::Error;

    fn try_from(value: pb::NoteSeizureAuthorizationBody) -> Result<Self> {
        let body = Self {
            chain_id: value.chain_id,
            note_commitment: value
                .note_commitment
                .context("note seizure authorization is missing note commitment")?
                .try_into()
                .context("invalid note seizure note commitment")?,
            nullifier: value
                .nullifier
                .context("note seizure authorization is missing nullifier")?
                .try_into()
                .context("invalid note seizure nullifier")?,
            address: value
                .address
                .context("note seizure authorization is missing address")?
                .try_into()
                .context("invalid note seizure address")?,
            asset_id: value
                .asset_id
                .context("note seizure authorization is missing asset ID")?
                .try_into()
                .context("invalid note seizure asset ID")?,
            amount: value
                .amount
                .context("note seizure authorization is missing amount")?
                .try_into()
                .context("invalid note seizure amount")?,
            freeze_generation: value.freeze_generation,
            frozen_since_height: value.frozen_since_height,
            withdrawal: value
                .withdrawal
                .context("note seizure authorization is missing withdrawal")?
                .try_into()
                .context("invalid note seizure withdrawal")?,
            expiry_height: value.expiry_height,
        };
        body.validate()?;
        Ok(body)
    }
}

impl From<NoteSeizureAuthorizationBody> for pb::NoteSeizureAuthorizationBody {
    fn from(value: NoteSeizureAuthorizationBody) -> Self {
        Self {
            chain_id: value.chain_id,
            note_commitment: Some(value.note_commitment.into()),
            nullifier: Some(value.nullifier.into()),
            address: Some(value.address.into()),
            asset_id: Some(value.asset_id.into()),
            amount: Some(value.amount.into()),
            freeze_generation: value.freeze_generation,
            frozen_since_height: value.frozen_since_height,
            withdrawal: Some(value.withdrawal.into()),
            expiry_height: value.expiry_height,
        }
    }
}

/// Capsule-specific resource identity for future ACP authorization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapsuleReleaseRequest {
    pub chain_id: String,
    pub ring_id: String,
    pub policy_id: String,
    pub permission: String,
    pub resource: String,
    pub ring_pk: Element,
    pub asset_id: asset::Id,
    pub address: Address,
    pub capk: Element,
    pub note_commitment: tct::StateCommitment,
    pub recovery_commitment: RecoveryCommitment,
    pub capsule_epk: Element,
    pub authority_instruction_commitment: Fq,
    pub expiry_height: u64,
}

impl CapsuleReleaseRequest {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.chain_id.is_empty() && self.chain_id.len() <= MAX_NOTE_SEIZURE_CHAIN_ID_BYTES,
            "invalid capsule release chain_id length"
        );
        for (label, value) in [
            ("ring_id", self.ring_id.as_str()),
            ("policy_id", self.policy_id.as_str()),
            ("permission", self.permission.as_str()),
            ("resource", self.resource.as_str()),
        ] {
            ensure!(
                !value.is_empty() && value.len() <= MAX_CERTIFICATE_TEXT_BYTES,
                "invalid capsule release {label} length"
            );
        }
        ensure!(
            !self.ring_pk.is_identity(),
            "capsule release ring_pk must not be identity"
        );
        ensure!(
            !self.capk.is_identity(),
            "capsule release capk must not be identity"
        );
        ensure!(
            !self.capsule_epk.is_identity(),
            "capsule release EPK must not be identity"
        );
        ensure!(
            self.authority_instruction_commitment != Fq::from(0u64),
            "capsule release authority commitment must be nonzero"
        );
        ensure!(
            self.expiry_height > 0,
            "capsule release expiry must be nonzero"
        );
        Ok(())
    }

    pub fn release_id(&self) -> Result<[u8; 32]> {
        self.validate()?;
        let mut state = blake2b_simd::Params::new().hash_length(64).to_state();
        state.update(CAPSULE_RELEASE_ID_DOMAIN);
        state.update(&[0]);
        state.update(&self.encode_to_vec());
        Ok(Fq::from_le_bytes_mod_order(state.finalize().as_bytes()).to_bytes())
    }
}

impl DomainType for CapsuleReleaseRequest {
    type Proto = pb::CapsuleReleaseRequest;
}

impl TryFrom<pb::CapsuleReleaseRequest> for CapsuleReleaseRequest {
    type Error = anyhow::Error;

    fn try_from(value: pb::CapsuleReleaseRequest) -> Result<Self> {
        let request = Self {
            chain_id: value.chain_id,
            ring_id: value.ring_id,
            policy_id: value.policy_id,
            permission: value.permission,
            resource: value.resource,
            ring_pk: decode_element(value.ring_pk, "capsule release ring_pk")?,
            asset_id: value
                .asset_id
                .context("capsule release is missing asset ID")?
                .try_into()?,
            address: value
                .address
                .context("capsule release is missing address")?
                .try_into()?,
            capk: decode_element(value.capk, "capsule release capk")?,
            note_commitment: value
                .note_commitment
                .context("capsule release is missing note commitment")?
                .try_into()?,
            recovery_commitment: RecoveryCommitment(decode_fq(
                value.recovery_commitment,
                "capsule release recovery commitment",
            )?),
            capsule_epk: decode_element(value.capsule_epk, "capsule release EPK")?,
            authority_instruction_commitment: decode_fq(
                value.authority_instruction_commitment,
                "capsule release authority commitment",
            )?,
            expiry_height: value.expiry_height,
        };
        request.validate()?;
        Ok(request)
    }
}

impl From<CapsuleReleaseRequest> for pb::CapsuleReleaseRequest {
    fn from(value: CapsuleReleaseRequest) -> Self {
        Self {
            chain_id: value.chain_id,
            ring_id: value.ring_id,
            policy_id: value.policy_id,
            permission: value.permission,
            resource: value.resource,
            ring_pk: value.ring_pk.vartime_compress().0.to_vec(),
            asset_id: Some(value.asset_id.into()),
            address: Some(value.address.into()),
            capk: value.capk.vartime_compress().0.to_vec(),
            note_commitment: Some(value.note_commitment.into()),
            recovery_commitment: value.recovery_commitment.0.to_bytes().to_vec(),
            capsule_epk: value.capsule_epk.vartime_compress().0.to_vec(),
            authority_instruction_commitment: value
                .authority_instruction_commitment
                .to_bytes()
                .to_vec(),
            expiry_height: value.expiry_height,
        }
    }
}

/// Verifiable opening of one capsule, not evidence of an ACP grant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapsuleReleaseEvidence {
    pub release_id: [u8; 32],
    pub recovered_point: Element,
    pub proof: DleqProof,
}

impl CapsuleReleaseEvidence {
    pub fn verify(&self, request: &CapsuleReleaseRequest) -> Result<Element> {
        ensure!(
            self.release_id == request.release_id()?,
            "capsule release ID mismatch"
        );
        ensure!(
            !self.recovered_point.is_identity(),
            "capsule recovered point must not be identity"
        );
        ensure!(
            !self.proof.commitment_g.is_identity(),
            "capsule DLEQ G commitment must not be identity"
        );
        ensure!(
            !self.proof.commitment_h.is_identity(),
            "capsule DLEQ EPK commitment must not be identity"
        );
        let challenge = capsule_release_challenge(request, self);
        verify_dleq(
            Element::GENERATOR,
            request.capsule_epk,
            request.capk,
            self.recovered_point,
            &self.proof,
            challenge,
        )?;
        Ok(self.recovered_point)
    }

    #[cfg(any(test, feature = "benchmark-helpers"))]
    /// Insecure local fixture: never use its fixed nonce with real capability keys.
    pub fn from_capability_secret_for_test(
        request: &CapsuleReleaseRequest,
        capability_secret: Fr,
    ) -> Self {
        assert_eq!(request.capk, Element::GENERATOR * capability_secret);
        let nonce = Fr::from(29u64);
        let mut evidence = Self {
            release_id: request
                .release_id()
                .expect("test release request must be valid"),
            recovered_point: request.capsule_epk * capability_secret,
            proof: DleqProof {
                commitment_g: Element::GENERATOR * nonce,
                commitment_h: request.capsule_epk * nonce,
                response: Fr::from(0u64),
            },
        };
        evidence.proof.response =
            nonce + capsule_release_challenge(request, &evidence) * capability_secret;
        evidence
    }
}

fn capsule_release_challenge(
    request: &CapsuleReleaseRequest,
    evidence: &CapsuleReleaseEvidence,
) -> Fr {
    let release_id = Fq::from_le_bytes_mod_order(&evidence.release_id);
    fq_to_challenge_scalar(poseidon377::hash_7(
        &CAPSULE_RELEASE_DLEQ_DOMAIN,
        (
            release_id,
            Element::GENERATOR.vartime_compress_to_field(),
            request.capk.vartime_compress_to_field(),
            request.capsule_epk.vartime_compress_to_field(),
            evidence.recovered_point.vartime_compress_to_field(),
            evidence.proof.commitment_g.vartime_compress_to_field(),
            evidence.proof.commitment_h.vartime_compress_to_field(),
        ),
    ))
}

/// Public statement opened by one note-seizure proof.
#[derive(Clone, Debug)]
pub struct NoteSeizureProofPublic {
    pub authorization: NoteSeizureAuthorizationBody,
    pub anchor: tct::Root,
    pub history_required: bool,
    pub recent_position_floor: u64,
    pub recovery_capsule: RecoveryCapsule,
    pub recovery_seed: Fq,
    pub rnk_commitment: Fq,
}

impl NoteSeizureProofPublic {
    pub fn statement_hash(&self) -> Result<Fq, StatementHashError> {
        note_seizure_statement_hash_from_public(self)
    }
}

/// Private opening proving that the authorization consumes the real SCT note.
#[derive(Clone, Debug)]
pub struct NoteSeizureProofPrivate {
    pub note_blinding: Fq,
    pub state_commitment_proof: tct::Proof,
    pub rnk: Fq,
}

impl NoteSeizureProofPrivate {
    pub fn validate_against(&self, public: &NoteSeizureProofPublic) -> Result<()> {
        let note_commitment = crate::note::commitment_from_address(
            public.authorization.address.clone(),
            Value {
                amount: public.authorization.amount,
                asset_id: public.authorization.asset_id,
            },
            self.note_blinding,
            public.recovery_capsule.commitment(),
        )?;
        ensure!(
            note_commitment == public.authorization.note_commitment,
            "note seizure witness note commitment mismatch"
        );
        ensure!(
            self.state_commitment_proof.commitment() == note_commitment,
            "note seizure SCT proof commitment mismatch"
        );
        ensure!(
            self.state_commitment_proof.root() == public.anchor,
            "note seizure SCT proof anchor mismatch"
        );
        ensure!(
            shieldd_sdk_compliance::compliance_nullifier_key_commitment(self.rnk)
                == public.rnk_commitment,
            "note seizure RNK commitment mismatch"
        );
        ensure!(
            Nullifier::derive(
                &shieldd_sdk_keys::keys::NullifierKey(self.rnk),
                self.state_commitment_proof.position(),
                &note_commitment,
            ) == public.authorization.nullifier,
            "note seizure canonical nullifier mismatch"
        );
        ensure!(
            public.history_required
                == (u64::from(self.state_commitment_proof.position())
                    < public.recent_position_floor),
            "note seizure history classification mismatch"
        );
        let plaintext = public
            .recovery_capsule
            .decrypt_with_seed(public.recovery_seed)?;
        ensure!(
            plaintext.amount == public.authorization.amount
                && plaintext.note_blinding == self.note_blinding,
            "note seizure recovery plaintext mismatch"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct NoteSeizureProof {
    pub inner: Vec<u8>,
}

impl NoteSeizureProof {
    fn decoded_proof(&self) -> Result<Proof<Bls12_377>> {
        crate::groth16_proof::decode(&self.inner)
    }

    pub fn verify_with_prepared_vk(
        &self,
        public: &NoteSeizureProofPublic,
        vk: &PreparedVerifyingKey<Bls12_377>,
    ) -> Result<()> {
        let proof = self.decoded_proof()?;
        let statement_hash = public.statement_hash()?;
        let verified = Groth16::<Bls12_377, LibsnarkReduction>::verify_with_processed_vk(
            vk,
            &[statement_hash],
            &proof,
        )?;
        ensure!(verified, "{NOTE_SEIZURE_PROOF_LABEL} proof did not verify");
        Ok(())
    }

    pub fn verify(&self, public: &NoteSeizureProofPublic) -> Result<()> {
        self.verify_with_prepared_vk(
            public,
            shieldd_sdk_proof_params::note_seizure_proof_verification_key(),
        )
    }

    pub fn validate_encoding(&self) -> Result<()> {
        self.decoded_proof()?;
        Ok(())
    }

    pub fn compressed_bytes(&self) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        self.decoded_proof()?.serialize_compressed(&mut bytes)?;
        Ok(bytes)
    }
}

impl DomainType for NoteSeizureProof {
    type Proto = pb::ZkNoteSeizureProof;
}

impl From<NoteSeizureProof> for pb::ZkNoteSeizureProof {
    fn from(value: NoteSeizureProof) -> Self {
        Self { inner: value.inner }
    }
}

impl TryFrom<pb::ZkNoteSeizureProof> for NoteSeizureProof {
    type Error = anyhow::Error;

    fn try_from(value: pb::ZkNoteSeizureProof) -> Result<Self> {
        let proof = Self { inner: value.inner };
        proof.validate_encoding()?;
        Ok(proof)
    }
}

/// Complete native and zero-knowledge evidence for consuming one frozen note.
#[derive(Clone, Debug)]
pub struct NoteSeizure {
    pub authorization: NoteSeizureAuthorizationBody,
    pub authority_signature: Signature<SpendAuth>,
    pub anchor: tct::Root,
    pub history_required: bool,
    pub recent_position_floor: u64,
    pub recovery_capsule: RecoveryCapsule,
    pub rnk_commitment: Fq,
    pub capsule_release: CapsuleReleaseEvidence,
    pub proof: NoteSeizureProof,
    pub nullifier_window: NullifierWindow,
    pub historical_nullifier_proof: Option<HistoricalNullifierProof>,
}

impl NoteSeizure {
    pub fn validate(&self) -> Result<()> {
        self.authorization.validate()?;
        self.recovery_capsule.validate()?;
        self.nullifier_window.validate()?;
        ensure!(
            self.recent_position_floor == self.nullifier_window.recent_position_floor,
            "note seizure recent position floor differs from its nullifier window"
        );
        match (self.history_required, &self.historical_nullifier_proof) {
            (true, Some(proof)) => {
                ensure!(
                    proof.nullifier == self.authorization.nullifier,
                    "historical proof nullifier differs from note seizure nullifier"
                );
                proof.validate_structure(self.nullifier_window)?;
            }
            (true, None) => anyhow::bail!("old note seizure requires a historical proof"),
            (false, Some(_)) => {
                anyhow::bail!("recent note seizure must not include a historical proof")
            }
            (false, None) => {}
        }
        Ok(())
    }

    pub fn proof_public(&self, recovery_seed: Fq) -> NoteSeizureProofPublic {
        NoteSeizureProofPublic {
            authorization: self.authorization.clone(),
            anchor: self.anchor,
            history_required: self.history_required,
            recent_position_floor: self.recent_position_floor,
            recovery_capsule: self.recovery_capsule.clone(),
            recovery_seed,
            rnk_commitment: self.rnk_commitment,
        }
    }
}

impl DomainType for NoteSeizure {
    type Proto = pb::NoteSeizure;
}

impl TryFrom<pb::NoteSeizure> for NoteSeizure {
    type Error = anyhow::Error;

    fn try_from(value: pb::NoteSeizure) -> Result<Self> {
        let seizure = Self {
            authorization: value
                .authorization
                .context("note seizure is missing authorization")?
                .try_into()?,
            authority_signature: value
                .authority_signature
                .context("note seizure is missing authority signature")?
                .try_into()
                .context("invalid note seizure authority signature encoding")?,
            anchor: value
                .anchor
                .context("note seizure is missing anchor")?
                .try_into()
                .context("invalid note seizure anchor")?,
            history_required: value.history_required,
            recent_position_floor: value.recent_position_floor,
            recovery_capsule: value
                .recovery_capsule
                .context("note seizure is missing recovery capsule")?
                .try_into()?,
            rnk_commitment: decode_fq(value.rnk_commitment, "note seizure RNK commitment")?,
            capsule_release: value
                .capsule_release
                .context("note seizure is missing capsule release evidence")?
                .try_into()?,
            proof: value
                .proof
                .context("note seizure is missing ZK proof")?
                .try_into()?,
            nullifier_window: value
                .nullifier_window
                .context("note seizure is missing nullifier window")?
                .try_into()?,
            historical_nullifier_proof: value
                .historical_nullifier_proof
                .map(TryInto::try_into)
                .transpose()?,
        };
        seizure.validate()?;
        Ok(seizure)
    }
}

impl From<NoteSeizure> for pb::NoteSeizure {
    fn from(value: NoteSeizure) -> Self {
        Self {
            authorization: Some(value.authorization.into()),
            authority_signature: Some(value.authority_signature.into()),
            anchor: Some(value.anchor.into()),
            history_required: value.history_required,
            recent_position_floor: value.recent_position_floor,
            recovery_capsule: Some(value.recovery_capsule.into()),
            rnk_commitment: value.rnk_commitment.to_bytes().to_vec(),
            proof: Some(value.proof.into()),
            nullifier_window: Some(value.nullifier_window.into()),
            historical_nullifier_proof: value.historical_nullifier_proof.map(Into::into),
            capsule_release: Some(value.capsule_release.into()),
        }
    }
}

impl TryFrom<pb::CapsuleReleaseEvidence> for CapsuleReleaseEvidence {
    type Error = anyhow::Error;

    fn try_from(value: pb::CapsuleReleaseEvidence) -> Result<Self> {
        let proof = value
            .proof
            .context("capsule release is missing DLEQ proof")?;
        Ok(Self {
            release_id: decode32(value.release_id, "capsule release ID")?,
            recovered_point: decode_element(value.recovered_point, "capsule recovered point")?,
            proof: DleqProof {
                commitment_g: decode_element(proof.commitment_g, "capsule DLEQ G commitment")?,
                commitment_h: decode_element(proof.commitment_h, "capsule DLEQ EPK commitment")?,
                response: decode_fr(proof.response, "capsule DLEQ response")?,
            },
        })
    }
}

impl From<CapsuleReleaseEvidence> for pb::CapsuleReleaseEvidence {
    fn from(value: CapsuleReleaseEvidence) -> Self {
        Self {
            release_id: value.release_id.to_vec(),
            recovered_point: value.recovered_point.vartime_compress().0.to_vec(),
            proof: Some(
                shieldd_sdk_proto::core::component::compliance::v1::DleqProof {
                    commitment_g: value.proof.commitment_g.vartime_compress().0.to_vec(),
                    commitment_h: value.proof.commitment_h.vartime_compress().0.to_vec(),
                    response: value.proof.response.to_bytes().to_vec(),
                },
            ),
        }
    }
}

fn decode32(bytes: Vec<u8>, label: &str) -> Result<[u8; 32]> {
    bytes
        .try_into()
        .map_err(|bytes: Vec<u8>| anyhow::anyhow!("{label} must be 32 bytes, got {}", bytes.len()))
}

fn decode_fq(bytes: Vec<u8>, label: &str) -> Result<Fq> {
    Fq::from_bytes_checked(&decode32(bytes, label)?)
        .map_err(|_| anyhow::anyhow!("{label} is not a canonical field element"))
}

fn decode_fr(bytes: Vec<u8>, label: &str) -> Result<Fr> {
    Fr::from_bytes_checked(&decode32(bytes, label)?)
        .map_err(|_| anyhow::anyhow!("{label} is not a canonical scalar"))
}

fn decode_element(bytes: Vec<u8>, label: &str) -> Result<Element> {
    let bytes = decode32(bytes, label)?;
    decaf377::Encoding(bytes)
        .vartime_decompress()
        .map_err(|_| anyhow::anyhow!("invalid {label} point encoding"))
}

#[cfg(test)]
mod tests {
    use decaf377_rdsa::SigningKey;
    use shieldd_sdk_keys::test_keys;

    use super::*;
    use crate::{HostTransfer, HostWithdrawalDestination};

    fn authorization() -> NoteSeizureAuthorizationBody {
        let asset_id = asset::Id(Fq::from(11u64));
        let amount = Amount::from(42u64);
        NoteSeizureAuthorizationBody {
            chain_id: "shieldd-test".to_owned(),
            note_commitment: tct::StateCommitment(Fq::from(7u64)),
            nullifier: Nullifier(Fq::from(8u64)),
            address: test_keys::ADDRESS_0.clone(),
            asset_id,
            amount,
            freeze_generation: 2,
            frozen_since_height: 10,
            withdrawal: HostWithdrawal {
                value: Value { amount, asset_id },
                destination: HostWithdrawalDestination::Transfer(HostTransfer {
                    recipient: "bank1seizureauthority".to_owned(),
                }),
            },
            expiry_height: 20,
        }
    }

    fn release_request(
        authorization: &NoteSeizureAuthorizationBody,
        capsule: &RecoveryCapsule,
        capk: Element,
    ) -> CapsuleReleaseRequest {
        CapsuleReleaseRequest {
            chain_id: authorization.chain_id.clone(),
            ring_id: "ring-11".to_owned(),
            policy_id: "policy-11".to_owned(),
            permission: "release".to_owned(),
            resource: "recovery-capsule".to_owned(),
            ring_pk: Element::GENERATOR * Fr::from(3u64),
            asset_id: authorization.asset_id,
            address: authorization.address.clone(),
            capk,
            note_commitment: authorization.note_commitment,
            recovery_commitment: capsule.commitment(),
            capsule_epk: capsule.epk,
            authority_instruction_commitment: authorization.commitment().unwrap(),
            expiry_height: authorization.expiry_height,
        }
    }

    fn release_evidence(
        request: &CapsuleReleaseRequest,
        capability_secret: Fr,
    ) -> CapsuleReleaseEvidence {
        CapsuleReleaseEvidence::from_capability_secret_for_test(request, capability_secret)
    }

    #[test]
    fn authority_signature_binds_the_complete_seizure_instruction() {
        let authority = SigningKey::<SpendAuth>::from(Fr::from(3u64));
        let authority_vk = VerificationKey::from(&authority);
        let body = authorization();
        let signature = authority.sign_deterministic(&body.signing_bytes().unwrap());

        body.verify_signature(&authority_vk, &signature).unwrap();

        let mut redirected = body.clone();
        redirected.withdrawal.destination = HostWithdrawalDestination::Transfer(HostTransfer {
            recipient: "bank1attacker".to_owned(),
        });
        redirected
            .verify_signature(&authority_vk, &signature)
            .expect_err("authority signature must bind the destination");

        let wrong_authority = SigningKey::<SpendAuth>::from(Fr::from(4u64));
        body.verify_signature(&VerificationKey::from(&wrong_authority), &signature)
            .expect_err("a different seizure authority must not authorize the note");
    }

    #[test]
    fn authorization_proto_round_trip_preserves_signing_bytes() {
        let body = authorization();
        let encoded = body.clone().encode_to_vec();
        let decoded = NoteSeizureAuthorizationBody::decode(encoded.as_slice()).unwrap();

        assert_eq!(decoded, body);
        assert_eq!(
            decoded.signing_bytes().unwrap(),
            body.signing_bytes().unwrap()
        );
        assert_eq!(decoded.commitment().unwrap(), body.commitment().unwrap());
    }

    #[test]
    fn capsule_release_dleq_binds_the_capsule_and_metadata() {
        let authorization = authorization();
        let capability_secret = Fr::from(19u64);
        let capk = Element::GENERATOR * capability_secret;
        let (capsule, _) = RecoveryCapsule::encrypt(
            authorization.amount,
            Fq::from(7u64),
            capk,
            crate::Rseed([17; 32]),
        )
        .unwrap();
        let request = release_request(&authorization, &capsule, capk);
        let evidence = release_evidence(&request, capability_secret);

        assert_eq!(evidence.verify(&request).unwrap(), evidence.recovered_point);

        let mutations: &[(&str, fn(&mut CapsuleReleaseRequest))] = &[
            ("chain", |r| r.chain_id.push('x')),
            ("ring", |r| r.ring_id.push('x')),
            ("policy", |r| r.policy_id.push('x')),
            ("permission", |r| r.permission.push('x')),
            ("resource", |r| r.resource.push('x')),
            ("ring key", |r| r.ring_pk += Element::GENERATOR),
            ("asset", |r| r.asset_id.0 += Fq::from(1u64)),
            ("address", |r| r.address = test_keys::ADDRESS_1.clone()),
            ("capability", |r| r.capk += Element::GENERATOR),
            ("note", |r| r.note_commitment.0 += Fq::from(1u64)),
            ("capsule", |r| r.recovery_commitment.0 += Fq::from(1u64)),
            ("EPK", |r| r.capsule_epk += Element::GENERATOR),
            ("authority instruction", |r| {
                r.authority_instruction_commitment += Fq::from(1u64)
            }),
            ("expiry", |r| r.expiry_height += 1),
        ];
        for (label, mutate) in mutations {
            let mut changed = request.clone();
            mutate(&mut changed);
            assert!(evidence.verify(&changed).is_err(), "unbound {label}");
            let mut rebound = evidence.clone();
            rebound.release_id = changed.release_id().unwrap();
            assert!(
                rebound.verify(&changed).is_err(),
                "rewriting the release ID must not rebind {label}"
            );
        }

        let mut other_point = evidence.clone();
        other_point.recovered_point += Element::GENERATOR;
        assert!(other_point.verify(&request).is_err());

        let mut maximal = request;
        maximal.chain_id = "c".repeat(MAX_NOTE_SEIZURE_CHAIN_ID_BYTES);
        maximal.ring_id = "r".repeat(MAX_CERTIFICATE_TEXT_BYTES);
        maximal.policy_id = "p".repeat(MAX_CERTIFICATE_TEXT_BYTES);
        maximal.permission = "a".repeat(MAX_CERTIFICATE_TEXT_BYTES);
        maximal.resource = "o".repeat(MAX_CERTIFICATE_TEXT_BYTES);
        maximal.validate().unwrap();
        let encoded = maximal.encode_to_vec();
        assert_eq!(
            CapsuleReleaseRequest::decode(encoded.as_slice()).unwrap(),
            maximal
        );
        maximal.permission.push('x');
        assert!(maximal.validate().is_err());
    }

    #[test]
    fn seizure_witness_needs_recovered_blinding_not_note_rseed() {
        let mut body = authorization();
        let capk = decaf377::Element::GENERATOR * Fr::from(19u64);
        let rseed = crate::Rseed([17; 32]);
        let note_blinding = rseed.derive_note_blinding();
        let (capsule, opening) =
            RecoveryCapsule::encrypt(body.amount, note_blinding, capk, rseed).unwrap();
        body.note_commitment = crate::note::commitment_from_address(
            body.address.clone(),
            Value {
                amount: body.amount,
                asset_id: body.asset_id,
            },
            note_blinding,
            capsule.commitment(),
        )
        .unwrap();

        let mut tree = tct::Tree::new();
        tree.insert(tct::Witness::Keep, body.note_commitment)
            .unwrap();
        let state_commitment_proof = tree.witness(body.note_commitment).unwrap();
        let rnk = Fq::from(23u64);
        body.nullifier = Nullifier::derive(
            &shieldd_sdk_keys::keys::NullifierKey(rnk),
            state_commitment_proof.position(),
            &body.note_commitment,
        );
        let public = NoteSeizureProofPublic {
            authorization: body,
            anchor: state_commitment_proof.root(),
            history_required: false,
            recent_position_floor: 0,
            recovery_capsule: capsule,
            recovery_seed: opening.seed,
            rnk_commitment: shieldd_sdk_compliance::compliance_nullifier_key_commitment(rnk),
        };
        let private = NoteSeizureProofPrivate {
            note_blinding,
            state_commitment_proof,
            rnk,
        };

        private.validate_against(&public).unwrap();
        crate::gnark::encode_note_seizure_witness(&public, &private).unwrap();

        let mut wrong = private;
        wrong.note_blinding += Fq::from(1u64);
        wrong
            .validate_against(&public)
            .expect_err("a different recovered blinding must not open the note");
    }
}
