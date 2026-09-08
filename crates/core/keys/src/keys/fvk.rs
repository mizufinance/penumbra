use anyhow::Context;
use ark_ff::Zero;
use ark_serialize::CanonicalDeserialize;
use decaf377::{Element, Fq, Fr};
use once_cell::sync::Lazy;
use poseidon377::hash_2;
use rand_core::{CryptoRng, RngCore};
use serde::{Deserialize, Serialize};

use shieldd_sdk_proto::{serializers::bech32str, shieldd::core::keys::v1 as pb, DomainType};

use crate::keys::wallet_id::WalletId;
use crate::PositionMetadataKey;
use crate::{
    ka, prf,
    rdsa::{SpendAuth, VerificationKey},
    Address, AddressView, BackreferenceKey,
};

use super::{AddressIndex, DiversifierKey, IncomingViewingKey, NullifierKey, OutgoingViewingKey};

pub(crate) static IVK_DOMAIN_SEP: Lazy<Fq> =
    Lazy::new(|| Fq::from_le_bytes_mod_order(b"shieldd.derive.ivk"));

static ACCOUNT_ID_DOMAIN_SEP: Lazy<Fq> =
    Lazy::new(|| Fq::from_le_bytes_mod_order(b"Shieldd_HashFVK"));

/// The root viewing capability for all data related to a given spend authority.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "pb::FullViewingKey", into = "pb::FullViewingKey")]
pub struct FullViewingKey {
    ak: VerificationKey<SpendAuth>,
    nk: NullifierKey,
    ovk: OutgoingViewingKey,
    ivk: IncomingViewingKey,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum FullViewingKeyError {
    #[error("spend authorization key has an invalid Decaf377 encoding")]
    InvalidAuthorizationKeyEncoding,
    #[error("spend authorization key is the Decaf377 identity")]
    IdentityAuthorizationKey,
    #[error("derived incoming viewing key reduces to zero")]
    ZeroIncomingViewingKey,
}

impl FullViewingKey {
    /// Derive a shielded payment address with the given [`AddressIndex`].
    pub fn payment_address(&self, index: AddressIndex) -> Address {
        self.incoming().payment_address(index)
    }

    /// Derive a random ephemeral address.
    pub fn ephemeral_address<R: RngCore + CryptoRng>(
        &self,
        rng: R,
        address_index: AddressIndex,
    ) -> Address {
        self.incoming().ephemeral_address(rng, address_index)
    }

    /// Views the structure of the supplied address with this viewing key.
    pub fn view_address(&self, address: Address) -> AddressView {
        // The IVK cannot supply the wallet ID carried by a decoded address.
        if self.incoming().views_address(&address) {
            AddressView::Decoded {
                index: self.incoming().index_for_diversifier(address.diversifier()),
                wallet_id: self.wallet_id(),
                address,
            }
        } else {
            AddressView::Opaque { address }
        }
    }

    /// Returns the index of the given address, if the address is viewed by this
    /// viewing key; otherwise, returns `None`.
    pub fn address_index(&self, address: &Address) -> Option<AddressIndex> {
        self.incoming().address_index(address)
    }

    /// Construct a full viewing key from its components.
    pub fn from_components(
        ak: VerificationKey<SpendAuth>,
        nk: NullifierKey,
    ) -> Result<Self, FullViewingKeyError> {
        let ak_s = Fq::from_bytes_checked(ak.as_ref())
            .map_err(|_| FullViewingKeyError::InvalidAuthorizationKeyEncoding)?;
        let ivk_mod_q = poseidon377::hash_2(&IVK_DOMAIN_SEP, (nk.0, ak_s));
        Self::from_components_with_ivk_hash(ak, nk, ivk_mod_q)
    }

    fn from_components_with_ivk_hash(
        ak: VerificationKey<SpendAuth>,
        nk: NullifierKey,
        ivk_mod_q: Fq,
    ) -> Result<Self, FullViewingKeyError> {
        if ak.as_ref() == &Element::default().vartime_compress().0 {
            return Err(FullViewingKeyError::IdentityAuthorizationKey);
        }

        let ovk = {
            let hash_result = prf::expand(b"ShielddDeriveOVK", &nk.0.to_bytes(), ak.as_ref());
            let mut ovk = [0; 32];
            ovk.copy_from_slice(&hash_result.as_bytes()[0..32]);
            ovk
        };

        let dk = {
            let hash_result = prf::expand(b"Shieldd_DeriveDK", &nk.0.to_bytes(), ak.as_ref());
            let mut dk = [0; 16];
            dk.copy_from_slice(&hash_result.as_bytes()[0..16]);
            dk
        };

        let ivk_mod_r = Fr::from_le_bytes_mod_order(&ivk_mod_q.to_bytes());
        if ivk_mod_r.is_zero() {
            return Err(FullViewingKeyError::ZeroIncomingViewingKey);
        }
        let ivk = ka::Secret::new_from_field(ivk_mod_r);

        let dk = DiversifierKey(dk);
        let ovk = OutgoingViewingKey(ovk);
        let ivk = IncomingViewingKey { ivk, dk };

        Ok(Self { ak, nk, ovk, ivk })
    }

