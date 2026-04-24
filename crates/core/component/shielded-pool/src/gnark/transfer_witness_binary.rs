use anyhow::{bail, Context, Result};

use crate::{
    gnark::{
        binary::{encode_vec_32, put_bytes, put_u32, put_u64, put_u8, BinaryCursor},
        transfer_witness::{
            TransferComplianceCiphertextWitnessV1, TransferOutputWitnessV1, TransferSpendWitnessV1,
            TransferTierRandomizersWitnessV1, TransferWitnessV1,
        },
        typed::{
            decode_indexed_leaf, encode_indexed_leaf, encode_merkle_path, encode_point_affine,
        },
    },
    transfer::{transfer_input_count, transfer_output_count, TRANSFER_PROOF_LABEL},
};

const TRANSFER_WITNESS_MAGIC: &[u8; 4] = b"PTWG";
const TRANSFER_WITNESS_VERSION: u32 = 7;

impl TransferWitnessV1 {
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        put_bytes(&mut buf, TRANSFER_WITNESS_MAGIC);
        put_u32(&mut buf, TRANSFER_WITNESS_VERSION);
        put_u32(&mut buf, 0);
        put_u32(&mut buf, self.n_in);
        put_u32(&mut buf, self.n_out);
        put_bytes(&mut buf, &self.anchor);
        put_bytes(&mut buf, &self.balance_commitment);
        put_bytes(&mut buf, &self.asset_anchor);
        put_bytes(&mut buf, &self.compliance_anchor);
        put_bytes(&mut buf, &self.target_timestamp);
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
        put_bytes(&mut buf, &self.sender_d);
        put_bytes(&mut buf, &self.transfer_nonce_root);
        encode_vec_32(&mut buf, &self.detection_ciphertext)?;
        encode_compliance_tier(&mut buf, &self.sender_core)?;
        encode_compliance_tier(&mut buf, &self.sender_ext)?;
        encode_compliance_tier(&mut buf, &self.output_core)?;
        encode_compliance_tier(&mut buf, &self.output_ext)?;
        encode_randomizers(&mut buf, &self.sender_randomizers);
        encode_randomizers(&mut buf, &self.output_randomizers);

        for spend in &self.spends {
            encode_spend(&mut buf, spend)?;
        }
        for output in &self.outputs {
            encode_output(&mut buf, output)?;
        }

        encode_point_affine(&mut buf, &self.balance_commitment_affine);
        encode_point_affine(&mut buf, &self.ak_affine);
        encode_point_affine(&mut buf, &self.asset_indexed_leaf_dk_pub_affine);
        encode_point_affine(&mut buf, &self.asset_indexed_leaf_ring_pk_affine);
        encode_point_affine(&mut buf, &self.sender_diversified_generator_affine);
        encode_point_affine(&mut buf, &self.sender_transmission_key_affine);

        let total_len = u32::try_from(buf.len()).context("encoded transfer witness exceeds u32")?;
        buf[8..12].copy_from_slice(&total_len.to_le_bytes());
        Ok(buf)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut cursor = BinaryCursor::new(bytes);
        if cursor.read_fixed::<4>()? != *TRANSFER_WITNESS_MAGIC {
            bail!("invalid transfer witness magic");
        }
        let version = cursor.read_u32()?;
        if version != TRANSFER_WITNESS_VERSION {
            bail!("unsupported transfer witness version {version}");
        }
        let total_length = cursor.read_u32()?;
        if total_length as usize != bytes.len() {
            bail!(
                "transfer witness length mismatch: header={}, actual={}",
                total_length,
                bytes.len()
            );
        }

        let n_in = cursor.read_u32()?;
        let n_out = cursor.read_u32()?;
        if n_in as usize != transfer_input_count() || n_out as usize != transfer_output_count() {
            bail!(
                "{TRANSFER_PROOF_LABEL} witness shape mismatch: got {}x{}, expected {}x{}",
                n_in,
                n_out,
                transfer_input_count(),
                transfer_output_count()
            );
        }

        let anchor = cursor.read_fixed::<32>()?;
        let balance_commitment = cursor.read_fixed::<32>()?;
        let asset_anchor = cursor.read_fixed::<32>()?;
        let compliance_anchor = cursor.read_fixed::<32>()?;
        let target_timestamp = cursor.read_fixed::<32>()?;
        let claimed_statement_hash = cursor.read_fixed::<32>()?;
        let statement_fields = cursor.read_vec_32()?;
        let action_balance_blinding = cursor.read_fixed::<32>()?;
        let ak = cursor.read_fixed::<32>()?;
        let nk = cursor.read_fixed::<32>()?;
        let asset_path = cursor.read_merkle_path()?;
        let asset_position = cursor.read_u64()?;
        let asset_indexed_leaf = decode_indexed_leaf(&mut cursor)?;
        let is_regulated = cursor.read_u8()? != 0;
        let sender_compliance_path = cursor.read_merkle_path()?;
        let sender_compliance_position = cursor.read_u64()?;
        let sender_asset_id = cursor.read_fixed::<32>()?;
        let sender_d = cursor.read_fixed::<32>()?;
        let transfer_nonce_root = cursor.read_fixed::<32>()?;
        let detection_ciphertext = cursor.read_vec_32()?;
        let sender_core = decode_compliance_tier(&mut cursor)?;
        let sender_ext = decode_compliance_tier(&mut cursor)?;
        let output_core = decode_compliance_tier(&mut cursor)?;
        let output_ext = decode_compliance_tier(&mut cursor)?;
        let sender_randomizers = decode_randomizers(&mut cursor)?;
        let output_randomizers = decode_randomizers(&mut cursor)?;

