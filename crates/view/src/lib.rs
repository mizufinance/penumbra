//! Wallet planning, scanning, compliance projection and durable local storage.

#![deny(clippy::unwrap_used)]
#![recursion_limit = "512"]
// Requires nightly.
#![cfg_attr(docsrs, feature(doc_cfg))]
mod client_compliance;
mod compliance_tree;
mod historical_proof_cache;
mod historical_proof_worker;
mod issued_address;
mod note_manager;
mod note_record;
mod status;
mod storage;
mod sync;
mod transaction_info;
mod worker;

pub use crate::client_compliance::{
    complete_plan_with_compliance, CompletionData, VolumeRecoveryRecord,
};
pub use crate::compliance_tree::{ComplianceAssetTree, ComplianceUserTree};
pub use crate::historical_proof_cache::{
    advance_historical_proof_cache, HistoricalProofCache, HistoricalProofCacheState,
    HistoricalProofProvider, HistoricalProofUpdateError, HistoricalWitnessSource,
};
pub use crate::issued_address::{AddressPurpose, IssuedAddress};
pub use crate::note_manager::{
    NoteManager, NoteManagerPlanningResult, NoteManagerResumeToken, TransferResumeToken,
};
pub use crate::note_record::SpendableNoteRecord;
pub use crate::status::StatusStreamResponse;
pub use crate::storage::{
    ConfirmedVolumeAccumulator, Storage, VolumeAccumulatorRecovery, VolumeAccumulatorReservation,
};
pub use crate::transaction_info::TransactionInfo;

pub mod planning_intent;

pub mod planning_io;

pub use worker::{SyncWorker, WalletBlock};

pub use historical_proof_worker::{HistoricalProofWorker, RpcHistoricalWitnessSource};

mod storage_planning;
pub use storage_planning::StoragePlanningIo;