    /// Returns the incoming viewing key for this full viewing key.
    pub fn incoming(&self) -> &IncomingViewingKey {
        &self.ivk
    }

    /// Returns the outgoing viewing key for this full viewing key.
    pub fn outgoing(&self) -> &OutgoingViewingKey {
        &self.ovk
    }

    pub fn nullifier_key(&self) -> &NullifierKey {
        &self.nk
    }

    /// Returns the spend verification key contained in this full viewing key.
    pub fn spend_verification_key(&self) -> &VerificationKey<SpendAuth> {
        &self.ak
    }

    /// Construct the backreference key for this full viewing key.
    pub fn backref_key(&self) -> BackreferenceKey {
        BackreferenceKey::derive(self.outgoing()).clone()
    }

    /// Construct a position metadata key for this full viewing key.
    pub fn position_metadata_key(&self) -> PositionMetadataKey {
        PositionMetadataKey::derive(self.outgoing())
    }

    /// Hashes the full viewing key into an [`WalletId`].
    pub fn wallet_id(&self) -> WalletId {
        let hash_result = hash_2(
            &ACCOUNT_ID_DOMAIN_SEP,
            (
                self.nk.0,
                Fq::from_le_bytes_mod_order(&self.ak.to_bytes()[..]),
            ),
        );
        let hash = hash_result.to_bytes()[..32]
            .try_into()
            .expect("hash is 32 bytes");
        WalletId(hash)
    }
}

impl TryFrom<&[u8]> for FullViewingKey {
    type Error = anyhow::Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() != 64 {
            anyhow::bail!("Wrong byte length, expected 64 but found {}", value.len());
        }

        let ak_bytes: [u8; 32] = value[0..32].try_into().context("fvk wrong length")?;
        let nk_bytes: [u8; 32] = value[32..64].try_into().context("fvk wrong length")?;

        let ak = ak_bytes.try_into()?;
        let nk = NullifierKey(
            Fq::deserialize_compressed(&nk_bytes[..])
                .context("could not deserialize nullifier key")?,
        );

        Ok(FullViewingKey::from_components(ak, nk)?)
    }
}

impl DomainType for FullViewingKey {
    type Proto = pb::FullViewingKey;
}

impl TryFrom<pb::FullViewingKey> for FullViewingKey {
    type Error = anyhow::Error;

    fn try_from(value: pb::FullViewingKey) -> Result<Self, Self::Error> {
        Self::try_from(value.inner.as_slice())
    }
}

impl From<FullViewingKey> for pb::FullViewingKey {
    fn from(value: FullViewingKey) -> pb::FullViewingKey {
        let mut inner = Vec::with_capacity(64);
        inner.extend_from_slice(&value.ak.to_bytes());
        inner.extend_from_slice(&value.nk.0.to_bytes());
        pb::FullViewingKey { inner }
    }
}

impl std::fmt::Display for FullViewingKey {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let proto = pb::FullViewingKey::from(self.clone());
        f.write_str(&bech32str::encode(
            &proto.inner,
            bech32str::full_viewing_key::BECH32_PREFIX,
            bech32str::Bech32m,
        ))
    }
}

impl std::fmt::Debug for FullViewingKey {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        <Self as std::fmt::Display>::fmt(self, f)
    }
}

impl std::str::FromStr for FullViewingKey {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        pb::FullViewingKey {
            inner: bech32str::decode(
                s,
                bech32str::full_viewing_key::BECH32_PREFIX,
                bech32str::Bech32m,
            )?,
        }
        .try_into()
    }
}

#[cfg(test)]
mod tests {
    use decaf377_rdsa::VerificationKeyBytes;

    use super::*;

    #[test]
    fn full_viewing_key_rejects_identity_authorization_key() {
        let identity = VerificationKey::<SpendAuth>::try_from(
            VerificationKeyBytes::<SpendAuth>::from(Element::default().vartime_compress().0),
        )
        .expect("the underlying verification-key type admits the identity");

        assert_eq!(
            FullViewingKey::from_components(identity, NullifierKey(Fq::from(1u64))),
            Err(FullViewingKeyError::IdentityAuthorizationKey)
        );
    }

    #[test]
    fn full_viewing_key_rejects_zero_reduced_incoming_viewing_key() {
        let ak = *crate::test_keys::FULL_VIEWING_KEY.spend_verification_key();
        assert_eq!(
            FullViewingKey::from_components_with_ivk_hash(
                ak,
                NullifierKey(Fq::from(1u64)),
                Fq::zero(),
            ),
            Err(FullViewingKeyError::ZeroIncomingViewingKey)
        );
    }

    #[test]
    fn full_viewing_key_accepts_valid_components() {
        let known = crate::test_keys::FULL_VIEWING_KEY.clone();
        let rebuilt = FullViewingKey::from_components(
            *known.spend_verification_key(),
            *known.nullifier_key(),
        )
        .expect("known components satisfy all full viewing key invariants");
        assert_eq!(rebuilt, known);
    }
}
