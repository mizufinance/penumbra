use shieldd_sdk_disclosure::*;
use shieldd_sdk_keys::{keys::SpendKey, symmetric::WrappedMemoKey, PayloadKey};
use shieldd_sdk_shielded_pool::{Note, RecoveryCommitment, Rseed};
use shieldd_sdk_transaction::memo::{MemoCiphertext, MemoPlaintext};

fn fixture() -> DisclosureWitness {
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
        Rseed([9; 32]),
        RecoveryCommitment::unavailable(),
    )
    .unwrap();
    let metadata = MetadataOpening {
        salt: [3; 32],
        document: MetadataDocument {
            entries: vec![
                MetadataEntry {
                    action: ActionRef::Body(0),
                    output: 0,
                    name: "invoice".into(),
                    value: "INV-42".into(),
                },
                MetadataEntry {
                    action: ActionRef::Body(0),
                    output: 0,
                    name: "secret".into(),
                    value: "CONFIDENTIAL-CONTEXT".into(),
                },
            ],
        },
    };
    let key = PayloadKey::random_key(&mut rand::rngs::OsRng);
    let memo = MemoPlaintext::new(address, metadata_commitment(&metadata).unwrap()).unwrap();
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
                metadata_fields: vec!["invoice".into()],
                spending_control: false,
            }],
            total: None,
        },
        outputs: vec![OutputWitness {
            public,
            note: note.to_bytes().to_vec(),
            metadata: Some(metadata),
            control_signature: None,
        }],
    }
}

#[test]
fn selective_claim_hides_note_and_memo() {
    let w = fixture();
    let s = evaluate(&w).unwrap();
    assert!(s.outputs[0].amount.is_none());
    assert!(s.outputs[0].recipient.is_none());
    assert!(s.outputs[0].memo.is_none());
    assert_eq!(s.outputs[0].metadata[0].value, "INV-42");
    let serialized = serde_json::to_string(&s).unwrap();
    assert!(!serialized.contains("CONFIDENTIAL-CONTEXT"));
    assert!(!serialized.contains("rseed"));
    assert!(!serialized.contains("payload_key"));
}

#[test]
fn note_facts_are_bound_by_the_commitment_without_decryption() {
    let mut witness = fixture();
    witness.request.outputs[0].metadata_fields.clear();
    witness.request.outputs[0].amount = true;
    witness.request.outputs[0].asset = true;
    witness.request.outputs[0].recipient = true;
    witness.outputs[0].public.encrypted_note[0] ^= 1;
    let statement = evaluate(&witness).unwrap();
    assert_eq!(statement.outputs[0].amount.as_deref(), Some("42"));
    assert!(statement.outputs[0].recipient.is_some());
    let mut note = witness.outputs[0].note.clone();
    note[48] ^= 1;
    witness.outputs[0].note = note;
    assert!(evaluate(&witness).is_err());
}

#[test]
fn hidden_metadata_does_not_claim_a_valid_memo_return_address() {
    use shieldd_sdk_keys::{address::ADDRESS_LEN_BYTES, symmetric::PayloadKind, Address};
    let mut witness = fixture();
    let output = &mut witness.outputs[0];
    let note = Note::try_from(output.note.as_slice()).unwrap();
    let key = WrappedMemoKey::try_from(output.public.wrapped_memo_key.as_slice())
        .unwrap()
        .decrypt_outgoing(&payload_key(&note).unwrap())
        .unwrap();
    let ciphertext = MemoCiphertext(
        output
            .public
            .memo_ciphertext
            .as_ref()
            .unwrap()
            .as_slice()
            .try_into()
            .unwrap(),
    );
    let mut plaintext = MemoCiphertext::decrypt_bytes(&key, ciphertext).unwrap();
    let invalid = (0..=255u8)
        .map(|b| [b; ADDRESS_LEN_BYTES])
        .find(|bytes| Address::try_from(bytes.as_slice()).is_err())
        .unwrap();
    plaintext[..ADDRESS_LEN_BYTES].copy_from_slice(&invalid);
    output.public.memo_ciphertext = Some(key.encrypt(plaintext.to_vec(), PayloadKind::Memo));
    assert!(evaluate(&witness).is_ok());
    witness.request.outputs[0].memo = true;
    assert!(evaluate(&witness).is_err());
    witness.request.outputs[0].memo = false;
    witness.outputs[0].public.memo_ciphertext.as_mut().unwrap()[0] ^= 1;
    assert!(evaluate(&witness).is_err());
}

