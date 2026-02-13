use core::fmt;
use std::fmt::Display;
use std::fmt::Formatter;

use anyhow::{Context, Result};
use async_trait::async_trait;

use ibc_types::core::client::ClientId;
use ibc_types::core::client::ClientType;
use ibc_types::core::client::Height;

use ibc_types::path::{ClientConsensusStatePath, ClientStatePath, ClientTypePath};

use cnidarium::{StateRead, StateWrite};
use ibc_types::lightclients::tendermint::{
    client_state::ClientState as TendermintClientState,
    consensus_state::ConsensusState as TendermintConsensusState,
    header::Header as TendermintHeader,
};
use penumbra_sdk_proto::{StateReadProto, StateWriteProto};
use prost::Message as _;

use crate::client_types::{
    AnyClientState, AnyConsensusState, AnyHeader, BankdClientState, BankdConsensusState,
    BankdHeader,
};
use crate::component::client_counter::{ClientCounter, VerifiedHeights};
use crate::prefix::MerklePrefixExt;
use crate::IBC_COMMITMENT_PREFIX;

use super::state_key;
use super::HostInterface;

/// ClientStatus represents the current status of an IBC client.
///
/// https://github.com/cosmos/ibc-go/blob/main/modules/core/exported/client.go#L30
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientStatus {
    /// Active is a status type of a client. An active client is allowed to be used.
    Active,
    /// Frozen is a status type of a client. A frozen client is not allowed to be used.
    Frozen,
    /// Expired is a status type of a client. An expired client is not allowed to be used.
    Expired,
    /// Unknown indicates there was an error in determining the status of a client.
    Unknown,
    /// Unauthorized indicates that the client type is not registered as an allowed client type.
    Unauthorized,
}

impl Display for ClientStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ClientStatus::Active => write!(f, "Active"),
            ClientStatus::Frozen => write!(f, "Frozen"),
            ClientStatus::Expired => write!(f, "Expired"),
            ClientStatus::Unknown => write!(f, "Unknown"),
            ClientStatus::Unauthorized => write!(f, "Unauthorized"),
        }
    }
}

#[async_trait]
pub(crate) trait Ics2ClientExt: StateWrite {
    // Given an already verified header and a trusted client state, compute
    // the next client and consensus states. Dispatches by client type.
    async fn next_client_state(
        &self,
        client_id: ClientId,
        trusted_client_state: AnyClientState,
        verified_header: AnyHeader,
    ) -> Result<(AnyClientState, AnyConsensusState)> {
        match (trusted_client_state, verified_header) {
            (AnyClientState::Tendermint(tm_cs), AnyHeader::Tendermint(tm_header)) => {
                self.next_tendermint_state_inner(client_id, tm_cs, tm_header)
                    .await
            }
            (AnyClientState::Bankd(bankd_cs), AnyHeader::Bankd(bankd_hdr)) => {
                self.next_bankd_state_inner(client_id, bankd_cs, bankd_hdr)
                    .await
            }
            _ => {
                anyhow::bail!("mismatched client state and header types");
            }
        }
    }

    // Tendermint-specific state transition logic. Checks for equivocation and
    // timestamp monotonicity violations, freezing the client if found.
    async fn next_tendermint_state_inner(
        &self,
        client_id: ClientId,
        trusted_client_state: TendermintClientState,
        verified_header: TendermintHeader,
    ) -> Result<(AnyClientState, AnyConsensusState)> {
        let verified_consensus_state = TendermintConsensusState::from(verified_header.clone());

        // if we have a stored consensus state for this height that conflicts, we need to freeze
        // the client. if it doesn't conflict, we can return early
        if let Ok(stored_cs_state) = self
            .get_verified_consensus_state(&verified_header.height(), &client_id)
            .await
        {
            let stored_tm_cs = match stored_cs_state {
                AnyConsensusState::Tendermint(cs) => cs,
                _ => anyhow::bail!("expected Tendermint consensus state"),
            };
            if stored_tm_cs == verified_consensus_state {
                return Ok((
                    AnyClientState::Tendermint(trusted_client_state),
                    AnyConsensusState::Tendermint(verified_consensus_state),
                ));
            } else {
                return Ok((
                    AnyClientState::Tendermint(
                        trusted_client_state
                            .with_header(verified_header.clone())
                            .expect("able to add header to client state")
                            .with_frozen_height(ibc_types::core::client::Height {
                                revision_number: 0,
                                revision_height: 1,
                            }),
                    ),
                    AnyConsensusState::Tendermint(verified_consensus_state),
                ));
            }
        }

        // check that updates have monotonic timestamps. we may receive client updates that are
        // disjoint: the header we received and validated may be older than the newest header we
        // have. In that case, we need to verify that the timestamp is correct. if it isn't, freeze
        // the client.
        let next_consensus_state = self
            .next_verified_consensus_state(&client_id, &verified_header.height())
            .await
            .expect("able to get next verified consensus state");
        let prev_consensus_state = self
            .prev_verified_consensus_state(&client_id, &verified_header.height())
            .await
            .expect("able to get previous verified consensus state");

        // case 1: if we have a verified consensus state previous to this header, verify that this
        // header's timestamp is greater than or equal to the stored consensus state's timestamp
        if let Some(prev_state) = prev_consensus_state {
            let prev_tm = match prev_state {
                AnyConsensusState::Tendermint(cs) => cs,
                _ => anyhow::bail!("expected Tendermint consensus state for prev"),
            };
            if verified_header.signed_header.header().time < prev_tm.timestamp {
                return Ok((
                    AnyClientState::Tendermint(
                        trusted_client_state
                            .with_header(verified_header.clone())
                            .expect("able to add header to client state")
                            .with_frozen_height(ibc_types::core::client::Height {
                                revision_number: 0,
                                revision_height: 1,
                            }),
                    ),
                    AnyConsensusState::Tendermint(verified_consensus_state),
                ));
            }
        }
        // case 2: if we have a verified consensus state with higher block height than this header,
        // verify that this header's timestamp is less than or equal to this header's timestamp.
        if let Some(next_state) = next_consensus_state {
            let next_tm = match next_state {
                AnyConsensusState::Tendermint(cs) => cs,
                _ => anyhow::bail!("expected Tendermint consensus state for next"),
            };
            if verified_header.signed_header.header().time > next_tm.timestamp {
                return Ok((
                    AnyClientState::Tendermint(
                        trusted_client_state
                            .with_header(verified_header.clone())
                            .expect("able to add header to client state")
                            .with_frozen_height(ibc_types::core::client::Height {
                                revision_number: 0,
                                revision_height: 1,
                            }),
                    ),
                    AnyConsensusState::Tendermint(verified_consensus_state),
                ));
            }
        }

        Ok((
            AnyClientState::Tendermint(
                trusted_client_state
                    .with_header(verified_header.clone())
                    .expect("able to add header to client state"),
            ),
            AnyConsensusState::Tendermint(verified_consensus_state),
        ))
    }

