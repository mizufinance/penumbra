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
        ChannelEnd, ChannelId, PortId, Version,
    },
    core::connection::ConnectionEnd,
    transfer::acknowledgement::TokenTransferAcknowledgement,
};
use shieldd_sdk_asset::{asset, asset::Metadata, Value};
use shieldd_sdk_compliance::{
    AssetPolicy, ComplianceRegistryRead as _, IbcComplianceMetadata, IbcRoute,
};
use shieldd_sdk_ibc::component::{ChannelStateReadExt, ConnectionStateReadExt};
use shieldd_sdk_keys::Address;
use shieldd_sdk_num::Amount;
use shieldd_sdk_proto::{
    shieldd::core::component::ibc::v1::FungibleTokenPacketData, DomainType as _, StateReadProto,
    StateWriteProto,
};
use shieldd_sdk_sct::CommitmentSource;

#[cfg(feature = "benchmark-helpers")]
use shieldd_sdk_ibc::benchmarking::{record_inbound_stage, InboundStage};
use shieldd_sdk_ibc::component::{
    app_handler::{AppHandler, AppHandlerCheck, AppHandlerExecute},
    packet::{
        Checked, IBCPacket, SendPacketRead as _, SendPacketWrite as _, Unchecked,
        WriteAcknowledgement as _,
    },
    state_key,
};
use tendermint::Time;

// returns a bool indicating if the provided denom was issued locally or if it was bridged in.
// this logic is a bit tricky, and adapted from https://github.com/cosmos/ibc/tree/main/spec/app/ics-020-fungible-token-transfer (sendFungibleTokens).
//
// what we want to do is to determine if the denom being withdrawn is a native token (one
// that originates from Shieldd) or a bridged token (one that was sent into shieldd from
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

struct ResolvedIbcRoute {
    route: IbcRoute,
    channel: ChannelEnd,
    connection: ConnectionEnd,
}

