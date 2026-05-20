use std::{str::FromStr, time::Instant};

use crate::{
    component::{AssetRegistry, NoteManager},
    event::{self, FungibleTokenTransferPacketMetadata},
    Ics20Withdrawal,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use cnidarium::{StateRead, StateWrite};
use ibc_types::core::channel::Packet;
use ibc_types::{
    core::channel::{
        channel::Order as ChannelOrder,
        msgs::{
            MsgAcknowledgement, MsgChannelCloseConfirm, MsgChannelCloseInit, MsgChannelOpenAck,
            MsgChannelOpenConfirm, MsgChannelOpenInit, MsgChannelOpenTry, MsgRecvPacket,
            MsgTimeout,
        },
        ChannelId, PortId, Version,
    },
    transfer::acknowledgement::TokenTransferAcknowledgement,
};
use penumbra_sdk_asset::{asset, asset::Metadata, Value};
use penumbra_sdk_compliance::{ComplianceRegistryRead as _, IbcComplianceMetadata, IbcRoute};
use penumbra_sdk_ibc::component::{ChannelStateReadExt, ConnectionStateReadExt};
use penumbra_sdk_keys::Address;
use penumbra_sdk_num::Amount;
use penumbra_sdk_proto::{
    penumbra::core::component::ibc::v1::FungibleTokenPacketData, DomainType as _, StateReadProto,
    StateWriteProto,
};
use penumbra_sdk_sct::CommitmentSource;

#[cfg(feature = "benchmark-helpers")]
use penumbra_sdk_ibc::benchmarking::{record_inbound_stage, InboundStage};
use penumbra_sdk_ibc::component::{
    app_handler::{AppHandler, AppHandlerCheck, AppHandlerExecute},
    packet::{
        IBCPacket, SendPacketRead as _, SendPacketWrite as _, Unchecked, WriteAcknowledgement as _,
    },
    state_key,
};
use tendermint::Time;

// returns a bool indicating if the provided denom was issued locally or if it was bridged in.
// this logic is a bit tricky, and adapted from https://github.com/cosmos/ibc/tree/main/spec/app/ics-020-fungible-token-transfer (sendFungibleTokens).
//
// what we want to do is to determine if the denom being withdrawn is a native token (one
// that originates from Penumbra) or a bridged token (one that was sent into penumbra from
// IBC).
//
// A simple way of doing this is by parsing the denom, looking for a prefix that is only
// appended in the case of a bridged token. That is what this logic does.
//
// note that in the case of a refund, eg. when this function is called from `onTimeoutPacket`,
// the logic is inverted, as a prefix will only be prepended in the case the token is bridged in.
fn is_source(
    source_port: &PortId,
    source_channel: &ChannelId,
    denom: &Metadata,
    is_refund: bool,
) -> bool {
    let prefix = format!("{source_port}/{source_channel}/");

    if is_refund {
        !denom.starts_with(&prefix)
    } else {
        denom.starts_with(&prefix)
    }
}

async fn resolve_ibc_route<S: StateRead + ?Sized>(
    state: &S,
    local_port: &PortId,
    local_channel: &ChannelId,
) -> Result<IbcRoute> {
    let channel = state
        .get_channel(local_channel, local_port)
        .await?
        .ok_or_else(|| anyhow::anyhow!("IBC route channel not found"))?;
    let connection_id = channel
        .connection_hops
        .first()
        .ok_or_else(|| anyhow::anyhow!("IBC route channel has no connection hop"))?;
    state
        .get_connection(connection_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("IBC route connection not found"))?;
    let counterparty_channel = channel
        .counterparty()
        .channel_id()
        .ok_or_else(|| anyhow::anyhow!("IBC route missing counterparty channel"))?;
    Ok(IbcRoute {
        local_port: local_port.to_string(),
        local_channel: local_channel.to_string(),
        connection_id: connection_id.to_string(),
        counterparty_port: channel.counterparty().port_id.to_string(),
        counterparty_channel: counterparty_channel.to_string(),
    })
}

fn received_asset_metadata(
    msg: &MsgRecvPacket,
    packet_data: &FungibleTokenPacketData,
    packet_denom: &asset::Metadata,
) -> Result<(asset::Metadata, bool)> {
    if is_source(
        &msg.packet.port_on_a,
        &msg.packet.chan_on_a,
        packet_denom,
        false,
    ) {
        let prefix = format!(
            "{source_port}/{source_chan}/",
            source_port = msg.packet.port_on_a,
            source_chan = msg.packet.chan_on_a
        );

        let denom: asset::Metadata = packet_data
            .denom
            .strip_prefix(&prefix)
            .context(format!(
                "denom in packet didn't begin with expected prefix {}",
                prefix
            ))?
            .try_into()
            .context("couldnt decode denom in ICS20 transfer")?;
        Ok((denom, true))
    } else {
        let prefixed_denomination = format!(
            "{}/{}/{}",
            msg.packet.port_on_b, msg.packet.chan_on_b, packet_data.denom
        );
        let denom: asset::Metadata = prefixed_denomination
            .as_str()
            .try_into()
            .context("unable to parse denom in ics20 transfer as DenomMetadata")?;
        Ok((denom, false))
    }
}

#[derive(Clone)]
struct Ics20ReceiveContext {
    packet_data: FungibleTokenPacketData,
    received_denom: asset::Metadata,
    returned_to_source: bool,
    receiver_amount: Amount,
    receiver_address: Address,
    compliance_metadata: Option<IbcComplianceMetadata>,
}

impl Ics20ReceiveContext {
    fn parse(msg: &MsgRecvPacket) -> Result<Self> {
        // NOTE: spec says proto but this is actually JSON according to the ibc-go implementation.
        let packet_data: FungibleTokenPacketData =
            serde_json::from_slice(msg.packet.data.as_slice())
                .with_context(|| "failed to decode FTPD packet")?;
        let packet_denom: asset::Metadata = packet_data
            .denom
            .as_str()
            .try_into()
            .context("couldnt decode denom in ICS20 transfer")?;
        let receiver_amount: Amount = packet_data
            .amount
            .clone()
            .try_into()
            .context("couldnt decode amount in ICS20 transfer")?;
        let receiver_address = Address::from_str(&packet_data.receiver)?;

        let compliance_metadata = IbcComplianceMetadata::from_memo(&packet_data.memo)
            .unwrap_or_else(|e| {
                tracing::debug!(?e, "failed to parse compliance metadata from ICS-20 memo");
                None
            });

        let (received_denom, returned_to_source) =
            received_asset_metadata(msg, &packet_data, &packet_denom)?;

        Ok(Self {
            packet_data,
            received_denom,
            returned_to_source,
            receiver_amount,
            receiver_address,
            compliance_metadata,
        })
    }
}

#[cfg(feature = "benchmark-helpers")]
pub(crate) fn benchmark_parse_ics20_receive_context(
    msg: &MsgRecvPacket,
) -> Result<(asset::Id, bool, Amount)> {
    let context = Ics20ReceiveContext::parse(msg)?;
    Ok((
        context.received_denom.id(),
        context.returned_to_source,
        context.receiver_amount,
    ))
}

async fn check_regulated_inbound_ics20<S: StateRead>(
    state: &S,
    route: &IbcRoute,
    context: &Ics20ReceiveContext,
) -> Result<()> {
    let received_asset_id = context.received_denom.id();
    let mut policy = state.get_asset_policy(received_asset_id).await?;

    if !context.returned_to_source {
        if let Some(origin_asset_id) = state
            .get_ibc_origin_asset_id(&context.packet_data.denom)
            .await?
        {
            anyhow::ensure!(
                origin_asset_id == received_asset_id,
                "regulated IBC origin {} arrived as unexpected asset {}",
                context.packet_data.denom,
                received_asset_id
            );
            if policy.is_none() {
                policy = state.get_asset_policy(origin_asset_id).await?;
            }
            anyhow::ensure!(
                policy.is_some(),
                "regulated IBC origin {} has no registered asset policy",
                context.packet_data.denom
            );
        }
    }

    if let Some(metadata) = &context.compliance_metadata {
        anyhow::ensure!(
            metadata.asset_id == received_asset_id,
            "IBC compliance metadata asset_id does not match received asset"
        );
        anyhow::ensure!(
            policy.is_some(),
            "IBC compliance metadata present for unregistered asset"
        );
    }

    if let Some(policy) = policy {
        IbcComplianceMetadata::validate_regulated_memo(&context.packet_data.memo)?;
        anyhow::ensure!(
            policy.permits_ibc_route(&route),
            "regulated asset is not allowed on IBC route {}:{} via {} to {}:{}",
            route.local_port,
            route.local_channel,
            route.connection_id,
            route.counterparty_port,
            route.counterparty_channel
        );
    }

    Ok(())
}

#[derive(Clone)]
pub struct Ics20Transfer {}

#[async_trait]
pub trait Ics20TransferExecutionExt: StateWrite {
    async fn withdrawal_check_cached(
        &mut self,
        withdrawal: &Ics20Withdrawal,
        current_block_time: Time,
    ) -> Result<()> {
        let packet: IBCPacket<Unchecked> = withdrawal.clone().into();
        let send_check_start = Instant::now();
        self.send_packet_check(packet, current_block_time).await?;
        tracing::debug!(
            elapsed_us = send_check_start.elapsed().as_micros(),
            channel = %withdrawal.source_channel,
            "ibc_outbound_send_packet_check"
        );

        let route_policy_start = Instant::now();
        if let Some(policy) = self.get_asset_policy(withdrawal.denom.id()).await? {
            IbcComplianceMetadata::validate_regulated_memo(&withdrawal.ics20_memo)?;
            let route =
                resolve_ibc_route(self, &PortId::transfer(), &withdrawal.source_channel).await?;
            anyhow::ensure!(
                policy.permits_ibc_route(&route),
                "regulated asset is not allowed on IBC route {}:{} via {} to {}:{}",
                route.local_port,
                route.local_channel,
                route.connection_id,
                route.counterparty_port,
                route.counterparty_channel
            );
        }
        tracing::debug!(
            elapsed_us = route_policy_start.elapsed().as_micros(),
            asset_id = %withdrawal.denom.id(),
            channel = %withdrawal.source_channel,
            "ibc_outbound_route_policy_check"
        );

        Ok(())
    }
}

impl<T: StateWrite + ?Sized> Ics20TransferExecutionExt for T {}

#[async_trait]
pub trait Ics20TransferWriteExt: StateWrite {
    async fn withdrawal_execute(&mut self, withdrawal: &Ics20Withdrawal) -> Result<()> {
        // create packet, assume it's already checked since the component caller contract calls `check` before `execute`
        let checked_packet = IBCPacket::<Unchecked>::from(withdrawal.clone()).assume_checked();

        let accounting_start = Instant::now();
        let prefix = format!("transfer/{}/", &withdrawal.source_channel);
        if !withdrawal.denom.starts_with(&prefix) {
            // we are the source. add the value balance to the escrow channel.
            let existing_value_balance: Amount = self
                .get(&state_key::ics20_value_balance::by_asset_id(
                    &withdrawal.source_channel,
                    &withdrawal.denom.id(),
                ))
                .await
                .expect("able to retrieve value balance in ics20 withdrawal! (execute)")
                .unwrap_or_else(Amount::zero);

            let new_value_balance = existing_value_balance
                .checked_add(&withdrawal.amount)
                .ok_or_else(|| {
                    anyhow::anyhow!("overflow adding value balance in ics20 withdrawal")
                })?;
            self.put(
                state_key::ics20_value_balance::by_asset_id(
                    &withdrawal.source_channel,
                    &withdrawal.denom.id(),
                ),
                new_value_balance,
            );
            self.record_proto(
                event::EventOutboundFungibleTokenTransfer {
                    value: Value {
                        amount: withdrawal.amount,
                        asset_id: withdrawal.denom.id(),
                    },
                    sender: withdrawal.return_address.clone(),
                    receiver: withdrawal.destination_chain_address.clone(),
                    meta: FungibleTokenTransferPacketMetadata {
                        channel: withdrawal.source_channel.0.clone(),
                        sequence: self
                            .get_send_sequence(
                                &withdrawal.source_channel,
                                &checked_packet.source_port(),
                            )
                            .await?,
                    },
                }
                .to_proto(),
            );
        } else {
            // receiver is the source, burn utxos

            // double check the value balance here.
            //
            // for assets not originating from Penumbra, never transfer out more tokens than were
            // transferred in. (Our counterparties should be checking this anyways, since if we
            // were Byzantine we could lie to them).
            let value_balance: Amount = self
                .get(&state_key::ics20_value_balance::by_asset_id(
                    &withdrawal.source_channel,
                    &withdrawal.denom.id(),
                ))
                .await?
                .unwrap_or_else(Amount::zero);

            if value_balance < withdrawal.amount {
                anyhow::bail!("insufficient balance to withdraw tokens");
            }

            let new_value_balance =
                value_balance
                    .checked_sub(&withdrawal.amount)
                    .ok_or_else(|| {
                        anyhow::anyhow!("underflow subtracting value balance in ics20 withdrawal")
                    })?;
            self.put(
                state_key::ics20_value_balance::by_asset_id(
                    &withdrawal.source_channel,
                    &withdrawal.denom.id(),
                ),
                new_value_balance,
            );
            self.record_proto(
                event::EventOutboundFungibleTokenTransfer {
                    value: Value {
                        amount: withdrawal.amount,
                        asset_id: withdrawal.denom.id(),
                    },
                    sender: withdrawal.return_address.clone(),
                    receiver: withdrawal.destination_chain_address.clone(),
                    meta: FungibleTokenTransferPacketMetadata {
                        channel: withdrawal.source_channel.0.clone(),
                        sequence: self
                            .get_send_sequence(
                                &withdrawal.source_channel,
                                &checked_packet.source_port(),
                            )
                            .await?,
                    },
                }
                .to_proto(),
            );
        }
        tracing::debug!(
            elapsed_us = accounting_start.elapsed().as_micros(),
            asset_id = %withdrawal.denom.id(),
            channel = %withdrawal.source_channel,
            "ibc_outbound_nullifier_note_accounting"
        );

        let send_execute_start = Instant::now();
        self.send_packet_execute(checked_packet).await;
        tracing::debug!(
            elapsed_us = send_execute_start.elapsed().as_micros(),
            channel = %withdrawal.source_channel,
            "ibc_outbound_send_packet_execute"
        );

        Ok(())
    }
}

impl<T: StateWrite + ?Sized> Ics20TransferWriteExt for T {}

// see: https://github.com/cosmos/ibc/tree/master/spec/app/ics-020-fungible-token-transfer
#[async_trait]
impl AppHandlerCheck for Ics20Transfer {
    async fn chan_open_init_check<S: StateRead>(_state: S, msg: &MsgChannelOpenInit) -> Result<()> {
        if msg.ordering != ChannelOrder::Unordered {
            anyhow::bail!("channel order must be unordered for Ics20 transfer");
        }
        let ics20_version = Version::new("ics20-1".to_string());
        if msg.version_proposal != ics20_version {
            anyhow::bail!("channel version must be ics20 for Ics20 transfer");
        }

        Ok(())
    }

    async fn chan_open_try_check<S: StateRead>(_state: S, msg: &MsgChannelOpenTry) -> Result<()> {
        if msg.ordering != ChannelOrder::Unordered {
            anyhow::bail!("channel order must be unordered for Ics20 transfer");
        }
        let ics20_version = Version::new("ics20-1".to_string());

        if msg.version_supported_on_a != ics20_version {
            anyhow::bail!("counterparty version must be ics20-1 for Ics20 transfer");
        }

        Ok(())
    }

    async fn chan_open_ack_check<S: StateRead>(_state: S, msg: &MsgChannelOpenAck) -> Result<()> {
        let ics20_version = Version::new("ics20-1".to_string());
        if msg.version_on_b != ics20_version {
            anyhow::bail!("counterparty version must be ics20-1 for Ics20 transfer");
        }

        Ok(())
    }

    async fn chan_open_confirm_check<S: StateRead>(
        _state: S,
        _msg: &MsgChannelOpenConfirm,
    ) -> Result<()> {
        // accept channel confirmations, port has already been validated, version has already been validated
        Ok(())
    }

    async fn chan_close_confirm_check<S: StateRead>(
        _state: S,
        _msg: &MsgChannelCloseConfirm,
    ) -> Result<()> {
        // no action necessary
        Ok(())
    }

    async fn chan_close_init_check<S: StateRead>(
        _state: S,
        _msg: &MsgChannelCloseInit,
    ) -> Result<()> {
        // always abort transaction
        anyhow::bail!("ics20 always aborts on close init");
    }

    async fn recv_packet_check<S: StateRead>(_state: S, _msg: &MsgRecvPacket) -> Result<()> {
        // all checks on recv_packet done in execute
        Ok(())
    }

    async fn timeout_packet_check<S: StateRead>(state: S, msg: &MsgTimeout) -> Result<()> {
        let packet_data: FungibleTokenPacketData =
            serde_json::from_slice(msg.packet.data.as_slice())?;
        let denom: asset::Metadata = packet_data.denom.as_str().try_into()?;

        if is_source(&msg.packet.port_on_a, &msg.packet.chan_on_a, &denom, true) {
            // check if we have enough balance to refund tokens to sender
            let value_balance: Amount = state
                .get(&state_key::ics20_value_balance::by_asset_id(
                    &msg.packet.chan_on_a,
                    &denom.id(),
                ))
                .await?
                .unwrap_or_else(Amount::zero);

            let amount_penumbra: Amount = packet_data.amount.try_into()?;
            if value_balance < amount_penumbra {
                anyhow::bail!("insufficient balance to refund tokens to sender");
            }
        }

        Ok(())
    }

    async fn acknowledge_packet_check<S: StateRead>(
        _state: S,
        _msg: &MsgAcknowledgement,
    ) -> Result<()> {
        Ok(())
    }
}

// the main entry point for ICS20 transfer packet handling
async fn recv_transfer_packet_inner<S: StateWrite>(
    mut state: S,
    msg: &MsgRecvPacket,
) -> Result<()> {
    // parse if we are source or dest, and mint or burn accordingly
    //
    // see this part of the spec for this logic:
    //
    // https://github.com/cosmos/ibc/tree/main/spec/app/ics-020-fungible-token-transfer (onRecvPacket)
    let decode_start = Instant::now();
    let context = Ics20ReceiveContext::parse(msg)?;
    let decode_elapsed = decode_start.elapsed();
    #[cfg(feature = "benchmark-helpers")]
    record_inbound_stage(InboundStage::PacketDataDecode, decode_elapsed);
    tracing::debug!(
        elapsed_us = decode_elapsed.as_micros(),
        sequence = %msg.packet.sequence,
        returned_to_source = context.returned_to_source,
        asset_id = %context.received_denom.id(),
        "ibc_recv_packet_data_decode"
    );

    let route_start = Instant::now();
    let route = resolve_ibc_route(&state, &msg.packet.port_on_b, &msg.packet.chan_on_b).await?;
    let route_elapsed = route_start.elapsed();
    #[cfg(feature = "benchmark-helpers")]
    record_inbound_stage(InboundStage::RouteResolve, route_elapsed);
    tracing::debug!(
        elapsed_us = route_elapsed.as_micros(),
        local_port = %route.local_port,
        local_channel = %route.local_channel,
        connection_id = %route.connection_id,
        counterparty_port = %route.counterparty_port,
        counterparty_channel = %route.counterparty_channel,
        "ibc_recv_route_resolve"
    );

    let compliance_start = Instant::now();
    check_regulated_inbound_ics20(&state, &route, &context).await?;
    let compliance_elapsed = compliance_start.elapsed();
    #[cfg(feature = "benchmark-helpers")]
    record_inbound_stage(InboundStage::ComplianceCheck, compliance_elapsed);
    tracing::debug!(
        elapsed_us = compliance_elapsed.as_micros(),
        asset_id = %context.received_denom.id(),
        "ibc_recv_compliance_check"
    );
    // NOTE: here we assume we are chain A.

    // 2. check if we are the source chain for the denom.
    let mint_unescrow_start = Instant::now();
    if context.returned_to_source {
        // mint tokens to receiver in the amount of packet_data.amount in the denom of denom (with
        // the source removed, since we're the source)
        let denom = context.received_denom.clone();

        let value: Value = Value {
            amount: context.receiver_amount,
            asset_id: denom.id(),
        };

        // assume AppHandlerCheck has already been called, and we have enough balance to mint tokens to receiver
        // check if we have enough balance to unescrow tokens to receiver
        #[cfg(feature = "benchmark-helpers")]
        let value_balance_read_start = Instant::now();
        let value_balance: Amount = state
            .get(&state_key::ics20_value_balance::by_asset_id(
                &msg.packet.chan_on_b,
                &denom.id(),
            ))
            .await?
            .unwrap_or_else(Amount::zero);
        #[cfg(feature = "benchmark-helpers")]
        record_inbound_stage(
            InboundStage::ValueBalanceRead,
            value_balance_read_start.elapsed(),
        );

        if value_balance < context.receiver_amount {
            // error text here is from the ics20 spec
            anyhow::bail!("transfer coins failed");
        }

        state
            .mint_note(
                value,
                &context.receiver_address,
                CommitmentSource::Ics20Transfer {
                    packet_seq: msg.packet.sequence.0,
                    // We are chain A
                    channel_id: msg.packet.chan_on_a.0.clone(),
                    sender: context.packet_data.sender.clone(),
                },
            )
            .await
            .context("unable to mint note when receiving ics20 transfer packet")?;

        // update the value balance
        // note: this arithmetic was checked above, but we do it again anyway.
        let new_value_balance = value_balance
            .checked_sub(&context.receiver_amount)
            .context("underflow subtracing value balance in ics20 transfer")?;
        #[cfg(feature = "benchmark-helpers")]
        let value_balance_write_start = Instant::now();
        state.put(
            state_key::ics20_value_balance::by_asset_id(&msg.packet.chan_on_b, &denom.id()),
            new_value_balance,
        );
        #[cfg(feature = "benchmark-helpers")]
        record_inbound_stage(
            InboundStage::ValueBalanceWrite,
            value_balance_write_start.elapsed(),
        );
        #[cfg(feature = "benchmark-helpers")]
        let event_record_start = Instant::now();
        state.record_proto(
            event::EventInboundFungibleTokenTransfer {
                value,
                sender: context.packet_data.sender.clone(),
                receiver: context.receiver_address.clone(),
                meta: FungibleTokenTransferPacketMetadata {
                    channel: msg.packet.chan_on_a.0.clone(),
                    sequence: msg.packet.sequence.0,
                },
            }
            .to_proto(),
        );
        #[cfg(feature = "benchmark-helpers")]
        record_inbound_stage(InboundStage::EventRecord, event_record_start.elapsed());
    } else {
        // create new denom:
        //
        // prefix = "{packet.destPort}/{packet.destChannel}/"
        // prefixedDenomination = prefix + data.denom
        //
        // then mint that denom to packet_data.receiver in packet_data.amount
        let denom = context.received_denom.clone();
        #[cfg(feature = "benchmark-helpers")]
        let register_denom_start = Instant::now();
        state.register_denom(&denom).await;
        #[cfg(feature = "benchmark-helpers")]
        record_inbound_stage(InboundStage::RegisterDenom, register_denom_start.elapsed());

        let value = Value {
            amount: context.receiver_amount,
            asset_id: denom.id(),
        };

        state
            .mint_note(
                value,
                &context.receiver_address,
                CommitmentSource::Ics20Transfer {
                    packet_seq: msg.packet.sequence.0,
                    // We are chain A
                    channel_id: msg.packet.chan_on_a.0.clone(),
                    sender: context.packet_data.sender.clone(),
                },
            )
            .await
            .context("failed to mint notes in ibc transfer")?;

        // update the value balance
        #[cfg(feature = "benchmark-helpers")]
        let value_balance_read_start = Instant::now();
        let value_balance: Amount = state
            .get(&state_key::ics20_value_balance::by_asset_id(
                &msg.packet.chan_on_b,
                &denom.id(),
            ))
            .await?
            .unwrap_or_else(Amount::zero);
        #[cfg(feature = "benchmark-helpers")]
        record_inbound_stage(
            InboundStage::ValueBalanceRead,
            value_balance_read_start.elapsed(),
        );

        let new_value_balance = value_balance.saturating_add(&value.amount);
        #[cfg(feature = "benchmark-helpers")]
        let value_balance_write_start = Instant::now();
        state.put(
            state_key::ics20_value_balance::by_asset_id(&msg.packet.chan_on_b, &denom.id()),
            new_value_balance,
        );
        #[cfg(feature = "benchmark-helpers")]
        record_inbound_stage(
            InboundStage::ValueBalanceWrite,
            value_balance_write_start.elapsed(),
        );
        #[cfg(feature = "benchmark-helpers")]
        let event_record_start = Instant::now();
        state.record_proto(
            event::EventInboundFungibleTokenTransfer {
                value,
                sender: context.packet_data.sender.clone(),
                receiver: context.receiver_address.clone(),
                meta: FungibleTokenTransferPacketMetadata {
                    channel: msg.packet.chan_on_a.0.clone(),
                    sequence: msg.packet.sequence.0,
                },
            }
            .to_proto(),
        );
        #[cfg(feature = "benchmark-helpers")]
        record_inbound_stage(InboundStage::EventRecord, event_record_start.elapsed());
    }
    let mint_unescrow_elapsed = mint_unescrow_start.elapsed();
    #[cfg(feature = "benchmark-helpers")]
    record_inbound_stage(InboundStage::MintUnescrowAccounting, mint_unescrow_elapsed);
    tracing::debug!(
        elapsed_us = mint_unescrow_elapsed.as_micros(),
        returned_to_source = context.returned_to_source,
        asset_id = %context.received_denom.id(),
        "ibc_recv_mint_unescrow_accounting"
    );

    // Store compliance metadata if present in memo.
    if let Some(metadata) = context.compliance_metadata {
        use penumbra_sdk_compliance::ComplianceRegistryWrite as _;
        state.store_ibc_compliance_metadata(
            &msg.packet.chan_on_a.0,
            msg.packet.sequence.0,
            &metadata,
        );
    }

    Ok(())
}

// see: https://github.com/cosmos/ibc/blob/8326e26e7e1188b95c32481ff00348a705b23700/spec/app/ics-020-fungible-token-transfer/README.md?plain=1#L297
async fn refund_tokens<S: StateWrite>(
    mut state: S,
    packet: &Packet,
    reason: event::FungibleTokenRefundReason,
) -> Result<()> {
    let packet_data: FungibleTokenPacketData = serde_json::from_slice(packet.data.as_slice())?;
    let denom: asset::Metadata = packet_data // CRITICAL: verify that this denom is validated in upstream timeout handling
        .denom
        .as_str()
        .try_into()
        .context("couldn't decode denom in ics20 transfer timeout")?;
    // receiver was source chain, mint vouchers back to sender
    let amount: Amount = packet_data
        .amount
        .try_into()
        .context("couldn't decode amount in ics20 transfer timeout")?;

    // packet_data.sender is the original sender for this packet that was not committed on the
    // other chain but was sent from penumbra. so, the penumbra refund receiver address is the
    // sender
    let receiver = Address::from_str(&packet_data.sender)
        .context("couldn't decode receiver address in ics20 timeout")?;

    let value: Value = Value {
        amount,
        asset_id: denom.id(),
    };

    if is_source(&packet.port_on_a, &packet.chan_on_a, &denom, true) {
        // sender was source chain, unescrow tokens back to sender
        let value_balance: Amount = state
            .get(&state_key::ics20_value_balance::by_asset_id(
                &packet.chan_on_a,
                &denom.id(),
            ))
            .await?
            .unwrap_or_else(Amount::zero);

        if value_balance < amount {
            anyhow::bail!("couldn't return coins in timeout: not enough value balance");
        }

        state
            .mint_note(
                value,
                &receiver,
                CommitmentSource::Ics20Transfer {
                    packet_seq: packet.sequence.0,
                    channel_id: packet.chan_on_a.0.clone(),
                    sender: packet_data.sender.clone(),
                },
            )
            .await
            .context("couldn't mint note in timeout_packet_inner")?;

        // update the value balance
        // note: this arithmetic was checked above, but we do it again anyway.
        let new_value_balance = value_balance
            .checked_sub(&amount)
            .context("underflow in ics20 timeout packet value balance subtraction")?;
        state.put(
            state_key::ics20_value_balance::by_asset_id(&packet.chan_on_a, &denom.id()),
            new_value_balance,
        );
        state.record_proto(
            event::EventOutboundFungibleTokenRefund {
                value,
                sender: receiver, // note, this comes from packet_data.sender
                receiver: packet_data.receiver.clone(),
                reason,
                // Use the destination channel, i.e. our name for it, to be consistent across events.
                meta: FungibleTokenTransferPacketMetadata {
                    channel: packet.chan_on_b.0.clone(),
                    sequence: packet.sequence.0,
                },
            }
            .to_proto(),
        );
    } else {
        let value_balance: Amount = state
            .get(&state_key::ics20_value_balance::by_asset_id(
                &packet.chan_on_a,
                &denom.id(),
            ))
            .await?
            .unwrap_or_else(Amount::zero);

        state
            .mint_note(
                value,
                &receiver,
                // NOTE: should this be Ics20TransferTimeout?
                CommitmentSource::Ics20Transfer {
                    packet_seq: packet.sequence.0,
                    channel_id: packet.chan_on_a.0.clone(),
                    sender: packet_data.sender.clone(),
                },
            )
            .await
            .context("failed to mint return voucher in ics20 transfer timeout")?;

        let new_value_balance = value_balance.saturating_add(&value.amount);
        state.put(
            state_key::ics20_value_balance::by_asset_id(&packet.chan_on_a, &denom.id()),
            new_value_balance,
        );
        state.record_proto(
            event::EventOutboundFungibleTokenRefund {
                value,
                sender: receiver, // note, this comes from packet_data.sender
                receiver: packet_data.receiver.clone(),
                reason,
                // Use the destination channel, i.e. our name for it, to be consistent across events.
                meta: FungibleTokenTransferPacketMetadata {
                    channel: packet.chan_on_b.0.clone(),
                    sequence: packet.sequence.0,
                },
            }
            .to_proto(),
        );
    }

    Ok(())
}

// NOTE: should these be fallible, now that our enclosing state machine is fallible in execution?
#[async_trait]
impl AppHandlerExecute for Ics20Transfer {
    async fn chan_open_init_execute<S: StateWrite>(_state: S, _msg: &MsgChannelOpenInit) {}
    async fn chan_open_try_execute<S: StateWrite>(_state: S, _msg: &MsgChannelOpenTry) {}
    async fn chan_open_ack_execute<S: StateWrite>(_state: S, _msg: &MsgChannelOpenAck) {}
    async fn chan_open_confirm_execute<S: StateWrite>(_state: S, _msg: &MsgChannelOpenConfirm) {}
    async fn chan_close_confirm_execute<S: StateWrite>(_state: S, _msg: &MsgChannelCloseConfirm) {}
    async fn chan_close_init_execute<S: StateWrite>(_state: S, _msg: &MsgChannelCloseInit) {}
    async fn recv_packet_execute<S: StateWrite>(mut state: S, msg: &MsgRecvPacket) -> Result<()> {
        // recv packet should never fail a transaction, but it should record a failure acknowledgement.
        let app_execute_start = Instant::now();
        let ack: Vec<u8> = match recv_transfer_packet_inner(&mut state, msg).await {
            Ok(_) => {
                // record packet acknowledgement without error
                TokenTransferAcknowledgement::success().into()
            }
            Err(e) => {
                tracing::debug!("couldnt execute transfer: {:#}", e);
                // record packet acknowledgement with error
                TokenTransferAcknowledgement::Error(e.to_string()).into()
            }
        };
        let app_execute_elapsed = app_execute_start.elapsed();
        #[cfg(feature = "benchmark-helpers")]
        record_inbound_stage(InboundStage::AppExecuteInner, app_execute_elapsed);
        tracing::debug!(
            elapsed_us = app_execute_elapsed.as_micros(),
            sequence = %msg.packet.sequence,
            "ibc_recv_app_execute"
        );

        let ack_start = Instant::now();
        state
            .write_acknowledgement(&msg.packet, &ack)
            .await
            .context("able to write acknowledgement")?;
        let ack_elapsed = ack_start.elapsed();
        #[cfg(feature = "benchmark-helpers")]
        record_inbound_stage(InboundStage::AcknowledgementTotal, ack_elapsed);
        tracing::debug!(
            elapsed_us = ack_elapsed.as_micros(),
            sequence = %msg.packet.sequence,
            "ibc_recv_acknowledgement_write"
        );

        Ok(())
    }

    async fn timeout_packet_execute<S: StateWrite>(mut state: S, msg: &MsgTimeout) -> Result<()> {
        // timeouts may fail due to counterparty chains sending transfers of u128-1
        refund_tokens(
            &mut state,
            &msg.packet,
            event::FungibleTokenRefundReason::Timeout,
        )
        .await
        .context("able to timeout packet")?;

        Ok(())
    }

    async fn acknowledge_packet_execute<S: StateWrite>(
        mut state: S,
        msg: &MsgAcknowledgement,
    ) -> Result<()> {
        let ack: TokenTransferAcknowledgement =
            serde_json::from_slice(msg.acknowledgement.as_slice())?;
        if !ack.is_successful() {
            // in the case where a counterparty chain acknowledges a packet with an error,
            // for example due to a middleware processing issue or other behavior,
            // the funds should be unescrowed back to the packet sender.
            refund_tokens(
                &mut state,
                &msg.packet,
                event::FungibleTokenRefundReason::Error,
            )
            .await
            .context("unable to refund packet acknowledgement")?;
        }

        Ok(())
    }
}

impl AppHandler for Ics20Transfer {}

#[cfg(test)]
mod tests {
    use super::*;
    use ibc_types::{
        core::{
            channel::{packet::Sequence, TimeoutHeight},
            client::Height,
            commitment::MerkleProof,
        },
        timestamp::Timestamp,
    };

    fn test_recv_packet(denom: &str) -> MsgRecvPacket {
        let mut rng = rand::thread_rng();
        let receiver = Address::dummy(&mut rng);
        let packet_data = FungibleTokenPacketData {
            denom: denom.to_string(),
            amount: "123".to_string(),
            sender: "bankd1sender".to_string(),
            receiver: receiver.to_string(),
            memo: String::new(),
        };

        MsgRecvPacket {
            packet: Packet {
                sequence: Sequence::from(1),
                port_on_a: PortId::transfer(),
                chan_on_a: ChannelId::from_str("channel-0").expect("valid channel"),
                port_on_b: PortId::transfer(),
                chan_on_b: ChannelId::from_str("channel-1").expect("valid channel"),
                data: serde_json::to_vec(&packet_data).expect("encode packet data"),
                timeout_height_on_b: TimeoutHeight::At(Height::new(0, 100).expect("valid height")),
                timeout_timestamp_on_b: Timestamp::from_nanoseconds(1_000_000_000)
                    .expect("valid timestamp"),
            },
            proof_commitment_on_a: MerkleProof { proofs: vec![] },
            proof_height_on_a: Height::new(0, 99).expect("valid proof height"),
            signer: receiver.to_string(),
        }
    }

    #[test]
    fn receive_context_derives_sink_zone_voucher_denom() {
        let msg = test_recv_packet("upenumbra");
        let context = Ics20ReceiveContext::parse(&msg).expect("parse receive context");

        assert!(!context.returned_to_source);
        assert_eq!(
            context.received_denom.to_string(),
            "transfer/channel-1/upenumbra"
        );
    }

    #[test]
    fn receive_context_derives_return_source_base_denom() {
        let msg = test_recv_packet("transfer/channel-0/upenumbra");
        let context = Ics20ReceiveContext::parse(&msg).expect("parse receive context");

        assert!(context.returned_to_source);
        assert_eq!(context.received_denom.to_string(), "upenumbra");
    }
}
