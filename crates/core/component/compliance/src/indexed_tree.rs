use anyhow::{anyhow, bail, Result};
use ark_ff::{BigInteger, PrimeField};
use decaf377::Fq;
use once_cell::sync::Lazy;
use penumbra_sdk_proto::{core::component::compliance::v1 as pb, DomainType};
use penumbra_sdk_tct::StateCommitment;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::structs::{canonical_route_policy_string, AssetParams, AssetPolicy, RingData};
use crate::tree::DEFAULT_DEPTH;

/// Canonical numeric ordering key for `Fq`.
///
/// `into_bigint()` returns canonical non-Montgomery limbs. Serializing those
/// limbs big-endian makes lexicographic byte ordering equal numeric ordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FqOrdKey([u8; 32]);

impl From<Fq> for FqOrdKey {
    fn from(value: Fq) -> Self {
        let bytes = value.into_bigint().to_bytes_be();
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        Self(key)
    }
}

#[cfg(test)]
#[inline]
fn fq_less_than(a: &Fq, b: &Fq) -> bool {
    FqOrdKey::from(*a) < FqOrdKey::from(*b)
}

// --- Domain separators ---

/// Domain separator for IMT leaf commitments.
pub static IMT_LEAF_DOMAIN_SEP: Lazy<Fq> = Lazy::new(|| {
    let hash = blake2b_simd::Params::default()
        .personal(b"pen.imt.leaf____")
        .hash(b"");
    Fq::from_le_bytes_mod_order(hash.as_bytes())
});

/// Domain separator for params sub-hash (Penumbra-decided: dk_pub, threshold, slot count, IBC route policy).
pub static PARAMS_DOMAIN_SEP: Lazy<Fq> = Lazy::new(|| {
    let hash = blake2b_simd::Params::default()
        .personal(b"pen.imt.params__")
        .hash(b"");
    Fq::from_le_bytes_mod_order(hash.as_bytes())
});

/// Domain separator for ring sub-hash (Orbis-decided: ring_pk, ring_id, policy_id, permission, resource).
pub static RING_DOMAIN_SEP: Lazy<Fq> = Lazy::new(|| {
    let hash = blake2b_simd::Params::default()
        .personal(b"pen.imt.ring____")
        .hash(b"");
    Fq::from_le_bytes_mod_order(hash.as_bytes())
});

/// The maximum value representable in the field (modulus - 1).
pub static FQ_MAX: Lazy<Fq> = Lazy::new(|| Fq::from(0u64) - Fq::from(1u64));

// --- String-to-Fq helpers ---

/// Hash a string to a field element for inclusion in the IMT leaf commitment.
pub fn string_to_fq(s: &str) -> Fq {
    let hash = blake2b_simd::Params::new()
        .hash_length(64)
        .personal(b"pen.imt.str_hash")
        .hash(s.as_bytes());
    Fq::from_le_bytes_mod_order(hash.as_bytes())
}

/// Hash a canonical IBC route policy to a field element.
pub fn route_policy_to_fq(params: &AssetParams) -> Fq {
    string_to_fq(&canonical_route_policy_string(
        &params.ibc_origin,
        &params.allowed_ibc_routes,
    ))
}

// --- Policy sub-structs ---

/// Penumbra-decided policy fields bound into the IMT leaf.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeafParams {
    pub dk_pub: decaf377::Element,
    pub threshold: u128,
    pub slot_count: u32,
    pub route_policy_hash: Fq,
}

impl LeafParams {
    /// Construct from an AssetParams (hashes the canonical route policy).
    pub fn from_asset_params(p: &AssetParams) -> Self {
        Self {
            dk_pub: p.dk_pub,
            threshold: p.threshold,
            slot_count: p.slot_count,
            route_policy_hash: route_policy_to_fq(p),
        }
    }
}

impl Default for LeafParams {
    fn default() -> Self {
        Self {
            dk_pub: decaf377::Element::default(),
            threshold: u128::MAX,
            slot_count: 0,
            route_policy_hash: string_to_fq(""),
        }
    }
}

/// Orbis-decided policy fields bound into the IMT leaf.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeafRing {
    pub ring_pk: decaf377::Element,
    pub ring_id_hash: Fq,
    pub policy_id_hash: Fq,
    pub permission_hash: Fq,
    pub resource_hash: Fq,
}

impl LeafRing {
    /// Construct from RingData (hashes all string fields).
    pub fn from_ring_data(r: &RingData) -> Self {
        Self {
            ring_pk: r.ring_pk,
            ring_id_hash: string_to_fq(&r.ring_id),
            policy_id_hash: string_to_fq(&r.policy_id),
            permission_hash: string_to_fq(&r.permission),
            resource_hash: string_to_fq(&r.resource),
        }
    }
}

impl Default for LeafRing {
    fn default() -> Self {
        Self {
            ring_pk: decaf377::Element::default(),
            ring_id_hash: string_to_fq(""),
            policy_id_hash: string_to_fq(""),
            permission_hash: string_to_fq(""),
            resource_hash: string_to_fq(""),
        }
    }
}

// --- Precomputed default sub-hashes ---

static DEFAULT_PARAMS_HASH: Lazy<Fq> = Lazy::new(|| {
    let p = LeafParams::default();
    let dk_pub_fq = p.dk_pub.vartime_compress_to_field();
    let threshold_fq = Fq::from(p.threshold);
    poseidon377::hash_4(
        &PARAMS_DOMAIN_SEP,
        (
            dk_pub_fq,
            threshold_fq,
            Fq::from(p.slot_count),
            p.route_policy_hash,
        ),
    )
});

static DEFAULT_RING_HASH: Lazy<Fq> = Lazy::new(|| {
    let r = LeafRing::default();
    let ring_pk_fq = r.ring_pk.vartime_compress_to_field();
    poseidon377::hash_5(
        &RING_DOMAIN_SEP,
        (
            ring_pk_fq,
            r.ring_id_hash,
            r.policy_id_hash,
            r.permission_hash,
            r.resource_hash,
        ),
    )
});

/// Precomputed zero hashes for each level of the IMT.
pub static IMT_ZERO_HASHES: Lazy<Vec<StateCommitment>> = Lazy::new(|| {
    let mut zeros = Vec::with_capacity((DEFAULT_DEPTH + 1) as usize);

    // Level 0: empty leaf with default (unregulated) policy
    let empty_leaf_hash = poseidon377::hash_5(
        &IMT_LEAF_DOMAIN_SEP,
        (
            Fq::from(0u64),       // value
            Fq::from(0u64),       // next_index
            Fq::from(0u64),       // next_value
            *DEFAULT_PARAMS_HASH, // params sub-hash
            *DEFAULT_RING_HASH,   // ring sub-hash
        ),
    );
    zeros.push(StateCommitment(empty_leaf_hash));

    for i in 1..=(DEFAULT_DEPTH as usize) {
        let prev = zeros[i - 1].0;
        let hash = poseidon377::hash_4(&Fq::from(0u64), (prev, prev, prev, prev));
        zeros.push(StateCommitment(hash));
    }

    zeros
});

// --- IndexedLeaf ---

/// A leaf in the Indexed Merkle Tree forming a sorted linked list.
///
/// All policy fields are bound into the commitment via sub-structured Poseidon:
///   params_hash = hash_4(PARAMS_DOMAIN, dk_pub_fq, threshold_fq, slot_count_fq, route_policy_hash)
///   ring_hash   = hash_5(RING_DOMAIN, ring_pk_fq, ring_id_hash, policy_id_hash, permission_hash, resource_hash)
///   leaf_commit = hash_5(LEAF_DOMAIN, value, next_index, next_value, params_hash, ring_hash)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexedLeaf {
    /// The value stored in this leaf (e.g., asset_id).
    pub value: Fq,
    /// Position in the tree of the next-higher value leaf.
    pub next_index: u64,
    /// The value at next_index (for efficient gap verification).
    pub next_value: Fq,
    /// Penumbra-decided policy (dk_pub, threshold, IBC routes).
    pub params: LeafParams,
    /// Orbis-decided policy (ring_pk, ring_id, policy_id, permission, resource).
    pub ring: LeafRing,
}

