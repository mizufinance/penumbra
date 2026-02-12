use anyhow::Result;

use crate::client_types::{AnyClientState, AnyConsensusState, AnyHeader};

/// Abstraction for header verification logic, allowing different light client
/// types to plug in their own verification.
pub trait ClientProvider {
    /// Verify an untrusted header against the trusted client state and consensus state.
    ///
    /// Returns the updated (client_state, consensus_state) pair on success.
    ///
    /// `host_timestamp` is the current block timestamp in unix seconds, used for
    /// deterministic trusting period checks.
    fn verify_header(
        &self,
        client_state: &AnyClientState,
        trusted_consensus: &AnyConsensusState,
        header: &AnyHeader,
        host_timestamp: u64,
    ) -> Result<(AnyClientState, AnyConsensusState)>;

    /// Check whether submitted misbehaviour evidence is valid.
    ///
    /// Returns `true` if the misbehaviour is confirmed and the client should be frozen.
    fn check_misbehaviour(
        &self,
        client_state: &AnyClientState,
        misbehaviour: &ibc_proto::google::protobuf::Any,
    ) -> Result<bool>;
}

/// Provider that wraps the existing Tendermint `ProdVerifier` logic.
///
/// This is a marker struct; the actual Tendermint verification logic remains
/// inline in `update_client.rs` and `misbehavior.rs` for now. This struct
/// exists so that future refactors can move the verification behind the trait.
pub struct TendermintProvider;