        let spends = (0..n_in)
            .map(|_| decode_spend(&mut cursor))
            .collect::<Result<Vec<_>>>()?;
        let outputs = (0..n_out)
            .map(|_| decode_output(&mut cursor))
            .collect::<Result<Vec<_>>>()?;

        let witness = Self {
            total_length,
            n_in,
            n_out,
            anchor,
            balance_commitment,
            asset_anchor,
            compliance_anchor,
            target_timestamp,
            claimed_statement_hash,
            statement_fields,
            action_balance_blinding,
            ak,
            nk,
            asset_path,
            asset_position,
            asset_indexed_leaf,
            is_regulated,
            sender_compliance_path,
            sender_compliance_position,
            sender_asset_id,
            sender_d,
            transfer_nonce_root,
            detection_ciphertext,
            sender_core,
            sender_ext,
            output_core,
            output_ext,
            sender_randomizers,
            output_randomizers,
            spends,
            outputs,
            balance_commitment_affine: cursor.read_point_affine()?,
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

fn encode_compliance_tier(
    buf: &mut Vec<u8>,
    tier: &TransferComplianceCiphertextWitnessV1,
) -> Result<()> {
    put_bytes(buf, &tier.c2);
    encode_vec_32(buf, &tier.ciphertext)?;
    put_bytes(buf, &tier.subject_b_d);
    put_bytes(buf, &tier.ring_id_hash);
    put_bytes(buf, &tier.policy_id_hash);
    put_bytes(buf, &tier.resource_hash);
    put_bytes(buf, &tier.permission_hash);
    put_u64(buf, tier.tier);
    put_bytes(buf, &tier.statement_target_timestamp);
    put_bytes(buf, &tier.salt);
    put_bytes(buf, &tier.challenge);
    put_bytes(buf, &tier.response);
    encode_point_affine(buf, &tier.epk_affine);
    encode_point_affine(buf, &tier.derived_pk_affine);
    encode_point_affine(buf, &tier.enc_cmt_affine);
    encode_point_affine(buf, &tier.shared_point_affine);
    Ok(())
}

fn decode_compliance_tier(
    cursor: &mut BinaryCursor<'_>,
) -> Result<TransferComplianceCiphertextWitnessV1> {
    Ok(TransferComplianceCiphertextWitnessV1 {
        c2: cursor.read_fixed::<32>()?,
        ciphertext: cursor.read_vec_32()?,
        subject_b_d: cursor.read_fixed::<32>()?,
        ring_id_hash: cursor.read_fixed::<32>()?,
        policy_id_hash: cursor.read_fixed::<32>()?,
        resource_hash: cursor.read_fixed::<32>()?,
        permission_hash: cursor.read_fixed::<32>()?,
        tier: cursor.read_u64()?,
        statement_target_timestamp: cursor.read_fixed::<32>()?,
        salt: cursor.read_fixed::<32>()?,
        challenge: cursor.read_fixed::<32>()?,
        response: cursor.read_fixed::<32>()?,
        epk_affine: cursor.read_point_affine()?,
        derived_pk_affine: cursor.read_point_affine()?,
        enc_cmt_affine: cursor.read_point_affine()?,
        shared_point_affine: cursor.read_point_affine()?,
    })
}

fn encode_randomizers(buf: &mut Vec<u8>, randomizers: &TransferTierRandomizersWitnessV1) {
    put_bytes(buf, &randomizers.core);
    put_bytes(buf, &randomizers.ext);
}

fn decode_randomizers(cursor: &mut BinaryCursor<'_>) -> Result<TransferTierRandomizersWitnessV1> {
    Ok(TransferTierRandomizersWitnessV1 {
        core: cursor.read_fixed::<32>()?,
        ext: cursor.read_fixed::<32>()?,
    })
}

fn encode_spend(buf: &mut Vec<u8>, spend: &TransferSpendWitnessV1) -> Result<()> {
    put_bytes(buf, &spend.nullifier);
    put_bytes(buf, &spend.spent_note_blinding);
    put_bytes(buf, &spend.spent_note_amount);
    put_bytes(buf, &spend.spent_note_asset_id);
    put_bytes(buf, &spend.spent_transmission_key);
    put_bytes(buf, &spend.spent_clue_key);
    put_bytes(buf, &spend.state_commitment_commitment);
    put_u64(buf, spend.state_commitment_position);
    put_u32(
        buf,
        u32::try_from(spend.state_commitment_auth_path.len())
            .context("state commitment path length exceeds u32")?,
    );
    for siblings in &spend.state_commitment_auth_path {
        for sibling in siblings {
            put_bytes(buf, sibling);
        }
    }
    put_bytes(buf, &spend.spend_auth_randomizer);
    put_u8(buf, u8::from(spend.is_dummy));
    put_bytes(buf, &spend.dummy_nullifier_seed);
    put_bytes(buf, &spend.dummy_spend_auth_key);
    encode_point_affine(buf, &spend.rk_affine);
    encode_point_affine(buf, &spend.spent_diversified_generator_affine);
    encode_point_affine(buf, &spend.spent_transmission_key_affine);
    Ok(())
}

fn decode_spend(cursor: &mut BinaryCursor<'_>) -> Result<TransferSpendWitnessV1> {
    Ok(TransferSpendWitnessV1 {
        nullifier: cursor.read_fixed::<32>()?,
        spent_note_blinding: cursor.read_fixed::<32>()?,
        spent_note_amount: cursor.read_fixed::<32>()?,
        spent_note_asset_id: cursor.read_fixed::<32>()?,
        spent_transmission_key: cursor.read_fixed::<32>()?,
        spent_clue_key: cursor.read_fixed::<32>()?,
        state_commitment_commitment: cursor.read_fixed::<32>()?,
        state_commitment_position: cursor.read_u64()?,
        state_commitment_auth_path: {
            let path_len = cursor.read_u32()? as usize;
            let mut state_commitment_auth_path = Vec::with_capacity(path_len);
            for _ in 0..path_len {
                state_commitment_auth_path.push([
                    cursor.read_fixed::<32>()?,
                    cursor.read_fixed::<32>()?,
                    cursor.read_fixed::<32>()?,
                ]);
            }
            state_commitment_auth_path
        },
        spend_auth_randomizer: cursor.read_fixed::<32>()?,
        is_dummy: cursor.read_u8()? != 0,
        dummy_nullifier_seed: cursor.read_fixed::<32>()?,
        dummy_spend_auth_key: cursor.read_fixed::<32>()?,
        rk_affine: cursor.read_point_affine()?,
        spent_diversified_generator_affine: cursor.read_point_affine()?,
        spent_transmission_key_affine: cursor.read_point_affine()?,
    })
}

fn encode_output(buf: &mut Vec<u8>, output: &TransferOutputWitnessV1) -> Result<()> {
    put_bytes(buf, &output.note_commitment);
    put_bytes(buf, &output.created_note_blinding);
    put_bytes(buf, &output.created_note_amount);
    put_bytes(buf, &output.created_note_asset_id);
    put_bytes(buf, &output.created_transmission_key);
    put_bytes(buf, &output.created_clue_key);
    encode_merkle_path(buf, &output.recipient_compliance_path)?;
    put_u64(buf, output.recipient_compliance_position);
    put_bytes(buf, &output.recipient_asset_id);
    put_bytes(buf, &output.recipient_d);
    put_u8(buf, u8::from(output.is_receiver));
    encode_point_affine(buf, &output.created_diversified_generator_affine);
    encode_point_affine(buf, &output.created_transmission_key_affine);
    encode_point_affine(buf, &output.recipient_diversified_generator_affine);
    encode_point_affine(buf, &output.recipient_transmission_key_affine);
    Ok(())
}

fn decode_output(cursor: &mut BinaryCursor<'_>) -> Result<TransferOutputWitnessV1> {
    Ok(TransferOutputWitnessV1 {
        note_commitment: cursor.read_fixed::<32>()?,
        created_note_blinding: cursor.read_fixed::<32>()?,
        created_note_amount: cursor.read_fixed::<32>()?,
        created_note_asset_id: cursor.read_fixed::<32>()?,
        created_transmission_key: cursor.read_fixed::<32>()?,
        created_clue_key: cursor.read_fixed::<32>()?,
        recipient_compliance_path: cursor.read_merkle_path()?,
        recipient_compliance_position: cursor.read_u64()?,
        recipient_asset_id: cursor.read_fixed::<32>()?,
        recipient_d: cursor.read_fixed::<32>()?,
        is_receiver: cursor.read_u8()? != 0,
        created_diversified_generator_affine: cursor.read_point_affine()?,
        created_transmission_key_affine: cursor.read_point_affine()?,
        recipient_diversified_generator_affine: cursor.read_point_affine()?,
        recipient_transmission_key_affine: cursor.read_point_affine()?,
    })
}
