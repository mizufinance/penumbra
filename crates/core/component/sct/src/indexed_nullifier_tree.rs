use anyhow::{ensure, Context};
use ark_ff::{BigInteger, PrimeField};
use decaf377::Fq;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use shieldd_sdk_proto::{core::component::sct::v1 as pb, DomainType};

use crate::Nullifier;

pub const DEPTH: u8 = 20;
pub const CAPACITY: u64 = 1u64 << (DEPTH as u32 * 2);

pub static LEAF_DOMAIN: Lazy<Fq> = Lazy::new(|| domain(b"shieldd.nullifier.imt.leaf"));

pub static ZERO_HASHES: Lazy<Vec<Fq>> = Lazy::new(|| {
    let mut hashes = Vec::with_capacity(DEPTH as usize + 1);
    hashes.push(Fq::from(0u64));
    for level in 1..=DEPTH as usize {
        let child = hashes[level - 1];
        hashes.push(poseidon377::hash_4(
            &Fq::from(0u64),
            (child, child, child, child),
        ));
    }
    hashes
});

fn domain(label: &[u8]) -> Fq {
    Fq::from_le_bytes_mod_order(blake2b_simd::blake2b(label).as_bytes())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct IndexedNullifierLeaf {
    pub value: [u8; 32],
    pub next_index: u64,
    pub next_value: [u8; 32],
    pub is_lower_sentinel: bool,
    pub is_terminal: bool,
}

impl IndexedNullifierLeaf {
    pub fn lower_sentinel() -> Self {
        Self {
            value: Fq::from(0u64).to_bytes(),
            next_index: 0,
            next_value: Fq::from(0u64).to_bytes(),
            is_lower_sentinel: true,
            is_terminal: true,
        }
    }

    pub fn ordinary(
        value: Nullifier,
        next_index: u64,
        next_value: [u8; 32],
        is_terminal: bool,
    ) -> Self {
        Self {
            value: value.to_bytes(),
            next_index,
            next_value,
            is_lower_sentinel: false,
            is_terminal,
        }
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        let _ = self.value_fq()?;
        let _ = self.next_value_fq()?;
        ensure!(
            !self.is_lower_sentinel || self.value_fq()? == Fq::from(0u64),
            "lower sentinel has a nonzero value"
        );
        ensure!(
            !self.is_terminal || (self.next_index == 0 && self.next_value_fq()? == Fq::from(0u64)),
            "terminal indexed leaf has a successor"
        );
        ensure!(
            self.is_terminal || self.next_index < CAPACITY,
            "indexed leaf successor exceeds capacity"
        );
        ensure!(
            self.is_terminal || self.next_index > 0,
            "nonterminal indexed leaf points at the sentinel"
        );
        Ok(())
    }

    pub fn value_fq(&self) -> anyhow::Result<Fq> {
        Fq::from_bytes_checked(&self.value).map_err(|_| anyhow::anyhow!("invalid leaf value"))
    }

    pub fn next_value_fq(&self) -> anyhow::Result<Fq> {
        Fq::from_bytes_checked(&self.next_value)
            .map_err(|_| anyhow::anyhow!("invalid leaf successor value"))
    }

    pub fn commitment(&self) -> anyhow::Result<Fq> {
        self.validate()?;
        Ok(poseidon377::hash_5(
            &LEAF_DOMAIN,
            (
                self.value_fq()?,
                Fq::from(self.next_index),
                self.next_value_fq()?,
                Fq::from(self.is_lower_sentinel as u64),
                Fq::from(self.is_terminal as u64),
            ),
        ))
    }
}

impl DomainType for IndexedNullifierLeaf {
    type Proto = pb::IndexedNullifierLeaf;
}

impl From<IndexedNullifierLeaf> for pb::IndexedNullifierLeaf {
    fn from(value: IndexedNullifierLeaf) -> Self {
        Self {
            value: value.value.to_vec(),
            next_index: value.next_index,
            next_value: value.next_value.to_vec(),
            is_lower_sentinel: value.is_lower_sentinel,
            is_terminal: value.is_terminal,
        }
    }
}

impl TryFrom<pb::IndexedNullifierLeaf> for IndexedNullifierLeaf {
    type Error = anyhow::Error;

    fn try_from(value: pb::IndexedNullifierLeaf) -> Result<Self, Self::Error> {
        let leaf = Self {
            value: decode32(value.value, "indexed leaf value")?,
            next_index: value.next_index,
            next_value: decode32(value.next_value, "indexed leaf successor value")?,
            is_lower_sentinel: value.is_lower_sentinel,
            is_terminal: value.is_terminal,
        };
        leaf.validate()?;
        Ok(leaf)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct IndexedNullifierWitness {
    pub leaf_position: u64,
    pub leaf: IndexedNullifierLeaf,
    pub auth_path: Vec<[[u8; 32]; 3]>,
}

impl IndexedNullifierWitness {
    pub fn validate(&self) -> anyhow::Result<()> {
        ensure!(
            self.leaf_position < CAPACITY,
            "indexed leaf position exceeds capacity"
        );
        self.leaf.validate()?;
        ensure!(
            self.leaf.is_lower_sentinel == (self.leaf_position == 0),
            "only the lower sentinel may occupy leaf position zero"
        );
        ensure!(
            self.auth_path.len() == DEPTH as usize,
            "indexed nullifier path must contain exactly {DEPTH} layers"
        );
        for layer in &self.auth_path {
            for sibling in layer {
                let _ = Fq::from_bytes_checked(sibling)
                    .map_err(|_| anyhow::anyhow!("invalid indexed path sibling"))?;
            }
        }
        Ok(())
    }

    pub fn root(&self) -> anyhow::Result<[u8; 32]> {
        self.validate()?;
        let mut current = self.leaf.commitment()?;
        let mut position = self.leaf_position;
        for layer in &self.auth_path {
            let siblings = layer
                .iter()
                .map(|bytes| {
                    Fq::from_bytes_checked(bytes)
                        .map_err(|_| anyhow::anyhow!("invalid indexed path sibling"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let children = match position % 4 {
                0 => [current, siblings[0], siblings[1], siblings[2]],
                1 => [siblings[0], current, siblings[1], siblings[2]],
                2 => [siblings[0], siblings[1], current, siblings[2]],
                3 => [siblings[0], siblings[1], siblings[2], current],
                _ => unreachable!(),
            };
            current = hash_children(children);
            position /= 4;
        }
        Ok(current.to_bytes())
    }

    pub fn verify_membership(
        &self,
        nullifier: Nullifier,
        expected_root: [u8; 32],
    ) -> anyhow::Result<()> {
        ensure!(
            !self.leaf.is_lower_sentinel,
            "sentinel cannot prove membership"
        );
        ensure!(
            self.leaf.value == nullifier.to_bytes(),
            "membership leaf value mismatch"
        );
        ensure!(
            self.root()? == expected_root,
            "indexed membership root mismatch"
        );
        Ok(())
    }

    pub fn verify_nonmembership(
        &self,
        nullifier: Nullifier,
        expected_root: [u8; 32],
    ) -> anyhow::Result<()> {
        ensure!(
            self.root()? == expected_root,
            "indexed nonmembership root mismatch"
        );
        let target = FqOrdKey::from(nullifier.0);
        if !self.leaf.is_lower_sentinel {
            ensure!(
                FqOrdKey::from(self.leaf.value_fq()?) < target,
                "target is not above predecessor"
            );
        }
        if !self.leaf.is_terminal {
            ensure!(
                target < FqOrdKey::from(self.leaf.next_value_fq()?),
                "target is not below successor"
            );
        }
        Ok(())
    }
}

impl DomainType for IndexedNullifierWitness {
    type Proto = pb::IndexedNullifierWitness;
}

impl From<IndexedNullifierWitness> for pb::IndexedNullifierWitness {
    fn from(value: IndexedNullifierWitness) -> Self {
        Self {
            leaf_position: value.leaf_position,
            leaf: Some(value.leaf.into()),
            auth_path: value
                .auth_path
                .into_iter()
                .map(|siblings| pb::IndexedNullifierPathLayer {
                    siblings: siblings
                        .into_iter()
                        .map(|sibling| sibling.to_vec())
                        .collect(),
                })
                .collect(),
        }
    }
}

impl TryFrom<pb::IndexedNullifierWitness> for IndexedNullifierWitness {
    type Error = anyhow::Error;

    fn try_from(value: pb::IndexedNullifierWitness) -> Result<Self, Self::Error> {
        let witness = Self {
            leaf_position: value.leaf_position,
            leaf: value
                .leaf
                .context("indexed witness leaf is missing")?
                .try_into()?,
            auth_path: value
                .auth_path
                .into_iter()
                .map(|layer| {
                    ensure!(
                        layer.siblings.len() == 3,
                        "indexed path layer must have three siblings"
                    );
                    let siblings = layer
                        .siblings
                        .into_iter()
                        .map(|bytes| decode32(bytes, "indexed path sibling"))
                        .collect::<anyhow::Result<Vec<_>>>()?;
                    Ok(siblings.try_into().expect("three siblings were checked"))
                })
                .collect::<anyhow::Result<Vec<_>>>()?,
        };
        witness.validate()?;
        Ok(witness)
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct FqOrdKey(pub [u8; 32]);

impl From<Fq> for FqOrdKey {
    fn from(value: Fq) -> Self {
        let bytes = value.into_bigint().to_bytes_be();
        let mut key = [0u8; 32];
        key[32 - bytes.len()..].copy_from_slice(&bytes);
        Self(key)
    }
}

pub fn hash_children(children: [Fq; 4]) -> Fq {
    poseidon377::hash_4(
        &Fq::from(0u64),
        (children[0], children[1], children[2], children[3]),
    )
}

fn decode32(bytes: Vec<u8>, label: &str) -> anyhow::Result<[u8; 32]> {
    bytes
        .try_into()
        .map_err(|bytes: Vec<u8>| anyhow::anyhow!("{label} must be 32 bytes, got {}", bytes.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentinel_gap_includes_field_boundaries() -> anyhow::Result<()> {
        let sentinel = IndexedNullifierLeaf::lower_sentinel();
        let witness = IndexedNullifierWitness {
            leaf_position: 0,
            leaf: sentinel,
            auth_path: (0..DEPTH)
                .map(|level| {
                    let sibling = ZERO_HASHES[level as usize].to_bytes();
                    [sibling, sibling, sibling]
                })
                .collect(),
        };
        let root = witness.root()?;
        witness.verify_nonmembership(Nullifier(Fq::from(0u64)), root)?;
        witness.verify_nonmembership(Nullifier(Fq::from(0u64) - Fq::from(1u64)), root)?;
        Ok(())
    }
}
