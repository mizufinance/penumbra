//! Batch Groth16 verification for BLS12-377 proofs.
//!
//! Combines `k` proofs sharing a single `PreparedVerifyingKey` into one
//! multi-pairing check: `k + 2` miller loops + 1 final exponentiation
//! instead of `4k` miller loops + `k` final exponentiations.
//!
//! The batch optimizer derives deterministic SHA-256 randomizers from each
//! key/proof/input tuple. It is retained for benchmarks and non-authoritative
//! optimization measurements. Consensus acceptance uses [`verify_each`], so
//! it does not rely on an unproved deterministic batching argument.

use std::sync::Arc;

use ark_ec::{
    pairing::Pairing, scalar_mul::variable_base::VariableBaseMSM, AffineRepr, CurveGroup,
};
use ark_ff::{CyclotomicMultSubgroup, PrimeField, Zero};
use ark_groth16::{PreparedVerifyingKey, Proof};
use ark_serialize::CanonicalSerialize;
use ark_std::cfg_iter;
use decaf377::Bls12_377;
#[cfg(feature = "parallel")]
use rayon::prelude::*;
use sha2::{Digest as _, Sha256};

use crate::DeployedProofKey;

type Fr = <Bls12_377 as Pairing>::ScalarField;
type G1 = <Bls12_377 as Pairing>::G1;
type G1Affine = <Bls12_377 as Pairing>::G1Affine;
type G1Prepared = <Bls12_377 as Pairing>::G1Prepared;
type G2Prepared = <Bls12_377 as Pairing>::G2Prepared;
type TargetField = <Bls12_377 as Pairing>::TargetField;

/// A single Groth16 proof bundled with its public inputs, ready for batch verification.
#[derive(Clone, PartialEq)]
pub struct BatchItem {
    pub proof: Proof<Bls12_377>,
    pub public_inputs: Vec<<Bls12_377 as Pairing>::ScalarField>,
}

/// Evidence that one exact public statement verified under one deployed key.
///
/// The proof serialization is deliberately not part of the capability identity:
/// independent and aggregate proofs may establish the same statement. Consumers
/// must bind the capability back to the action-derived public inputs and key.
#[derive(Clone)]
pub struct VerifiedBatchItem {
    key: DeployedProofKey,
    item: Arc<BatchItem>,
}

impl VerifiedBatchItem {
    /// Require this capability to name the exact key and public inputs.
    pub fn ensure_binds(
        &self,
        key: DeployedProofKey,
        item: &BatchItem,
    ) -> Result<(), BatchVerifyError> {
        if self.key != key || self.item.public_inputs != item.public_inputs {
            return Err(BatchVerifyError::VerifiedItemMismatch);
        }
        Ok(())
    }

    /// Mint evidence after another verifier has established this exact statement.
    ///
    /// # Safety
    ///
    /// The caller must have verified a sound proof for `item.public_inputs`
    /// under `key`. This is exposed only so the SnarkPack verifier crate can
    /// translate its authenticated accepted result into the shared capability.
    #[doc(hidden)]
    pub unsafe fn from_verified_statement(key: DeployedProofKey, item: Arc<BatchItem>) -> Self {
        Self { key, item }
    }
}

/// Error returned by batch verification.
#[derive(Debug)]
pub enum BatchVerifyError {
    /// The batch check failed. In production, no per-proof attribution is provided.
    /// Use `identify_failing_proofs` in test/diagnostic contexts for per-proof detail.
    BatchFailed,
    /// Public input count does not match the verification key at the given index.
    InputLengthMismatch {
        index: usize,
        expected: usize,
        got: usize,
    },
    /// A verified capability was presented for another key or proof item.
    VerifiedItemMismatch,
}

impl std::fmt::Display for BatchVerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BatchFailed => write!(f, "batch Groth16 verification failed"),
            Self::InputLengthMismatch {
                index,
                expected,
                got,
            } => {
                write!(
                    f,
                    "proof {index}: expected {expected} public inputs, got {got}"
                )
            }
            Self::VerifiedItemMismatch => write!(
                f,
                "verified proof capability does not bind this deployed statement"
            ),
        }
    }
}

