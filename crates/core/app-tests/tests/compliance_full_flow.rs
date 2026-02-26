//! Compliance integration tests: user segregation, black hole, ZK proofs.
//! Orbis-dependent tests (DKG, FROST, PRE) live in crates/bin/orbis-test.

use {
    anyhow::Result,
    ark_groth16::Groth16,
    ark_snark::SNARK,
    cnidarium::{StateDelta, TempStorage},
    common::TempStorageExt as _,
    decaf377::{Bls12_377, Fq, Fr},
    penumbra_sdk_asset::{asset, Value},
    penumbra_sdk_compliance::{
        derive_compliance_scalar,
        registry::{ComplianceRegistryRead, ComplianceRegistryWrite},
        scanning::{decrypt_with_role, FullComplianceData, ScannerRole},
        structs::{ComplianceCiphertext, ComplianceLeaf},
        BLACK_HOLE_ACK,
    },
    penumbra_sdk_keys::{
        keys::{Bip44Path, SeedPhrase, SpendKey},
        Address,
    },
    penumbra_sdk_num,
    penumbra_sdk_proof_params::DummyWitness,
    penumbra_sdk_shielded_pool::{output::OutputCircuit, spend::SpendCircuit, Note, Rseed},
    penumbra_sdk_tct as tct,
    penumbra_sdk_transaction::plan::{ActionPlan, TransactionPlan},
    penumbra_sdk_view::enrich_plan_with_compliance,
    rand_core::OsRng,
};

mod common;

// ============================================================================
// Helpers
// ============================================================================

fn create_wallet() -> (Address, SpendKey) {
    use rand_core::RngCore;
    let mut seed_bytes = [0u8; 32];
    OsRng.fill_bytes(&mut seed_bytes);
    let seed = SeedPhrase::from_randomness(&seed_bytes);
    let spend_key = SpendKey::from_seed_phrase_bip44(seed, &Bip44Path::new(0));
    let fvk = spend_key.full_viewing_key();
    let address = fvk.payment_address(0u32.into()).0;
    (address, spend_key)
}

// ============================================================================
// Test harness
// ============================================================================

struct ComplianceTestHarness {
    storage: TempStorage,
    regulated_token_id: asset::Id,
    unregulated_token_id: asset::Id,
}

impl ComplianceTestHarness {
    async fn new() -> Result<Self> {
        let storage = TempStorage::new_with_penumbra_prefixes().await?;
        let regulated_token_id = asset::Id(Fq::from(10001u64));
        let unregulated_token_id = asset::Id(Fq::from(20002u64));
        Ok(Self {
            storage,
            regulated_token_id,
            unregulated_token_id,
        })
    }

    async fn register_asset_with_params(
        &self,
        asset_id: asset::Id,
        dk_pub: decaf377::Element,
        threshold: u128,
        ring_pk: decaf377::Element,
    ) -> Result<()> {
        let mut state = StateDelta::new(self.storage.latest_snapshot());
        state
            .register_regulated_asset(
                asset_id,
                penumbra_sdk_compliance::structs::AssetPolicy::simple(dk_pub, threshold, ring_pk),
            )
            .await?;
        self.storage.commit(state).await?;
        Ok(())
    }

    async fn register_user_simple(&self, addr: Address, asset_id: asset::Id) -> Result<u64> {
        let b_d_fq = addr.diversified_generator().vartime_compress_to_field();
        let d = derive_compliance_scalar(b_d_fq);
        self.register_user_with_d(addr, asset_id, d).await
    }

    async fn register_user_with_d(&self, addr: Address, asset_id: asset::Id, d: Fq) -> Result<u64> {
        let mut state = StateDelta::new(self.storage.latest_snapshot());
        let leaf = ComplianceLeaf {
            address: addr,
            asset_id,
            d,
        };
        state.add_compliance_leaf(leaf).await?;
        let count = state.get_user_count().await?;
        self.storage.commit(state).await?;
        Ok(count - 1)
    }
}

// ============================================================================
// Decryption helpers
// ============================================================================