    // Bankd state transition logic. Checks for equivocation and timestamp
    // monotonicity violations, freezing the client if found.
    async fn next_bankd_state_inner(
        &self,
        client_id: ClientId,
        trusted_client_state: BankdClientState,
        verified_header: BankdHeader,
    ) -> Result<(AnyClientState, AnyConsensusState)> {
        let verified_consensus_state = BankdConsensusState {
            root: verified_header.new_root.clone(),
            timestamp: verified_header.timestamp,
            group_public_key: trusted_client_state.group_public_key.clone(),
        };

        let header_height = verified_header
            .height
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("bankd header missing height"))?;
        let header_height =
            Height::new(header_height.revision_number, header_height.revision_height)?;

        // If we have a stored consensus state at this height that conflicts, freeze.
        if let Ok(stored_cs_state) = self
            .get_verified_consensus_state(&header_height, &client_id)
            .await
        {
            let stored_bankd = match stored_cs_state {
                AnyConsensusState::Bankd(cs) => cs,
                _ => anyhow::bail!("expected bankd consensus state"),
            };
            if stored_bankd == verified_consensus_state {
                return Ok((
                    AnyClientState::Bankd(trusted_client_state),
                    AnyConsensusState::Bankd(verified_consensus_state),
                ));
            } else {
                return Ok((
                    AnyClientState::Bankd(trusted_client_state)
                        .with_frozen_height(Height::new(0, 1)?),
                    AnyConsensusState::Bankd(verified_consensus_state),
                ));
            }
        }

        // Check timestamp monotonicity against adjacent heights.
        let next_consensus_state = self
            .next_verified_consensus_state(&client_id, &header_height)
            .await
            .expect("able to get next verified consensus state");
        let prev_consensus_state = self
            .prev_verified_consensus_state(&client_id, &header_height)
            .await
            .expect("able to get previous verified consensus state");

        if let Some(prev_state) = prev_consensus_state {
            let prev_bankd = match prev_state {
                AnyConsensusState::Bankd(cs) => cs,
                _ => anyhow::bail!("expected bankd consensus state for prev"),
            };
            if verified_header.timestamp < prev_bankd.timestamp {
                return Ok((
                    AnyClientState::Bankd(trusted_client_state)
                        .with_frozen_height(Height::new(0, 1)?),
                    AnyConsensusState::Bankd(verified_consensus_state),
                ));
            }
        }

        if let Some(next_state) = next_consensus_state {
            let next_bankd = match next_state {
                AnyConsensusState::Bankd(cs) => cs,
                _ => anyhow::bail!("expected bankd consensus state for next"),
            };
            if verified_header.timestamp > next_bankd.timestamp {
                return Ok((
                    AnyClientState::Bankd(trusted_client_state)
                        .with_frozen_height(Height::new(0, 1)?),
                    AnyConsensusState::Bankd(verified_consensus_state),
                ));
            }
        }

        // All good — update latest height if this header advances it
        let mut updated_cs = trusted_client_state;
        let latest = updated_cs
            .latest_height
            .as_ref()
            .map_or(0, |h| h.revision_height);
        if header_height.revision_height > latest {
            updated_cs.latest_height = verified_header.height.clone();
        }

        Ok((
            AnyClientState::Bankd(updated_cs),
            AnyConsensusState::Bankd(verified_consensus_state),
        ))
    }
}

impl<T: StateWrite + ?Sized> Ics2ClientExt for T {}

#[async_trait]
pub trait ConsensusStateWriteExt: StateWrite + Sized {
    async fn put_verified_consensus_state<HI: HostInterface>(
        &mut self,
        height: Height,
        client_id: ClientId,
        consensus_state: AnyConsensusState,
    ) -> Result<()> {
        let any_proto: ibc_proto::google::protobuf::Any = consensus_state.into();
        self.put_raw(
            IBC_COMMITMENT_PREFIX
                .apply_string(ClientConsensusStatePath::new(&client_id, &height).to_string()),
            any_proto.encode_to_vec(),
        );

        let current_height = HI::get_block_height(&self).await?;
        let revision_number = HI::get_revision_number(&self).await?;
        let current_time: ibc_types::timestamp::Timestamp =
            HI::get_block_timestamp(&self).await?.into();

        self.put_proto::<u64>(
            state_key::client_processed_times(&client_id, &height),
            current_time.nanoseconds(),
        );

        self.put(
            state_key::client_processed_heights(&client_id, &height),
            ibc_types::core::client::Height::new(revision_number, current_height)?,
        );

        // update verified heights
        let mut verified_heights =
            self.get_verified_heights(&client_id)
                .await?
                .unwrap_or(VerifiedHeights {
                    heights: Vec::new(),
                });

        verified_heights.heights.push(height);

        self.put_verified_heights(&client_id, verified_heights);

        Ok(())
    }
}

