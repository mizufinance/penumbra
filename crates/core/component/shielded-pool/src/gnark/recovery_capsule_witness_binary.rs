use anyhow::Result;

use crate::gnark::{
    binary::{put_bytes, BinaryCursor},
    recovery_capsule_witness::RecoveryCapsuleWitness,
    typed::encode_point_affine,
};

pub(crate) fn encode_recovery_capsule(buf: &mut Vec<u8>, capsule: &RecoveryCapsuleWitness) {
    encode_point_affine(buf, &capsule.epk_affine);
    put_bytes(buf, &capsule.c2);
    put_bytes(buf, &capsule.salt);
    put_bytes(buf, &capsule.key_confirmation);
    put_bytes(buf, &capsule.encrypted_amount);
    put_bytes(buf, &capsule.encrypted_note_blinding);
    put_bytes(buf, &capsule.seed);
    put_bytes(buf, &capsule.randomizer);
}

pub(crate) fn decode_recovery_capsule(
    cursor: &mut BinaryCursor<'_>,
) -> Result<RecoveryCapsuleWitness> {
    Ok(RecoveryCapsuleWitness {
        epk_affine: cursor.read_point_affine()?,
        c2: cursor.read_fixed::<32>()?,
        salt: cursor.read_fixed::<32>()?,
        key_confirmation: cursor.read_fixed::<32>()?,
        encrypted_amount: cursor.read_fixed::<32>()?,
        encrypted_note_blinding: cursor.read_fixed::<32>()?,
        seed: cursor.read_fixed::<32>()?,
        randomizer: cursor.read_fr()?,
    })
}
