use anyhow::{bail, Context, Result};

use crate::{
    gnark::{
        binary::{encode_triple_path_32, put_bytes, put_u32, put_u64, put_u8, BinaryCursor},
        recovery_capsule_witness_binary::{decode_recovery_capsule, encode_recovery_capsule},
        shielded_ics20_withdrawal_witness::{
            ShieldedIcs20WithdrawalChangeWitness, ShieldedIcs20WithdrawalOptionalSpendWitness,
            ShieldedIcs20WithdrawalRequiredSpendWitness, ShieldedIcs20WithdrawalWitness,
        },
        transfer_witness_binary::{decode_volume_accumulator, encode_volume_accumulator},
        typed::{
            decode_indexed_leaf, encode_indexed_leaf, encode_merkle_path, encode_point_affine,
        },
    },
    ShieldedIcs20WithdrawalFamilyId,
};

const SHIELDED_ICS20_WITHDRAWAL_WITNESS_MAGIC: &[u8; 4] = b"PIWG";

impl ShieldedIcs20WithdrawalWitness {
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        put_bytes(&mut buf, SHIELDED_ICS20_WITHDRAWAL_WITNESS_MAGIC);
        put_u32(&mut buf, 0);
        put_u32(&mut buf, self.family_id.get());
        put_u32(&mut buf, self.n_in);
        put_bytes(&mut buf, &self.anchor);
        put_bytes(&mut buf, &self.asset_anchor);
        put_bytes(&mut buf, &self.compliance_anchor);
        put_bytes(&mut buf, &self.target_timestamp);
        put_bytes(&mut buf, &self.outbound_asset_id);
        put_bytes(&mut buf, &self.outbound_amount);
        for limb in &self.withdrawal_effect_hash_limbs {
            put_bytes(&mut buf, limb);
        }
        put_bytes(&mut buf, &self.claimed_statement_hash);
        put_bytes(&mut buf, &self.routing_tag);
        put_bytes(&mut buf, &self.routing_parameter_set_id);
        encode_point_affine(&mut buf, &self.withdrawal_epk_affine);
        put_bytes(&mut buf, &self.withdrawal_c2);
        put_bytes(&mut buf, &self.withdrawal_key_confirmation);
        for word in &self.withdrawal_encrypted_sender_address {
            put_bytes(&mut buf, word);
        }
        put_bytes(&mut buf, &self.recent_position_floor);
        encode_volume_accumulator(&mut buf, &self.volume_accumulator)?;
        put_bytes(&mut buf, &self.volume_accumulator_seed);
        put_bytes(&mut buf, &self.action_balance_blinding);
        put_bytes(&mut buf, &self.nk);
        encode_merkle_path(&mut buf, &self.asset_path)?;
        put_u64(&mut buf, self.asset_position);
        encode_indexed_leaf(&mut buf, &self.asset_indexed_leaf);
        put_u8(&mut buf, u8::from(self.is_regulated));
        put_u8(&mut buf, self.regulated_precision);
        put_u8(&mut buf, self.unregulated_precision);
        put_u64(&mut buf, self.routing_as_of_height);
        put_bytes(&mut buf, &self.routing_nonce);
        encode_merkle_path(&mut buf, &self.sender_compliance_path)?;
        put_u64(&mut buf, self.sender_compliance_position);
        encode_point_affine(&mut buf, &self.sender_capk_affine);
        encode_point_affine(&mut buf, &self.sender_rnk_dh_pk_affine);
        put_bytes(&mut buf, &self.sender_rnk_commitment);
        put_bytes(&mut buf, &self.sender_status);
        put_bytes(&mut buf, &self.withdrawal_seed);
        put_bytes(&mut buf, &self.withdrawal_randomizer);
        encode_required_spend(&mut buf, &self.required_spend)?;
        encode_optional_spend(&mut buf, &self.optional_spend)?;
        encode_change_output(&mut buf, &self.change_output);
        encode_point_affine(&mut buf, &self.ak_affine);
        encode_point_affine(&mut buf, &self.asset_indexed_leaf_dk_pub_affine);
        encode_point_affine(&mut buf, &self.asset_indexed_leaf_ring_pk_affine);
        encode_point_affine(&mut buf, &self.sender_diversified_generator_affine);

