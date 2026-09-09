use anyhow::{anyhow, ensure, Result};
use ark_groth16::Proof;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use decaf377::Bls12_377;
use shieldd_sdk_proof_params::GROTH16_PROOF_LENGTH_BYTES;

/// Decodes the deployed Groth16 proof encoding without accepting byte aliases.
pub(crate) fn decode(bytes: &[u8]) -> Result<Proof<Bls12_377>> {
    ensure!(
        bytes.len() == GROTH16_PROOF_LENGTH_BYTES,
        "malformed Groth16 proof length: expected {GROTH16_PROOF_LENGTH_BYTES} bytes, got {}",
        bytes.len()
    );

    let mut remaining = bytes;
    let proof = Proof::deserialize_compressed(&mut remaining)
        .map_err(|error| anyhow!("malformed Groth16 proof encoding: {error}"))?;
    ensure!(
        remaining.is_empty(),
        "malformed Groth16 proof encoding: trailing bytes"
    );

    let mut canonical = Vec::with_capacity(GROTH16_PROOF_LENGTH_BYTES);
    proof
        .serialize_compressed(&mut canonical)
        .map_err(|error| anyhow!("could not re-serialize Groth16 proof: {error}"))?;
    ensure!(canonical == bytes, "non-canonical Groth16 proof encoding");

    Ok(proof)
}

#[cfg(test)]
mod tests {
    use ark_ec::{pairing::Pairing, AffineRepr};
    use ark_groth16::Proof;
    use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
    use decaf377::Bls12_377;
    use shieldd_sdk_proto::shieldd::core::component::shielded_pool::v1 as pb;

    use super::decode;
    use crate::{NoteReshapeProof, ShieldedWithdrawalProof, TransferProof};

    type G1 = <Bls12_377 as Pairing>::G1Affine;
    type G2 = <Bls12_377 as Pairing>::G2Affine;

    fn canonical_proof_bytes() -> Vec<u8> {
        let proof = Proof::<Bls12_377> {
            a: G1::zero(),
            b: G2::zero(),
            c: G1::zero(),
        };
        let mut bytes = Vec::new();
        proof
            .serialize_compressed(&mut bytes)
            .expect("zero proof should serialize");
        bytes
    }

    fn assert_all_domain_decoders_accept(bytes: &[u8]) {
        TransferProof::try_from(pb::ZkTransferProof {
            inner: bytes.to_vec(),
        })
        .expect("transfer should accept canonical proof bytes");
        NoteReshapeProof::try_from(pb::ZkNoteReshapeProof {
            inner: bytes.to_vec(),
        })
        .expect("note reshape should accept canonical proof bytes");
        ShieldedWithdrawalProof::try_from(pb::ZkShieldedWithdrawalProof {
            inner: bytes.to_vec(),
        })
        .expect("shielded withdrawal should accept canonical proof bytes");
    }

    fn assert_all_domain_decoders_reject(bytes: &[u8]) {
        assert!(
            TransferProof::try_from(pb::ZkTransferProof {
                inner: bytes.to_vec()
            })
            .is_err(),
            "transfer accepted a malformed proof"
        );
        assert!(
            NoteReshapeProof::try_from(pb::ZkNoteReshapeProof {
                inner: bytes.to_vec()
            })
            .is_err(),
            "note reshape accepted a malformed proof"
        );
        assert!(
            ShieldedWithdrawalProof::try_from(pb::ZkShieldedWithdrawalProof {
                inner: bytes.to_vec()
            })
            .is_err(),
            "shielded withdrawal accepted a malformed proof"
        );
    }

    #[test]
    fn all_domain_decoders_accept_the_canonical_encoding() {
        let bytes = canonical_proof_bytes();
        decode(&bytes).expect("strict decoder should accept canonical proof bytes");
        assert_all_domain_decoders_accept(&bytes);
    }

    #[test]
    fn all_domain_decoders_reject_short_and_trailing_encodings() {
        let bytes = canonical_proof_bytes();
        assert_all_domain_decoders_reject(&bytes[..bytes.len() - 1]);

        let mut trailing = bytes;
        trailing.push(0);
        assert_all_domain_decoders_reject(&trailing);
    }

    #[test]
    fn all_domain_decoders_reject_arkworks_infinity_alias() {
        let mut alias = canonical_proof_bytes();
        // Compressed field coordinates are little-endian. Arkworks accepts a
        // nonzero coordinate on an infinity point and discards the coordinate.
        alias[0] |= 1;
        assert!(
            Proof::<Bls12_377>::deserialize_compressed(&alias[..]).is_ok(),
            "test vector must exercise an encoding accepted by the permissive decoder"
        );
        assert_all_domain_decoders_reject(&alias);
    }
}
