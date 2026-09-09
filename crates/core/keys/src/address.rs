//! [Payment address][Address] facilities.

use std::{
    fmt::Display,
    io::{Cursor, Read, Write},
    sync::OnceLock,
};

use anyhow::Context;
use decaf377::{Element, Encoding, Fq};
use f4jumble::{f4jumble, f4jumble_inv};
use rand::{CryptoRng, Rng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use shieldd_sdk_proto::{serializers::bech32str, shieldd::core::keys::v1 as pb, DomainType};

mod view;
pub use view::AddressView;

use crate::{ka, keys::Diversifier};

pub const TRANSPARENT_ADDRESS_BECH32_PREFIX: &str = "tshieldd";

/// The length of an [`Address`] in bytes.
pub const ADDRESS_LEN_BYTES: usize = 48;

/// Number of bits in the address short form divided by the number of bits per Bech32m character
pub const ADDRESS_NUM_CHARS_SHORT_FORM: usize = 24;

/// A valid payment address.
#[derive(Clone, Eq, Serialize, Deserialize)]
#[serde(try_from = "pb::Address", into = "pb::Address")]
pub struct Address {
    /// The address diversifier.
    d: Diversifier,
    /// A cached copy of the diversified base.
    g_d: OnceLock<decaf377::Element>,

    /// The public key for this payment address.
    ///
    /// The bytes are a canonical field encoding of a valid, nonidentity
    /// Decaf377 point, so note commitments can hash its s-coordinate safely.
    pk_d: ka::Public,
    /// The transmission key s value.
    transmission_key_s: Fq,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AddressError {
    #[error("diversified generator is the Decaf377 identity")]
    IdentityDiversifiedGenerator,
    #[error("transmission key has a noncanonical field encoding")]
    NoncanonicalTransmissionKey,
    #[error("transmission key is not a valid Decaf377 point")]
    InvalidTransmissionKey,
    #[error("transmission key is the Decaf377 identity")]
    IdentityTransmissionKey,
}

impl std::cmp::PartialEq for Address {
    fn eq(
        &self,
        rhs @ Self {
            d: rhs_d,
            g_d: rhs_g_d,
            pk_d: rhs_pk_d,
            transmission_key_s: rhs_transmission_key_s,
        }: &Self,
    ) -> bool {
        let lhs @ Self {
            d: lhs_d,
            g_d: lhs_g_d,
            pk_d: lhs_pk_d,
            transmission_key_s: lhs_transmission_key_s,
        } = self;

        // When a `OnceLock<T>` value is compared, it will only call `get()`, refraining from
        // initializing the value. To make sure that an address that *hasn't* yet accessed its
        // diversified base is considered equal to an address that *has*, compute the base points
        // if they have not already been generated.
        lhs.diversified_generator();
        rhs.diversified_generator();

        // Compare all of the fields.
        lhs_d.eq(rhs_d)
            && lhs_g_d.eq(rhs_g_d)
            && lhs_pk_d.eq(rhs_pk_d)
            && lhs_transmission_key_s.eq(rhs_transmission_key_s)
    }
}

impl std::cmp::PartialOrd for Address {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.to_vec().cmp(&other.to_vec()))
    }
}

impl std::cmp::Ord for Address {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.to_vec().cmp(&other.to_vec())
    }
}

impl std::hash::Hash for Address {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.to_vec().hash(state)
    }
}

impl Address {
    /// Constructs a payment address from its components.
    ///
    /// Rejects identity generators, invalid points, and noncanonical or
    /// identity transmission keys.
    pub fn from_components(d: Diversifier, pk_d: ka::Public) -> Result<Self, AddressError> {
        let diversified_generator = d.diversified_generator();
        Self::from_components_with_diversified_generator(d, diversified_generator, pk_d)
    }