/// Result of an IMT insertion with full data for client sync.
#[derive(Debug, Clone)]
pub struct InsertResult {
    pub position: u64,
    pub indexed_leaf: IndexedLeaf,
    pub low_leaf_position: u64,
    pub updated_low_leaf: IndexedLeaf,
}

impl IndexedLeaf {
    /// Create a leaf with explicit policy sub-structs.
    pub fn new(
        value: Fq,
        next_index: u64,
        next_value: Fq,
        params: LeafParams,
        ring: LeafRing,
    ) -> Self {
        Self {
            value,
            next_index,
            next_value,
            params,
            ring,
        }
    }

    /// Create a leaf from a full AssetPolicy (hashes strings internally).
    pub fn from_policy(value: Fq, next_index: u64, next_value: Fq, policy: &AssetPolicy) -> Self {
        Self {
            value,
            next_index,
            next_value,
            params: LeafParams::from_asset_params(&policy.params),
            ring: LeafRing::from_ring_data(&policy.ring),
        }
    }

    /// Create a leaf with default (unregulated) policy.
    pub fn with_default_policy(value: Fq, next_index: u64, next_value: Fq) -> Self {
        Self {
            value,
            next_index,
            next_value,
            params: LeafParams::default(),
            ring: LeafRing::default(),
        }
    }

    /// Compute the Poseidon commitment for this leaf (3 hashes).
    pub fn commit(&self) -> StateCommitment {
        let dk_pub_fq = self.params.dk_pub.vartime_compress_to_field();
        let threshold_fq = Fq::from(self.params.threshold);
        let params_hash = poseidon377::hash_4(
            &PARAMS_DOMAIN_SEP,
            (
                dk_pub_fq,
                threshold_fq,
                Fq::from(self.params.slot_count),
                self.params.route_policy_hash,
            ),
        );

        let ring_pk_fq = self.ring.ring_pk.vartime_compress_to_field();
        let ring_hash = poseidon377::hash_5(
            &RING_DOMAIN_SEP,
            (
                ring_pk_fq,
                self.ring.ring_id_hash,
                self.ring.policy_id_hash,
                self.ring.permission_hash,
                self.ring.resource_hash,
            ),
        );

        let hash = poseidon377::hash_5(
            &IMT_LEAF_DOMAIN_SEP,
            (
                self.value,
                Fq::from(self.next_index),
                self.next_value,
                params_hash,
                ring_hash,
            ),
        );
        StateCommitment(hash)
    }

    /// Convenience accessors for circuit-relevant policy fields.
    pub fn dk_pub(&self) -> &decaf377::Element {
        &self.params.dk_pub
    }
    pub fn threshold(&self) -> u128 {
        self.params.threshold
    }
    pub fn ring_pk(&self) -> &decaf377::Element {
        &self.ring.ring_pk
    }
}

// --- Serialization ---

#[derive(Serialize, Deserialize)]
struct IndexedLeafSerde {
    value: [u8; 32],
    next_index: u64,
    next_value: [u8; 32],
    dk_pub: [u8; 32],
    threshold: u128,
    slot_count: u32,
    route_policy_hash: [u8; 32],
    ring_pk: [u8; 32],
    ring_id_hash: [u8; 32],
    policy_id_hash: [u8; 32],
    permission_hash: [u8; 32],
    resource_hash: [u8; 32],
}

impl Serialize for IndexedLeaf {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let helper = IndexedLeafSerde {
            value: self.value.to_bytes(),
            next_index: self.next_index,
            next_value: self.next_value.to_bytes(),
            dk_pub: self.params.dk_pub.vartime_compress().0,
            threshold: self.params.threshold,
            slot_count: self.params.slot_count,
            route_policy_hash: self.params.route_policy_hash.to_bytes(),
            ring_pk: self.ring.ring_pk.vartime_compress().0,
            ring_id_hash: self.ring.ring_id_hash.to_bytes(),
            policy_id_hash: self.ring.policy_id_hash.to_bytes(),
            permission_hash: self.ring.permission_hash.to_bytes(),
            resource_hash: self.ring.resource_hash.to_bytes(),
        };
        helper.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for IndexedLeaf {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let h = IndexedLeafSerde::deserialize(deserializer)?;

        let value = Fq::from_bytes_checked(&h.value)
            .map_err(|_| serde::de::Error::custom("invalid value Fq bytes"))?;
        let next_value = Fq::from_bytes_checked(&h.next_value)
            .map_err(|_| serde::de::Error::custom("invalid next_value Fq bytes"))?;
        let dk_pub = decaf377::Encoding(h.dk_pub)
            .vartime_decompress()
            .map_err(|_| serde::de::Error::custom("invalid dk_pub encoding"))?;
        let route_policy_hash = Fq::from_bytes_checked(&h.route_policy_hash)
            .map_err(|_| serde::de::Error::custom("invalid route_policy_hash Fq bytes"))?;
        let ring_pk = decaf377::Encoding(h.ring_pk)
            .vartime_decompress()
            .map_err(|_| serde::de::Error::custom("invalid ring_pk encoding"))?;
        let ring_id_hash = Fq::from_bytes_checked(&h.ring_id_hash)
            .map_err(|_| serde::de::Error::custom("invalid ring_id_hash Fq bytes"))?;
        let policy_id_hash = Fq::from_bytes_checked(&h.policy_id_hash)
            .map_err(|_| serde::de::Error::custom("invalid policy_id_hash Fq bytes"))?;
        let permission_hash = Fq::from_bytes_checked(&h.permission_hash)
            .map_err(|_| serde::de::Error::custom("invalid permission_hash Fq bytes"))?;
        let resource_hash = Fq::from_bytes_checked(&h.resource_hash)
            .map_err(|_| serde::de::Error::custom("invalid resource_hash Fq bytes"))?;

        Ok(IndexedLeaf {
            value,
            next_index: h.next_index,
            next_value,
            params: LeafParams {
                dk_pub,
                threshold: h.threshold,
                slot_count: h.slot_count,
                route_policy_hash,
            },
            ring: LeafRing {
                ring_pk,
                ring_id_hash,
                policy_id_hash,
                permission_hash,
                resource_hash,
            },
        })
    }
}

// --- Proto conversions ---

impl DomainType for IndexedLeaf {
    type Proto = pb::IndexedLeafData;
}

impl From<IndexedLeaf> for pb::IndexedLeafData {
    fn from(leaf: IndexedLeaf) -> Self {
        pb::IndexedLeafData {
            value: leaf.value.to_bytes().to_vec(),
            next_index: leaf.next_index,
            next_value: leaf.next_value.to_bytes().to_vec(),
            dk_pub: leaf.params.dk_pub.vartime_compress().0.to_vec(),
            threshold: leaf.params.threshold.to_le_bytes().to_vec(),
            route_policy_hash: leaf.params.route_policy_hash.to_bytes().to_vec(),
            slot_count: leaf.params.slot_count.to_le_bytes().to_vec(),
            ring_pk: leaf.ring.ring_pk.vartime_compress().0.to_vec(),
            ring_id_hash: leaf.ring.ring_id_hash.to_bytes().to_vec(),
            policy_id_hash: leaf.ring.policy_id_hash.to_bytes().to_vec(),
            permission_hash: leaf.ring.permission_hash.to_bytes().to_vec(),
            resource_hash: leaf.ring.resource_hash.to_bytes().to_vec(),
        }
    }
}

