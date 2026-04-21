//! The view RPC library for the Penumbra Zone.
//!
//! This crate provides a [`ViewClient`] and a [`ViewServer`]. These form a client-server pair to
//! synchronize and interact with public chain state using one or more full viewing keys. See the
//! documentation of [`ViewClient`] and a [`ViewServer`] for more information.
//!
//! This crate also provides a wallet-facing [`NoteManager`] for shielded transfer,
//! consolidate, split, ICS-20 withdrawal, and transfer-funded action planning.
//!
//! Finally, this crate provides a [`Storage`] type for managing persistent sqlite storage.

#![deny(clippy::unwrap_used)]
#![recursion_limit = "512"]
// Requires nightly.
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
mod client;
mod client_compliance;
mod compliance_tree;
mod metrics;
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
    enrich_plan_with_compliance, ViewClientComplianceExt, ViewClientComplianceProvider,
};
pub use crate::compliance_tree::{ComplianceAssetTree, ComplianceUserTree};
pub use crate::metrics::register_metrics;
pub use crate::note_manager::{
    NoteManager, NoteManagerPlanningResult, NoteManagerResumeToken, TransferPlanningResult,
    TransferResumeToken,
};
pub use crate::note_record::SpendableNoteRecord;
pub use crate::service::ViewServer;
pub use crate::status::StatusStreamResponse;
pub use crate::storage::Storage;
pub use crate::transaction_info::TransactionInfo;
