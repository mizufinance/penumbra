//! Half-size BLS12-377 target-group encoding for the SnarkPack v2 wire.
//!
//! The codec changes only proof transport. Verification reconstructs the v1
//! target values and applies the same group and proof-shape validation.

use ark_ec::pairing::{Pairing, PairingOutput};
use ark_ff::{batch_inversion, Field, Zero};
use ark_ip_proofs::applications::groth16_aggregation::{
    aggregate_proof_target_values, try_map_aggregate_proof_target, validate_aggregate_proof,
    validate_decoded_aggregate_proof, AggregateProof, AggregateProofWithTarget,
    ValidatedAggregateProof,
};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize, SerializationError};
use decaf377::{Bls12_377, Fp};
use digest::Digest;

use crate::strict_deserialize::deserialize_compressed_strict_with;

#[derive(Clone, Debug, Default, Eq, PartialEq, CanonicalDeserialize, CanonicalSerialize)]
struct TorusTarget([Fp; 6]);

type TorusAggregateProof<D> = AggregateProofWithTarget<Bls12_377, TorusTarget, D>;

fn coordinate_from_wire(value: &TorusTarget) -> <Bls12_377 as Pairing>::TargetField {
    let mut holder = <Bls12_377 as Pairing>::TargetField::zero();
    holder.c0.c0.c0 = value.0[0];
    holder.c0.c0.c1 = value.0[1];
    holder.c0.c1.c0 = value.0[2];
    holder.c0.c1.c1 = value.0[3];
    holder.c0.c2.c0 = value.0[4];
    holder.c0.c2.c1 = value.0[5];
    holder
}

fn compress_targets(
    values: &[&PairingOutput<Bls12_377>],
) -> Result<Vec<TorusTarget>, SerializationError> {
    let mut c1_inverses = Vec::with_capacity(values.len());
    for value in values {
        if value.is_zero() {
            c1_inverses.push(Default::default());
        } else if value.0.c1.is_zero() {
            // The only norm-one value omitted by the affine torus chart is
            // -1. It is not in the odd-order BLS12-377 target subgroup.
            return Err(SerializationError::InvalidData);
        } else {
            c1_inverses.push(value.0.c1);
        }
    }
    batch_inversion(&mut c1_inverses);

    Ok(values
        .iter()
        .zip(c1_inverses)
        .map(|(value, c1_inverse)| {
            if value.is_zero() {
                TorusTarget::default()
            } else {
                let mut numerator = value.0.c0;
                numerator.c0.c0 += Fp::from(1u64);
                let coordinate = numerator * c1_inverse;
                TorusTarget([
                    coordinate.c0.c0,
                    coordinate.c0.c1,
                    coordinate.c1.c0,
                    coordinate.c1.c1,
                    coordinate.c2.c0,
                    coordinate.c2.c1,
                ])
            }
        })
        .collect())
}

fn decompress_targets(
    values: &[&TorusTarget],
) -> Result<Vec<PairingOutput<Bls12_377>>, SerializationError> {
    let coordinates = values
        .iter()
        .map(|wire| coordinate_from_wire(wire).c0)
        .collect::<Vec<_>>();
    let squares = coordinates.iter().map(Field::square).collect::<Vec<_>>();
    let mut nonresidue_holder = <Bls12_377 as Pairing>::TargetField::zero();
    nonresidue_holder.c0.c1.c0 = Fp::from(1u64);
    let nonresidue = nonresidue_holder.c0;
    let mut denominator_inverses = coordinates
        .iter()
        .zip(&squares)
        .map(|(coordinate, square)| {
            if coordinate.is_zero() {
                Default::default()
            } else {
                *square - nonresidue
            }
        })
        .collect::<Vec<_>>();
    batch_inversion(&mut denominator_inverses);

    coordinates
        .into_iter()
        .zip(squares)
        .zip(denominator_inverses)
        .map(|((coordinate, square), denominator_inverse)| {
            if coordinate.is_zero() {
                return Ok(PairingOutput::<Bls12_377>::zero());
            }
            if denominator_inverse.is_zero() {
                return Err(SerializationError::InvalidData);
            }

            let mut target = <Bls12_377 as Pairing>::TargetField::zero();
            target.c0 = (square + nonresidue) * denominator_inverse;
            target.c1 = (coordinate + coordinate) * denominator_inverse;
            Ok(PairingOutput(target))
        })
        .collect()
}

fn validate_regular_proof<D: Digest + Send + Sync>(
    proof: &AggregateProof<Bls12_377, D>,
) -> Result<(), SerializationError> {
    validate_decoded_aggregate_proof(
        proof,
        crate::backend::validate_bls12_377_g1_fast,
        crate::backend::validate_bls12_377_g2_fast,
        crate::backend::validate_bls12_377_gt_fast,
    )
}