impl<T: StateWrite> ConsensusStateWriteExt for T {}

#[async_trait]
pub trait StateWriteExt: StateWrite + StateReadExt {
    fn put_client_counter(&mut self, counter: ClientCounter) {
        self.put("ibc_client_counter".into(), counter);
    }

    fn put_client(&mut self, client_id: &ClientId, client_state: AnyClientState) {
        self.put_proto(
            IBC_COMMITMENT_PREFIX
                .apply_string(ibc_types::path::ClientTypePath(client_id.clone()).to_string()),
            client_state.client_type().to_string(),
        );

        let any_proto: ibc_proto::google::protobuf::Any = client_state.into();
        self.put_raw(
            IBC_COMMITMENT_PREFIX.apply_string(ClientStatePath(client_id.clone()).to_string()),
            any_proto.encode_to_vec(),
        );
    }

    fn put_verified_heights(&mut self, client_id: &ClientId, verified_heights: VerifiedHeights) {
        self.put(
            format!(
                // NOTE: this is an implementation detail of the Penumbra ICS2 implementation, so
                // it's not in the same path namespace.
                "penumbra_verified_heights/{client_id}/verified_heights"
            ),
            verified_heights,
        );
    }

    // returns the ConsensusState for the penumbra chain (this chain) at the given height
    fn put_penumbra_sdk_consensus_state(
        &mut self,
        height: Height,
        consensus_state: TendermintConsensusState,
    ) {
        // NOTE: this is an implementation detail of the Penumbra ICS2 implementation, so
        // it's not in the same path namespace.
        self.put(
            format!("penumbra_consensus_states/{height}"),
            consensus_state,
        );
    }
}

impl<T: StateWrite + ?Sized> StateWriteExt for T {}

#[async_trait]
pub trait StateReadExt: StateRead {
    async fn client_counter(&self) -> Result<ClientCounter> {
        self.get("ibc_client_counter")
            .await
            .map(|counter| counter.unwrap_or(ClientCounter(0)))
    }

    async fn get_client_type(&self, client_id: &ClientId) -> Result<ClientType> {
        self.get_proto(
            &IBC_COMMITMENT_PREFIX.apply_string(ClientTypePath(client_id.clone()).to_string()),
        )
        .await?
        .context(format!("could not find client type for {client_id}"))
        .map(ClientType::new)
    }

    async fn get_client_state(&self, client_id: &ClientId) -> Result<AnyClientState> {
        let raw_bytes = self
            .get_raw(
                &IBC_COMMITMENT_PREFIX.apply_string(ClientStatePath(client_id.clone()).to_string()),
            )
            .await?
            .context(format!("could not find client state for {client_id}"))?;

        let any = ibc_proto::google::protobuf::Any::decode(raw_bytes.as_slice())
            .context("failed to decode client state as Any")?;
        AnyClientState::try_from(any)
    }

    async fn get_client_status(
        &self,
        client_id: &ClientId,
        current_block_time: tendermint::Time,
    ) -> ClientStatus {
        let client_type = self.get_client_type(client_id).await;

        if client_type.is_err() {
            return ClientStatus::Unknown;
        }

        let client_state = self.get_client_state(client_id).await;

        if client_state.is_err() {
            return ClientStatus::Unknown;
        }

        let client_state = client_state.expect("client state is Ok");

        if client_state.is_frozen() {
            return ClientStatus::Frozen;
        }

        // get latest height (may fail for malformed bankd state)
        let latest_height = match client_state.latest_height() {
            Ok(h) => h,
            Err(_) => return ClientStatus::Unknown,
        };

        // get latest consensus state to check for expiry
        let latest_consensus_state = self
            .get_verified_consensus_state(&latest_height, client_id)
            .await;

        if latest_consensus_state.is_err() {
            // if the client state does not have an associated consensus state for its latest height
            // then it must be expired
            return ClientStatus::Expired;
        }

        let latest_consensus_state = latest_consensus_state.expect("latest consensus state is Ok");

        // Dispatch expiry check by client type
        match (&client_state, &latest_consensus_state) {
            (AnyClientState::Tendermint(tm_cs), AnyConsensusState::Tendermint(tm_cons)) => {
                let time_elapsed = current_block_time.duration_since(tm_cons.timestamp);
                if time_elapsed.is_err() {
                    return ClientStatus::Unknown;
                }
                let time_elapsed = time_elapsed.expect("time elapsed is Ok");
                if tm_cs.expired(time_elapsed) {
                    return ClientStatus::Expired;
                }
            }
            (AnyClientState::Bankd(_), AnyConsensusState::Bankd(_)) => {
                // Bankd has no trusting period — clients don't expire from time alone.
                // They can only be frozen by misbehaviour evidence.
            }
            _ => {
                // Mismatched client/consensus types
                return ClientStatus::Unknown;
            }
        }

        ClientStatus::Active
    }

    async fn get_verified_heights(&self, client_id: &ClientId) -> Result<Option<VerifiedHeights>> {
        self.get(&format!(
            // NOTE: this is an implementation detail of the Penumbra ICS2 implementation, so
            // it's not in the same path namespace.
            "penumbra_verified_heights/{client_id}/verified_heights"
        ))
        .await
    }

