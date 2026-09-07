//! Proof-bound routing words for encrypted note discovery.
//! Addresses map to one stable word; protocol parameters select nested prefixes.
//! Public tags are always 32 bits and never reveal the selected precision.
//! Full compact-block scanning remains the authoritative recovery path.

use anyhow::Result;
use decaf377::Fq;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use shieldd_sdk_keys::Address;
use shieldd_sdk_proto::{core::component::shielded_pool::v1 as pb, DomainType};

pub mod state_key;

pub const DEFAULT_REGULATED_PRECISION_BITS: u8 = 12;
pub const DEFAULT_UNREGULATED_PRECISION_BITS: u8 = 18;
pub const DEFAULT_GRACE_PERIOD_BLOCKS: u64 = 16;

static ROUTE_DOMAIN: Lazy<Fq> = Lazy::new(|| domain(b"shieldd.discovery.route"));
static TAG_RANDOM_DOMAIN: Lazy<Fq> = Lazy::new(|| domain(b"shieldd.discovery.tag_random"));
static TAG_PERMUTATION_DOMAIN: Lazy<Fq> = Lazy::new(|| domain(b"shieldd.discovery.permutation"));
static PARAMETERS_DOMAIN: Lazy<Fq> = Lazy::new(|| domain(b"shieldd.discovery.parameters"));

fn domain(label: &[u8]) -> Fq {
    Fq::from_le_bytes_mod_order(blake2b_simd::blake2b(label).as_bytes())
}

fn trunc32(value: Fq) -> u32 {
    u32::from_le_bytes(
        value.to_bytes()[..4]
            .try_into()
            .expect("a field encoding contains four bytes"),
    )
}

/// Protocol-wide prefix precision for one asset class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Precision(u8);

impl Precision {
    pub const MIN: Self = Self(0);
    pub const MAX: Self = Self(32);

    pub fn new(bits: u8) -> Result<Self> {
        anyhow::ensure!(
            bits <= Self::MAX.0,
            "routing precision must be at most 32 bits"
        );
        Ok(Self(bits))
    }

    pub fn bits(self) -> u8 {
        self.0
    }

    pub fn mask(self) -> u32 {
        match self.0 {
            0 => 0,
            32 => u32::MAX,
            bits => (1u32 << bits) - 1,
        }
    }
}

impl TryFrom<u32> for Precision {
    type Error = anyhow::Error;

    fn try_from(value: u32) -> Result<Self> {
        Self::new(value.try_into()?)
    }
}

impl From<Precision> for u32 {
    fn from(value: Precision) -> Self {
        value.bits().into()
    }
}

/// Public onchain routing value. Its precision is deliberately absent.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(try_from = "pb::RoutingTag", into = "pb::RoutingTag")]
pub struct RoutingTag {
    pub value: u32,
}

impl DomainType for RoutingTag {
    type Proto = pb::RoutingTag;
}

impl TryFrom<pb::RoutingTag> for RoutingTag {
    type Error = anyhow::Error;

    fn try_from(value: pb::RoutingTag) -> Result<Self> {
        Ok(Self { value: value.value })
    }
}

impl From<RoutingTag> for pb::RoutingTag {
    fn from(value: RoutingTag) -> Self {
        Self { value: value.value }
    }
}

/// Local or query-side prefix matcher.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "pb::RoutingSelector", into = "pb::RoutingSelector")]
pub struct RoutingSelector {
    pub precision: Precision,
    pub prefix: u32,
}

impl RoutingSelector {
    pub fn for_address(address: &Address, precision: Precision) -> Self {
        Self {
            precision,
            prefix: route_word(address) & precision.mask(),
        }
    }

    pub fn matches(self, tag: RoutingTag) -> bool {
        tag.value & self.precision.mask() == self.prefix
    }

    pub fn is_canonical(self) -> bool {
        self.prefix & !self.precision.mask() == 0
    }
}

impl DomainType for RoutingSelector {
    type Proto = pb::RoutingSelector;
}

impl TryFrom<pb::RoutingSelector> for RoutingSelector {
    type Error = anyhow::Error;

    fn try_from(value: pb::RoutingSelector) -> Result<Self> {
        let selector = Self {
            precision: value.precision_bits.try_into()?,
            prefix: value.prefix,
        };
        anyhow::ensure!(
            selector.is_canonical(),
            "routing selector has non-zero unused bits"
        );
        Ok(selector)
    }
}

