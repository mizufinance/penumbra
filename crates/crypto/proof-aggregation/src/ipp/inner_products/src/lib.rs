use ark_ec::{
    pairing::{Pairing, PairingOutput},
    CurveGroup,
};
use ark_ff::Field;
use ark_std::cfg_iter;
use std::{
    error::Error as ErrorTrait,
    fmt::{Display, Formatter, Result as FmtResult},
    marker::PhantomData,
    ops::Add,
};

#[cfg(feature = "parallel")]
use ark_ec::pairing::MillerLoopOutput;
#[cfg(feature = "parallel")]
use rayon::prelude::*;

pub type Error = Box<dyn ErrorTrait>;

/// Exact normalization and preparation work submitted to pairing kernels.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PairingPreparationCount {
    pub g1_normalization_batches: usize,
    pub g2_normalization_batches: usize,
    pub g1_normalized_elements: usize,
    pub g2_normalized_elements: usize,
    pub g1_prepared_elements: usize,
    pub g2_prepared_elements: usize,
}

impl Add for PairingPreparationCount {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            g1_normalization_batches: self.g1_normalization_batches
                + other.g1_normalization_batches,
            g2_normalization_batches: self.g2_normalization_batches
                + other.g2_normalization_batches,
            g1_normalized_elements: self.g1_normalized_elements + other.g1_normalized_elements,
            g2_normalized_elements: self.g2_normalized_elements + other.g2_normalized_elements,
            g1_prepared_elements: self.g1_prepared_elements + other.g1_prepared_elements,
            g2_prepared_elements: self.g2_prepared_elements + other.g2_prepared_elements,
        }
    }
}

#[derive(Debug)]
pub enum InnerProductError {
    EmptyInput,
    MessageLengthInvalid(usize, usize),
    PairingUnavailable,
}

/// Validate the shape required by one non-empty pairing product.
pub fn validate_pairing_shape(left_len: usize, right_len: usize) -> Result<(), InnerProductError> {
    if left_len != right_len {
        return Err(InnerProductError::MessageLengthInvalid(left_len, right_len));
    }
    if left_len == 0 {
        return Err(InnerProductError::EmptyInput);
    }
    Ok(())
}

impl ErrorTrait for InnerProductError {
    fn source(self: &Self) -> Option<&(dyn ErrorTrait + 'static)> {
        None
    }
}

impl Display for InnerProductError {
    fn fmt(self: &Self, f: &mut Formatter<'_>) -> FmtResult {
        let msg = match self {
            InnerProductError::EmptyInput => "inner product requires non-empty inputs".to_string(),
            InnerProductError::MessageLengthInvalid(left, right) => {
                format!("left length, right length: {}, {}", left, right)
            }
            InnerProductError::PairingUnavailable => {
                "cfg_multi_pairing returned no pairing result".to_string()
            }
        };
        write!(f, "{}", msg)
    }
}

pub trait InnerProduct: Copy {
    type LeftMessage;
    type RightMessage;
    type Output;

    fn inner_product(
        left: &[Self::LeftMessage],
        right: &[Self::RightMessage],
    ) -> Result<Self::Output, Error>;
}

#[derive(Copy, Clone)]
pub struct PairingInnerProduct<P: Pairing> {
    _pair: PhantomData<P>,
}

// The parallel path wins at all supported batch sizes.
const PAIRING_PARALLEL_THRESHOLD: usize = 1;

impl<P: Pairing> InnerProduct for PairingInnerProduct<P> {
    type LeftMessage = P::G1;
    type RightMessage = P::G2;
    type Output = PairingOutput<P>;

    fn inner_product(
        left: &[Self::LeftMessage],
        right: &[Self::RightMessage],
    ) -> Result<Self::Output, Error> {
        validate_pairing_shape(left.len(), right.len())
            .map_err(|error| Box::new(error) as Error)?;

        cfg_multi_pairing(left, right)
            .ok_or_else(|| Box::new(InnerProductError::PairingUnavailable) as Error)
    }
}

/// Equivalent to `P::multi_pairing`, but with more parallelism (if enabled)
pub fn cfg_multi_pairing<P: Pairing>(left: &[P::G1], right: &[P::G2]) -> Option<PairingOutput<P>> {
    cpu_multi_pairing_projective::<P>(left, right)
}