#[test]
fn tampered_claims_and_openings_fail() {
    let base = fixture();
    let mut w = base.clone();
    w.request.version += 1;
    assert!(evaluate(&w).is_err());
    let mut w = base.clone();
    w.outputs[0].public.commitment = "00".repeat(32);
    assert!(evaluate(&w).is_err());
    let mut w = base.clone();
    w.outputs[0].public.memo_ciphertext.as_mut().unwrap()[0] ^= 1;
    assert!(evaluate(&w).is_err());
    let mut w = base.clone();
    w.outputs[0].metadata.as_mut().unwrap().salt[0] ^= 1;
    assert!(evaluate(&w).is_err());
    let mut w = base.clone();
    w.request.outputs[0].predicate = Some(AmountPredicate::GreaterThan("42".into()));
    assert!(evaluate(&w).is_err());
    let mut w = base.clone();
    w.request.outputs[0].predicate = Some(AmountPredicate::AtLeast("042".into()));
    assert!(evaluate(&w).is_err());
    let mut w = base.clone();
    w.request.outputs[0].reference.height += 1;
    assert!(evaluate(&w).is_err());
    let mut w = base.clone();
    w.request.outputs.push(w.request.outputs[0].clone());
    w.outputs.push(w.outputs[0].clone());
    assert!(evaluate(&w).is_err());
    let mut w = base.clone();
    w.request.outputs[0].metadata_fields = vec!["absent".into()];
    assert!(evaluate(&w).is_err());
    let mut w = base;
    w.request.outputs[0].predicate = Some(AmountPredicate::InclusiveRange {
        lower: "42".into(),
        upper: "42".into(),
    });
    assert!(evaluate(&w).is_ok());
}

#[test]
fn note_ciphertext_and_ephemeral_key_are_bound() {
    let base = fixture();
    for index in [0, 31] {
        let mut w = base.clone();
        w.outputs[0].public.ephemeral_key[index] ^= 1;
        assert!(evaluate(&w).is_err());
    }
    for index in [0, 48, 96, base.outputs[0].public.encrypted_note.len() - 1] {
        let mut w = base.clone();
        w.outputs[0].public.encrypted_note[index] ^= 1;
        assert!(evaluate(&w).is_err());
    }
    let mut w = base;
    w.outputs[0].public.encrypted_note.pop();
    assert!(evaluate(&w).is_err());
}

#[test]
fn memo_reveal_and_selected_total() {
    let mut w = fixture();
    w.request.outputs[0].memo = true;
    w.request.total = Some(TotalClaim {
        reveal: true,
        predicate: Some(AmountPredicate::AtMost("42".into())),
    });
    let s = evaluate(&w).unwrap();
    assert!(s.outputs[0].memo.is_some());
    assert_eq!(
        s.selected_output_total.unwrap().amount.as_deref(),
        Some("42")
    );
}

#[cfg(feature = "prover")]
#[test]
#[ignore = "generates a real RISC Zero note and hidden-memo proof"]
fn real_note_and_hidden_memo_proof() {
    use risc0_zkvm::{Executor, ExecutorEnv, ExternalProver};
    let witness = fixture();
    let encoded = serde_json::to_vec(&witness).unwrap();
    let env = ExecutorEnv::builder()
        .write_slice(&encoded)
        .segment_limit_po2(18)
        .build()
        .unwrap();
    let session = ExternalProver::new("local", "r0vm")
        .execute(env, shieldd_disclosure_methods::DISCLOSURE_GUEST_ELF)
        .unwrap();
    eprintln!(
        "guest execution: {} cycles, {} segments",
        session.cycles(),
        session.segments.len()
    );
    let start = std::time::Instant::now();
    let package = prove(&witness).unwrap();
    let encoded = serde_json::to_vec(&package).unwrap();
    eprintln!(
        "real disclosure proof: {:?}, {} package bytes",
        start.elapsed(),
        encoded.len()
    );
    if let Ok(path) = std::env::var("SHIELDD_DISCLOSURE_TEST_RECEIPT") {
        std::fs::write(path, &encoded).unwrap();
    }
    if let Evidence::ZkReceipt(bytes) = &package.evidence {
        let receipt: risc0_zkvm::Receipt = serde_json::from_slice(bytes).unwrap();
        assert!(
            receipt.verify([0u32; 8]).is_err(),
            "wrong guest identity accepted"
        );
    }
    assert!(verify(&package).unwrap().cryptography_verified);
    assert!(!verify(&package).unwrap().fully_verified());
    let mut changed = package.clone();
    changed.statement.request.chain_id = "other-chain".into();
    assert!(verify(&changed).is_err());
    let mut changed = package.clone();
    changed.statement.outputs[0].metadata[0].value = "forged".into();
    assert!(verify(&changed).is_err());
    let mut changed = package;
    changed.evidence = Evidence::ZkReceipt(vec![0]);
    assert!(verify(&changed).is_err());
}