    // returns the ConsensusState for the penumbra chain (this chain) at the given height
    async fn get_penumbra_sdk_consensus_state(
        &self,
        height: Height,
    ) -> Result<TendermintConsensusState> {
        // NOTE: this is an implementation detail of the Penumbra ICS2 implementation, so
        // it's not in the same path namespace.
        self.get(&format!("penumbra_consensus_states/{height}"))
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!("penumbra consensus state not found for height {height}")
            })
    }

    async fn get_verified_consensus_state(
        &self,
        height: &Height,
        client_id: &ClientId,
    ) -> Result<AnyConsensusState> {
        let raw_bytes = self
            .get_raw(
                &IBC_COMMITMENT_PREFIX
                    .apply_string(ClientConsensusStatePath::new(client_id, height).to_string()),
            )
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "counterparty consensus state not found for client {client_id} at height {height}"
                )
            })?;

        let any = ibc_proto::google::protobuf::Any::decode(raw_bytes.as_slice())
            .context("failed to decode consensus state as Any")?;
        AnyConsensusState::try_from(any)
    }

    async fn get_client_update_height(
        &self,
        client_id: &ClientId,
        height: &Height,
    ) -> Result<ibc_types::core::client::Height> {
        self.get(&state_key::client_processed_heights(client_id, height))
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "client update time not found for client {client_id} at height {height}"
                )
            })
    }

    async fn get_client_update_time(
        &self,
        client_id: &ClientId,
        height: &Height,
    ) -> Result<ibc_types::timestamp::Timestamp> {
        let timestamp_nanos = self
            .get_proto::<u64>(&state_key::client_processed_times(client_id, height))
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "client update time not found for client {client_id} at height {height}"
                )
            })?;

        ibc_types::timestamp::Timestamp::from_nanoseconds(timestamp_nanos)
            .context("invalid client update time")
    }

    // returns the lowest verified consensus state that is higher than the given height, if it
    // exists.
    async fn next_verified_consensus_state(
        &self,
        client_id: &ClientId,
        height: &Height,
    ) -> Result<Option<AnyConsensusState>> {
        let mut verified_heights =
            self.get_verified_heights(client_id)
                .await?
                .unwrap_or(VerifiedHeights {
                    heights: Vec::new(),
                });

        // WARNING: load-bearing sort
        verified_heights.heights.sort();

        if let Some(next_height) = verified_heights
            .heights
            .iter()
            .find(|&verified_height| verified_height > &height)
        {
            let next_cons_state = self
                .get_verified_consensus_state(next_height, client_id)
                .await?;
            return Ok(Some(next_cons_state));
        } else {
            return Ok(None);
        }
    }

    // returns the highest verified consensus state that is lower than the given height, if it
    // exists.
    async fn prev_verified_consensus_state(
        &self,
        client_id: &ClientId,
        height: &Height,
    ) -> Result<Option<AnyConsensusState>> {
        let mut verified_heights =
            self.get_verified_heights(client_id)
                .await?
                .unwrap_or(VerifiedHeights {
                    heights: Vec::new(),
                });

        // WARNING: load-bearing sort
        verified_heights.heights.sort();

        if let Some(prev_height) = verified_heights
            .heights
            .iter()
            .find(|&verified_height| verified_height < &height)
        {
            let prev_cons_state = self
                .get_verified_consensus_state(prev_height, client_id)
                .await?;
            return Ok(Some(prev_cons_state));
        } else {
            return Ok(None);
        }
    }
}

impl<T: StateRead + ?Sized> StateReadExt for T {}

#[cfg(test)]
mod tests {
    use base64::prelude::*;
    use std::sync::Arc;

    use super::*;
    use cnidarium::{ArcStateDeltaExt, StateDelta};
    use ibc_types::core::client::msgs::MsgUpdateClient;
    use ibc_types::{core::client::msgs::MsgCreateClient, DomainType};
    use penumbra_sdk_sct::component::clock::{EpochManager as _, EpochRead};
    use std::str::FromStr;
    use tendermint::Time;

    use crate::component::ibc_action_with_handler::IbcRelayWithHandlers;
    use crate::component::ClientStateReadExt;
    use crate::{IbcRelay, StateWriteExt};

    use crate::client_types::{
        BankdMisbehaviour, BANKD_CLIENT_STATE_TYPE_URL, BANKD_CONSENSUS_STATE_TYPE_URL,
        BANKD_HEADER_TYPE_URL, BANKD_MISBEHAVIOUR_TYPE_URL,
    };
    use crate::component::app_handler::{AppHandler, AppHandlerCheck, AppHandlerExecute};
    use ibc_types::core::channel::msgs::{
        MsgAcknowledgement, MsgChannelCloseConfirm, MsgChannelCloseInit, MsgChannelOpenAck,
        MsgChannelOpenConfirm, MsgChannelOpenInit, MsgChannelOpenTry, MsgRecvPacket, MsgTimeout,
    };
    use ibc_types::core::client::msgs::MsgSubmitMisbehaviour;

    struct MockHost {}

    #[async_trait]
    impl HostInterface for MockHost {
        async fn get_chain_id<S: StateRead>(_state: S) -> Result<String> {
            Ok("mock_chain_id".to_string())
        }

        async fn get_revision_number<S: StateRead>(_state: S) -> Result<u64> {
            Ok(0u64)
        }

        async fn get_block_height<S: StateRead>(state: S) -> Result<u64> {
            Ok(state.get_block_height().await?)
        }

        async fn get_block_timestamp<S: StateRead>(state: S) -> Result<tendermint::Time> {
            state.get_current_block_timestamp().await
        }
    }

    struct MockAppHandler {}

