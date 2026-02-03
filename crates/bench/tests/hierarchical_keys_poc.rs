//! POC: Hierarchical Key Derivation for Compliance
//!
//! Key hierarchy:
//!   MK (Orbis ring) → UK = Hash(MK, user_id) → AK = UK * B_d → DK = AK + T * B_d
//!
//! Demonstrates:
//! - Client knows AK (public, from registry) but NOT UK
//! - Client derives daily public key: DK = AK + T * B_d
//! - Orbis knows UK and decrypts via: dk = UK + T, S = dk * EPK
//!
//! Math equivalence:
//!   Client:  S = r * DK = r * (AK + T * B_d) = r * (UK + T) * B_d
//!   Orbis:   S = dk * EPK = (UK + T) * r * B_d
//! Both equal r * (UK + T) * B_d ✓

use decaf377::{Element, Fr};
use penumbra_sdk_keys::keys::{derive_daily_tweak, Diversifier, KeyType};
use rand_core::{CryptoRngCore, OsRng};
use sha2::{Digest, Sha512};

/// MK - Master Key (Orbis ring, scalar)
struct MasterKey(Fr);

/// UK - User Key (Orbis only, scalar)
struct UserKey(Fr);

/// AK - Address Key (public, stored in registry)
/// AK = UK * B_d
struct AddressKey(Element);

impl MasterKey {
    fn random(rng: &mut impl CryptoRngCore) -> Self {
        Self(Fr::rand(rng))
    }

    /// UK = Hash(MK, user_id)
    fn derive_user_key(&self, user_id: &[u8]) -> UserKey {
        let mut hasher = Sha512::new();
        hasher.update(b"penumbra_uk_derivation");
        hasher.update(self.0.to_bytes());
        hasher.update(user_id);
        let hash = hasher.finalize();
        UserKey(Fr::from_le_bytes_mod_order(&hash))
    }
}

impl UserKey {
    /// AK = UK * B_d
    /// Computed by Orbis and stored in the public registry
    fn derive_address_key(&self, diversifier: &Diversifier) -> AddressKey {
        AddressKey(diversifier.diversified_generator() * self.0)
    }

    /// dk = UK + T (daily scalar for decryption)
    /// Used by Orbis to decrypt
    fn derive_daily_scalar(&self, key_type: KeyType, date: u64) -> Fr {
        let tweak = derive_daily_tweak(key_type, date);
        self.0 + tweak
    }
}

impl AddressKey {
    /// DK = AK + T * B_d (daily public key for encryption)
    /// This is what the CLIENT uses - no UK needed!
    fn derive_daily_key(&self, diversifier: &Diversifier, key_type: KeyType, date: u64) -> Element {
        let tweak = derive_daily_tweak(key_type, date);
        let b_d = diversifier.diversified_generator();
        // DK = AK + T * B_d
        self.0 + (b_d * tweak)
    }
}

fn encrypt(plaintext: &[u8], shared_secret: &Element) -> Vec<u8> {
    let key = shared_secret.vartime_compress().0;
    plaintext
        .iter()
        .enumerate()
        .map(|(i, &b)| b ^ key[i % 32])
        .collect()
}

fn decrypt(ciphertext: &[u8], shared_secret: &Element) -> Vec<u8> {
    encrypt(ciphertext, shared_secret)
}