#[test]
fn spending_control_rejects_replay_and_substituted_input() {
    use decaf377_rdsa::{SigningKey, SpendAuth, VerificationKey};
    let mut w = fixture();
    let key = SigningKey::<SpendAuth>::new(rand::rngs::OsRng);
    let vk: [u8; 32] = VerificationKey::from(&key).into();
    w.outputs[0].public.spend_verification_key = Some(vk.to_vec());
    w.request.outputs[0].spending_control = true;
    w.request.challenge = Some("fresh-recipient-challenge".into());
    w.request.recipient = Some("recipient-A".into());
    let signature: [u8; 64] = key
        .sign(rand::rngs::OsRng, &control_message(&w.request).unwrap())
        .into();
    w.outputs[0].control_signature = Some(signature.to_vec());
    assert!(evaluate(&w).is_ok());
    let mut changed = w.clone();
    changed.request.recipient = Some("recipient-B".into());
    assert!(evaluate(&changed).is_err());
    let mut changed = w.clone();
    changed.request.challenge = Some("new-challenge".into());
    assert!(evaluate(&changed).is_err());
    let mut changed = w.clone();
    changed.request.outputs[0].amount = true;
    assert!(evaluate(&changed).is_err());
    let mut changed = w.clone();
    changed.outputs[0].control_signature = None;
    assert!(evaluate(&changed).is_err());
    let other: [u8; 32] =
        VerificationKey::from(&SigningKey::<SpendAuth>::new(rand::rngs::OsRng)).into();
    w.outputs[0].public.spend_verification_key = Some(other.to_vec());
    assert!(evaluate(&w).is_err());
}

#[test]
fn missing_acceptance_and_wrong_chain_are_not_verified() {
    let statement = evaluate(&fixture()).unwrap();
    assert!(confirm_acceptance(&statement, "wrong-chain", &[]).is_err());
    assert!(confirm_acceptance(&statement, "test-chain", &[]).is_err());
    let empty = AcceptedBlock {
        height: 5,
        transactions: vec![],
    };
    assert!(confirm_acceptance(&statement, "test-chain", &[empty]).is_err());
}

#[test]
fn strict_and_inclusive_boundaries() {
    let mut w = fixture();
    for (predicate, valid) in [
        (AmountPredicate::LessThan("42".into()), false),
        (AmountPredicate::LessThan("43".into()), true),
        (AmountPredicate::AtLeast("42".into()), true),
        (AmountPredicate::AtMost("42".into()), true),
        (AmountPredicate::GreaterThan(u128::MAX.to_string()), false),
        (AmountPredicate::AtMost(u128::MAX.to_string()), true),
        (
            AmountPredicate::AtLeast("340282366920938463463374607431768211456".into()),
            false,
        ),
        (
            AmountPredicate::InclusiveRange {
                lower: "43".into(),
                upper: "42".into(),
            },
            false,
        ),
    ] {
        w.request.outputs[0].predicate = Some(predicate);
        assert_eq!(evaluate(&w).is_ok(), valid);
    }
}

#[cfg(feature = "proof")]
#[test]
fn payload_keys_are_an_explicit_decryption_capability() {
    let mut w = fixture();
    assert!(export_payload_keys(&w).is_err());
    w.request.outputs[0].metadata_fields.clear();
    let package = export_payload_keys(&w).unwrap();
    assert!(verify(&package).unwrap().cryptography_verified);
    assert!(
        inspect(&package)
            .unwrap()
            .grants_transaction_wide_memo_decryption
    );
    let mut changed = package.clone();
    changed.version += 1;
    assert!(verify(&changed).is_err());
    let mut changed = package;
    changed.statement.outputs[0].amount = Some("42".into());
    assert!(verify(&changed).is_err());
}