fn compute_shared_secrets(
    sk_ring: Fr,
    address: &Address,
    ct: &ComplianceCiphertext,
) -> (decaf377::Element, decaf377::Element, decaf377::Element) {
    let b_d_fq = address.diversified_generator().vartime_compress_to_field();
    let d = derive_compliance_scalar(b_d_fq);
    let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
    let ss_core = ct.epk_1 * (d_fr * sk_ring);
    let ss_ext = ct
        .epk_2
        .map(|epk| epk * (d_fr * sk_ring))
        .unwrap_or(decaf377::Element::default());
    let ss_sext = ct
        .epk_3
        .map(|epk| epk * (d_fr * sk_ring))
        .unwrap_or(decaf377::Element::default());
    (ss_core, ss_ext, ss_sext)
}

fn try_decrypt_with_role(
    sender_ciphertext_bytes: &[u8],
    receiver_ciphertext_bytes: &[u8],
    sk_ring: Fr,
    address: &Address,
    role: ScannerRole,
    asset_id: asset::Id,
) -> anyhow::Result<Option<FullComplianceData>> {
    let sender_ct = ComplianceCiphertext::from_bytes(sender_ciphertext_bytes)?;
    let receiver_ct = ComplianceCiphertext::from_bytes(receiver_ciphertext_bytes)?;
    let (ss_core, ss_ext, ss_sext) = compute_shared_secrets(
        sk_ring,
        address,
        match role {
            ScannerRole::Sender => &sender_ct,
            ScannerRole::Receiver => &receiver_ct,
        },
    );
    let ss_sext_for_sender = if role == ScannerRole::Sender {
        let (_, _, s) = compute_shared_secrets(sk_ring, address, &receiver_ct);
        s
    } else {
        ss_sext
    };
    decrypt_with_role(
        &ss_core,
        &ss_ext,
        &ss_sext_for_sender,
        &sender_ct,
        &receiver_ct,
        role,
        asset_id,
    )
}

fn create_spend_dual_ciphertext(
    sender_sk_ring: Fr,
    sender_address: &Address,
    sender_dk_pub: &decaf377::Element,
    receiver_sk_ring: Fr,
    receiver_address: &Address,
    receiver_dk_pub: &decaf377::Element,
    asset_id: asset::Id,
    amount: penumbra_sdk_num::Amount,
    is_regulated: bool,
) -> Result<(Vec<u8>, Vec<u8>)> {
    use penumbra_sdk_compliance::crypto::{encrypt_output, encrypt_spend};

    let (ack_s, dk_s) = if is_regulated {
        let b_d_fq = sender_address
            .diversified_generator()
            .vartime_compress_to_field();
        let ring_pk_s = decaf377::Element::GENERATOR * sender_sk_ring;
        let d = derive_compliance_scalar(b_d_fq);
        let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
        (ring_pk_s * d_fr, *sender_dk_pub)
    } else {
        (*BLACK_HOLE_ACK, *BLACK_HOLE_ACK)
    };

    let (ack_r, ack_sext_s, dk_r) = if is_regulated {
        let b_d_r = receiver_address
            .diversified_generator()
            .vartime_compress_to_field();
        let b_d_s = sender_address
            .diversified_generator()
            .vartime_compress_to_field();
        let ring_pk_r = decaf377::Element::GENERATOR * receiver_sk_ring;
        let ring_pk_s = decaf377::Element::GENERATOR * sender_sk_ring;
        let d_r = derive_compliance_scalar(b_d_r);
        let d_r_fr = Fr::from_le_bytes_mod_order(&d_r.to_bytes());
        let d_s = derive_compliance_scalar(b_d_s);
        let d_s_fr = Fr::from_le_bytes_mod_order(&d_s.to_bytes());
        (ring_pk_r * d_r_fr, ring_pk_s * d_s_fr, *receiver_dk_pub)
    } else {
        (*BLACK_HOLE_ACK, *BLACK_HOLE_ACK, *BLACK_HOLE_ACK)
    };

    let spend_result = encrypt_spend(
        &mut OsRng,
        &ack_s,
        &dk_s,
        sender_address,
        asset_id,
        amount,
        false,
        Fq::from(0u64),
    )?;
    let output_result = encrypt_output(
        &mut OsRng,
        &ack_r,
        &ack_sext_s,
        &dk_r,
        receiver_address,
        sender_address,
        asset_id,
        amount,
        false,
        Fq::from(0u64),
    )?;

    Ok((
        spend_result.ciphertext.to_bytes(),
        output_result.ciphertext.to_bytes(),
    ))
}