    #[async_trait]
    impl AppHandlerCheck for MockAppHandler {
        async fn chan_open_init_check<S: StateRead>(
            _state: S,
            _msg: &MsgChannelOpenInit,
        ) -> Result<()> {
            Ok(())
        }
        async fn chan_open_try_check<S: StateRead>(
            _state: S,
            _msg: &MsgChannelOpenTry,
        ) -> Result<()> {
            Ok(())
        }
        async fn chan_open_ack_check<S: StateRead>(
            _state: S,
            _msg: &MsgChannelOpenAck,
        ) -> Result<()> {
            Ok(())
        }
        async fn chan_open_confirm_check<S: StateRead>(
            _state: S,
            _msg: &MsgChannelOpenConfirm,
        ) -> Result<()> {
            Ok(())
        }
        async fn chan_close_confirm_check<S: StateRead>(
            _state: S,
            _msg: &MsgChannelCloseConfirm,
        ) -> Result<()> {
            Ok(())
        }
        async fn chan_close_init_check<S: StateRead>(
            _state: S,
            _msg: &MsgChannelCloseInit,
        ) -> Result<()> {
            Ok(())
        }
        async fn recv_packet_check<S: StateRead>(_state: S, _msg: &MsgRecvPacket) -> Result<()> {
            Ok(())
        }
        async fn timeout_packet_check<S: StateRead>(_state: S, _msg: &MsgTimeout) -> Result<()> {
            Ok(())
        }
        async fn acknowledge_packet_check<S: StateRead>(
            _state: S,
            _msg: &MsgAcknowledgement,
        ) -> Result<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl AppHandlerExecute for MockAppHandler {
        async fn chan_open_init_execute<S: StateWrite>(_state: S, _msg: &MsgChannelOpenInit) {}
        async fn chan_open_try_execute<S: StateWrite>(_state: S, _msg: &MsgChannelOpenTry) {}
        async fn chan_open_ack_execute<S: StateWrite>(_state: S, _msg: &MsgChannelOpenAck) {}
        async fn chan_open_confirm_execute<S: StateWrite>(_state: S, _msg: &MsgChannelOpenConfirm) {
        }
        async fn chan_close_confirm_execute<S: StateWrite>(
            _state: S,
            _msg: &MsgChannelCloseConfirm,
        ) {
        }
        async fn chan_close_init_execute<S: StateWrite>(_state: S, _msg: &MsgChannelCloseInit) {}
        async fn recv_packet_execute<S: StateWrite>(_state: S, _msg: &MsgRecvPacket) -> Result<()> {
            Ok(())
        }
        async fn timeout_packet_execute<S: StateWrite>(_state: S, _msg: &MsgTimeout) -> Result<()> {
            Ok(())
        }
        async fn acknowledge_packet_execute<S: StateWrite>(
            _state: S,
            _msg: &MsgAcknowledgement,
        ) -> Result<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl AppHandler for MockAppHandler {}

    // test that we can create and update a light client.
    #[tokio::test]
    async fn test_create_and_update_light_client() -> anyhow::Result<()> {
        use penumbra_sdk_sct::epoch::Epoch;
        // create a storage backend for testing

        // TODO(erwan): `apply_default_genesis` is not available here. We need a component
        // equivalent.
        let mut state = Arc::new(StateDelta::new(()));
        {
            // TODO: this is copied out of App::init_chain, can we put it somewhere else?
            let mut state_tx = state.try_begin_transaction().unwrap();
            state_tx.put_block_height(0);
            state_tx.put_epoch_by_height(
                0,
                Epoch {
                    index: 0,
                    start_height: 0,
                },
            );
            state_tx.put_epoch_by_height(
                1,
                Epoch {
                    index: 0,
                    start_height: 0,
                },
            );
            state_tx.apply();
        }

        // Light client verification is time-dependent.  In practice, the latest
        // (consensus) time will be delivered in each BeginBlock and written
        // into the state.  Here, set the block timestamp manually so it's
        // available to the unit test.
        let timestamp = Time::parse_from_rfc3339("2022-02-11T17:30:50.425417198Z")?;
        let mut state_tx = state.try_begin_transaction().unwrap();
        state_tx.put_block_timestamp(1u64, timestamp);
        state_tx.put_block_height(1);
        state_tx.put_ibc_params(crate::params::IBCParameters {
            ibc_enabled: true,
            inbound_ics20_transfers_enabled: true,
            outbound_ics20_transfers_enabled: true,
        });
        state_tx.put_epoch_by_height(
            1,
            Epoch {
                index: 0,
                start_height: 0,
            },
        );
        state_tx.apply();

        // base64 encoded MsgCreateClient that was used to create the currently in-use Stargaze
        // light client on the cosmos hub:
        // https://cosmos.bigdipper.live/transactions/13C1ECC54F088473E2925AD497DDCC092101ADE420BC64BADE67D34A75769CE9
        let msg_create_client_stargaze_raw = BASE64_STANDARD
            .decode(include_str!("./test/create_client.msg").replace('\n', ""))
            .unwrap();
        let msg_create_stargaze_client =
            MsgCreateClient::decode(msg_create_client_stargaze_raw.as_slice()).unwrap();

        // base64 encoded MsgUpdateClient that was used to issue the first update to the in-use stargaze light client on the cosmos hub:
        // https://cosmos.bigdipper.live/transactions/24F1E19F218CAF5CA41D6E0B653E85EB965843B1F3615A6CD7BCF336E6B0E707
        let msg_update_client_stargaze_raw = BASE64_STANDARD
            .decode(include_str!("./test/update_client_1.msg").replace('\n', ""))
            .unwrap();
        let mut msg_update_stargaze_client =
            MsgUpdateClient::decode(msg_update_client_stargaze_raw.as_slice()).unwrap();

        msg_update_stargaze_client.client_id = ClientId::from_str("07-tendermint-0").unwrap();

        let create_client_action = IbcRelayWithHandlers::<MockAppHandler, MockHost>::new(
            IbcRelay::CreateClient(msg_create_stargaze_client),
        );
        let update_client_action = IbcRelayWithHandlers::<MockAppHandler, MockHost>::new(
            IbcRelay::UpdateClient(msg_update_stargaze_client),
        );

        create_client_action.check_stateless(()).await?;
        create_client_action.check_historical(state.clone()).await?;
        let mut state_tx = state.try_begin_transaction().unwrap();
        create_client_action
            .check_and_execute(&mut state_tx)
            .await?;
        state_tx.apply();

        // Check that state reflects +1 client apps registered.
        assert_eq!(state.client_counter().await.unwrap().0, 1);

        // Now we update the client and confirm that the update landed in state.
        update_client_action.check_stateless(()).await?;
        update_client_action.check_historical(state.clone()).await?;
        let mut state_tx = state.try_begin_transaction().unwrap();
        update_client_action
            .check_and_execute(&mut state_tx)
            .await?;
        state_tx.apply();

        // We've had one client update, yes. What about second client update?
        // https://cosmos.bigdipper.live/transactions/ED217D360F51E622859F7B783FEF98BDE3544AA32BBD13C6C77D8D0D57A19FFD
        let msg_update_second = BASE64_STANDARD
            .decode(include_str!("./test/update_client_2.msg").replace('\n', ""))
            .unwrap();

        let mut second_update = MsgUpdateClient::decode(msg_update_second.as_slice()).unwrap();
        second_update.client_id = ClientId::from_str("07-tendermint-0").unwrap();
        let second_update_client_action = IbcRelayWithHandlers::<MockAppHandler, MockHost>::new(
            IbcRelay::UpdateClient(second_update),
        );

        second_update_client_action.check_stateless(()).await?;
        second_update_client_action
            .check_historical(state.clone())
            .await?;
        let mut state_tx = state.try_begin_transaction().unwrap();
        second_update_client_action
            .check_and_execute(&mut state_tx)
            .await?;
        state_tx.apply();

        Ok(())
    }

    #[tokio::test]
    /// Check that we're not able to create a client if the IBC component is disabled.
    async fn test_disabled_ibc_component() -> anyhow::Result<()> {
        let mut state = Arc::new(StateDelta::new(()));
        let mut state_tx = state.try_begin_transaction().unwrap();
        state_tx.put_ibc_params(crate::params::IBCParameters {
            ibc_enabled: false,
            inbound_ics20_transfers_enabled: true,
            outbound_ics20_transfers_enabled: true,
        });

        let msg_create_client_stargaze_raw = BASE64_STANDARD
            .decode(include_str!("./test/create_client.msg").replace('\n', ""))
            .unwrap();
        let msg_create_stargaze_client =
            MsgCreateClient::decode(msg_create_client_stargaze_raw.as_slice()).unwrap();

        let create_client_action = IbcRelayWithHandlers::<MockAppHandler, MockHost>::new(
            IbcRelay::CreateClient(msg_create_stargaze_client),
        );
        state_tx.apply();

        create_client_action.check_stateless(()).await?;
        create_client_action
            .check_historical(state.clone())
            .await
            .expect_err("should not be able to create a client");

        Ok(())
    }

    // ---------------------------------------------------------------
    // Bankd handler integration test helpers
    // ---------------------------------------------------------------

    fn bankd_test_keypair() -> (blst::min_sig::SecretKey, [u8; 96]) {
        let ikm = [42u8; 32];
        let sk = blst::min_sig::SecretKey::key_gen(&ikm, &[]).expect("keygen");
        let pk = sk.sk_to_pk();
        let pk_bytes: [u8; 96] = pk.compress();
        (sk, pk_bytes)
    }

    fn sign_bankd_header(sk: &blst::min_sig::SecretKey, header: &BankdHeader) -> Vec<u8> {
        let encoded = crate::bankd_provider::encode_block(header).expect("encode");
        let block_id = crate::bankd_provider::keccak256(&encoded);
        let consensus_digest = crate::bankd_provider::sha256(&block_id);
        let payload = crate::bankd_verification::union_unique(
            crate::bankd_verification::SIMPLEX_NAMESPACE,
            &consensus_digest,
        );
        let sig = sk.sign(&payload, crate::bankd_verification::BLS_DST, &[]);
        sig.compress().to_vec()
    }

    fn make_bankd_client_state(pk_bytes: &[u8; 96], height: u64) -> BankdClientState {
        BankdClientState {
            chain_id: "bankd-test-1".to_string(),
            latest_height: Some(ibc_proto::ibc::core::client::v1::Height {
                revision_number: 0,
                revision_height: height,
            }),
            frozen_height: None,
            proof_specs: vec![],
            group_public_key: pk_bytes.to_vec(),
            trusting_period_secs: 86_400,
        }
    }

    fn make_bankd_consensus_state(pk_bytes: &[u8; 96], timestamp: u64) -> BankdConsensusState {
        BankdConsensusState {
            root: vec![0xaa; 32],
            timestamp,
            group_public_key: pk_bytes.to_vec(),
        }
    }

    fn make_bankd_header(height: u64, timestamp: u64) -> BankdHeader {
        BankdHeader {
            height: Some(ibc_proto::ibc::core::client::v1::Height {
                revision_number: 0,
                revision_height: height,
            }),
            trusted_height: Some(ibc_proto::ibc::core::client::v1::Height {
                revision_number: 0,
                revision_height: height - 1,
            }),
            timestamp,
            new_root: vec![0xbb; 32],
            parent_hash: vec![0x01; 32],
            prevrandao: vec![0x02; 32],
            state_root: vec![0x03; 32],
            ibc_root: vec![0x04; 32],
            transactions: vec![vec![0x05; 64]],
            finalization_certificate: vec![], // filled in by sign_bankd_header
        }
    }

    /// Set up cnidarium state and create a bankd client via the handler.
    /// Returns (state, client_id, secret_key, public_key_bytes).
    async fn setup_bankd_client() -> anyhow::Result<(
        Arc<StateDelta<()>>,
        ClientId,
        blst::min_sig::SecretKey,
        [u8; 96],
    )> {
        use penumbra_sdk_sct::epoch::Epoch;

        let (sk, pk_bytes) = bankd_test_keypair();

        let mut state = Arc::new(StateDelta::new(()));
        {
            let mut state_tx = state.try_begin_transaction().unwrap();
            state_tx.put_block_height(0);
            state_tx.put_epoch_by_height(
                0,
                Epoch {
                    index: 0,
                    start_height: 0,
                },
            );
            state_tx.put_epoch_by_height(
                1,
                Epoch {
                    index: 0,
                    start_height: 0,
                },
            );
            state_tx.apply();
        }

        // Host time: 1,700,001,000 unix seconds (1000s after consensus state timestamp)
        let host_timestamp = Time::from_unix_timestamp(1_700_001_000, 0).expect("valid timestamp");
        {
            let mut state_tx = state.try_begin_transaction().unwrap();
            state_tx.put_block_timestamp(1u64, host_timestamp);
            state_tx.put_block_height(1);
            state_tx.put_ibc_params(crate::params::IBCParameters {
                ibc_enabled: true,
                inbound_ics20_transfers_enabled: true,
                outbound_ics20_transfers_enabled: true,
            });
            state_tx.put_epoch_by_height(
                1,
                Epoch {
                    index: 0,
                    start_height: 0,
                },
            );
            state_tx.apply();
        }

        // Client at height 10, consensus timestamp 1,700,000,000
        let bankd_cs = make_bankd_client_state(&pk_bytes, 10);
        let bankd_cons = make_bankd_consensus_state(&pk_bytes, 1_700_000_000);

        let msg_create = MsgCreateClient {
            client_state: ibc_proto::google::protobuf::Any {
                type_url: BANKD_CLIENT_STATE_TYPE_URL.to_string(),
                value: bankd_cs.encode_to_vec(),
            },
            consensus_state: ibc_proto::google::protobuf::Any {
                type_url: BANKD_CONSENSUS_STATE_TYPE_URL.to_string(),
                value: bankd_cons.encode_to_vec(),
            },
            signer: "test".to_string(),
        };

        let create_action = IbcRelayWithHandlers::<MockAppHandler, MockHost>::new(
            IbcRelay::CreateClient(msg_create),
        );

        create_action.check_stateless(()).await?;
        create_action.check_historical(state.clone()).await?;
        let mut state_tx = state.try_begin_transaction().unwrap();
        create_action.check_and_execute(&mut state_tx).await?;
        state_tx.apply();

        let client_id = ClientId::from_str("08-commonware-bls-0")?;

        Ok((state, client_id, sk, pk_bytes))
    }

    // ---------------------------------------------------------------
    // Priority 2: Full MsgCreateClient → MsgUpdateClient flow
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_bankd_create_and_update_client() -> anyhow::Result<()> {
        let (mut state, client_id, sk, _pk_bytes) = setup_bankd_client().await?;

        // Verify client counter incremented
        assert_eq!(state.client_counter().await?.0, 1);

        // Verify client state stored correctly
        let stored_cs = state.get_client_state(&client_id).await?;
        match &stored_cs {
            AnyClientState::Bankd(cs) => {
                assert_eq!(cs.chain_id, "bankd-test-1");
                assert_eq!(cs.latest_height.as_ref().unwrap().revision_height, 10);
                assert!(!stored_cs.is_frozen());
            }
            _ => panic!("expected Bankd client state"),
        }

        // Verify consensus state stored at height 10
        let height_10 = Height::new(0, 10)?;
        let cons_10 = state
            .get_verified_consensus_state(&height_10, &client_id)
            .await?;
        match &cons_10 {
            AnyConsensusState::Bankd(cs) => {
                assert_eq!(cs.root, vec![0xaa; 32]);
                assert_eq!(cs.timestamp, 1_700_000_000);
            }
            _ => panic!("expected Bankd consensus state"),
        }

        // Build a valid bankd header at height 11, sign with BLS key
        let mut header = make_bankd_header(11, 1_700_000_500);
        header.finalization_certificate = sign_bankd_header(&sk, &header);

        let msg_update = MsgUpdateClient {
            client_id: client_id.clone(),
            client_message: ibc_proto::google::protobuf::Any {
                type_url: BANKD_HEADER_TYPE_URL.to_string(),
                value: header.encode_to_vec(),
            },
            signer: "test".to_string(),
        };

        let update_action = IbcRelayWithHandlers::<MockAppHandler, MockHost>::new(
            IbcRelay::UpdateClient(msg_update),
        );

        update_action.check_stateless(()).await?;
        update_action.check_historical(state.clone()).await?;
        let mut state_tx = state.try_begin_transaction().unwrap();
        update_action.check_and_execute(&mut state_tx).await?;
        state_tx.apply();

        // Verify client state updated to height 11
        let updated_cs = state.get_client_state(&client_id).await?;
        match &updated_cs {
            AnyClientState::Bankd(cs) => {
                assert_eq!(cs.latest_height.as_ref().unwrap().revision_height, 11);
            }
            _ => panic!("expected Bankd client state"),
        }

        // Verify new consensus state stored at height 11
        let height_11 = Height::new(0, 11)?;
        let cons_11 = state
            .get_verified_consensus_state(&height_11, &client_id)
            .await?;
        match &cons_11 {
            AnyConsensusState::Bankd(cs) => {
                assert_eq!(cs.root, vec![0xbb; 32]); // new_root from header
                assert_eq!(cs.timestamp, 1_700_000_500);
            }
            _ => panic!("expected Bankd consensus state"),
        }

        // Original consensus state at height 10 should still be there
        let cons_10_after = state
            .get_verified_consensus_state(&height_10, &client_id)
            .await?;
        match cons_10_after {
            AnyConsensusState::Bankd(cs) => {
                assert_eq!(cs.timestamp, 1_700_000_000);
            }
            _ => panic!("expected Bankd consensus state"),
        }

        Ok(())
    }

    // ---------------------------------------------------------------
    // Priority 3: Client status checks
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_bankd_client_status_active() -> anyhow::Result<()> {
        let (state, client_id, _sk, _pk) = setup_bankd_client().await?;

        let current_time = Time::from_unix_timestamp(1_700_001_000, 0)?;
        let status = state.get_client_status(&client_id, current_time).await;
        assert_eq!(status, ClientStatus::Active);

        Ok(())
    }

    #[tokio::test]
    async fn test_bankd_client_does_not_expire() -> anyhow::Result<()> {
        let (state, client_id, _sk, _pk) = setup_bankd_client().await?;

        // Far-future time: 1 year after consensus state timestamp
        let far_future = Time::from_unix_timestamp(1_700_000_000 + 365 * 86_400, 0)?;
        let status = state.get_client_status(&client_id, far_future).await;
        assert_eq!(status, ClientStatus::Active);

        Ok(())
    }

    #[tokio::test]
    async fn test_bankd_client_status_frozen_after_misbehaviour() -> anyhow::Result<()> {
        let (mut state, client_id, sk, _pk) = setup_bankd_client().await?;

        // Build two headers at height 11 with different data (equivocation)
        let mut h1 = make_bankd_header(11, 1_700_000_500);
        h1.finalization_certificate = sign_bankd_header(&sk, &h1);

        let mut h2 = make_bankd_header(11, 1_700_000_500);
        h2.state_root = vec![0xff; 32]; // different state_root = different block
        h2.finalization_certificate = sign_bankd_header(&sk, &h2);

        let mb = BankdMisbehaviour {
            client_id: client_id.to_string(),
            header_1: Some(h1),
            header_2: Some(h2),
        };

        let msg = MsgSubmitMisbehaviour {
            client_id: client_id.clone(),
            misbehaviour: ibc_proto::google::protobuf::Any {
                type_url: BANKD_MISBEHAVIOUR_TYPE_URL.to_string(),
                value: prost::Message::encode_to_vec(&mb),
            },
            signer: "test".to_string(),
        };

        let misbehaviour_action =
            IbcRelayWithHandlers::<MockAppHandler, MockHost>::new(IbcRelay::SubmitMisbehavior(msg));

        misbehaviour_action.check_stateless(()).await?;
        misbehaviour_action.check_historical(state.clone()).await?;
        let mut state_tx = state.try_begin_transaction().unwrap();
        misbehaviour_action.check_and_execute(&mut state_tx).await?;
        state_tx.apply();

        // Client should now be frozen
        let current_time = Time::from_unix_timestamp(1_700_001_000, 0)?;
        let status = state.get_client_status(&client_id, current_time).await;
        assert_eq!(status, ClientStatus::Frozen);

        // Verify the client state is_frozen flag
        let cs = state.get_client_state(&client_id).await?;
        assert!(cs.is_frozen());

        Ok(())
    }

    // ---------------------------------------------------------------
    // Priority 4: Misbehaviour edge cases
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_bankd_misbehaviour_invalid_signature_rejected() -> anyhow::Result<()> {
        let (mut state, client_id, sk, _pk) = setup_bankd_client().await?;

        // Sign header_1 with the correct key, header_2 with a different key
        let mut h1 = make_bankd_header(11, 1_700_000_500);
        h1.finalization_certificate = sign_bankd_header(&sk, &h1);

        let mut h2 = make_bankd_header(11, 1_700_000_500);
        h2.state_root = vec![0xff; 32]; // different block
                                        // Sign with a wrong key
        let wrong_ikm = [99u8; 32];
        let wrong_sk = blst::min_sig::SecretKey::key_gen(&wrong_ikm, &[]).expect("keygen");
        h2.finalization_certificate = sign_bankd_header(&wrong_sk, &h2);

        let mb = BankdMisbehaviour {
            client_id: client_id.to_string(),
            header_1: Some(h1),
            header_2: Some(h2),
        };

        let msg = MsgSubmitMisbehaviour {
            client_id: client_id.clone(),
            misbehaviour: ibc_proto::google::protobuf::Any {
                type_url: BANKD_MISBEHAVIOUR_TYPE_URL.to_string(),
                value: prost::Message::encode_to_vec(&mb),
            },
            signer: "test".to_string(),
        };

        let misbehaviour_action =
            IbcRelayWithHandlers::<MockAppHandler, MockHost>::new(IbcRelay::SubmitMisbehavior(msg));

        // Stateless checks should pass (format is valid)
        misbehaviour_action.check_stateless(()).await?;
        misbehaviour_action.check_historical(state.clone()).await?;

        // Execution should fail because header_2's BLS signature is invalid
        let mut state_tx = state.try_begin_transaction().unwrap();
        let result = misbehaviour_action.check_and_execute(&mut state_tx).await;
        assert!(
            result.is_err(),
            "should reject misbehaviour with invalid BLS signature"
        );

        // Client should NOT be frozen (misbehaviour was rejected)
        let cs = state.get_client_state(&client_id).await?;
        assert!(!cs.is_frozen());

        Ok(())
    }
}
