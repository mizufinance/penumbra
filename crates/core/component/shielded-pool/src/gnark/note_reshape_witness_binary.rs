use anyhow::{bail, Context, Result};

use crate::{
    gnark::{
        binary::{encode_vec_32, put_bytes, put_u32, put_u64, BinaryCursor},
        note_reshape_witness::{
            ConsolidateWitnessV1, NoteReshapeOutputWitnessV1, NoteReshapeSpendWitnessV1,
            SplitWitnessV1,
        },
        typed::encode_point_affine,
    },
    ConsolidateFamilyId, SplitFamilyId,
};

const CONSOLIDATE_WITNESS_MAGIC: &[u8; 4] = b"PCWG";
const SPLIT_WITNESS_MAGIC: &[u8; 4] = b"PSWG";
const NOTE_RESHAPE_WITNESS_VERSION: u32 = 1;

impl ConsolidateWitnessV1 {
    pub fn encode(&self) -> Result<Vec<u8>> {
        encode_note_reshape(
            CONSOLIDATE_WITNESS_MAGIC,
            self.family_id.get(),
            self.n_in,
            self.n_out,
            &self.anchor,
            &self.balance_commitment,
            &self.claimed_statement_hash,
            &self.statement_fields,
            &self.action_balance_blinding,
            &self.ak,
            &self.nk,
            &self.spends,
            &self.outputs,
            &self.balance_commitment_affine,
            &self.ak_affine,
        )
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let decoded = decode_note_reshape(CONSOLIDATE_WITNESS_MAGIC, bytes)?;
        let family_id = ConsolidateFamilyId::try_from(decoded.family_id)?;
        if decoded.n_in as usize != family_id.input_count()
            || decoded.n_out as usize != family_id.output_count()
        {
            bail!(
                "{} witness shape mismatch: got {}x{}, expected {}x{}",
                family_id.label(),
                decoded.n_in,
                decoded.n_out,
                family_id.input_count(),
                family_id.output_count()
            );
        }
        Ok(Self {
            family_id,
            total_length: decoded.total_length,
            n_in: decoded.n_in,
            n_out: decoded.n_out,
            anchor: decoded.anchor,
            balance_commitment: decoded.balance_commitment,
            claimed_statement_hash: decoded.claimed_statement_hash,
            statement_fields: decoded.statement_fields,
            action_balance_blinding: decoded.action_balance_blinding,
            ak: decoded.ak,
            nk: decoded.nk,
            spends: decoded.spends,
            outputs: decoded.outputs,
            balance_commitment_affine: decoded.balance_commitment_affine,
            ak_affine: decoded.ak_affine,
        })
    }
}

impl SplitWitnessV1 {
    pub fn encode(&self) -> Result<Vec<u8>> {
        encode_note_reshape(
            SPLIT_WITNESS_MAGIC,
            self.family_id.get(),
            self.n_in,
            self.n_out,
            &self.anchor,
            &self.balance_commitment,
            &self.claimed_statement_hash,
            &self.statement_fields,
            &self.action_balance_blinding,
            &self.ak,
            &self.nk,
            &self.spends,
            &self.outputs,
            &self.balance_commitment_affine,
            &self.ak_affine,
        )
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let decoded = decode_note_reshape(SPLIT_WITNESS_MAGIC, bytes)?;
        let family_id = SplitFamilyId::try_from(decoded.family_id)?;
        if decoded.n_in as usize != family_id.input_count()
            || decoded.n_out as usize != family_id.output_count()
        {
            bail!(
                "{} witness shape mismatch: got {}x{}, expected {}x{}",
                family_id.label(),
                decoded.n_in,
                decoded.n_out,
                family_id.input_count(),
                family_id.output_count()
            );
        }
        Ok(Self {
            family_id,
            total_length: decoded.total_length,
            n_in: decoded.n_in,
            n_out: decoded.n_out,
            anchor: decoded.anchor,
            balance_commitment: decoded.balance_commitment,
            claimed_statement_hash: decoded.claimed_statement_hash,
            statement_fields: decoded.statement_fields,
            action_balance_blinding: decoded.action_balance_blinding,
            ak: decoded.ak,
            nk: decoded.nk,
            spends: decoded.spends,
            outputs: decoded.outputs,
            balance_commitment_affine: decoded.balance_commitment_affine,
            ak_affine: decoded.ak_affine,
        })
    }
}

struct DecodedNoteReshapeWitness {
    family_id: u32,
    total_length: u32,
    n_in: u32,
    n_out: u32,
    anchor: [u8; 32],
    balance_commitment: [u8; 32],
    claimed_statement_hash: [u8; 32],
    statement_fields: Vec<[u8; 32]>,
    action_balance_blinding: [u8; 32],
    ak: [u8; 32],
    nk: [u8; 32],
    spends: Vec<NoteReshapeSpendWitnessV1>,
    outputs: Vec<NoteReshapeOutputWitnessV1>,
    balance_commitment_affine: crate::gnark::typed::PointAffineBytes,
    ak_affine: crate::gnark::typed::PointAffineBytes,
}