// ============================================================================
// Test: User segregation
// ============================================================================

#[tokio::test]
async fn test_user_segregation() -> Result<()> {
    let _guard = common::set_tracing_subscriber();
    let harness = ComplianceTestHarness::new().await?;

    let alice_sk_ring = Fr::rand(&mut OsRng);
    let alice_dk_pub = decaf377::Element::GENERATOR * Fr::rand(&mut OsRng);
    let bob_sk_ring = Fr::rand(&mut OsRng);
    let bob_dk_pub = decaf377::Element::GENERATOR * Fr::rand(&mut OsRng);
    let (alice_addr, _) = create_wallet();
    let (bob_addr, _) = create_wallet();

    harness
        .register_asset_with_params(
            harness.regulated_token_id,
            decaf377::Element::GENERATOR,
            u128::MAX,
            decaf377::Element::GENERATOR,
        )
        .await?;

    let value = Value {
        amount: 100u64.into(),
        asset_id: harness.regulated_token_id,
    };

    let (alice_sender_ct, bob_receiver_ct) = create_spend_dual_ciphertext(
        alice_sk_ring,
        &alice_addr,
        &alice_dk_pub,
        bob_sk_ring,
        &bob_addr,
        &bob_dk_pub,
        value.asset_id,
        value.amount,
        true,
    )?;

    assert!(
        try_decrypt_with_role(
            &alice_sender_ct,
            &bob_receiver_ct,
            alice_sk_ring,
            &alice_addr,
            ScannerRole::Sender,
            value.asset_id,
        )?
        .is_some(),
        "Alice SHOULD decrypt her own send"
    );

    assert!(
        try_decrypt_with_role(
            &alice_sender_ct,
            &bob_receiver_ct,
            alice_sk_ring,
            &alice_addr,
            ScannerRole::Receiver,
            value.asset_id,
        )?
        .is_none(),
        "Alice CANNOT decrypt Bob's receiver ciphertext"
    );

    assert!(
        try_decrypt_with_role(
            &alice_sender_ct,
            &bob_receiver_ct,
            bob_sk_ring,
            &bob_addr,
            ScannerRole::Receiver,
            value.asset_id,
        )?
        .is_some(),
        "Bob SHOULD decrypt his own receive"
    );

    assert!(
        try_decrypt_with_role(
            &alice_sender_ct,
            &bob_receiver_ct,
            bob_sk_ring,
            &bob_addr,
            ScannerRole::Sender,
            value.asset_id,
        )?
        .is_none(),
        "Bob CANNOT decrypt Alice's sender ciphertext"
    );

    assert!(
        try_decrypt_with_role(
            &alice_sender_ct,
            &bob_receiver_ct,
            bob_sk_ring,
            &alice_addr,
            ScannerRole::Sender,
            value.asset_id,
        )?
        .is_none(),
        "Wrong ring key SHOULD fail"
    );

    assert!(
        try_decrypt_with_role(
            &alice_sender_ct,
            &bob_receiver_ct,
            alice_sk_ring,
            &alice_addr,
            ScannerRole::Receiver,
            value.asset_id,
        )?
        .is_none(),
        "Wrong role SHOULD fail"
    );

    Ok(())
}

// ============================================================================
// Test: Unregulated asset → BLACK_HOLE_ACK → undecryptable
// ============================================================================

