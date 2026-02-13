use anyhow::{Context, Result};
use async_trait::async_trait;
use cnidarium::StateWrite;
use ibc_types::core::client::{events::CreateClient, msgs::MsgCreateClient, ClientId};

use crate::client_types::{AnyClientState, AnyConsensusState};
use crate::component::{
    client::{ConsensusStateWriteExt as _, StateReadExt as _, StateWriteExt as _},
    client_counter::ClientCounter,
    HostInterface, MsgHandler,
};

#[async_trait]
impl MsgHandler for MsgCreateClient {
    async fn check_stateless<H>(&self) -> Result<()> {
        // Accepts any known client type. Bankd arms will be fully
        // implemented in B06-T3; for now they pass stateless checks
        // but bail at execution time.
        AnyClientState::try_from(self.client_state.clone())
            .context("MsgCreateClient: unsupported client state type")?;
        AnyConsensusState::try_from(self.consensus_state.clone())
            .context("MsgCreateClient: unsupported consensus state type")?;

        Ok(())
    }

    // execute IBC CreateClient.
    //
    //  we compute the client's ID (a concatenation of a monotonically increasing integer, the
    //  number of clients on Penumbra, and the client type) and commit the following to our state:
    // - client type
    // - consensus state
    // - processed time and height
    async fn try_execute<S: StateWrite, AH, HI: HostInterface>(&self, mut state: S) -> Result<()> {
        tracing::debug!(msg = ?self);

        let any_client_state = AnyClientState::try_from(self.client_state.clone())?;
        let any_consensus_state = AnyConsensusState::try_from(self.consensus_state.clone())?;

        // get the current client counter
        let id_counter = state.client_counter().await?;
        let client_type =
            ibc_types::core::client::ClientType(any_client_state.client_type().to_string());
        let client_id = ClientId::new(client_type.clone(), id_counter.0)?;

        tracing::info!("creating client {:?}", client_id);

        let latest_height = any_client_state
            .latest_height()
            .context("unable to get latest height from client state")?;

        // store the client data
        state.put_client(&client_id, any_client_state);

        // store the genesis consensus state
        state
            .put_verified_consensus_state::<HI>(
                latest_height,
                client_id.clone(),
                any_consensus_state,
            )
            .await
            .context("unable to put verified consensus state")?;

        // increment client counter
        let counter = state.client_counter().await.unwrap_or(ClientCounter(0));
        state.put_client_counter(ClientCounter(counter.0 + 1));

        state.record(
            CreateClient {
                client_id: client_id.clone(),
                client_type,
                consensus_height: latest_height,
            }
            .into(),
        );

        Ok(())
    }
}