    fn from_components_with_diversified_generator(
        d: Diversifier,
        diversified_generator: Element,
        pk_d: ka::Public,
    ) -> Result<Self, AddressError> {
        if diversified_generator == Element::default() {
            return Err(AddressError::IdentityDiversifiedGenerator);
        }
        let transmission_key_s = Fq::from_bytes_checked(&pk_d.0)
            .map_err(|_| AddressError::NoncanonicalTransmissionKey)?;
        let transmission_key = Encoding(pk_d.0)
            .vartime_decompress()
            .map_err(|_| AddressError::InvalidTransmissionKey)?;
        if transmission_key == Element::default() {
            return Err(AddressError::IdentityTransmissionKey);
        }
        Ok(Self {
            d,
            g_d: OnceLock::from(diversified_generator),
            pk_d,
            transmission_key_s,
        })
    }

    /// Returns a reference to the address diversifier.
    pub fn diversifier(&self) -> &Diversifier {
        &self.d
    }

    /// Returns a reference to the diversified base.
    ///
    /// This method computes the diversified base if it has not been computed yet. This value is
    /// cached after it has been computed once.
    pub fn diversified_generator(&self) -> &decaf377::Element {
        self.g_d
            .get_or_init(|| self.diversifier().diversified_generator())
    }

    /// Returns a reference to the transmission key.
    pub fn transmission_key(&self) -> &ka::Public {
        &self.pk_d
    }

    /// Returns a reference to the transmission key `s` value.
    pub fn transmission_key_s(&self) -> &Fq {
        &self.transmission_key_s
    }

    /// Converts this address to a vector of bytes.
    pub fn to_vec(&self) -> Vec<u8> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        bytes
            .write_all(&self.diversifier().0)
            .expect("can write diversifier into vec");
        bytes
            .write_all(&self.transmission_key().0)
            .expect("can write transmission key into vec");
        f4jumble(bytes.get_ref()).expect("can jumble")
    }

    /// Generates a randomized dummy address.
    pub fn dummy<R: CryptoRng + Rng>(rng: &mut R) -> Self {
        loop {
            let mut diversifier_bytes = [0u8; 16];
            rng.fill_bytes(&mut diversifier_bytes);

            let mut pk_d_bytes = [0u8; 32];
            rng.fill_bytes(&mut pk_d_bytes);

            let diversifier = Diversifier(diversifier_bytes);
            let addr = Address::from_components(diversifier, ka::Public(pk_d_bytes));

            if let Ok(addr) = addr {
                return addr;
            }
        }
    }

    /// Short form suitable for displaying in a UI.
    pub fn display_short_form(&self) -> String {
        let full_address = format!("{self}");
        // Fixed prefix is `shielddv2t` plus the Bech32m separator `1`.
        let fixed_prefix = format!("{}{}", bech32str::address::BECH32_PREFIX, '1');
        let num_chars_to_display = fixed_prefix.len() + ADDRESS_NUM_CHARS_SHORT_FORM;

        format!("{}…", &full_address[0..num_chars_to_display])
    }

    /// Generate a Noble forwarding address.
    pub fn noble_forwarding_address(&self, channel: &str) -> NobleForwardingAddress {
        NobleForwardingAddress {
            channel: channel.to_string(),
            recipient: format!("{}", self),
        }
    }

    /// Encodes the address as a transparent address if it has the reserved zero diversifier.
    /// Returns `None` if the address doesn't meet the requirements for a transparent address.
    pub fn encode_as_transparent_address(&self) -> Option<String> {
        // Check if diversifier is zero
        if self.diversifier().0 != [0u8; 16] {
            return None;
        }

        // Encode the transmission key.
        Some(bech32str::encode(
            &self.transmission_key().0,
            TRANSPARENT_ADDRESS_BECH32_PREFIX,
            bech32str::Bech32,
        ))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NobleForwardingAddress {
    pub channel: String,
    pub recipient: String,
}

impl NobleForwardingAddress {
    pub fn bytes(&self) -> Vec<u8> {
        // Based on https://github.com/noble-assets/forwarding/blob/9d7657a89a5d4c68a082f28629e759b60b39b0fd/types/account.go#L17
        let channel = self.channel.clone();
        let recipient = self.recipient.clone();
        let bz = format!("{channel}{recipient}").as_bytes().to_owned();
        let th = Sha256::digest("forwarding".as_bytes());
        let mut hasher = Sha256::new();
        hasher.update(th);
        hasher.update(bz);

        // This constructs the account bytes for the Noble forwarding address
        // Only use bytes 12 and on:
        hasher.finalize()[12..].to_vec()
    }
}

impl Display for NobleForwardingAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let addr_bytes = &self.bytes();

        write!(
            f,
            "{}",
            bech32str::encode(&addr_bytes, "noble", bech32str::Bech32)
        )
    }
}

