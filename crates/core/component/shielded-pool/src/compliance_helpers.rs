//! Proto conversion helpers for compliance data structures.

use anyhow::Result;
use decaf377::{Fq, Fr};
use penumbra_sdk_compliance::ComplianceLeaf;
use penumbra_sdk_proto::core::component::compliance::v1 as compliance_pb;
use penumbra_sdk_tct::StateCommitment;

/// Convert a `ComplianceLeaf` to its proto representation.
pub fn compliance_leaf_to_proto(leaf: &ComplianceLeaf) -> compliance_pb::ComplianceLeaf {
    compliance_pb::ComplianceLeaf {
        address: Some(leaf.address.clone().into()),
        asset_id: Some(leaf.asset_id.into()),
        d: leaf.d.to_bytes().to_vec(),
        slot_id: leaf.slot_id,
        slot_derivation: leaf.slot_derivation.to_bytes().to_vec(),
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
    leaf: &penumbra_sdk_compliance::IndexedLeaf,
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

/// Parse a tx_blinding_nonce (Fr) from proto bytes.
pub fn parse_tx_blinding_nonce(bytes: &[u8]) -> Result<Option<Fr>> {
    if bytes.is_empty() {
        return Ok(None);
    }
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid tx_blinding_nonce length"))?;
    Fr::from_bytes_checked(&arr)
        .map(Some)
        .map_err(|_| anyhow::anyhow!("invalid tx_blinding_nonce bytes"))
}

/// Parse a StateCommitment from an optional proto.
pub fn parse_state_commitment(
    proto: Option<penumbra_sdk_proto::penumbra::crypto::tct::v1::StateCommitment>,
) -> Result<Option<StateCommitment>> {
    proto.map(|c| c.try_into().map_err(Into::into)).transpose()
}

/// Parse a MerklePath from an optional proto.
pub fn parse_merkle_path(
    proto: Option<compliance_pb::MerklePath>,
) -> Result<Option<penumbra_sdk_compliance::MerklePath>> {
    proto.map(|p| p.try_into()).transpose()
}

/// Parse an IndexedLeaf from an optional proto.
pub fn parse_indexed_leaf(
    proto: Option<compliance_pb::IndexedLeafData>,
) -> Result<Option<penumbra_sdk_compliance::IndexedLeaf>> {
    proto
        .map(penumbra_sdk_compliance::IndexedLeaf::try_from)
        .transpose()
}

pub fn default_state_commitment() -> StateCommitment {
    StateCommitment(Fq::from(0u64))
}

pub fn default_indexed_leaf() -> penumbra_sdk_compliance::IndexedLeaf {
    penumbra_sdk_compliance::IndexedLeaf::with_default_policy(
        Fq::from(0u64),
        0,
        penumbra_sdk_compliance::indexed_tree::FQ_MAX.clone(),
    )
}
