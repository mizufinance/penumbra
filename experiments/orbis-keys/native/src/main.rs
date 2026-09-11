//! Synthetic threshold fixtures interoperating with the official vetKeys client.
//! The local dealer and polynomial refresh are not distributed DKG protocols.
use ic_bls12_381::{
    hash_to_curve::{ExpandMsgXmd, HashToCurve},
    pairing, G1Affine, G1Projective, G2Affine, G2Projective, Scalar,
};
use ic_vetkeys::{
    DerivedPublicKey, EncryptedVetKey, IbeCiphertext, IbeIdentity, IbeSeed, TransportSecretKey,
    VetKey,
};
use rand::{rngs::OsRng, RngCore};
use serde_json::json;
use sha2::{Digest, Sha512};
use std::time::Instant;

fn scalar() -> Scalar {
    let mut b = [0; 64];
    OsRng.fill_bytes(&mut b);
    Scalar::from_bytes_wide(&b)
}

fn identity_hash(pk: &G2Affine, id: &[u8]) -> G1Affine {
    let mut input = pk.to_compressed().to_vec();
    input.extend_from_slice(id);
    G1Affine::from(
        <G1Projective as HashToCurve<ExpandMsgXmd<sha2::Sha256>>>::hash_to_curve(
            input,
            b"BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_AUG_",
        ),
    )
}

#[derive(Clone)]
struct Partial {
    index: u64,
    c1: G1Projective,
    c2: G2Projective,
    c3: G1Projective,
}

fn issue(index: u64, share: Scalar, h: G1Affine, reader: G1Affine) -> Partial {
    let r = scalar();
    Partial {
        index,
        c1: G1Affine::generator() * r,
        c2: G2Affine::generator() * r,
        c3: reader * r + h * share,
    }
}

fn verify(p: &Partial, share_pk: G2Affine, h: G1Affine, reader: G1Affine) -> bool {
    pairing(&G1Affine::from(p.c1), &G2Affine::generator())
        == pairing(&G1Affine::generator(), &G2Affine::from(p.c2))
        && pairing(&G1Affine::from(p.c3), &G2Affine::generator())
            == pairing(&reader, &G2Affine::from(p.c2)) + pairing(&h, &share_pk)
}

fn combine(parts: &[Partial]) -> Result<Vec<u8>, &'static str> {
    let mut a = G1Projective::identity();
    let mut b = G2Projective::identity();
    let mut c = G1Projective::identity();
    for p in parts {
        if p.index == 0 || parts.iter().filter(|q| q.index == p.index).count() != 1 {
            return Err("invalid or duplicate node index");
        }
        let i = Scalar::from(p.index);
        let mut weight = Scalar::one();
        for q in parts {
            if q.index != p.index {
                let j = Scalar::from(q.index);
                weight *= j * (j - i).invert().unwrap();
            }
        }
        a += p.c1 * weight;
        b += p.c2 * weight;
        c += p.c3 * weight;
    }
    let mut bytes = G1Affine::from(a).to_compressed().to_vec();
    bytes.extend(G2Affine::from(b).to_compressed());
    bytes.extend(G1Affine::from(c).to_compressed());
    Ok(bytes)
}

fn evaluate(coefficients: &[Scalar], index: u64) -> Scalar {
    coefficients
        .iter()
        .rev()
        .fold(Scalar::zero(), |a, c| a * Scalar::from(index) + c)
}

