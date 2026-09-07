use anyhow::{anyhow, bail, Result};
use decaf377::{Encoding, Fq};
use shieldd_sdk_asset::Balance;

use crate::{
    gnark::transfer_witness::TransferVolumeAccumulatorWitness,
    gnark::typed::{
        compliance_leaf_from_typed, indexed_leaf_from_typed, merkle_path_from_typed,
        point_affine_bytes, ComplianceLeafBinary, IndexedLeafBinary, MerklePathBinary,
        PointAffineBytes,
    },
    gnark::RecoveryCapsuleWitness,
    public_input_hash::shielded_ics20_withdrawal_statement_hash_from_public,
    shielded_ics20_withdrawal::{
        ShieldedIcs20WithdrawalChangePrivate, ShieldedIcs20WithdrawalChangePublic,
        ShieldedIcs20WithdrawalInputPublic, ShieldedIcs20WithdrawalProofPrivate,
        ShieldedIcs20WithdrawalProofPublic, ShieldedIcs20WithdrawalRequiredInputPrivate,
    },
    ShieldedIcs20WithdrawalFamilyId,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShieldedIcs20WithdrawalRequiredSpendWitness {
    pub nullifier: [u8; 32],
    pub spent_note_blinding: [u8; 32],
    pub spent_note_amount: [u8; 32],
    pub spent_note_recovery_commitment: [u8; 32],
    pub state_commitment_position: u64,
    pub state_commitment_auth_path: Vec<[[u8; 32]; 3]>,
    pub spend_auth_randomizer: [u8; 32],
    pub rk_affine: PointAffineBytes,
    pub history_required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShieldedIcs20WithdrawalOptionalSpendWitness {
    pub spend: ShieldedIcs20WithdrawalRequiredSpendWitness,
    pub is_dummy: bool,
    pub dummy_nullifier_seed: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShieldedIcs20WithdrawalChangeWitness {
    pub note_commitment: [u8; 32],
    pub recovery_commitment: [u8; 32],
    pub created_note_blinding: [u8; 32],
    pub created_note_amount: [u8; 32],
    pub recovery_capsule: RecoveryCapsuleWitness,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShieldedIcs20WithdrawalWitness {
    pub family_id: ShieldedIcs20WithdrawalFamilyId,
    pub total_length: u32,
    pub n_in: u32,
    pub anchor: [u8; 32],
    pub asset_anchor: [u8; 32],
    pub compliance_anchor: [u8; 32],
    pub target_timestamp: [u8; 32],
    pub outbound_asset_id: [u8; 32],
    pub outbound_amount: [u8; 32],
    pub withdrawal_effect_hash_limbs: [[u8; 32]; 4],
    pub claimed_statement_hash: [u8; 32],
    pub routing_tag: [u8; 32],
    pub routing_parameter_set_id: [u8; 32],
    pub withdrawal_epk_affine: PointAffineBytes,
    pub withdrawal_c2: [u8; 32],
    pub withdrawal_key_confirmation: [u8; 32],
    pub withdrawal_encrypted_sender_address: [[u8; 32]; 3],
    pub recent_position_floor: [u8; 32],
    pub action_balance_blinding: [u8; 32],
    pub nk: [u8; 32],
    pub asset_path: MerklePathBinary,
    pub asset_position: u64,
    pub asset_indexed_leaf: IndexedLeafBinary,
    pub is_regulated: bool,
    pub regulated_precision: u8,
    pub unregulated_precision: u8,
    pub routing_as_of_height: u64,
    pub routing_nonce: [u8; 32],
    pub sender_compliance_path: MerklePathBinary,
    pub sender_compliance_position: u64,
    pub sender_capk_affine: PointAffineBytes,
    pub sender_rnk_dh_pk_affine: PointAffineBytes,
    pub sender_rnk_commitment: [u8; 32],
    pub sender_status: [u8; 32],
    pub withdrawal_seed: [u8; 32],
    pub withdrawal_randomizer: [u8; 32],
    pub volume_accumulator: TransferVolumeAccumulatorWitness,
    pub volume_accumulator_seed: [u8; 32],
    pub required_spend: ShieldedIcs20WithdrawalRequiredSpendWitness,
    pub optional_spend: ShieldedIcs20WithdrawalOptionalSpendWitness,
    pub change_output: ShieldedIcs20WithdrawalChangeWitness,
    pub ak_affine: PointAffineBytes,
    pub asset_indexed_leaf_dk_pub_affine: PointAffineBytes,
    pub asset_indexed_leaf_ring_pk_affine: PointAffineBytes,
    pub sender_diversified_generator_affine: PointAffineBytes,
}

fn compliance_leaf_parts(
    leaf: &ComplianceLeafBinary,
) -> (PointAffineBytes, PointAffineBytes, [u8; 32], [u8; 32]) {
    (
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

fn spend_witness(
    public_input: &ShieldedIcs20WithdrawalInputPublic,
    private_input: &ShieldedIcs20WithdrawalRequiredInputPrivate,
    index: usize,
) -> Result<ShieldedIcs20WithdrawalRequiredSpendWitness> {
    let state_commitment_auth_path = private_input
        .state_commitment_proof
        .auth_path()
        .iter()
        .map(|siblings| siblings.map(|sibling| Fq::from(sibling).to_bytes()))
        .collect::<Vec<_>>();

    Ok(ShieldedIcs20WithdrawalRequiredSpendWitness {
        nullifier: public_input.nullifier.0.to_bytes(),
        spent_note_blinding: private_input.spent_note.note_blinding().to_bytes(),
        spent_note_amount: Fq::from(private_input.spent_note.value().amount).to_bytes(),
        spent_note_recovery_commitment: private_input.spent_note.recovery_commitment().0.to_bytes(),
        state_commitment_position: u64::from(private_input.state_commitment_proof.position()),
        state_commitment_auth_path,
        spend_auth_randomizer: private_input.spend_auth_randomizer.to_bytes(),
        rk_affine: point_affine_bytes(verification_key_point(
            public_input.rk,
            &format!("shielded_ics20_withdrawal_rk_{index}"),
        )?)?,
        history_required: public_input.history_required,
    })
}

fn change_witness(
    public_output: &ShieldedIcs20WithdrawalChangePublic,
    private_output: &ShieldedIcs20WithdrawalChangePrivate,
    capk: decaf377::Element,
) -> Result<ShieldedIcs20WithdrawalChangeWitness> {
    Ok(ShieldedIcs20WithdrawalChangeWitness {
        note_commitment: public_output.note_commitment.0.to_bytes(),
        recovery_commitment: public_output.recovery_commitment.0.to_bytes(),
        created_note_blinding: private_output.created_note.note_blinding().to_bytes(),
        created_note_amount: Fq::from(private_output.created_note.value().amount).to_bytes(),
        recovery_capsule: RecoveryCapsuleWitness::from_note(&private_output.created_note, capk)?,
    })
}

fn u128_from_field(value: Fq, label: &str) -> Result<u128> {
    let bytes = value.to_bytes();
    if bytes[16..].iter().any(|byte| *byte != 0) {
        bail!("{label} exceeds the circuit's 128-bit amount range");
    }
    Ok(u128::from_le_bytes(
        bytes[..16]
            .try_into()
            .expect("field prefix contains exactly 16 bytes"),
    ))
}

impl ShieldedIcs20WithdrawalWitness {
    pub fn from_public_private(
        public: &ShieldedIcs20WithdrawalProofPublic,
        private: &ShieldedIcs20WithdrawalProofPrivate,
    ) -> Result<Self> {
        public.validate_shape()?;
        private.validate_shape()?;
        if public.family_id != private.family_id {
            bail!(
                "shielded ICS-20 withdrawal witness family mismatch: public={} private={}",
                public.family_id.get(),
                private.family_id.get()
            );
        }

        let required_amount: u128 = private.required_input.spent_note.value().amount.into();
        let optional_amount: u128 = private
            .optional_input
            .spend
            .spent_note
            .value()
            .amount
            .into();
        let change_amount: u128 = private.change_output.created_note.value().amount.into();
        let outbound_amount = u128_from_field(public.outbound_amount, "outbound amount")?;
        let input_total = required_amount
            .checked_add(optional_amount)
            .ok_or_else(|| anyhow!("shielded ICS-20 withdrawal input amount sum overflows u128"))?;
        let output_total = change_amount.checked_add(outbound_amount).ok_or_else(|| {
            anyhow!("shielded ICS-20 withdrawal output amount sum overflows u128")
        })?;
        if input_total != output_total {
            bail!(
                "shielded ICS-20 withdrawal must conserve value internally: inputs={input_total}, outputs={output_total}"
            );
        }
        let expected_balance_commitment =
            Balance::default().commit(private.action_balance_blinding);
        if public.balance_commitment != expected_balance_commitment {
            bail!(
                "shielded ICS-20 withdrawal balance commitment must be the blinding-only commitment"
            );
        }

        let claimed_statement_hash =
            shielded_ics20_withdrawal_statement_hash_from_public(public)
                .map_err(|e| anyhow!("compute {} statement hash: {e}", public.family_id.label()))?;
        let sender_leaf = compliance_leaf_from_typed(&private.sender_leaf)?;
        let (sender_capk_affine, sender_rnk_dh_pk_affine, sender_rnk_commitment, sender_status) =
            compliance_leaf_parts(&sender_leaf);

        let required_spend = spend_witness(&public.inputs[0], &private.required_input, 0)?;
        let optional_spend = ShieldedIcs20WithdrawalOptionalSpendWitness {
            spend: spend_witness(&public.inputs[1], &private.optional_input.spend, 1)?,
            is_dummy: private.optional_input.is_dummy,
            dummy_nullifier_seed: private.optional_input.dummy_nullifier_seed.to_bytes(),
        };

        let volume_plan = &private.volume_accumulator.plan;
        let prior_state = volume_plan.prior_state();
        let successor_state = volume_plan.successor_state();
        let volume_accumulator = TransferVolumeAccumulatorWitness {
            nullifier: public.volume_accumulator.nullifier.0.to_bytes(),
            commitment: public.volume_accumulator.commitment.0.to_bytes(),
            day_start: Fq::from(public.volume_accumulator.day_start).to_bytes(),
            proof_context: crate::TransferProofContext::Ordinary.as_field().to_bytes(),
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
            family_id: public.family_id,
            total_length: 0,
            n_in: public.inputs.len() as u32,
            anchor: Fq::from(public.anchor).to_bytes(),
            asset_anchor: public.asset_anchor.0.to_bytes(),
            compliance_anchor: public.compliance_anchor.0.to_bytes(),
            target_timestamp: public.target_timestamp.to_bytes(),
            outbound_asset_id: public.outbound_asset_id.to_bytes(),
            outbound_amount: public.outbound_amount.to_bytes(),
            withdrawal_effect_hash_limbs: public
                .withdrawal_effect_hash_limbs
                .map(|limb| limb.to_bytes()),
            claimed_statement_hash: claimed_statement_hash.to_bytes(),
            routing_tag: Fq::from(public.routing_tag.value).to_bytes(),
            routing_parameter_set_id: public.routing_parameter_set_id.to_bytes(),
            withdrawal_epk_affine: point_affine_bytes(public.withdrawal_compliance_ciphertext.epk)?,
            withdrawal_c2: public.withdrawal_compliance_ciphertext.c2.to_bytes(),
            withdrawal_key_confirmation: public
                .withdrawal_compliance_ciphertext
                .key_confirmation
                .to_bytes(),
            withdrawal_encrypted_sender_address: std::array::from_fn(|index| {
                public
                    .withdrawal_compliance_ciphertext
                    .encrypted_sender_address[index * 32..(index + 1) * 32]
                    .try_into()
                    .expect("withdrawal ciphertext word is exactly 32 bytes")
            }),
            recent_position_floor: Fq::from(public.recent_position_floor).to_bytes(),
            action_balance_blinding: private.action_balance_blinding.to_bytes(),
            nk: private.nk.0.to_bytes(),
            asset_path: merkle_path_from_typed(&private.asset_path)?,
            asset_position: private.asset_position,
            asset_indexed_leaf: indexed_leaf_from_typed(&private.asset_indexed_leaf),
            is_regulated: private.is_regulated,
            regulated_precision: private.routing_parameters.regulated_precision.bits(),
            unregulated_precision: private.routing_parameters.unregulated_precision.bits(),
            routing_as_of_height: private.routing_parameters.as_of_height,
            routing_nonce: private.routing_nonce.to_bytes(),
            sender_compliance_path: merkle_path_from_typed(&private.sender_compliance_path)?,
            sender_compliance_position: private.sender_compliance_position,
            sender_capk_affine,
            sender_rnk_dh_pk_affine,
            sender_rnk_commitment,
            sender_status,
            withdrawal_seed: private.withdrawal_seed.to_bytes(),
            withdrawal_randomizer: private.withdrawal_randomizer.to_bytes(),
            volume_accumulator,
            volume_accumulator_seed: private.volume_accumulator_seed.to_bytes(),
            required_spend,
            optional_spend,
            change_output: change_witness(
                &public.change_output,
                &private.change_output,
                private.sender_leaf.capk,
            )?,
            ak_affine: point_affine_bytes(verification_key_point(
                private.ak,
                "shielded_ics20_withdrawal_ak",
            )?)?,
            asset_indexed_leaf_dk_pub_affine: point_affine_bytes(
                private.asset_indexed_leaf.params.dk_pub,
            )?,
            asset_indexed_leaf_ring_pk_affine: point_affine_bytes(
                private.asset_indexed_leaf.ring.ring_pk,
            )?,
            sender_diversified_generator_affine: point_affine_bytes(
                *private.sender_leaf.address.diversified_generator(),
            )?,
        };
        witness.total_length = u32::try_from(witness.encode()?.len())
            .map_err(|_| anyhow!("encoded {} witness exceeds u32", witness.family_id.label()))?;
        Ok(witness)
    }
}