#[cfg(feature = "proof")]
#[test]
fn development_receipts_are_always_rejected() {
    use risc0_zkvm::{FakeReceipt, InnerReceipt, Receipt, ReceiptClaim};
    let statement = evaluate(&fixture()).unwrap();
    let journal = serde_json::to_vec(&statement).unwrap();
    let receipt = Receipt::new(
        InnerReceipt::Fake(FakeReceipt::new(ReceiptClaim::ok(
            [0u32; 8],
            journal.clone(),
        ))),
        journal,
    );
    assert!(
        !matches!(
            std::panic::catch_unwind(|| receipt.verify([0u32; 8])),
            Ok(Ok(()))
        ),
        "development receipt verification must be disabled"
    );
    let package = DisclosurePackage {
        version: VERSION,
        statement,
        evidence: Evidence::ZkReceipt(serde_json::to_vec(&receipt).unwrap()),
        context: vec![],
    };
    assert!(verify(&package).is_err());
}

#[test]
fn selected_totals_reject_mixed_assets_and_overflow() {
    fn with_second(amount: u128, asset: shieldd_sdk_asset::asset::Id) -> DisclosureWitness {
        let mut w = fixture();
        w.request.outputs[0].metadata_fields.clear();
        w.request.outputs[0].predicate = None;
        w.request.outputs.push(w.request.outputs[0].clone());
        w.outputs.push(w.outputs[0].clone());
        w.request.outputs[1].reference.output = 1;
        w.outputs[1].public.reference.output = 1;
        let first = Note::try_from(w.outputs[0].note.as_slice()).unwrap();
        let note = Note::from_parts(
            first.address(),
            shieldd_sdk_asset::Value {
                amount: amount.into(),
                asset_id: asset,
            },
            Rseed([11; 32]),
            RecoveryCommitment::unavailable(),
        )
        .unwrap();
        w.outputs[1].note = note.to_bytes().to_vec();
        w.outputs[1].public.commitment = hex::encode(note.commit().0.to_bytes());
        w.outputs[1].public.ephemeral_key = note.ephemeral_public_key().0.to_vec();
        w.outputs[1].public.encrypted_note = note.encrypt().0.to_vec();
        w.request.total = Some(TotalClaim {
            reveal: true,
            predicate: None,
        });
        w
    }
    let asset = Note::try_from(fixture().outputs[0].note.as_slice())
        .unwrap()
        .asset_id();
    let w = with_second(42, asset);
    assert_eq!(
        evaluate(&w)
            .unwrap()
            .selected_output_total
            .unwrap()
            .amount
            .as_deref(),
        Some("84")
    );
    assert!(evaluate(&with_second(u128::MAX, asset)).is_err());
    assert!(evaluate(&with_second(
        42,
        shieldd_sdk_asset::asset::Id(decaf377::Fq::from(99u64))
    ))
    .is_err());
}

#[cfg(feature = "proof")]
#[test]
fn later_attestations_are_bound_to_statement_and_document() {
    use decaf377_rdsa::{SigningKey, SpendAuth, VerificationKey};
    let mut witness = fixture();
    witness.request.outputs[0].metadata_fields.clear();
    let mut package = export_payload_keys(&witness).unwrap();
    let key = SigningKey::<SpendAuth>::new(rand::rngs::OsRng);
    let vk: [u8; 32] = VerificationKey::from(&key).into();
    let mut context = ContextAttachment {
        name: "invoice-digest".into(),
        document_sha256: "ab".repeat(32),
        text: "Later signed statement".into(),
        attestation: None,
    };
    let signature: [u8; 64] = key
        .sign(
            rand::rngs::OsRng,
            &attestation_message(&package.statement, &context).unwrap(),
        )
        .into();
    context.attestation = Some(Attestation {
        verification_key: vk.to_vec(),
        signature: signature.to_vec(),
    });
    package.context.push(context);
    assert_eq!(
        verify(&package).unwrap().verified_attestations,
        vec!["invoice-digest"]
    );
    let mut duplicate = package.context[0].clone();
    duplicate.attestation = None;
    package.context.push(duplicate);
    assert!(verify(&package).is_err());
    package.context.pop();
    package.context[0].document_sha256 = "cd".repeat(32);
    assert!(verify(&package).is_err());
    package.context[0].attestation = None;
    assert!(verify(&package).unwrap().verified_attestations.is_empty());
}