        let total_len = u32::try_from(buf.len())
            .context("encoded shielded ICS-20 withdrawal witness exceeds u32")?;
        buf[4..8].copy_from_slice(&total_len.to_le_bytes());
        Ok(buf)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut cursor = BinaryCursor::new(bytes);
        if cursor.read_fixed::<4>()? != *SHIELDED_ICS20_WITHDRAWAL_WITNESS_MAGIC {
            bail!("invalid shielded ICS-20 withdrawal witness magic");
        }
        let total_length = cursor.read_u32()?;
        if total_length as usize != bytes.len() {
            bail!(
                "shielded ICS-20 withdrawal witness length mismatch: header={}, actual={}",
                total_length,
                bytes.len()
            );
        }

        let family_id = ShieldedIcs20WithdrawalFamilyId::try_from(cursor.read_u32()?)?;
        let n_in = cursor.read_u32()?;
        if n_in as usize != family_id.input_count() {
            bail!(
                "{} witness shape mismatch: got {} inputs, expected {}",
                family_id.label(),
                n_in,
                family_id.input_count()
            );
        }

        let witness = Self {
            family_id,
            total_length,
            n_in,
            anchor: cursor.read_fixed::<32>()?,
            asset_anchor: cursor.read_fixed::<32>()?,
            compliance_anchor: cursor.read_fixed::<32>()?,
            target_timestamp: cursor.read_fixed::<32>()?,
            outbound_asset_id: cursor.read_fixed::<32>()?,
            outbound_amount: cursor.read_fixed::<32>()?,
            withdrawal_effect_hash_limbs: [
                cursor.read_u128_field()?,
                cursor.read_u128_field()?,
                cursor.read_u128_field()?,
                cursor.read_u128_field()?,
            ],
            claimed_statement_hash: cursor.read_fixed::<32>()?,
            routing_tag: cursor.read_fixed::<32>()?,
            routing_parameter_set_id: cursor.read_fixed::<32>()?,
            withdrawal_epk_affine: cursor.read_point_affine()?,
            withdrawal_c2: cursor.read_fixed::<32>()?,
            withdrawal_key_confirmation: cursor.read_fixed::<32>()?,
            withdrawal_encrypted_sender_address: [
                cursor.read_fixed::<32>()?,
                cursor.read_fixed::<32>()?,
                cursor.read_fixed::<32>()?,
            ],
            recent_position_floor: cursor.read_fixed::<32>()?,
            volume_accumulator: decode_volume_accumulator(&mut cursor)?,
            volume_accumulator_seed: cursor.read_fixed::<32>()?,
            action_balance_blinding: cursor.read_fr()?,
            nk: cursor.read_fixed::<32>()?,
            asset_path: cursor.read_merkle_path()?,
            asset_position: cursor.read_u64()?,
            asset_indexed_leaf: decode_indexed_leaf(&mut cursor)?,
            is_regulated: cursor.read_bool()?,
            regulated_precision: cursor.read_u8()?,
            unregulated_precision: cursor.read_u8()?,
            routing_as_of_height: cursor.read_u64()?,
            routing_nonce: cursor.read_fixed::<32>()?,
            sender_compliance_path: cursor.read_merkle_path()?,
            sender_compliance_position: cursor.read_u64()?,
            sender_capk_affine: cursor.read_point_affine()?,
            sender_rnk_dh_pk_affine: cursor.read_point_affine()?,
            sender_rnk_commitment: cursor.read_fixed::<32>()?,
            sender_status: cursor.read_fixed::<32>()?,
            withdrawal_seed: cursor.read_fixed::<32>()?,
            withdrawal_randomizer: cursor.read_fr()?,
            required_spend: decode_required_spend(&mut cursor)?,
            optional_spend: decode_optional_spend(&mut cursor)?,
            change_output: decode_change_output(&mut cursor)?,
            ak_affine: cursor.read_point_affine()?,
            asset_indexed_leaf_dk_pub_affine: cursor.read_point_affine()?,
            asset_indexed_leaf_ring_pk_affine: cursor.read_point_affine()?,
            sender_diversified_generator_affine: cursor.read_point_affine()?,
        };

