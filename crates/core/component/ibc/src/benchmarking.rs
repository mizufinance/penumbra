use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

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

struct AtomicStageTiming {
    count: AtomicU64,
    total_us: AtomicU64,
}

impl AtomicStageTiming {
    const fn new() -> Self {
        Self {
            count: AtomicU64::new(0),
            total_us: AtomicU64::new(0),
        }
    }

    fn record(&self, elapsed: Duration) {
        self.count.fetch_add(1, Ordering::Relaxed);
        self.total_us
            .fetch_add(elapsed.as_micros() as u64, Ordering::Relaxed);
    }

    fn reset(&self) {
        self.count.store(0, Ordering::Relaxed);
        self.total_us.store(0, Ordering::Relaxed);
    }

    fn snapshot(&self) -> StageTiming {
        StageTiming {
            count: self.count.load(Ordering::Relaxed),
            total_us: self.total_us.load(Ordering::Relaxed),
        }
    }
}

struct AtomicInboundReceiveBreakdown {
    channel_read: AtomicStageTiming,
    connection_read: AtomicStageTiming,
    timeout_check: AtomicStageTiming,
    packet_proof_verify: AtomicStageTiming,
    duplicate_sequence_check: AtomicStageTiming,
    app_check: AtomicStageTiming,
    receipt_write: AtomicStageTiming,
    app_execute_total: AtomicStageTiming,
    packet_data_decode: AtomicStageTiming,
    route_resolve: AtomicStageTiming,
    compliance_check: AtomicStageTiming,
    mint_unescrow_accounting: AtomicStageTiming,
    register_denom: AtomicStageTiming,
    value_balance_read: AtomicStageTiming,
    mint_note_total: AtomicStageTiming,
    mint_note_sct_append: AtomicStageTiming,
    mint_note_build: AtomicStageTiming,
    mint_note_add_payload_total: AtomicStageTiming,
    mint_note_pending_payload: AtomicStageTiming,
    value_balance_write: AtomicStageTiming,
    event_record: AtomicStageTiming,
    app_execute_inner: AtomicStageTiming,
    acknowledgement_read: AtomicStageTiming,
    acknowledgement_write: AtomicStageTiming,
    acknowledgement_total: AtomicStageTiming,
    deferred_sct_reserve: AtomicStageTiming,
    deferred_sct_materialize: AtomicStageTiming,
    deferred_sct_pending_payload: AtomicStageTiming,
}

impl AtomicInboundReceiveBreakdown {
    const fn new() -> Self {
        Self {
            channel_read: AtomicStageTiming::new(),
            connection_read: AtomicStageTiming::new(),
            timeout_check: AtomicStageTiming::new(),
            packet_proof_verify: AtomicStageTiming::new(),
            duplicate_sequence_check: AtomicStageTiming::new(),
            app_check: AtomicStageTiming::new(),
            receipt_write: AtomicStageTiming::new(),
            app_execute_total: AtomicStageTiming::new(),
            packet_data_decode: AtomicStageTiming::new(),
            route_resolve: AtomicStageTiming::new(),
            compliance_check: AtomicStageTiming::new(),
            mint_unescrow_accounting: AtomicStageTiming::new(),
            register_denom: AtomicStageTiming::new(),
            value_balance_read: AtomicStageTiming::new(),
            mint_note_total: AtomicStageTiming::new(),
            mint_note_sct_append: AtomicStageTiming::new(),
            mint_note_build: AtomicStageTiming::new(),
            mint_note_add_payload_total: AtomicStageTiming::new(),
            mint_note_pending_payload: AtomicStageTiming::new(),
            value_balance_write: AtomicStageTiming::new(),
            event_record: AtomicStageTiming::new(),
            app_execute_inner: AtomicStageTiming::new(),
            acknowledgement_read: AtomicStageTiming::new(),
            acknowledgement_write: AtomicStageTiming::new(),
            acknowledgement_total: AtomicStageTiming::new(),
            deferred_sct_reserve: AtomicStageTiming::new(),
            deferred_sct_materialize: AtomicStageTiming::new(),
            deferred_sct_pending_payload: AtomicStageTiming::new(),
        }
    }

    fn reset(&self) {
        self.channel_read.reset();
        self.connection_read.reset();
        self.timeout_check.reset();
        self.packet_proof_verify.reset();
        self.duplicate_sequence_check.reset();
        self.app_check.reset();
        self.receipt_write.reset();
        self.app_execute_total.reset();
        self.packet_data_decode.reset();
        self.route_resolve.reset();
        self.compliance_check.reset();
        self.mint_unescrow_accounting.reset();
        self.register_denom.reset();
        self.value_balance_read.reset();
        self.mint_note_total.reset();
        self.mint_note_sct_append.reset();
        self.mint_note_build.reset();
        self.mint_note_add_payload_total.reset();
        self.mint_note_pending_payload.reset();
        self.value_balance_write.reset();
        self.event_record.reset();
        self.app_execute_inner.reset();
        self.acknowledgement_read.reset();
        self.acknowledgement_write.reset();
        self.acknowledgement_total.reset();
        self.deferred_sct_reserve.reset();
        self.deferred_sct_materialize.reset();
        self.deferred_sct_pending_payload.reset();
    }

