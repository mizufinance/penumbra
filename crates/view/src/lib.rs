//! The view RPC library for the Shieldd Zone.
//!
//! This crate provides a [`ViewClient`] and a [`ViewServer`]. These form a client-server pair to
//! synchronize and interact with public chain state using one or more full viewing keys. See the
//! documentation of [`ViewClient`] and a [`ViewServer`] for more information.
//!
//! This crate also provides a wallet-facing [`NoteManager`] for shielded transfer,
//! note reshape, ICS-20 and host withdrawal, and transfer-funded action planning.
//!
//! Finally, this crate provides a [`Storage`] type for managing persistent sqlite storage.

#![deny(clippy::unwrap_used)]
#![recursion_limit = "512"]
// Requires nightly.
#![cfg_attr(docsrs, feature(doc_cfg))]
mod client;
mod client_compliance;
mod compliance_tree;
mod historical_proof_cache;
mod historical_proof_worker;
mod issued_address;
mod note_manager;
mod note_record;
mod service;
mod status;
mod storage;
mod sync;
mod transaction_info;
mod worker;

pub use crate::client::ViewClient;
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
pub use crate::service::ViewServer;
pub use crate::status::StatusStreamResponse;
pub use crate::storage::Storage;
pub use crate::transaction_info::TransactionInfo;

pub mod planning_intent;

pub mod planning_io;
