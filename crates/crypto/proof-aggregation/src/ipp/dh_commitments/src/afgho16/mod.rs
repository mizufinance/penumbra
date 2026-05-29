use ark_ec::pairing::{Pairing, PairingOutput};
use ark_ec::CurveGroup;
use ark_std::rand::Rng;
use std::marker::PhantomData;

use crate::{random_generators, DoublyHomomorphicCommitment, Error};

use ark_inner_products::{InnerProduct, PairingInnerProduct};

#[derive(Clone)]
pub struct AFGHOCommitment<P: Pairing> {
    _pair: PhantomData<P>,
}

#[derive(Clone)]
pub struct AFGHOCommitmentG1<P: Pairing>(AFGHOCommitment<P>);

#[derive(Clone)]
pub struct AFGHOCommitmentG2<P: Pairing>(AFGHOCommitment<P>);

impl<P: Pairing> DoublyHomomorphicCommitment for AFGHOCommitmentG1<P> {
    type Scalar = P::ScalarField;
    type Message = P::G1;
    type Key = P::G2;
    type Output = PairingOutput<P>;

    fn setup<R: Rng>(rng: &mut R, size: usize) -> Result<Vec<Self::Key>, Error> {
        Ok(random_generators(rng, size))
    }

    fn commit(k: &[Self::Key], m: &[Self::Message]) -> Result<Self::Output, Error> {
        Ok(PairingInnerProduct::<P>::inner_product(m, k)?)
    }

    fn msm_keys(keys: &[Self::Key], scalars: &[Self::Scalar]) -> Self::Key {
        msm_group::<P::G2>(keys, scalars)
    }
}

impl<P: Pairing> DoublyHomomorphicCommitment for AFGHOCommitmentG2<P> {
    type Scalar = P::ScalarField;
    type Message = P::G2;
    type Key = P::G1;
    type Output = PairingOutput<P>;

    fn setup<R: Rng>(rng: &mut R, size: usize) -> Result<Vec<Self::Key>, Error> {
        Ok(random_generators(rng, size))
    }

    fn commit(k: &[Self::Key], m: &[Self::Message]) -> Result<Self::Output, Error> {
        Ok(PairingInnerProduct::<P>::inner_product(k, m)?)
    }

    fn msm_keys(keys: &[Self::Key], scalars: &[Self::Scalar]) -> Self::Key {
        msm_group::<P::G1>(keys, scalars)
    }
}

