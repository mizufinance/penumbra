//! Execution lifecycle and transaction action traits for cnidarium components.

#![deny(clippy::unwrap_used)]
// Requires nightly.
#![cfg_attr(docsrs, feature(doc_cfg))]

mod action_handler;
mod component;

pub use action_handler::ActionHandler;
pub use component::{BlockContext, Component};