fn cpu_multi_pairing_projective<P: Pairing>(
    left: &[P::G1],
    right: &[P::G2],
) -> Option<PairingOutput<P>> {
    // We make the input affine, then convert to prepared. We do this for speed, since the
    // conversion from projective to prepared always goes through affine.

    let aff_left = P::G1::normalize_batch(left);
    let aff_right = P::G2::normalize_batch(right);

    #[cfg(feature = "parallel")]
    let use_parallel = left.len() >= PAIRING_PARALLEL_THRESHOLD;
    #[cfg(not(feature = "parallel"))]
    let use_parallel = false;

    let left = if use_parallel {
        cfg_iter!(aff_left)
            .map(P::G1Prepared::from)
            .collect::<Vec<_>>()
    } else {
        aff_left
            .iter()
            .cloned()
            .map(P::G1Prepared::from)
            .collect::<Vec<_>>()
    };
    let right = if use_parallel {
        cfg_iter!(aff_right)
            .map(P::G2Prepared::from)
            .collect::<Vec<_>>()
    } else {
        aff_right
            .iter()
            .cloned()
            .map(P::G2Prepared::from)
            .collect::<Vec<_>>()
    };

    cfg_multi_pairing_prepared::<P>(&left, &right, use_parallel)
}

/// Prepared G1 operands that can be reused across independent pairing products.
pub struct PreparedG1<P: Pairing> {
    values: Vec<P::G1Prepared>,
    preparation: PairingPreparationCount,
}

impl<P: Pairing> PreparedG1<P> {
    pub fn preparation(&self) -> PairingPreparationCount {
        self.preparation
    }
}

/// Prepared G2 operands that can be reused across independent pairing products.
pub struct PreparedG2<P: Pairing> {
    values: Vec<P::G2Prepared>,
    preparation: PairingPreparationCount,
}

impl<P: Pairing> PreparedG2<P> {
    pub fn preparation(&self) -> PairingPreparationCount {
        self.preparation
    }
}

/// Normalize and prepare one reusable G1 operand vector.
pub fn prepare_g1<P: Pairing>(values: &[P::G1]) -> PreparedG1<P> {
    let affine = P::G1::normalize_batch(values);

    #[cfg(feature = "parallel")]
    let use_parallel = values.len() >= PAIRING_PARALLEL_THRESHOLD;
    #[cfg(not(feature = "parallel"))]
    let use_parallel = false;

    let prepared = if use_parallel {
        cfg_iter!(affine)
            .map(P::G1Prepared::from)
            .collect::<Vec<_>>()
    } else {
        affine
            .iter()
            .cloned()
            .map(P::G1Prepared::from)
            .collect::<Vec<_>>()
    };

    let preparation = PairingPreparationCount {
        g1_normalization_batches: 1,
        g1_normalized_elements: values.len(),
        g1_prepared_elements: values.len(),
        ..PairingPreparationCount::default()
    };

    PreparedG1 {
        values: prepared,
        preparation,
    }
}

/// Normalize and prepare one reusable G2 operand vector.
pub fn prepare_g2<P: Pairing>(values: &[P::G2]) -> PreparedG2<P> {
    let affine = P::G2::normalize_batch(values);

    #[cfg(feature = "parallel")]
    let use_parallel = values.len() >= PAIRING_PARALLEL_THRESHOLD;
    #[cfg(not(feature = "parallel"))]
    let use_parallel = false;

    let prepared = if use_parallel {
        cfg_iter!(affine)
            .map(P::G2Prepared::from)
            .collect::<Vec<_>>()
    } else {
        affine
            .iter()
            .cloned()
            .map(P::G2Prepared::from)
            .collect::<Vec<_>>()
    };

    let preparation = PairingPreparationCount {
        g2_normalization_batches: 1,
        g2_normalized_elements: values.len(),
        g2_prepared_elements: values.len(),
        ..PairingPreparationCount::default()
    };

    PreparedG2 {
        values: prepared,
        preparation,
    }
}

/// Evaluate one pairing product from reusable prepared operands.
pub fn pair_prepared<P: Pairing>(
    left: &PreparedG1<P>,
    right: &PreparedG2<P>,
) -> Result<PairingOutput<P>, Error> {
    validate_pairing_shape(left.values.len(), right.values.len())
        .map_err(|error| Box::new(error) as Error)?;

    #[cfg(feature = "parallel")]
    let use_parallel = left.values.len() >= PAIRING_PARALLEL_THRESHOLD;
    #[cfg(not(feature = "parallel"))]
    let use_parallel = false;

    cfg_multi_pairing_prepared::<P>(&left.values, &right.values, use_parallel)
        .ok_or_else(|| Box::new(InnerProductError::PairingUnavailable) as Error)
}

pub fn cfg_multi_pairing_g1_affine_g2_prepared<P: Pairing>(
    left: &[P::G1Affine],
    right: &[P::G2Prepared],
) -> Option<PairingOutput<P>> {
    #[cfg(feature = "parallel")]
    let use_parallel = left.len() >= PAIRING_PARALLEL_THRESHOLD;
    #[cfg(not(feature = "parallel"))]
    let use_parallel = false;

    let left = if use_parallel {
        cfg_iter!(left).map(P::G1Prepared::from).collect::<Vec<_>>()
    } else {
        left.iter()
            .cloned()
            .map(P::G1Prepared::from)
            .collect::<Vec<_>>()
    };

    cfg_multi_pairing_prepared::<P>(&left, right, use_parallel)
}

