#![deny(clippy::unwrap_used)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#[cfg(feature = "component")]
pub mod component;

pub mod ics20_withdrawal;
pub use ics20_withdrawal::Ics20Withdrawal;

pub mod event;
pub mod fmd;
pub mod genesis;
pub mod gnark;
pub mod params;
pub mod state_key;

pub mod note;
mod note_payload;
mod note_reshape;
pub mod rseed;
mod shielded_note_plan;

pub use note::{Note, NoteCiphertext, NoteView};
pub use note_payload::NotePayload;
pub use rseed::Rseed;

pub use shielded_note_plan::{ShieldedInputPlan, ShieldedOutputPlan};

pub mod consolidate;
pub mod public_input_hash;
pub mod r1cs;
pub mod shielded_ics20_withdrawal;
pub mod split;
pub mod transfer;

pub mod backref;
pub use backref::{Backref, EncryptedBackref};

pub mod compliance_helpers;

pub mod proof_error;
pub use proof_error::{ProofError, ProofResult};

pub use consolidate::{
    Consolidate, ConsolidateBody, ConsolidateFamilyId, ConsolidateFamilySpec, ConsolidateInputBody,
    ConsolidateInputPrivate, ConsolidateInputPublic, ConsolidateOutputBody,
    ConsolidateOutputPrivate, ConsolidateOutputPublic, ConsolidatePlan, ConsolidateProof,
    ConsolidateProofPrivate, ConsolidateProofPublic, ConsolidateView, CONSOLIDATE_FAMILY_SPECS,
};
pub use shielded_ics20_withdrawal::{
    ShieldedIcs20Withdrawal, ShieldedIcs20WithdrawalBody, ShieldedIcs20WithdrawalChangeBody,
    ShieldedIcs20WithdrawalChangePrivate, ShieldedIcs20WithdrawalChangePublic,
    ShieldedIcs20WithdrawalFamilyId, ShieldedIcs20WithdrawalFamilySpec,
    ShieldedIcs20WithdrawalInputPrivate, ShieldedIcs20WithdrawalInputPublic,
    ShieldedIcs20WithdrawalPlan, ShieldedIcs20WithdrawalProof, ShieldedIcs20WithdrawalProofPrivate,
    ShieldedIcs20WithdrawalProofPublic, ShieldedIcs20WithdrawalView,
    SHIELDED_ICS20_WITHDRAWAL_FAMILY_SPECS,
};
pub use split::{
    Split, SplitBody, SplitFamilyId, SplitFamilySpec, SplitInputBody, SplitInputPrivate,
    SplitInputPublic, SplitOutputBody, SplitOutputPrivate, SplitOutputPublic, SplitPlan,
    SplitProof, SplitProofPrivate, SplitProofPublic, SplitView, SPLIT_FAMILY_SPECS,
};
pub use transfer::{
    transfer_auth_sig_count, transfer_input_count, transfer_output_count, Transfer, TransferBody,
    TransferInputBody, TransferOutputBody, TransferOutputPrivate, TransferOutputPublic,
    TransferPlan, TransferProof, TransferProofPrivate, TransferProofPublic, TransferSpendPrivate,
    TransferSpendPublic, TransferView, PADDED_TRANSFER_INPUTS, PADDED_TRANSFER_OUTPUTS,
    TRANSFER_ARTIFACT_NAME, TRANSFER_PROOF_LABEL, TRANSFER_STATEMENT_FIELD_COUNT,
};

#[cfg(feature = "benchmark-helpers")]
pub mod benchmark_helpers;
#[cfg(any(test, feature = "benchmark-helpers"))]
pub mod test_proof_helpers;
