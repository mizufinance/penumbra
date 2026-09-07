use anyhow::{bail, Context, Result};

use crate::{
    gnark::{
        binary::encode_triple_path_32,
        binary::{encode_vec_32, put_bytes, put_u32, put_u64, put_u8, BinaryCursor},
        recovery_capsule_witness_binary::{decode_recovery_capsule, encode_recovery_capsule},
        transfer_witness::{
            TransferChangeOutputWitness, TransferComplianceCiphertextWitness,
            TransferOptionalSpendWitness, TransferReceiverOutputWitness,
            TransferRequiredSpendWitness, TransferTierRandomizersWitness,
            TransferVolumeAccumulatorWitness, TransferWitness,
        },
        typed::{
            decode_indexed_leaf, encode_indexed_leaf, encode_merkle_path, encode_point_affine,
        },
    },
    transfer::TRANSFER_PROOF_LABEL,
};

const TRANSFER_WITNESS_MAGIC: &[u8; 4] = b"PTWG";

impl TransferWitness {
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        put_bytes(&mut buf, TRANSFER_WITNESS_MAGIC);
        put_u32(&mut buf, 0);
        put_bytes(&mut buf, &self.anchor);
        put_bytes(&mut buf, &self.asset_anchor);
        put_bytes(&mut buf, &self.compliance_anchor);
        put_bytes(&mut buf, &self.target_timestamp);
        put_bytes(&mut buf, &self.claimed_statement_hash);
        for tag in &self.routing_tags {
            put_bytes(&mut buf, tag);
        }
        put_bytes(&mut buf, &self.routing_parameter_set_id);
        put_bytes(&mut buf, &self.recent_position_floor);
        encode_volume_accumulator(&mut buf, &self.volume_accumulator)?;
        put_bytes(&mut buf, &self.action_balance_blinding);
        put_bytes(&mut buf, &self.nk);
        encode_merkle_path(&mut buf, &self.asset_path)?;
        put_u64(&mut buf, self.asset_position);
        encode_indexed_leaf(&mut buf, &self.asset_indexed_leaf);
        put_u8(&mut buf, u8::from(self.is_regulated));
        put_u8(&mut buf, self.regulated_precision);
        put_u8(&mut buf, self.unregulated_precision);
        put_u64(&mut buf, self.routing_as_of_height);
        encode_merkle_path(&mut buf, &self.sender_compliance_path)?;
        put_u64(&mut buf, self.sender_compliance_position);
        encode_point_affine(&mut buf, &self.sender_capk_affine);
        encode_point_affine(&mut buf, &self.sender_rnk_dh_pk_affine);
        put_bytes(&mut buf, &self.sender_rnk_commitment);
        put_bytes(&mut buf, &self.sender_status);
        put_bytes(&mut buf, &self.transfer_nonce_root);
        encode_vec_32(&mut buf, &self.detection_ciphertext)?;
        put_bytes(&mut buf, &self.sender_core_key_confirmation);
        put_bytes(&mut buf, &self.output_core_key_confirmation);
        put_bytes(&mut buf, &self.ring_id_hash);
        put_bytes(&mut buf, &self.policy_id_hash);
        put_bytes(&mut buf, &self.resource_hash);
        put_bytes(&mut buf, &self.permission_hash);
        put_bytes(&mut buf, &self.metadata_target_timestamp);
        put_bytes(&mut buf, &self.sender_core_salt);
        put_bytes(&mut buf, &self.sender_ext_salt);
        put_bytes(&mut buf, &self.output_core_salt);
        put_bytes(&mut buf, &self.output_ext_salt);
        encode_compliance_tier(&mut buf, &self.sender_core)?;
        encode_compliance_tier(&mut buf, &self.sender_ext)?;
        encode_compliance_tier(&mut buf, &self.output_core)?;
        encode_compliance_tier(&mut buf, &self.output_ext)?;
        encode_randomizers(&mut buf, &self.sender_randomizers);
        encode_randomizers(&mut buf, &self.output_randomizers);

        encode_required_spend(&mut buf, &self.required_spend)?;
        encode_optional_spend(&mut buf, &self.optional_spend)?;
        encode_receiver_output(&mut buf, &self.receiver_output)?;
        encode_change_output(&mut buf, &self.change_output);

        encode_point_affine(&mut buf, &self.ak_affine);
        encode_point_affine(&mut buf, &self.asset_indexed_leaf_dk_pub_affine);
        encode_point_affine(&mut buf, &self.asset_indexed_leaf_ring_pk_affine);
        encode_point_affine(&mut buf, &self.sender_diversified_generator_affine);
        encode_point_affine(&mut buf, &self.sender_transmission_key_affine);

