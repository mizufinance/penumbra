//! Proto conversion helpers for compliance data structures.

use anyhow::Result;
use decaf377::Fr;
use shieldd_sdk_compliance::ComplianceLeaf;
use shieldd_sdk_proto::core::component::compliance::v1 as compliance_pb;
use shieldd_sdk_tct::StateCommitment;

/// Convert a `ComplianceLeaf` to its proto representation.
pub fn compliance_leaf_to_proto(leaf: &ComplianceLeaf) -> compliance_pb::ComplianceLeaf {
    compliance_pb::ComplianceLeaf {
        address: Some(leaf.address.clone().into()),
        asset_id: Some(leaf.asset_id.into()),
        capk: leaf.capk.vartime_compress().0.to_vec(),
        rnk_dh_pk: leaf.rnk_dh_pk.vartime_compress().0.to_vec(),
        rnk_commitment: leaf.rnk_commitment.to_bytes().to_vec(),
        status: compliance_pb::UserAssetStatus::from(leaf.status) as i32,
        freeze_generation: leaf.freeze_generation,
        frozen_since_height: leaf.frozen_since_height,
    }
}

/// Parse a `ComplianceLeaf` from its proto representation.
pub fn compliance_leaf_from_proto(
    proto: compliance_pb::ComplianceLeaf,
    context: &str,
) -> Result<ComplianceLeaf> {
    ComplianceLeaf::try_from(proto)
        .map_err(|e| anyhow::anyhow!("invalid compliance leaf in {}: {}", context, e))
}

/// Convert an `IndexedLeaf` to its proto representation.
pub fn indexed_leaf_to_proto(
    leaf: &shieldd_sdk_compliance::IndexedLeaf,
) -> compliance_pb::IndexedLeafData {
    leaf.clone().into()
}

/// Parse an ephemeral secret (Fr) from proto bytes.
pub fn parse_ephemeral_secret(bytes: &[u8]) -> Result<Option<Fr>> {
    if bytes.is_empty() {
        return Ok(None);
    }
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid ephemeral secret length"))?;
    let fr = Fr::from_bytes_checked(&arr)
        .map_err(|_| anyhow::anyhow!("invalid ephemeral secret bytes"))?;
    Ok(Some(fr))
}

/// Parse the required per-transaction compliance nonce.
pub fn parse_tx_blinding_nonce(bytes: &[u8]) -> Result<Fr> {
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid tx_blinding_nonce length"))?;
    Fr::from_bytes_checked(&arr).map_err(|_| anyhow::anyhow!("invalid tx_blinding_nonce bytes"))
}

/// Parse a StateCommitment from an optional proto.
pub fn parse_state_commitment(
    proto: Option<shieldd_sdk_proto::shieldd::crypto::tct::v1::StateCommitment>,
) -> Result<Option<StateCommitment>> {
    proto.map(|c| c.try_into().map_err(Into::into)).transpose()
}

/// Parse a MerklePath from an optional proto.
pub fn parse_merkle_path(
    proto: Option<compliance_pb::MerklePath>,
) -> Result<Option<shieldd_sdk_compliance::MerklePath>> {
    proto.map(|p| p.try_into()).transpose()
}

/// Parse an IndexedLeaf from an optional proto.
pub fn parse_indexed_leaf(
    proto: Option<compliance_pb::IndexedLeafData>,
) -> Result<Option<shieldd_sdk_compliance::IndexedLeaf>> {
    proto
        .map(shieldd_sdk_compliance::IndexedLeaf::try_from)
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fr_modulus_bytes() -> [u8; 32] {
        let mut modulus = (-Fr::from(1u64)).to_bytes();
        for byte in &mut modulus {
            let (next, carry) = byte.overflowing_add(1);
            *byte = next;
            if !carry {
                break;
            }
        }
        modulus
    }

    #[test]
    fn transaction_blinding_nonce_is_required_and_canonical() {
        assert_eq!(
            parse_tx_blinding_nonce(&Fr::from(7u64).to_bytes()).expect("canonical nonce"),
            Fr::from(7u64)
        );
        for bytes in [&[][..], &[0u8; 31], &fr_modulus_bytes()] {
            parse_tx_blinding_nonce(bytes)
                .expect_err("missing, short, or non-canonical nonce must fail");
        }
    }
}
