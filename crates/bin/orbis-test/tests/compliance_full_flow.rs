//! Compliance integration tests: user segregation, black hole, ZK proofs, and real Orbis flow.

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
// Context structs for modular test composition
// ============================================================================

struct RingContext {
    ring_pk: decaf377::Element,
    secret_shares: Vec<crypto::r#trait::PriShare<Fr>>,
    pub_poly: crypto::decaf377::common::PubPoly,
    n: usize,
    t: usize,
}

struct IssuerContext {
    dk_scalar: Fr,
    dk_pub: decaf377::Element,
    sk_issuer: Fr,
    pk_issuer: decaf377::Element,
}

struct UserContext {
    address: Address,
    #[allow(dead_code)]
    spend_key: SpendKey,
    b_d_fq: Fq,
    #[allow(dead_code)]
    d: Fq,
    #[allow(dead_code)]
    d_fr: Fr,
    ack: decaf377::Element,
}

// ============================================================================
// Builder methods
// ============================================================================

fn create_ring(n: usize, t: usize) -> Result<RingContext> {
    use crypto::decaf377::dkg::DKGNode;
    use crypto::r#trait::Dkg;
    use crypto::test_helper::DKGCoordinator;

    let mut coordinator = DKGCoordinator::new(
        |id, threshold, total_nodes, session_id| DKGNode::new(id, threshold, total_nodes, session_id),
        n,
        t,
    )?;
    let (ring_pk, secret_shares, pub_poly) = coordinator.run_dkg()?;
    Ok(RingContext {
        ring_pk,
        secret_shares,
        pub_poly,
        n,
        t,
    })
}

fn create_issuer() -> IssuerContext {
    let dk_scalar = Fr::rand(&mut OsRng);
    let dk_pub = decaf377::Element::GENERATOR * dk_scalar;
    let sk_issuer = Fr::rand(&mut OsRng);
    let pk_issuer = decaf377::Element::GENERATOR * sk_issuer;
    IssuerContext {
        dk_scalar,
        dk_pub,
        sk_issuer,
        pk_issuer,
    }
}

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

/// Register a user: derive d, compute ACK, FROST-sign registration, insert compliance leaf.
async fn register_user(
    harness: &ComplianceTestHarness,
    ring: &RingContext,
    asset_id: asset::Id,
) -> Result<UserContext> {
    let (address, spend_key) = create_wallet();
    let b_d_fq = address.diversified_generator().vartime_compress_to_field();
    let d = derive_compliance_scalar(b_d_fq);
    let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
    let ack = ring.ring_pk * d_fr;

    let registration_msg = build_ack_registration_message(&address, &ack, &ring.ring_pk, asset_id);
    let sig = orbis_frost_sign(
        &ring.secret_shares,
        &ring.pub_poly,
        &registration_msg,
        ring.t,
        ring.n,
    )?;
    verify_ack_signature(&ring.ring_pk, &registration_msg, &sig)?;

    harness
        .register_user_with_d(address.clone(), asset_id, d)
        .await?;
    Ok(UserContext {
        address,
        spend_key,
        b_d_fq,
        d,
        d_fr,
        ack,
    })
}

/// Derive seed from c2: seed = c2 - (ack × r).compress()
fn derive_seed(c2: &Fq, ack: &decaf377::Element, r: &Fr) -> Fq {
    let ss = (*ack * *r).vartime_compress_to_field();
    *c2 - ss
}

// ============================================================================
// Simplified test harness
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

    async fn register_asset(
        &self,
        asset_id: asset::Id,
        issuer: &IssuerContext,
        ring: &RingContext,
    ) -> Result<()> {
        let mut state = StateDelta::new(self.storage.latest_snapshot());
        state
            .register_regulated_asset(
                asset_id,
                penumbra_sdk_compliance::structs::AssetPolicy::simple(
                    issuer.dk_pub,
                    u128::MAX,
                    ring.ring_pk,
                ),
            )
            .await?;
        self.storage.commit(state).await?;
        Ok(())
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

    #[allow(dead_code)]
    async fn register_user_simple(&self, addr: Address, asset_id: asset::Id) -> Result<u64> {
        let b_d_fq = addr.diversified_generator().vartime_compress_to_field();
        let d = derive_compliance_scalar(b_d_fq);
        self.register_user_with_d(addr, asset_id, d).await
    }
}