impl std::error::Error for BatchVerifyError {}

/// Batch-verify Groth16 proofs sharing a single `PreparedVerifyingKey`.
///
/// On failure, returns `Err(BatchVerifyError::BatchFailed)` immediately.
/// No per-proof fallback is performed in production — use `identify_failing_proofs`
/// separately in test or diagnostic contexts.
///
/// # Panics
///
/// Panics if the RNG fails to produce a nonzero scalar (astronomically unlikely).
pub fn batch_verify(
    pvk: &PreparedVerifyingKey<Bls12_377>,
    items: &[BatchItem],
) -> Result<(), BatchVerifyError> {
    if items.is_empty() {
        return Ok(());
    }

    // For a single proof, fall through to the batch math — it's equivalent
    // to individual verification and keeps one code path.

    let k = items.len();
    let num_public_inputs = pvk
        .vk
        .gamma_abc_g1
        .len()
        .checked_sub(1)
        .ok_or(BatchVerifyError::BatchFailed)?;

    // Validate input lengths upfront.
    for (i, item) in items.iter().enumerate() {
        if item.public_inputs.len() != num_public_inputs {
            return Err(BatchVerifyError::InputLengthMismatch {
                index: i,
                expected: num_public_inputs,
                got: item.public_inputs.len(),
            });
        }
    }

    // 1. Derive deterministic nonzero randomizers from batch contents.
    // This keeps verification deterministic across validators.
    let scalars = derive_batch_randomizers(pvk, items)?;

    // 2. For each proof, compute the public input accumulation point L_i in G1:
    //    L_i = gamma_abc_g1[0] + sum_j { pub_j * gamma_abc_g1[j+1] }
    let l_points: Vec<G1> = cfg_iter!(items)
        .map(|item| prepare_inputs(&pvk.vk.gamma_abc_g1, &item.public_inputs))
        .collect();

    // 3. MSM to fold: sum_L = Σ r_i·L_i, sum_C = Σ r_i·C_i, sum_r = Σ r_i
    let l_affine: Vec<G1Affine> = l_points.iter().map(|p| p.into_affine()).collect();
    let c_affine: Vec<G1Affine> = items.iter().map(|item| item.proof.c).collect();

    let sum_l: G1 = <G1 as VariableBaseMSM>::msm_unchecked(&l_affine, &scalars);
    let sum_c: G1 = <G1 as VariableBaseMSM>::msm_unchecked(&c_affine, &scalars);
    let sum_r: Fr = scalars.iter().copied().sum();

    // 4. Build miller loop inputs.
    //    LHS: Σ e(r_i·A_i, B_i) — we scale A_i by r_i
    //    RHS (moved to LHS with negation): e(sum_L, -gamma) + e(sum_C, -delta)
    //
    //    multi_miller_loop takes two separate iterators: G1Prepared and G2Prepared.
    let mut ml_a: Vec<G1Prepared> = Vec::with_capacity(k + 2);
    let mut ml_b: Vec<G2Prepared> = Vec::with_capacity(k + 2);

    for (i, item) in items.iter().enumerate() {
        let scaled_a: G1 = item.proof.a.into_group() * scalars[i];
        ml_a.push(scaled_a.into_affine().into());
        ml_b.push(item.proof.b.into());
    }

    // Append the folded L and C terms with negated gamma/delta from the PVK.
    ml_a.push(sum_l.into_affine().into());
    ml_b.push(pvk.gamma_g2_neg_pc.clone());

    ml_a.push(sum_c.into_affine().into());
    ml_b.push(pvk.delta_g2_neg_pc.clone());

    // 5. Multi-miller-loop + final exponentiation.
    let ml_result = Bls12_377::multi_miller_loop(ml_a, ml_b);
    let pairing_result =
        Bls12_377::final_exponentiation(ml_result).ok_or(BatchVerifyError::BatchFailed)?;

    // 6. Compare against alpha_beta^sum_r.
    //    pvk.alpha_g1_beta_g2 is e(alpha, beta).0 stored as TargetField (Fp12).
    //    We need: pairing_result.0 == alpha_g1_beta_g2^sum_r
    //    TargetField implements CyclotomicMultSubgroup with cyclotomic_exp.
    let expected: TargetField =
        CyclotomicMultSubgroup::cyclotomic_exp(&pvk.alpha_g1_beta_g2, sum_r.into_bigint().as_ref());

    if pairing_result.0 == expected {
        Ok(())
    } else {
        Err(BatchVerifyError::BatchFailed)
    }
}