#[tokio::test]
async fn test_unregulated_asset_black_hole_undecryptable() -> Result<()> {
    let _guard = common::set_tracing_subscriber();
    let harness = ComplianceTestHarness::new().await?;

    let alice_sk_ring = Fr::rand(&mut OsRng);
    let alice_dk_pub = decaf377::Element::GENERATOR * Fr::rand(&mut OsRng);
    let bob_sk_ring = Fr::rand(&mut OsRng);
    let bob_dk_pub = decaf377::Element::GENERATOR * Fr::rand(&mut OsRng);
    let (alice_addr, _) = create_wallet();
    let (bob_addr, _) = create_wallet();

    let value = Value {
        amount: 100u64.into(),
        asset_id: harness.unregulated_token_id,
    };

    let (sender_ct, receiver_ct) = create_spend_dual_ciphertext(
        alice_sk_ring,
        &alice_addr,
        &alice_dk_pub,
        bob_sk_ring,
        &bob_addr,
        &bob_dk_pub,
        value.asset_id,
        value.amount,
        false,
    )?;

    for (sk, addr, role, label) in [
        (
            alice_sk_ring,
            &alice_addr,
            ScannerRole::Sender,
            "Alice sender",
        ),
        (
            alice_sk_ring,
            &alice_addr,
            ScannerRole::Receiver,
            "Alice receiver",
        ),
        (bob_sk_ring, &bob_addr, ScannerRole::Sender, "Bob sender"),
        (
            bob_sk_ring,
            &bob_addr,
            ScannerRole::Receiver,
            "Bob receiver",
        ),
    ] {
        assert!(
            try_decrypt_with_role(&sender_ct, &receiver_ct, sk, addr, role, value.asset_id)?
                .is_none(),
            "{label} CANNOT decrypt BLACK_HOLE ciphertext"
        );
    }

    Ok(())
}

// ============================================================================
// Test: ZK proof roundtrip (spend + output circuit verification)
// ============================================================================