#[test]
fn test_client_encrypts_without_uk() {
    let mut rng = OsRng;

    // === ORBIS SETUP (knows MK, UK) ===
    let mk = MasterKey::random(&mut rng);
    let uk = mk.derive_user_key(b"alice");

    // Two addresses with different diversifiers
    let div1 = Diversifier([1u8; 16]);
    let div2 = Diversifier([2u8; 16]);

    // Orbis computes AKs and registers them publicly
    let ak1 = uk.derive_address_key(&div1); // AK1 = UK * B_d1
    let ak2 = uk.derive_address_key(&div2); // AK2 = UK * B_d2

    // === CLIENT ENCRYPTION (does NOT know UK, only AK from registry) ===
    let b_d1 = div1.diversified_generator();
    let b_d2 = div2.diversified_generator();

    // Client encrypts for address 1 using only public data
    let msg1 = b"Hello from address 1";
    let r1 = Fr::rand(&mut rng);
    let epk1 = b_d1 * r1; // EPK = r * B_d
    let ss_sender1 = ak1.0 * r1; // S = r * AK (no UK needed!)
    let ct1 = encrypt(msg1, &ss_sender1);

    // Client encrypts for address 2
    let msg2 = b"Hello from address 2";
    let r2 = Fr::rand(&mut rng);
    let epk2 = b_d2 * r2;
    let ss_sender2 = ak2.0 * r2;
    let ct2 = encrypt(msg2, &ss_sender2);

    // === ORBIS DECRYPTION (uses UK) ===
    let ss_orbis1 = epk1 * uk.0; // S = UK * EPK = UK * r * B_d
    let ss_orbis2 = epk2 * uk.0;

    let decrypted1 = decrypt(&ct1, &ss_orbis1);
    let decrypted2 = decrypt(&ct2, &ss_orbis2);

    println!(
        "Decrypted 1: {}",
        String::from_utf8(decrypted1.clone()).unwrap()
    );
    println!(
        "Decrypted 2: {}",
        String::from_utf8(decrypted2.clone()).unwrap()
    );

    assert_eq!(decrypted1, msg1);
    assert_eq!(decrypted2, msg2);
}

#[test]
fn test_client_encrypts_with_daily_keys() {
    let mut rng = OsRng;

    // === ORBIS SETUP ===
    let mk = MasterKey::random(&mut rng);
    let uk = mk.derive_user_key(b"alice");

    let div1 = Diversifier([1u8; 16]);
    let div2 = Diversifier([2u8; 16]);
    let b_d1 = div1.diversified_generator();
    let b_d2 = div2.diversified_generator();

    // Orbis registers AKs publicly
    let ak1 = uk.derive_address_key(&div1);
    let ak2 = uk.derive_address_key(&div2);

    let date = 19000u64;
    let key_type = KeyType::Core;

    // === CLIENT ENCRYPTION (uses AK + public tweak, NOT UK) ===
    // Client derives daily public key from AK: DK = AK + T * B_d
    let dk1 = ak1.derive_daily_key(&div1, key_type, date);
    let dk2 = ak2.derive_daily_key(&div2, key_type, date);

    let msg1 = b"Hello from address 1 (day 19000, core)";
    let r1 = Fr::rand(&mut rng);
    let epk1 = b_d1 * r1;
    let ss_sender1 = dk1 * r1; // S = r * DK = r * (AK + T * B_d)
    let ct1 = encrypt(msg1, &ss_sender1);

    let msg2 = b"Hello from address 2 (day 19000, core)";
    let r2 = Fr::rand(&mut rng);
    let epk2 = b_d2 * r2;
    let ss_sender2 = dk2 * r2;
    let ct2 = encrypt(msg2, &ss_sender2);

    // === ORBIS DECRYPTION (uses UK + tweak) ===
    // Orbis computes daily scalar: dk = UK + T
    let dk_scalar = uk.derive_daily_scalar(key_type, date);
    let ss_orbis1 = epk1 * dk_scalar; // S = dk * EPK = (UK + T) * r * B_d
    let ss_orbis2 = epk2 * dk_scalar;

    let decrypted1 = decrypt(&ct1, &ss_orbis1);
    let decrypted2 = decrypt(&ct2, &ss_orbis2);

    println!(
        "Decrypted 1: {}",
        String::from_utf8(decrypted1.clone()).unwrap()
    );
    println!(
        "Decrypted 2: {}",
        String::from_utf8(decrypted2.clone()).unwrap()
    );

    assert_eq!(decrypted1, msg1);
    assert_eq!(decrypted2, msg2);

    // === VERIFY MATH EQUIVALENCE ===
    // Show that AK-based and UK-based derivation produce the same DK
    let tweak = derive_daily_tweak(key_type, date);

    // Method 1: DK = AK + T * B_d (what client computes)
    let dk_from_ak = ak1.0 + (b_d1 * tweak);

    // Method 2: DK = (UK + T) * B_d (what Orbis could compute)
    let dk_from_uk = b_d1 * (uk.0 + tweak);

    assert_eq!(dk_from_ak, dk_from_uk, "Both methods produce same DK");
}
