use anyhow::{anyhow, bail, Result};
use decaf377::{Encoding, Fq};

use crate::{
    consolidate::{
        ConsolidateInputPrivate, ConsolidateInputPublic, ConsolidateOutputPrivate,
        ConsolidateOutputPublic, ConsolidateProofPrivate, ConsolidateProofPublic,
    },
    gnark::typed::{point_affine_bytes, PointAffineBytes},
    public_input_hash::{
        consolidate_statement_fields, consolidate_statement_hash_from_public,
        split_statement_fields, split_statement_hash_from_public,
    },
    split::{
        SplitInputPrivate, SplitInputPublic, SplitOutputPrivate, SplitOutputPublic,
        SplitProofPrivate, SplitProofPublic,
    },
    ConsolidateFamilyId, SplitFamilyId,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteReshapeSpendWitnessV1 {
    pub nullifier: [u8; 32],
    pub spent_note_blinding: [u8; 32],
    pub spent_note_amount: [u8; 32],
    pub spent_note_asset_id: [u8; 32],
    pub spent_transmission_key: [u8; 32],
    pub spent_clue_key: [u8; 32],
    pub state_commitment_commitment: [u8; 32],
    pub state_commitment_position: u64,
    pub state_commitment_auth_path: Vec<[[u8; 32]; 3]>,
    pub spend_auth_randomizer: [u8; 32],
    pub rk_affine: PointAffineBytes,
    pub spent_diversified_generator_affine: PointAffineBytes,
    pub spent_transmission_key_affine: PointAffineBytes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteReshapeOutputWitnessV1 {
    pub note_commitment: [u8; 32],
    pub created_note_blinding: [u8; 32],
    pub created_note_amount: [u8; 32],
    pub created_note_asset_id: [u8; 32],
    pub created_transmission_key: [u8; 32],
    pub created_clue_key: [u8; 32],
    pub created_diversified_generator_affine: PointAffineBytes,
    pub created_transmission_key_affine: PointAffineBytes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsolidateWitnessV1 {
    pub family_id: ConsolidateFamilyId,
    pub total_length: u32,
    pub n_in: u32,
    pub n_out: u32,
    pub anchor: [u8; 32],
    pub balance_commitment: [u8; 32],
    pub claimed_statement_hash: [u8; 32],
    pub statement_fields: Vec<[u8; 32]>,
    pub action_balance_blinding: [u8; 32],
    pub ak: [u8; 32],
    pub nk: [u8; 32],
    pub spends: Vec<NoteReshapeSpendWitnessV1>,
    pub outputs: Vec<NoteReshapeOutputWitnessV1>,
    pub balance_commitment_affine: PointAffineBytes,
    pub ak_affine: PointAffineBytes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitWitnessV1 {
    pub family_id: SplitFamilyId,
    pub total_length: u32,
    pub n_in: u32,
    pub n_out: u32,
    pub anchor: [u8; 32],
    pub balance_commitment: [u8; 32],
    pub claimed_statement_hash: [u8; 32],
    pub statement_fields: Vec<[u8; 32]>,
    pub action_balance_blinding: [u8; 32],
    pub ak: [u8; 32],
    pub nk: [u8; 32],
    pub spends: Vec<NoteReshapeSpendWitnessV1>,
    pub outputs: Vec<NoteReshapeOutputWitnessV1>,
    pub balance_commitment_affine: PointAffineBytes,
    pub ak_affine: PointAffineBytes,
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
    public_input: &ConsolidateInputPublic,
    private_input: &ConsolidateInputPrivate,
    index: usize,
) -> Result<NoteReshapeSpendWitnessV1> {
    let state_commitment_auth_path = private_input
        .state_commitment_proof
        .auth_path()
        .iter()
        .map(|siblings| siblings.map(|sibling| Fq::from(sibling).to_bytes()))
        .collect::<Vec<_>>();
    Ok(NoteReshapeSpendWitnessV1 {
        nullifier: public_input.nullifier.0.to_bytes(),
        spent_note_blinding: private_input.spent_note.note_blinding().to_bytes(),
        spent_note_amount: Fq::from(private_input.spent_note.value().amount).to_bytes(),
        spent_note_asset_id: private_input.spent_note.asset_id().0.to_bytes(),
        spent_transmission_key: private_input.spent_note.transmission_key().0,
        spent_clue_key: Fq::from_le_bytes_mod_order(&private_input.spent_note.clue_key().0)
            .to_bytes(),
        state_commitment_commitment: private_input
            .state_commitment_proof
            .commitment()
            .0
            .to_bytes(),
        state_commitment_position: u64::from(private_input.state_commitment_proof.position()),
        state_commitment_auth_path,
        spend_auth_randomizer: private_input.spend_auth_randomizer.to_bytes(),
        rk_affine: point_affine_bytes(verification_key_point(
            public_input.rk,
            &format!("rk_{index}"),
        )?)?,
        spent_diversified_generator_affine: point_affine_bytes(
            private_input.spent_note.diversified_generator(),
        )?,
        spent_transmission_key_affine: point_affine_bytes(
            Encoding(private_input.spent_note.transmission_key().0)
                .vartime_decompress()
                .map_err(|e| anyhow!("decompress spent transmission key {index}: {e:?}"))?,
        )?,
    })
}

fn split_spend_witness(
    public_input: &SplitInputPublic,
    private_input: &SplitInputPrivate,
    index: usize,
) -> Result<NoteReshapeSpendWitnessV1> {
    let state_commitment_auth_path = private_input
        .state_commitment_proof
        .auth_path()
        .iter()
        .map(|siblings| siblings.map(|sibling| Fq::from(sibling).to_bytes()))
        .collect::<Vec<_>>();
    Ok(NoteReshapeSpendWitnessV1 {
        nullifier: public_input.nullifier.0.to_bytes(),
        spent_note_blinding: private_input.spent_note.note_blinding().to_bytes(),
        spent_note_amount: Fq::from(private_input.spent_note.value().amount).to_bytes(),
        spent_note_asset_id: private_input.spent_note.asset_id().0.to_bytes(),
        spent_transmission_key: private_input.spent_note.transmission_key().0,
        spent_clue_key: Fq::from_le_bytes_mod_order(&private_input.spent_note.clue_key().0)
            .to_bytes(),
        state_commitment_commitment: private_input
            .state_commitment_proof
            .commitment()
            .0
            .to_bytes(),
        state_commitment_position: u64::from(private_input.state_commitment_proof.position()),
        state_commitment_auth_path,
        spend_auth_randomizer: private_input.spend_auth_randomizer.to_bytes(),
        rk_affine: point_affine_bytes(verification_key_point(
            public_input.rk,
            &format!("rk_{index}"),
        )?)?,
        spent_diversified_generator_affine: point_affine_bytes(
            private_input.spent_note.diversified_generator(),
        )?,
        spent_transmission_key_affine: point_affine_bytes(
            Encoding(private_input.spent_note.transmission_key().0)
                .vartime_decompress()
                .map_err(|e| anyhow!("decompress spent transmission key {index}: {e:?}"))?,
        )?,
    })
}

fn output_witness(
    public_output: &ConsolidateOutputPublic,
    private_output: &ConsolidateOutputPrivate,
    index: usize,
) -> Result<NoteReshapeOutputWitnessV1> {
    Ok(NoteReshapeOutputWitnessV1 {
        note_commitment: public_output.note_commitment.0.to_bytes(),
        created_note_blinding: private_output.created_note.note_blinding().to_bytes(),
        created_note_amount: Fq::from(private_output.created_note.value().amount).to_bytes(),
        created_note_asset_id: private_output.created_note.asset_id().0.to_bytes(),
        created_transmission_key: private_output.created_note.transmission_key().0,
        created_clue_key: Fq::from_le_bytes_mod_order(&private_output.created_note.clue_key().0)
            .to_bytes(),
        created_diversified_generator_affine: point_affine_bytes(
            private_output.created_note.diversified_generator(),
        )?,
        created_transmission_key_affine: point_affine_bytes(
            Encoding(private_output.created_note.transmission_key().0)
                .vartime_decompress()
                .map_err(|e| anyhow!("decompress created transmission key {index}: {e:?}"))?,
        )?,
    })
}

fn split_output_witness(
    public_output: &SplitOutputPublic,
    private_output: &SplitOutputPrivate,
    index: usize,
) -> Result<NoteReshapeOutputWitnessV1> {
    Ok(NoteReshapeOutputWitnessV1 {
        note_commitment: public_output.note_commitment.0.to_bytes(),
        created_note_blinding: private_output.created_note.note_blinding().to_bytes(),
        created_note_amount: Fq::from(private_output.created_note.value().amount).to_bytes(),
        created_note_asset_id: private_output.created_note.asset_id().0.to_bytes(),
        created_transmission_key: private_output.created_note.transmission_key().0,
        created_clue_key: Fq::from_le_bytes_mod_order(&private_output.created_note.clue_key().0)
            .to_bytes(),
        created_diversified_generator_affine: point_affine_bytes(
            private_output.created_note.diversified_generator(),
        )?,
        created_transmission_key_affine: point_affine_bytes(
            Encoding(private_output.created_note.transmission_key().0)
                .vartime_decompress()
                .map_err(|e| anyhow!("decompress created transmission key {index}: {e:?}"))?,
        )?,
    })
}

impl ConsolidateWitnessV1 {
    pub fn from_public_private(
        public: &ConsolidateProofPublic,
        private: &ConsolidateProofPrivate,
    ) -> Result<Self> {
        public.validate_shape()?;
        private.validate_shape()?;
        if public.family_id != private.family_id {
            bail!(
                "consolidate witness family mismatch: public={} private={}",
                public.family_id.get(),
                private.family_id.get()
            );
        }

        let claimed_statement_hash = consolidate_statement_hash_from_public(public)
            .map_err(|e| anyhow!("compute {} statement hash: {e}", public.family_id.label()))?;
        let statement_fields = consolidate_statement_fields(public)
            .map_err(|e| anyhow!("compute {} statement fields: {e}", public.family_id.label()))?;

        let spends = public
            .inputs
            .iter()
            .zip(private.inputs.iter())
            .enumerate()
            .map(|(index, (public_input, private_input))| {
                spend_witness(public_input, private_input, index)
            })
            .collect::<Result<Vec<_>>>()?;

        let outputs = public
            .outputs
            .iter()
            .zip(private.outputs.iter())
            .enumerate()
            .map(|(index, (public_output, private_output))| {
                output_witness(public_output, private_output, index)
            })
            .collect::<Result<Vec<_>>>()?;

        let mut witness = Self {
            family_id: public.family_id,
            total_length: 0,
            n_in: public.inputs.len() as u32,
            n_out: public.outputs.len() as u32,
            anchor: Fq::from(public.anchor).to_bytes(),
            balance_commitment: public.balance_commitment.to_bytes(),
            claimed_statement_hash: claimed_statement_hash.to_bytes(),
            statement_fields: statement_fields.iter().map(|v| v.to_bytes()).collect(),
            action_balance_blinding: private.action_balance_blinding.to_bytes(),
            ak: private.ak.to_bytes(),
            nk: private.nk.0.to_bytes(),
            spends,
            outputs,
            balance_commitment_affine: point_affine_bytes(public.balance_commitment.0)?,
            ak_affine: point_affine_bytes(verification_key_point(private.ak, "ak")?)?,
        };
        witness.total_length = u32::try_from(witness.encode()?.len())
            .map_err(|_| anyhow!("encoded {} witness exceeds u32", witness.family_id.label()))?;
        Ok(witness)
    }
}

impl SplitWitnessV1 {
    pub fn from_public_private(
        public: &SplitProofPublic,
        private: &SplitProofPrivate,
    ) -> Result<Self> {
        public.validate_shape()?;
        private.validate_shape()?;
        if public.family_id != private.family_id {
            bail!(
                "split witness family mismatch: public={} private={}",
                public.family_id.get(),
                private.family_id.get()
            );
        }

        let claimed_statement_hash = split_statement_hash_from_public(public)
            .map_err(|e| anyhow!("compute {} statement hash: {e}", public.family_id.label()))?;
        let statement_fields = split_statement_fields(public)
            .map_err(|e| anyhow!("compute {} statement fields: {e}", public.family_id.label()))?;

        let spends = public
            .inputs
            .iter()
            .zip(private.inputs.iter())
            .enumerate()
            .map(|(index, (public_input, private_input))| {
                split_spend_witness(public_input, private_input, index)
            })
            .collect::<Result<Vec<_>>>()?;

        let outputs = public
            .outputs
            .iter()
            .zip(private.outputs.iter())
            .enumerate()
            .map(|(index, (public_output, private_output))| {
                split_output_witness(public_output, private_output, index)
            })
            .collect::<Result<Vec<_>>>()?;

        let mut witness = Self {
            family_id: public.family_id,
            total_length: 0,
            n_in: public.inputs.len() as u32,
            n_out: public.outputs.len() as u32,
            anchor: Fq::from(public.anchor).to_bytes(),
            balance_commitment: public.balance_commitment.to_bytes(),
            claimed_statement_hash: claimed_statement_hash.to_bytes(),
            statement_fields: statement_fields.iter().map(|v| v.to_bytes()).collect(),
            action_balance_blinding: private.action_balance_blinding.to_bytes(),
            ak: private.ak.to_bytes(),
            nk: private.nk.0.to_bytes(),
            spends,
            outputs,
            balance_commitment_affine: point_affine_bytes(public.balance_commitment.0)?,
            ak_affine: point_affine_bytes(verification_key_point(private.ak, "ak")?)?,
        };
        witness.total_length = u32::try_from(witness.encode()?.len())
            .map_err(|_| anyhow!("encoded {} witness exceeds u32", witness.family_id.label()))?;
        Ok(witness)
    }
}