fn cfg_multi_pairing_prepared<P: Pairing>(
    left: &[P::G1Prepared],
    right: &[P::G2Prepared],
    use_parallel: bool,
) -> Option<PairingOutput<P>> {
    debug_assert_eq!(left.len(), right.len());

    #[cfg(feature = "parallel")]
    if use_parallel {
        let chunk_size = (left.len() / rayon::current_num_threads()).max(1);
        // MillerLoopOutput does not implement Product; multiply its target fields.
        let product = left
            .par_chunks(chunk_size)
            .zip(right.par_chunks(chunk_size))
            .map(|(a, b)| P::multi_miller_loop(a.iter().cloned(), b.iter().cloned()).0)
            .product();
        return P::final_exponentiation(MillerLoopOutput(product));
    }
    #[cfg(not(feature = "parallel"))]
    let _ = use_parallel;
    P::final_exponentiation(P::multi_miller_loop(
        left.iter().cloned(),
        right.iter().cloned(),
    ))
}

#[derive(Copy, Clone)]
pub struct MultiexponentiationInnerProduct<G: CurveGroup> {
    _projective: PhantomData<G>,
}

impl<G: CurveGroup> InnerProduct for MultiexponentiationInnerProduct<G> {
    type LeftMessage = G;
    type RightMessage = G::ScalarField;
    type Output = G;

    fn inner_product(
        left: &[Self::LeftMessage],
        right: &[Self::RightMessage],
    ) -> Result<Self::Output, Error> {
        if left.len() != right.len() {
            return Err(Box::new(InnerProductError::MessageLengthInvalid(
                left.len(),
                right.len(),
            )));
        };

        // Can unwrap because we did the length check above
        Ok(G::msm(&G::normalize_batch(left), &right).unwrap())
    }
}

#[derive(Copy, Clone)]
pub struct ScalarInnerProduct<F: Field> {
    _field: PhantomData<F>,
}

impl<F: Field> InnerProduct for ScalarInnerProduct<F> {
    type LeftMessage = F;
    type RightMessage = F;
    type Output = F;

    fn inner_product(
        left: &[Self::LeftMessage],
        right: &[Self::RightMessage],
    ) -> Result<Self::Output, Error> {
        if left.len() != right.len() {
            return Err(Box::new(InnerProductError::MessageLengthInvalid(
                left.len(),
                right.len(),
            )));
        };
        Ok(cfg_iter!(left).zip(right).map(|(x, y)| *x * y).sum())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_bls12_381::Bls12_381;
    use ark_ec::CurveGroup;
    use ark_ff::UniformRand;
    use ark_std::rand::{rngs::StdRng, SeedableRng};

    #[test]
    fn cfg_multi_pairing_matches_reference_across_threshold_boundary() {
        let mut rng = StdRng::seed_from_u64(7);
        let below_threshold = PAIRING_PARALLEL_THRESHOLD.saturating_sub(1).max(1);
        let sizes = [1usize, below_threshold, PAIRING_PARALLEL_THRESHOLD];

        for size in sizes {
            let left = (0..size)
                .map(|_| <Bls12_381 as Pairing>::G1::rand(&mut rng))
                .collect::<Vec<_>>();
            let right = (0..size)
                .map(|_| <Bls12_381 as Pairing>::G2::rand(&mut rng))
                .collect::<Vec<_>>();

            let expected = Bls12_381::multi_pairing(
                <Bls12_381 as Pairing>::G1::normalize_batch(&left),
                <Bls12_381 as Pairing>::G2::normalize_batch(&right),
            );
            let actual = cfg_multi_pairing::<Bls12_381>(&left, &right).expect("pairing result");

            assert_eq!(actual, expected, "mismatch at size {size}");
        }
    }

    #[test]
    fn cfg_multi_pairing_g1_affine_g2_prepared_matches_projective_path() {
        let mut rng = StdRng::seed_from_u64(29);
        let size = 4usize;
        let left = (0..size)
            .map(|_| <Bls12_381 as Pairing>::G1::rand(&mut rng))
            .collect::<Vec<_>>();
        let right = (0..size)
            .map(|_| <Bls12_381 as Pairing>::G2::rand(&mut rng))
            .collect::<Vec<_>>();
        let left_affine = <Bls12_381 as Pairing>::G1::normalize_batch(&left);
        let right_prepared = <Bls12_381 as Pairing>::G2::normalize_batch(&right)
            .into_iter()
            .map(<Bls12_381 as Pairing>::G2Prepared::from)
            .collect::<Vec<_>>();

        let projective = cfg_multi_pairing::<Bls12_381>(&left, &right).expect("projective result");

        let prepared =
            cfg_multi_pairing_g1_affine_g2_prepared::<Bls12_381>(&left_affine, &right_prepared)
                .expect("prepared result");

        assert_eq!(projective, prepared);
    }
}