#[tokio::test]
async fn test_full_compliance_proof_roundtrip() -> Result<()> {
    use penumbra_sdk_mock_client::StateReadComplianceProvider;
    use penumbra_sdk_shielded_pool::{OutputPlan, SpendPlan};

    let _guard = common::set_tracing_subscriber();
    let harness = ComplianceTestHarness::new().await?;

    harness
        .register_asset_with_params(
            harness.regulated_token_id,
            decaf377::Element::GENERATOR,
            u128::MAX,
            decaf377::Element::GENERATOR,
        )
        .await?;

    let (alice_addr, alice_spend_key) = create_wallet();
    let (bob_addr, _) = create_wallet();

    harness
        .register_user_simple(alice_addr.clone(), harness.regulated_token_id)
        .await?;
    harness
        .register_user_simple(bob_addr.clone(), harness.regulated_token_id)
        .await?;

    let value = Value {
        amount: 100u64.into(),
        asset_id: harness.regulated_token_id,
    };

    let spend_note = Note::from_parts(alice_addr.clone(), value, Rseed::generate(&mut OsRng))?;
    let mut sct = tct::Tree::new();
    sct.insert(tct::Witness::Keep, spend_note.commit())?;
    let anchor = sct.root();
    let state_commitment_proof = sct
        .witness(spend_note.commit())
        .expect("note was just inserted");

    let fvk = alice_spend_key.full_viewing_key();

    let spend_plan = SpendPlan::new(
        &mut OsRng,
        spend_note.clone(),
        state_commitment_proof.position(),
    );
    let output_plan = OutputPlan::new(&mut OsRng, value, bob_addr.clone());

    let mut plan = TransactionPlan {
        actions: vec![
            ActionPlan::Spend(spend_plan),
            ActionPlan::Output(output_plan),
        ],
        ..Default::default()
    };

    let snapshot = harness.storage.latest_snapshot();
    let provider = StateReadComplianceProvider::new(snapshot);
    enrich_plan_with_compliance(&mut plan, &provider, &mut OsRng, None).await?;

    let ActionPlan::Spend(spend_plan) = plan.actions[0].clone() else {
        panic!("expected spend")
    };
    let ActionPlan::Output(output_plan) = plan.actions[1].clone() else {
        panic!("expected output")
    };

    let tx_blinding_nonce = spend_plan.tx_blinding_nonce;
    let compliance_anchor = spend_plan.compliance_anchor;
    let asset_anchor = spend_plan.asset_anchor;

    let fresh_spend_circuit = SpendCircuit::with_dummy_witness();
    let (fresh_spend_pk, fresh_spend_vk) =
        Groth16::<Bls12_377>::circuit_specific_setup(fresh_spend_circuit, &mut OsRng)
            .expect("spend circuit setup");
    let fresh_spend_vk = ark_groth16::PreparedVerifyingKey::from(fresh_spend_vk);

    let spend_proof = spend_plan.spend_proof(
        fvk,
        state_commitment_proof.clone(),
        anchor,
        &fresh_spend_pk,
        None,
    )?;

    use penumbra_sdk_shielded_pool::spend::SpendProofPublic;
    let spend_ct = ComplianceCiphertext::from_bytes(&spend_plan.compliance_ciphertext)?;
    let (spend_epk, spend_c2_core, spend_ct_circuit) = spend_ct.to_spend_circuit_public_inputs();
    let spend_sender_leaf_hash = spend_plan.compliance_leaf.clone().unwrap().commit();
    let spend_sender_blinded =
        penumbra_sdk_compliance::blind_sender_leaf(spend_sender_leaf_hash, tx_blinding_nonce);

    let spend_public = SpendProofPublic {
        anchor,
        balance_commitment: spend_plan.balance().commit(spend_plan.value_blinding),
        nullifier: spend_plan.nullifier(fvk),
        rk: spend_plan.rk(fvk),
        asset_anchor,
        compliance_anchor,
        epk: spend_epk,
        c2_core: spend_c2_core,
        compliance_ciphertext: spend_ct_circuit,
        target_timestamp: Fq::from(spend_plan.target_timestamp),
        dleq_c: spend_plan.dleq_c,
        dleq_s: spend_plan.dleq_s,
        sender_leaf_hash: spend_sender_blinded,
    };
    spend_proof.verify(&fresh_spend_vk, spend_public)?;

    let fresh_output_circuit = OutputCircuit::with_dummy_witness();
    let (fresh_output_pk, fresh_output_vk) =
        Groth16::<Bls12_377>::circuit_specific_setup(fresh_output_circuit, &mut OsRng)
            .expect("output circuit setup");
    let fresh_output_vk = ark_groth16::PreparedVerifyingKey::from(fresh_output_vk);

    let output_proof = output_plan.output_proof(&fresh_output_pk, None)?;

    use penumbra_sdk_shielded_pool::output::OutputProofPublic;
    let output_ct = ComplianceCiphertext::from_bytes(&output_plan.compliance_ciphertext)?;
    let (oepk1, oepk2, oepk3, oc2c, oc2e, oc2s, oct_circuit) =
        output_ct.to_output_circuit_public_inputs();
    let output_cp_leaf_hash = output_plan.counterparty_leaf.clone().unwrap().commit();
    let output_sender_blinded =
        penumbra_sdk_compliance::blind_sender_leaf(output_cp_leaf_hash, tx_blinding_nonce);

    let output_public = OutputProofPublic {
        balance_commitment: output_plan.balance().commit(output_plan.value_blinding),
        note_commitment: output_plan.output_note().commit(),
        epk_1: oepk1,
        epk_2: oepk2,
        epk_3: oepk3,
        c2_core: oc2c,
        c2_ext: oc2e,
        c2_sext: oc2s,
        compliance_ciphertext: oct_circuit,
        asset_anchor,
        compliance_anchor,
        target_timestamp: Fq::from(output_plan.target_timestamp),
        dleq_c_1: output_plan.dleq_c_1,
        dleq_s_1: output_plan.dleq_s_1,
        dleq_c_2: output_plan.dleq_c_2,
        dleq_s_2: output_plan.dleq_s_2,
        dleq_c_3: output_plan.dleq_c_3,
        dleq_s_3: output_plan.dleq_s_3,
        counterparty_leaf_hash: output_sender_blinded,
    };
    output_proof.verify(&fresh_output_vk, output_public)?;

    assert_eq!(
        output_sender_blinded, spend_sender_blinded,
        "Leaf hash binding must match"
    );

    Ok(())
}

// ============================================================================
// Test: Flagged spend proof roundtrip
// ============================================================================