fn encode_note_reshape(
    magic: &[u8; 4],
    family_id: u32,
    n_in: u32,
    n_out: u32,
    anchor: &[u8; 32],
    balance_commitment: &[u8; 32],
    claimed_statement_hash: &[u8; 32],
    statement_fields: &[[u8; 32]],
    action_balance_blinding: &[u8; 32],
    ak: &[u8; 32],
    nk: &[u8; 32],
    spends: &[NoteReshapeSpendWitnessV1],
    outputs: &[NoteReshapeOutputWitnessV1],
    balance_commitment_affine: &crate::gnark::typed::PointAffineBytes,
    ak_affine: &crate::gnark::typed::PointAffineBytes,
) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    put_bytes(&mut buf, magic);
    put_u32(&mut buf, NOTE_RESHAPE_WITNESS_VERSION);
    put_u32(&mut buf, 0);
    put_u32(&mut buf, family_id);
    put_u32(&mut buf, n_in);
    put_u32(&mut buf, n_out);
    put_bytes(&mut buf, anchor);
    put_bytes(&mut buf, balance_commitment);
    put_bytes(&mut buf, claimed_statement_hash);
    encode_vec_32(&mut buf, statement_fields)?;
    put_bytes(&mut buf, action_balance_blinding);
    put_bytes(&mut buf, ak);
    put_bytes(&mut buf, nk);
    for spend in spends {
        encode_spend(&mut buf, spend)?;
    }
    for output in outputs {
        encode_output(&mut buf, output);
    }
    encode_point_affine(&mut buf, balance_commitment_affine);
    encode_point_affine(&mut buf, ak_affine);

    let total_len = u32::try_from(buf.len()).context("encoded note reshape witness exceeds u32")?;
    buf[8..12].copy_from_slice(&total_len.to_le_bytes());
    Ok(buf)
}

fn decode_note_reshape(magic: &[u8; 4], bytes: &[u8]) -> Result<DecodedNoteReshapeWitness> {
    let mut cursor = BinaryCursor::new(bytes);
    if cursor.read_fixed::<4>()? != *magic {
        bail!("invalid note reshape witness magic");
    }
    let version = cursor.read_u32()?;
    if version != NOTE_RESHAPE_WITNESS_VERSION {
        bail!("unsupported note reshape witness version {version}");
    }
    let total_length = cursor.read_u32()?;
    if total_length as usize != bytes.len() {
        bail!(
            "note reshape witness length mismatch: header={}, actual={}",
            total_length,
            bytes.len()
        );
    }
    let family_id = cursor.read_u32()?;
    let n_in = cursor.read_u32()?;
    let n_out = cursor.read_u32()?;
    let anchor = cursor.read_fixed::<32>()?;
    let balance_commitment = cursor.read_fixed::<32>()?;
    let claimed_statement_hash = cursor.read_fixed::<32>()?;
    let statement_fields = cursor.read_vec_32()?;
    let action_balance_blinding = cursor.read_fixed::<32>()?;
    let ak = cursor.read_fixed::<32>()?;
    let nk = cursor.read_fixed::<32>()?;
    let spends = (0..n_in)
        .map(|_| decode_spend(&mut cursor))
        .collect::<Result<Vec<_>>>()?;
    let outputs = (0..n_out)
        .map(|_| decode_output(&mut cursor))
        .collect::<Result<Vec<_>>>()?;
    let balance_commitment_affine = cursor.read_point_affine()?;
    let ak_affine = cursor.read_point_affine()?;
    cursor.finish("note reshape witness")?;

    Ok(DecodedNoteReshapeWitness {
        family_id,
        total_length,
        n_in,
        n_out,
        anchor,
        balance_commitment,
        claimed_statement_hash,
        statement_fields,
        action_balance_blinding,
        ak,
        nk,
        spends,
        outputs,
        balance_commitment_affine,
        ak_affine,
    })
}

fn encode_spend(buf: &mut Vec<u8>, spend: &NoteReshapeSpendWitnessV1) -> Result<()> {
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
    encode_point_affine(buf, &spend.rk_affine);
    encode_point_affine(buf, &spend.spent_diversified_generator_affine);
    encode_point_affine(buf, &spend.spent_transmission_key_affine);
    Ok(())
}

fn decode_spend(cursor: &mut BinaryCursor<'_>) -> Result<NoteReshapeSpendWitnessV1> {
    Ok(NoteReshapeSpendWitnessV1 {
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
        rk_affine: cursor.read_point_affine()?,
        spent_diversified_generator_affine: cursor.read_point_affine()?,
        spent_transmission_key_affine: cursor.read_point_affine()?,
    })
}

fn encode_output(buf: &mut Vec<u8>, output: &NoteReshapeOutputWitnessV1) {
    put_bytes(buf, &output.note_commitment);
    put_bytes(buf, &output.created_note_blinding);
    put_bytes(buf, &output.created_note_amount);
    put_bytes(buf, &output.created_note_asset_id);
    put_bytes(buf, &output.created_transmission_key);
    put_bytes(buf, &output.created_clue_key);
    encode_point_affine(buf, &output.created_diversified_generator_affine);
    encode_point_affine(buf, &output.created_transmission_key_affine);
}

fn decode_output(cursor: &mut BinaryCursor<'_>) -> Result<NoteReshapeOutputWitnessV1> {
    Ok(NoteReshapeOutputWitnessV1 {
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
