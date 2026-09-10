use shieldd_sdk_disclosure::*;
use shieldd_sdk_keys::{keys::SpendKey, symmetric::WrappedMemoKey, PayloadKey};
use shieldd_sdk_shielded_pool::{Note, RecoveryCommitment, Rseed};
use shieldd_sdk_transaction::memo::{MemoCiphertext, MemoPlaintext};

fn fixture() -> DisclosureWitness {
    fixture_with_seed(9)
}

fn fixture_with_seed(seed: u8) -> DisclosureWitness {
    let fvk = SpendKey::try_from(shieldd_sdk_keys::keys::SpendKeyBytes([7u8; 32]))
        .unwrap()
        .full_viewing_key()
        .clone();
    let address = fvk.payment_address(0u32.into());
    let note = Note::from_parts(
        address.clone(),
        shieldd_sdk_asset::Value {
            amount: 42u64.into(),
            asset_id: shieldd_sdk_asset::asset::REGISTRY
                .parse_denom("ubrl")
                .unwrap()
                .id(),
        },
        Rseed([seed; 32]),
        RecoveryCommitment::unavailable(),
    )
    .unwrap();
    let key = PayloadKey::random_key(&mut rand::rngs::OsRng);
    let memo = MemoPlaintext::new(address, "PRIVATE-MEMO".into()).unwrap();
    let ciphertext = MemoCiphertext::encrypt(key.clone(), &memo).unwrap();
    let wrapped = WrappedMemoKey::encrypt(
        &key,
        note.ephemeral_secret_key(),
        note.transmission_key(),
        &note.diversified_generator(),
    );
    let reference = OutputRef {
        transaction_id: "ab".repeat(32),
        height: 5,
        action: ActionRef::Body(0),
        output: 0,
    };
    let public = PublicOutput {
        reference: reference.clone(),
        commitment: hex::encode(note.commit().0.to_bytes()),
        ephemeral_key: note.ephemeral_public_key().0.to_vec(),
        encrypted_note: note.encrypt().0.to_vec(),
        wrapped_memo_key: wrapped.0.to_vec(),
        memo_ciphertext: Some(ciphertext.0.to_vec()),
        spend_verification_key: None,
    };
    DisclosureWitness {
        request: DisclosureRequest {
            version: VERSION,
            chain_id: "test-chain".into(),
            recipient: None,
            challenge: None,
            outputs: vec![OutputClaim {
                reference,
                amount: false,
                asset: false,
                recipient: false,
                predicate: Some(AmountPredicate::GreaterThan("41".into())),
                memo: false,
                spending_control: false,
            }],
            total: None,
        },
        outputs: vec![OutputWitness {
            public,
            note: note.to_bytes().to_vec(),
            control_signature: None,
        }],
    }
}