#[tokio::test]
async fn test_flagged_spend_proof_roundtrip() -> Result<()> {
    use penumbra_sdk_shielded_pool::{OutputPlan, SpendPlan};

    let _guard = common::set_tracing_subscriber();
    let storage = TempStorage::new_with_penumbra_prefixes().await?;

    let spend_circuit = SpendCircuit::with_dummy_witness();
    let (spend_pk, _spend_vk) =
        Groth16::<Bls12_377>::circuit_specific_setup(spend_circuit, &mut OsRng)
            .expect("spend circuit setup");

    let regulated_token_id = asset::Id(Fq::from(10001u64));
    let dummy_dk_pub = decaf377::Element::GENERATOR;
    {
        let mut state = StateDelta::new(storage.latest_snapshot());
        state
            .register_regulated_asset(
                regulated_token_id,
                penumbra_sdk_compliance::structs::AssetPolicy::simple(
                    dummy_dk_pub,
                    500u128,
                    decaf377::Element::GENERATOR,
                ),
            )
            .await?;
        storage.commit(state).await?;
    }

    let mut seed_bytes = [0u8; 32];
    rand_core::RngCore::fill_bytes(&mut OsRng, &mut seed_bytes);
    let alice_spend_key = SpendKey::from_seed_phrase_bip44(
        SeedPhrase::from_randomness(&seed_bytes),
        &Bip44Path::new(0),
    );
    let alice_fvk = alice_spend_key.full_viewing_key();
    let alice_addr = alice_fvk.incoming().payment_address(0u32.into()).0;

    rand_core::RngCore::fill_bytes(&mut OsRng, &mut seed_bytes);
    let bob_spend_key = SpendKey::from_seed_phrase_bip44(
        SeedPhrase::from_randomness(&seed_bytes),
        &Bip44Path::new(0),
    );
    let bob_fvk = bob_spend_key.full_viewing_key();
    let bob_addr = bob_fvk.incoming().payment_address(0u32.into()).0;

    {
        let mut state = StateDelta::new(storage.latest_snapshot());
        let b_d_fq = alice_addr
            .diversified_generator()
            .vartime_compress_to_field();
        let d = derive_compliance_scalar(b_d_fq);
        state
            .add_compliance_leaf(ComplianceLeaf {
                address: alice_addr.clone(),
                asset_id: regulated_token_id,
                d,
            })
            .await?;
        storage.commit(state).await?;
    }
    {
        let mut state = StateDelta::new(storage.latest_snapshot());
        let b_d_fq = bob_addr.diversified_generator().vartime_compress_to_field();
        let d = derive_compliance_scalar(b_d_fq);
        state
            .add_compliance_leaf(ComplianceLeaf {
                address: bob_addr.clone(),
                asset_id: regulated_token_id,
                d,
            })
            .await?;
        storage.commit(state).await?;
    }

    let value = Value {
        amount: 1000u64.into(),
        asset_id: regulated_token_id,
    };
    let spend_note = Note::from_parts(alice_addr.clone(), value, Rseed::generate(&mut OsRng))?;

    let mut sct = tct::Tree::new();
    sct.insert(tct::Witness::Keep, spend_note.commit())?;
    let anchor = sct.root();
    let state_commitment_proof = sct
        .witness(spend_note.commit())
        .expect("note was just inserted");

    let spend_plan = SpendPlan::new(
        &mut OsRng,
        spend_note.clone(),
        state_commitment_proof.position(),
    );
    let output_plan = OutputPlan::new(&mut OsRng, value, bob_addr.clone());

    let mut plan = TransactionPlan {
        actions: vec![
            ActionPlan::Spend(spend_plan),
            ActionPlan::Output(output_plan),
        ],
        ..Default::default()
    };

    let snapshot = storage.latest_snapshot();
    let provider = penumbra_sdk_mock_client::StateReadComplianceProvider::new(snapshot);
    enrich_plan_with_compliance(&mut plan, &provider, &mut OsRng, None).await?;

    let ActionPlan::Spend(spend_plan) = plan.actions[0].clone() else {
        panic!("expected spend")
    };

    assert!(
        spend_plan.is_flagged,
        "spend should be flagged (amount 1000 >= threshold 500)"
    );

    let spend_proof =
        spend_plan.spend_proof(alice_fvk, state_commitment_proof, anchor, &spend_pk, None);
    assert!(
        spend_proof.is_ok(),
        "Spend proof should succeed for flagged transaction: {:?}",
        spend_proof.err()
    );

    Ok(())
}
