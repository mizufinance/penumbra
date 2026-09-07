use ark_serialize::{CanonicalDeserialize, CanonicalSerialize, SerializationError};

use ark_serialize::{Compress, Validate};

/// Accept only the serializer's canonical image: full byte consumption plus an
/// exact compressed re-serialization round trip. This rejects arkworks'
/// permissive infinity aliases, trailing bytes, and alternate accepted forms.
pub(crate) fn deserialize_compressed_strict<T>(bytes: &[u8]) -> Result<T, SerializationError>
where
    T: CanonicalDeserialize + CanonicalSerialize,
{
    let mut remaining = bytes;
    let value = T::deserialize_compressed(&mut remaining)?;
    if !remaining.is_empty() {
        return Err(SerializationError::InvalidData);
    }

    let mut canonical = Vec::new();
    value.serialize_compressed(&mut canonical)?;
    if canonical != bytes {
        return Err(SerializationError::InvalidData);
    }

    Ok(value)
}

/// Strict canonical decoding with a caller-owned validation predicate.
///
/// This is used when an equivalent validation
/// kernel replaces a type's default `Valid::check` implementation.

pub(crate) fn deserialize_compressed_strict_with<T>(
    bytes: &[u8],
    validate: impl FnOnce(&T) -> Result<(), SerializationError>,
) -> Result<T, SerializationError>
where
    T: CanonicalDeserialize + CanonicalSerialize,
{
    let mut remaining = bytes;
    let value = T::deserialize_with_mode(&mut remaining, Compress::Yes, Validate::No)?;
    if !remaining.is_empty() {
        return Err(SerializationError::InvalidData);
    }
    validate(&value)?;

    let mut canonical = Vec::new();
    value.serialize_compressed(&mut canonical)?;
    if canonical != bytes {
        return Err(SerializationError::InvalidData);
    }

    Ok(value)
}

#[cfg(test)]
mod tests {
    use ark_ec::{pairing::Pairing, AffineRepr};
    use ark_ff::UniformRand;
    use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
    use decaf377::Bls12_377;
    use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};

    use super::deserialize_compressed_strict;

    type G1 = <Bls12_377 as Pairing>::G1Affine;
    type G2 = <Bls12_377 as Pairing>::G2Affine;

    fn compressed<T: CanonicalSerialize>(value: &T) -> Vec<u8> {
        let mut bytes = Vec::new();
        value.serialize_compressed(&mut bytes).unwrap();
        bytes
    }

    fn infinity_alias(mut bytes: Vec<u8>) -> Vec<u8> {
        // Compressed field coordinates are little-endian. Keep the infinity
        // flag and set a coordinate bit that arkworks permissively discards.
        bytes[0] |= 1;
        bytes
    }

    #[test]
    fn arkworks_accepts_but_strict_rejects_g1_infinity_alias() {
        let canonical = compressed(&G1::zero());
        let alias = infinity_alias(canonical);
        assert!(G1::deserialize_compressed(&alias[..]).is_ok());
        assert!(deserialize_compressed_strict::<G1>(&alias).is_err());
    }

    #[test]
    fn arkworks_accepts_but_strict_rejects_g2_infinity_alias_in_each_coordinate() {
        let canonical = compressed(&G2::zero());
        let field_bytes = canonical.len() / 2;
        for offset in [0, field_bytes] {
            let mut alias = canonical.clone();
            alias[offset] |= 1;
            assert!(G2::deserialize_compressed(&alias[..]).is_ok());
            assert!(deserialize_compressed_strict::<G2>(&alias).is_err());
        }
    }

    #[test]
    fn canonical_points_and_zero_coordinates_succeed() {
        let mut rng = ChaCha20Rng::seed_from_u64(0x51a7e);
        for point in [G1::zero(), G1::generator(), G1::rand(&mut rng)] {
            let bytes = compressed(&point);
            let _ = deserialize_compressed_strict::<G1>(&bytes).unwrap();
        }
        for point in [G2::zero(), G2::generator(), G2::rand(&mut rng)] {
            let bytes = compressed(&point);
            let _ = deserialize_compressed_strict::<G2>(&bytes).unwrap();
        }
    }

    #[test]
    fn trailing_and_malformed_bytes_reject() {
        let g1 = compressed(&G1::generator());
        let mut trailing = g1.clone();
        trailing.push(0);
        assert!(deserialize_compressed_strict::<G1>(&trailing).is_err());
        assert!(deserialize_compressed_strict::<G1>(&g1[..g1.len() - 1]).is_err());

        let mut illegal_flags = g1.clone();
        let last = illegal_flags.len() - 1;
        illegal_flags[last] |= 0xc0;
        assert!(deserialize_compressed_strict::<G1>(&illegal_flags).is_err());
        assert!(deserialize_compressed_strict::<G1>(&vec![0xff; g1.len()]).is_err());

        let g2 = compressed(&G2::generator());
        assert!(deserialize_compressed_strict::<G2>(&g2[..g2.len() - 1]).is_err());
        assert!(deserialize_compressed_strict::<G2>(&vec![0xff; g2.len()]).is_err());
    }
}