impl DomainType for Address {
    type Proto = pb::Address;
}

impl From<Address> for pb::Address {
    fn from(a: Address) -> Self {
        Self::from(&a)
    }
}

impl From<&Address> for pb::Address {
    fn from(a: &Address) -> Self {
        pb::Address {
            inner: a.to_vec(),
            // Always produce encodings without the alt format.
            alt_bech32m: String::new(),
        }
    }
}

impl TryFrom<pb::Address> for Address {
    type Error = anyhow::Error;

    fn try_from(value: pb::Address) -> Result<Self, Self::Error> {
        match (value.inner.is_empty(), value.alt_bech32m.is_empty()) {
            (false, true) => value.inner.try_into(),
            (true, false) => value.alt_bech32m.parse(),
            (false, false) => Err(anyhow::anyhow!(
                "Address proto has both inner and alt_bech32m fields set"
            )),
            (true, true) => Err(anyhow::anyhow!(
                "Address proto has neither inner nor alt_bech32m fields set"
            )),
        }
    }
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let proto_address = pb::Address::from(self);
        f.write_str(&bech32str::encode(
            &proto_address.inner,
            bech32str::address::BECH32_PREFIX,
            bech32str::Bech32m,
        ))
    }
}

impl std::fmt::Debug for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        <Self as std::fmt::Display>::fmt(self, f)
    }
}

impl std::str::FromStr for Address {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with(TRANSPARENT_ADDRESS_BECH32_PREFIX) {
            let dzero = Diversifier([0u8; 16]);

            let pk_dzero_bytes: [u8; 32] =
                bech32str::decode(s, TRANSPARENT_ADDRESS_BECH32_PREFIX, bech32str::Bech32)?
                    .try_into()
                    .map_err(|bytes: Vec<u8>| {
                        anyhow::anyhow!("wrong length {}, expected 32", bytes.len())
                    })?;
            let pk_dzero = ka::Public(pk_dzero_bytes);

            let address = Self::from_components(dzero, pk_dzero)
                .context("could not reconstruct transparent address")?;

            // Verify this is a valid transparent address, bailing if not
            if address.encode_as_transparent_address().is_none() {
                return Err(anyhow::anyhow!("invalid transparent address components"));
            }

            Ok(address)
        } else {
            pb::Address {
                inner: bech32str::decode(s, bech32str::address::BECH32_PREFIX, bech32str::Bech32m)?,
                alt_bech32m: String::new(),
            }
            .try_into()
        }
    }
}

impl TryFrom<Vec<u8>> for Address {
    type Error = anyhow::Error;

    fn try_from(jumbled_vec: Vec<u8>) -> Result<Self, Self::Error> {
        (&jumbled_vec[..]).try_into()
    }
}

impl TryFrom<&Vec<u8>> for Address {
    type Error = anyhow::Error;

    fn try_from(jumbled_vec: &Vec<u8>) -> Result<Self, Self::Error> {
        (jumbled_vec[..]).try_into()
    }
}

impl TryFrom<&[u8]> for Address {
    type Error = anyhow::Error;

    fn try_from(jumbled_bytes: &[u8]) -> Result<Self, Self::Error> {
        if jumbled_bytes.len() != ADDRESS_LEN_BYTES {
            anyhow::bail!("address malformed");
        }

        let unjumbled_bytes = f4jumble_inv(jumbled_bytes).context("invalid address")?;
        let mut bytes = Cursor::new(unjumbled_bytes);

        let mut diversifier_bytes = [0u8; 16];
        bytes
            .read_exact(&mut diversifier_bytes)
            .context("could not read diversifier bytes")?;

        let mut pk_d_bytes = [0u8; 32];
        bytes
            .read_exact(&mut pk_d_bytes)
            .context("could not read transmission key bytes")?;

        let diversifier = Diversifier(diversifier_bytes);

        Address::from_components(diversifier, ka::Public(pk_d_bytes))
            .context("could not create address from components")
    }
}