/// Parse a proto bytes field as Fq, defaulting to string_to_fq("") if empty.
fn parse_fq_or_default(bytes: &[u8], field_name: &str) -> Result<Fq> {
    if bytes.len() == 32 {
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| anyhow!("{} must be 32 bytes", field_name))?;
        Fq::from_bytes_checked(&arr).map_err(|_| anyhow!("invalid {} Fq bytes", field_name))
    } else if bytes.is_empty() {
        Ok(string_to_fq(""))
    } else {
        bail!("{} must be 32 bytes, got {}", field_name, bytes.len())
    }
}

impl TryFrom<pb::IndexedLeafData> for IndexedLeaf {
    type Error = anyhow::Error;

    fn try_from(proto: pb::IndexedLeafData) -> Result<Self> {
        let value_bytes: [u8; 32] = proto.value.try_into().map_err(|v: Vec<u8>| {
            anyhow!("IndexedLeaf proto: value must be 32 bytes, got {}", v.len())
        })?;
        let next_value_bytes: [u8; 32] = proto.next_value.try_into().map_err(|v: Vec<u8>| {
            anyhow!(
                "IndexedLeaf proto: next_value must be 32 bytes, got {}",
                v.len()
            )
        })?;

        // Policy fields (default to unregulated if missing/empty)
        let dk_pub = if proto.dk_pub.len() == 32 {
            let bytes: [u8; 32] = proto
                .dk_pub
                .try_into()
                .map_err(|_| anyhow!("dk_pub must be 32 bytes"))?;
            decaf377::Encoding(bytes)
                .vartime_decompress()
                .map_err(|_| anyhow!("invalid dk_pub encoding"))?
        } else {
            decaf377::Element::default()
        };

        let threshold = if proto.threshold.len() == 16 {
            let bytes: [u8; 16] = proto
                .threshold
                .try_into()
                .map_err(|_| anyhow!("threshold must be 16 bytes"))?;
            u128::from_le_bytes(bytes)
        } else {
            u128::MAX
        };

        let route_policy_hash = parse_fq_or_default(&proto.route_policy_hash, "route_policy_hash")?;
        let slot_count = if proto.slot_count.len() == 4 {
            let bytes: [u8; 4] = proto
                .slot_count
                .try_into()
                .map_err(|_| anyhow!("slot_count must be 4 bytes"))?;
            u32::from_le_bytes(bytes)
        } else if proto.slot_count.is_empty() {
            0
        } else {
            bail!("slot_count must be 4 bytes, got {}", proto.slot_count.len())
        };

        let ring_pk = if proto.ring_pk.len() == 32 {
            let bytes: [u8; 32] = proto
                .ring_pk
                .try_into()
                .map_err(|_| anyhow!("ring_pk must be 32 bytes"))?;
            decaf377::Encoding(bytes)
                .vartime_decompress()
                .map_err(|_| anyhow!("invalid ring_pk encoding"))?
        } else {
            decaf377::Element::default()
        };

        let ring_id_hash = parse_fq_or_default(&proto.ring_id_hash, "ring_id_hash")?;
        let policy_id_hash = parse_fq_or_default(&proto.policy_id_hash, "policy_id_hash")?;
        let permission_hash = parse_fq_or_default(&proto.permission_hash, "permission_hash")?;
        let resource_hash = parse_fq_or_default(&proto.resource_hash, "resource_hash")?;

        Ok(IndexedLeaf {
            value: Fq::from_bytes_checked(&value_bytes)
                .map_err(|e| anyhow!("invalid value: {}", e))?,
            next_index: proto.next_index,
            next_value: Fq::from_bytes_checked(&next_value_bytes)
                .map_err(|e| anyhow!("invalid next_value: {}", e))?,
            params: LeafParams {
                dk_pub,
                threshold,
                slot_count,
                route_policy_hash,
            },
            ring: LeafRing {
                ring_pk,
                ring_id_hash,
                policy_id_hash,
                permission_hash,
                resource_hash,
            },
        })
    }
}

// --- IndexedMerkleTree ---

/// An Indexed Merkle Tree (IMT) for the asset registry.
///
/// Only regulated assets are stored. Unregulated status is proven via
/// non-membership proofs (the asset falls in a "gap" between two adjacent leaves).
#[derive(Clone, Debug)]
pub struct IndexedMerkleTree {
    depth: u8,
    /// Internal node hashes. Key format: level << 48 | position
    nodes: BTreeMap<u64, StateCommitment>,
    /// Leaf data at each position.
    leaves: BTreeMap<u64, IndexedLeaf>,
    /// Reverse index: value -> position for O(1) lookup.
    value_index: BTreeMap<[u8; 32], u64>,
    /// Ordered value -> position index for predecessor lookups.
    predecessor_index: BTreeMap<FqOrdKey, u64>,
    /// Current number of leaves (including sentinel).
    leaf_count: u64,
}

/// Serialization helper for IndexedMerkleTree.
#[derive(Serialize, Deserialize)]
struct IndexedMerkleTreeSerde {
    depth: u8,
    nodes: Vec<(u64, [u8; 32])>,
    leaves: Vec<(u64, IndexedLeafSerde)>,
    value_index: Vec<([u8; 32], u64)>,
    leaf_count: u64,
}

impl Serialize for IndexedMerkleTree {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let nodes: Vec<(u64, [u8; 32])> = self
            .nodes
            .iter()
            .map(|(k, v)| (*k, v.0.to_bytes()))
            .collect();

        let leaves: Vec<(u64, IndexedLeafSerde)> = self
            .leaves
            .iter()
            .map(|(k, v)| {
                (
                    *k,
                    IndexedLeafSerde {
                        value: v.value.to_bytes(),
                        next_index: v.next_index,
                        next_value: v.next_value.to_bytes(),
                        dk_pub: v.params.dk_pub.vartime_compress().0,
                        threshold: v.params.threshold,
                        slot_count: v.params.slot_count,
                        route_policy_hash: v.params.route_policy_hash.to_bytes(),
                        ring_pk: v.ring.ring_pk.vartime_compress().0,
                        ring_id_hash: v.ring.ring_id_hash.to_bytes(),
                        policy_id_hash: v.ring.policy_id_hash.to_bytes(),
                        permission_hash: v.ring.permission_hash.to_bytes(),
                        resource_hash: v.ring.resource_hash.to_bytes(),
                    },
                )
            })
            .collect();

        let value_index: Vec<([u8; 32], u64)> =
            self.value_index.iter().map(|(k, v)| (*k, *v)).collect();