#[cfg(feature = "prover")]
#[test]
#[ignore = "executes the real guest to measure cycle count; does not prove"]
fn guest_execution_cost() {
    use risc0_zkvm::{Executor, ExecutorEnv, ExternalProver};
    let witness = fixture();
    let bytes = serde_json::to_vec(&witness).unwrap();
    let env = ExecutorEnv::builder()
        .write_slice(&bytes)
        .segment_limit_po2(18)
        .build()
        .unwrap();
    let start = std::time::Instant::now();
    let session = ExternalProver::new("local", "r0vm")
        .execute(env, shieldd_disclosure_methods::DISCLOSURE_GUEST_ELF)
        .unwrap();
    let statement: DisclosureStatement = serde_json::from_slice(&session.journal.bytes).unwrap();
    assert_eq!(statement, evaluate(&witness).unwrap());
    eprintln!(
        "guest execution only: {:?}, {} cycles, {} segments",
        start.elapsed(),
        session.cycles(),
        session.segments.len()
    );
}

#[cfg(feature = "prover")]
#[test]
#[ignore = "measures guest scaling with synthetic output witnesses; no acceptance or proof"]
fn guest_execution_scaling() {
    use risc0_zkvm::{Executor, ExecutorEnv, ExternalProver};
    for hidden_memo in [false, true] {
        for count in [1usize, 2, 8] {
            let mut witness = fixture();
            if !hidden_memo {
                witness.request.outputs[0].metadata_fields.clear();
            }
            let claim = witness.request.outputs[0].clone();
            let output = witness.outputs[0].clone();
            witness.request.outputs.clear();
            witness.outputs.clear();
            for index in 0..count {
                let mut claim = claim.clone();
                let mut output = output.clone();
                claim.reference.transaction_id = format!("{index:064x}");
                output.public.reference = claim.reference.clone();
                witness.request.outputs.push(claim);
                witness.outputs.push(output);
            }
            let bytes = serde_json::to_vec(&witness).unwrap();
            let env = ExecutorEnv::builder()
                .write_slice(&bytes)
                .segment_limit_po2(18)
                .build()
                .unwrap();
            let session = ExternalProver::new("local", "r0vm")
                .execute(env, shieldd_disclosure_methods::DISCLOSURE_GUEST_ELF)
                .unwrap();
            let actual: DisclosureStatement =
                serde_json::from_slice(&session.journal.bytes).unwrap();
            assert_eq!(actual, evaluate(&witness).unwrap());
            eprintln!(
                "{count} synthetic outputs (hidden memo: {hidden_memo}): {} cycles, {} segments",
                session.cycles(),
                session.segments.len()
            );
        }
    }
}

#[cfg(feature = "prover")]
#[test]
#[ignore = "compares segment sizes using the same witness; does not generate proofs"]
fn guest_segment_sizes() {
    use risc0_zkvm::{Executor, ExecutorEnv, ExternalProver};
    let witness = fixture();
    let bytes = serde_json::to_vec(&witness).unwrap();
    let expected = evaluate(&witness).unwrap();
    for po2 in [18, 19, 20] {
        let env = ExecutorEnv::builder()
            .write_slice(&bytes)
            .segment_limit_po2(po2)
            .build()
            .unwrap();
        let session = ExternalProver::new("local", "r0vm")
            .execute(env, shieldd_disclosure_methods::DISCLOSURE_GUEST_ELF)
            .unwrap();
        let actual: DisclosureStatement = serde_json::from_slice(&session.journal.bytes).unwrap();
        assert_eq!(actual, expected);
        eprintln!(
            "segment limit 2^{po2}: {} cycles, {} segments",
            session.cycles(),
            session.segments.len()
        );
    }
}

fn control_batch(count: usize) -> DisclosureWitness {
    use decaf377_rdsa::{SigningKey, SpendAuth, VerificationKey};
    let mut witness = fixture();
    witness.request.outputs[0].metadata_fields.clear();
    witness.request.outputs[0].spending_control = true;
    witness.request.challenge = Some("batch-control-challenge".into());
    let key = SigningKey::<SpendAuth>::new(rand::rngs::OsRng);
    let vk: [u8; 32] = VerificationKey::from(&key).into();
    witness.outputs[0].public.spend_verification_key = Some(vk.to_vec());
    let claim = witness.request.outputs[0].clone();
    let output = witness.outputs[0].clone();
    witness.request.outputs.clear();
    witness.outputs.clear();
    for index in 0..count {
        let mut claim = claim.clone();
        let mut output = output.clone();
        claim.reference.output = index as u32;
        output.public.reference = claim.reference.clone();
        witness.request.outputs.push(claim);
        witness.outputs.push(output);
    }
    let signature: [u8; 64] = key
        .sign(
            rand::rngs::OsRng,
            &control_message(&witness.request).unwrap(),
        )
        .into();
    for output in &mut witness.outputs {
        output.control_signature = Some(signature.to_vec());
    }
    witness
}

