use anyhow::{anyhow, Context, Result};
use ark_ec::{AffineRepr, CurveGroup};
#[cfg(test)]
use ark_ff::One;
use shieldd_sdk_compliance::{ComplianceLeaf, IndexedLeaf, MerklePath};

use crate::gnark::binary::{put_bytes, put_u32, BinaryCursor, MAX_MERKLE_PATH_LAYERS};

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
    pub address: [u8; 48],
    pub asset_id: [u8; 32],
    pub capk_affine: PointAffineBytes,
    pub rnk_dh_pk_affine: PointAffineBytes,
    pub rnk_commitment: [u8; 32],
    pub status: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexedLeafBinary {
    pub value: [u8; 32],
    pub next_index: u64,
    pub next_value: [u8; 32],
    pub daily_volume_limit: [u8; 16],
    pub route_policy_hash: [u8; 32],
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
    if path.layers.len() != MAX_MERKLE_PATH_LAYERS {
        return Err(anyhow!(
            "merkle path layer count {} must equal {MAX_MERKLE_PATH_LAYERS}",
            path.layers.len()
        ));
    }
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
    put_bytes(buf, &leaf.daily_volume_limit);
    put_bytes(buf, &leaf.route_policy_hash);
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
        daily_volume_limit: cursor.read_fixed::<16>()?,
        route_policy_hash: cursor.read_fixed::<32>()?,
        ring_id_hash: cursor.read_fixed::<32>()?,
        policy_id_hash: cursor.read_fixed::<32>()?,
        permission_hash: cursor.read_fixed::<32>()?,
        resource_hash: cursor.read_fixed::<32>()?,
    })
}

pub(crate) fn merkle_path_from_typed(path: &MerklePath) -> Result<MerklePathBinary> {
    if path.layers.len() != MAX_MERKLE_PATH_LAYERS {
        return Err(anyhow!(
            "merkle path layer count {} must equal {MAX_MERKLE_PATH_LAYERS}",
            path.layers.len()
        ));
    }
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
        daily_volume_limit: leaf.params.daily_volume_limit.to_le_bytes(),
        route_policy_hash: leaf.params.route_policy_hash.to_bytes(),
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
        address_bytes.len() == 48,
        "compliance leaf address is {} bytes, expected 48",
        address_bytes.len()
    );
    let mut address = [0u8; 48];
    address.copy_from_slice(&address_bytes);
    Ok(ComplianceLeafBinary {
        address,
        asset_id: leaf.asset_id.0.to_bytes(),
        capk_affine: point_affine_bytes(leaf.capk)?,
        rnk_dh_pk_affine: point_affine_bytes(leaf.rnk_dh_pk)?,
        rnk_commitment: leaf.rnk_commitment.to_bytes(),
        status: leaf.lifecycle_field().to_bytes(),
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

#[cfg(test)]
pub(crate) fn point_affine_compress_to_field_bytes(point: &PointAffineBytes) -> [u8; 32] {
    fn abs(value: decaf377::Fq) -> decaf377::Fq {
        if value.to_bytes()[0] & 1 == 0 {
            value
        } else {
            -value
        }
    }

    let x = decaf377::Fq::from_le_bytes_mod_order(&point.x);
    let y = decaf377::Fq::from_le_bytes_mod_order(&point.y);
    let t = x * y;
    let u1 = (x + t) * (x - t);
    let a_minus_d = -decaf377::Fq::from(3022u64);
    let (_, v) =
        decaf377::Fq::sqrt_ratio_zeta(&decaf377::Fq::one(), &(u1 * a_minus_d * x.square()));
    let u2 = abs(v * u1);
    abs(a_minus_d * v * (u2 - t) * x).to_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_merkle_path_requires_exact_depth() {
        for layer_count in [MAX_MERKLE_PATH_LAYERS - 1, MAX_MERKLE_PATH_LAYERS + 1] {
            let path = MerklePathBinary {
                layers: vec![[[0u8; 32]; 3]; layer_count],
            };
            let err = encode_merkle_path(&mut Vec::new(), &path)
                .expect_err("non-exact merkle path should fail");
            assert!(
                err.to_string().contains("merkle path layer count"),
                "unexpected error: {err:#}"
            );
        }
    }
}