        let helper = IndexedMerkleTreeSerde {
            depth: self.depth,
            nodes,
            leaves,
            value_index,
            leaf_count: self.leaf_count,
        };
        helper.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for IndexedMerkleTree {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let helper = IndexedMerkleTreeSerde::deserialize(deserializer)?;

        if helper.depth > DEFAULT_DEPTH {
            return Err(serde::de::Error::custom(format!(
                "IndexedMerkleTree depth {} exceeds maximum {}",
                helper.depth, DEFAULT_DEPTH
            )));
        }

        let nodes: BTreeMap<u64, StateCommitment> = helper
            .nodes
            .into_iter()
            .map(|(k, bytes)| {
                let fq = Fq::from_bytes_checked(&bytes)
                    .map_err(|_| serde::de::Error::custom("invalid node Fq bytes"))?;
                Ok((k, StateCommitment(fq)))
            })
            .collect::<Result<_, D::Error>>()?;

        let leaves: BTreeMap<u64, IndexedLeaf> = helper
            .leaves
            .into_iter()
            .map(|(k, h)| {
                let value = Fq::from_bytes_checked(&h.value)
                    .map_err(|_| serde::de::Error::custom("invalid leaf value Fq bytes"))?;
                let next_value = Fq::from_bytes_checked(&h.next_value)
                    .map_err(|_| serde::de::Error::custom("invalid leaf next_value Fq bytes"))?;
                let dk_pub = decaf377::Encoding(h.dk_pub)
                    .vartime_decompress()
                    .map_err(|_| serde::de::Error::custom("invalid dk_pub encoding"))?;
                let route_policy_hash = Fq::from_bytes_checked(&h.route_policy_hash)
                    .map_err(|_| serde::de::Error::custom("invalid route_policy_hash Fq bytes"))?;
                let ring_pk = decaf377::Encoding(h.ring_pk)
                    .vartime_decompress()
                    .map_err(|_| serde::de::Error::custom("invalid ring_pk encoding"))?;
                let ring_id_hash = Fq::from_bytes_checked(&h.ring_id_hash)
                    .map_err(|_| serde::de::Error::custom("invalid ring_id_hash Fq bytes"))?;
                let policy_id_hash = Fq::from_bytes_checked(&h.policy_id_hash)
                    .map_err(|_| serde::de::Error::custom("invalid policy_id_hash Fq bytes"))?;
                let permission_hash = Fq::from_bytes_checked(&h.permission_hash)
                    .map_err(|_| serde::de::Error::custom("invalid permission_hash Fq bytes"))?;
                let resource_hash = Fq::from_bytes_checked(&h.resource_hash)
                    .map_err(|_| serde::de::Error::custom("invalid resource_hash Fq bytes"))?;
                Ok((
                    k,
                    IndexedLeaf {
                        value,
                        next_index: h.next_index,
                        next_value,
                        params: LeafParams {
                            dk_pub,
                            threshold: h.threshold,
                            slot_count: h.slot_count,
                            route_policy_hash,
                        },
                        ring: LeafRing {
                            ring_pk,
                            ring_id_hash,
                            policy_id_hash,
                            permission_hash,
                            resource_hash,
                        },
                    },
                ))
            })
            .collect::<Result<_, D::Error>>()?;

        let value_index: BTreeMap<[u8; 32], u64> = helper.value_index.into_iter().collect();
        let predecessor_index = Self::build_predecessor_index(&leaves);

        Ok(IndexedMerkleTree {
            depth: helper.depth,
            nodes,
            leaves,
            value_index,
            predecessor_index,
            leaf_count: helper.leaf_count,
        })
    }
}

impl IndexedMerkleTree {
    /// Create a new IMT with the low sentinel at position 0.
    pub fn new() -> Self {
        let mut tree = Self {
            depth: DEFAULT_DEPTH,
            nodes: BTreeMap::new(),
            leaves: BTreeMap::new(),
            value_index: BTreeMap::new(),
            predecessor_index: BTreeMap::new(),
            leaf_count: 0,
        };

        let sentinel = IndexedLeaf::with_default_policy(Fq::from(0u64), 0, *FQ_MAX);

        let commitment = sentinel.commit();
        tree.predecessor_index
            .insert(FqOrdKey::from(sentinel.value), 0);
        tree.leaves.insert(0, sentinel);
        // Sentinel (value=0) is not added to value_index, matching load_leaf() which skips value==0.
        tree.leaf_count = 1;
        tree.update_path(0, commitment);

        tree
    }

    /// Create a new IMT with a custom depth.
    pub fn with_depth(depth: u8) -> Self {
        assert!(
            depth <= DEFAULT_DEPTH,
            "depth {} exceeds maximum of {}",
            depth,
            DEFAULT_DEPTH
        );
        let mut tree = Self {
            depth,
            nodes: BTreeMap::new(),
            leaves: BTreeMap::new(),
            value_index: BTreeMap::new(),
            predecessor_index: BTreeMap::new(),
            leaf_count: 0,
        };

        let sentinel = IndexedLeaf::with_default_policy(Fq::from(0u64), 0, *FQ_MAX);

        let commitment = sentinel.commit();
        tree.predecessor_index
            .insert(FqOrdKey::from(sentinel.value), 0);
        tree.leaves.insert(0, sentinel);
        // Sentinel (value=0) is not added to value_index, matching load_leaf() which skips value==0.
        tree.leaf_count = 1;
        tree.update_path(0, commitment);

        tree
    }

    /// Reconstruct an IMT from sparse stored nodes and leaves.
    pub fn from_sparse_parts(
        depth: u8,
        nodes: BTreeMap<u64, StateCommitment>,
        leaves: BTreeMap<u64, IndexedLeaf>,
        leaf_count: u64,
    ) -> Self {
        assert!(
            depth <= DEFAULT_DEPTH,
            "depth {} exceeds maximum of {}",
            depth,
            DEFAULT_DEPTH
        );
        let value_index = leaves
            .iter()
            .filter_map(|(&position, leaf)| {
                (leaf.value != Fq::from(0u64)).then_some((leaf.value.to_bytes(), position))
            })
            .collect();
        let predecessor_index = Self::build_predecessor_index(&leaves);
        Self {
            depth,
            nodes,
            leaves,
            value_index,
            predecessor_index,
            leaf_count,
        }
    }

    /// Return the packed storage key for a node.
    pub fn packed_node_key(level: u8, position: u64) -> u64 {
        Self::node_key(level, position)
    }

    /// Return node entries along the leaf-to-root path for a position.
    pub fn nodes_on_path(&self, position: u64) -> Result<Vec<(u8, u64, StateCommitment)>> {
        let max_leaves = Self::max_leaves_for_depth(self.depth);
        if position >= max_leaves {
            bail!(
                "Position {} exceeds maximum leaves {} for depth {}",
                position,
                max_leaves,
                self.depth
            );
        }

        let mut entries = Vec::with_capacity(self.depth as usize + 1);
        let mut current_position = position;
        for level in 0..=self.depth {
            entries.push((
                level,
                current_position,
                self.get_node(level, current_position),
            ));
            current_position /= 4;
        }
        Ok(entries)
    }

