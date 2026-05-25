use anyhow::{bail, Context, Result};

use crate::{
    gnark::{
        binary::{
            encode_triple_path_32, encode_vec_32, put_bytes, put_u32, put_u64, put_u8, BinaryCursor,
        },
        shielded_ics20_withdrawal_witness::{
            ShieldedIcs20WithdrawalChangeWitnessV1, ShieldedIcs20WithdrawalSpendWitnessV1,
            ShieldedIcs20WithdrawalWitnessV1,
        },
        typed::{
            decode_indexed_leaf, encode_indexed_leaf, encode_merkle_path, encode_point_affine,
        },
    },
    ShieldedIcs20WithdrawalFamilyId,
};

const SHIELDED_ICS20_WITHDRAWAL_WITNESS_MAGIC: &[u8; 4] = b"PIWG";
const SHIELDED_ICS20_WITHDRAWAL_WITNESS_VERSION: u32 = 2;

impl ShieldedIcs20WithdrawalWitnessV1 {
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        put_bytes(&mut buf, SHIELDED_ICS20_WITHDRAWAL_WITNESS_MAGIC);
        put_u32(&mut buf, SHIELDED_ICS20_WITHDRAWAL_WITNESS_VERSION);
        put_u32(&mut buf, 0);
        put_u32(&mut buf, self.family_id.get());
        put_u32(&mut buf, self.n_in);
        put_bytes(&mut buf, &self.anchor);
        put_bytes(&mut buf, &self.balance_commitment);
        put_bytes(&mut buf, &self.asset_anchor);
        put_bytes(&mut buf, &self.compliance_anchor);
        put_bytes(&mut buf, &self.target_timestamp);
        put_bytes(&mut buf, &self.outbound_asset_id);
        put_bytes(&mut buf, &self.outbound_amount);
        put_bytes(&mut buf, &self.withdrawal_effect_hash_lo);
        put_bytes(&mut buf, &self.withdrawal_effect_hash_hi);
        put_bytes(&mut buf, &self.claimed_statement_hash);
        encode_vec_32(&mut buf, &self.statement_fields)?;
        put_bytes(&mut buf, &self.action_balance_blinding);
        put_bytes(&mut buf, &self.ak);
        put_bytes(&mut buf, &self.nk);
        encode_merkle_path(&mut buf, &self.asset_path)?;
        put_u64(&mut buf, self.asset_position);
        encode_indexed_leaf(&mut buf, &self.asset_indexed_leaf);
        put_u8(&mut buf, u8::from(self.is_regulated));
        encode_merkle_path(&mut buf, &self.sender_compliance_path)?;
        put_u64(&mut buf, self.sender_compliance_position);
        put_bytes(&mut buf, &self.sender_asset_id);
        put_bytes(&mut buf, &self.sender_slot_id);
        put_bytes(&mut buf, &self.sender_slot_derivation);
        put_bytes(&mut buf, &self.sender_d);
        for spend in &self.spends {
            encode_spend(&mut buf, spend)?;
        }
        encode_change_output(&mut buf, &self.change_output);
        encode_point_affine(&mut buf, &self.balance_commitment_affine);
        encode_point_affine(&mut buf, &self.ak_affine);
        encode_point_affine(&mut buf, &self.asset_indexed_leaf_dk_pub_affine);
        encode_point_affine(&mut buf, &self.asset_indexed_leaf_ring_pk_affine);
        encode_point_affine(&mut buf, &self.sender_diversified_generator_affine);
        encode_point_affine(&mut buf, &self.sender_transmission_key_affine);

        let total_len = u32::try_from(buf.len())
            .context("encoded shielded ICS-20 withdrawal witness exceeds u32")?;
        buf[8..12].copy_from_slice(&total_len.to_le_bytes());
        Ok(buf)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut cursor = BinaryCursor::new(bytes);
        if cursor.read_fixed::<4>()? != *SHIELDED_ICS20_WITHDRAWAL_WITNESS_MAGIC {
            bail!("invalid shielded ICS-20 withdrawal witness magic");
        }
        let version = cursor.read_u32()?;
        if version != SHIELDED_ICS20_WITHDRAWAL_WITNESS_VERSION {
            bail!("unsupported shielded ICS-20 withdrawal witness version {version}");
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
            balance_commitment: cursor.read_fixed::<32>()?,
            asset_anchor: cursor.read_fixed::<32>()?,
            compliance_anchor: cursor.read_fixed::<32>()?,
            target_timestamp: cursor.read_fixed::<32>()?,
            outbound_asset_id: cursor.read_fixed::<32>()?,
            outbound_amount: cursor.read_fixed::<32>()?,
            withdrawal_effect_hash_lo: cursor.read_fixed::<32>()?,
            withdrawal_effect_hash_hi: cursor.read_fixed::<32>()?,
            claimed_statement_hash: cursor.read_fixed::<32>()?,
            statement_fields: cursor.read_vec_32()?,
            action_balance_blinding: cursor.read_fixed::<32>()?,
            ak: cursor.read_fixed::<32>()?,
            nk: cursor.read_fixed::<32>()?,
            asset_path: cursor.read_merkle_path()?,
            asset_position: cursor.read_u64()?,
            asset_indexed_leaf: decode_indexed_leaf(&mut cursor)?,
            is_regulated: cursor.read_u8()? != 0,
            sender_compliance_path: cursor.read_merkle_path()?,
            sender_compliance_position: cursor.read_u64()?,
            sender_asset_id: cursor.read_fixed::<32>()?,
            sender_slot_id: cursor.read_fixed::<32>()?,
            sender_slot_derivation: cursor.read_fixed::<32>()?,
            sender_d: cursor.read_fixed::<32>()?,
            spends: (0..n_in)
                .map(|_| decode_spend(&mut cursor))
                .collect::<Result<Vec<_>>>()?,
            change_output: decode_change_output(&mut cursor)?,
            balance_commitment_affine: cursor.read_point_affine()?,
            ak_affine: cursor.read_point_affine()?,
            asset_indexed_leaf_dk_pub_affine: cursor.read_point_affine()?,
            asset_indexed_leaf_ring_pk_affine: cursor.read_point_affine()?,
            sender_diversified_generator_affine: cursor.read_point_affine()?,
            sender_transmission_key_affine: cursor.read_point_affine()?,
        };

