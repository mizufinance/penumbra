use ibc_types::core::{channel::ChannelId, client::Height as IbcHeight};
use serde::{Deserialize, Serialize};
use shieldd_sdk_asset::{
    asset::{self, Metadata},
    Balance, Value,
};
use shieldd_sdk_keys::Address;
use shieldd_sdk_num::Amount;
use shieldd_sdk_proto::{
    shieldd::core::component::ibc::v1::{self as pb, FungibleTokenPacketData},
    DomainType,
};
use shieldd_sdk_txhash::{EffectHash, EffectingData};
use std::str::FromStr;

#[cfg(feature = "component")]
use ibc_types::core::channel::PortId;

#[cfg(feature = "component")]
use shieldd_sdk_ibc::component::packet::{IBCPacket, Unchecked};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "pb::Ics20Withdrawal", into = "pb::Ics20Withdrawal")]
pub struct Ics20Withdrawal {
    // a transparent value consisting of an amount and a denom.
    pub amount: Amount,
    pub denom: asset::Metadata,
    // the address on the destination chain to send the transfer to
    pub destination_chain_address: String,
    // a "sender" shieldd address to use to return funds from this withdrawal.
    // this should be an ephemeral address
    pub return_address: Address,
    // the height (on Shieldd) at which this transfer expires (and funds are sent
    // back to the return address?). NOTE: if funds are sent back to the sender,
    // we MUST verify a nonexistence proof before accepting the timeout, to
    // prevent relayer censorship attacks. The core IBC implementation does this
    // in its handling of validation of timeouts.
    pub timeout_height: IbcHeight,
    // the timestamp at which this transfer expires, in nanoseconds after unix epoch.
    pub timeout_time: u64,
    // the source channel used for the withdrawal
    pub source_channel: ChannelId,

    // Arbitrary string data to be included in the `memo` field
    // of the ICS-20 FungibleTokenPacketData for this withdrawal.
    // Commonly used for packet forwarding support, or other protocols that may support usage of the memo field.
    pub ics20_memo: String,
    // Whether to use a transparent address for the return address in the withdrawal.
    pub use_transparent_address: bool,
}

#[cfg(feature = "component")]
impl TryFrom<Ics20Withdrawal> for IBCPacket<Unchecked> {
    type Error = anyhow::Error;

    fn try_from(withdrawal: Ics20Withdrawal) -> Result<Self, Self::Error> {
        Ok(Self::new(
            PortId::transfer(),
            withdrawal.source_channel.clone(),
            withdrawal.timeout_height,
            withdrawal.timeout_time,
            withdrawal.packet_data()?,
        ))
    }
}

impl Ics20Withdrawal {
    pub fn value(&self) -> Value {
        Value {
            amount: self.amount,
            asset_id: self.denom.id(),
        }
    }

    pub fn balance(&self) -> Balance {
        -Balance::from(self.value())
    }

    pub fn packet_data(&self) -> anyhow::Result<Vec<u8>> {
        self.validate()?;
        let ordinary_return_address = self.return_address.to_string();
        let return_address = if self.use_transparent_address {
            self.return_address
                .encode_as_transparent_address()
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "transparent return-address mode requires a transparently \
                         encodable return address"
                    )
                })?
        } else {
            ordinary_return_address
        };
        let ftpd = FungibleTokenPacketData {
            amount: self.value().amount.to_string(),
            denom: self.denom.to_string(),
            receiver: self.destination_chain_address.clone(),
            sender: return_address,
            memo: self.ics20_memo.clone(),
        };

        // In violation of the ICS20 spec, ibc-go encodes transfer packets as JSON.
        serde_json::to_vec(&ftpd).map_err(Into::into)
    }

    // stateless validation of an Ics20 withdrawal action.
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.amount == Amount::zero() {
            anyhow::bail!("withdrawal amount must be non-zero");
        }

        if self.use_transparent_address
            && self
                .return_address
                .encode_as_transparent_address()
                .is_none()
        {
            anyhow::bail!(
                "transparent return-address mode requires a transparently encodable return address"
            );
        }

        if self.timeout_time == 0 {
            anyhow::bail!("timeout time must be non-zero");
        }

        // in order to prevent clients from inadvertently identifying themselves by their clock
        // skew, enforce that timeout time is rounded to the nearest minute
        if self.timeout_time % 60_000_000_000 != 0 {
            anyhow::bail!(
                "withdrawal timeout timestamp {} is not rounded to one minute",
                self.timeout_time
            );
        }

        // NOTE: we could validate the destination chain address as bech32 to prevent mistyped
        // addresses, but this would preclude sending to chains that don't use bech32 addresses.

        Ok(())
    }
}

impl EffectingData for Ics20Withdrawal {
    fn effect_hash(&self) -> EffectHash {
        EffectHash::from_proto_effecting_data(&self.to_proto())
    }
}

impl DomainType for Ics20Withdrawal {
    type Proto = pb::Ics20Withdrawal;
}

