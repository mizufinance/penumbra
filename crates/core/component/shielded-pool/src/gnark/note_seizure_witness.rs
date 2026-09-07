use anyhow::Result;
use decaf377::Fq;

use crate::{
    gnark::{typed::point_affine_bytes, PointAffineBytes},
    public_input_hash::note_seizure_statement_hash_from_public,
    NoteSeizureProofPrivate, NoteSeizureProofPublic,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteSeizureRecoveryWitness {
    pub commitment: [u8; 32],
    pub epk_affine: PointAffineBytes,
    pub c2: [u8; 32],
    pub salt: [u8; 32],
    pub key_confirmation: [u8; 32],
    pub encrypted_amount: [u8; 32],
    pub encrypted_note_blinding: [u8; 32],
    pub seed: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteSeizureWitness {
    pub total_length: u32,
    pub claimed_statement_hash: [u8; 32],
    pub anchor: [u8; 32],
    pub note_commitment: [u8; 32],
    pub nullifier: [u8; 32],
    pub history_required: bool,
    pub recent_position_floor: [u8; 32],
    pub address_diversified_generator: [u8; 32],
    pub address_transmission_key: [u8; 32],
    pub asset_id: [u8; 32],
    pub amount: [u8; 32],
    pub authorization_commitment: [u8; 32],
    pub recovery: NoteSeizureRecoveryWitness,
    pub note_blinding: [u8; 32],
    pub position: u64,
    pub state_commitment_auth_path: Vec<[[u8; 32]; 3]>,
    pub rnk: [u8; 32],
    pub rnk_commitment: [u8; 32],
}

impl NoteSeizureWitness {
    pub fn from_public_private(
        public: &NoteSeizureProofPublic,
        private: &NoteSeizureProofPrivate,
    ) -> Result<Self> {
        private.validate_against(public)?;
        let capsule = &public.recovery_capsule;
        Ok(Self {
            total_length: 0,
            claimed_statement_hash: note_seizure_statement_hash_from_public(public)?.to_bytes(),
            anchor: Fq::from(public.anchor).to_bytes(),
            note_commitment: public.authorization.note_commitment.0.to_bytes(),
            nullifier: public.authorization.nullifier.0.to_bytes(),
            history_required: public.history_required,
            recent_position_floor: Fq::from(public.recent_position_floor).to_bytes(),
            address_diversified_generator: public
                .authorization
                .address
                .diversified_generator()
                .vartime_compress_to_field()
                .to_bytes(),
            address_transmission_key: public.authorization.address.transmission_key_s().to_bytes(),
            asset_id: public.authorization.asset_id.0.to_bytes(),
            amount: Fq::from(public.authorization.amount).to_bytes(),
            authorization_commitment: public.authorization.commitment()?.to_bytes(),
            recovery: NoteSeizureRecoveryWitness {
                commitment: capsule.commitment().0.to_bytes(),
                epk_affine: point_affine_bytes(capsule.epk)?,
                c2: capsule.c2.to_bytes(),
                salt: capsule.salt.to_bytes(),
                key_confirmation: capsule.key_confirmation.to_bytes(),
                encrypted_amount: capsule.encrypted_amount.to_bytes(),
                encrypted_note_blinding: capsule.encrypted_note_blinding.to_bytes(),
                seed: public.recovery_seed.to_bytes(),
            },
            note_blinding: private.note_blinding.to_bytes(),
            position: u64::from(private.state_commitment_proof.position()),
            state_commitment_auth_path: private
                .state_commitment_proof
                .auth_path()
                .iter()
                .map(|siblings| siblings.map(|sibling| Fq::from(sibling).to_bytes()))
                .collect(),
            rnk: private.rnk.to_bytes(),
            rnk_commitment: public.rnk_commitment.to_bytes(),
        })
    }
}
