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
pub use action_handler::output::{
    output_build_public, output_check_stateless_and_extract, output_extract_public,
    output_parse_ciphertext_fields, output_parse_dleq_fields, output_to_batch_item,
    OutputCiphertextFields, OutputDleqFields,
};
pub use action_handler::spend::{
    spend_build_public, spend_check_stateless_and_extract, spend_extract_public,
    spend_parse_ciphertext_fields, spend_parse_dleq_fields, spend_to_batch_item,
    spend_verify_auth_sig, SpendCiphertextFields, SpendDleqFields,
};

pub mod rpc;
