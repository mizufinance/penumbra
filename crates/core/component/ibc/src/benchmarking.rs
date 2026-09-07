use std::{cell::RefCell, future::Future, time::Duration};

#[derive(Clone, Copy, Debug)]
pub enum InboundStage {
    ChannelRead,
    ConnectionRead,
    TimeoutCheck,
    PacketProofVerify,
    DuplicateSequenceCheck,
    AppCheck,
    ReceiptWrite,
    AppExecuteTotal,
    PacketDataDecode,
    RouteResolve,
    ComplianceCheck,
    MintUnescrowAccounting,
    RegisterDenom,
    ValueBalanceRead,
    MintNoteTotal,
    MintNoteSctAppend,
    MintNoteBuild,
    MintNoteAddPayloadTotal,
    MintNotePendingPayload,
    ValueBalanceWrite,
    EventRecord,
    AppExecuteInner,
    AcknowledgementRead,
    AcknowledgementWrite,
    AcknowledgementTotal,
    DeferredSctReserve,
    DeferredSctMaterialize,
    DeferredSctPendingPayload,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StageTiming {
    pub count: u64,
    pub total_us: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct InboundReceiveBreakdown {
    pub channel_read: StageTiming,
    pub connection_read: StageTiming,
    pub timeout_check: StageTiming,
    pub packet_proof_verify: StageTiming,
    pub duplicate_sequence_check: StageTiming,
    pub app_check: StageTiming,
    pub receipt_write: StageTiming,
    pub app_execute_total: StageTiming,
    pub packet_data_decode: StageTiming,
    pub route_resolve: StageTiming,
    pub compliance_check: StageTiming,
    pub mint_unescrow_accounting: StageTiming,
    pub register_denom: StageTiming,
    pub value_balance_read: StageTiming,
    pub mint_note_total: StageTiming,
    pub mint_note_sct_append: StageTiming,
    pub mint_note_build: StageTiming,
    pub mint_note_add_payload_total: StageTiming,
    pub mint_note_pending_payload: StageTiming,
    pub value_balance_write: StageTiming,
    pub event_record: StageTiming,
    pub app_execute_inner: StageTiming,
    pub acknowledgement_read: StageTiming,
    pub acknowledgement_write: StageTiming,
    pub acknowledgement_total: StageTiming,
    pub deferred_sct_reserve: StageTiming,
    pub deferred_sct_materialize: StageTiming,
    pub deferred_sct_pending_payload: StageTiming,
}

tokio::task_local! {
    static INBOUND_RECEIVE: RefCell<InboundReceiveBreakdown>;
}

/// Measure only this future; concurrent and nested benchmark calls remain isolated.
pub async fn measure_inbound_receive<F: Future>(work: F) -> (F::Output, InboundReceiveBreakdown) {
    INBOUND_RECEIVE
        .scope(RefCell::new(InboundReceiveBreakdown::default()), async {
            let result = work.await;
            let timings = INBOUND_RECEIVE.with(|timings| *timings.borrow());
            (result, timings)
        })
        .await
}

pub fn record_inbound_stage(stage: InboundStage, elapsed: Duration) {
    let _ = INBOUND_RECEIVE.try_with(|timings| {
        let mut timings = timings.borrow_mut();
        let timing = match stage {
            InboundStage::ChannelRead => &mut timings.channel_read,
            InboundStage::ConnectionRead => &mut timings.connection_read,
            InboundStage::TimeoutCheck => &mut timings.timeout_check,
            InboundStage::PacketProofVerify => &mut timings.packet_proof_verify,
            InboundStage::DuplicateSequenceCheck => &mut timings.duplicate_sequence_check,
            InboundStage::AppCheck => &mut timings.app_check,
            InboundStage::ReceiptWrite => &mut timings.receipt_write,
            InboundStage::AppExecuteTotal => &mut timings.app_execute_total,
            InboundStage::PacketDataDecode => &mut timings.packet_data_decode,
            InboundStage::RouteResolve => &mut timings.route_resolve,
            InboundStage::ComplianceCheck => &mut timings.compliance_check,
            InboundStage::MintUnescrowAccounting => &mut timings.mint_unescrow_accounting,
            InboundStage::RegisterDenom => &mut timings.register_denom,
            InboundStage::ValueBalanceRead => &mut timings.value_balance_read,
            InboundStage::MintNoteTotal => &mut timings.mint_note_total,
            InboundStage::MintNoteSctAppend => &mut timings.mint_note_sct_append,
            InboundStage::MintNoteBuild => &mut timings.mint_note_build,
            InboundStage::MintNoteAddPayloadTotal => &mut timings.mint_note_add_payload_total,
            InboundStage::MintNotePendingPayload => &mut timings.mint_note_pending_payload,
            InboundStage::ValueBalanceWrite => &mut timings.value_balance_write,
            InboundStage::EventRecord => &mut timings.event_record,
            InboundStage::AppExecuteInner => &mut timings.app_execute_inner,
            InboundStage::AcknowledgementRead => &mut timings.acknowledgement_read,
            InboundStage::AcknowledgementWrite => &mut timings.acknowledgement_write,
            InboundStage::AcknowledgementTotal => &mut timings.acknowledgement_total,
            InboundStage::DeferredSctReserve => &mut timings.deferred_sct_reserve,
            InboundStage::DeferredSctMaterialize => &mut timings.deferred_sct_materialize,
            InboundStage::DeferredSctPendingPayload => &mut timings.deferred_sct_pending_payload,
        };
        timing.count = timing.count.saturating_add(1);
        timing.total_us = timing
            .total_us
            .saturating_add(elapsed.as_micros().min(u64::MAX as u128) as u64);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn concurrent_and_nested_measurements_are_isolated() {
        let measure = |count: u64| {
            measure_inbound_receive(async move {
                for _ in 0..count {
                    record_inbound_stage(InboundStage::ChannelRead, Duration::from_micros(7));
                    tokio::task::yield_now().await;
                }
            })
        };
        let (a, b) = tokio::join!(measure(3), measure(5));
        assert_eq!((a.1.channel_read.count, a.1.channel_read.total_us), (3, 21));
        assert_eq!((b.1.channel_read.count, b.1.channel_read.total_us), (5, 35));
        let (inner, outer) = measure_inbound_receive(measure(2)).await;
        assert_eq!(inner.1.channel_read.count, 2);
        assert_eq!(outer.channel_read.count, 0);
        record_inbound_stage(InboundStage::ChannelRead, Duration::from_micros(99));
        assert_eq!(measure(0).await.1.channel_read.count, 0);
    }
}