/// Assert the addresses are both [`Send`] and [`Sync`].
//  NB: allow dead code, because this block only contains compile-time assertions.
#[allow(dead_code)]
mod assert_address_is_send_and_sync {
    fn is_send<T: Send>() {}
    fn is_sync<T: Sync>() {}
    fn f() {
        is_send::<super::Address>();
        is_sync::<super::Address>();
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use rand_core::OsRng;

    use super::*;
    use crate::keys::{Bip44Path, SeedPhrase, SpendKey};

    #[test]
    fn test_address_encoding() {
        let rng = OsRng;
        let seed_phrase = SeedPhrase::generate(rng);
        let sk = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0))
            .expect("generated spend key satisfies key refinements");
        let fvk = sk.full_viewing_key();
        let ivk = fvk.incoming();
        let dest = ivk.payment_address(0u32.into());

        let bech32m_addr = format!("{dest}");

        let addr = Address::from_str(&bech32m_addr).expect("can decode valid address");

        use shieldd_sdk_proto::Message;

        let proto_addr = dest.encode_to_vec();
        let proto_addr_bech32m = pb::Address {
            inner: Vec::new(),
            alt_bech32m: bech32m_addr,
        }
        .encode_to_vec();
        let proto_addr_direct: pb::Address = dest.clone().into();
        let addr_from_proto: Address = proto_addr_direct
            .try_into()
            .expect("can convert from proto back to address");

        let addr2 = Address::decode(proto_addr.as_ref()).expect("can decode valid address");
        let addr3 = Address::decode(proto_addr_bech32m.as_ref()).expect("can decode valid address");

        assert_eq!(addr, dest);
        assert_eq!(addr2, dest);
        assert_eq!(addr3, dest);
        assert_eq!(addr_from_proto, dest);
        let raw: Address = dest.to_vec().try_into().expect("canonical raw address");
        assert_eq!(raw, dest);
    }

    #[test]
    fn rejects_unsupported_80_byte_address() {
        let bytes = vec![0u8; 80];
        assert!(Address::try_from(bytes).is_err());
    }

    #[test]
    fn test_address_keys_are_diversified() {
        let rng = OsRng;
        let seed_phrase = SeedPhrase::generate(rng);
        let sk = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0))
            .expect("generated spend key satisfies key refinements");
        let fvk = sk.full_viewing_key();
        let ivk = fvk.incoming();
        let dest1 = ivk.payment_address(0u32.into());
        let dest2 = ivk.payment_address(1u32.into());

        assert!(dest1.transmission_key() != dest2.transmission_key());
    }

    #[test]
    fn address_components_reject_identity_diversified_generator() {
        let known = crate::test_keys::ADDRESS_0.clone();
        assert_eq!(
            Address::from_components_with_diversified_generator(
                *known.diversifier(),
                Element::default(),
                *known.transmission_key(),
            ),
            Err(AddressError::IdentityDiversifiedGenerator)
        );
    }

    #[test]
    fn address_components_reject_identity_or_invalid_transmission_key() {
        let known = crate::test_keys::ADDRESS_0.clone();
        let identity = ka::Public(Element::default().vartime_compress().0);
        assert_eq!(
            Address::from_components(*known.diversifier(), identity),
            Err(AddressError::IdentityTransmissionKey)
        );

        let invalid_point = (0u64..)
            .map(Fq::from)
            .map(|field| ka::Public(field.to_bytes()))
            .find(|candidate| Encoding(candidate.0).vartime_decompress().is_err())
            .expect("some canonical field encodings are not valid Decaf377 points");
        assert_eq!(
            Address::from_components(*known.diversifier(), invalid_point),
            Err(AddressError::InvalidTransmissionKey)
        );
    }
}