/// Verify every Groth16 proof independently under the supplied prepared key.
///
/// Consensus execution uses this path when no ceremony-derived aggregation
/// SRS is available. Acceptance therefore does not rely on batch
/// randomization or aggregate-proof soundness.
pub fn verify_each(
    pvk: &PreparedVerifyingKey<Bls12_377>,
    items: &[BatchItem],
) -> Result<(), BatchVerifyError> {
    use ark_groth16::r1cs_to_qap::LibsnarkReduction;
    use ark_groth16::Groth16;
    use ark_snark::SNARK;

    let num_public_inputs = pvk
        .vk
        .gamma_abc_g1
        .len()
        .checked_sub(1)
        .ok_or(BatchVerifyError::BatchFailed)?;
    for (index, item) in items.iter().enumerate() {
        if item.public_inputs.len() != num_public_inputs {
            return Err(BatchVerifyError::InputLengthMismatch {
                index,
                expected: num_public_inputs,
                got: item.public_inputs.len(),
            });
        }
        let accepted = Groth16::<Bls12_377, LibsnarkReduction>::verify_with_processed_vk(
            pvk,
            &item.public_inputs,
            &item.proof,
        )
        .map_err(|_| BatchVerifyError::BatchFailed)?;
        if !accepted {
            return Err(BatchVerifyError::BatchFailed);
        }
    }
    Ok(())
}

/// Verify every item and return an opaque capability for each input item.
///
/// Capabilities preserve input order and retain only a compact key identity
/// and their exact item. No capability retains the prepared key or unrelated
/// batch items, and none is returned unless the complete slice verifies.
pub fn verify_each_with_capabilities(
    key: DeployedProofKey,
    items: Vec<Arc<BatchItem>>,
) -> Result<Vec<VerifiedBatchItem>, BatchVerifyError> {
    let pvk = key.bundled_pvk();
    for (index, item) in items.iter().enumerate() {
        verify_each(pvk, std::slice::from_ref(item.as_ref())).map_err(|error| match error {
            BatchVerifyError::InputLengthMismatch { expected, got, .. } => {
                BatchVerifyError::InputLengthMismatch {
                    index,
                    expected,
                    got,
                }
            }
            other => other,
        })?;
    }
    Ok(items
        .into_iter()
        .map(|item| VerifiedBatchItem { key, item })
        .collect())
}

/// Diagnostic function: individually verify each proof and return indices of failures.
/// Only for tests and offline diagnostics — never called in production consensus.
pub fn identify_failing_proofs(
    pvk: &PreparedVerifyingKey<Bls12_377>,
    items: &[BatchItem],
) -> Vec<usize> {
    use ark_groth16::r1cs_to_qap::LibsnarkReduction;
    use ark_groth16::Groth16;
    use ark_snark::SNARK;

    items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| {
            let ok = Groth16::<Bls12_377, LibsnarkReduction>::verify_with_processed_vk(
                pvk,
                &item.public_inputs,
                &item.proof,
            )
            .map(|valid| valid)
            .unwrap_or(false);

            if ok {
                None
            } else {
                Some(i)
            }
        })
        .collect()
}