impl From<RoutingSelector> for pb::RoutingSelector {
    fn from(value: RoutingSelector) -> Self {
        Self {
            precision_bits: value.precision.into(),
            prefix: value.prefix,
        }
    }
}

/// Fixed public routing shape for a transfer action.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "pb::TransferRouting", into = "pb::TransferRouting")]
pub struct TransferRouting {
    pub tags: [RoutingTag; 2],
}

impl DomainType for TransferRouting {
    type Proto = pb::TransferRouting;
}

impl TryFrom<pb::TransferRouting> for TransferRouting {
    type Error = anyhow::Error;

    fn try_from(value: pb::TransferRouting) -> Result<Self> {
        let tags: Vec<RoutingTag> = value
            .tags
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<_>>()?;
        Ok(Self {
            tags: tags.try_into().map_err(|tags: Vec<_>| {
                anyhow::anyhow!("transfer routing requires 2 tags, got {}", tags.len())
            })?,
        })
    }
}

impl From<TransferRouting> for pb::TransferRouting {
    fn from(value: TransferRouting) -> Self {
        Self {
            tags: value.tags.into_iter().map(Into::into).collect(),
        }
    }
}

/// Active class-dependent routing precisions and their activation height.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "pb::DiscoveryParameters", into = "pb::DiscoveryParameters")]
pub struct Parameters {
    pub regulated_precision: Precision,
    pub unregulated_precision: Precision,
    pub as_of_height: u64,
}

impl Parameters {
    pub fn new(
        regulated_precision: Precision,
        unregulated_precision: Precision,
        as_of_height: u64,
    ) -> Result<Self> {
        anyhow::ensure!(
            regulated_precision <= unregulated_precision,
            "regulated routing precision must not exceed unregulated precision"
        );
        Ok(Self {
            regulated_precision,
            unregulated_precision,
            as_of_height,
        })
    }

    pub fn precision(&self, is_regulated: bool) -> Precision {
        if is_regulated {
            self.regulated_precision
        } else {
            self.unregulated_precision
        }
    }

    /// A proof-bound identifier for this complete parameter set.
    pub fn id(&self) -> Fq {
        poseidon377::hash_3(
            &PARAMETERS_DOMAIN,
            (
                Fq::from(self.regulated_precision.bits()),
                Fq::from(self.unregulated_precision.bits()),
                Fq::from(self.as_of_height),
            ),
        )
    }

    pub fn accepted_at(
        &self,
        previous: &Self,
        grace_period_blocks: u64,
        block_height: u64,
        parameter_set_id: Fq,
    ) -> bool {
        parameter_set_id == self.id()
            || (parameter_set_id == previous.id()
                && block_height < self.as_of_height.saturating_add(grace_period_blocks))
    }
}

impl Default for Parameters {
    fn default() -> Self {
        Self::new(
            Precision::new(DEFAULT_REGULATED_PRECISION_BITS).expect("default precision is valid"),
            Precision::new(DEFAULT_UNREGULATED_PRECISION_BITS).expect("default precision is valid"),
            1,
        )
        .expect("default routing parameters are ordered")
    }
}

impl DomainType for Parameters {
    type Proto = pb::DiscoveryParameters;
}

impl TryFrom<pb::DiscoveryParameters> for Parameters {
    type Error = anyhow::Error;

    fn try_from(value: pb::DiscoveryParameters) -> Result<Self> {
        Self::new(
            value.regulated_precision_bits.try_into()?,
            value.unregulated_precision_bits.try_into()?,
            value.as_of_height,
        )
    }
}

impl From<Parameters> for pb::DiscoveryParameters {
    fn from(value: Parameters) -> Self {
        Self {
            regulated_precision_bits: value.regulated_precision.into(),
            unregulated_precision_bits: value.unregulated_precision.into(),
            as_of_height: value.as_of_height,
        }
    }
}

pub fn route_word(address: &Address) -> u32 {
    trunc32(poseidon377::hash_1(
        &ROUTE_DOMAIN,
        *address.transmission_key_s(),
    ))
}

pub fn random_word(routing_nonce: Fq, tag_slot: u8) -> u32 {
    trunc32(poseidon377::hash_2(
        &TAG_RANDOM_DOMAIN,
        (routing_nonce, Fq::from(tag_slot)),
    ))
}