        let total_len = u32::try_from(buf.len()).context("encoded transfer witness exceeds u32")?;
        buf[4..8].copy_from_slice(&total_len.to_le_bytes());
        Ok(buf)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut cursor = BinaryCursor::new(bytes);
        if cursor.read_fixed::<4>()? != *TRANSFER_WITNESS_MAGIC {
            bail!("invalid transfer witness magic");
        }
        let total_length = cursor.read_u32()?;
        if total_length as usize != bytes.len() {
            bail!(
                "transfer witness length mismatch: header={}, actual={}",
                total_length,
                bytes.len()
            );
        }

        let witness = Self {
            total_length,
            anchor: cursor.read_fixed::<32>()?,
            asset_anchor: cursor.read_fixed::<32>()?,
            compliance_anchor: cursor.read_fixed::<32>()?,
            target_timestamp: cursor.read_fixed::<32>()?,
            claimed_statement_hash: cursor.read_fixed::<32>()?,
            routing_tags: [cursor.read_fixed::<32>()?, cursor.read_fixed::<32>()?],
            routing_parameter_set_id: cursor.read_fixed::<32>()?,
            recent_position_floor: cursor.read_fixed::<32>()?,
            volume_accumulator: decode_volume_accumulator(&mut cursor)?,
            action_balance_blinding: cursor.read_fr()?,
            nk: cursor.read_fixed::<32>()?,
            asset_path: cursor.read_merkle_path()?,
            asset_position: cursor.read_u64()?,
            asset_indexed_leaf: decode_indexed_leaf(&mut cursor)?,
            is_regulated: cursor.read_bool()?,
            regulated_precision: cursor.read_u8()?,
            unregulated_precision: cursor.read_u8()?,
            routing_as_of_height: cursor.read_u64()?,
            sender_compliance_path: cursor.read_merkle_path()?,
            sender_compliance_position: cursor.read_u64()?,
            sender_capk_affine: cursor.read_point_affine()?,
            sender_rnk_dh_pk_affine: cursor.read_point_affine()?,
            sender_rnk_commitment: cursor.read_fixed::<32>()?,
            sender_status: cursor.read_fixed::<32>()?,
            transfer_nonce_root: cursor.read_fixed::<32>()?,
            detection_ciphertext: cursor.read_vec_32()?,
            sender_core_key_confirmation: cursor.read_fixed::<32>()?,
            output_core_key_confirmation: cursor.read_fixed::<32>()?,
            ring_id_hash: cursor.read_fixed::<32>()?,
            policy_id_hash: cursor.read_fixed::<32>()?,
            resource_hash: cursor.read_fixed::<32>()?,
            permission_hash: cursor.read_fixed::<32>()?,
            metadata_target_timestamp: cursor.read_fixed::<32>()?,
            sender_core_salt: cursor.read_fixed::<32>()?,
            sender_ext_salt: cursor.read_fixed::<32>()?,
            output_core_salt: cursor.read_fixed::<32>()?,
            output_ext_salt: cursor.read_fixed::<32>()?,
            sender_core: decode_compliance_tier(&mut cursor)?,
            sender_ext: decode_compliance_tier(&mut cursor)?,
            output_core: decode_compliance_tier(&mut cursor)?,
            output_ext: decode_compliance_tier(&mut cursor)?,
            sender_randomizers: decode_randomizers(&mut cursor)?,
            output_randomizers: decode_randomizers(&mut cursor)?,
            required_spend: decode_required_spend(&mut cursor)?,
            optional_spend: decode_optional_spend(&mut cursor)?,
            receiver_output: decode_receiver_output(&mut cursor)?,
            change_output: decode_change_output(&mut cursor)?,
            ak_affine: cursor.read_point_affine()?,
            asset_indexed_leaf_dk_pub_affine: cursor.read_point_affine()?,
            asset_indexed_leaf_ring_pk_affine: cursor.read_point_affine()?,
            sender_diversified_generator_affine: cursor.read_point_affine()?,
            sender_transmission_key_affine: cursor.read_point_affine()?,
        };

        cursor.finish(TRANSFER_PROOF_LABEL)?;
        Ok(witness)
    }
}

