use rand_core::{CryptoRng, RngCore};

use super::{AddressIndex, Diversifier, DiversifierKey};
use crate::{ka, Address};

pub const IVK_LEN_BYTES: usize = 64;
/// Allows viewing incoming notes, i.e., notes sent to the spending key this
/// key is derived from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IncomingViewingKey {
    pub(super) ivk: ka::Secret,
    pub(super) dk: DiversifierKey,
}

impl IncomingViewingKey {
    /// Derive a shielded payment address with the given [`AddressIndex`].
    pub fn payment_address(&self, index: AddressIndex) -> Address {
        let d = self.dk.diversifier_for_index(&index);
        let g_d = d.diversified_generator();
        let pk_d = self.ivk.diversified_public(&g_d);

        Address::from_components(d, pk_d).expect("pk_d is valid")
    }

    /// Derive the (encoding of the) transparent address for the given IVK.
    ///
    /// This intentionally returns a `String` rather than an `Address`, as it's not
    /// safe to truncate arbitrary addresses.
    pub fn transparent_address(&self) -> String {
        // The transparent address uses an all-zero diversifier.
        let dzero = Diversifier([0u8; 16]);
        let g_dzero = dzero.diversified_generator();
        let pk_dzero = self.ivk.diversified_public(&g_dzero);
        let address = Address::from_components(dzero, pk_dzero).expect("valid address");

        // This should never fail as we just constructed a valid transparent address
        address
            .encode_as_transparent_address()
            .expect("address meets transparent requirements")
    }

    /// Derive an ephemeral address for the provided account.
    pub fn ephemeral_address<R: RngCore + CryptoRng>(
        &self,
        mut rng: R,
        mut address_index: AddressIndex,
    ) -> Address {
        let mut random_index = [0u8; 12];

        rng.fill_bytes(&mut random_index);

        address_index.randomizer = random_index;

        self.payment_address(address_index)
    }

    /// Perform key agreement with a given public key.
    pub fn key_agreement_with(&self, pk: &ka::Public) -> Result<ka::SharedSecret, ka::Error> {
        self.ivk.key_agreement_with(pk)
    }

    /// Perform key agreement with a compressed Decaf377 point.
    pub fn key_agreement_with_element(
        &self,
        point: decaf377::Element,
    ) -> Result<[u8; 32], ka::Error> {
        self.ivk
            .key_agreement_with(&ka::Public(point.vartime_compress().0))
            .map(|shared| shared.0)
    }

    /// Derive a transmission key from the given diversified base.
    pub fn diversified_public(&self, diversified_generator: &decaf377::Element) -> ka::Public {
        self.ivk.diversified_public(diversified_generator)
    }

    /// Returns the index used to create the given diversifier (if it was
    /// created using this incoming viewing key)
    pub fn index_for_diversifier(&self, diversifier: &Diversifier) -> AddressIndex {
        self.dk.index_for_diversifier(diversifier)
    }

    /// Check whether this address is viewable by this incoming viewing key.
    pub fn views_address(&self, address: &Address) -> bool {
        self.ivk.diversified_public(address.diversified_generator()) == *address.transmission_key()
    }

    /// Returns the index of the given address, if the address is viewed by this
    /// viewing key; otherwise, returns `None`.
    // TODO: re-evaluate relative to FVK methods
    pub(super) fn address_index(&self, address: &Address) -> Option<AddressIndex> {
        if self.views_address(address) {
            Some(self.index_for_diversifier(address.diversifier()))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod test {
    use crate::{
        keys::{Bip44Path, SeedPhrase, SpendKey},
        test_keys,
    };
    use proptest::prelude::*;
    use std::str::FromStr;

    use super::*;

    #[test]
    fn transparent_address_generation_and_parsing() {
        // Use test seed phrase for test vector
        let seed_phrase = SeedPhrase::from_str(test_keys::SEED_PHRASE).expect("valid seed phrase");
        let spend_key = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0))
            .expect("test spend key satisfies key refinements");
        let ivk = spend_key.full_viewing_key().incoming();

        let transparent_address_str = ivk.transparent_address();

        let reconstructed: Address = transparent_address_str
            .parse()
            .expect("can parse transparent address");

        assert!(ivk.views_address(&reconstructed));

        let address_index = ivk.address_index(&reconstructed).expect("views address");

        let actual_address = ivk.payment_address(address_index);

        // The diversifiers will not match, as the encryption of the 0 account `AddressIndex`
        // is not the null ciphertext, so when deriving `actual_address` from the 0 account
        // `AddressIndex`, we end up with a different diversifier.
        assert_ne!(reconstructed.diversifier(), actual_address.diversifier());
        // The transmission keys also will not match, as the null diversifier is not the
        // same as the diversifier for the 0 account `AddressIndex`.
        assert_ne!(
            reconstructed.transmission_key(),
            actual_address.transmission_key()
        );
        println!("Transparent address: {}", transparent_address_str);
        println!("Reconstructed address: {}", reconstructed);
        println!("Address index: {:?}", address_index);
        println!("Actual address for index: {}", actual_address);
    }

    #[test]
    fn views_address_succeeds_on_own_address() {
        let rng = rand::rngs::OsRng;
        let spend_key =
            SpendKey::from_seed_phrase_bip44(SeedPhrase::generate(rng), &Bip44Path::new(0))
                .expect("generated spend key satisfies key refinements");
        let ivk = spend_key.full_viewing_key().incoming();
        let own_address = ivk.payment_address(AddressIndex::from(0u32));
        assert!(ivk.views_address(&own_address));
    }

    proptest! {
        #[test]
        fn views_address_succeeds_on_own_ephemeral_address(address_index in any::<u32>()) {
            let rng = rand::rngs::OsRng;
            let spend_key = SpendKey::from_seed_phrase_bip44(SeedPhrase::generate(rng), &Bip44Path::new(0))
                .expect("generated spend key satisfies key refinements");
            let fvk = spend_key.full_viewing_key();
            let own_address = fvk.ephemeral_address(rng, AddressIndex::from(address_index));
            let ivk = fvk.incoming();
            assert!(ivk.views_address(&own_address));

            let derived_address_index = fvk.address_index(&own_address);
            assert_eq!(derived_address_index.expect("index exists").account, AddressIndex::from(address_index).account);
        }
    }

    #[test]
    fn views_address_fails_on_other_address() {
        let rng = rand::rngs::OsRng;
        let spend_key =
            SpendKey::from_seed_phrase_bip44(SeedPhrase::generate(rng), &Bip44Path::new(0))
                .expect("generated spend key satisfies key refinements");
        let ivk = spend_key.full_viewing_key().incoming();

        let other_address =
            SpendKey::from_seed_phrase_bip44(SeedPhrase::generate(rng), &Bip44Path::new(0))
                .expect("generated spend key satisfies key refinements")
                .full_viewing_key()
                .incoming()
                .payment_address(AddressIndex::from(0u32));

        assert!(!ivk.views_address(&other_address));
    }
}