// ============================================================================
// User-side decryption helpers (for segregation tests)
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
// FROST signing helpers
// ============================================================================

fn build_ack_registration_message(
    address: &Address,
    ack: &decaf377::Element,
    pk_ack_g: &decaf377::Element,
    asset_id: asset::Id,
) -> Vec<u8> {
    use ark_serialize::CanonicalSerialize;
    let mut msg = Vec::new();
    msg.extend_from_slice(b"orbis-ack-registration-v1");
    address
        .diversified_generator()
        .serialize_compressed(&mut msg)
        .expect("serialize B_d");
    msg.extend_from_slice(&address.transmission_key().0);
    ack.serialize_compressed(&mut msg).expect("serialize ACK");
    pk_ack_g
        .serialize_compressed(&mut msg)
        .expect("serialize pk_ack_g");
    msg.extend_from_slice(&asset_id.0.to_bytes());
    msg
}

fn orbis_frost_sign(
    secret_shares: &[crypto::r#trait::PriShare<Fr>],
    pub_poly: &crypto::decaf377::common::PubPoly,
    msg: &[u8],
    t: usize,
    n: usize,
) -> anyhow::Result<crypto::decaf377::sign::SchnorrSignature> {
    use crypto::decaf377::sign::ThresholdDecafSigner;
    use crypto::r#trait::{DistKeyShare, PriShare, ThresholdSigner};

    let signer = ThresholdDecafSigner::new();
    let dist_key_shares: Vec<DistKeyShare<Fr>> = secret_shares
        .iter()
        .take(t)
        .map(|s| DistKeyShare {
            pri_share: PriShare { i: s.i, v: s.v },
        })
        .collect();

    let mut commitments = Vec::new();
    let mut states = Vec::new();
    for dks in &dist_key_shares {
        let (commitment, state) = signer.generate_nonces(dks)?;
        commitments.push((dks.pri_share.i, commitment));
        states.push(state);
    }

    let mut sig_shares = Vec::new();
    for (dks, state) in dist_key_shares.iter().zip(states.iter()) {
        let sig_share = signer.sign(dks, msg, pub_poly, Some(state), &commitments)?;
        signer.verify_share(msg, pub_poly, &sig_share, &commitments)?;
        sig_shares.push(sig_share);
    }

    let sig = signer.recover(&sig_shares, t, n, msg, &commitments)?;
    sig.ok_or_else(|| anyhow::anyhow!("FROST signature recovery returned None"))
}

fn verify_ack_signature(
    ring_pk: &decaf377::Element,
    msg: &[u8],
    signature: &crypto::decaf377::sign::SchnorrSignature,
) -> anyhow::Result<()> {
    use crypto::decaf377::sign::ThresholdDecafSigner;
    use crypto::r#trait::ThresholdSigner;
    let verifier = ThresholdDecafSigner::new();
    verifier.verify(ring_pk, msg, signature)?;
    Ok(())
}

// ============================================================================
// Orbis threshold PRE (accepts Secret + reader pk for threshold re-encryption)
// ============================================================================

fn orbis_threshold_pre(
    ring: &RingContext,
    enc_cmt: &decaf377::Element,
    secret: &crypto::r#trait::Secret,
    pk_issuer: &decaf377::Element,
    derivation_bytes: Option<&[u8]>,
) -> anyhow::Result<decaf377::Element> {
    use crypto::decaf377::pre::ThresholdDealerNode;
    use crypto::r#trait::{DistKeyShare, PriShare, ThresholdDealer};

    let dealer = ThresholdDealerNode::new();
    let mut reencrypt_shares = Vec::new();

    for share in ring.secret_shares.iter().take(ring.t) {
        let dist_key_share = DistKeyShare {
            pri_share: PriShare {
                i: share.i,
                v: share.v,
            },
        };
        let reply = dealer.reencrypt(&dist_key_share, secret, pk_issuer, derivation_bytes)?;
        dealer.verify(pk_issuer, &ring.pub_poly, enc_cmt, &reply, derivation_bytes)?;
        reencrypt_shares.push(reply.share);
    }

    let result = dealer.recover(&reencrypt_shares, ring.t, ring.n)?;
    result.ok_or_else(|| anyhow::anyhow!("PRE recovery returned None"))
}

// ============================================================================
// Adjusted reader key PRE (matches orbis-audit production flow)
// ============================================================================

/// Simulate Orbis store_secret: encrypt dummy data to get enc_cmt_orbis.
/// One context per (ring, derivation) pair, matching production.
fn create_dummy_orbis_context(
    ring: &RingContext,
    derivation_bytes: &[u8],
) -> Result<(decaf377::Element, crypto::r#trait::Secret)> {
    use crypto::decaf377::pre::ThresholdDealerNode;
    use crypto::r#trait::ThresholdDealer;
    let (enc_cmt, secret, _proof) = ThresholdDealerNode::encrypt_secret(
        &ring.ring_pk,
        &[0u8; 32],
        Some(derivation_bytes),
        None,
    )?;
    Ok((enc_cmt, secret))
}

/// PRE for one tier using the adjusted reader key trick.
/// Mirrors orbis-audit: adjusted_pk → threshold PRE → recover_seed.
fn adjusted_reader_pre(
    ring: &RingContext,
    enc_cmt_orbis: &decaf377::Element,
    dummy_secret: &crypto::r#trait::Secret,
    pk_issuer: &decaf377::Element,
    sk_issuer: &Fr,
    epk_chain: &decaf377::Element,
    ack: &decaf377::Element,
    c2: &Fq,
    derivation_bytes: &[u8],
) -> Result<Fq> {
    use penumbra_sdk_compliance::compute_adjusted_reader_pk;
    use penumbra_sdk_compliance::orbis::recover_seed;

    let adjusted_pk = compute_adjusted_reader_pk(pk_issuer, epk_chain, enc_cmt_orbis);
    let xnc_cmt = orbis_threshold_pre(
        ring,
        enc_cmt_orbis,
        dummy_secret,
        &adjusted_pk,
        Some(derivation_bytes),
    )?;
    Ok(recover_seed(&xnc_cmt, sk_issuer, ack, c2))
}

// ============================================================================
// Test 1: Full Orbis flow — DKG → registration → encryption → Orbis PRE → decryption
// ============================================================================

#[tokio::test]
async fn test_full_orbis_flow() -> Result<()> {
    use penumbra_sdk_compliance::crypto::{
        compute_dleq_native, compute_metadata_hash, decrypt_detection_tier, encrypt_output,
        encrypt_spend, verify_dleq_native,
    };
    use penumbra_sdk_compliance::scanning::{
        decrypt_core_with_seed, decrypt_extension_with_seed, decrypt_spend_ext_with_seed,
    };

    let _guard = common::set_tracing_subscriber();
    let harness = ComplianceTestHarness::new().await?;

    // Phase 1: DKG → ring
    let ring = create_ring(5, 3)?;

    // Phase 2: Issuer keypair
    let issuer = create_issuer();

    // Phase 3: Register regulated asset with real ring_pk + dk_pub
    harness
        .register_asset(harness.regulated_token_id, &issuer, &ring)
        .await?;

    // Phase 4: Register users with FROST-signed ACK
    let alice = register_user(&harness, &ring, harness.regulated_token_id).await?;
    let bob = register_user(&harness, &ring, harness.regulated_token_id).await?;

    // Phase 5: Encrypt output (Alice → Bob)
    let value = Value {
        amount: 100u64.into(),
        asset_id: harness.regulated_token_id,
    };

    let output_result = encrypt_output(
        &mut OsRng,
        &bob.ack,   // receiver ACK → core + ext
        &alice.ack, // sender ACK → sext
        &issuer.dk_pub,
        &bob.address,
        &alice.address,
        value.asset_id,
        value.amount,
        false,
        Fq::from(0u64),
    )?;
    let ct = &output_result.ciphertext;

    // Phase 6: Derive seeds from c2 values
    let seed_core = derive_seed(&ct.c2_core, &bob.ack, &output_result.r_1);
    let seed_ext = derive_seed(
        &ct.c2_ext.expect("output has c2_ext"),
        &bob.ack,
        &output_result.r_2,
    );
    let seed_sext = derive_seed(
        &ct.c2_sext.expect("output has c2_sext"),
        &alice.ack,
        &output_result.r_3,
    );

    // Phase 7: DLEQ proofs for all output tiers
    let metadata_hash = compute_metadata_hash(
        Fq::from(1u64),
        Fq::from(2u64),
        Fq::from(3u64),
        Fq::from(1u64),
        Fq::from(1_700_000_000u64),
        Fq::from(0u64),
    );

    let s_core = bob.ack * output_result.r_1;
    let dleq_core = compute_dleq_native(
        output_result.r_1,
        Fr::rand(&mut OsRng),
        &bob.ack,
        &ct.epk_1,
        metadata_hash,
    );
    verify_dleq_native(
        &bob.ack,
        &ct.epk_1,
        &s_core,
        &dleq_core.c,
        &dleq_core.s,
        metadata_hash,
    )
    .expect("Core DLEQ should verify");

    let epk_2 = ct.epk_2.expect("output has epk_2");
    let s_ext = bob.ack * output_result.r_2;
    let dleq_ext = compute_dleq_native(
        output_result.r_2,
        Fr::rand(&mut OsRng),
        &bob.ack,
        &epk_2,
        metadata_hash,
    );
    verify_dleq_native(
        &bob.ack,
        &epk_2,
        &s_ext,
        &dleq_ext.c,
        &dleq_ext.s,
        metadata_hash,
    )
    .expect("Ext DLEQ should verify");

    let epk_3 = ct.epk_3.expect("output has epk_3");
    let s_sext = alice.ack * output_result.r_3;
    let dleq_sext = compute_dleq_native(
        output_result.r_3,
        Fr::rand(&mut OsRng),
        &alice.ack,
        &epk_3,
        metadata_hash,
    );
    verify_dleq_native(
        &alice.ack,
        &epk_3,
        &s_sext,
        &dleq_sext.c,
        &dleq_sext.s,
        metadata_hash,
    )
    .expect("Sext DLEQ should verify");

    // Phase 8: Adjusted reader key PRE for each tier
    // Create dummy Orbis contexts per derivation (one per user, matching production)
    let bob_b_d_bytes = bob.b_d_fq.to_bytes();
    let (enc_cmt_bob, secret_bob) = create_dummy_orbis_context(&ring, &bob_b_d_bytes)?;

    let alice_b_d_bytes = alice.b_d_fq.to_bytes();
    let (enc_cmt_alice, secret_alice) = create_dummy_orbis_context(&ring, &alice_b_d_bytes)?;

    // Core tier (uses Bob's derivation — he's the receiver)
    let recovered_core = adjusted_reader_pre(
        &ring,
        &enc_cmt_bob,
        &secret_bob,
        &issuer.pk_issuer,
        &issuer.sk_issuer,
        &ct.epk_1,
        &bob.ack,
        &ct.c2_core,
        &bob_b_d_bytes,
    )?;
    assert_eq!(
        recovered_core, seed_core,
        "PRE-recovered seed must match direct derivation"
    );
    let core_data = decrypt_core_with_seed(recovered_core, ct)?;
    assert!(core_data.is_some(), "Core tier should decrypt");
    let core = core_data.unwrap();
    assert_eq!(core.amount, value.amount, "Core: amount mismatch");
    assert_eq!(
        core.self_transmission_key,
        bob.address.transmission_key().0,
        "Core: receiver TK mismatch"
    );

    // Ext tier (uses Bob's derivation, reuses same dummy context)
    let recovered_ext = adjusted_reader_pre(
        &ring,
        &enc_cmt_bob,
        &secret_bob,
        &issuer.pk_issuer,
        &issuer.sk_issuer,
        &epk_2,
        &bob.ack,
        &ct.c2_ext.expect("output has c2_ext"),
        &bob_b_d_bytes,
    )?;
    assert_eq!(
        recovered_ext, seed_ext,
        "PRE-recovered ext seed must match direct derivation"
    );
    let ext_data = decrypt_extension_with_seed(recovered_ext, ct)?;
    assert!(ext_data.is_some(), "Ext tier should decrypt");
    let ext = ext_data.unwrap();
    assert_eq!(
        ext.counterparty_diversified_generator,
        *alice.address.diversified_generator(),
        "Ext: counterparty should be Alice"
    );

    // Sext tier (uses Alice's derivation — she's the sender)
    let recovered_sext = adjusted_reader_pre(
        &ring,
        &enc_cmt_alice,
        &secret_alice,
        &issuer.pk_issuer,
        &issuer.sk_issuer,
        &epk_3,
        &alice.ack,
        &ct.c2_sext.expect("output has c2_sext"),
        &alice_b_d_bytes,
    )?;
    assert_eq!(
        recovered_sext, seed_sext,
        "PRE-recovered sext seed must match direct derivation"
    );
    let sext_data = decrypt_spend_ext_with_seed(recovered_sext, ct)?;
    assert!(sext_data.is_some(), "Sext tier should decrypt");

    // Phase 9: Detection tier
    let det = decrypt_detection_tier(
        &issuer.dk_scalar,
        &ct.epk_1,
        &ct.detection_tag,
        &value.asset_id,
    )?;
    let (det_asset_id, det_flag, _det_salt) = det;
    assert_eq!(det_asset_id, value.asset_id, "Detection: asset_id mismatch");
    assert!(!det_flag, "Detection: should NOT be flagged");

    // Phase 10: Spend path
    let spend_result = encrypt_spend(
        &mut OsRng,
        &alice.ack,
        &issuer.dk_pub,
        &alice.address,
        value.asset_id,
        value.amount,
        false,
        Fq::from(0u64),
    )?;
    let spend_ct = &spend_result.ciphertext;
    let seed_spend = derive_seed(&spend_ct.c2_core, &alice.ack, &spend_result.r_s);

    let s_spend = alice.ack * spend_result.r_s;
    let dleq_spend = compute_dleq_native(
        spend_result.r_s,
        Fr::rand(&mut OsRng),
        &alice.ack,
        &spend_ct.epk_1,
        metadata_hash,
    );
    verify_dleq_native(
        &alice.ack,
        &spend_ct.epk_1,
        &s_spend,
        &dleq_spend.c,
        &dleq_spend.s,
        metadata_hash,
    )
    .expect("Spend DLEQ should verify");

    let recovered_spend = adjusted_reader_pre(
        &ring,
        &enc_cmt_alice,
        &secret_alice,
        &issuer.pk_issuer,
        &issuer.sk_issuer,
        &spend_ct.epk_1,
        &alice.ack,
        &spend_ct.c2_core,
        &alice_b_d_bytes,
    )?;
    assert_eq!(
        recovered_spend, seed_spend,
        "Spend PRE-recovered seed must match direct derivation"
    );
    let spend_core = decrypt_core_with_seed(recovered_spend, spend_ct)?;
    assert!(spend_core.is_some(), "Spend core should decrypt");
    assert_eq!(
        spend_core.unwrap().amount,
        value.amount,
        "Spend: amount mismatch"
    );

    // Phase 11: Negative tests

    // DLEQ with tampered c
    let bad_c = dleq_core.c + Fq::from(1u64);
    assert!(
        verify_dleq_native(
            &bob.ack,
            &ct.epk_1,
            &s_core,
            &bad_c,
            &dleq_core.s,
            metadata_hash
        )
        .is_err(),
        "Tampered c should fail DLEQ verification"
    );

    // DLEQ with wrong metadata
    let wrong_metadata = compute_metadata_hash(
        Fq::from(99u64),
        Fq::from(2u64),
        Fq::from(3u64),
        Fq::from(1u64),
        Fq::from(1_700_000_000u64),
        Fq::from(0u64),
    );
    assert!(
        verify_dleq_native(
            &bob.ack,
            &ct.epk_1,
            &s_core,
            &dleq_core.c,
            &dleq_core.s,
            wrong_metadata
        )
        .is_err(),
        "Wrong metadata should fail DLEQ verification"
    );

    // Wrong issuer key → adjusted reader PRE produces wrong seed
    {
        let wrong_sk = Fr::rand(&mut OsRng);
        let wrong_pk = decaf377::Element::GENERATOR * wrong_sk;
        // PRE targets wrong_pk, but we try to recover with correct sk_issuer
        let wrong_seed = adjusted_reader_pre(
            &ring,
            &enc_cmt_bob,
            &secret_bob,
            &wrong_pk,
            &issuer.sk_issuer,
            &ct.epk_1,
            &bob.ack,
            &ct.c2_core,
            &bob_b_d_bytes,
        )?;
        let wrong_core = decrypt_core_with_seed(wrong_seed, ct)?;
        if let Some(wc) = wrong_core {
            assert_ne!(
                wc.amount, value.amount,
                "Wrong issuer key should produce wrong amount"
            );
        }
    }

    // Wrong enc_cmt_orbis → adjusted reader key produces wrong xnc_cmt
    {
        let fake_enc_cmt = decaf377::Element::GENERATOR * Fr::rand(&mut OsRng);
        // Use mismatched enc_cmt (fake) with the real dummy_secret (which has the real enc_cmt)
        // This simulates a corrupted or stale enc_cmt_orbis — the cancellation breaks.
        let bad_seed = adjusted_reader_pre(
            &ring,
            &fake_enc_cmt,
            &secret_bob,
            &issuer.pk_issuer,
            &issuer.sk_issuer,
            &ct.epk_1,
            &bob.ack,
            &ct.c2_core,
            &bob_b_d_bytes,
        );
        // Should either error (verify fails) or produce wrong seed
        if let Ok(seed) = bad_seed {
            let bad_core = decrypt_core_with_seed(seed, ct)?;
            if let Some(bc) = bad_core {
                assert_ne!(
                    bc.amount, value.amount,
                    "Wrong enc_cmt should produce wrong amount"
                );
            }
        }
    }

    // FROST signature tampered message
    let tampered_msg = b"tampered-registration-message";
    let registration_msg = build_ack_registration_message(
        &alice.address,
        &alice.ack,
        &ring.ring_pk,
        harness.regulated_token_id,
    );
    let sig_alice = orbis_frost_sign(
        &ring.secret_shares,
        &ring.pub_poly,
        &registration_msg,
        ring.t,
        ring.n,
    )?;
    assert!(
        verify_ack_signature(&ring.ring_pk, tampered_msg, &sig_alice).is_err(),
        "Tampered message should fail FROST verification"
    );

    // Detection tier with wrong DK
    let wrong_dk = Fr::rand(&mut OsRng);
    let wrong_det =
        decrypt_detection_tier(&wrong_dk, &ct.epk_1, &ct.detection_tag, &value.asset_id);
    if let Ok((wrong_asset_id, _, _)) = wrong_det {
        assert_ne!(
            wrong_asset_id, value.asset_id,
            "Wrong DK should fail detection"
        );
    }

    Ok(())
}

// ============================================================================
// Test 2: Flagged flow — direct issuer decryption (no Orbis PRE)
// ============================================================================

#[tokio::test]
async fn test_full_orbis_flow_flagged() -> Result<()> {
    use penumbra_sdk_compliance::crypto::{decrypt_detection_tier, encrypt_output, encrypt_spend};
    use penumbra_sdk_compliance::scanning::{decrypt_core_flagged, decrypt_extension_flagged};

    let _guard = common::set_tracing_subscriber();
    let harness = ComplianceTestHarness::new().await?;
    let ring = create_ring(5, 3)?;
    let issuer = create_issuer();
    harness
        .register_asset(harness.regulated_token_id, &issuer, &ring)
        .await?;

    let alice = register_user(&harness, &ring, harness.regulated_token_id).await?;
    let bob = register_user(&harness, &ring, harness.regulated_token_id).await?;

    let value = Value {
        amount: 1000u64.into(),
        asset_id: harness.regulated_token_id,
    };

    // Encrypt output as flagged
    let output_result = encrypt_output(
        &mut OsRng,
        &bob.ack,
        &alice.ack,
        &issuer.dk_pub,
        &bob.address,
        &alice.address,
        value.asset_id,
        value.amount,
        true, // is_flagged
        Fq::from(0u64),
    )?;
    let ct = &output_result.ciphertext;

    // Issuer decrypts directly using detection key (no PRE needed)
    let core_data = decrypt_core_flagged(&issuer.dk_scalar, ct)?;
    assert!(
        core_data.is_some(),
        "Issuer should decrypt flagged core directly"
    );
    assert_eq!(
        core_data.unwrap().amount,
        value.amount,
        "Amount should match"
    );

    let ext_data = decrypt_extension_flagged(&issuer.dk_scalar, ct)?;
    assert!(
        ext_data.is_some(),
        "Issuer should decrypt flagged ext directly"
    );
    assert_eq!(
        ext_data.unwrap().counterparty_diversified_generator,
        *alice.address.diversified_generator(),
        "Counterparty should be Alice"
    );

    // Detection tier should show flagged
    let det = decrypt_detection_tier(
        &issuer.dk_scalar,
        &ct.epk_1,
        &ct.detection_tag,
        &value.asset_id,
    )?;
    let (det_asset_id, det_flag, _) = det;
    assert_eq!(det_asset_id, value.asset_id);
    assert!(det_flag, "Detection: should be flagged");

    // Spend path flagged
    let spend_result = encrypt_spend(
        &mut OsRng,
        &alice.ack,
        &issuer.dk_pub,
        &alice.address,
        value.asset_id,
        value.amount,
        true,
        Fq::from(0u64),
    )?;
    let spend_ct = &spend_result.ciphertext;
    let spend_core = decrypt_core_flagged(&issuer.dk_scalar, spend_ct)?;
    assert!(
        spend_core.is_some(),
        "Flagged spend should decrypt directly"
    );
    assert_eq!(spend_core.unwrap().amount, value.amount);

    // Wrong DK → wrong decryption
    let wrong_dk = Fr::rand(&mut OsRng);
    let wrong_result = decrypt_core_flagged(&wrong_dk, ct)?;
    if let Some(wrong_core) = wrong_result {
        assert_ne!(
            wrong_core.amount, value.amount,
            "Wrong DK should produce wrong amount"
        );
    }

    Ok(())
}

// ============================================================================
// Test 3: User segregation (merged with key specificity)
// ============================================================================

#[tokio::test]
async fn test_user_segregation() -> Result<()> {
    let _guard = common::set_tracing_subscriber();
    let harness = ComplianceTestHarness::new().await?;

    // Use random per-user ring keys for isolation testing
    let alice_sk_ring = Fr::rand(&mut OsRng);
    let alice_dk_pub = decaf377::Element::GENERATOR * Fr::rand(&mut OsRng);
    let bob_sk_ring = Fr::rand(&mut OsRng);
    let bob_dk_pub = decaf377::Element::GENERATOR * Fr::rand(&mut OsRng);
    let (alice_addr, _) = create_wallet();
    let (bob_addr, _) = create_wallet();

    // Register asset with dummy policy
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

    // Alice (sender) can decrypt her own spend
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

    // Alice cannot decrypt Bob's receiver CT
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

    // Bob can decrypt his own receive
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

    // Bob cannot decrypt Alice's sender CT
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

    // Wrong ring key fails
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

    // Wrong role fails
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
// Test 4: Unregulated asset → BLACK_HOLE_ACK → undecryptable
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
        false, // UNREGULATED
    )?;

    // Nobody can decrypt BLACK_HOLE ciphertexts
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
// Test 5: ZK proof roundtrip (spend + output circuit verification)
// ============================================================================

#[tokio::test]
async fn test_full_compliance_proof_roundtrip() -> Result<()> {
    use penumbra_sdk_mock_client::StateReadComplianceProvider;
    use penumbra_sdk_shielded_pool::{OutputPlan, SpendPlan};

    let _guard = common::set_tracing_subscriber();
    let harness = ComplianceTestHarness::new().await?;

    // Register asset with dummy policy for ZK tests
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

    // Fresh Spend PK/VK
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

    // Fresh Output PK/VK
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
// Test 6: Flagged spend proof roundtrip
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
