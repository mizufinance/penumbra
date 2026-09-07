use anyhow::{anyhow, Context, Result};
use decaf377::{Encoding, Fq};

use crate::{
    gnark::typed::{
        compliance_leaf_from_typed, indexed_leaf_from_typed, merkle_path_from_typed,
        point_affine_bytes, ComplianceLeafBinary, IndexedLeafBinary, MerklePathBinary,
        PointAffineBytes,
    },
    gnark::RecoveryCapsuleWitness,
    public_input_hash::transfer_statement_hash_from_public,
    transfer::{
        TransferComplianceCiphertextPublic, TransferProofPrivate, TransferProofPublic,
        TransferSpendPrivate, TransferSpendPublic, TRANSFER_PROOF_LABEL,
    },
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferRequiredSpendWitness {
    pub nullifier: [u8; 32],
    pub spent_note_blinding: [u8; 32],
    pub spent_note_amount: [u8; 32],
    pub spent_note_asset_id: [u8; 32],
    pub spent_note_recovery_commitment: [u8; 32],
    pub state_commitment_position: u64,
    pub state_commitment_auth_path: Vec<[[u8; 32]; 3]>,
    pub spend_auth_randomizer: [u8; 32],
    pub rk_affine: PointAffineBytes,
    pub history_required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferOptionalSpendWitness {
    pub nullifier: [u8; 32],
    pub spent_note_blinding: [u8; 32],
    pub spent_note_amount: [u8; 32],
    pub spent_note_recovery_commitment: [u8; 32],
    pub state_commitment_position: u64,
    pub state_commitment_auth_path: Vec<[[u8; 32]; 3]>,
    pub spend_auth_randomizer: [u8; 32],
    pub rk_affine: PointAffineBytes,
    pub is_dummy: bool,
    pub dummy_nullifier_seed: [u8; 32],
    pub history_required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferReceiverOutputWitness {
    pub note_commitment: [u8; 32],
    pub recovery_commitment: [u8; 32],
    pub created_note_blinding: [u8; 32],
    pub created_note_amount: [u8; 32],
    pub recovery_capsule: RecoveryCapsuleWitness,
    pub recipient_compliance_path: MerklePathBinary,
    pub recipient_compliance_position: u64,
    pub recipient_capk_affine: PointAffineBytes,
    pub recipient_rnk_dh_pk_affine: PointAffineBytes,
    pub recipient_rnk_commitment: [u8; 32],
    pub recipient_status: [u8; 32],
    pub recipient_diversified_generator_affine: PointAffineBytes,
    pub recipient_transmission_key_affine: PointAffineBytes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferChangeOutputWitness {
    pub note_commitment: [u8; 32],
    pub recovery_commitment: [u8; 32],
    pub created_note_blinding: [u8; 32],
    pub created_note_amount: [u8; 32],
    pub recovery_capsule: RecoveryCapsuleWitness,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferVolumeAccumulatorWitness {
    pub nullifier: [u8; 32],
    pub commitment: [u8; 32],
    pub day_start: [u8; 32],
    pub proof_context: [u8; 32],
    pub use_real: bool,
    pub starts_new_day: bool,
    pub subject: [u8; 32],
    pub prior_volume: [u8; 32],
    pub prior_blinding: [u8; 32],
    pub prior_commitment: [u8; 32],
    pub prior_position: u64,
    pub prior_auth_path: Vec<[[u8; 32]; 3]>,
    pub successor_volume: [u8; 32],
    pub successor_blinding: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferComplianceCiphertextWitness {
    pub c2: [u8; 32],
    pub ciphertext: Vec<[u8; 32]>,
    pub epk_affine: PointAffineBytes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferTierRandomizersWitness {
    pub core: [u8; 32],
    pub ext: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferWitness {
    pub total_length: u32,
    pub anchor: [u8; 32],
    pub asset_anchor: [u8; 32],
    pub compliance_anchor: [u8; 32],
    pub target_timestamp: [u8; 32],
    pub claimed_statement_hash: [u8; 32],
    pub routing_tags: [[u8; 32]; 2],
    pub routing_parameter_set_id: [u8; 32],
    pub recent_position_floor: [u8; 32],
    pub volume_accumulator: TransferVolumeAccumulatorWitness,
    pub action_balance_blinding: [u8; 32],
    pub nk: [u8; 32],
    pub asset_path: MerklePathBinary,
    pub asset_position: u64,
    pub asset_indexed_leaf: IndexedLeafBinary,
    pub is_regulated: bool,
    pub regulated_precision: u8,
    pub unregulated_precision: u8,
    pub routing_as_of_height: u64,
    pub sender_compliance_path: MerklePathBinary,
    pub sender_compliance_position: u64,
    pub sender_capk_affine: PointAffineBytes,
    pub sender_rnk_dh_pk_affine: PointAffineBytes,
    pub sender_rnk_commitment: [u8; 32],
    pub sender_status: [u8; 32],
    pub transfer_nonce_root: [u8; 32],
    pub detection_ciphertext: Vec<[u8; 32]>,
    pub sender_core_key_confirmation: [u8; 32],
    pub output_core_key_confirmation: [u8; 32],
    pub ring_id_hash: [u8; 32],
    pub policy_id_hash: [u8; 32],
    pub resource_hash: [u8; 32],
    pub permission_hash: [u8; 32],
    pub metadata_target_timestamp: [u8; 32],
    pub sender_core_salt: [u8; 32],
    pub sender_ext_salt: [u8; 32],
    pub output_core_salt: [u8; 32],
    pub output_ext_salt: [u8; 32],
    pub sender_core: TransferComplianceCiphertextWitness,
    pub sender_ext: TransferComplianceCiphertextWitness,
    pub output_core: TransferComplianceCiphertextWitness,
    pub output_ext: TransferComplianceCiphertextWitness,
    pub sender_randomizers: TransferTierRandomizersWitness,
    pub output_randomizers: TransferTierRandomizersWitness,
    pub required_spend: TransferRequiredSpendWitness,
    pub optional_spend: TransferOptionalSpendWitness,
    pub receiver_output: TransferReceiverOutputWitness,
    pub change_output: TransferChangeOutputWitness,
    pub ak_affine: PointAffineBytes,
    pub asset_indexed_leaf_dk_pub_affine: PointAffineBytes,
    pub asset_indexed_leaf_ring_pk_affine: PointAffineBytes,
    pub sender_diversified_generator_affine: PointAffineBytes,
    pub sender_transmission_key_affine: PointAffineBytes,
}

fn compliance_leaf_parts(
    leaf: &ComplianceLeafBinary,
) -> (
    [u8; 48],
    [u8; 32],
    PointAffineBytes,
    PointAffineBytes,
    [u8; 32],
    [u8; 32],
) {
    (
        leaf.address,
        leaf.asset_id,
        leaf.capk_affine.clone(),
        leaf.rnk_dh_pk_affine.clone(),
        leaf.rnk_commitment,
        leaf.status,
    )
}

fn verification_key_point(
    vk: decaf377_rdsa::VerificationKey<decaf377_rdsa::SpendAuth>,
    label: &str,
) -> Result<decaf377::Element> {
    Encoding(vk.to_bytes())
        .vartime_decompress()
        .map_err(|e| anyhow!("decompress {label}: {e:?}"))
}

struct TransferSpendWitnessParts {
    nullifier: [u8; 32],
    spent_note_blinding: [u8; 32],
    spent_note_amount: [u8; 32],
    spent_note_asset_id: [u8; 32],
    spent_note_recovery_commitment: [u8; 32],
    state_commitment_position: u64,
    state_commitment_auth_path: Vec<[[u8; 32]; 3]>,
    spend_auth_randomizer: [u8; 32],
    rk_affine: PointAffineBytes,
}

fn spend_witness_parts(
    public: &TransferSpendPublic,
    private: &TransferSpendPrivate,
    index: usize,
) -> Result<TransferSpendWitnessParts> {
    let state_commitment_auth_path = private
        .state_commitment_proof
        .auth_path()
        .iter()
        .map(|siblings| siblings.map(|sibling| Fq::from(sibling).to_bytes()))
        .collect::<Vec<_>>();
    Ok(TransferSpendWitnessParts {
        nullifier: public.nullifier.0.to_bytes(),
        spent_note_blinding: private.spent_note.note_blinding().to_bytes(),
        spent_note_amount: Fq::from(private.spent_note.value().amount).to_bytes(),
        spent_note_asset_id: private.spent_note.asset_id().0.to_bytes(),
        spent_note_recovery_commitment: private.spent_note.recovery_commitment().0.to_bytes(),
        state_commitment_position: u64::from(private.state_commitment_proof.position()),
        state_commitment_auth_path,
        spend_auth_randomizer: private.spend_auth_randomizer.to_bytes(),
        rk_affine: point_affine_bytes(verification_key_point(public.rk, &format!("rk_{index}"))?)?,
    })
}

fn compliance_tier_witness(
    tier: &TransferComplianceCiphertextPublic,
) -> Result<TransferComplianceCiphertextWitness> {
    Ok(TransferComplianceCiphertextWitness {
        c2: tier.c2.to_bytes(),
        ciphertext: tier
            .ciphertext
            .iter()
            .map(|value| value.to_bytes())
            .collect(),
        epk_affine: point_affine_bytes(tier.epk)?,
    })
}

impl TransferWitness {
    pub fn from_public_private(
        public: &TransferProofPublic,
        private: &TransferProofPrivate,
    ) -> Result<Self> {
        public.validate_shape()?;

        let claimed_statement_hash = transfer_statement_hash_from_public(public)
            .with_context(|| format!("compute {TRANSFER_PROOF_LABEL} statement hash"))?;

        let sender_leaf = compliance_leaf_from_typed(&private.sender_leaf)?;
        let (
            _,
            _,
            sender_capk_affine,
            sender_rnk_dh_pk_affine,
            sender_rnk_commitment,
            sender_status,
        ) = compliance_leaf_parts(&sender_leaf);
        let required = spend_witness_parts(&public.inputs[0], &private.required_input, 0)?;
        let required_spend = TransferRequiredSpendWitness {
            nullifier: required.nullifier,
            spent_note_blinding: required.spent_note_blinding,
            spent_note_amount: required.spent_note_amount,
            spent_note_asset_id: required.spent_note_asset_id,
            spent_note_recovery_commitment: required.spent_note_recovery_commitment,
            state_commitment_position: required.state_commitment_position,
            state_commitment_auth_path: required.state_commitment_auth_path,
            spend_auth_randomizer: required.spend_auth_randomizer,
            rk_affine: required.rk_affine,
            history_required: public.inputs[0].history_required,
        };
        let optional = spend_witness_parts(&public.inputs[1], &private.optional_input.spend, 1)?;
        let optional_spend = TransferOptionalSpendWitness {
            nullifier: optional.nullifier,
            spent_note_blinding: optional.spent_note_blinding,
            spent_note_amount: optional.spent_note_amount,
            spent_note_recovery_commitment: optional.spent_note_recovery_commitment,
            state_commitment_position: optional.state_commitment_position,
            state_commitment_auth_path: optional.state_commitment_auth_path,
            spend_auth_randomizer: optional.spend_auth_randomizer,
            rk_affine: optional.rk_affine,
            is_dummy: private.optional_input.is_dummy,
            dummy_nullifier_seed: private.optional_input.dummy_nullifier_seed.to_bytes(),
            history_required: public.inputs[1].history_required,
        };

        let receiver_private = &private.receiver_output;
        let receiver_leaf = compliance_leaf_from_typed(&receiver_private.recipient_leaf)?;
        let (
            _,
            _,
            recipient_capk_affine,
            recipient_rnk_dh_pk_affine,
            recipient_rnk_commitment,
            receiver_status,
        ) = compliance_leaf_parts(&receiver_leaf);
        let receiver_output = TransferReceiverOutputWitness {
            note_commitment: public.outputs[0].note_commitment.0.to_bytes(),
            recovery_commitment: public.outputs[0].recovery_commitment.0.to_bytes(),
            created_note_blinding: receiver_private.created_note.note_blinding().to_bytes(),
            created_note_amount: Fq::from(receiver_private.created_note.value().amount).to_bytes(),
            recovery_capsule: RecoveryCapsuleWitness::from_note(
                &receiver_private.created_note,
                receiver_private.recipient_leaf.capk,
            )?,
            recipient_compliance_path: merkle_path_from_typed(
                &receiver_private.recipient_compliance_path,
            )?,
            recipient_compliance_position: receiver_private.recipient_compliance_position,
            recipient_capk_affine,
            recipient_rnk_dh_pk_affine,
            recipient_rnk_commitment,
            recipient_status: receiver_status,
            recipient_diversified_generator_affine: point_affine_bytes(
                *receiver_private
                    .recipient_leaf
                    .address
                    .diversified_generator(),
            )?,
            recipient_transmission_key_affine: point_affine_bytes(
                Encoding(receiver_private.recipient_leaf.address.transmission_key().0)
                    .vartime_decompress()
                    .map_err(|e| anyhow!("decompress receiver transmission key: {e:?}"))?,
            )?,
        };
        let change_output = TransferChangeOutputWitness {
            note_commitment: public.outputs[1].note_commitment.0.to_bytes(),
            recovery_commitment: public.outputs[1].recovery_commitment.0.to_bytes(),
            created_note_blinding: private
                .change_output
                .created_note
                .note_blinding()
                .to_bytes(),
            created_note_amount: Fq::from(private.change_output.created_note.value().amount)
                .to_bytes(),
            recovery_capsule: RecoveryCapsuleWitness::from_note(
                &private.change_output.created_note,
                private.sender_leaf.capk,
            )?,
        };
        let volume_plan = &private.volume_accumulator.plan;
        let prior_state = volume_plan.prior_state();
        let successor_state = volume_plan.successor_state();
        let volume_accumulator = TransferVolumeAccumulatorWitness {
            nullifier: public.volume_accumulator.nullifier.0.to_bytes(),
            commitment: public.volume_accumulator.commitment.0.to_bytes(),
            day_start: Fq::from(public.volume_accumulator.day_start).to_bytes(),
            proof_context: public.proof_context.as_field().to_bytes(),
            use_real: volume_plan.is_real(),
            starts_new_day: volume_plan.starts_new_day(),
            subject: successor_state
                .as_ref()
                .map(|state| state.subject)
                .unwrap_or_else(|| Fq::from(0u64))
                .to_bytes(),
            prior_volume: Fq::from(
                prior_state
                    .map(|state| state.undisclosed_volume)
                    .unwrap_or(0),
            )
            .to_bytes(),
            prior_blinding: prior_state
                .map(|state| state.blinding)
                .unwrap_or_else(|| Fq::from(0u64))
                .to_bytes(),
            prior_commitment: volume_plan.prior_commitment().0.to_bytes(),
            prior_position: u64::from(private.volume_accumulator.prior_proof.position()),
            prior_auth_path: private
                .volume_accumulator
                .prior_proof
                .auth_path()
                .iter()
                .map(|siblings| siblings.map(|sibling| Fq::from(sibling).to_bytes()))
                .collect(),
            successor_volume: Fq::from(
                successor_state
                    .as_ref()
                    .map(|state| state.undisclosed_volume)
                    .unwrap_or(0),
            )
            .to_bytes(),
            successor_blinding: successor_state
                .map(|state| state.blinding)
                .unwrap_or_else(|| Fq::from(0u64))
                .to_bytes(),
        };

        let mut witness = Self {
            total_length: 0,
            anchor: Fq::from(public.anchor).to_bytes(),
            asset_anchor: public.asset_anchor.0.to_bytes(),
            compliance_anchor: public.compliance_anchor.0.to_bytes(),
            target_timestamp: public.target_timestamp.to_bytes(),
            claimed_statement_hash: claimed_statement_hash.to_bytes(),
            routing_tags: public
                .routing
                .tags
                .map(|tag| Fq::from(tag.value).to_bytes()),
            routing_parameter_set_id: public.routing_parameter_set_id.to_bytes(),
            recent_position_floor: Fq::from(public.recent_position_floor).to_bytes(),
            volume_accumulator,
            action_balance_blinding: private.action_balance_blinding.to_bytes(),
            nk: private.nk.0.to_bytes(),
            asset_path: merkle_path_from_typed(&private.asset_path)?,
            asset_position: private.asset_position,
            asset_indexed_leaf: indexed_leaf_from_typed(&private.asset_indexed_leaf),
            is_regulated: private.is_regulated,
            regulated_precision: private.routing_parameters.regulated_precision.bits(),
            unregulated_precision: private.routing_parameters.unregulated_precision.bits(),
            routing_as_of_height: private.routing_parameters.as_of_height,
            sender_compliance_path: merkle_path_from_typed(&private.sender_compliance_path)?,
            sender_compliance_position: private.sender_compliance_position,
            sender_capk_affine,
            sender_rnk_dh_pk_affine,
            sender_rnk_commitment,
            sender_status,
            transfer_nonce_root: private.compliance.transfer_nonce_root.to_bytes(),
            detection_ciphertext: public
                .compliance
                .detection_ciphertext
                .iter()
                .map(|value| value.to_bytes())
                .collect(),
            sender_core_key_confirmation: public.compliance.sender_core_key_confirmation.to_bytes(),
            output_core_key_confirmation: public.compliance.output_core_key_confirmation.to_bytes(),
            ring_id_hash: public.compliance.metadata.ring_id_hash_bytes,
            policy_id_hash: public.compliance.metadata.policy_id_hash_bytes,
            resource_hash: public.compliance.metadata.resource_hash_bytes,
            permission_hash: public.compliance.metadata.permission_hash_bytes,
            metadata_target_timestamp: Fq::from(public.compliance.metadata.target_timestamp)
                .to_bytes(),
            sender_core_salt: public.compliance.metadata.sender_core_salt_bytes,
            sender_ext_salt: public.compliance.metadata.sender_ext_salt_bytes,
            output_core_salt: public.compliance.metadata.output_core_salt_bytes,
            output_ext_salt: public.compliance.metadata.output_ext_salt_bytes,
            sender_core: compliance_tier_witness(&public.compliance.sender_core)?,
            sender_ext: compliance_tier_witness(&public.compliance.sender_ext)?,
            output_core: compliance_tier_witness(&public.compliance.output_core)?,
            output_ext: compliance_tier_witness(&public.compliance.output_ext)?,
            sender_randomizers: TransferTierRandomizersWitness {
                core: private.compliance.sender.core.to_bytes(),
                ext: private.compliance.sender.ext.to_bytes(),
            },
            output_randomizers: TransferTierRandomizersWitness {
                core: private.compliance.output.core.to_bytes(),
                ext: private.compliance.output.ext.to_bytes(),
            },
            required_spend,
            optional_spend,
            receiver_output,
            change_output,
            ak_affine: point_affine_bytes(verification_key_point(private.ak, "ak")?)?,
            asset_indexed_leaf_dk_pub_affine: point_affine_bytes(
                private.asset_indexed_leaf.params.dk_pub,
            )?,
            asset_indexed_leaf_ring_pk_affine: point_affine_bytes(
                private.asset_indexed_leaf.ring.ring_pk,
            )?,
            sender_diversified_generator_affine: point_affine_bytes(
                *private.sender_leaf.address.diversified_generator(),
            )?,
            sender_transmission_key_affine: point_affine_bytes(
                Encoding(private.sender_leaf.address.transmission_key().0)
                    .vartime_decompress()
                    .map_err(|e| anyhow!("decompress sender transmission key: {e:?}"))?,
            )?,
        };
        witness.total_length = u32::try_from(witness.encode()?.len())
            .map_err(|_| anyhow!("encoded {TRANSFER_PROOF_LABEL} witness exceeds u32"))?;
        Ok(witness)
    }
}