pub(crate) fn serialize_torus_aggregate_proof<D: Digest + Send + Sync>(
    proof: &AggregateProof<Bls12_377, D>,
) -> Result<Vec<u8>, SerializationError> {
    // Torus encoding is injective only on the certified odd-order target
    // subgroup. AggregateProof does not carry that invariant in its Rust type,
    // so enforce it before entering the chart. If construction later returns a
    // proof type with a private validity certificate, this check can move into
    // that constructor without weakening this boundary.
    validate_regular_proof(proof)?;
    let targets = aggregate_proof_target_values::<Bls12_377, PairingOutput<Bls12_377>, D>(proof);
    let compressed = compress_targets(&targets)?;
    let mut compressed = compressed.into_iter();
    let proof = try_map_aggregate_proof_target::<
        Bls12_377,
        PairingOutput<Bls12_377>,
        TorusTarget,
        D,
        SerializationError,
    >(proof, |_| {
        compressed.next().ok_or(SerializationError::InvalidData)
    })?;
    if compressed.next().is_some() {
        return Err(SerializationError::InvalidData);
    }

    let mut bytes = Vec::with_capacity(proof.compressed_size());
    proof.serialize_compressed(&mut bytes)?;
    Ok(bytes)
}

pub(crate) fn deserialize_torus_aggregate_proof<D: Digest + Send + Sync>(
    bytes: &[u8],
) -> Result<ValidatedAggregateProof<Bls12_377, D>, SerializationError> {
    let proof = deserialize_torus_aggregate_proof_wire(bytes)?;
    validate_aggregate_proof(&proof).map_err(|_| SerializationError::InvalidData)
}

fn deserialize_torus_aggregate_proof_wire<D: Digest + Send + Sync>(
    bytes: &[u8],
) -> Result<AggregateProof<Bls12_377, D>, SerializationError> {
    // Decode fields and points without trusting their subgroup membership.
    // The reconstructed regular proof is validated below in one place.

    let proof = deserialize_compressed_strict_with::<TorusAggregateProof<D>>(bytes, |_| Ok(()))?;

    let coordinates = aggregate_proof_target_values::<Bls12_377, TorusTarget, D>(&proof);
    let targets = decompress_targets(&coordinates)?;
    let mut targets = targets.into_iter();
    let proof = try_map_aggregate_proof_target::<
        Bls12_377,
        TorusTarget,
        PairingOutput<Bls12_377>,
        D,
        SerializationError,
    >(&proof, |_| {
        targets.next().ok_or(SerializationError::InvalidData)
    })?;
    if targets.next().is_some() {
        return Err(SerializationError::InvalidData);
    }
    validate_regular_proof(&proof)?;
    Ok(proof)
}

#[cfg(test)]
mod tests {
    use ark_ec::pairing::{Pairing, PairingOutput};
    use ark_ff::{One, UniformRand, Zero};
    use decaf377::{Bls12_377, Fp};
    use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};

    use super::{compress_targets, decompress_targets, TorusTarget};
    use crate::{
        decode_wrapped_aggregate_proof, decode_wrapped_torus_v2_aggregate_proof,
        encode_wrapped_aggregate_proof, encode_wrapped_torus_v2_aggregate_proof,
        AggregateProofBytesError,
    };

    #[test]
    fn wrapper_versions_are_disjoint() {
        let digest = [7u8; 32];
        let inner = [1, 2, 3, 4, 5];
        let v1 = encode_wrapped_aggregate_proof(digest, &inner).unwrap();
        let v2 = encode_wrapped_torus_v2_aggregate_proof(digest, &inner).unwrap();

        assert_eq!(
            decode_wrapped_torus_v2_aggregate_proof(&v2, digest, None).unwrap(),
            inner
        );
        assert_eq!(
            decode_wrapped_aggregate_proof(&v2, digest, None),
            Err(AggregateProofBytesError::BadVersion)
        );
        assert_eq!(
            decode_wrapped_torus_v2_aggregate_proof(&v1, digest, None),
            Err(AggregateProofBytesError::BadVersion)
        );
    }

    #[test]
    fn target_torus_round_trip_including_identity() {
        let mut rng = ChaCha20Rng::seed_from_u64(0x7022_377);
        let mut values = vec![PairingOutput::<Bls12_377>::zero()];
        values.extend((0..128).map(|_| PairingOutput::<Bls12_377>::rand(&mut rng)));
        let refs = values.iter().collect::<Vec<_>>();
        let compressed = compress_targets(&refs).expect("compress target values");
        let compressed_refs = compressed.iter().collect::<Vec<_>>();
        let decoded = decompress_targets(&compressed_refs).expect("decompress target values");
        assert_eq!(decoded, values);
    }

    #[test]
    fn every_torus_coordinate_round_trips() {
        let mut rng = ChaCha20Rng::seed_from_u64(0xaff1_6e);
        let mut coordinates = vec![TorusTarget::default()];
        coordinates.extend((0..128).map(|_| {
            TorusTarget([
                Fp::rand(&mut rng),
                Fp::rand(&mut rng),
                Fp::rand(&mut rng),
                Fp::rand(&mut rng),
                Fp::rand(&mut rng),
                Fp::rand(&mut rng),
            ])
        }));
        let refs = coordinates.iter().collect::<Vec<_>>();
        let targets = decompress_targets(&refs).expect("decompress coordinates");
        let target_refs = targets.iter().collect::<Vec<_>>();
        let encoded = compress_targets(&target_refs).expect("recompress target values");
        assert_eq!(encoded, coordinates);
    }

    #[test]
    fn identity_uses_zero_and_minus_one_is_rejected() {
        let identity = PairingOutput::<Bls12_377>::zero();
        assert_eq!(
            compress_targets(&[&identity]).unwrap(),
            [TorusTarget::default()]
        );

        let minus_one = PairingOutput::<Bls12_377>(-<Bls12_377 as Pairing>::TargetField::one());
        assert!(compress_targets(&[&minus_one]).is_err());
    }
}