        cursor.finish(family_id.label())?;
        Ok(witness)
    }
}

fn encode_spend(buf: &mut Vec<u8>, spend: &ShieldedIcs20WithdrawalSpendWitnessV1) -> Result<()> {
    put_bytes(buf, &spend.nullifier);
    put_bytes(buf, &spend.spent_note_blinding);
    put_bytes(buf, &spend.spent_note_amount);
    put_bytes(buf, &spend.spent_note_asset_id);
    put_bytes(buf, &spend.spent_transmission_key);
    put_bytes(buf, &spend.spent_clue_key);
    put_bytes(buf, &spend.state_commitment_commitment);
    put_u64(buf, spend.state_commitment_position);
    encode_triple_path_32(buf, &spend.state_commitment_auth_path)?;
    put_bytes(buf, &spend.spend_auth_randomizer);
    put_u8(buf, u8::from(spend.is_dummy));
    put_bytes(buf, &spend.dummy_nullifier_seed);
    put_bytes(buf, &spend.dummy_spend_auth_key);
    encode_point_affine(buf, &spend.rk_affine);
    encode_point_affine(buf, &spend.spent_diversified_generator_affine);
    encode_point_affine(buf, &spend.spent_transmission_key_affine);
    Ok(())
}

fn decode_spend(cursor: &mut BinaryCursor<'_>) -> Result<ShieldedIcs20WithdrawalSpendWitnessV1> {
    Ok(ShieldedIcs20WithdrawalSpendWitnessV1 {
        nullifier: cursor.read_fixed::<32>()?,
        spent_note_blinding: cursor.read_fixed::<32>()?,
        spent_note_amount: cursor.read_fixed::<32>()?,
        spent_note_asset_id: cursor.read_fixed::<32>()?,
        spent_transmission_key: cursor.read_fixed::<32>()?,
        spent_clue_key: cursor.read_fixed::<32>()?,
        state_commitment_commitment: cursor.read_fixed::<32>()?,
        state_commitment_position: cursor.read_u64()?,
        state_commitment_auth_path: cursor.read_triple_path_32()?,
        spend_auth_randomizer: cursor.read_fixed::<32>()?,
        is_dummy: cursor.read_u8()? != 0,
        dummy_nullifier_seed: cursor.read_fixed::<32>()?,
        dummy_spend_auth_key: cursor.read_fixed::<32>()?,
        rk_affine: cursor.read_point_affine()?,
        spent_diversified_generator_affine: cursor.read_point_affine()?,
        spent_transmission_key_affine: cursor.read_point_affine()?,
    })
}

fn encode_change_output(buf: &mut Vec<u8>, output: &ShieldedIcs20WithdrawalChangeWitnessV1) {
    put_bytes(buf, &output.note_commitment);
    put_bytes(buf, &output.created_note_blinding);
    put_bytes(buf, &output.created_note_amount);
    put_bytes(buf, &output.created_note_asset_id);
    put_bytes(buf, &output.created_transmission_key);
    put_bytes(buf, &output.created_clue_key);
    encode_point_affine(buf, &output.created_diversified_generator_affine);
    encode_point_affine(buf, &output.created_transmission_key_affine);
}

fn decode_change_output(
    cursor: &mut BinaryCursor<'_>,
) -> Result<ShieldedIcs20WithdrawalChangeWitnessV1> {
    Ok(ShieldedIcs20WithdrawalChangeWitnessV1 {
        note_commitment: cursor.read_fixed::<32>()?,
        created_note_blinding: cursor.read_fixed::<32>()?,
        created_note_amount: cursor.read_fixed::<32>()?,
        created_note_asset_id: cursor.read_fixed::<32>()?,
        created_transmission_key: cursor.read_fixed::<32>()?,
        created_clue_key: cursor.read_fixed::<32>()?,
        created_diversified_generator_affine: cursor.read_point_affine()?,
        created_transmission_key_affine: cursor.read_point_affine()?,
    })
}