impl From<Ics20Withdrawal> for pb::Ics20Withdrawal {
    fn from(w: Ics20Withdrawal) -> Self {
        pb::Ics20Withdrawal {
            amount: Some(w.amount.into()),
            denom: Some(w.denom.base_denom().into()),
            destination_chain_address: w.destination_chain_address,
            return_address: Some(w.return_address.into()),
            timeout_height: Some(w.timeout_height.into()),
            timeout_time: w.timeout_time,
            source_channel: w.source_channel.to_string(),
            ics20_memo: w.ics20_memo.to_string(),
            use_transparent_address: w.use_transparent_address,
        }
    }
}

impl TryFrom<pb::Ics20Withdrawal> for Ics20Withdrawal {
    type Error = anyhow::Error;
    fn try_from(s: pb::Ics20Withdrawal) -> Result<Self, Self::Error> {
        let withdrawal = Self {
            amount: s
                .amount
                .ok_or_else(|| anyhow::anyhow!("missing amount"))?
                .try_into()?,
            denom: Metadata::default_for(
                &s.denom
                    .ok_or_else(|| anyhow::anyhow!("missing denom metadata"))?
                    .try_into()?,
            )
            .ok_or_else(|| anyhow::anyhow!("could not generate default denom metadata"))?,
            destination_chain_address: s.destination_chain_address,
            return_address: s
                .return_address
                .ok_or_else(|| anyhow::anyhow!("missing sender"))?
                .try_into()?,
            timeout_height: s
                .timeout_height
                .ok_or_else(|| anyhow::anyhow!("missing timeout height"))?
                .try_into()?,
            timeout_time: s.timeout_time,
            source_channel: ChannelId::from_str(&s.source_channel)?,
            ics20_memo: s.ics20_memo,
            use_transparent_address: s.use_transparent_address,
        };
        withdrawal.validate()?;
        Ok(withdrawal)
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Deref;

    use super::*;
    use ibc_types::core::client::Height as IbcHeight;
    use shieldd_sdk_asset::BASE_ASSET_DENOM;
    use shieldd_sdk_keys::test_keys;

    fn withdrawal() -> Ics20Withdrawal {
        Ics20Withdrawal {
            amount: 1u64.into(),
            denom: BASE_ASSET_DENOM.clone(),
            destination_chain_address: "cosmos1destination".to_owned(),
            return_address: test_keys::ADDRESS_0.deref().clone(),
            timeout_height: IbcHeight::new(1, 10).expect("valid timeout height"),
            timeout_time: 60_000_000_000,
            source_channel: ChannelId::from_str("channel-0").expect("valid channel"),
            ics20_memo: String::new(),
            use_transparent_address: false,
        }
    }

    #[test]
    fn transparent_mode_rejects_an_ordinary_return_address() {
        let mut withdrawal = withdrawal();
        withdrawal.use_transparent_address = true;

        let error = withdrawal
            .validate()
            .expect_err("transparent mode must not silently use the ordinary encoding");
        assert!(error.to_string().contains("transparently encodable"));
        assert!(
            withdrawal.packet_data().is_err(),
            "packet encoding must not alias invalid transparent mode to an ordinary sender"
        );
    }

    #[test]
    fn decoding_rejects_transparent_mode_with_an_ordinary_return_address() {
        let mut proto: pb::Ics20Withdrawal = withdrawal().into();
        proto.use_transparent_address = true;

        let error = Ics20Withdrawal::try_from(proto)
            .expect_err("domain decoding must reject a no-op transparent flag");
        assert!(error.to_string().contains("transparently encodable"));
    }

    #[test]
    fn timeout_must_be_nonzero_and_minute_rounded() {
        let mut zero = withdrawal();
        zero.timeout_time = 0;
        let error = zero.validate().expect_err("zero timeout must be rejected");
        assert!(error.to_string().contains("non-zero"));

        let mut subminute = withdrawal();
        subminute.timeout_time += 1;
        let error = subminute
            .validate()
            .expect_err("sub-minute timeout precision must be rejected");
        assert!(error.to_string().contains("rounded to one minute"));
    }

    #[test]
    fn transparent_mode_uses_the_transparent_packet_sender() {
        let mut withdrawal = withdrawal();
        withdrawal.return_address = test_keys::FULL_VIEWING_KEY
            .incoming()
            .transparent_address()
            .parse()
            .expect("generated transparent address parses");
        withdrawal.use_transparent_address = true;
        withdrawal
            .validate()
            .expect("transparent return address must validate");

        let packet: FungibleTokenPacketData = serde_json::from_slice(
            &withdrawal
                .packet_data()
                .expect("validated packet data encodes"),
        )
        .expect("packet data decodes");
        assert_eq!(
            packet.sender,
            withdrawal
                .return_address
                .encode_as_transparent_address()
                .expect("validated transparent encoding")
        );
        assert_ne!(packet.sender, withdrawal.return_address.to_string());
    }
}