#[test]
fn repeated_control_requires_the_same_valid_key_signature_and_request() {
    let witness = control_batch(2);
    evaluate(&witness).unwrap();
    let mut changed = witness.clone();
    changed.outputs[1].control_signature.as_mut().unwrap()[0] ^= 1;
    assert!(evaluate(&changed).is_err());
    let mut changed = witness.clone();
    changed.outputs[1]
        .public
        .spend_verification_key
        .as_mut()
        .unwrap()[0] ^= 1;
    assert!(evaluate(&changed).is_err());
    let mut changed = witness;
    changed.request.challenge = Some("replayed-challenge".into());
    assert!(evaluate(&changed).is_err());
}

#[cfg(feature = "prover")]
#[test]
#[ignore = "measures repeated authority verification on synthetic selected outputs"]
fn guest_control_batch_cost() {
    use risc0_zkvm::{Executor, ExecutorEnv, ExternalProver};
    for count in [1, 2, 8] {
        let witness = control_batch(count);
        let bytes = serde_json::to_vec(&witness).unwrap();
        let env = ExecutorEnv::builder()
            .write_slice(&bytes)
            .segment_limit_po2(18)
            .build()
            .unwrap();
        let session = ExternalProver::new("local", "r0vm")
            .execute(env, shieldd_disclosure_methods::DISCLOSURE_GUEST_ELF)
            .unwrap();
        let actual: DisclosureStatement = serde_json::from_slice(&session.journal.bytes).unwrap();
        assert_eq!(actual, evaluate(&witness).unwrap());
        eprintln!(
            "{count} synthetic outputs with shared authority: {} cycles, {} segments",
            session.cycles(),
            session.segments.len()
        );
    }
}

#[test]
fn identity_authority_is_not_proof_of_secret_control() {
    use decaf377_rdsa::{SigningKey, SpendAuth, VerificationKey};
    let key = SigningKey::<SpendAuth>::new_from_field(decaf377::Fr::from(0u64));
    let vk: [u8; 32] = VerificationKey::from(&key).into();
    let mut witness = fixture();
    witness.request.outputs[0].spending_control = true;
    witness.request.challenge = Some("fresh".into());
    witness.outputs[0].public.spend_verification_key = Some(vk.to_vec());
    let signature: [u8; 64] = key
        .sign(
            rand::rngs::OsRng,
            &control_message(&witness.request).unwrap(),
        )
        .into();
    witness.outputs[0].control_signature = Some(signature.to_vec());
    assert!(evaluate(&witness).is_err());
}

#[cfg(feature = "prover")]
#[test]
#[ignore = "executes disclosure branches in the real guest; does not prove acceptance"]
fn guest_claim_variants_match_native() {
    use decaf377_rdsa::{SigningKey, SpendAuth, VerificationKey};
    use risc0_zkvm::{Executor, ExecutorEnv, ExternalProver};
    let mut revealed = fixture();
    revealed.request.outputs[0].amount = true;
    revealed.request.outputs[0].asset = true;
    revealed.request.outputs[0].recipient = true;
    revealed.request.outputs[0].memo = true;

    let mut total = fixture();
    total.request.outputs[0].metadata_fields.clear();
    let mut claim = total.request.outputs[0].clone();
    claim.reference.transaction_id = "cd".repeat(32);
    let mut output = total.outputs[0].clone();
    output.public.reference = claim.reference.clone();
    total.request.outputs.push(claim);
    total.outputs.push(output);
    total.request.total = Some(TotalClaim {
        reveal: true,
        predicate: Some(AmountPredicate::InclusiveRange {
            lower: "84".into(),
            upper: "84".into(),
        }),
    });

    let mut control = fixture();
    control.request.outputs[0].metadata_fields.clear();
    control.request.outputs[0].spending_control = true;
    control.request.challenge = Some("fresh-guest-challenge".into());
    let key = SigningKey::<SpendAuth>::new(rand::rngs::OsRng);
    let vk: [u8; 32] = VerificationKey::from(&key).into();
    control.outputs[0].public.spend_verification_key = Some(vk.to_vec());
    let signature: [u8; 64] = key
        .sign(
            rand::rngs::OsRng,
            &control_message(&control.request).unwrap(),
        )
        .into();
    control.outputs[0].control_signature = Some(signature.to_vec());

    for witness in [revealed, total, control] {
        let bytes = serde_json::to_vec(&witness).unwrap();
        let env = ExecutorEnv::builder()
            .write_slice(&bytes)
            .segment_limit_po2(18)
            .build()
            .unwrap();
        let session = ExternalProver::new("local", "r0vm")
            .execute(env, shieldd_disclosure_methods::DISCLOSURE_GUEST_ELF)
            .unwrap();
        let actual: DisclosureStatement = serde_json::from_slice(&session.journal.bytes).unwrap();
        assert_eq!(actual, evaluate(&witness).unwrap());
    }
}

