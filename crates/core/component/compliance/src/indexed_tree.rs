use anyhow::{anyhow, bail, Result};
use ark_ff::{BigInteger, PrimeField};
use decaf377::Fq;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use shieldd_sdk_proto::{core::component::compliance::v1 as pb, DomainType};
use shieldd_sdk_tct::StateCommitment;
use std::collections::{BTreeMap, BTreeSet};

use crate::structs::{canonical_route_policy_string, AssetParams, AssetPolicy, RingData};
use crate::tree::DEFAULT_DEPTH;

/// Canonical numeric ordering key for `Fq`.
///
/// `into_bigint()` returns canonical non-Montgomery limbs. Serializing those
/// limbs big-endian makes lexicographic byte ordering equal numeric ordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FqOrdKey([u8; 32]);

impl FqOrdKey {
    pub fn to_bytes(self) -> [u8; 32] {
        self.0
    }

    pub fn descending_bytes(value: Fq) -> [u8; 32] {
        let mut bytes = Self::from(value).0;
        for byte in &mut bytes {
            *byte = !*byte;
        }
        bytes
    }
}

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

/// Domain separator for params sub-hash (Shieldd-decided: dk_pub, daily_volume_limit, IBC route policy).
pub static PARAMS_DOMAIN_SEP: Lazy<Fq> = Lazy::new(|| {
    let hash = blake2b_simd::Params::default()
        .personal(b"pen.imt.params2_")
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

/// Shieldd-decided policy fields bound into the IMT leaf.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeafParams {
    pub dk_pub: decaf377::Element,
    pub daily_volume_limit: u128,
    pub route_policy_hash: Fq,
}

impl LeafParams {
    /// Construct from an AssetParams (hashes the canonical route policy).
    pub fn from_asset_params(p: &AssetParams) -> Self {
        Self {
            dk_pub: p.dk_pub,
            daily_volume_limit: p.daily_volume_limit,
            route_policy_hash: route_policy_to_fq(p),
        }
    }
}

impl Default for LeafParams {
    fn default() -> Self {
        Self {
            dk_pub: *crate::crypto::UNREGULATED_SINK_DK_PUB,
            daily_volume_limit: u128::MAX,
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
            ring_pk: *crate::crypto::UNREGULATED_SINK_RING_PK,
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
    let daily_volume_limit_fq = Fq::from(p.daily_volume_limit);
    poseidon377::hash_3(
        &PARAMS_DOMAIN_SEP,
        (dk_pub_fq, daily_volume_limit_fq, p.route_policy_hash),
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
///   params_hash = hash_3(PARAMS_DOMAIN, dk_pub_fq, daily_volume_limit_fq, route_policy_hash)
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
    /// Shieldd-decided policy (dk_pub, daily_volume_limit, IBC routes).
    pub params: LeafParams,
    /// Orbis-decided policy (ring_pk, ring_id, policy_id, permission, resource).
    pub ring: LeafRing,
}

/// Canonical policy sub-commitments carried by compact circuit witnesses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IndexedLeafCommitments {
    pub params_hash: Fq,
    pub ring_hash: Fq,
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

    /// Compute the canonical policy sub-commitments for this leaf.
    pub fn commitment_parts(&self) -> IndexedLeafCommitments {
        let dk_pub_fq = self.params.dk_pub.vartime_compress_to_field();
        let daily_volume_limit_fq = Fq::from(self.params.daily_volume_limit);
        let params_hash = poseidon377::hash_3(
            &PARAMS_DOMAIN_SEP,
            (
                dk_pub_fq,
                daily_volume_limit_fq,
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

        IndexedLeafCommitments {
            params_hash,
            ring_hash,
        }
    }

    /// Compute the Poseidon commitment for this leaf (3 hashes).
    pub fn commit(&self) -> StateCommitment {
        let parts = self.commitment_parts();
        let hash = poseidon377::hash_5(
            &IMT_LEAF_DOMAIN_SEP,
            (
                self.value,
                Fq::from(self.next_index),
                self.next_value,
                parts.params_hash,
                parts.ring_hash,
            ),
        );
        StateCommitment(hash)
    }

    /// Convenience accessors for circuit-relevant policy fields.
    pub fn dk_pub(&self) -> &decaf377::Element {
        &self.params.dk_pub
    }
    pub fn daily_volume_limit(&self) -> u128 {
        self.params.daily_volume_limit
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
    daily_volume_limit: u128,
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
            daily_volume_limit: self.params.daily_volume_limit,
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
                daily_volume_limit: h.daily_volume_limit,
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
            daily_volume_limit: leaf.params.daily_volume_limit.to_le_bytes().to_vec(),
            route_policy_hash: leaf.params.route_policy_hash.to_bytes().to_vec(),
            ring_pk: leaf.ring.ring_pk.vartime_compress().0.to_vec(),
            ring_id_hash: leaf.ring.ring_id_hash.to_bytes().to_vec(),
            policy_id_hash: leaf.ring.policy_id_hash.to_bytes().to_vec(),
            permission_hash: leaf.ring.permission_hash.to_bytes().to_vec(),
            resource_hash: leaf.ring.resource_hash.to_bytes().to_vec(),
        }
    }
}

fn parse_fq(bytes: &[u8], field_name: &str) -> Result<Fq> {
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow!("{} must be 32 bytes, got {}", field_name, bytes.len()))?;
    Fq::from_bytes_checked(&arr).map_err(|_| anyhow!("invalid {} Fq bytes", field_name))
}

fn parse_element(bytes: &[u8], field_name: &str) -> Result<decaf377::Element> {
    let encoded: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow!("{} must be 32 bytes, got {}", field_name, bytes.len()))?;
    decaf377::Encoding(encoded)
        .vartime_decompress()
        .map_err(|_| anyhow!("invalid {field_name} encoding"))
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

        let dk_pub = parse_element(&proto.dk_pub, "dk_pub")?;
        let daily_volume_limit_bytes: [u8; 16] = proto
            .daily_volume_limit
            .as_slice()
            .try_into()
            .map_err(|_| {
                anyhow!(
                    "daily_volume_limit must be 16 bytes, got {}",
                    proto.daily_volume_limit.len()
                )
            })?;
        let daily_volume_limit = u128::from_le_bytes(daily_volume_limit_bytes);
        let route_policy_hash = parse_fq(&proto.route_policy_hash, "route_policy_hash")?;
        let ring_pk = parse_element(&proto.ring_pk, "ring_pk")?;
        let ring_id_hash = parse_fq(&proto.ring_id_hash, "ring_id_hash")?;
        let policy_id_hash = parse_fq(&proto.policy_id_hash, "policy_id_hash")?;
        let permission_hash = parse_fq(&proto.permission_hash, "permission_hash")?;
        let resource_hash = parse_fq(&proto.resource_hash, "resource_hash")?;

        Ok(IndexedLeaf {
            value: Fq::from_bytes_checked(&value_bytes)
                .map_err(|e| anyhow!("invalid value: {}", e))?,
            next_index: proto.next_index,
            next_value: Fq::from_bytes_checked(&next_value_bytes)
                .map_err(|e| anyhow!("invalid next_value: {}", e))?,
            params: LeafParams {
                dk_pub,
                daily_volume_limit,
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
                        daily_volume_limit: v.params.daily_volume_limit,
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

        let serialized_node_count = helper.nodes.len();
        let nodes: BTreeMap<u64, StateCommitment> = helper
            .nodes
            .into_iter()
            .map(|(k, bytes)| {
                let fq = Fq::from_bytes_checked(&bytes)
                    .map_err(|_| serde::de::Error::custom("invalid node Fq bytes"))?;
                Ok((k, StateCommitment(fq)))
            })
            .collect::<Result<_, D::Error>>()?;
        if nodes.len() != serialized_node_count {
            return Err(serde::de::Error::custom(
                "IndexedMerkleTree contains duplicate node keys",
            ));
        }

        let serialized_leaf_count = helper.leaves.len();
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
                            daily_volume_limit: h.daily_volume_limit,
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
        if leaves.len() != serialized_leaf_count {
            return Err(serde::de::Error::custom(
                "IndexedMerkleTree contains duplicate leaf positions",
            ));
        }

        let serialized_value_index_count = helper.value_index.len();
        let value_index: BTreeMap<[u8; 32], u64> = helper.value_index.into_iter().collect();
        if value_index.len() != serialized_value_index_count {
            return Err(serde::de::Error::custom(
                "IndexedMerkleTree contains duplicate value-index keys",
            ));
        }
        let predecessor_index = Self::build_predecessor_index(&leaves);

        let tree = IndexedMerkleTree {
            depth: helper.depth,
            nodes,
            leaves,
            value_index,
            predecessor_index,
            leaf_count: helper.leaf_count,
        };
        tree.validate_well_formed()
            .map_err(serde::de::Error::custom)?;
        Ok(tree)
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

    /// Reconstruct and validate an IMT from sparse stored nodes and leaves.
    pub fn try_from_sparse_parts(
        depth: u8,
        nodes: BTreeMap<u64, StateCommitment>,
        leaves: BTreeMap<u64, IndexedLeaf>,
        leaf_count: u64,
    ) -> Result<Self> {
        let value_index = leaves
            .iter()
            .filter_map(|(&position, leaf)| {
                (leaf.value != Fq::from(0u64)).then_some((leaf.value.to_bytes(), position))
            })
            .collect();
        let predecessor_index = Self::build_predecessor_index(&leaves);
        let tree = Self {
            depth,
            nodes,
            leaves,
            value_index,
            predecessor_index,
            leaf_count,
        };
        tree.validate_well_formed()?;
        Ok(tree)
    }

    /// Validate all structural, index, linked-list, and Merkle invariants.
    pub fn validate_well_formed(&self) -> Result<()> {
        anyhow::ensure!(
            self.depth <= DEFAULT_DEPTH,
            "IMT depth {} exceeds maximum {}",
            self.depth,
            DEFAULT_DEPTH
        );
        let capacity = Self::max_leaves_for_depth(self.depth);
        anyhow::ensure!(
            self.leaf_count > 0,
            "IMT leaf count must include the sentinel"
        );
        anyhow::ensure!(
            self.leaf_count <= capacity,
            "IMT leaf count {} exceeds capacity {} for depth {}",
            self.leaf_count,
            capacity,
            self.depth
        );
        anyhow::ensure!(
            self.leaves.len() as u64 == self.leaf_count,
            "IMT leaf map has {} entries but leaf count is {}",
            self.leaves.len(),
            self.leaf_count
        );
        for position in 0..self.leaf_count {
            anyhow::ensure!(
                self.leaves.contains_key(&position),
                "IMT leaf positions are not contiguous: missing position {position}"
            );
        }
        anyhow::ensure!(
            self.leaves
                .keys()
                .all(|position| *position < self.leaf_count),
            "IMT leaf position exceeds leaf count"
        );

        let sentinel = self
            .leaves
            .get(&0)
            .ok_or_else(|| anyhow!("IMT sentinel is missing"))?;
        anyhow::ensure!(
            sentinel.value == Fq::from(0u64),
            "IMT sentinel value must be zero"
        );
        anyhow::ensure!(
            sentinel.params == LeafParams::default() && sentinel.ring == LeafRing::default(),
            "IMT sentinel policy is not canonical"
        );

        let mut values = BTreeSet::new();
        for (&position, leaf) in &self.leaves {
            if position != 0 {
                anyhow::ensure!(
                    leaf.value != Fq::from(0u64),
                    "IMT non-sentinel leaf at position {position} has zero value"
                );
                anyhow::ensure!(
                    FqOrdKey::from(leaf.value) < FqOrdKey::from(*FQ_MAX),
                    "IMT leaf at position {position} uses reserved maximum value"
                );
                anyhow::ensure!(
                    values.insert(FqOrdKey::from(leaf.value)),
                    "IMT contains duplicate nonzero leaf value"
                );
            }
        }

        let mut visited = BTreeSet::new();
        let mut position = 0u64;
        loop {
            anyhow::ensure!(
                visited.insert(position),
                "IMT successor chain contains a cycle at position {position}"
            );
            let leaf = self.leaves.get(&position).ok_or_else(|| {
                anyhow!("IMT successor chain points to missing position {position}")
            })?;
            if leaf.next_index == 0 {
                anyhow::ensure!(
                    leaf.next_value == *FQ_MAX,
                    "IMT terminal leaf must point to the maximum-value boundary"
                );
                break;
            }

            anyhow::ensure!(
                leaf.next_index < self.leaf_count,
                "IMT successor index {} exceeds leaf count {}",
                leaf.next_index,
                self.leaf_count
            );
            let successor = self.leaves.get(&leaf.next_index).ok_or_else(|| {
                anyhow!(
                    "IMT successor chain points to missing position {}",
                    leaf.next_index
                )
            })?;
            anyhow::ensure!(
                FqOrdKey::from(leaf.value) < FqOrdKey::from(successor.value),
                "IMT successor chain is not strictly increasing"
            );
            anyhow::ensure!(
                leaf.next_value == successor.value,
                "IMT successor value does not match successor leaf"
            );
            position = leaf.next_index;
        }
        anyhow::ensure!(
            visited.len() as u64 == self.leaf_count,
            "IMT successor chain visits {} of {} leaves",
            visited.len(),
            self.leaf_count
        );

        let expected_value_index: BTreeMap<[u8; 32], u64> = self
            .leaves
            .iter()
            .filter_map(|(&position, leaf)| {
                (position != 0).then_some((leaf.value.to_bytes(), position))
            })
            .collect();
        anyhow::ensure!(
            self.value_index == expected_value_index,
            "IMT value index does not exactly match the leaves"
        );
        let expected_predecessor_index = Self::build_predecessor_index(&self.leaves);
        anyhow::ensure!(
            self.predecessor_index == expected_predecessor_index,
            "IMT predecessor index does not exactly match the leaves"
        );

        let mut canonical = Self {
            depth: self.depth,
            nodes: BTreeMap::new(),
            leaves: BTreeMap::new(),
            value_index: BTreeMap::new(),
            predecessor_index: BTreeMap::new(),
            leaf_count: self.leaf_count,
        };
        for (&position, leaf) in &self.leaves {
            canonical.update_path(position, leaf.commit());
        }
        anyhow::ensure!(
            self.nodes == canonical.nodes,
            "IMT stored nodes do not exactly match recomputed leaf commitments"
        );
        Ok(())
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

    pub fn hash_children(
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

    fn checked_mutation<T>(&mut self, mutation: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        self.validate_well_formed()?;
        let previous = self.clone();
        match mutation(self).and_then(|value| {
            self.validate_well_formed()?;
            Ok(value)
        }) {
            Ok(value) => Ok(value),
            Err(error) => {
                *self = previous;
                Err(error)
            }
        }
    }

    /// Update the policy data for an existing value and recompute its Merkle path.
    pub fn update_policy(&mut self, value: Fq, policy: &AssetPolicy) -> Result<IndexedLeaf> {
        self.checked_mutation(|tree| tree.update_policy_unchecked(value, policy))
    }

    fn update_policy_unchecked(&mut self, value: Fq, policy: &AssetPolicy) -> Result<IndexedLeaf> {
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
        self.checked_mutation(|tree| tree.insert_unchecked(value, policy))
    }

    fn insert_unchecked(&mut self, value: Fq, policy: &AssetPolicy) -> Result<InsertResult> {
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
        self.checked_mutation(|tree| {
            tree.sync_from_event_unchecked(
                new_leaf,
                new_position,
                updated_low_leaf,
                low_leaf_position,
            )
        })
    }

    fn sync_from_event_unchecked(
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
    pub fn rebuild_hashes(&mut self) -> Result<()> {
        self.nodes.clear();
        for position in 0..self.leaf_count {
            if let Some(leaf) = self.leaves.get(&position) {
                let commitment = leaf.commit();
                self.update_path(position, commitment);
            }
        }
        self.validate_well_formed()
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
#[path = "indexed_tree_tests.rs"]
mod tests;