pub fn permutation_bit(routing_nonce: Fq) -> bool {
    trunc32(poseidon377::hash_1(&TAG_PERMUTATION_DOMAIN, routing_nonce)) & 1 == 1
}

fn tag_for_word(route_word: u32, precision: Precision, random_word: u32) -> RoutingTag {
    let mask = precision.mask();
    RoutingTag {
        value: (route_word & mask) | (random_word & !mask),
    }
}

pub fn transfer_routing(
    sender: &Address,
    receiver: &Address,
    is_regulated: bool,
    has_change: bool,
    parameters: &Parameters,
    routing_nonce: Fq,
) -> (TransferRouting, bool) {
    let precision = parameters.precision(is_regulated);
    let sender_word = route_word(sender);
    let receiver_word = route_word(receiver);
    let swapped = permutation_bit(routing_nonce);
    let mut role_words = [sender_word, receiver_word];
    let mut role_meaningful = [is_regulated || has_change, true];
    if swapped {
        role_words.swap(0, 1);
        role_meaningful.swap(0, 1);
    }

    let tags = std::array::from_fn(|slot| {
        let random = random_word(routing_nonce, slot as u8);
        if role_meaningful[slot] {
            tag_for_word(role_words[slot], precision, random)
        } else {
            RoutingTag { value: random }
        }
    });
    (TransferRouting { tags }, swapped)
}

pub fn single_tag(
    address: &Address,
    is_regulated: bool,
    parameters: &Parameters,
    routing_nonce: Fq,
) -> RoutingTag {
    tag_for_word(
        route_word(address),
        parameters.precision(is_regulated),
        random_word(routing_nonce, 0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selectors_are_nested_and_match_honest_tags() {
        let address = &shieldd_sdk_keys::test_keys::ADDRESS_0;
        let short = RoutingSelector::for_address(address, Precision::new(12).unwrap());
        let long = RoutingSelector::for_address(address, Precision::new(20).unwrap());
        assert_eq!(short.prefix, long.prefix & short.precision.mask());

        let parameters =
            Parameters::new(Precision::new(12).unwrap(), Precision::new(20).unwrap(), 9).unwrap();
        let tag = single_tag(address, false, &parameters, Fq::from(7u64));
        assert!(long.matches(tag));
        assert!(short.matches(tag));
    }

    #[test]
    fn full_tags_vary_while_prefixes_stay_stable() {
        let address = &shieldd_sdk_keys::test_keys::ADDRESS_0;
        let parameters = Parameters::default();
        let selector = RoutingSelector::for_address(address, parameters.unregulated_precision);
        let first = single_tag(address, false, &parameters, Fq::from(1u64));
        let second = single_tag(address, false, &parameters, Fq::from(2u64));
        assert_ne!(first, second);
        assert!(selector.matches(first));
        assert!(selector.matches(second));
    }

    #[test]
    fn transfer_shape_hides_roles_and_no_change_branch() {
        let parameters = Parameters::default();
        let sender = &shieldd_sdk_keys::test_keys::ADDRESS_0;
        let receiver = &shieldd_sdk_keys::test_keys::ADDRESS_1;
        let sender_selector =
            RoutingSelector::for_address(sender, parameters.unregulated_precision);
        let receiver_selector =
            RoutingSelector::for_address(receiver, parameters.unregulated_precision);

        for nonce in 1u64..32 {
            let (routing, _) =
                transfer_routing(sender, receiver, false, false, &parameters, Fq::from(nonce));
            assert_eq!(
                routing
                    .tags
                    .iter()
                    .filter(|tag| receiver_selector.matches(**tag))
                    .count(),
                1
            );
            assert_eq!(
                routing
                    .tags
                    .iter()
                    .filter(|tag| sender_selector.matches(**tag))
                    .count(),
                0
            );
        }
    }

    #[test]
    fn parameters_reject_inverted_precisions() {
        assert!(
            Parameters::new(Precision::new(20).unwrap(), Precision::new(12).unwrap(), 1,).is_err()
        );
    }

    #[test]
    fn decoding_rejects_non_canonical_unused_bits() {
        let proto = pb::RoutingSelector {
            precision_bits: 12,
            prefix: 1 << 12,
        };
        assert!(RoutingSelector::try_from(proto).is_err());
    }
}