#[cfg(feature = "proof")]
#[test]
fn identity_attestations_are_not_authenticated() {
    use decaf377_rdsa::{SigningKey, SpendAuth, VerificationKey};
    let mut witness = fixture();
    witness.request.outputs[0].metadata_fields.clear();
    let mut package = export_payload_keys(&witness).unwrap();
    let key = SigningKey::<SpendAuth>::new_from_field(decaf377::Fr::from(0u64));
    let vk: [u8; 32] = VerificationKey::from(&key).into();
    let mut attachment = ContextAttachment {
        name: "identity".into(),
        document_sha256: "ab".repeat(32),
        text: "publicly forgeable".into(),
        attestation: None,
    };
    let signature: [u8; 64] = key
        .sign(
            rand::rngs::OsRng,
            &attestation_message(&package.statement, &attachment).unwrap(),
        )
        .into();
    attachment.attestation = Some(Attestation {
        verification_key: vk.to_vec(),
        signature: signature.to_vec(),
    });
    package.context.push(attachment);
    assert!(verify(&package).is_err());
}

#[cfg(feature = "proof")]
#[test]
#[ignore = "verifies a real receipt produced by the end-to-end test"]
fn saved_real_receipt_rejects_wrong_guest_and_tampering() {
    let path =
        std::env::var("SHIELDD_DISCLOSURE_TEST_RECEIPT").expect("set the saved real receipt path");
    let bytes = std::fs::read(path).unwrap();
    let package = decode_package(&bytes).unwrap();
    assert!(verify(&package).unwrap().cryptography_verified);
    let Evidence::ZkReceipt(bytes) = &package.evidence else {
        panic!("expected real receipt")
    };
    let receipt: risc0_zkvm::Receipt = serde_json::from_slice(bytes).unwrap();
    assert!(receipt.verify([0u32; 8]).is_err());
    let mut changed = package.clone();
    changed.version += 1;
    assert!(verify(&changed).is_err());
    let mut changed = package.clone();
    changed.statement.request.chain_id = "wrong-chain".into();
    assert!(verify(&changed).is_err());
    let mut changed = package.clone();
    changed.statement.outputs[0].metadata[0].value = "altered".into();
    assert!(verify(&changed).is_err());
    let mut changed = package;
    changed.evidence = Evidence::ZkReceipt(vec![0]);
    assert!(verify(&changed).is_err());
}

#[test]
fn receipt_nesting_is_bounded_before_verification() {
    use risc0_zkvm::{CompositeReceipt, InnerAssumptionReceipt, InnerReceipt, Receipt};
    let empty = || {
        serde_json::from_value::<CompositeReceipt>(serde_json::json!({
            "segments": [],
            "assumption_receipts": [],
            "verifier_parameters": [0, 0, 0, 0, 0, 0, 0, 0]
        }))
        .unwrap()
    };
    let mut composite = empty();
    for _ in 0..80 {
        let mut parent = empty();
        parent
            .assumption_receipts
            .push(InnerAssumptionReceipt::Composite(composite));
        composite = parent;
    }
    let receipt = Receipt::new(InnerReceipt::Composite(composite), Vec::new());
    let package = DisclosurePackage {
        version: VERSION,
        statement: evaluate(&fixture()).unwrap(),
        evidence: Evidence::ZkReceipt(serde_json::to_vec(&receipt).unwrap()),
        context: Vec::new(),
    };
    let error = verify(&package).unwrap_err();
    assert!(
        format!("{error:#}").contains("recursion limit"),
        "{error:#}"
    );
}
