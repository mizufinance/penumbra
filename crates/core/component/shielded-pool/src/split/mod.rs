mod action;
mod generated;
mod plan;
mod proof;
#[cfg(any(unix, windows))]
mod prover_runtime;
mod view;

pub use action::{Split, SplitBody, SplitInputBody, SplitOutputBody};
pub use generated::{SplitFamilyId, SplitFamilySpec, SPLIT_FAMILY_SPECS};
pub use plan::SplitPlan;
pub use proof::{
    SplitInputPrivate, SplitInputPublic, SplitOutputPrivate, SplitOutputPublic, SplitProof,
    SplitProofPrivate, SplitProofPublic,
};
pub use view::SplitView;