fn run(n: usize, threshold: usize, batch: usize) {
    let coefficients: Vec<_> = (0..threshold).map(|_| scalar()).collect();
    let shares: Vec<_> = (1..=n).map(|i| evaluate(&coefficients, i as u64)).collect();
    let master_pk = G2Affine::from(G2Affine::generator() * coefficients[0]);
    let dpk = DerivedPublicKey::deserialize(&master_pk.to_compressed()).unwrap();
    let y = scalar();
    let reader = G1Affine::from(G1Affine::generator() * y);
    let tsk = TransportSecretKey::deserialize(&y.to_bytes()).unwrap();
    let plaintext = [42u8; 32];
    let mut encrypt_us = 0;
    let mut issue_us = 0;
    let mut share_verify_us = 0;
    let mut combine_us = 0;
    let mut decrypt_us = 0;
    for j in 0..batch {
        // Synthetic pre-encryption references, not production transaction IDs.
        let id = format!("test-chain/key-v1/output-{j}/named/amount/Alice");
        let now = Instant::now();
        let ct = IbeCiphertext::encrypt(
            &dpk,
            &IbeIdentity::from_bytes(id.as_bytes()),
            &plaintext,
            &IbeSeed::random(&mut OsRng),
        );
        encrypt_us += now.elapsed().as_micros();
        let now = Instant::now();
        let h = identity_hash(&master_pk, id.as_bytes());
        let parts: Vec<_> = (0..threshold)
            .map(|i| issue((i + 1) as u64, shares[i], h, reader))
            .collect();
        issue_us += now.elapsed().as_micros();
        let now = Instant::now();
        for (i, p) in parts.iter().enumerate() {
            assert!(verify(
                p,
                G2Affine::from(G2Affine::generator() * shares[i]),
                h,
                reader
            ));
        }
        share_verify_us += now.elapsed().as_micros();
        let now = Instant::now();
        let encoded = combine(&parts).unwrap();
        combine_us += now.elapsed().as_micros();
        let now = Instant::now();
        let key = EncryptedVetKey::deserialize(&encoded)
            .unwrap()
            .decrypt_and_verify(&tsk, &dpk, id.as_bytes())
            .unwrap();
        assert_eq!(ct.decrypt(&key).unwrap(), plaintext);
        decrypt_us += now.elapsed().as_micros();
        if j == 0 {
            assert_eq!(encoded.len(), 192);
            assert_eq!(ct.serialize().len(), 168);
            for other in [
                id.replace("Alice", "Bob"),
                id.replace("amount", "sender"),
                id.replace("output-0", "output-1"),
                id.replace("named", "general"),
                id.replace("test-chain", "other-chain"),
            ] {
                let other_h = identity_hash(&master_pk, other.as_bytes());
                let other_parts: Vec<_> = (0..threshold)
                    .map(|i| issue((i + 1) as u64, shares[i], other_h, reader))
                    .collect();
                let other_key = EncryptedVetKey::deserialize(&combine(&other_parts).unwrap())
                    .unwrap()
                    .decrypt_and_verify(&tsk, &dpk, other.as_bytes())
                    .unwrap();
                assert!(ct.decrypt(&other_key).is_err());
                // Try the public-hash scalar-ratio transformation that breaks
                // multiplicative child derivation; the stock IBE must reject it.
                let da = Scalar::from_bytes_wide(&Sha512::digest(id.as_bytes()).into());
                let db = Scalar::from_bytes_wide(&Sha512::digest(other.as_bytes()).into());
                let point = G1Affine::from_compressed(other_key.serialize()).unwrap();
                let forged = G1Affine::from(point * (da * db.invert().unwrap()));
                let forged_key = VetKey::deserialize(&forged.to_compressed()).unwrap();
                assert!(ct.decrypt(&forged_key).is_err());
                assert!(EncryptedVetKey::deserialize(&encoded)
                    .unwrap()
                    .decrypt_and_verify(&tsk, &dpk, other.as_bytes())
                    .is_err());
            }
            let wrong_tsk = TransportSecretKey::deserialize(&scalar().to_bytes()).unwrap();
            assert!(EncryptedVetKey::deserialize(&encoded)
                .unwrap()
                .decrypt_and_verify(&wrong_tsk, &dpk, id.as_bytes())
                .is_err());
            let mut altered = ct.serialize();
            *altered.last_mut().unwrap() ^= 1;
            assert!(IbeCiphertext::deserialize(&altered)
                .unwrap()
                .decrypt(&key)
                .is_err());
            let mut bad = parts[0].clone();
            bad.c3 += G1Affine::generator();
            assert!(!verify(
                &bad,
                G2Affine::from(G2Affine::generator() * shares[0]),
                h,
                reader
            ));
            assert!(combine(&[parts[0].clone(), parts[0].clone()]).is_err());
            assert!(
                EncryptedVetKey::deserialize(&combine(&parts[..threshold - 1]).unwrap())
                    .unwrap()
                    .decrypt_and_verify(&tsk, &dpk, id.as_bytes())
                    .is_err()
            );
            let mut zero_poly: Vec<_> = (0..threshold).map(|_| scalar()).collect();
            zero_poly[0] = Scalar::zero();
            let refreshed: Vec<_> = (0..threshold)
                .map(|i| {
                    issue(
                        (i + 1) as u64,
                        shares[i] + evaluate(&zero_poly, (i + 1) as u64),
                        h,
                        reader,
                    )
                })
                .collect();
            let new_key = EncryptedVetKey::deserialize(&combine(&refreshed).unwrap())
                .unwrap()
                .decrypt_and_verify(&tsk, &dpk, id.as_bytes())
                .unwrap();
            assert_eq!(ct.decrypt(&new_key).unwrap(), plaintext);
            // Exercise a different authorized subset, not just the first nodes.
            let alternate: Vec<_> = (n - threshold..n)
                .map(|i| issue((i + 1) as u64, shares[i], h, reader))
                .collect();
            let alt_key = EncryptedVetKey::deserialize(&combine(&alternate).unwrap())
                .unwrap()
                .decrypt_and_verify(&tsk, &dpk, id.as_bytes())
                .unwrap();
            assert_eq!(ct.decrypt(&alt_key).unwrap(), plaintext);
        }
    }
    println!(
        "{}",
        json!({"case":"vetkeys_native", "nodes":n,"threshold":threshold,"batch":batch,
        "encrypt_us":encrypt_us,"issue_us":issue_us,"verify_shares_us":share_verify_us,
        "combine_us":combine_us,"client_verify_decrypt_us":decrypt_us,
        "ibe_bytes":168,"encrypted_key_bytes":192,"negative_tests":"passed",
        "fixture":"local Shamir dealer, not distributed DKG","transport":"in-process; no network"})
    );
}

fn main() {
    for (n, t) in [(3, 2), (5, 3), (7, 5)] {
        for batch in [1, 8, 32] {
            run(n, t, batch);
        }
    }
}