    fn snapshot(&self) -> InboundReceiveBreakdown {
        InboundReceiveBreakdown {
            channel_read: self.channel_read.snapshot(),
            connection_read: self.connection_read.snapshot(),
            timeout_check: self.timeout_check.snapshot(),
            packet_proof_verify: self.packet_proof_verify.snapshot(),
            duplicate_sequence_check: self.duplicate_sequence_check.snapshot(),
            app_check: self.app_check.snapshot(),
            receipt_write: self.receipt_write.snapshot(),
            app_execute_total: self.app_execute_total.snapshot(),
            packet_data_decode: self.packet_data_decode.snapshot(),
            route_resolve: self.route_resolve.snapshot(),
            compliance_check: self.compliance_check.snapshot(),
            mint_unescrow_accounting: self.mint_unescrow_accounting.snapshot(),
            register_denom: self.register_denom.snapshot(),
            value_balance_read: self.value_balance_read.snapshot(),
            mint_note_total: self.mint_note_total.snapshot(),
            mint_note_sct_append: self.mint_note_sct_append.snapshot(),
            mint_note_build: self.mint_note_build.snapshot(),
            mint_note_add_payload_total: self.mint_note_add_payload_total.snapshot(),
            mint_note_pending_payload: self.mint_note_pending_payload.snapshot(),
            value_balance_write: self.value_balance_write.snapshot(),
            event_record: self.event_record.snapshot(),
            app_execute_inner: self.app_execute_inner.snapshot(),
            acknowledgement_read: self.acknowledgement_read.snapshot(),
            acknowledgement_write: self.acknowledgement_write.snapshot(),
            acknowledgement_total: self.acknowledgement_total.snapshot(),
            deferred_sct_reserve: self.deferred_sct_reserve.snapshot(),
            deferred_sct_materialize: self.deferred_sct_materialize.snapshot(),
            deferred_sct_pending_payload: self.deferred_sct_pending_payload.snapshot(),
        }
    }
}

static INBOUND_RECEIVE: AtomicInboundReceiveBreakdown = AtomicInboundReceiveBreakdown::new();

pub fn reset_inbound_receive_breakdown() {
    INBOUND_RECEIVE.reset();
}

pub fn snapshot_inbound_receive_breakdown() -> InboundReceiveBreakdown {
    INBOUND_RECEIVE.snapshot()
}

pub fn record_inbound_stage(stage: InboundStage, elapsed: Duration) {
    match stage {
        InboundStage::ChannelRead => INBOUND_RECEIVE.channel_read.record(elapsed),
        InboundStage::ConnectionRead => INBOUND_RECEIVE.connection_read.record(elapsed),
        InboundStage::TimeoutCheck => INBOUND_RECEIVE.timeout_check.record(elapsed),
        InboundStage::PacketProofVerify => INBOUND_RECEIVE.packet_proof_verify.record(elapsed),
        InboundStage::DuplicateSequenceCheck => {
            INBOUND_RECEIVE.duplicate_sequence_check.record(elapsed)
        }
        InboundStage::AppCheck => INBOUND_RECEIVE.app_check.record(elapsed),
        InboundStage::ReceiptWrite => INBOUND_RECEIVE.receipt_write.record(elapsed),
        InboundStage::AppExecuteTotal => INBOUND_RECEIVE.app_execute_total.record(elapsed),
        InboundStage::PacketDataDecode => INBOUND_RECEIVE.packet_data_decode.record(elapsed),
        InboundStage::RouteResolve => INBOUND_RECEIVE.route_resolve.record(elapsed),
        InboundStage::ComplianceCheck => INBOUND_RECEIVE.compliance_check.record(elapsed),
        InboundStage::MintUnescrowAccounting => {
            INBOUND_RECEIVE.mint_unescrow_accounting.record(elapsed)
        }
        InboundStage::RegisterDenom => INBOUND_RECEIVE.register_denom.record(elapsed),
        InboundStage::ValueBalanceRead => INBOUND_RECEIVE.value_balance_read.record(elapsed),
        InboundStage::MintNoteTotal => INBOUND_RECEIVE.mint_note_total.record(elapsed),
        InboundStage::MintNoteSctAppend => INBOUND_RECEIVE.mint_note_sct_append.record(elapsed),
        InboundStage::MintNoteBuild => INBOUND_RECEIVE.mint_note_build.record(elapsed),
        InboundStage::MintNoteAddPayloadTotal => {
            INBOUND_RECEIVE.mint_note_add_payload_total.record(elapsed)
        }
        InboundStage::MintNotePendingPayload => {
            INBOUND_RECEIVE.mint_note_pending_payload.record(elapsed)
        }
        InboundStage::ValueBalanceWrite => INBOUND_RECEIVE.value_balance_write.record(elapsed),
        InboundStage::EventRecord => INBOUND_RECEIVE.event_record.record(elapsed),
        InboundStage::AppExecuteInner => INBOUND_RECEIVE.app_execute_inner.record(elapsed),
        InboundStage::AcknowledgementRead => INBOUND_RECEIVE.acknowledgement_read.record(elapsed),
        InboundStage::AcknowledgementWrite => INBOUND_RECEIVE.acknowledgement_write.record(elapsed),
        InboundStage::AcknowledgementTotal => INBOUND_RECEIVE.acknowledgement_total.record(elapsed),
        InboundStage::DeferredSctReserve => INBOUND_RECEIVE.deferred_sct_reserve.record(elapsed),
        InboundStage::DeferredSctMaterialize => {
            INBOUND_RECEIVE.deferred_sct_materialize.record(elapsed)
        }
        InboundStage::DeferredSctPendingPayload => {
            INBOUND_RECEIVE.deferred_sct_pending_payload.record(elapsed)
        }
    }
}
