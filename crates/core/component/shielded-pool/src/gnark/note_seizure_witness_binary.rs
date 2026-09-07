use anyhow::{bail, Context, Result};

use crate::gnark::{
    binary::{encode_triple_path_32, put_bytes, put_u32, put_u64, put_u8, BinaryCursor},
    note_seizure_witness::{NoteSeizureRecoveryWitness, NoteSeizureWitness},
    typed::encode_point_affine,
};

const NOTE_SEIZURE_WITNESS_MAGIC: &[u8; 4] = b"NSZW";

impl NoteSeizureWitness {
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        put_bytes(&mut buf, NOTE_SEIZURE_WITNESS_MAGIC);
        put_u32(&mut buf, 0);
        for value in [
            &self.claimed_statement_hash,
            &self.anchor,
            &self.note_commitment,
            &self.nullifier,
        ] {
            put_bytes(&mut buf, value);
        }
        put_u8(&mut buf, u8::from(self.history_required));
        for value in [
            &self.recent_position_floor,
            &self.address_diversified_generator,
            &self.address_transmission_key,
            &self.asset_id,
            &self.amount,
            &self.authorization_commitment,
            &self.recovery.commitment,
        ] {
            put_bytes(&mut buf, value);
        }
        encode_point_affine(&mut buf, &self.recovery.epk_affine);
        for value in [
            &self.recovery.c2,
            &self.recovery.salt,
            &self.recovery.key_confirmation,
            &self.recovery.encrypted_amount,
            &self.recovery.encrypted_note_blinding,
            &self.recovery.seed,
            &self.note_blinding,
        ] {
            put_bytes(&mut buf, value);
        }
        put_u64(&mut buf, self.position);
        encode_triple_path_32(&mut buf, &self.state_commitment_auth_path)?;
        put_bytes(&mut buf, &self.rnk);
        put_bytes(&mut buf, &self.rnk_commitment);

        let total_length = u32::try_from(buf.len()).context("note seizure witness exceeds u32")?;
        buf[4..8].copy_from_slice(&total_length.to_le_bytes());
        Ok(buf)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut cursor = BinaryCursor::new(bytes);
        if cursor.read_fixed::<4>()? != *NOTE_SEIZURE_WITNESS_MAGIC {
            bail!("invalid note seizure witness magic")
        }
        let total_length = cursor.read_u32()?;
        ensure_length(total_length, bytes.len())?;
        let claimed_statement_hash = cursor.read_fixed::<32>()?;
        let anchor = cursor.read_fixed::<32>()?;
        let note_commitment = cursor.read_fixed::<32>()?;
        let nullifier = cursor.read_fixed::<32>()?;
        let history_required = cursor.read_bool()?;
        let recent_position_floor = cursor.read_fixed::<32>()?;
        let address_diversified_generator = cursor.read_fixed::<32>()?;
        let address_transmission_key = cursor.read_fixed::<32>()?;
        let asset_id = cursor.read_fixed::<32>()?;
        let amount = cursor.read_u128_field()?;
        let authorization_commitment = cursor.read_fixed::<32>()?;
        let recovery = NoteSeizureRecoveryWitness {
            commitment: cursor.read_fixed::<32>()?,
            epk_affine: cursor.read_point_affine()?,
            c2: cursor.read_fixed::<32>()?,
            salt: cursor.read_fixed::<32>()?,
            key_confirmation: cursor.read_fixed::<32>()?,
            encrypted_amount: cursor.read_fixed::<32>()?,
            encrypted_note_blinding: cursor.read_fixed::<32>()?,
            seed: cursor.read_fixed::<32>()?,
        };
        let note_blinding = cursor.read_fixed::<32>()?;
        let position = cursor.read_u64()?;
        let state_commitment_auth_path = cursor.read_triple_path_32()?;
        let rnk = cursor.read_fixed::<32>()?;
        let rnk_commitment = cursor.read_fixed::<32>()?;
        cursor.finish("note seizure witness")?;
        Ok(Self {
            total_length,
            claimed_statement_hash,
            anchor,
            note_commitment,
            nullifier,
            history_required,
            recent_position_floor,
            address_diversified_generator,
            address_transmission_key,
            asset_id,
            amount,
            authorization_commitment,
            recovery,
            note_blinding,
            position,
            state_commitment_auth_path,
            rnk,
            rnk_commitment,
        })
    }
}

fn ensure_length(encoded: u32, actual: usize) -> Result<()> {
    if encoded as usize != actual {
        bail!("note seizure witness length mismatch: header={encoded}, actual={actual}")
    }
    Ok(())
}