pub(crate) fn encode_volume_accumulator(
    buf: &mut Vec<u8>,
    value: &TransferVolumeAccumulatorWitness,
) -> Result<()> {
    put_bytes(buf, &value.nullifier);
    put_bytes(buf, &value.commitment);
    put_bytes(buf, &value.day_start);
    put_bytes(buf, &value.proof_context);
    put_u8(buf, u8::from(value.use_real));
    put_u8(buf, u8::from(value.starts_new_day));
    put_bytes(buf, &value.subject);
    put_bytes(buf, &value.prior_volume);
    put_bytes(buf, &value.prior_blinding);
    put_bytes(buf, &value.prior_commitment);
    put_u64(buf, value.prior_position);
    encode_triple_path_32(buf, &value.prior_auth_path)?;
    put_bytes(buf, &value.successor_volume);
    put_bytes(buf, &value.successor_blinding);
    Ok(())
}

pub(crate) fn decode_volume_accumulator(
    cursor: &mut BinaryCursor<'_>,
) -> Result<TransferVolumeAccumulatorWitness> {
    Ok(TransferVolumeAccumulatorWitness {
        nullifier: cursor.read_fixed::<32>()?,
        commitment: cursor.read_fixed::<32>()?,
        day_start: cursor.read_fixed::<32>()?,
        proof_context: cursor.read_fixed::<32>()?,
        use_real: cursor.read_bool()?,
        starts_new_day: cursor.read_bool()?,
        subject: cursor.read_fixed::<32>()?,
        prior_volume: cursor.read_fixed::<32>()?,
        prior_blinding: cursor.read_fixed::<32>()?,
        prior_commitment: cursor.read_fixed::<32>()?,
        prior_position: cursor.read_u64()?,
        prior_auth_path: cursor.read_triple_path_32()?,
        successor_volume: cursor.read_fixed::<32>()?,
        successor_blinding: cursor.read_fixed::<32>()?,
    })
}

fn encode_compliance_tier(
    buf: &mut Vec<u8>,
    tier: &TransferComplianceCiphertextWitness,
) -> Result<()> {
    put_bytes(buf, &tier.c2);
    encode_vec_32(buf, &tier.ciphertext)?;
    encode_point_affine(buf, &tier.epk_affine);
    Ok(())
}

fn decode_compliance_tier(
    cursor: &mut BinaryCursor<'_>,
) -> Result<TransferComplianceCiphertextWitness> {
    Ok(TransferComplianceCiphertextWitness {
        c2: cursor.read_fixed::<32>()?,
        ciphertext: cursor.read_vec_32()?,
        epk_affine: cursor.read_point_affine()?,
    })
}

fn encode_randomizers(buf: &mut Vec<u8>, randomizers: &TransferTierRandomizersWitness) {
    put_bytes(buf, &randomizers.core);
    put_bytes(buf, &randomizers.ext);
}

fn decode_randomizers(cursor: &mut BinaryCursor<'_>) -> Result<TransferTierRandomizersWitness> {
    Ok(TransferTierRandomizersWitness {
        core: cursor.read_fr()?,
        ext: cursor.read_fr()?,
    })
}

fn encode_required_spend(buf: &mut Vec<u8>, spend: &TransferRequiredSpendWitness) -> Result<()> {
    put_bytes(buf, &spend.nullifier);
    put_bytes(buf, &spend.spent_note_blinding);
    put_bytes(buf, &spend.spent_note_amount);
    put_bytes(buf, &spend.spent_note_asset_id);
    put_bytes(buf, &spend.spent_note_recovery_commitment);
    put_u64(buf, spend.state_commitment_position);
    encode_triple_path_32(buf, &spend.state_commitment_auth_path)?;
    put_bytes(buf, &spend.spend_auth_randomizer);
    encode_point_affine(buf, &spend.rk_affine);
    put_u8(buf, u8::from(spend.history_required));
    Ok(())
}

fn decode_required_spend(cursor: &mut BinaryCursor<'_>) -> Result<TransferRequiredSpendWitness> {
    Ok(TransferRequiredSpendWitness {
        nullifier: cursor.read_fixed::<32>()?,
        spent_note_blinding: cursor.read_fixed::<32>()?,
        spent_note_amount: cursor.read_fixed::<32>()?,
        spent_note_asset_id: cursor.read_fixed::<32>()?,
        spent_note_recovery_commitment: cursor.read_fixed::<32>()?,
        state_commitment_position: cursor.read_u64()?,
        state_commitment_auth_path: cursor.read_triple_path_32()?,
        spend_auth_randomizer: cursor.read_fr()?,
        rk_affine: cursor.read_point_affine()?,
        history_required: cursor.read_bool()?,
    })
}

