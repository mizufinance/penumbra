use anyhow::{anyhow, Context, Result};
use ark_ec::{AffineRepr, CurveGroup};
use penumbra_sdk_compliance::{ComplianceLeaf, IndexedLeaf, MerklePath};

use crate::gnark::binary::{put_bytes, put_u32, BinaryCursor};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PointAffineBytes {
    pub x: [u8; 32],
    pub y: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MerklePathBinary {
    pub layers: Vec<[[u8; 32]; 3]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComplianceLeafBinary {
    pub address: [u8; 80],
    pub asset_id: [u8; 32],
    pub d: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexedLeafBinary {
    pub value: [u8; 32],
    pub next_index: u64,
    pub next_value: [u8; 32],
    pub dk_pub: [u8; 32],
    pub threshold: [u8; 16],
    pub channels_hash: [u8; 32],
    pub ring_pk: [u8; 32],
    pub ring_id_hash: [u8; 32],
    pub policy_id_hash: [u8; 32],
    pub permission_hash: [u8; 32],
    pub resource_hash: [u8; 32],
}

pub(crate) fn encode_point_affine(buf: &mut Vec<u8>, point: &PointAffineBytes) {
    put_bytes(buf, &point.x);
    put_bytes(buf, &point.y);
}

pub(crate) fn encode_merkle_path(buf: &mut Vec<u8>, path: &MerklePathBinary) -> Result<()> {
    put_u32(
        buf,
        u32::try_from(path.layers.len()).context("merkle layer count exceeds u32")?,
    );
    for layer in &path.layers {
        put_u32(buf, 3);
        for sibling in layer {
            put_bytes(buf, sibling);
        }
    }
    Ok(())
}

pub(crate) fn encode_indexed_leaf(buf: &mut Vec<u8>, leaf: &IndexedLeafBinary) {
    put_bytes(buf, &leaf.value);
    put_bytes(buf, &leaf.next_index.to_le_bytes());
    put_bytes(buf, &leaf.next_value);
    put_bytes(buf, &leaf.dk_pub);
    put_bytes(buf, &leaf.threshold);
    put_bytes(buf, &leaf.channels_hash);
    put_bytes(buf, &leaf.ring_pk);
    put_bytes(buf, &leaf.ring_id_hash);
    put_bytes(buf, &leaf.policy_id_hash);
    put_bytes(buf, &leaf.permission_hash);
    put_bytes(buf, &leaf.resource_hash);
}

pub(crate) fn decode_indexed_leaf(cursor: &mut BinaryCursor<'_>) -> Result<IndexedLeafBinary> {
    Ok(IndexedLeafBinary {
        value: cursor.read_fixed::<32>()?,
        next_index: cursor.read_u64()?,
        next_value: cursor.read_fixed::<32>()?,
        dk_pub: cursor.read_fixed::<32>()?,
        threshold: cursor.read_fixed::<16>()?,
        channels_hash: cursor.read_fixed::<32>()?,
        ring_pk: cursor.read_fixed::<32>()?,
        ring_id_hash: cursor.read_fixed::<32>()?,
        policy_id_hash: cursor.read_fixed::<32>()?,
        permission_hash: cursor.read_fixed::<32>()?,
        resource_hash: cursor.read_fixed::<32>()?,
    })
}

pub(crate) fn merkle_path_from_typed(path: &MerklePath) -> Result<MerklePathBinary> {
    let mut layers = Vec::with_capacity(path.layers.len());
    for layer in &path.layers {
        if layer.siblings.len() != 3 {
            return Err(anyhow!(
                "expected 3 siblings per merkle layer, got {}",
                layer.siblings.len()
            ));
        }
        let mut encoded = [[0u8; 32]; 3];
        for (index, sibling) in layer.siblings.iter().enumerate() {
            encoded[index].copy_from_slice(
                sibling
                    .get(..32)
                    .ok_or_else(|| anyhow!("merkle sibling must be 32 bytes"))?,
            );
        }
        layers.push(encoded);
    }
    Ok(MerklePathBinary { layers })
}

pub(crate) fn indexed_leaf_from_typed(leaf: &IndexedLeaf) -> IndexedLeafBinary {
    IndexedLeafBinary {
        value: leaf.value.to_bytes(),
        next_index: leaf.next_index,
        next_value: leaf.next_value.to_bytes(),
        dk_pub: leaf.params.dk_pub.vartime_compress().0,
        threshold: leaf.params.threshold.to_le_bytes(),
        channels_hash: leaf.params.channels_hash.to_bytes(),
        ring_pk: leaf.ring.ring_pk.vartime_compress().0,
        ring_id_hash: leaf.ring.ring_id_hash.to_bytes(),
        policy_id_hash: leaf.ring.policy_id_hash.to_bytes(),
        permission_hash: leaf.ring.permission_hash.to_bytes(),
        resource_hash: leaf.ring.resource_hash.to_bytes(),
    }
}

pub(crate) fn compliance_leaf_from_typed(
    leaf: &ComplianceLeaf,
) -> anyhow::Result<ComplianceLeafBinary> {
    let address_bytes = leaf.address.to_vec();
    anyhow::ensure!(
        address_bytes.len() == 80,
        "compliance leaf address is {} bytes, expected 80",
        address_bytes.len()
    );
    let mut address = [0u8; 80];
    address.copy_from_slice(&address_bytes);
    Ok(ComplianceLeafBinary {
        address,
        asset_id: leaf.asset_id.0.to_bytes(),
        d: leaf.d.to_bytes(),
    })
}

pub(crate) fn point_affine_bytes(point: decaf377::Element) -> Result<PointAffineBytes> {
    let affine = point.into_affine();
    let (x, y) = affine
        .xy()
        .ok_or_else(|| anyhow!("decaf point is identity, affine coordinates unavailable"))?;
    Ok(PointAffineBytes {
        x: x.to_bytes(),
        y: y.to_bytes(),
    })
}

pub(crate) fn point_affine_bytes_with_fallback(
    point: decaf377::Element,
    fallback: decaf377::Element,
) -> Result<PointAffineBytes> {
    match point_affine_bytes(point) {
        Ok(bytes) => Ok(bytes),
        Err(_) => point_affine_bytes(fallback),
    }
}