    /// Iterate over explicitly stored non-zero nodes.
    pub fn stored_nodes(&self) -> impl Iterator<Item = (u8, u64, StateCommitment)> + '_ {
        self.nodes.iter().map(|(&key, &hash)| {
            let level = (key >> 48) as u8;
            let position = key & ((1u64 << 48) - 1);
            (level, position, hash)
        })
    }

    /// Iterate over stored leaves.
    pub fn stored_leaves(&self) -> impl Iterator<Item = (u64, &IndexedLeaf)> + '_ {
        self.leaves.iter().map(|(&position, leaf)| (position, leaf))
    }

    #[inline]
    fn node_key(level: u8, position: u64) -> u64 {
        ((level as u64) << 48) | position
    }

    #[inline]
    fn max_leaves_for_depth(depth: u8) -> u64 {
        debug_assert!(depth <= 31, "depth must be <= 31 to avoid shift overflow");
        1u64 << ((depth as u32) * 2)
    }

    fn build_predecessor_index(leaves: &BTreeMap<u64, IndexedLeaf>) -> BTreeMap<FqOrdKey, u64> {
        leaves
            .iter()
            .map(|(&position, leaf)| (FqOrdKey::from(leaf.value), position))
            .collect()
    }

    fn get_node(&self, level: u8, position: u64) -> StateCommitment {
        let key = Self::node_key(level, position);
        self.nodes
            .get(&key)
            .copied()
            .unwrap_or_else(|| IMT_ZERO_HASHES[level as usize])
    }

    fn set_node(&mut self, level: u8, position: u64, hash: StateCommitment) {
        let key = Self::node_key(level, position);
        if hash.0 != IMT_ZERO_HASHES[level as usize].0 {
            self.nodes.insert(key, hash);
        } else {
            self.nodes.remove(&key);
        }
    }

    fn hash_children(
        child0: StateCommitment,
        child1: StateCommitment,
        child2: StateCommitment,
        child3: StateCommitment,
    ) -> StateCommitment {
        let hash = poseidon377::hash_4(&Fq::from(0u64), (child0.0, child1.0, child2.0, child3.0));
        StateCommitment(hash)
    }

    fn update_path(&mut self, position: u64, leaf_hash: StateCommitment) {
        self.set_node(0, position, leaf_hash);

        let mut current_position = position;
        for level in 0..self.depth {
            let parent_position = current_position / 4;
            let base_position = parent_position * 4;

            let child0 = self.get_node(level, base_position);
            let child1 = self.get_node(level, base_position + 1);
            let child2 = self.get_node(level, base_position + 2);
            let child3 = self.get_node(level, base_position + 3);

            let parent_hash = Self::hash_children(child0, child1, child2, child3);
            self.set_node(level + 1, parent_position, parent_hash);

            current_position = parent_position;
        }
    }

    /// Find the "low leaf" for a given value.
    pub fn find_low_leaf(&self, target: Fq) -> Option<(u64, IndexedLeaf)> {
        if let Some(&pos) = self.value_index.get(&target.to_bytes()) {
            let leaf = self.leaves.get(&pos)?;
            return Some((pos, leaf.clone()));
        }

        let target_key = FqOrdKey::from(target);
        if let Some((_, &pos)) = self.predecessor_index.range(..target_key).next_back() {
            let leaf = self.leaves.get(&pos)?;
            let next_key = FqOrdKey::from(leaf.next_value);
            if target_key < next_key {
                return Some((pos, leaf.clone()));
            }
        }

        None
    }

    /// Check if a value exists in the tree.
    pub fn contains(&self, value: Fq) -> bool {
        self.value_index.contains_key(&value.to_bytes())
    }

    /// Get the position of a value in the tree if it exists.
    pub fn get_position(&self, value: Fq) -> Option<u64> {
        self.value_index.get(&value.to_bytes()).copied()
    }

    /// Get the leaf at a given position.
    pub fn get_leaf(&self, position: u64) -> Option<&IndexedLeaf> {
        self.leaves.get(&position)
    }

    /// Update the policy data for an existing value and recompute its Merkle path.
    pub fn update_policy(&mut self, value: Fq, policy: &AssetPolicy) -> Result<IndexedLeaf> {
        let position = *self.value_index.get(&value.to_bytes()).ok_or_else(|| {
            anyhow::anyhow!(
                "IMT policy update failed: value {:?} not found",
                value.to_bytes()
            )
        })?;
        let current_leaf = self.leaves.get(&position).ok_or_else(|| {
            anyhow::anyhow!(
                "IMT internal error: leaf not found at position {}",
                position
            )
        })?;
        let updated_leaf = IndexedLeaf::from_policy(
            value,
            current_leaf.next_index,
            current_leaf.next_value,
            policy,
        );
        self.leaves.insert(position, updated_leaf.clone());
        self.update_path(position, updated_leaf.commit());
        Ok(updated_leaf)
    }

    /// Insert a new value into the IMT with the given policy.
    pub fn insert(&mut self, value: Fq, policy: &AssetPolicy) -> Result<InsertResult> {
        if self.contains(value) {
            bail!(
                "IMT insert failed: value {:?} already exists at position {:?}",
                value.to_bytes(),
                self.get_position(value)
            );
        }

        if value == Fq::from(0u64) {
            bail!("IMT insert failed: zero value is reserved for sentinel leaf");
        }

        let (low_pos, low_leaf) = self.find_low_leaf(value).ok_or_else(|| {
            anyhow::anyhow!(
                "IMT insert failed: could not find low leaf for value {:?} (tree has {} leaves)",
                value.to_bytes(),
                self.leaf_count
            )
        })?;

        if low_leaf.value == value {
            bail!(
                "IMT insert failed: value {:?} already exists (exact match at position {})",
                value.to_bytes(),
                low_pos
            );
        }

        let max_leaves = Self::max_leaves_for_depth(self.depth);
        if self.leaf_count >= max_leaves {
            bail!(
                "IMT insert failed: tree is full ({}/{} leaves, depth {})",
                self.leaf_count,
                max_leaves,
                self.depth
            );
        }

        let new_pos = self.leaf_count;

        // New leaf gets the provided policy
        let new_leaf =
            IndexedLeaf::from_policy(value, low_leaf.next_index, low_leaf.next_value, policy);

        // Low leaf keeps its own policy, only structural fields update
        let updated_low_leaf = IndexedLeaf {
            value: low_leaf.value,
            next_index: new_pos,
            next_value: value,
            params: low_leaf.params.clone(),
            ring: low_leaf.ring.clone(),
        };

        let new_leaf_commitment = new_leaf.commit();
        let updated_low_commitment = updated_low_leaf.commit();

        let result_new_leaf = new_leaf.clone();
        let result_low_leaf = updated_low_leaf.clone();

        self.leaves.insert(new_pos, new_leaf);
        self.value_index.insert(value.to_bytes(), new_pos);
        self.predecessor_index
            .insert(FqOrdKey::from(value), new_pos);
        self.leaf_count += 1;
        self.leaves.insert(low_pos, updated_low_leaf);

        self.update_path(new_pos, new_leaf_commitment);
        self.update_path(low_pos, updated_low_commitment);

        Ok(InsertResult {
            position: new_pos,
            indexed_leaf: result_new_leaf,
            low_leaf_position: low_pos,
            updated_low_leaf: result_low_leaf,
        })
    }

    /// Sync a leaf from an event (for client sync from CompactBlock).
    pub fn sync_from_event(
        &mut self,
        new_leaf: IndexedLeaf,
        new_position: u64,
        updated_low_leaf: IndexedLeaf,
        low_leaf_position: u64,
    ) -> Result<()> {
        let value = new_leaf.value;

        if self.contains(value) {
            let position = self.get_position(value).ok_or_else(|| {
                anyhow::anyhow!("IMT sync failed: value index missing for existing value")
            })?;
            self.leaves.insert(position, new_leaf.clone());
            self.update_path(position, new_leaf.commit());
            return Ok(());
        }

        if value == Fq::from(0u64) {
            bail!("IMT sync failed: zero value is reserved for sentinel leaf");
        }

        let new_leaf_commitment = new_leaf.commit();
        let updated_low_commitment = updated_low_leaf.commit();

        self.leaves.insert(new_position, new_leaf);
        self.value_index.insert(value.to_bytes(), new_position);
        self.predecessor_index
            .insert(FqOrdKey::from(value), new_position);

        if new_position >= self.leaf_count {
            self.leaf_count = new_position + 1;
        }

        self.leaves.insert(low_leaf_position, updated_low_leaf);

        self.update_path(new_position, new_leaf_commitment);
        self.update_path(low_leaf_position, updated_low_commitment);

        Ok(())
    }

    /// Load a leaf directly at a position (for tree reconstruction from storage).
    pub fn load_leaf(&mut self, position: u64, leaf: IndexedLeaf) {
        let value = leaf.value;
        self.leaves.insert(position, leaf);
        self.predecessor_index
            .insert(FqOrdKey::from(value), position);
        if value != Fq::from(0u64) {
            self.value_index.insert(value.to_bytes(), position);
        }
        if position >= self.leaf_count {
            self.leaf_count = position + 1;
        }
    }

    /// Rebuild all internal hashes from leaf data.
    pub fn rebuild_hashes(&mut self) {
        for position in 0..self.leaf_count {
            if let Some(leaf) = self.leaves.get(&position) {
                let commitment = leaf.commit();
                self.update_path(position, commitment);
            }
        }
    }

    /// Get the authentication path for a position.
    pub fn auth_path(&self, position: u64) -> Result<Vec<[StateCommitment; 3]>> {
        let max_leaves = Self::max_leaves_for_depth(self.depth);
        if position >= max_leaves {
            bail!(
                "Position {} exceeds maximum leaves {} for depth {}",
                position,
                max_leaves,
                self.depth
            );
        }

        let mut path = Vec::with_capacity(self.depth as usize);
        let mut current_position = position;

        for level in 0..self.depth {
            let child_index = (current_position % 4) as usize;
            let base_position = (current_position / 4) * 4;

            let children = [
                self.get_node(level, base_position),
                self.get_node(level, base_position + 1),
                self.get_node(level, base_position + 2),
                self.get_node(level, base_position + 3),
            ];

            let siblings = match child_index {
                0 => [children[1], children[2], children[3]],
                1 => [children[0], children[2], children[3]],
                2 => [children[0], children[1], children[3]],
                3 => [children[0], children[1], children[2]],
                _ => unreachable!(),
            };

            path.push(siblings);
            current_position /= 4;
        }

        Ok(path)
    }

    /// Get a membership proof for a value that exists in the tree.
    pub fn membership_proof(
        &self,
        value: Fq,
    ) -> Result<(u64, IndexedLeaf, Vec<[StateCommitment; 3]>)> {
        let position = self.value_index.get(&value.to_bytes()).ok_or_else(|| {
            anyhow::anyhow!(
                "IMT membership proof failed: value {:?} not found in tree (tree has {} leaves)",
                value.to_bytes(),
                self.leaf_count
            )
        })?;

        let leaf = self
            .leaves
            .get(position)
            .ok_or_else(|| {
                anyhow::anyhow!(
                "IMT internal error: leaf not found at position {} (index exists but leaf missing)",
                position
            )
            })?
            .clone();

        let path = self.auth_path(*position)?;

        Ok((*position, leaf, path))
    }

    /// Get a non-membership proof for a value that does NOT exist in the tree.
    pub fn non_membership_proof(
        &self,
        value: Fq,
    ) -> Result<(u64, IndexedLeaf, Vec<[StateCommitment; 3]>)> {
        if self.contains(value) {
            bail!(
                "IMT non-membership proof failed: value {:?} exists in tree at position {:?}",
                value.to_bytes(),
                self.get_position(value)
            );
        }

        let (low_pos, low_leaf) = self.find_low_leaf(value).ok_or_else(|| {
            anyhow::anyhow!(
                "IMT non-membership proof failed: could not find gap for value {:?} (tree has {} leaves)",
                value.to_bytes(),
                self.leaf_count
            )
        })?;

        let value_key = FqOrdKey::from(value);
        if FqOrdKey::from(low_leaf.value) >= value_key
            || value_key >= FqOrdKey::from(low_leaf.next_value)
        {
            bail!(
                "IMT non-membership proof failed: value {:?} not in gap [{:?}, {:?})",
                value.to_bytes(),
                low_leaf.value.to_bytes(),
                low_leaf.next_value.to_bytes()
            );
        }

        let path = self.auth_path(low_pos)?;

        Ok((low_pos, low_leaf, path))
    }

    /// Get the root hash of the tree.
    pub fn root(&self) -> StateCommitment {
        self.get_node(self.depth, 0)
    }

    pub fn depth(&self) -> u8 {
        self.depth
    }

    pub fn leaf_count(&self) -> u64 {
        self.leaf_count
    }

    /// Verify an authentication path.
    pub fn verify_auth_path(
        position: u64,
        leaf: &IndexedLeaf,
        auth_path: &[[StateCommitment; 3]],
        expected_root: StateCommitment,
        depth: u8,
    ) -> bool {
        let mut current_hash = leaf.commit();
        let mut current_position = position;

        for siblings in auth_path.iter().take(depth as usize) {
            let child_index = (current_position % 4) as usize;

            let children = match child_index {
                0 => [current_hash, siblings[0], siblings[1], siblings[2]],
                1 => [siblings[0], current_hash, siblings[1], siblings[2]],
                2 => [siblings[0], siblings[1], current_hash, siblings[2]],
                3 => [siblings[0], siblings[1], siblings[2], current_hash],
                _ => unreachable!(),
            };

            current_hash = Self::hash_children(children[0], children[1], children[2], children[3]);
            current_position /= 4;
        }

        current_hash.0 == expected_root.0
    }
}