fn encode_optional_spend(buf: &mut Vec<u8>, spend: &TransferOptionalSpendWitness) -> Result<()> {
    put_bytes(buf, &spend.nullifier);
    put_bytes(buf, &spend.spent_note_blinding);
    put_bytes(buf, &spend.spent_note_amount);
    put_bytes(buf, &spend.spent_note_recovery_commitment);
    put_u64(buf, spend.state_commitment_position);
    encode_triple_path_32(buf, &spend.state_commitment_auth_path)?;
    put_bytes(buf, &spend.spend_auth_randomizer);
    encode_point_affine(buf, &spend.rk_affine);
    put_u8(buf, u8::from(spend.is_dummy));
    put_bytes(buf, &spend.dummy_nullifier_seed);
    put_u8(buf, u8::from(spend.history_required));
    Ok(())
}

fn decode_optional_spend(cursor: &mut BinaryCursor<'_>) -> Result<TransferOptionalSpendWitness> {
    Ok(TransferOptionalSpendWitness {
        nullifier: cursor.read_fixed::<32>()?,
        spent_note_blinding: cursor.read_fixed::<32>()?,
        spent_note_amount: cursor.read_fixed::<32>()?,
        spent_note_recovery_commitment: cursor.read_fixed::<32>()?,
        state_commitment_position: cursor.read_u64()?,
        state_commitment_auth_path: cursor.read_triple_path_32()?,
        spend_auth_randomizer: cursor.read_fr()?,
        rk_affine: cursor.read_point_affine()?,
        is_dummy: cursor.read_bool()?,
        dummy_nullifier_seed: cursor.read_fixed::<32>()?,
        history_required: cursor.read_bool()?,
    })
}

fn encode_receiver_output(buf: &mut Vec<u8>, output: &TransferReceiverOutputWitness) -> Result<()> {
    put_bytes(buf, &output.note_commitment);
    put_bytes(buf, &output.recovery_commitment);
    put_bytes(buf, &output.created_note_blinding);
    put_bytes(buf, &output.created_note_amount);
    encode_recovery_capsule(buf, &output.recovery_capsule);
    encode_merkle_path(buf, &output.recipient_compliance_path)?;
    put_u64(buf, output.recipient_compliance_position);
    encode_point_affine(buf, &output.recipient_capk_affine);
    encode_point_affine(buf, &output.recipient_rnk_dh_pk_affine);
    put_bytes(buf, &output.recipient_rnk_commitment);
    put_bytes(buf, &output.recipient_status);
    encode_point_affine(buf, &output.recipient_diversified_generator_affine);
    encode_point_affine(buf, &output.recipient_transmission_key_affine);
    Ok(())
}

fn decode_receiver_output(cursor: &mut BinaryCursor<'_>) -> Result<TransferReceiverOutputWitness> {
    Ok(TransferReceiverOutputWitness {
        note_commitment: cursor.read_fixed::<32>()?,
        recovery_commitment: cursor.read_fixed::<32>()?,
        created_note_blinding: cursor.read_fixed::<32>()?,
        created_note_amount: cursor.read_fixed::<32>()?,
        recovery_capsule: decode_recovery_capsule(cursor)?,
        recipient_compliance_path: cursor.read_merkle_path()?,
        recipient_compliance_position: cursor.read_u64()?,
        recipient_capk_affine: cursor.read_point_affine()?,
        recipient_rnk_dh_pk_affine: cursor.read_point_affine()?,
        recipient_rnk_commitment: cursor.read_fixed::<32>()?,
        recipient_status: cursor.read_fixed::<32>()?,
        recipient_diversified_generator_affine: cursor.read_point_affine()?,
        recipient_transmission_key_affine: cursor.read_point_affine()?,
    })
}

fn encode_change_output(buf: &mut Vec<u8>, output: &TransferChangeOutputWitness) {
    put_bytes(buf, &output.note_commitment);
    put_bytes(buf, &output.recovery_commitment);
    put_bytes(buf, &output.created_note_blinding);
    put_bytes(buf, &output.created_note_amount);
    encode_recovery_capsule(buf, &output.recovery_capsule);
}

fn decode_change_output(cursor: &mut BinaryCursor<'_>) -> Result<TransferChangeOutputWitness> {
    Ok(TransferChangeOutputWitness {
        note_commitment: cursor.read_fixed::<32>()?,
        recovery_commitment: cursor.read_fixed::<32>()?,
        created_note_blinding: cursor.read_fixed::<32>()?,
        created_note_amount: cursor.read_fixed::<32>()?,
        recovery_capsule: decode_recovery_capsule(cursor)?,
    })
}