#[test]
fn predicates_distinguish_false_and_unavailable() {
    let mut w = fixture();
    assert_eq!(
        evaluate(&w).unwrap().outputs[0].predicate_result,
        Some(true)
    );
    w.request.outputs[0].predicate = Some(AmountPredicate::GreaterThan("42".into()));
    assert_eq!(
        evaluate(&w).unwrap().outputs[0].predicate_result,
        Some(false)
    );
    w.request.outputs[0].predicate = None;
    assert_eq!(evaluate(&w).unwrap().outputs[0].predicate_result, None);
    for (p, expected) in [
        (AmountPredicate::AtMost("42".into()), true),
        (AmountPredicate::LessThan("42".into()), false),
        (
            AmountPredicate::InclusiveRange {
                lower: "42".into(),
                upper: "42".into(),
            },
            true,
        ),
    ] {
        assert_eq!(predicate_result(42, &p).unwrap(), expected)
    }
    assert!(predicate_result(42, &AmountPredicate::AtLeast("042".into())).is_err());
    assert!(predicate_result(
        42,
        &AmountPredicate::InclusiveRange {
            lower: "43".into(),
            upper: "42".into()
        }
    )
    .is_err());
    assert!(predicate_result(u128::MAX, &AmountPredicate::AtMost(u128::MAX.to_string())).unwrap());
}
#[test]
fn selection_and_opening_integrity() {
    let w = fixture();
    let mut bad = w.clone();
    bad.request.outputs.push(bad.request.outputs[0].clone());
    bad.request.outputs[1].reference.height += 1;
    assert!(validate_request(&bad.request).is_err());
    let mut bad = w.clone();
    bad.outputs[0].public.commitment = "00".repeat(32);
    assert!(evaluate(&bad).is_err());
    let mut bad = w.clone();
    bad.request.outputs[0].reference.output += 1;
    assert!(evaluate(&bad).is_err());
    let mut bad = w.clone();
    bad.request.version += 1;
    assert!(evaluate(&bad).is_err());
    let s = evaluate(&w).unwrap();
    assert!(
        s.outputs[0].amount.is_none()
            && s.outputs[0].recipient.is_none()
            && s.outputs[0].memo.is_none()
    );
    assert!(!serde_json::to_string(&s).unwrap().contains("PRIVATE-MEMO"));
}
#[test]
fn totals_can_span_transactions_but_reject_duplicates() {
    let mut w = fixture();
    let mut c = w.request.outputs[0].clone();
    c.reference.transaction_id = "cd".repeat(32);
    let mut o = w.outputs[0].clone();
    o.public.reference = c.reference.clone();
    w.outputs.push(o);
    w.request.outputs.push(c);
    w.request.total = Some(TotalClaim {
        reveal: true,
        predicate: Some(AmountPredicate::GreaterThan("84".into())),
    });
    let t = evaluate(&w).unwrap().selected_output_total.unwrap();
    assert_eq!(t.amount.as_deref(), Some("84"));
    assert_eq!(t.predicate_result, Some(false));
}
#[cfg(feature = "proof")]
#[test]
fn full_openings_and_payload_exports() {
    let mut w = fixture();
    assert!(export_openings(&w).is_err());
    let c = &mut w.request.outputs[0];
    c.amount = true;
    c.asset = true;
    c.recipient = true;
    let p = export_openings(&w).unwrap();
    assert!(verify(&p).unwrap().cryptography_verified);
    assert!(!verify(&p).unwrap().fully_verified());
    let bytes = serde_json::to_string(&p).unwrap();
    assert!(!bytes.contains("note_seed") && !bytes.contains("PRIVATE-MEMO"));
    let mut bad = p.clone();
    bad.statement.outputs[0].amount = Some("43".into());
    assert!(verify(&bad).is_err());
    let mut bad = p.clone();
    if let Evidence::Openings { openings, .. } = &mut bad.evidence {
        openings[0].blinding = "00".repeat(32)
    }
    assert!(verify(&bad).is_err());
    w.request.outputs[0].memo = true;
    let p = export_payload_keys(&w).unwrap();
    assert_eq!(
        p.statement.outputs[0].memo.as_ref().unwrap().text,
        "PRIVATE-MEMO"
    );
    assert!(inspect(&p).unwrap().grants_transaction_wide_memo_decryption);
    let mut bad = p.clone();
    bad.statement.outputs[0]
        .public
        .memo_ciphertext
        .as_mut()
        .unwrap()[0] ^= 1;
    assert!(verify(&bad).is_err());
}
#[cfg(feature = "prover")]
#[test]
#[ignore = "requires local development circuit artifacts and backend"]
fn real_proofs() {
    let mut w = fixture();
    for count in [1, 8, 32] {
        while w.outputs.len() < count {
            let mut c = w.request.outputs[0].clone();
            c.reference.transaction_id = format!("{:064x}", w.outputs.len());
            let mut o = fixture_with_seed(10 + w.outputs.len() as u8)
                .outputs
                .remove(0);
            o.public.reference = c.reference.clone();
            w.outputs.push(o);
            w.request.outputs.push(c);
        }
        for claim in &mut w.request.outputs {
            claim.amount = count == 1;
            claim.recipient = count == 1;
        }
        w.request.total = Some(TotalClaim {
            reveal: false,
            predicate: Some(AmountPredicate::GreaterThan((42 * count).to_string())),
        });
        let started = std::time::Instant::now();
        let p = prove(&w).unwrap();
        eprintln!(
            "outputs={count} elapsed={:?} bytes={}",
            started.elapsed(),
            serde_json::to_vec(&p).unwrap().len()
        );
        assert!(verify(&p).unwrap().cryptography_verified);
        assert_eq!(
            p.statement
                .selected_output_total
                .as_ref()
                .unwrap()
                .predicate_result,
            Some(false)
        );
        let mut bad = p.clone();
        bad.version += 1;
        assert!(verify(&bad).is_err());
        let mut bad = p.clone();
        if let Evidence::Groth16 { circuit, .. } = &mut bad.evidence {
            circuit.push('x');
        }
        assert!(verify(&bad).is_err());
        let mut bad = p.clone();
        if let Evidence::Groth16 {
            verification_key_sha256,
            ..
        } = &mut bad.evidence
        {
            *verification_key_sha256 = "00".repeat(32);
        }
        assert!(verify(&bad).is_err());
        let mut bad = p.clone();
        bad.statement.request.chain_id.push('x');
        assert!(verify(&bad).is_err());
        let mut bad = p.clone();
        bad.statement.outputs[0].predicate_result = Some(false);
        assert!(verify(&bad).is_err());
        let mut bad = p.clone();
        if let Evidence::Groth16 { proof, .. } = &mut bad.evidence {
            proof[0] ^= 1
        }
        assert!(verify(&bad).is_err());
    }
}

