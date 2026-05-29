use ark_ec::PrimeGroup;
use ark_ff::fields::PrimeField;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ark_std::rand::Rng;
use std::{
    cmp::Eq,
    error::Error as ErrorTrait,
    ops::{Add, MulAssign},
};

pub mod afgho16;
pub mod identity;
pub mod pedersen;

pub type Error = Box<dyn ErrorTrait>;

//TODO: support CanonicalSerialize
//TODO: Using MulAssign instead of Mul because the group trait does not support Mul

pub trait DoublyHomomorphicCommitment: Clone {
    type Scalar: PrimeField;
    type Message: CanonicalSerialize
        + CanonicalDeserialize
        + Clone
        + Default
        + Eq
        + Send
        + Sync
        + Add<Self::Message, Output = Self::Message>
        + MulAssign<Self::Scalar>;
    type Key: CanonicalSerialize
        + CanonicalDeserialize
        + Clone
        + Default
        + Eq
        + Send
        + Sync
        + Add<Self::Key, Output = Self::Key>
        + MulAssign<Self::Scalar>;
    type Output: CanonicalSerialize
        + CanonicalDeserialize
        + Clone
        + Default
        + Eq
        + Add<Self::Output, Output = Self::Output>
        + MulAssign<Self::Scalar>;

    fn setup<R: Rng>(r: &mut R, size: usize) -> Result<Vec<Self::Key>, Error>;

    /// Multiexponentiation `Σ scalarsᵢ · keysᵢ`.
    ///
    /// The default mirrors the original sequential accumulation exactly (first
    /// term initializes the accumulator, the rest fold in). Group-backed
    /// commitments override this with a real MSM; the result is the same group
    /// element either way, so callers stay byte-for-byte identical.
    ///
    /// Precondition: `keys` is non-empty and `keys.len() == scalars.len()`.
    /// Both are asserted (the default seeds the accumulator from `keys[0]`).
    fn msm_keys(keys: &[Self::Key], scalars: &[Self::Scalar]) -> Self::Key {
        assert!(!keys.is_empty(), "msm_keys requires a non-empty key vector");
        assert_eq!(
            keys.len(),
            scalars.len(),
            "msm_keys requires matching key/scalar lengths"
        );
        let mut acc = keys[0].clone();
        acc.mul_assign(scalars[0]);
        for (key, scalar) in keys[1..].iter().zip(&scalars[1..]) {
            let mut term = key.clone();
            term.mul_assign(*scalar);
            acc = acc + term;
        }
        acc
    }

    fn commit(k: &[Self::Key], m: &[Self::Message]) -> Result<Self::Output, Error>;

    fn verify(k: &[Self::Key], m: &[Self::Message], com: &Self::Output) -> Result<bool, Error> {
        Ok(Self::commit(k, m)? == *com)
    }
}

// Helpers for generator commitment keys used by Pedersen and AFGHO16

pub fn random_generators<R: Rng, G: PrimeGroup>(rng: &mut R, num: usize) -> Vec<G> {
    (0..num).map(|_| G::rand(rng)).collect()
}