impl Default for IndexedMerkleTree {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute the IMT root from a leaf commitment and authentication path.
pub fn recompute_root(
    leaf_commitment: StateCommitment,
    path: &crate::structs::MerklePath,
    position: u64,
) -> StateCommitment {
    use crate::tree::DEFAULT_DEPTH;

    let mut current_hash = leaf_commitment;
    let mut current_position = position;

    for layer in path.layers.iter().take(DEFAULT_DEPTH as usize) {
        let child_index = (current_position % 4) as usize;

        if layer.siblings.len() != 3 {
            tracing::error!(
                "Invalid path layer: expected 3 siblings, got {}",
                layer.siblings.len()
            );
            return StateCommitment(Fq::from(0u64));
        }

        let siblings: [StateCommitment; 3] = [
            StateCommitment(Fq::from_le_bytes_mod_order(&layer.siblings[0])),
            StateCommitment(Fq::from_le_bytes_mod_order(&layer.siblings[1])),
            StateCommitment(Fq::from_le_bytes_mod_order(&layer.siblings[2])),
        ];

        let children = match child_index {
            0 => [current_hash, siblings[0], siblings[1], siblings[2]],
            1 => [siblings[0], current_hash, siblings[1], siblings[2]],
            2 => [siblings[0], siblings[1], current_hash, siblings[2]],
            3 => [siblings[0], siblings[1], siblings[2], current_hash],
            _ => unreachable!(),
        };

        current_hash =
            IndexedMerkleTree::hash_children(children[0], children[1], children[2], children[3]);
        current_position /= 4;
    }

    current_hash
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a test policy.
    fn test_policy() -> AssetPolicy {
        AssetPolicy::default_unregulated()
    }

    #[test]
    fn test_imt_new_has_sentinel() {
        let tree = IndexedMerkleTree::new();
        assert_eq!(tree.leaf_count(), 1);

        let sentinel = tree.get_leaf(0).expect("Sentinel should exist");
        assert_eq!(sentinel.value, Fq::from(0u64));
        assert_eq!(sentinel.next_index, 0);
        assert_eq!(sentinel.next_value, *FQ_MAX);
    }

    #[test]
    fn test_imt_insert_single() {
        let mut tree = IndexedMerkleTree::new();
        let value = Fq::from(100u64);

        let result = tree.insert(value, &test_policy()).unwrap();
        assert_eq!(result.position, 1);
        assert_eq!(tree.leaf_count(), 2);
        assert!(tree.contains(value));

        let leaf = tree.get_leaf(1).unwrap();
        assert_eq!(leaf.value, value);
        assert_eq!(leaf.next_index, 0);
        assert_eq!(leaf.next_value, *FQ_MAX);

        let sentinel = tree.get_leaf(0).unwrap();
        assert_eq!(sentinel.next_index, 1);
        assert_eq!(sentinel.next_value, value);
    }

    #[test]
    fn test_imt_insert_multiple_maintains_order() {
        let mut tree = IndexedMerkleTree::with_depth(4);
        let policy = test_policy();

        let values = [
            Fq::from(500u64),
            Fq::from(100u64),
            Fq::from(300u64),
            Fq::from(200u64),
        ];

        for v in values {
            tree.insert(v, &policy).unwrap();
        }

        let mut collected = Vec::new();
        let mut pos = 0u64;
        loop {
            let leaf = tree.get_leaf(pos).unwrap();
            if leaf.next_value == *FQ_MAX {
                break;
            }
            collected.push(leaf.next_value);
            pos = leaf.next_index;
        }

        let expected = vec![
            Fq::from(100u64),
            Fq::from(200u64),
            Fq::from(300u64),
            Fq::from(500u64),
        ];
        assert_eq!(collected, expected);
    }

    #[test]
    fn test_imt_membership_proof() {
        let mut tree = IndexedMerkleTree::new();
        let value = Fq::from(42u64);
        tree.insert(value, &test_policy()).unwrap();

        let (pos, leaf, path) = tree.membership_proof(value).unwrap();
        assert_eq!(pos, 1);
        assert_eq!(leaf.value, value);
        assert_eq!(path.len(), DEFAULT_DEPTH as usize);

        let root = tree.root();
        assert!(IndexedMerkleTree::verify_auth_path(
            pos,
            &leaf,
            &path,
            root,
            DEFAULT_DEPTH
        ));
    }

    #[test]
    fn test_imt_non_membership_proof() {
        let mut tree = IndexedMerkleTree::new();
        let policy = test_policy();
        tree.insert(Fq::from(100u64), &policy).unwrap();
        tree.insert(Fq::from(300u64), &policy).unwrap();

        let value = Fq::from(200u64);
        let (pos, leaf, path) = tree.non_membership_proof(value).unwrap();

        assert_eq!(leaf.value, Fq::from(100u64));
        assert!(leaf.value < value);
        assert!(value < leaf.next_value);

        let root = tree.root();
        assert!(IndexedMerkleTree::verify_auth_path(
            pos,
            &leaf,
            &path,
            root,
            DEFAULT_DEPTH
        ));
    }

    #[test]
    fn test_imt_non_membership_empty_tree() {
        let tree = IndexedMerkleTree::new();
        let value = Fq::from(12345u64);
        let (pos, leaf, path) = tree.non_membership_proof(value).unwrap();

        assert_eq!(pos, 0);
        assert_eq!(leaf.value, Fq::from(0u64));
        assert!(leaf.value < value);
        assert!(value < leaf.next_value);

        let root = tree.root();
        assert!(IndexedMerkleTree::verify_auth_path(
            pos,
            &leaf,
            &path,
            root,
            DEFAULT_DEPTH
        ));
    }

    #[test]
    fn test_imt_cannot_insert_duplicate() {
        let mut tree = IndexedMerkleTree::new();
        let value = Fq::from(100u64);

        tree.insert(value, &test_policy()).unwrap();
        let result = tree.insert(value, &test_policy());
        assert!(result.is_err());
    }

    #[test]
    fn test_imt_cannot_insert_zero() {
        let mut tree = IndexedMerkleTree::new();
        let result = tree.insert(Fq::from(0u64), &test_policy());
        assert!(result.is_err());
    }

    #[test]
    fn test_imt_serialization_json() {
        let mut tree = IndexedMerkleTree::new();
        let policy = test_policy();
        tree.insert(Fq::from(100u64), &policy).unwrap();
        tree.insert(Fq::from(200u64), &policy).unwrap();

        let serialized = serde_json::to_string(&tree).expect("serialization failed");
        let deserialized: IndexedMerkleTree =
            serde_json::from_str(&serialized).expect("deserialization failed");

        assert_eq!(tree.root().0, deserialized.root().0);
        assert_eq!(tree.leaf_count(), deserialized.leaf_count());
        assert!(deserialized.contains(Fq::from(100u64)));
        assert!(deserialized.contains(Fq::from(200u64)));
    }

    #[test]
    fn test_imt_serialization_bincode() {
        let mut tree = IndexedMerkleTree::new();
        let policy = test_policy();
        tree.insert(Fq::from(100u64), &policy).unwrap();
        tree.insert(Fq::from(200u64), &policy).unwrap();

        let serialized = bincode::serialize(&tree).expect("bincode serialization failed");
        let deserialized: IndexedMerkleTree =
            bincode::deserialize(&serialized).expect("bincode deserialization failed");

        assert_eq!(tree.root().0, deserialized.root().0);
        assert_eq!(tree.leaf_count(), deserialized.leaf_count());
        assert!(deserialized.contains(Fq::from(100u64)));
        assert!(deserialized.contains(Fq::from(200u64)));

        let (pos1, leaf1, path1) = tree.non_membership_proof(Fq::from(999u64)).unwrap();
        let (pos2, leaf2, path2) = deserialized.non_membership_proof(Fq::from(999u64)).unwrap();
        assert_eq!(pos1, pos2);
        assert_eq!(leaf1.value, leaf2.value);
        assert_eq!(leaf1.next_index, leaf2.next_index);
        assert_eq!(leaf1.next_value, leaf2.next_value);
        assert_eq!(path1.len(), path2.len());

        assert!(IndexedMerkleTree::verify_auth_path(
            pos2,
            &leaf2,
            &path2,
            deserialized.root(),
            DEFAULT_DEPTH
        ));
    }

    #[test]
    fn test_imt_root_changes_on_insert() {
        let mut tree = IndexedMerkleTree::new();
        let policy = test_policy();
        let root1 = tree.root();

        tree.insert(Fq::from(100u64), &policy).unwrap();
        let root2 = tree.root();

        tree.insert(Fq::from(200u64), &policy).unwrap();
        let root3 = tree.root();

        assert_ne!(root1.0, root2.0);
        assert_ne!(root2.0, root3.0);
        assert_ne!(root1.0, root3.0);
    }

    #[test]
    fn test_imt_membership_fails_for_missing() {
        let tree = IndexedMerkleTree::new();
        let result = tree.membership_proof(Fq::from(100u64));
        assert!(result.is_err());
    }

    #[test]
    fn test_imt_non_membership_fails_for_existing() {
        let mut tree = IndexedMerkleTree::new();
        tree.insert(Fq::from(100u64), &test_policy()).unwrap();

        let result = tree.non_membership_proof(Fq::from(100u64));
        assert!(result.is_err());
    }

    #[test]
    fn test_imt_with_custom_depth() {
        let mut tree = IndexedMerkleTree::with_depth(4);
        assert_eq!(tree.depth(), 4);

        tree.insert(Fq::from(100u64), &test_policy()).unwrap();
        let (_, leaf, path) = tree.membership_proof(Fq::from(100u64)).unwrap();
        assert_eq!(path.len(), 4);

        let root = tree.root();
        assert!(IndexedMerkleTree::verify_auth_path(
            1, &leaf, &path, root, 4
        ));
    }

    #[test]
    fn test_imt_find_low_leaf_edge_cases() {
        let mut tree = IndexedMerkleTree::new();
        let policy = test_policy();
        tree.insert(Fq::from(100u64), &policy).unwrap();
        tree.insert(Fq::from(200u64), &policy).unwrap();

        let (pos, leaf) = tree.find_low_leaf(Fq::from(50u64)).unwrap();
        assert_eq!(pos, 0);
        assert_eq!(leaf.value, Fq::from(0u64));

        let (_, leaf) = tree.find_low_leaf(Fq::from(150u64)).unwrap();
        assert_eq!(leaf.value, Fq::from(100u64));

        let (_, leaf) = tree.find_low_leaf(Fq::from(100u64)).unwrap();
        assert_eq!(leaf.value, Fq::from(100u64));
    }

    #[test]
    fn test_fq_ord_key_numeric_order() {
        let ordered = [
            Fq::from(0u64),
            Fq::from(1u64),
            Fq::from(2u64),
            Fq::from(10u64),
            Fq::from(u64::MAX),
            *FQ_MAX,
        ];

        for window in ordered.windows(2) {
            assert!(FqOrdKey::from(window[0]) < FqOrdKey::from(window[1]));
        }
    }

    #[test]
    fn test_imt_predecessor_index_matches_linear_scan() {
        let mut tree = IndexedMerkleTree::new();
        let policy = test_policy();
        for value in [400u64, 100, 900, 250, 700] {
            tree.insert(Fq::from(value), &policy).unwrap();
        }

        let linear_low_leaf = |target: Fq| -> Option<(u64, IndexedLeaf)> {
            if let Some(&pos) = tree.value_index.get(&target.to_bytes()) {
                return tree.leaves.get(&pos).cloned().map(|leaf| (pos, leaf));
            }
            let target_key = FqOrdKey::from(target);
            tree.leaves
                .iter()
                .find(|(_, leaf)| {
                    FqOrdKey::from(leaf.value) < target_key
                        && target_key < FqOrdKey::from(leaf.next_value)
                })
                .map(|(&pos, leaf)| (pos, leaf.clone()))
        };

        for target in [
            Fq::from(1u64),
            Fq::from(100u64),
            Fq::from(101u64),
            Fq::from(399u64),
            Fq::from(700u64),
            Fq::from(800u64),
            *FQ_MAX - Fq::from(1u64),
        ] {
            let indexed = tree.find_low_leaf(target).unwrap();
            let linear = linear_low_leaf(target).unwrap();
            assert_eq!(indexed.0, linear.0);
            assert_eq!(indexed.1.value, linear.1.value);
            assert_eq!(indexed.1.next_value, linear.1.next_value);
        }

        let mut changed_policy = test_policy();
        changed_policy.params.threshold = 42;
        tree.update_policy(Fq::from(400u64), &changed_policy)
            .unwrap();
        let (_, leaf) = tree.find_low_leaf(Fq::from(450u64)).unwrap();
        assert_eq!(leaf.value, Fq::from(400u64));
        assert_eq!(leaf.threshold(), 42);
    }

    #[test]
    fn test_fq_max_is_field_modulus_minus_one() {
        let fq_max = *FQ_MAX;
        let fq_max_plus_one = fq_max + Fq::from(1u64);
        assert_eq!(fq_max_plus_one, Fq::from(0u64));
        assert!(fq_less_than(&Fq::from(0u64), &fq_max));
        assert!(fq_less_than(&Fq::from(u64::MAX), &fq_max));
    }

    #[test]
    fn test_non_membership_near_fq_max() {
        let tree = IndexedMerkleTree::new();
        let near_max = *FQ_MAX - Fq::from(1u64);
        let (pos, leaf, path) = tree.non_membership_proof(near_max).unwrap();

        assert_eq!(pos, 0);
        assert_eq!(leaf.value, Fq::from(0u64));
        assert_eq!(leaf.next_value, *FQ_MAX);
        assert!(fq_less_than(&leaf.value, &near_max));
        assert!(fq_less_than(&near_max, &leaf.next_value));

        let root = tree.root();
        assert!(IndexedMerkleTree::verify_auth_path(
            pos,
            &leaf,
            &path,
            root,
            DEFAULT_DEPTH
        ));
    }

    #[test]
    fn test_fq_max_no_non_membership_proof() {
        let tree = IndexedMerkleTree::new();
        let result = tree.non_membership_proof(*FQ_MAX);
        assert!(result.is_err());
    }

    #[test]
    fn test_sentinel_covers_full_range() {
        let tree = IndexedMerkleTree::new();
        let sentinel = tree.get_leaf(0).unwrap();

        assert_eq!(sentinel.value, Fq::from(0u64));
        assert_eq!(sentinel.next_value, *FQ_MAX);

        for v in [1u64, 1000, u64::MAX / 2, u64::MAX] {
            let value = Fq::from(v);
            let result = tree.non_membership_proof(value);
            assert!(result.is_ok(), "Should have proof for value {}", v);
        }
    }

    #[test]
    fn test_indexed_leaf_proto_roundtrip() {
        let leaf = IndexedLeaf::with_default_policy(Fq::from(100u64), 2, Fq::from(200u64));

        let proto: pb::IndexedLeafData = leaf.clone().into();
        let back = IndexedLeaf::try_from(proto).unwrap();

        assert_eq!(back.value, leaf.value);
        assert_eq!(back.next_index, leaf.next_index);
        assert_eq!(back.next_value, leaf.next_value);
        assert_eq!(back.params.dk_pub, leaf.params.dk_pub);
        assert_eq!(back.params.threshold, leaf.params.threshold);
        assert_eq!(back.params.route_policy_hash, leaf.params.route_policy_hash);
        assert_eq!(back.ring.ring_pk, leaf.ring.ring_pk);
        assert_eq!(back.ring.ring_id_hash, leaf.ring.ring_id_hash);
    }

    #[test]
    fn test_indexed_leaf_bincode_roundtrip() {
        let leaf = IndexedLeaf::with_default_policy(Fq::from(100u64), 2, Fq::from(200u64));

        let bytes = bincode::serialize(&leaf).unwrap();
        let back: IndexedLeaf = bincode::deserialize(&bytes).unwrap();

        assert_eq!(back.value, leaf.value);
        assert_eq!(back.next_index, leaf.next_index);
        assert_eq!(back.next_value, leaf.next_value);
        assert_eq!(back.params, leaf.params);
        assert_eq!(back.ring, leaf.ring);
    }

    #[test]
    fn test_insert_result_contains_correct_data() {
        let mut tree = IndexedMerkleTree::with_depth(4);
        let policy = test_policy();

        let result = tree.insert(Fq::from(100u64), &policy).unwrap();

        assert_eq!(result.indexed_leaf.value, Fq::from(100u64));
        assert_eq!(result.updated_low_leaf.value, Fq::from(0u64));
        assert_eq!(result.updated_low_leaf.next_index, 1);
        assert_eq!(result.updated_low_leaf.next_value, Fq::from(100u64));

        let stored_new_leaf = tree.get_leaf(result.position).unwrap();
        assert_eq!(*stored_new_leaf, result.indexed_leaf);

        let stored_low_leaf = tree.get_leaf(result.low_leaf_position).unwrap();
        assert_eq!(*stored_low_leaf, result.updated_low_leaf);
    }

    #[test]
    fn test_string_to_fq_deterministic() {
        let a = string_to_fq("hello");
        let b = string_to_fq("hello");
        assert_eq!(a, b);

        let c = string_to_fq("world");
        assert_ne!(a, c);
    }

    #[test]
    fn test_route_policy_to_fq_order_independent() {
        use crate::structs::IbcRoute;
        let mut a = AssetPolicy::default_unregulated().params;
        a.allowed_ibc_routes = vec![
            IbcRoute::transfer("channel-0", "connection-0", "channel-7"),
            IbcRoute::transfer("channel-1", "connection-1", "channel-8"),
        ];
        let mut b = a.clone();
        b.allowed_ibc_routes.reverse();
        assert_eq!(route_policy_to_fq(&a), route_policy_to_fq(&b));
    }

    #[test]
    fn test_leaf_commit_includes_policy() {
        let leaf1 = IndexedLeaf::with_default_policy(Fq::from(100u64), 0, *FQ_MAX);

        // Same structural values but different policy
        let leaf2 = IndexedLeaf {
            value: Fq::from(100u64),
            next_index: 0,
            next_value: *FQ_MAX,
            params: LeafParams {
                dk_pub: decaf377::Element::default(),
                threshold: 1000u128,
                slot_count: crate::structs::DEFAULT_COMPLIANCE_SLOT_COUNT,
                route_policy_hash: string_to_fq(""),
            },
            ring: LeafRing::default(),
        };

        assert_ne!(
            leaf1.commit().0,
            leaf2.commit().0,
            "Different policy should produce different commitment"
        );
    }

    #[test]
    fn test_leaf_from_policy() {
        let policy = AssetPolicy::default_unregulated();
        let leaf = IndexedLeaf::from_policy(Fq::from(42u64), 0, *FQ_MAX, &policy);

        assert_eq!(leaf.value, Fq::from(42u64));
        assert_eq!(leaf.params.dk_pub, *crate::crypto::UNREGULATED_SINK_DK_PUB);
        assert_eq!(leaf.params.threshold, u128::MAX);
        assert_eq!(leaf.ring.ring_pk, *crate::crypto::UNREGULATED_SINK_RING_PK);
    }
}