#[test]
fn control_signatures_bind_fresh_requests() {
    use decaf377_rdsa::{SigningKey, SpendAuth, VerificationKey};
    let mut w = fixture();
    w.request.challenge = Some("fresh-challenge".into());
    w.request.recipient = Some("auditor".into());
    w.request.outputs[0].spending_control = true;
    let key = SigningKey::<SpendAuth>::from(decaf377::Fr::from(5u64));
    let vk = VerificationKey::from(&key);
    w.outputs[0].public.spend_verification_key = Some(vk.to_bytes().to_vec());
    let signature: [u8; 64] = key
        .sign(rand::rngs::OsRng, &control_message(&w.request).unwrap())
        .into();
    w.outputs[0].control_signature = Some(signature.to_vec());
    assert!(evaluate(&w).is_ok());
    let mut bad = w.clone();
    bad.request.challenge = Some("another-challenge".into());
    assert!(evaluate(&bad).is_err());
    let mut bad = w.clone();
    bad.request.recipient = Some("another-auditor".into());
    assert!(evaluate(&bad).is_err());
    let mut bad = w.clone();
    bad.outputs[0].control_signature = None;
    assert!(evaluate(&bad).is_err());
    let mut bad = w.clone();
    bad.outputs[0].public.spend_verification_key = Some(vec![0; 32]);
    assert!(evaluate(&bad).is_err());
}

#[test]
fn sum_overflow_is_rejected_without_limiting_independent_notes() {
    let mut w = fixture();
    let old = Note::try_from(w.outputs[0].note.as_slice()).unwrap();
    let note = Note::from_parts(
        old.address(),
        shieldd_sdk_asset::Value {
            amount: u128::MAX.into(),
            asset_id: old.asset_id(),
        },
        Rseed([11; 32]),
        RecoveryCommitment::unavailable(),
    )
    .unwrap();
    w.outputs[0].note = note.to_bytes().to_vec();
    w.outputs[0].public.commitment = hex::encode(note.commit().0.to_bytes());
    let mut claim = w.request.outputs[0].clone();
    claim.reference.transaction_id = "cd".repeat(32);
    let mut output = w.outputs[0].clone();
    output.public.reference = claim.reference.clone();
    w.request.outputs.push(claim);
    w.outputs.push(output);
    assert!(evaluate(&w).is_ok());
    w.request.total = Some(TotalClaim {
        reveal: true,
        predicate: None,
    });
    assert!(evaluate(&w).is_err());
}

#[cfg(all(feature = "prover", not(debug_assertions)))]
#[test]
#[ignore = "requires configured development artifacts"]
fn release_rejects_development_artifacts() {
    let witness = fixture();
    let error = prove(&witness).unwrap_err();
    assert!(error
        .to_string()
        .contains("no approved production disclosure setup"));
    let package = DisclosurePackage {
        version: VERSION,
        statement: evaluate(&witness).unwrap(),
        evidence: Evidence::Groth16 {
            circuit: CIRCUIT_ID.into(),
            verification_key_sha256: "00".repeat(32),
            proof: vec![],
            control_signatures: vec![None],
        },
    };
    let error = verify(&package).unwrap_err();
    assert!(error
        .to_string()
        .contains("no approved production disclosure setup"));
}
