use anyhow::{bail, Context, Result};

use crate::gnark::{
    binary::{encode_triple_path_32, put_bytes, put_u32, put_u64, put_u8, BinaryCursor},
    note_reshape_witness::{
        NoteReshapeOutputWitness, NoteReshapeSharedNoteContextWitness, NoteReshapeSpendWitness,
        NoteReshapeWitness,
    },
    recovery_capsule_witness_binary::{decode_recovery_capsule, encode_recovery_capsule},
    typed::{decode_indexed_leaf, encode_indexed_leaf, encode_merkle_path, encode_point_affine},
};

const NOTE_RESHAPE_WITNESS_MAGIC: &[u8; 4] = b"PNWG";

impl NoteReshapeWitness {
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        put_bytes(&mut buf, NOTE_RESHAPE_WITNESS_MAGIC);
        put_u32(&mut buf, 0);
        put_u32(&mut buf, self.family_id.get());
        put_u32(&mut buf, self.n_in);
        put_u32(&mut buf, self.n_out);
        put_bytes(&mut buf, &self.anchor);
        put_bytes(&mut buf, &self.claimed_statement_hash);
        put_bytes(&mut buf, &self.asset_anchor);
        put_bytes(&mut buf, &self.compliance_anchor);
        put_bytes(&mut buf, &self.routing_tag);
        put_bytes(&mut buf, &self.routing_parameter_set_id);
        put_bytes(&mut buf, &self.recent_position_floor);
        put_bytes(&mut buf, &self.action_balance_blinding);
        put_bytes(&mut buf, &self.nk);
        encode_merkle_path(&mut buf, &self.asset_path)?;
        put_u64(&mut buf, self.asset_position);
        encode_indexed_leaf(&mut buf, &self.asset_indexed_leaf);
        encode_point_affine(&mut buf, &self.asset_indexed_leaf_dk_pub_affine);
        encode_point_affine(&mut buf, &self.asset_indexed_leaf_ring_pk_affine);
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
        put_bytes(&mut buf, &self.shared.asset_id);
        encode_point_affine(&mut buf, &self.shared.diversified_generator_affine);
        for spend in &self.spends {
            encode_spend(
                &mut buf,
                spend,
                self.family_id.spec().input_padding
                    == crate::note_reshape::InputPaddingPolicy::SyntheticPrivate,
            )?;
        }
        for output in &self.outputs {
            encode_output(&mut buf, output);
        }
        encode_point_affine(&mut buf, &self.balance_commitment_affine);
        encode_point_affine(&mut buf, &self.ak_affine);

