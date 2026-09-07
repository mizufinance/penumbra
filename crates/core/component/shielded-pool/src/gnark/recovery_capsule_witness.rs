use anyhow::{ensure, Result};
use decaf377::Element;

use crate::{
    gnark::typed::{point_affine_bytes, PointAffineBytes},
    Note, RecoveryCapsule,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryCapsuleWitness {
    pub epk_affine: PointAffineBytes,
    pub c2: [u8; 32],
    pub salt: [u8; 32],
    pub key_confirmation: [u8; 32],
    pub encrypted_amount: [u8; 32],
    pub encrypted_note_blinding: [u8; 32],
    pub seed: [u8; 32],
    pub randomizer: [u8; 32],
}

impl RecoveryCapsuleWitness {
    pub fn from_note(note: &Note, capk: Element) -> Result<Self> {
        let (capsule, opening) =
            RecoveryCapsule::encrypt(note.amount(), note.note_blinding(), capk, note.rseed())?;
        ensure!(
            capsule.commitment() == note.recovery_commitment(),
            "note recovery commitment does not match its canonical capsule"
        );
        Ok(Self {
            epk_affine: point_affine_bytes(capsule.epk)?,
            c2: capsule.c2.to_bytes(),
            salt: capsule.salt.to_bytes(),
            key_confirmation: capsule.key_confirmation.to_bytes(),
            encrypted_amount: capsule.encrypted_amount.to_bytes(),
            encrypted_note_blinding: capsule.encrypted_note_blinding.to_bytes(),
            seed: opening.seed.to_bytes(),
            randomizer: opening.r.to_bytes(),
        })
    }
}