        cursor.finish(family_id.label())?;
        Ok(witness)
    }
}

fn encode_required_spend(
    buf: &mut Vec<u8>,
    spend: &ShieldedIcs20WithdrawalRequiredSpendWitness,
) -> Result<()> {
    put_bytes(buf, &spend.nullifier);
    put_bytes(buf, &spend.spent_note_blinding);
    put_bytes(buf, &spend.spent_note_amount);
    put_bytes(buf, &spend.spent_note_recovery_commitment);
    put_u64(buf, spend.state_commitment_position);
    encode_triple_path_32(buf, &spend.state_commitment_auth_path)?;
    put_bytes(buf, &spend.spend_auth_randomizer);
    encode_point_affine(buf, &spend.rk_affine);
    put_u8(buf, u8::from(spend.history_required));
    Ok(())
}

fn decode_required_spend(
    cursor: &mut BinaryCursor<'_>,
) -> Result<ShieldedIcs20WithdrawalRequiredSpendWitness> {
    Ok(ShieldedIcs20WithdrawalRequiredSpendWitness {
        nullifier: cursor.read_fixed::<32>()?,
        spent_note_blinding: cursor.read_fixed::<32>()?,
        spent_note_amount: cursor.read_fixed::<32>()?,
        spent_note_recovery_commitment: cursor.read_fixed::<32>()?,
        state_commitment_position: cursor.read_u64()?,
        state_commitment_auth_path: cursor.read_triple_path_32()?,
        spend_auth_randomizer: cursor.read_fr()?,
        rk_affine: cursor.read_point_affine()?,
        history_required: cursor.read_bool()?,
    })
}

fn encode_optional_spend(
    buf: &mut Vec<u8>,
    spend: &ShieldedIcs20WithdrawalOptionalSpendWitness,
) -> Result<()> {
    encode_required_spend(buf, &spend.spend)?;
    put_u8(buf, u8::from(spend.is_dummy));
    put_bytes(buf, &spend.dummy_nullifier_seed);
    Ok(())
}

fn decode_optional_spend(
    cursor: &mut BinaryCursor<'_>,
) -> Result<ShieldedIcs20WithdrawalOptionalSpendWitness> {
    Ok(ShieldedIcs20WithdrawalOptionalSpendWitness {
        spend: decode_required_spend(cursor)?,
        is_dummy: cursor.read_bool()?,
        dummy_nullifier_seed: cursor.read_fixed::<32>()?,
    })
}

fn encode_change_output(buf: &mut Vec<u8>, output: &ShieldedIcs20WithdrawalChangeWitness) {
    put_bytes(buf, &output.note_commitment);
    put_bytes(buf, &output.recovery_commitment);
    put_bytes(buf, &output.created_note_blinding);
    put_bytes(buf, &output.created_note_amount);
    encode_recovery_capsule(buf, &output.recovery_capsule);
}

fn decode_change_output(
    cursor: &mut BinaryCursor<'_>,
) -> Result<ShieldedIcs20WithdrawalChangeWitness> {
    Ok(ShieldedIcs20WithdrawalChangeWitness {
        note_commitment: cursor.read_fixed::<32>()?,
        recovery_commitment: cursor.read_fixed::<32>()?,
        created_note_blinding: cursor.read_fixed::<32>()?,
        created_note_amount: cursor.read_fixed::<32>()?,
        recovery_capsule: decode_recovery_capsule(cursor)?,
    })
}