/// Compute L = gamma_abc_g1[0] + Σ pub_i * gamma_abc_g1[i+1].
/// Matches the logic of `ark_groth16::Groth16::prepare_inputs`.
fn prepare_inputs(gamma_abc_g1: &[G1Affine], public_inputs: &[Fr]) -> G1 {
    use core::ops::AddAssign;
    let mut acc: G1 = gamma_abc_g1[0].into_group();
    for (input, base) in public_inputs.iter().zip(gamma_abc_g1.iter().skip(1)) {
        acc.add_assign(&base.mul_bigint(input.into_bigint()));
    }
    acc
}

fn derive_batch_randomizers(
    pvk: &PreparedVerifyingKey<Bls12_377>,
    items: &[BatchItem],
) -> Result<Vec<Fr>, BatchVerifyError> {
    let vk_digest = hash_vk(pvk)?;
    let mut scalars = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let item_digest = hash_batch_item(item)?;
        scalars.push(derive_nonzero_fr(&vk_digest, &item_digest, index));
    }
    Ok(scalars)
}

fn hash_vk(pvk: &PreparedVerifyingKey<Bls12_377>) -> Result<[u8; 32], BatchVerifyError> {
    let mut bytes = Vec::new();
    pvk.vk
        .serialize_compressed(&mut bytes)
        .map_err(|_| BatchVerifyError::BatchFailed)?;
    Ok(Sha256::digest(&bytes).into())
}

fn hash_batch_item(item: &BatchItem) -> Result<[u8; 32], BatchVerifyError> {
    let mut hasher = Sha256::new();

    let mut proof_bytes = Vec::new();
    item.proof
        .serialize_compressed(&mut proof_bytes)
        .map_err(|_| BatchVerifyError::BatchFailed)?;
    hasher.update((proof_bytes.len() as u64).to_le_bytes());
    hasher.update(&proof_bytes);

    for input in &item.public_inputs {
        let mut bytes = Vec::new();
        input
            .serialize_compressed(&mut bytes)
            .map_err(|_| BatchVerifyError::BatchFailed)?;
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }

    Ok(hasher.finalize().into())
}