        let total_len =
            u32::try_from(buf.len()).context("encoded note reshape witness exceeds u32")?;
        buf[4..8].copy_from_slice(&total_len.to_le_bytes());
        Ok(buf)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut cursor = BinaryCursor::new(bytes);
        if cursor.read_fixed::<4>()? != *NOTE_RESHAPE_WITNESS_MAGIC {
            bail!("invalid note reshape witness magic")
        }
        let total_length = cursor.read_u32()?;
        if total_length as usize != bytes.len() {
            bail!(
                "note reshape witness length mismatch: header={}, actual={}",
                total_length,
                bytes.len()
            )
        }
        let family_id = crate::NoteReshapeFamilyId::try_from(cursor.read_u32()?)?;
        let n_in = cursor.read_u32()?;
        let n_out = cursor.read_u32()?;
        let anchor = cursor.read_fixed::<32>()?;
        let claimed_statement_hash = cursor.read_fixed::<32>()?;
        let asset_anchor = cursor.read_fixed::<32>()?;
        let compliance_anchor = cursor.read_fixed::<32>()?;
        let routing_tag = cursor.read_fixed::<32>()?;
        let routing_parameter_set_id = cursor.read_fixed::<32>()?;
        let recent_position_floor = cursor.read_fixed::<32>()?;
        let action_balance_blinding = cursor.read_fr()?;
        let nk = cursor.read_fixed::<32>()?;
        let asset_path = cursor.read_merkle_path()?;
        let asset_position = cursor.read_u64()?;
        let asset_indexed_leaf = decode_indexed_leaf(&mut cursor)?;
        let asset_indexed_leaf_dk_pub_affine = cursor.read_point_affine()?;
        let asset_indexed_leaf_ring_pk_affine = cursor.read_point_affine()?;
        let is_regulated = cursor.read_bool()?;
        let regulated_precision = cursor.read_u8()?;
        let unregulated_precision = cursor.read_u8()?;
        let routing_as_of_height = cursor.read_u64()?;
        let routing_nonce = cursor.read_fixed::<32>()?;
        let sender_compliance_path = cursor.read_merkle_path()?;
        let sender_compliance_position = cursor.read_u64()?;
        let sender_capk_affine = cursor.read_point_affine()?;
        let sender_rnk_dh_pk_affine = cursor.read_point_affine()?;
        let sender_rnk_commitment = cursor.read_fixed::<32>()?;
        let sender_status = cursor.read_fixed::<32>()?;
        let shared = NoteReshapeSharedNoteContextWitness {
            asset_id: cursor.read_fixed::<32>()?,
            diversified_generator_affine: cursor.read_point_affine()?,
        };
        if n_in as usize != family_id.input_count() || n_out as usize != family_id.output_count() {
            bail!(
                "{} witness shape mismatch: got {}x{}, expected {}x{}",
                family_id.label(),
                n_in,
                n_out,
                family_id.input_count(),
                family_id.output_count()
            )
        }
        let spends = (0..n_in)
            .map(|_| {
                decode_spend(
                    &mut cursor,
                    family_id.spec().input_padding
                        == crate::note_reshape::InputPaddingPolicy::SyntheticPrivate,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let outputs = (0..n_out)
            .map(|_| decode_output(&mut cursor))
            .collect::<Result<Vec<_>>>()?;
        let balance_commitment_affine = cursor.read_point_affine()?;
        let ak_affine = cursor.read_point_affine()?;
        cursor.finish("note reshape witness")?;

        Ok(Self {
            family_id,
            total_length,
            n_in,
            n_out,
            anchor,
            claimed_statement_hash,
            asset_anchor,
            compliance_anchor,
            routing_tag,
            routing_parameter_set_id,
            recent_position_floor,
            action_balance_blinding,
            nk,
            asset_path,
            asset_position,
            asset_indexed_leaf,
            asset_indexed_leaf_dk_pub_affine,
            asset_indexed_leaf_ring_pk_affine,
            is_regulated,
            regulated_precision,
            unregulated_precision,
            routing_as_of_height,
            routing_nonce,
            sender_compliance_path,
            sender_compliance_position,
            sender_capk_affine,
            sender_rnk_dh_pk_affine,
            sender_rnk_commitment,
            sender_status,
            shared,
            spends,
            outputs,
            balance_commitment_affine,
            ak_affine,
        })
    }
}

fn encode_spend(
    buf: &mut Vec<u8>,
    spend: &NoteReshapeSpendWitness,
    synthetic_private_padding: bool,
) -> Result<()> {
    if synthetic_private_padding {
        put_u32(buf, if spend.is_dummy { 1 } else { 0 });
        put_bytes(buf, &spend.dummy_nullifier_seed);
    }
    put_bytes(buf, &spend.nullifier);
    put_bytes(buf, &spend.spent_note_blinding);
    put_bytes(buf, &spend.spent_note_amount);
    put_bytes(buf, &spend.spent_note_recovery_commitment);
    put_bytes(buf, &spend.state_commitment_commitment);
    put_u64(buf, spend.state_commitment_position);
    encode_triple_path_32(buf, &spend.state_commitment_auth_path)?;
    put_bytes(buf, &spend.spend_auth_randomizer);
    encode_point_affine(buf, &spend.rk_affine);
    put_u8(buf, u8::from(spend.history_required));
    Ok(())
}

fn decode_spend(
    cursor: &mut BinaryCursor<'_>,
    synthetic_private_padding: bool,
) -> Result<NoteReshapeSpendWitness> {
    let (is_dummy, dummy_nullifier_seed) = if synthetic_private_padding {
        let is_dummy = match cursor.read_u32()? {
            0 => false,
            1 => true,
            value => bail!("invalid note reshape input dummy flag {value}"),
        };
        (is_dummy, cursor.read_fixed::<32>()?)
    } else {
        (false, [0u8; 32])
    };
    Ok(NoteReshapeSpendWitness {
        is_dummy,
        nullifier: cursor.read_fixed::<32>()?,
        dummy_nullifier_seed,
        spent_note_blinding: cursor.read_fixed::<32>()?,
        spent_note_amount: cursor.read_fixed::<32>()?,
        spent_note_recovery_commitment: cursor.read_fixed::<32>()?,
        state_commitment_commitment: cursor.read_fixed::<32>()?,
        state_commitment_position: cursor.read_u64()?,
        state_commitment_auth_path: cursor.read_triple_path_32()?,
        spend_auth_randomizer: cursor.read_fr()?,
        rk_affine: cursor.read_point_affine()?,
        history_required: cursor.read_bool()?,
    })
}

fn encode_output(buf: &mut Vec<u8>, output: &NoteReshapeOutputWitness) {
    put_bytes(buf, &output.note_commitment);
    put_bytes(buf, &output.recovery_commitment);
    put_bytes(buf, &output.created_note_blinding);
    put_bytes(buf, &output.created_note_amount);
    encode_recovery_capsule(buf, &output.recovery_capsule);
}

fn decode_output(cursor: &mut BinaryCursor<'_>) -> Result<NoteReshapeOutputWitness> {
    Ok(NoteReshapeOutputWitness {
        note_commitment: cursor.read_fixed::<32>()?,
        recovery_commitment: cursor.read_fixed::<32>()?,
        created_note_blinding: cursor.read_fixed::<32>()?,
        created_note_amount: cursor.read_fixed::<32>()?,
        recovery_capsule: decode_recovery_capsule(cursor)?,
    })
}
