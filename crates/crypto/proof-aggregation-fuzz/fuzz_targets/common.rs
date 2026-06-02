#![allow(dead_code, unused_imports)]

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use ark_ff::{UniformRand, Zero};
use ark_groth16::{r1cs_to_qap::LibsnarkReduction, Groth16, PreparedVerifyingKey};
use ark_r1cs_std::{alloc::AllocVar, eq::EqGadget, fields::fp::FpVar};
use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
use ark_snark::SNARK;
use decaf377::{Bls12_377, Fq, Fr};
use decaf377_rdsa as rdsa;
use penumbra_sdk_app::stateless_cache::TxArtifact;
use penumbra_sdk_fee::Fee;
use penumbra_sdk_proof_aggregation::{
    aggregate_family, pad_items_to_power_of_two, srs_id, AggregateBundle, AggregatePreflightInput,
    AggregateStatement, DevSrs, FamilyAggregate, ProofFamilyId, AGGREGATE_PROTOCOL_VERSION,
};
use penumbra_sdk_proof_params::batch::BatchItem;
use penumbra_sdk_transaction::{Action, DetectionData, Transaction, TransactionParameters};
use penumbra_sdk_txhash::AuthorizingData;
use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};

struct SquareCircuit {
    x: Option<Fq>,
}

impl ConstraintSynthesizer<Fq> for SquareCircuit {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fq>) -> Result<(), SynthesisError> {
        let x = FpVar::new_witness(cs.clone(), || {
            self.x.ok_or(SynthesisError::AssignmentMissing)
        })?;
        let x_sq = &x * &x;
        let public = FpVar::new_input(cs, || {
            let x = self.x.ok_or(SynthesisError::AssignmentMissing)?;
            Ok(x * x)
        })?;
        x_sq.enforce_equal(&public)?;
        Ok(())
    }
}

pub struct Fixture {
    pub pvk: PreparedVerifyingKey<Bls12_377>,
    pub statement: AggregateStatement,
    pub srs: DevSrs,
    pub aggregate_proof: Vec<u8>,
}

static FIXTURE: OnceLock<Fixture> = OnceLock::new();

pub fn fixture() -> &'static Fixture {
    FIXTURE.get_or_init(|| {
        let mut rng = ChaCha20Rng::seed_from_u64(0x5eed);
        let pk =
            Groth16::<Bls12_377, LibsnarkReduction>::generate_random_parameters_with_reduction(
                SquareCircuit {
                    x: Some(Fq::from(1u64)),
                },
                &mut rng,
            )
            .expect("fuzz fixture setup");
        let pvk = PreparedVerifyingKey::from(pk.vk.clone());
        let x = Fq::rand(&mut rng);
        let proof = Groth16::<Bls12_377, LibsnarkReduction>::prove(
            &pk,
            SquareCircuit { x: Some(x) },
            &mut rng,
        )
        .expect("fuzz fixture proof");
        let item = BatchItem {
            proof,
            public_inputs: vec![x * x],
        };
        let srs = DevSrs::default();
        let items = pad_items_to_power_of_two(&[item], srs.max_padded_count as usize)
            .expect("fuzz fixture padding");
        let rows = items
            .iter()
            .map(|item| item.public_inputs.clone())
            .collect::<Vec<_>>();
        let statement = AggregateStatement::new(
            AGGREGATE_PROTOCOL_VERSION,
            ProofFamilyId::Transfer,
            srs_id(&srs),
            &pvk,
            1,
            &rows,
        )
        .expect("fuzz fixture statement");
        let aggregate_proof =
            aggregate_family(&statement, &pvk, &items, &srs).expect("fuzz fixture aggregate");
        Fixture {
            pvk,
            statement,
            srs,
            aggregate_proof,
        }
    })
}

pub fn preflight_input<'a>(aggregate_proof_bytes: &'a [u8]) -> AggregatePreflightInput<'a> {
    let fixture = fixture();
    AggregatePreflightInput {
        statement: &fixture.statement,
        pvk: &fixture.pvk,
        aggregate_proof_bytes,
        srs: &fixture.srs,
    }
}

pub fn bundle_from_bytes(data: &[u8]) -> AggregateBundle {
    let version = u32_from(data, 0);
    let real_count = u32_from(data, 4);
    let padded_count = u32_from(data, 8);
    let srs_len = data.get(12).copied().unwrap_or(0) as usize % 65;
    let proof_start = 13usize.saturating_add(srs_len).min(data.len());
    let srs_id = data
        .get(13..proof_start)
        .map(ToOwned::to_owned)
        .unwrap_or_default();
    let aggregate_proof = data.get(proof_start..).unwrap_or_default().to_vec();
    AggregateBundle {
        version,
        srs_id,
        families: vec![FamilyAggregate {
            family_id: ProofFamilyId::Transfer,
            real_count,
            padded_count,
            aggregate_proof,
        }],
    }
}

pub fn aggregate_bundle_tx(data: &[u8]) -> Transaction {
    let mode = data.first().copied().unwrap_or(0) % 6;
    let bundle = bundle_from_bytes(data.get(1..).unwrap_or_default());
    let mut tx = Transaction {
        transaction_body: penumbra_sdk_transaction::TransactionBody {
            actions: vec![Action::AggregateBundle(bundle.clone())],
            transaction_parameters: TransactionParameters {
                expiry_height: 0,
                chain_id: "penumbra-fuzz".to_owned(),
                fee: Fee::default(),
            },
            fee_funding: None,
            detection_data: None,
            memo: None,
        },
        binding_sig: [0; 64].into(),
        anchor: penumbra_sdk_tct::Root(penumbra_sdk_tct::structure::Hash::zero()),
    };

    match mode {
        0 => tx.transaction_body.actions.clear(),
        1 => {
            tx.transaction_body.detection_data = Some(DetectionData { fmd_clues: vec![] });
        }
        2 => {
            tx.transaction_body.transaction_parameters.fee =
                Fee::from_staking_token_amount(1u64.into());
        }
        3 => tx
            .transaction_body
            .actions
            .push(Action::AggregateBundle(bundle)),
        _ => {
            let binding_signing_key = rdsa::SigningKey::from(Fr::zero());
            let auth_hash = tx.transaction_body.auth_hash();
            tx.binding_sig = binding_signing_key.sign_deterministic(auth_hash.as_bytes());
        }
    }
    tx
}

pub fn artifact_from_tx(tx: Arc<Transaction>, total_proof_count: usize) -> Arc<TxArtifact> {
    Arc::new(TxArtifact {
        tx,
        proof_items: BTreeMap::new(),
        spend_nullifiers: Vec::new(),
        anchor_pairs: Vec::new(),
        total_proof_count,
        historical_validation: None,
    })
}

fn u32_from(data: &[u8], start: usize) -> u32 {
    let mut bytes = [0u8; 4];
    if let Some(slice) = data.get(start..start.saturating_add(4)) {
        bytes[..slice.len().min(4)].copy_from_slice(&slice[..slice.len().min(4)]);
    }
    u32::from_le_bytes(bytes)
}
