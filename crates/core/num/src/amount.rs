use ark_ff::ToConstraintField;
use decaf377::{Fq, Fr};
use serde::{Deserialize, Serialize};
use shieldd_sdk_proto::{shieldd::core::num::v1 as pb, DomainType};
use std::{fmt::Display, iter::Sum, num::NonZeroU128, ops};

#[derive(Serialize, Default, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
#[serde(try_from = "pb::Amount", into = "pb::Amount")]
pub struct Amount {
    inner: u128,
}

impl std::fmt::Debug for Amount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl Amount {
    pub fn value(&self) -> u128 {
        self.inner
    }

    pub fn zero() -> Self {
        Self { inner: 0 }
    }

    // We need fixed length encoding to produce encrypted `Note`s.
    pub fn to_le_bytes(&self) -> [u8; 16] {
        self.inner.to_le_bytes()
    }

    pub fn to_be_bytes(&self) -> [u8; 16] {
        self.inner.to_be_bytes()
    }

    pub fn from_le_bytes(bytes: [u8; 16]) -> Amount {
        Amount {
            inner: u128::from_le_bytes(bytes),
        }
    }

    pub fn from_be_bytes(bytes: [u8; 16]) -> Amount {
        Amount {
            inner: u128::from_be_bytes(bytes),
        }
    }

    pub fn checked_sub(&self, rhs: &Self) -> Option<Self> {
        self.inner
            .checked_sub(rhs.inner)
            .map(|inner| Self { inner })
    }

    pub fn checked_add(&self, rhs: &Self) -> Option<Self> {
        self.inner
            .checked_add(rhs.inner)
            .map(|inner| Self { inner })
    }

    pub fn checked_mul(&self, rhs: &Self) -> Option<Self> {
        self.inner
            .checked_mul(rhs.inner)
            .map(|inner| Self { inner })
    }

    pub fn saturating_add(&self, rhs: &Self) -> Self {
        Self {
            inner: self.inner.saturating_add(rhs.inner),
        }
    }

    pub fn saturating_sub(&self, rhs: &Self) -> Self {
        Self {
            inner: self.inner.saturating_sub(rhs.inner),
        }
    }
}

impl ops::Not for Amount {
    type Output = Self;

    fn not(self) -> Self::Output {
        Self { inner: !self.inner }
    }
}

impl ToConstraintField<Fq> for Amount {
    fn to_field_elements(&self) -> Option<Vec<Fq>> {
        let mut elements = Vec::new();
        elements.extend_from_slice(&[Fq::from(self.inner)]);
        Some(elements)
    }
}

impl From<Amount> for pb::Amount {
    fn from(a: Amount) -> Self {
        let lo = a.inner as u64;
        let hi = (a.inner >> 64) as u64;
        pb::Amount { lo, hi }
    }
}

impl TryFrom<pb::Amount> for Amount {
    type Error = anyhow::Error;

    fn try_from(amount: pb::Amount) -> Result<Self, Self::Error> {
        let lo = amount.lo as u128;
        let hi = amount.hi as u128;
        let inner = (hi << 64) | lo;

        Ok(Amount { inner })
    }
}

impl TryFrom<std::string::String> for Amount {
    type Error = anyhow::Error;

    fn try_from(s: std::string::String) -> Result<Self, Self::Error> {
        let inner = s.parse::<u128>()?;
        Ok(Amount { inner })
    }
}

impl DomainType for Amount {
    type Proto = pb::Amount;
}

impl From<u64> for Amount {
    fn from(amount: u64) -> Amount {
        Amount {
            inner: amount as u128,
        }
    }
}

impl From<u32> for Amount {
    fn from(amount: u32) -> Amount {
        Amount {
            inner: amount as u128,
        }
    }
}

impl From<u16> for Amount {
    fn from(amount: u16) -> Amount {
        Amount {
            inner: amount as u128,
        }
    }
}

impl From<u8> for Amount {
    fn from(amount: u8) -> Amount {
        Amount {
            inner: amount as u128,
        }
    }
}

impl From<Amount> for f64 {
    fn from(amount: Amount) -> f64 {
        amount.inner as f64
    }
}

impl Display for Amount {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl ops::Add<Amount> for Amount {
    type Output = Amount;

    fn add(self, rhs: Amount) -> Amount {
        Amount {
            inner: self.inner + rhs.inner,
        }
    }
}

impl ops::AddAssign<Amount> for Amount {
    fn add_assign(&mut self, rhs: Amount) {
        self.inner += rhs.inner;
    }
}

impl ops::Sub<Amount> for Amount {
    type Output = Amount;