/// Derive a nonzero Fr scalar deterministically from domain-separated hashes.
fn derive_nonzero_fr(vk_digest: &[u8; 32], item_digest: &[u8; 32], index: usize) -> Fr {
    let mut counter = 0u32;
    loop {
        let mut hasher = Sha256::new();
        hasher.update(b"shieldd.batch.verify");
        hasher.update(vk_digest);
        hasher.update(item_digest);
        hasher.update((index as u64).to_le_bytes());
        hasher.update(counter.to_le_bytes());
        let digest = hasher.finalize();
        let r = Fr::from_le_bytes_mod_order(&digest);
        if !r.is_zero() {
            return r;
        }
        counter = counter.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_groth16::r1cs_to_qap::LibsnarkReduction;
    use ark_groth16::Groth16;
    use ark_snark::SNARK;

    /// Verify that batch_verify agrees with individual verification for valid proofs,
    /// and that both reject mutated proofs.
    ///
    /// This requires generating real proofs, which depends on the proof test helpers.
    /// For now, we test the mathematical properties with a minimal circuit.
    mod math_tests {
        use super::*;
        use ark_groth16::ProvingKey;
        use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
        use rand::SeedableRng;
        use rand_chacha::ChaCha20Rng;

        /// Minimal circuit: proves knowledge of x such that x * x = public_input.
        #[derive(Clone)]
        struct SquareCircuit {
            x: Option<Fr>,
        }

        impl ConstraintSynthesizer<Fr> for SquareCircuit {
            fn generate_constraints(
                self,
                cs: ConstraintSystemRef<Fr>,
            ) -> Result<(), SynthesisError> {
                use ark_r1cs_std::alloc::AllocVar;
                use ark_r1cs_std::eq::EqGadget;
                use ark_r1cs_std::fields::fp::FpVar;

                let x = FpVar::new_witness(cs.clone(), || {
                    self.x.ok_or(SynthesisError::AssignmentMissing)
                })?;
                let x_sq = &x * &x;

                let public = FpVar::new_input(cs.clone(), || {
                    let x_val = self.x.ok_or(SynthesisError::AssignmentMissing)?;
                    Ok(x_val * x_val)
                })?;

                x_sq.enforce_equal(&public)?;
                Ok(())
            }
        }

        fn setup_and_prove(
            rng: &mut impl rand_core::CryptoRngCore,
            x: Fr,
        ) -> (ProvingKey<Bls12_377>, Proof<Bls12_377>, Vec<Fr>) {
            let circuit = SquareCircuit { x: Some(x) };
            let pk =
                Groth16::<Bls12_377, LibsnarkReduction>::generate_random_parameters_with_reduction(
                    circuit.clone(),
                    rng,
                )
                .expect("setup should succeed");

            let proof = Groth16::<Bls12_377, LibsnarkReduction>::prove(&pk, circuit, rng)
                .expect("prove should succeed");

            let public_inputs = vec![x * x];
            (pk, proof, public_inputs)
        }

        #[test]
        fn batch_single_valid_proof() {
            let mut rng = ChaCha20Rng::seed_from_u64(0);
            let x = Fr::from(7u64);
            let (pk, proof, public_inputs) = setup_and_prove(&mut rng, x);
            let pvk: PreparedVerifyingKey<Bls12_377> = pk.vk.into();

            // Individual verify
            let individual_ok = Groth16::<Bls12_377, LibsnarkReduction>::verify_with_processed_vk(
                &pvk,
                &public_inputs,
                &proof,
            )
            .expect("verify should not error");

            assert!(individual_ok, "individual verify should pass");

            // Batch verify
            let items = vec![BatchItem {
                proof,
                public_inputs,
            }];
            assert!(
                batch_verify(&pvk, &items).is_ok(),
                "batch verify should pass"
            );
        }

        #[test]
        fn batch_multiple_valid_proofs() {
            let mut rng = ChaCha20Rng::seed_from_u64(0);
            let witnesses = [
                Fr::from(3u64),
                Fr::from(5u64),
                Fr::from(11u64),
                Fr::from(17u64),
            ];

            // All proofs must use the same circuit/VK, so setup once.
            let setup_circuit = SquareCircuit {
                x: Some(witnesses[0]),
            };
            let pk =
                Groth16::<Bls12_377, LibsnarkReduction>::generate_random_parameters_with_reduction(
                    setup_circuit,
                    &mut rng,
                )
                .expect("setup should succeed");
            let pvk: PreparedVerifyingKey<Bls12_377> = pk.vk.clone().into();

            let items: Vec<BatchItem> = witnesses
                .iter()
                .map(|&x| {
                    let circuit = SquareCircuit { x: Some(x) };
                    let proof =
                        Groth16::<Bls12_377, LibsnarkReduction>::prove(&pk, circuit, &mut rng)
                            .expect("prove should succeed");
                    let public_inputs = vec![x * x];

                    // Differential: individual verify must also pass
                    let ok = Groth16::<Bls12_377, LibsnarkReduction>::verify_with_processed_vk(
                        &pvk,
                        &public_inputs,
                        &proof,
                    )
                    .expect("verify should not error");
                    assert!(ok, "individual verify should pass");

                    BatchItem {
                        proof,
                        public_inputs,
                    }
                })
                .collect();

            assert!(
                batch_verify(&pvk, &items).is_ok(),
                "batch verify should pass for 4 valid proofs"
            );
        }

        #[test]
        fn verified_capabilities_bind_exact_key_item_and_order() {
            let mut rng = ChaCha20Rng::seed_from_u64(19);
            let setup_circuit = SquareCircuit {
                x: Some(Fr::from(1u64)),
            };
            let pk =
                Groth16::<Bls12_377, LibsnarkReduction>::generate_random_parameters_with_reduction(
                    setup_circuit,
                    &mut rng,
                )
                .expect("setup should succeed");
            let items = [Fr::from(3u64), Fr::from(5u64)]
                .into_iter()
                .map(|x| BatchItem {
                    proof: Groth16::<Bls12_377, LibsnarkReduction>::prove(
                        &pk,
                        SquareCircuit { x: Some(x) },
                        &mut rng,
                    )
                    .expect("prove should succeed"),
                    public_inputs: vec![x * x],
                })
                .collect::<Vec<_>>();
            let expected = items.clone();

            let capabilities = items
                .into_iter()
                .map(|item| VerifiedBatchItem {
                    key: DeployedProofKey::Transfer,
                    item: Arc::new(item),
                })
                .collect::<Vec<_>>();
            assert_eq!(capabilities.len(), expected.len());
            for (capability, item) in capabilities.iter().zip(&expected) {
                capability
                    .ensure_binds(DeployedProofKey::Transfer, item)
                    .expect("capability order must match input order");
            }
            assert!(matches!(
                capabilities[0].ensure_binds(DeployedProofKey::Transfer, &expected[1]),
                Err(BatchVerifyError::VerifiedItemMismatch)
            ));

            let mut wrong_item = expected[0].clone();
            wrong_item.public_inputs[0] += Fr::from(1u64);
            assert!(matches!(
                capabilities[0].ensure_binds(DeployedProofKey::Transfer, &wrong_item),
                Err(BatchVerifyError::VerifiedItemMismatch)
            ));

            for wrong_key in DeployedProofKey::ALL
                .into_iter()
                .filter(|key| *key != DeployedProofKey::Transfer)
            {
                assert!(matches!(
                    capabilities[0].ensure_binds(wrong_key, &expected[0]),
                    Err(BatchVerifyError::VerifiedItemMismatch)
                ));
            }
        }

        #[test]
        fn verified_capability_retains_only_one_item_and_compact_key_id() {
            assert!(
                std::mem::size_of::<VerifiedBatchItem>() <= 2 * std::mem::size_of::<usize>(),
                "capability must not retain a batch vector or prepared verification key"
            );
        }

        #[test]
        fn batch_rejects_mutated_proof() {
            let mut rng = ChaCha20Rng::seed_from_u64(0);
            let x = Fr::from(7u64);
            let (pk, proof, public_inputs) = setup_and_prove(&mut rng, x);
            let pvk: PreparedVerifyingKey<Bls12_377> = pk.vk.into();

            // Mutate proof: use a different A point
            let mut bad_proof = proof.clone();
            bad_proof.a = G1Affine::generator();

            // Individual should reject
            let individual_ok = Groth16::<Bls12_377, LibsnarkReduction>::verify_with_processed_vk(
                &pvk,
                &public_inputs,
                &bad_proof,
            )
            .unwrap_or(false);
            assert!(
                !individual_ok,
                "individual verify should reject mutated proof"
            );

            // Batch should reject
            let items = vec![BatchItem {
                proof: bad_proof,
                public_inputs,
            }];
            assert!(
                batch_verify(&pvk, &items).is_err(),
                "batch verify should reject mutated proof"
            );
            assert!(
                verify_each(&pvk, &items).is_err(),
                "independent verification should reject mutated proof"
            );
        }

        #[test]
        fn batch_rejects_wrong_public_inputs() {
            let mut rng = ChaCha20Rng::seed_from_u64(0);
            let x = Fr::from(7u64);
            let (pk, proof, _) = setup_and_prove(&mut rng, x);
            let pvk: PreparedVerifyingKey<Bls12_377> = pk.vk.into();

            let wrong_inputs = vec![Fr::from(999u64)]; // x^2 = 49, not 999

            // Both should reject
            let individual_ok = Groth16::<Bls12_377, LibsnarkReduction>::verify_with_processed_vk(
                &pvk,
                &wrong_inputs,
                &proof,
            )
            .unwrap_or(false);
            assert!(
                !individual_ok,
                "individual verify should reject wrong inputs"
            );

            let items = vec![BatchItem {
                proof,
                public_inputs: wrong_inputs,
            }];
            assert!(
                batch_verify(&pvk, &items).is_err(),
                "batch verify should reject wrong inputs"
            );
            assert!(
                verify_each(&pvk, &items).is_err(),
                "independent verification should reject wrong inputs"
            );
        }

        #[test]
        fn batch_identifies_failing_proof_in_mixed_batch() {
            let mut rng = ChaCha20Rng::seed_from_u64(0);
            let setup_circuit = SquareCircuit {
                x: Some(Fr::from(1u64)),
            };
            let pk =
                Groth16::<Bls12_377, LibsnarkReduction>::generate_random_parameters_with_reduction(
                    setup_circuit,
                    &mut rng,
                )
                .expect("setup should succeed");
            let pvk: PreparedVerifyingKey<Bls12_377> = pk.vk.clone().into();

            let witnesses = [Fr::from(3u64), Fr::from(5u64), Fr::from(7u64)];
            let mut items: Vec<BatchItem> = witnesses
                .iter()
                .map(|&x| {
                    let circuit = SquareCircuit { x: Some(x) };
                    let proof =
                        Groth16::<Bls12_377, LibsnarkReduction>::prove(&pk, circuit, &mut rng)
                            .expect("prove should succeed");
                    BatchItem {
                        proof,
                        public_inputs: vec![x * x],
                    }
                })
                .collect();

            // Mutate the middle proof
            items[1].proof.a = G1Affine::generator();

            // Batch should reject
            assert!(batch_verify(&pvk, &items).is_err());

            // Diagnostic fallback should identify index 1
            let failing = identify_failing_proofs(&pvk, &items);
            assert_eq!(failing, vec![1]);
        }

        #[test]
        fn batch_empty_returns_ok() {
            let mut rng = ChaCha20Rng::seed_from_u64(0);
            let setup_circuit = SquareCircuit {
                x: Some(Fr::from(1u64)),
            };
            let pk =
                Groth16::<Bls12_377, LibsnarkReduction>::generate_random_parameters_with_reduction(
                    setup_circuit,
                    &mut rng,
                )
                .expect("setup should succeed");
            let pvk: PreparedVerifyingKey<Bls12_377> = pk.vk.into();

            assert!(batch_verify(&pvk, &[]).is_ok());
            assert!(verify_each(&pvk, &[]).is_ok());
        }

        #[test]
        fn batch_rejects_wrong_input_length() {
            let mut rng = ChaCha20Rng::seed_from_u64(0);
            let x = Fr::from(7u64);
            let (pk, proof, _) = setup_and_prove(&mut rng, x);
            let pvk: PreparedVerifyingKey<Bls12_377> = pk.vk.into();

            // Wrong length: 0 inputs instead of 1
            let items = vec![BatchItem {
                proof,
                public_inputs: vec![],
            }];
            match batch_verify(&pvk, &items) {
                Err(BatchVerifyError::InputLengthMismatch {
                    index: 0,
                    expected: 1,
                    got: 0,
                }) => {}
                other => panic!("expected InputLengthMismatch, got {:?}", other.err()),
            }
            match verify_each(&pvk, &items) {
                Err(BatchVerifyError::InputLengthMismatch {
                    index: 0,
                    expected: 1,
                    got: 0,
                }) => {}
                other => panic!("expected InputLengthMismatch, got {:?}", other.err()),
            }
        }

        #[test]
        fn verification_rejects_key_without_input_accumulator() {
            let mut rng = ChaCha20Rng::seed_from_u64(0);
            let x = Fr::from(7u64);
            let (pk, proof, public_inputs) = setup_and_prove(&mut rng, x);
            let mut pvk: PreparedVerifyingKey<Bls12_377> = pk.vk.into();
            pvk.vk.gamma_abc_g1.clear();
            let items = vec![BatchItem {
                proof,
                public_inputs,
            }];

            assert!(matches!(
                batch_verify(&pvk, &items),
                Err(BatchVerifyError::BatchFailed)
            ));
            assert!(matches!(
                verify_each(&pvk, &items),
                Err(BatchVerifyError::BatchFailed)
            ));
        }
    }
}