/// `Σ scalarsᵢ · keysᵢ` via variable-base MSM. Identical group element to the
/// sequential fold, computed faster. Used by the GIPA final commitment-key
/// recombination on the verifier path.
fn msm_group<G: CurveGroup>(keys: &[G], scalars: &[G::ScalarField]) -> G {
    assert!(!keys.is_empty(), "msm_keys requires a non-empty key vector");
    assert_eq!(
        keys.len(),
        scalars.len(),
        "msm_keys requires matching key/scalar lengths"
    );
    let bases = G::normalize_batch(keys);
    G::msm(&bases, scalars).expect("msm bases and scalars have equal length")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_bls12_381::Bls12_381;
    use ark_ff::UniformRand;
    use ark_std::rand::{rngs::StdRng, SeedableRng};

    type C1 = AFGHOCommitmentG1<Bls12_381>;
    type C2 = AFGHOCommitmentG2<Bls12_381>;
    const TEST_SIZE: usize = 8;

    #[test]
    fn afgho_g1_test() {
        let mut rng = StdRng::seed_from_u64(0u64);
        let commit_keys = C1::setup(&mut rng, TEST_SIZE).unwrap();
        let mut message = Vec::new();
        let mut wrong_message = Vec::new();
        for _ in 0..TEST_SIZE {
            message.push(<Bls12_381 as Pairing>::G1::rand(&mut rng));
            wrong_message.push(<Bls12_381 as Pairing>::G1::rand(&mut rng));
        }
        let com = C1::commit(&commit_keys, &message).unwrap();
        assert!(C1::verify(&commit_keys, &message, &com).unwrap());
        assert!(!C1::verify(&commit_keys, &wrong_message, &com).unwrap());
        message.push(<Bls12_381 as Pairing>::G1::rand(&mut rng));
        assert!(C1::verify(&commit_keys, &message, &com).is_err());
    }

    // Sequential reference for the MSM override: this is the prior
    // `_compute_final_commitment_keys` accumulation, kept here as the
    // byte-equivalence oracle for `msm_keys`.
    fn sequential_fold<G: ark_ec::CurveGroup>(keys: &[G], scalars: &[G::ScalarField]) -> G {
        let mut acc = keys[0];
        acc *= scalars[0];
        for (k, s) in keys[1..].iter().zip(&scalars[1..]) {
            let mut term = *k;
            term *= *s;
            acc += term;
        }
        acc
    }

    #[test]
    fn msm_keys_equals_sequential_fold() {
        let mut rng = StdRng::seed_from_u64(7u64);
        for size in [1usize, 2, 4, 8, 64, 256] {
            // C1's key side is G2; C2's key side is G1. Both verifier
            // commitment-key recombinations go through `msm_keys`, so cover
            // both override impls.
            let g2_keys: Vec<_> = (0..size)
                .map(|_| <Bls12_381 as Pairing>::G2::rand(&mut rng))
                .collect();
            let g1_keys: Vec<_> = (0..size)
                .map(|_| <Bls12_381 as Pairing>::G1::rand(&mut rng))
                .collect();
            let scalars: Vec<_> = (0..size)
                .map(|_| <Bls12_381 as ark_ec::pairing::Pairing>::ScalarField::rand(&mut rng))
                .collect();

            assert_eq!(
                C1::msm_keys(&g2_keys, &scalars),
                sequential_fold(&g2_keys, &scalars),
                "G1 commitment (G2 key side) msm_keys must match the fold at size {size}"
            );
            assert_eq!(
                C2::msm_keys(&g1_keys, &scalars),
                sequential_fold(&g1_keys, &scalars),
                "G2 commitment (G1 key side) msm_keys must match the fold at size {size}"
            );
        }
    }

    #[test]
    #[ignore = "informational performance comparison; prints msm vs sequential-fold timing"]
    fn msm_keys_performance_report() {
        use std::time::Instant;
        let mut rng = StdRng::seed_from_u64(11u64);
        for size in [256usize, 1024, 4096, 16384] {
            let keys: Vec<_> = (0..size)
                .map(|_| <Bls12_381 as Pairing>::G2::rand(&mut rng))
                .collect();
            let scalars: Vec<_> = (0..size)
                .map(|_| <Bls12_381 as ark_ec::pairing::Pairing>::ScalarField::rand(&mut rng))
                .collect();

            let t0 = Instant::now();
            let via_fold = sequential_fold(&keys, &scalars);
            let fold_ns = t0.elapsed().as_nanos();

            let t1 = Instant::now();
            let via_msm = C1::msm_keys(&keys, &scalars);
            let msm_ns = t1.elapsed().as_nanos();

            assert_eq!(via_msm, via_fold);
            println!(
                "size={size:>6}  fold={fold_ns:>12}ns  msm={msm_ns:>12}ns  speedup={:.2}x",
                fold_ns as f64 / msm_ns as f64
            );
        }
    }

    #[test]
    fn afgho_g2_test() {
        let mut rng = StdRng::seed_from_u64(0u64);
        let commit_keys = C2::setup(&mut rng, TEST_SIZE).unwrap();
        let mut message = Vec::new();
        let mut wrong_message = Vec::new();
        for _ in 0..TEST_SIZE {
            message.push(<Bls12_381 as Pairing>::G2::rand(&mut rng));
            wrong_message.push(<Bls12_381 as Pairing>::G2::rand(&mut rng));
        }
        let com = C2::commit(&commit_keys, &message).unwrap();
        assert!(C2::verify(&commit_keys, &message, &com).unwrap());
        assert!(!C2::verify(&commit_keys, &wrong_message, &com).unwrap());
        message.push(<Bls12_381 as Pairing>::G2::rand(&mut rng));
        assert!(C2::verify(&commit_keys, &message, &com).is_err());
    }
}