    fn sub(self, rhs: Amount) -> Amount {
        Amount {
            inner: self.inner - rhs.inner,
        }
    }
}

impl ops::SubAssign<Amount> for Amount {
    fn sub_assign(&mut self, rhs: Amount) {
        self.inner -= rhs.inner;
    }
}

impl ops::Rem<Amount> for Amount {
    type Output = Amount;

    fn rem(self, rhs: Amount) -> Amount {
        Amount {
            inner: self.inner % rhs.inner,
        }
    }
}

impl ops::Mul<Amount> for Amount {
    type Output = Amount;

    fn mul(self, rhs: Amount) -> Amount {
        Amount {
            inner: self.inner * rhs.inner,
        }
    }
}

impl ops::Div<Amount> for Amount {
    type Output = Amount;

    fn div(self, rhs: Amount) -> Amount {
        Amount {
            inner: self.inner / rhs.inner,
        }
    }
}

impl From<NonZeroU128> for Amount {
    fn from(n: NonZeroU128) -> Self {
        Self { inner: n.get() }
    }
}

impl From<Amount> for Fq {
    fn from(amount: Amount) -> Fq {
        Fq::from(amount.inner)
    }
}

impl From<Amount> for Fr {
    fn from(amount: Amount) -> Fr {
        Fr::from(amount.inner)
    }
}

impl From<u128> for Amount {
    fn from(amount: u128) -> Amount {
        Amount { inner: amount }
    }
}

impl From<Amount> for u128 {
    fn from(amount: Amount) -> u128 {
        amount.inner
    }
}

impl From<i128> for Amount {
    fn from(amount: i128) -> Amount {
        Amount {
            inner: amount as u128,
        }
    }
}

impl From<Amount> for i128 {
    fn from(amount: Amount) -> i128 {
        amount.inner as i128
    }
}

impl Sum for Amount {
    fn sum<I: Iterator<Item = Amount>>(iter: I) -> Amount {
        iter.fold(Amount::zero(), |acc, x| acc + x)
    }
}

#[cfg(test)]
mod test {
    use crate::Amount;
    use shieldd_sdk_proto::shieldd::core::num::v1 as pb;

    fn encode_decode(value: u128) -> u128 {
        let amount = Amount { inner: value };
        let proto: pb::Amount = amount.into();
        Amount::try_from(proto).unwrap().inner
    }

    #[test]
    fn encode_decode_boundaries() {
        for (name, value) in [
            ("max", u128::MAX),
            ("zero", 0),
            ("right_border_bit", 1 << 64),
            ("left_border_bit", 1 << 63),
            ("u64_max", u64::MAX as u128),
        ] {
            assert_eq!(value, encode_decode(value), "{name}");
        }
    }

    proptest::proptest! {
        #[test]
        fn encode_decode_random(value: u128, half: u64) {
            for value in [value, half as u128, (half as u128) << 64] {
                proptest::prop_assert_eq!(value, encode_decode(value));
            }
        }
    }

    // Regression tests for ZK-ASSUME-AMOUNT-RANGE / ZK-PROP-AMOUNT-RANGE-128.
    //
    // The circuit enforces `amount < 2^128` in-gadget via `ScalarMulLE(.., 128)`
    // (pinned Go-side by `TestAmountRangeBoundIs128Bits`). These tests pin the
    // Rust-side half of the assumption: every amount that can exist off-circuit is
    // < 2^128 by construction, and its scalar-field embedding — the value the
    // net-balance commitment sums — does not wrap the curve order, so a valid
    // amount can never alias a smaller one mod `Fr`.

    #[test]
    fn scalar_embedding_of_max_amount_does_not_wrap() {
        use ark_ff::{BigInteger, PrimeField};
        use decaf377::Fr;

        // The net-balance commitment scalar-multiplies by `Fr::from(amount)`.
        // If `2^128 - 1 >= Fr::MODULUS` the embedding would wrap and two distinct
        // amounts could collide mod the curve order. Prove the max amount embeds
        // with no reduction: its little-endian field bytes are exactly the 16
        // bytes of u128::MAX, zero-padded — i.e. the integer is unchanged.
        let fr = Fr::from(u128::MAX);
        let le = fr.into_bigint().to_bytes_le();
        assert_eq!(&le[0..16], &u128::MAX.to_le_bytes());
        assert!(le[16..].iter().all(|&b| b == 0));
    }
}
