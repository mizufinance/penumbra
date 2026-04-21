//! The Penumbra shielded pool [`Component`] and [`ActionHandler`] implementations.

mod action_handler;
mod assets;
mod fmd;
mod ics20_withdrawal_with_handler;
mod metrics;
mod note_manager;
mod shielded_pool;
mod transfer;

pub use self::metrics::register_metrics;
pub use assets::{AssetRegistry, AssetRegistryRead};
pub use fmd::ClueManager;
pub use ics20_withdrawal_with_handler::Ics20WithdrawalWithHandler;
pub use note_manager::NoteManager;
pub use shielded_pool::{ShieldedPool, StateReadExt, StateWriteExt};
pub use transfer::Ics20Transfer;

// Batch verification helpers for process_proposal
pub use action_handler::consolidate::{
    consolidate_check_stateless_and_extract, consolidate_extract_public, consolidate_to_batch_item,
    consolidate_verify_auth_sigs,
};
pub use action_handler::shielded_ics20_withdrawal::{
    shielded_ics20_withdrawal_check_stateless_and_extract,
    shielded_ics20_withdrawal_extract_public, shielded_ics20_withdrawal_to_batch_item,
    shielded_ics20_withdrawal_verify_auth_sigs,
};
pub use action_handler::split::{
    split_check_stateless_and_extract, split_extract_public, split_to_batch_item,
    split_verify_auth_sigs,
};
pub use action_handler::transfer::{
    transfer_check_stateless_and_extract, transfer_extract_public, transfer_to_batch_item,
    transfer_verify_auth_sigs,
};

pub mod rpc;