async fn resolve_ibc_route<S: StateRead + ?Sized>(
    state: &S,
    local_port: &PortId,
    local_channel: &ChannelId,
) -> Result<ResolvedIbcRoute> {
    let channel = state
        .get_channel(local_channel, local_port)
        .await?
        .ok_or_else(|| anyhow::anyhow!("IBC route channel not found"))?;
    anyhow::ensure!(
        channel.ordering == ChannelOrder::Unordered,
        "IBC transfer route channel must be unordered"
    );
    anyhow::ensure!(
        channel.version == Version::new("ics20-1".to_string()),
        "IBC transfer route channel must use version ics20-1, found {}",
        channel.version
    );
    anyhow::ensure!(
        channel.connection_hops.len() == 1,
        "IBC route channel must have exactly one connection hop, found {}",
        channel.connection_hops.len()
    );
    let connection_id = channel
        .connection_hops
        .first()
        .ok_or_else(|| anyhow::anyhow!("IBC route channel has no connection hop"))?;
    let connection = state
        .get_connection(connection_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("IBC route connection not found"))?;
    let counterparty_channel = channel
        .counterparty()
        .channel_id()
        .ok_or_else(|| anyhow::anyhow!("IBC route missing counterparty channel"))?;
    Ok(ResolvedIbcRoute {
        route: IbcRoute {
            local_port: local_port.to_string(),
            local_channel: local_channel.to_string(),
            connection_id: connection_id.to_string(),
            counterparty_port: channel.counterparty().port_id.to_string(),
            counterparty_channel: counterparty_channel.to_string(),
        },
        channel,
        connection,
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

/// Non-forgeable evidence that the exact withdrawal packet, IBC state, and
/// compliance route policy were checked together.
pub(super) struct CheckedWithdrawal {
    withdrawal: Ics20Withdrawal,
    packet: IBCPacket<Checked>,
    route: IbcRoute,
    channel: ChannelEnd,
    connection: ConnectionEnd,
    send_sequence: u64,
    policy: Option<AssetPolicy>,
    current_block_time: Time,
}

async fn validated_withdrawal_route<S: StateRead + ?Sized>(
    state: &S,
    withdrawal: &Ics20Withdrawal,
) -> Result<(IbcRoute, ChannelEnd, ConnectionEnd, Option<AssetPolicy>)> {
    let policy = state.get_asset_policy(withdrawal.denom.id()).await?;
    if policy.is_some() {
        IbcComplianceMetadata::validate_regulated_memo(&withdrawal.ics20_memo)?;
        if let Some(metadata) = IbcComplianceMetadata::from_memo(&withdrawal.ics20_memo)? {
            anyhow::ensure!(
                metadata.asset_id == withdrawal.denom.id(),
                "outbound IBC compliance metadata asset_id does not match withdrawal asset"
            );
        }
    }
    let resolved =
        resolve_ibc_route(state, &PortId::transfer(), &withdrawal.source_channel).await?;
    let ResolvedIbcRoute {
        route,
        channel,
        connection,
    } = resolved;
    if let Some(policy) = &policy {
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
    Ok((route, channel, connection, policy))
}

pub(super) async fn withdrawal_check<S: StateWrite + ?Sized>(
    state: &mut S,
    withdrawal: &Ics20Withdrawal,
    current_block_time: Time,
) -> Result<CheckedWithdrawal> {
    let packet: IBCPacket<Unchecked> = withdrawal.clone().try_into()?;
    let send_check_start = Instant::now();
    let packet = state.send_packet_check(packet, current_block_time).await?;
    tracing::debug!(
        elapsed_us = send_check_start.elapsed().as_micros(),
        channel = %withdrawal.source_channel,
        "ibc_outbound_send_packet_check"
    );
    let send_sequence = state
        .get_send_sequence(&withdrawal.source_channel, packet.source_port())
        .await?;

    let route_policy_start = Instant::now();
    let (route, channel, connection, policy) =
        validated_withdrawal_route(state, withdrawal).await?;
    tracing::debug!(
        elapsed_us = route_policy_start.elapsed().as_micros(),
        asset_id = %withdrawal.denom.id(),
        channel = %withdrawal.source_channel,
        "ibc_outbound_route_policy_check"
    );

    Ok(CheckedWithdrawal {
        withdrawal: withdrawal.clone(),
        packet,
        route,
        channel,
        connection,
        send_sequence,
        policy,
        current_block_time,
    })
}

fn same_packet(left: &IBCPacket<Checked>, right: &IBCPacket<Checked>) -> bool {
    left.source_port() == right.source_port()
        && left.source_channel() == right.source_channel()
        && left.timeout_height() == right.timeout_height()
        && left.timeout_timestamp() == right.timeout_timestamp()
        && left.data() == right.data()
}

pub(super) async fn withdrawal_execute<S: StateWrite + ?Sized>(
    state: &mut S,
    checked: CheckedWithdrawal,
) -> Result<()> {
    let CheckedWithdrawal {
        withdrawal,
        packet,
        route,
        channel,
        connection,
        send_sequence,
        policy,
        current_block_time,
    } = checked;

    // Re-read every state-dependent fact immediately before mutation. This
    // rejects a token if its channel, client, route, or policy snapshot became
    // stale between action-local proof effects and packet execution.
    let (current_route, current_channel, current_connection, current_policy) =
        validated_withdrawal_route(state, &withdrawal).await?;
    anyhow::ensure!(
        current_route == route
            && current_channel == channel
            && current_connection == connection
            && current_policy == policy,
        "withdrawal route, channel, connection, client, or compliance policy changed after validation"
    );
    let refreshed_packet = state
        .send_packet_check(
            IBCPacket::<Unchecked>::try_from(withdrawal.clone())?,
            current_block_time,
        )
        .await?;
    let current_send_sequence = state
        .get_send_sequence(&withdrawal.source_channel, refreshed_packet.source_port())
        .await?;
    anyhow::ensure!(
        current_send_sequence == send_sequence,
        "withdrawal send sequence changed after validation: expected {}, found {}",
        send_sequence,
        current_send_sequence
    );
    anyhow::ensure!(
        same_packet(&packet, &refreshed_packet),
        "withdrawal packet changed after validation"
    );

    // Resolve every fallible accounting read and arithmetic check before the
    // first write. Once packet execution succeeds, the remaining balance and
    // event writes are infallible StateWrite operations.
    let accounting_start = Instant::now();
    let balance_key = state_key::ics20_value_balance::by_asset_id(
        &withdrawal.source_channel,
        &withdrawal.denom.id(),
    );
    let existing_value_balance: Amount =
        state.get(&balance_key).await?.unwrap_or_else(Amount::zero);
    let prefix = format!("transfer/{}/", &withdrawal.source_channel);
    let new_value_balance = if !withdrawal.denom.starts_with(&prefix) {
        existing_value_balance
            .checked_add(&withdrawal.amount)
            .ok_or_else(|| anyhow::anyhow!("overflow adding value balance in ics20 withdrawal"))?
    } else {
        anyhow::ensure!(
            existing_value_balance >= withdrawal.amount,
            "insufficient balance to withdraw tokens"
        );
        existing_value_balance
            .checked_sub(&withdrawal.amount)
            .ok_or_else(|| {
                anyhow::anyhow!("underflow subtracting value balance in ics20 withdrawal")
            })?
    };
    let outbound_event = event::EventOutboundFungibleTokenTransfer {
        value: Value {
            amount: withdrawal.amount,
            asset_id: withdrawal.denom.id(),
        },
        sender: withdrawal.return_address.clone(),
        receiver: withdrawal.destination_chain_address.clone(),
        meta: FungibleTokenTransferPacketMetadata {
            channel: withdrawal.source_channel.0.clone(),
            sequence: send_sequence,
        },
    }
    .to_proto();

    let send_execute_start = Instant::now();
    state.send_packet_execute(refreshed_packet).await?;
    tracing::debug!(
        elapsed_us = send_execute_start.elapsed().as_micros(),
        channel = %withdrawal.source_channel,
        "ibc_outbound_send_packet_execute"
    );

    state.put(balance_key, new_value_balance);
    state.record_proto(outbound_event);
    tracing::debug!(
        elapsed_us = accounting_start.elapsed().as_micros(),
        asset_id = %withdrawal.denom.id(),
        channel = %withdrawal.source_channel,
        "ibc_outbound_nullifier_note_accounting"
    );

    Ok(())
}

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

            let amount_shieldd: Amount = packet_data.amount.try_into()?;
            if value_balance < amount_shieldd {
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
    let route = resolve_ibc_route(&state, &msg.packet.port_on_b, &msg.packet.chan_on_b)
        .await?
        .route;
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
        use shieldd_sdk_compliance::ComplianceRegistryWrite as _;
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
    // other chain but was sent from shieldd. so, the shieldd refund receiver address is the
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
    use std::ops::Deref;

    use super::*;
    use cnidarium::{StateDelta, TempStorage};
    use ibc_types::{
        core::{
            channel::{
                channel::{Order, State as ChannelState},
                packet::Sequence,
                ChannelEnd, Counterparty, TimeoutHeight,
            },
            client::{ClientId, Height},
            commitment::{MerkleProof, MerkleRoot},
            connection::{
                ChainId, ConnectionEnd, ConnectionId, Counterparty as ConnectionCounterparty,
                State as ConnectionState,
            },
        },
        lightclients::tendermint::{
            client_state::{AllowUpdate, ClientState},
            consensus_state::ConsensusState,
            TrustThreshold,
        },
        path::ClientConsensusStatePath,
        timestamp::Timestamp,
    };
    use shieldd_sdk_asset::BASE_ASSET_DENOM;
    use shieldd_sdk_compliance::ComplianceRegistryWrite as _;
    use shieldd_sdk_ibc::component::{
        commit_packet, ChannelStateWriteExt as _, ClientStateReadExt as _,
        ClientStateWriteExt as _, ConnectionStateWriteExt as _, ConsensusStateWriteExt,
        HostInterface,
    };
    use shieldd_sdk_ibc::{MerklePrefixExt as _, IBC_COMMITMENT_PREFIX, IBC_PROOF_SPECS};
    use shieldd_sdk_keys::test_keys;
    use shieldd_sdk_proto::event::EventDomainType as _;

    struct TestHost;

    #[async_trait]
    impl HostInterface for TestHost {
        async fn get_chain_id<S: StateRead>(_state: S) -> Result<String> {
            Ok("shieldd-test-1".to_string())
        }

        async fn get_revision_number<S: StateRead>(_state: S) -> Result<u64> {
            Ok(1)
        }

        async fn get_block_height<S: StateRead>(_state: S) -> Result<u64> {
            Ok(10)
        }

        async fn get_block_timestamp<S: StateRead>(_state: S) -> Result<Time> {
            Time::from_unix_timestamp(2, 0).context("valid test block time")
        }
    }

    fn test_withdrawal() -> Ics20Withdrawal {
        Ics20Withdrawal {
            amount: Amount::from(123u64),
            denom: BASE_ASSET_DENOM.clone(),
            destination_chain_address: "cosmos1destination".to_string(),
            return_address: test_keys::ADDRESS_0.deref().clone(),
            timeout_height: Height::new(1, 100).expect("valid timeout height"),
            timeout_time: 60_000_000_000,
            source_channel: ChannelId::from_str("channel-0").expect("valid channel"),
            ics20_memo: String::new(),
            use_transparent_address: false,
        }
    }

    fn regulated_test_withdrawal() -> Ics20Withdrawal {
        Ics20Withdrawal {
            denom: "regulated_test_asset"
                .try_into()
                .expect("custom test denom should parse as a base denom"),
            ..test_withdrawal()
        }
    }

    fn policy_authorizing(routes: Vec<IbcRoute>) -> AssetPolicy {
        let mut policy = AssetPolicy::simple(
            decaf377::Element::GENERATOR,
            u128::MAX,
            decaf377::Element::GENERATOR,
        );
        policy.replace_allowed_ibc_routes(routes);
        policy
    }

    fn event_attribute<'a>(event: &'a tendermint::abci::Event, key: &str) -> &'a str {
        event
            .attributes
            .iter()
            .find_map(|attribute| {
                if attribute.key_str().ok()? == key {
                    attribute.value_str().ok()
                } else {
                    None
                }
            })
            .unwrap_or_else(|| panic!("event {} missing attribute {key}", event.kind))
    }

    fn consensus_state_key(client_id: &ClientId, height: &Height) -> String {
        IBC_COMMITMENT_PREFIX
            .apply_string(ClientConsensusStatePath::new(client_id, height).to_string())
    }

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

    async fn put_open_outbound_route<S: StateWrite>(
        state: &mut S,
        withdrawal: &Ics20Withdrawal,
        counterparty_channel: ChannelId,
    ) -> Result<()> {
        let client_id = ClientId::from_str("07-tendermint-0")?;
        let latest_height = Height::new(1, 10)?;
        let client_state = ClientState::new(
            ChainId::new("counterparty".to_string(), 1),
            TrustThreshold::ONE_THIRD,
            std::time::Duration::from_secs(86_400),
            std::time::Duration::from_secs(172_800),
            std::time::Duration::from_secs(5),
            latest_height,
            IBC_PROOF_SPECS.to_vec(),
            vec![],
            AllowUpdate {
                after_expiry: false,
                after_misbehaviour: false,
            },
            None,
        )?;
        state.put_client(&client_id, client_state);
        state
            .put_verified_consensus_state::<TestHost>(
                latest_height,
                client_id.clone(),
                ConsensusState::new(
                    MerkleRoot {
                        hash: vec![1u8; 32],
                    },
                    Time::from_unix_timestamp(1, 0)?,
                    tendermint::Hash::Sha256([2u8; 32]),
                ),
            )
            .await?;

        let connection_id = ConnectionId::new(0);
        state
            .put_new_connection(
                &connection_id,
                ConnectionEnd {
                    state: ConnectionState::Open,
                    client_id,
                    counterparty: ConnectionCounterparty::default(),
                    versions: vec![],
                    delay_period: std::time::Duration::ZERO,
                },
            )
            .await?;

        let port = PortId::transfer();
        state.put_channel(
            &withdrawal.source_channel,
            &port,
            ChannelEnd {
                state: ChannelState::Open,
                ordering: Order::Unordered,
                remote: Counterparty::new(port.clone(), Some(counterparty_channel)),
                connection_hops: vec![connection_id],
                version: Version::new("ics20-1".to_string()),
                ..ChannelEnd::default()
            },
        );
        state.put_send_sequence(&withdrawal.source_channel, &port, 1);
        Ok(())
    }

    #[test]
    fn receive_context_derives_sink_zone_voucher_denom() {
        let msg = test_recv_packet("ushieldd");
        let context = Ics20ReceiveContext::parse(&msg).expect("parse receive context");

        assert!(!context.returned_to_source);
        assert_eq!(
            context.received_denom.to_string(),
            "transfer/channel-1/ushieldd"
        );
    }

    #[test]
    fn receive_context_derives_return_source_base_denom() {
        let msg = test_recv_packet("transfer/channel-0/ushieldd");
        let context = Ics20ReceiveContext::parse(&msg).expect("parse receive context");

        assert!(context.returned_to_source);
        assert_eq!(context.received_denom.to_string(), "ushieldd");
    }

    #[tokio::test]
    async fn withdrawal_check_rejects_missing_route_state() {
        let storage = TempStorage::new()
            .await
            .expect("temporary state must initialize");
        let mut state = StateDelta::new(storage.latest_snapshot());
        let current_time = Time::from_unix_timestamp(1, 0).expect("valid current block time");

        let error = match withdrawal_check(&mut state, &test_withdrawal(), current_time).await {
            Ok(_) => panic!("a withdrawal must not use an absent channel"),
            Err(error) => error,
        };

        assert!(
            error.to_string().contains("channel channel-0")
                && error.to_string().contains("does not exist"),
            "unexpected missing-route error: {error:#}"
        );
    }

    #[tokio::test]
    async fn withdrawal_check_rejects_missing_counterparty_channel() {
        let storage = TempStorage::new()
            .await
            .expect("temporary state must initialize");
        let mut state = StateDelta::new(storage.latest_snapshot());
        let withdrawal = test_withdrawal();
        let port = PortId::transfer();
        put_open_outbound_route(
            &mut state,
            &withdrawal,
            ChannelId::from_str("channel-7").expect("valid counterparty channel"),
        )
        .await
        .expect("valid outbound route");
        let mut channel = state
            .get_channel(&withdrawal.source_channel, &port)
            .await
            .expect("channel read must succeed")
            .expect("test channel must exist");
        channel.remote = Counterparty::new(port, None);
        state.put_channel(&withdrawal.source_channel, &PortId::transfer(), channel);

        let error = match withdrawal_check(
            &mut state,
            &withdrawal,
            Time::from_unix_timestamp(2, 0).expect("valid current block time"),
        )
        .await
        {
            Ok(_) => panic!("a channel without a counterparty channel must reject withdrawal"),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("has no counterparty channel"),
            "unexpected missing-counterparty error: {error:#}"
        );
    }

    #[tokio::test]
    async fn withdrawal_check_rejects_zero_connection_hops() {
        let storage = TempStorage::new()
            .await
            .expect("temporary state must initialize");
        let mut state = StateDelta::new(storage.latest_snapshot());
        let withdrawal = test_withdrawal();
        let port = PortId::transfer();
        put_open_outbound_route(
            &mut state,
            &withdrawal,
            ChannelId::from_str("channel-7").expect("valid counterparty channel"),
        )
        .await
        .expect("valid outbound route");
        let mut channel = state
            .get_channel(&withdrawal.source_channel, &port)
            .await
            .expect("channel read must succeed")
            .expect("test channel must exist");
        channel.connection_hops.clear();
        state.put_channel(&withdrawal.source_channel, &port, channel);

        let error = match withdrawal_check(
            &mut state,
            &withdrawal,
            Time::from_unix_timestamp(2, 0).expect("valid current block time"),
        )
        .await
        {
            Ok(_) => panic!("a channel without a connection hop must reject withdrawal"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("must have exactly one connection hop"),
            "unexpected zero-hop error: {error:#}"
        );
    }

    #[tokio::test]
    async fn withdrawal_check_rejects_missing_connection() {
        let storage = TempStorage::new()
            .await
            .expect("temporary state must initialize");
        let mut state = StateDelta::new(storage.latest_snapshot());
        let withdrawal = test_withdrawal();
        let port = PortId::transfer();
        put_open_outbound_route(
            &mut state,
            &withdrawal,
            ChannelId::from_str("channel-7").expect("valid counterparty channel"),
        )
        .await
        .expect("valid outbound route");
        let mut channel = state
            .get_channel(&withdrawal.source_channel, &port)
            .await
            .expect("channel read must succeed")
            .expect("test channel must exist");
        channel.connection_hops = vec![ConnectionId::new(9)];
        state.put_channel(&withdrawal.source_channel, &port, channel);

        let error = match withdrawal_check(
            &mut state,
            &withdrawal,
            Time::from_unix_timestamp(2, 0).expect("valid current block time"),
        )
        .await
        {
            Ok(_) => panic!("a missing referenced connection must reject withdrawal"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("connection connection-9 does not exist"),
            "unexpected missing-connection error: {error:#}"
        );
    }

    #[tokio::test]
    async fn withdrawal_check_rejects_ordered_channel() {
        let storage = TempStorage::new()
            .await
            .expect("temporary state must initialize");
        let mut state = StateDelta::new(storage.latest_snapshot());
        let withdrawal = test_withdrawal();
        let port = PortId::transfer();
        put_open_outbound_route(
            &mut state,
            &withdrawal,
            ChannelId::from_str("channel-7").expect("valid counterparty channel"),
        )
        .await
        .expect("valid outbound route");
        let mut channel = state
            .get_channel(&withdrawal.source_channel, &port)
            .await
            .expect("channel read must succeed")
            .expect("test channel must exist");
        channel.ordering = Order::Ordered;
        state.put_channel(&withdrawal.source_channel, &port, channel);

        let error = match withdrawal_check(
            &mut state,
            &withdrawal,
            Time::from_unix_timestamp(2, 0).expect("valid current block time"),
        )
        .await
        {
            Ok(_) => panic!("an ordered channel must reject ICS-20 withdrawal"),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("must be unordered"),
            "unexpected ordered-channel error: {error:#}"
        );
    }

    #[tokio::test]
    async fn withdrawal_check_rejects_non_ics20_channel_version() {
        let storage = TempStorage::new()
            .await
            .expect("temporary state must initialize");
        let mut state = StateDelta::new(storage.latest_snapshot());
        let withdrawal = test_withdrawal();
        let port = PortId::transfer();
        put_open_outbound_route(
            &mut state,
            &withdrawal,
            ChannelId::from_str("channel-7").expect("valid counterparty channel"),
        )
        .await
        .expect("valid outbound route");
        let mut channel = state
            .get_channel(&withdrawal.source_channel, &port)
            .await
            .expect("channel read must succeed")
            .expect("test channel must exist");
        channel.version = Version::new("ics20-2".to_string());
        state.put_channel(&withdrawal.source_channel, &port, channel);

        let error = match withdrawal_check(
            &mut state,
            &withdrawal,
            Time::from_unix_timestamp(2, 0).expect("valid current block time"),
        )
        .await
        {
            Ok(_) => panic!("a non-ics20-1 channel must reject withdrawal"),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("must use version ics20-1"),
            "unexpected channel-version error: {error:#}"
        );
    }

    #[tokio::test]
    async fn withdrawal_check_rejects_unauthorized_regulated_route() {
        let storage = TempStorage::new()
            .await
            .expect("temporary state must initialize");
        let mut state = StateDelta::new(storage.latest_snapshot());
        let withdrawal = regulated_test_withdrawal();
        put_open_outbound_route(
            &mut state,
            &withdrawal,
            ChannelId::from_str("channel-7").expect("valid counterparty channel"),
        )
        .await
        .expect("valid outbound route");
        state
            .test_only_register_asset(
                withdrawal.denom.id(),
                AssetPolicy::simple(
                    decaf377::Element::GENERATOR,
                    u128::MAX,
                    decaf377::Element::GENERATOR,
                ),
                true,
            )
            .await
            .expect("regulated asset policy must initialize");

        let error = match withdrawal_check(
            &mut state,
            &withdrawal,
            Time::from_unix_timestamp(2, 0).expect("valid current block time"),
        )
        .await
        {
            Ok(_) => panic!("a regulated withdrawal must use an authorized exact route"),
            Err(error) => error,
        };

        assert!(
            error
                .to_string()
                .contains("regulated asset is not allowed on IBC route"),
            "unexpected route-policy error: {error:#}"
        );
    }

    #[tokio::test]
    async fn withdrawal_check_enforces_regulated_memo_shape_and_asset_binding() {
        let storage = TempStorage::new()
            .await
            .expect("temporary state must initialize");
        let mut state = StateDelta::new(storage.latest_snapshot());
        let mut withdrawal = regulated_test_withdrawal();
        put_open_outbound_route(
            &mut state,
            &withdrawal,
            ChannelId::from_str("channel-7").expect("valid counterparty channel"),
        )
        .await
        .expect("valid outbound route");
        let route = IbcRoute::transfer("channel-0", "connection-0", "channel-7");
        state
            .test_only_register_asset(withdrawal.denom.id(), policy_authorizing(vec![route]), true)
            .await
            .expect("regulated asset policy must initialize");
        let valid_metadata = IbcComplianceMetadata {
            compliance_ciphertext: vec![
                7u8;
                shieldd_sdk_compliance::structs::TRANSFER_INPUT_WIRE_BYTES
            ],
            asset_id: withdrawal.denom.id(),
        };
        let wrong_asset_metadata = IbcComplianceMetadata {
            compliance_ciphertext: valid_metadata.compliance_ciphertext.clone(),
            asset_id: asset::Id(decaf377::Fq::from(999u64)),
        };
        let malformed_memo = "not-json".to_string();
        let forwarding_memo = valid_metadata
            .encode_to_memo("forward-to-next-chain")
            .expect("memo encoding must succeed");
        let wrong_asset_memo = wrong_asset_metadata
            .encode_to_memo("")
            .expect("memo encoding must succeed");

        for (case, memo, expected_error) in [
            (
                "malformed",
                malformed_memo,
                "regulated IBC memo must be valid JSON",
            ),
            (
                "forwarding",
                forwarding_memo,
                "may only contain Shieldd compliance metadata",
            ),
            (
                "wrong asset",
                wrong_asset_memo,
                "asset_id does not match withdrawal asset",
            ),
        ] {
            withdrawal.ics20_memo = memo;
            let error = match withdrawal_check(
                &mut state,
                &withdrawal,
                Time::from_unix_timestamp(2, 0).expect("valid current block time"),
            )
            .await
            {
                Ok(_) => panic!("{case} regulated memo must be rejected"),
                Err(error) => error,
            };
            assert!(
                error.to_string().contains(expected_error),
                "{case} memo failed for the wrong reason: {error:#}"
            );
        }

        withdrawal.ics20_memo = valid_metadata
            .encode_to_memo("")
            .expect("memo encoding must succeed");
        withdrawal_check(
            &mut state,
            &withdrawal,
            Time::from_unix_timestamp(2, 0).expect("valid current block time"),
        )
        .await
        .expect("an exact compliance wrapper bound to the withdrawal asset must be accepted");
    }

    #[tokio::test]
    async fn withdrawal_execute_updates_supply_and_packet_state() {
        let storage = TempStorage::new()
            .await
            .expect("temporary state must initialize");
        let mut state = StateDelta::new(storage.latest_snapshot());
        let withdrawal = test_withdrawal();
        let port = PortId::transfer();
        let counterparty_channel =
            ChannelId::from_str("channel-7").expect("valid counterparty channel");
        put_open_outbound_route(&mut state, &withdrawal, counterparty_channel.clone())
            .await
            .expect("valid outbound route");
        let expected_packet = Packet {
            sequence: Sequence::from(1),
            port_on_a: port.clone(),
            chan_on_a: withdrawal.source_channel.clone(),
            port_on_b: port.clone(),
            chan_on_b: counterparty_channel,
            data: withdrawal
                .packet_data()
                .expect("valid withdrawal packet data"),
            timeout_height_on_b: withdrawal.timeout_height.clone().into(),
            timeout_timestamp_on_b: Timestamp::from_nanoseconds(withdrawal.timeout_time)
                .expect("valid withdrawal timeout timestamp"),
        };
        let checked = withdrawal_check(
            &mut state,
            &withdrawal,
            Time::from_unix_timestamp(2, 0).expect("valid current block time"),
        )
        .await
        .expect("valid withdrawal must check");
        withdrawal_execute(&mut state, checked)
            .await
            .expect("validated withdrawal must execute");

        let escrowed: Amount = state
            .get(&state_key::ics20_value_balance::by_asset_id(
                &withdrawal.source_channel,
                &withdrawal.denom.id(),
            ))
            .await
            .expect("escrow read must succeed")
            .expect("escrow balance must be written");
        assert_eq!(escrowed, withdrawal.amount);
        assert_eq!(
            state
                .get_send_sequence(&withdrawal.source_channel, &port)
                .await
                .expect("send sequence read must succeed"),
            2
        );
        let stored_packet_commitment = state
            .get_packet_commitment_by_id(&withdrawal.source_channel, &port, 1)
            .await
            .expect("packet commitment read must succeed")
            .expect("withdrawal packet commitment must exist");
        assert_eq!(
            stored_packet_commitment,
            commit_packet(&expected_packet),
            "the exact withdrawal packet must be committed at the allocated sequence"
        );

        let (_, mut changes) = state.flatten();
        let events = changes.take_events();
        let outbound_events = events
            .iter()
            .filter_map(|event| {
                event::EventOutboundFungibleTokenTransfer::try_from_event(event).ok()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            outbound_events.len(),
            1,
            "withdrawal execution must emit exactly one outbound transfer event"
        );
        let outbound = &outbound_events[0];
        assert_eq!(outbound.value.amount, withdrawal.amount);
        assert_eq!(outbound.value.asset_id, withdrawal.denom.id());
        assert_eq!(outbound.sender, withdrawal.return_address);
        assert_eq!(outbound.receiver, withdrawal.destination_chain_address);
        assert_eq!(outbound.meta.channel, withdrawal.source_channel.0);
        assert_eq!(outbound.meta.sequence, 1);

        let send_events = events
            .iter()
            .filter(|event| event.kind == "send_packet")
            .collect::<Vec<_>>();
        assert_eq!(
            send_events.len(),
            1,
            "withdrawal execution must emit exactly one IBC send_packet event"
        );
        let send = send_events[0];
        assert_eq!(
            send.attributes.len(),
            11,
            "the send_packet event must contain the complete canonical attribute set"
        );
        assert_eq!(
            event_attribute(send, "packet_data_hex"),
            hex::encode(&expected_packet.data)
        );
        assert_eq!(
            event_attribute(send, "packet_data"),
            std::str::from_utf8(&expected_packet.data).expect("packet JSON must be UTF-8")
        );
        assert_eq!(
            event_attribute(send, "packet_timeout_height"),
            expected_packet.timeout_height_on_b.to_string()
        );
        assert_eq!(
            event_attribute(send, "packet_timeout_timestamp"),
            expected_packet
                .timeout_timestamp_on_b
                .nanoseconds()
                .to_string()
        );
        assert_eq!(
            event_attribute(send, "packet_sequence"),
            expected_packet.sequence.to_string()
        );
        assert_eq!(
            event_attribute(send, "packet_src_port"),
            expected_packet.port_on_a.to_string()
        );
        assert_eq!(
            event_attribute(send, "packet_src_channel"),
            expected_packet.chan_on_a.to_string()
        );
        assert_eq!(
            event_attribute(send, "packet_dst_port"),
            expected_packet.port_on_b.to_string()
        );
        assert_eq!(
            event_attribute(send, "packet_dst_channel"),
            expected_packet.chan_on_b.to_string()
        );
        assert_eq!(
            event_attribute(send, "packet_channel_ordering"),
            Order::Unordered.as_str()
        );
        assert_eq!(
            event_attribute(send, "packet_connection"),
            ConnectionId::new(0).to_string()
        );
    }

    #[tokio::test]
    async fn voucher_withdrawal_subtracts_channel_asset_balance_and_commits_packet() {
        let storage = TempStorage::new()
            .await
            .expect("temporary state must initialize");
        let mut state = StateDelta::new(storage.latest_snapshot());
        let mut withdrawal = test_withdrawal();
        withdrawal.denom = format!("transfer/{}/uatom", withdrawal.source_channel)
            .as_str()
            .try_into()
            .expect("valid voucher denomination");
        let port = PortId::transfer();
        let counterparty_channel =
            ChannelId::from_str("channel-7").expect("valid counterparty channel");
        put_open_outbound_route(&mut state, &withdrawal, counterparty_channel.clone())
            .await
            .expect("valid outbound route");
        let balance_key = state_key::ics20_value_balance::by_asset_id(
            &withdrawal.source_channel,
            &withdrawal.denom.id(),
        );
        state.put(balance_key.clone(), Amount::from(500u64));
        let expected_packet = Packet {
            sequence: Sequence::from(1),
            port_on_a: port.clone(),
            chan_on_a: withdrawal.source_channel.clone(),
            port_on_b: port.clone(),
            chan_on_b: counterparty_channel,
            data: withdrawal
                .packet_data()
                .expect("valid withdrawal packet data"),
            timeout_height_on_b: withdrawal.timeout_height.clone().into(),
            timeout_timestamp_on_b: Timestamp::from_nanoseconds(withdrawal.timeout_time)
                .expect("valid withdrawal timeout timestamp"),
        };

        let checked = withdrawal_check(
            &mut state,
            &withdrawal,
            Time::from_unix_timestamp(2, 0).expect("valid current block time"),
        )
        .await
        .expect("voucher withdrawal must check");
        withdrawal_execute(&mut state, checked)
            .await
            .expect("voucher withdrawal must execute");

        assert_eq!(
            state
                .get::<Amount>(&balance_key)
                .await
                .expect("balance read must succeed"),
            Some(Amount::from(377u64)),
            "returning a voucher to its source must subtract the exact packet amount"
        );
        assert_eq!(
            state
                .get_send_sequence(&withdrawal.source_channel, &port)
                .await
                .expect("send sequence read must succeed"),
            2
        );
        assert_eq!(
            state
                .get_packet_commitment_by_id(&withdrawal.source_channel, &port, 1)
                .await
                .expect("packet commitment read must succeed"),
            Some(commit_packet(&expected_packet))
        );

        let (_, mut changes) = state.flatten();
        let events = changes.take_events();
        let outbound = events
            .iter()
            .filter_map(|event| {
                event::EventOutboundFungibleTokenTransfer::try_from_event(event).ok()
            })
            .collect::<Vec<_>>();
        assert_eq!(outbound.len(), 1);
        assert_eq!(outbound[0].value, withdrawal.value());
        assert_eq!(
            events
                .iter()
                .filter(|event| event.kind == "send_packet")
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn withdrawal_execute_rejects_stale_route_token_before_mutation() {
        let storage = TempStorage::new()
            .await
            .expect("temporary state must initialize");
        let mut state = StateDelta::new(storage.latest_snapshot());
        let withdrawal = test_withdrawal();
        let port = PortId::transfer();
        put_open_outbound_route(
            &mut state,
            &withdrawal,
            ChannelId::from_str("channel-7").expect("valid counterparty channel"),
        )
        .await
        .expect("valid outbound route");
        let checked = withdrawal_check(
            &mut state,
            &withdrawal,
            Time::from_unix_timestamp(2, 0).expect("valid current block time"),
        )
        .await
        .expect("valid withdrawal must check");

        state.put_channel(
            &withdrawal.source_channel,
            &port,
            ChannelEnd {
                state: ChannelState::Open,
                ordering: Order::Unordered,
                remote: Counterparty::new(
                    port.clone(),
                    Some(
                        ChannelId::from_str("channel-8")
                            .expect("valid replacement counterparty channel"),
                    ),
                ),
                connection_hops: vec![ConnectionId::new(0)],
                version: Version::new("ics20-1".to_string()),
                ..ChannelEnd::default()
            },
        );

        let error = withdrawal_execute(&mut state, checked)
            .await
            .expect_err("a checked withdrawal must reject a substituted route");
        assert!(
            error.to_string().contains("changed after validation"),
            "unexpected stale-route error: {error:#}"
        );
        assert_eq!(
            state
                .get_send_sequence(&withdrawal.source_channel, &port)
                .await
                .expect("send sequence read must succeed"),
            1,
            "stale validation must not allocate a packet sequence"
        );
        let escrowed: Option<Amount> = state
            .get(&state_key::ics20_value_balance::by_asset_id(
                &withdrawal.source_channel,
                &withdrawal.denom.id(),
            ))
            .await
            .expect("escrow read must succeed");
        assert!(
            escrowed.is_none(),
            "stale validation must not mutate escrow accounting"
        );
    }

    #[tokio::test]
    async fn withdrawal_execute_rejects_stale_policy_token_before_mutation() {
        let storage = TempStorage::new()
            .await
            .expect("temporary state must initialize");
        let mut state = StateDelta::new(storage.latest_snapshot());
        let withdrawal = regulated_test_withdrawal();
        let port = PortId::transfer();
        put_open_outbound_route(
            &mut state,
            &withdrawal,
            ChannelId::from_str("channel-7").expect("valid counterparty channel"),
        )
        .await
        .expect("valid outbound route");
        let route = IbcRoute::transfer("channel-0", "connection-0", "channel-7");
        let extra_route = IbcRoute::transfer("channel-9", "connection-9", "channel-10");
        let policy = policy_authorizing(vec![route.clone(), extra_route]);
        let expected_route_policy_hash =
            shieldd_sdk_compliance::indexed_tree::route_policy_to_fq(&policy.params).to_bytes();
        state
            .test_only_register_asset(withdrawal.denom.id(), policy, true)
            .await
            .expect("regulated asset policy must initialize");
        let checked = withdrawal_check(
            &mut state,
            &withdrawal,
            Time::from_unix_timestamp(2, 0).expect("valid current block time"),
        )
        .await
        .expect("the original policy must authorize the live route");

        state
            .test_only_replace_asset_ibc_policy(
                withdrawal.denom.id(),
                expected_route_policy_hash,
                vec![route],
            )
            .await
            .expect("replacement policy must remain route-authorizing");

        let error = withdrawal_execute(&mut state, checked)
            .await
            .expect_err("execution must reject a changed policy snapshot");
        assert!(
            error.to_string().contains("changed after validation"),
            "unexpected stale-policy error: {error:#}"
        );
        assert_eq!(
            state
                .get_send_sequence(&withdrawal.source_channel, &port)
                .await
                .expect("send sequence read must succeed"),
            1,
            "stale policy validation must not allocate a packet sequence"
        );
        assert!(
            state
                .get_packet_commitment_by_id(&withdrawal.source_channel, &port, 1)
                .await
                .expect("packet commitment read must succeed")
                .is_none(),
            "stale policy validation must not commit a packet"
        );
        assert!(
            state
                .get::<Amount>(&state_key::ics20_value_balance::by_asset_id(
                    &withdrawal.source_channel,
                    &withdrawal.denom.id(),
                ))
                .await
                .expect("escrow read must succeed")
                .is_none(),
            "stale policy validation must not mutate escrow accounting"
        );
        let (_, mut changes) = state.flatten();
        let events = changes.take_events();
        assert!(
            events.iter().all(|event| event.kind != "send_packet"),
            "stale policy validation must not emit a send_packet event"
        );
        assert!(
            events.iter().all(|event| {
                event::EventOutboundFungibleTokenTransfer::try_from_event(event).is_err()
            }),
            "stale policy validation must not emit an outbound transfer event"
        );
    }

    #[tokio::test]
    async fn withdrawal_execute_rejects_stale_sequence_token_before_mutation() {
        let storage = TempStorage::new()
            .await
            .expect("temporary state must initialize");
        let mut state = StateDelta::new(storage.latest_snapshot());
        let withdrawal = test_withdrawal();
        let port = PortId::transfer();
        put_open_outbound_route(
            &mut state,
            &withdrawal,
            ChannelId::from_str("channel-7").expect("valid counterparty channel"),
        )
        .await
        .expect("valid outbound route");
        let checked = withdrawal_check(
            &mut state,
            &withdrawal,
            Time::from_unix_timestamp(2, 0).expect("valid current block time"),
        )
        .await
        .expect("valid withdrawal must check");
        state.put_send_sequence(&withdrawal.source_channel, &port, 2);

        let error = withdrawal_execute(&mut state, checked)
            .await
            .expect_err("a checked withdrawal must reject a substituted sequence");
        assert!(
            error.to_string().contains("send sequence changed"),
            "unexpected stale-sequence error: {error:#}"
        );
        assert_eq!(
            state
                .get_send_sequence(&withdrawal.source_channel, &port)
                .await
                .expect("send sequence read must succeed"),
            2,
            "stale validation must not allocate another packet sequence"
        );
        let escrowed: Option<Amount> = state
            .get(&state_key::ics20_value_balance::by_asset_id(
                &withdrawal.source_channel,
                &withdrawal.denom.id(),
            ))
            .await
            .expect("escrow read must succeed");
        assert!(
            escrowed.is_none(),
            "stale validation must not mutate escrow accounting"
        );
        assert!(
            state
                .get_packet_commitment_by_id(&withdrawal.source_channel, &port, 2)
                .await
                .expect("packet commitment read must succeed")
                .is_none(),
            "stale validation must not commit a packet at the substituted sequence"
        );
    }

    #[tokio::test]
    async fn withdrawal_execute_rejects_stale_client_token_before_mutation() {
        let storage = TempStorage::new()
            .await
            .expect("temporary state must initialize");
        let mut state = StateDelta::new(storage.latest_snapshot());
        let withdrawal = test_withdrawal();
        let port = PortId::transfer();
        put_open_outbound_route(
            &mut state,
            &withdrawal,
            ChannelId::from_str("channel-7").expect("valid counterparty channel"),
        )
        .await
        .expect("valid outbound route");
        let checked = withdrawal_check(
            &mut state,
            &withdrawal,
            Time::from_unix_timestamp(2, 0).expect("valid current block time"),
        )
        .await
        .expect("valid withdrawal must check");
        state.update_connection(
            &ConnectionId::new(0),
            ConnectionEnd {
                state: ConnectionState::Open,
                client_id: ClientId::from_str("07-tendermint-1").expect("valid replacement client"),
                counterparty: ConnectionCounterparty::default(),
                versions: vec![],
                delay_period: std::time::Duration::ZERO,
            },
        );

        let error = withdrawal_execute(&mut state, checked)
            .await
            .expect_err("a checked withdrawal must reject a substituted client");
        assert!(
            error
                .to_string()
                .contains("route, channel, connection, client, or compliance policy changed"),
            "unexpected stale-client error: {error:#}"
        );
        assert_eq!(
            state
                .get_send_sequence(&withdrawal.source_channel, &port)
                .await
                .expect("send sequence read must succeed"),
            1,
            "stale validation must not allocate a packet sequence"
        );
        let escrowed: Option<Amount> = state
            .get(&state_key::ics20_value_balance::by_asset_id(
                &withdrawal.source_channel,
                &withdrawal.denom.id(),
            ))
            .await
            .expect("escrow read must succeed");
        assert!(
            escrowed.is_none(),
            "stale validation must not mutate escrow accounting"
        );

        // A stable connection/client identifier is not enough: execution must
        // also revalidate the current state of that same client.
        let storage = TempStorage::new()
            .await
            .expect("temporary state must initialize");
        let mut state = StateDelta::new(storage.latest_snapshot());
        let withdrawal = test_withdrawal();
        let port = PortId::transfer();
        put_open_outbound_route(
            &mut state,
            &withdrawal,
            ChannelId::from_str("channel-7").expect("valid counterparty channel"),
        )
        .await
        .expect("valid outbound route");
        let checked = withdrawal_check(
            &mut state,
            &withdrawal,
            Time::from_unix_timestamp(2, 0).expect("valid current block time"),
        )
        .await
        .expect("valid withdrawal must check");
        let client_id = ClientId::from_str("07-tendermint-0").expect("valid client ID");
        let client = state
            .get_client_state(&client_id)
            .await
            .expect("stored client state");
        state.put_client(
            &client_id,
            client.with_frozen_height(Height::new(1, 1).expect("valid nonzero frozen height")),
        );

        let error = withdrawal_execute(&mut state, checked)
            .await
            .expect_err("a checked withdrawal must reject a newly frozen client");
        assert!(
            error.to_string().contains("is frozen"),
            "unexpected frozen-client revalidation error: {error:#}"
        );
        assert_eq!(
            state
                .get_send_sequence(&withdrawal.source_channel, &port)
                .await
                .expect("send sequence read must succeed"),
            1,
            "client revalidation failure must not allocate a packet sequence"
        );
        let escrowed: Option<Amount> = state
            .get(&state_key::ics20_value_balance::by_asset_id(
                &withdrawal.source_channel,
                &withdrawal.denom.id(),
            ))
            .await
            .expect("escrow read must succeed");
        assert!(
            escrowed.is_none(),
            "client revalidation failure must not mutate escrow accounting"
        );
        assert!(
            state
                .get_packet_commitment_by_id(&withdrawal.source_channel, &port, 1)
                .await
                .expect("packet commitment read must succeed")
                .is_none(),
            "client revalidation failure must not commit a packet"
        );
    }

    #[tokio::test]
    async fn withdrawal_check_rejects_closed_channel() {
        let storage = TempStorage::new()
            .await
            .expect("temporary state must initialize");
        let mut state = StateDelta::new(storage.latest_snapshot());
        let withdrawal = test_withdrawal();
        let port = PortId::transfer();
        put_open_outbound_route(
            &mut state,
            &withdrawal,
            ChannelId::from_str("channel-7").expect("valid counterparty channel"),
        )
        .await
        .expect("valid outbound route");
        state.put_channel(
            &withdrawal.source_channel,
            &port,
            ChannelEnd {
                state: ChannelState::Closed,
                ordering: Order::Unordered,
                remote: Counterparty::new(
                    port.clone(),
                    Some(ChannelId::from_str("channel-7").expect("valid counterparty channel")),
                ),
                connection_hops: vec![ConnectionId::new(0)],
                version: Version::new("ics20-1".to_string()),
                ..ChannelEnd::default()
            },
        );

        let error = match withdrawal_check(
            &mut state,
            &withdrawal,
            Time::from_unix_timestamp(2, 0).expect("valid current block time"),
        )
        .await
        {
            Ok(_) => panic!("a closed channel must reject withdrawal"),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("is not open"),
            "unexpected closed-channel error: {error:#}"
        );
    }

    #[tokio::test]
    async fn withdrawal_check_rejects_preopen_channel() {
        let storage = TempStorage::new()
            .await
            .expect("temporary state must initialize");
        let mut state = StateDelta::new(storage.latest_snapshot());
        let withdrawal = test_withdrawal();
        let port = PortId::transfer();
        put_open_outbound_route(
            &mut state,
            &withdrawal,
            ChannelId::from_str("channel-7").expect("valid counterparty channel"),
        )
        .await
        .expect("valid outbound route");
        state.put_channel(
            &withdrawal.source_channel,
            &port,
            ChannelEnd {
                state: ChannelState::Init,
                ordering: Order::Unordered,
                remote: Counterparty::new(
                    port.clone(),
                    Some(ChannelId::from_str("channel-7").expect("valid counterparty channel")),
                ),
                connection_hops: vec![ConnectionId::new(0)],
                version: Version::new("ics20-1".to_string()),
                ..ChannelEnd::default()
            },
        );

        let error = match withdrawal_check(
            &mut state,
            &withdrawal,
            Time::from_unix_timestamp(2, 0).expect("valid current block time"),
        )
        .await
        {
            Ok(_) => panic!("a pre-open channel must reject withdrawal"),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("is not open"),
            "unexpected pre-open-channel error: {error:#}"
        );
    }

    #[tokio::test]
    async fn withdrawal_check_rejects_ambiguous_connection_hops() {
        let storage = TempStorage::new()
            .await
            .expect("temporary state must initialize");
        let mut state = StateDelta::new(storage.latest_snapshot());
        let withdrawal = test_withdrawal();
        let port = PortId::transfer();
        put_open_outbound_route(
            &mut state,
            &withdrawal,
            ChannelId::from_str("channel-7").expect("valid counterparty channel"),
        )
        .await
        .expect("valid outbound route");
        state.put_channel(
            &withdrawal.source_channel,
            &port,
            ChannelEnd {
                state: ChannelState::Open,
                ordering: Order::Unordered,
                remote: Counterparty::new(
                    port.clone(),
                    Some(ChannelId::from_str("channel-7").expect("valid counterparty channel")),
                ),
                connection_hops: vec![ConnectionId::new(0), ConnectionId::new(1)],
                version: Version::new("ics20-1".to_string()),
                ..ChannelEnd::default()
            },
        );

        let error = match withdrawal_check(
            &mut state,
            &withdrawal,
            Time::from_unix_timestamp(2, 0).expect("valid current block time"),
        )
        .await
        {
            Ok(_) => panic!("a channel with ambiguous connection hops must reject withdrawal"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("must have exactly one connection hop"),
            "unexpected connection-hop error: {error:#}"
        );
    }

    #[tokio::test]
    async fn withdrawal_check_rejects_non_open_connection() {
        let storage = TempStorage::new()
            .await
            .expect("temporary state must initialize");
        let mut state = StateDelta::new(storage.latest_snapshot());
        let withdrawal = test_withdrawal();
        put_open_outbound_route(
            &mut state,
            &withdrawal,
            ChannelId::from_str("channel-7").expect("valid counterparty channel"),
        )
        .await
        .expect("valid outbound route");
        state.update_connection(
            &ConnectionId::new(0),
            ConnectionEnd {
                state: ConnectionState::Init,
                client_id: ClientId::from_str("07-tendermint-0").expect("valid client ID"),
                counterparty: ConnectionCounterparty::default(),
                versions: vec![],
                delay_period: std::time::Duration::ZERO,
            },
        );

        let error = match withdrawal_check(
            &mut state,
            &withdrawal,
            Time::from_unix_timestamp(2, 0).expect("valid current block time"),
        )
        .await
        {
            Ok(_) => panic!("a non-open connection must reject withdrawal"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("connection connection-0 is not open"),
            "unexpected non-open-connection error: {error:#}"
        );
    }

    #[tokio::test]
    async fn withdrawal_check_rejects_missing_client() {
        let storage = TempStorage::new()
            .await
            .expect("temporary state must initialize");
        let mut state = StateDelta::new(storage.latest_snapshot());
        let withdrawal = test_withdrawal();
        put_open_outbound_route(
            &mut state,
            &withdrawal,
            ChannelId::from_str("channel-7").expect("valid counterparty channel"),
        )
        .await
        .expect("valid outbound route");
        let connection_id = ConnectionId::new(0);
        let mut connection = state
            .get_connection(&connection_id)
            .await
            .expect("connection read must succeed")
            .expect("test connection must exist");
        connection.client_id =
            ClientId::from_str("07-tendermint-9").expect("valid missing client ID");
        state.update_connection(&connection_id, connection);

        let error = match withdrawal_check(
            &mut state,
            &withdrawal,
            Time::from_unix_timestamp(2, 0).expect("valid current block time"),
        )
        .await
        {
            Ok(_) => panic!("a missing connection client must reject withdrawal"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("could not find client state for 07-tendermint-9"),
            "unexpected missing-client error: {error:#}"
        );
    }

    #[tokio::test]
    async fn withdrawal_check_rejects_missing_latest_consensus_state() {
        let storage = TempStorage::new()
            .await
            .expect("temporary state must initialize");
        let mut state = StateDelta::new(storage.latest_snapshot());
        let withdrawal = test_withdrawal();
        put_open_outbound_route(
            &mut state,
            &withdrawal,
            ChannelId::from_str("channel-7").expect("valid counterparty channel"),
        )
        .await
        .expect("valid outbound route");
        let client_id = ClientId::from_str("07-tendermint-0").expect("valid client ID");
        let latest_height = Height::new(1, 10).expect("valid latest height");
        state.delete(consensus_state_key(&client_id, &latest_height));

        let error = match withdrawal_check(
            &mut state,
            &withdrawal,
            Time::from_unix_timestamp(2, 0).expect("valid current block time"),
        )
        .await
        {
            Ok(_) => panic!("a client without its latest consensus state must reject withdrawal"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("counterparty consensus state not found"),
            "unexpected missing-consensus-state error: {error:#}"
        );
    }

    #[tokio::test]
    async fn withdrawal_check_rejects_future_latest_consensus_timestamp() {
        let storage = TempStorage::new()
            .await
            .expect("temporary state must initialize");
        let mut state = StateDelta::new(storage.latest_snapshot());
        let withdrawal = test_withdrawal();
        put_open_outbound_route(
            &mut state,
            &withdrawal,
            ChannelId::from_str("channel-7").expect("valid counterparty channel"),
        )
        .await
        .expect("valid outbound route");
        let client_id = ClientId::from_str("07-tendermint-0").expect("valid client ID");
        let latest_height = Height::new(1, 10).expect("valid latest height");
        state.put(
            consensus_state_key(&client_id, &latest_height),
            ConsensusState::new(
                MerkleRoot {
                    hash: vec![3u8; 32],
                },
                Time::from_unix_timestamp(3, 0).expect("valid future consensus timestamp"),
                tendermint::Hash::Sha256([4u8; 32]),
            ),
        );

        let error = match withdrawal_check(
            &mut state,
            &withdrawal,
            Time::from_unix_timestamp(2, 0).expect("valid current block time"),
        )
        .await
        {
            Ok(_) => panic!("a future latest consensus timestamp must reject withdrawal"),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("duration value out of range"),
            "unexpected future-consensus-time error: {error:#}"
        );
    }

    #[tokio::test]
    async fn withdrawal_check_rejects_exhausted_send_sequence() {
        let storage = TempStorage::new()
            .await
            .expect("temporary state must initialize");
        let mut state = StateDelta::new(storage.latest_snapshot());
        let withdrawal = test_withdrawal();
        let port = PortId::transfer();
        put_open_outbound_route(
            &mut state,
            &withdrawal,
            ChannelId::from_str("channel-7").expect("valid counterparty channel"),
        )
        .await
        .expect("valid outbound route");
        state.put_send_sequence(&withdrawal.source_channel, &port, u64::MAX);

        let error = match withdrawal_check(
            &mut state,
            &withdrawal,
            Time::from_unix_timestamp(2, 0).expect("valid current block time"),
        )
        .await
        {
            Ok(_) => panic!("an exhausted send sequence must reject withdrawal"),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("send sequence is exhausted"),
            "unexpected exhausted-sequence error: {error:#}"
        );
    }

    #[tokio::test]
    async fn withdrawal_execute_rejects_occupied_packet_slot_before_mutation() {
        let storage = TempStorage::new()
            .await
            .expect("temporary state must initialize");
        let mut state = StateDelta::new(storage.latest_snapshot());
        let withdrawal = test_withdrawal();
        let port = PortId::transfer();
        let counterparty_channel =
            ChannelId::from_str("channel-7").expect("valid counterparty channel");
        put_open_outbound_route(&mut state, &withdrawal, counterparty_channel.clone())
            .await
            .expect("valid outbound route");
        let checked = withdrawal_check(
            &mut state,
            &withdrawal,
            Time::from_unix_timestamp(2, 0).expect("valid current block time"),
        )
        .await
        .expect("valid withdrawal must check");
        state.put_packet_commitment(&Packet {
            sequence: Sequence::from(1),
            port_on_a: port.clone(),
            chan_on_a: withdrawal.source_channel.clone(),
            port_on_b: port.clone(),
            chan_on_b: counterparty_channel,
            data: b"occupied".to_vec(),
            timeout_height_on_b: TimeoutHeight::At(
                Height::new(1, 100).expect("valid timeout height"),
            ),
            timeout_timestamp_on_b: Timestamp::from_nanoseconds(60_000_000_000)
                .expect("valid timeout timestamp"),
        });

        let error = withdrawal_execute(&mut state, checked)
            .await
            .expect_err("an occupied packet slot must reject withdrawal");
        assert!(
            error
                .to_string()
                .contains("packet commitment already exists"),
            "unexpected occupied-slot error: {error:#}"
        );
        let escrowed: Option<Amount> = state
            .get(&state_key::ics20_value_balance::by_asset_id(
                &withdrawal.source_channel,
                &withdrawal.denom.id(),
            ))
            .await
            .expect("escrow read must succeed");
        assert!(
            escrowed.is_none(),
            "occupied packet slot must not mutate escrow accounting"
        );
        assert_eq!(
            state
                .get_send_sequence(&withdrawal.source_channel, &port)
                .await
                .expect("send sequence read must succeed"),
            1,
            "occupied packet slot must not allocate a sequence"
        );
    }

    #[tokio::test]
    async fn withdrawal_check_rejects_elapsed_height_and_timestamp_timeouts() {
        let storage = TempStorage::new()
            .await
            .expect("temporary state must initialize");
        let mut state = StateDelta::new(storage.latest_snapshot());
        let mut withdrawal = test_withdrawal();
        put_open_outbound_route(
            &mut state,
            &withdrawal,
            ChannelId::from_str("channel-7").expect("valid counterparty channel"),
        )
        .await
        .expect("valid outbound route");
        let current_time = Time::from_unix_timestamp(2, 0).expect("valid current block time");

        withdrawal.timeout_height = Height::new(1, 10).expect("valid timeout height");
        let height_error = match withdrawal_check(&mut state, &withdrawal, current_time).await {
            Ok(_) => panic!("elapsed timeout height must reject withdrawal"),
            Err(error) => error,
        };
        assert!(
            height_error.to_string().contains("timeout height"),
            "unexpected height-timeout error: {height_error:#}"
        );

        withdrawal.timeout_height = Height::new(1, 100).expect("valid timeout height");
        withdrawal.timeout_time = 1_000_000_000;
        let time_error = match withdrawal_check(&mut state, &withdrawal, current_time).await {
            Ok(_) => panic!("elapsed timeout timestamp must reject withdrawal"),
            Err(error) => error,
        };
        assert!(
            time_error.to_string().contains("timeout timestamp"),
            "unexpected timestamp-timeout error: {time_error:#}"
        );
    }

    #[tokio::test]
    async fn withdrawal_check_rejects_frozen_light_client() {
        let storage = TempStorage::new()
            .await
            .expect("temporary state must initialize");
        let mut state = StateDelta::new(storage.latest_snapshot());
        let withdrawal = test_withdrawal();
        put_open_outbound_route(
            &mut state,
            &withdrawal,
            ChannelId::from_str("channel-7").expect("valid counterparty channel"),
        )
        .await
        .expect("valid outbound route");
        let client_id = ClientId::from_str("07-tendermint-0").expect("valid client ID");
        let client = state
            .get_client_state(&client_id)
            .await
            .expect("stored client state");
        state.put_client(
            &client_id,
            client.with_frozen_height(Height::new(1, 1).expect("valid nonzero frozen height")),
        );

        let error = match withdrawal_check(
            &mut state,
            &withdrawal,
            Time::from_unix_timestamp(2, 0).expect("valid current block time"),
        )
        .await
        {
            Ok(_) => panic!("a frozen light client must reject withdrawal"),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("is frozen"),
            "unexpected frozen-client error: {error:#}"
        );
    }

    #[tokio::test]
    async fn withdrawal_check_rejects_expired_light_client() {
        let storage = TempStorage::new()
            .await
            .expect("temporary state must initialize");
        let mut state = StateDelta::new(storage.latest_snapshot());
        let withdrawal = test_withdrawal();
        put_open_outbound_route(
            &mut state,
            &withdrawal,
            ChannelId::from_str("channel-7").expect("valid counterparty channel"),
        )
        .await
        .expect("valid outbound route");

        let error = match withdrawal_check(
            &mut state,
            &withdrawal,
            Time::from_unix_timestamp(86_402, 0).expect("time beyond the client's trusting period"),
        )
        .await
        {
            Ok(_) => panic!("an expired light client must reject withdrawal"),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("is expired"),
            "unexpected expired-client error: {error:#}"
        );
    }

    #[tokio::test]
    async fn voucher_withdrawal_rejects_insufficient_channel_asset_balance() {
        let storage = TempStorage::new()
            .await
            .expect("temporary state must initialize");
        let mut state = StateDelta::new(storage.latest_snapshot());
        let mut withdrawal = test_withdrawal();
        let voucher_denom = format!("transfer/{}/uatom", withdrawal.source_channel);
        withdrawal.denom = voucher_denom
            .as_str()
            .try_into()
            .expect("valid voucher denomination");
        let port = PortId::transfer();
        put_open_outbound_route(
            &mut state,
            &withdrawal,
            ChannelId::from_str("channel-7").expect("valid counterparty channel"),
        )
        .await
        .expect("valid outbound route");
        let checked = withdrawal_check(
            &mut state,
            &withdrawal,
            Time::from_unix_timestamp(2, 0).expect("valid current block time"),
        )
        .await
        .expect("voucher packet and route must check");

        let error = withdrawal_execute(&mut state, checked)
            .await
            .expect_err("voucher withdrawal must not exceed channel-asset balance");
        assert!(
            error.to_string().contains("insufficient balance"),
            "unexpected voucher-balance error: {error:#}"
        );
        assert_eq!(
            state
                .get_send_sequence(&withdrawal.source_channel, &port)
                .await
                .expect("send sequence read must succeed"),
            1,
            "failed voucher accounting must not allocate a packet"
        );
    }

    #[tokio::test]
    async fn local_withdrawal_rejects_escrow_overflow_before_packet_allocation() {
        let storage = TempStorage::new()
            .await
            .expect("temporary state must initialize");
        let mut state = StateDelta::new(storage.latest_snapshot());
        let mut withdrawal = test_withdrawal();
        withdrawal.amount = Amount::from(1u64);
        let port = PortId::transfer();
        put_open_outbound_route(
            &mut state,
            &withdrawal,
            ChannelId::from_str("channel-7").expect("valid counterparty channel"),
        )
        .await
        .expect("valid outbound route");
        state.put(
            state_key::ics20_value_balance::by_asset_id(
                &withdrawal.source_channel,
                &withdrawal.denom.id(),
            ),
            Amount::from(u128::MAX),
        );
        let checked = withdrawal_check(
            &mut state,
            &withdrawal,
            Time::from_unix_timestamp(2, 0).expect("valid current block time"),
        )
        .await
        .expect("local packet and route must check");

        let error = withdrawal_execute(&mut state, checked)
            .await
            .expect_err("local escrow addition must reject overflow");
        assert!(
            error.to_string().contains("overflow adding value balance"),
            "unexpected escrow-overflow error: {error:#}"
        );
        assert_eq!(
            state
                .get_send_sequence(&withdrawal.source_channel, &port)
                .await
                .expect("send sequence read must succeed"),
            1,
            "failed source accounting must not allocate a packet"
        );
    }
}
