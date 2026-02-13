use anyhow::{Context, Result};
use async_trait::async_trait;
use cnidarium::{StateRead, StateWrite};
use ibc_types::{
    core::{client::events::UpdateClient, client::msgs::MsgUpdateClient, client::ClientId},
    lightclients::tendermint::client_state::ClientState as TendermintClientState,
    lightclients::tendermint::header::Header as TendermintHeader,
    lightclients::tendermint::{
        consensus_state::ConsensusState as TendermintConsensusState, TENDERMINT_CLIENT_TYPE,
    },
};
use tendermint::validator;
use tendermint_light_client_verifier::{
    types::{TrustedBlockState, UntrustedBlockState},
    ProdVerifier, Verdict, Verifier,
};

use crate::bankd_provider::BankdProvider;
use crate::client_provider::ClientProvider;
use crate::client_types::{AnyClientState, AnyConsensusState, AnyHeader};
use crate::component::{
    client::{
        ConsensusStateWriteExt as _, Ics2ClientExt as _, StateReadExt as _, StateWriteExt as _,
    },
    HostInterface, MsgHandler,
};

#[async_trait]
impl MsgHandler for MsgUpdateClient {
    async fn check_stateless<AH>(&self) -> Result<()> {
        // Accepts any known client type. Bankd arms will be fully
        // implemented in B06-T3; for now they pass stateless checks
        // but bail at execution time.
        AnyHeader::try_from(self.client_message.clone())
            .context("MsgUpdateClient: unsupported header type")?;

        Ok(())
    }

    async fn try_execute<S: StateWrite, AH, HI: HostInterface>(&self, mut state: S) -> Result<()> {
        // Optimization: no-op if the update is already committed.  We no-op
        // to Ok(()) rather than erroring to avoid having two "racing" relay
        // transactions fail just because they both contain the same client
        // update.
        if update_is_already_committed(&state, self).await? {
            tracing::debug!("skipping duplicate update");
            return Ok(());
        }
        tracing::debug!(msg = ?self);

        let client_state = client_is_present(&state, self).await?;

        client_is_not_frozen(&client_state)?;
        client_is_not_expired::<&S, HI>(&state, &self.client_id, &client_state).await?;

        let trusted_client_state = client_state;

        let any_header = AnyHeader::try_from(self.client_message.clone())?;

        match any_header {
            AnyHeader::Tendermint(ref untrusted_header) => {
                let trusted_tm_cs = match &trusted_client_state {
                    AnyClientState::Tendermint(cs) => cs,
                    _ => anyhow::bail!("expected Tendermint client state for Tendermint header"),
                };

                header_revision_matches_client_state(trusted_tm_cs, untrusted_header)?;
                header_height_is_consistent(untrusted_header)?;

                // The (still untrusted) header uses the `trusted_height` field to
                // specify the trusted anchor data it is extending.
                let trusted_height = untrusted_header.trusted_height;

                // We use the specified trusted height to query the trusted
                // consensus state the update extends.
                let last_trusted_consensus_state = state
                    .get_verified_consensus_state(&trusted_height, &self.client_id)
                    .await?;

                let last_trusted_tm_consensus = match &last_trusted_consensus_state {
                    AnyConsensusState::Tendermint(cs) => cs,
                    _ => anyhow::bail!("expected Tendermint consensus state"),
                };

                // We also have to convert from an IBC height, which has two
                // components, to a Tendermint height, which has only one.
                let trusted_height = trusted_height
                    .revision_height()
                    .try_into()
                    .context("invalid header height")?;

                let trusted_validator_set =
                    verify_header_validator_set(untrusted_header, last_trusted_tm_consensus)?;

                // Now we build the trusted and untrusted states to feed to the Tendermint light client.

                let trusted_state = TrustedBlockState {
                    // TODO(erwan): do we need an additional check on `chain_id`
                    chain_id: &trusted_tm_cs.chain_id.clone().into(),
                    header_time: last_trusted_tm_consensus.timestamp,
                    height: trusted_height,
                    next_validators: trusted_validator_set,
                    next_validators_hash: last_trusted_tm_consensus.next_validators_hash,
                };

                let untrusted_state = UntrustedBlockState {
                    signed_header: &untrusted_header.signed_header,
                    validators: &untrusted_header.validator_set,
                    next_validators: None, // TODO: do we need this?
                };

                let options = trusted_tm_cs.as_light_client_options()?;
                let verifier = ProdVerifier::default();

                let verdict = verifier.verify_update_header(
                    untrusted_state,
                    trusted_state,
                    &options,
                    HI::get_block_timestamp(&state).await?,
                );

                match verdict {
                    Verdict::Success => Ok(()),
                    Verdict::NotEnoughTrust(voting_power_tally) => Err(anyhow::anyhow!(
                        "not enough trust, voting power tally: {:?}",
                        voting_power_tally
                    )),
                    Verdict::Invalid(detail) => Err(anyhow::anyhow!(
                        "could not verify tendermint header: invalid: {:?}",
                        detail
                    )),
                }?;

                let trusted_header = untrusted_header.clone();

                // get the latest client state
                let client_state = state
                    .get_client_state(&self.client_id)
                    .await
                    .context("unable to get client state")?;

                // NOTE: next_client_state will freeze the client on equivocation.
                let (next_client_state, next_consensus_state) = state
                    .next_client_state(
                        self.client_id.clone(),
                        client_state.clone(),
                        AnyHeader::Tendermint(trusted_header.clone()),
                    )
                    .await?;

                // store the updated client and consensus states
                state.put_client(&self.client_id, next_client_state);
                state
                    .put_verified_consensus_state::<HI>(
                        trusted_header.height(),
                        self.client_id.clone(),
                        next_consensus_state,
                    )
                    .await?;

                state.record(
                    UpdateClient {
                        client_id: self.client_id.clone(),
                        client_type: ibc_types::core::client::ClientType(
                            TENDERMINT_CLIENT_TYPE.to_string(),
                        ),
                        consensus_height: trusted_header.height(),
                        header:
                            <ibc_types::lightclients::tendermint::header::Header as ibc_proto::Protobuf<
                                ibc_proto::ibc::lightclients::tendermint::v1::Header,
                            >>::encode_vec(trusted_header),
                    }
                    .into(),
                );
            }
            AnyHeader::Bankd(ref bankd_header) => {
                let trusted_height = bankd_header
                    .trusted_height
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("bankd header missing trusted_height"))?;
                let trusted_height = ibc_types::core::client::Height::new(
                    trusted_height.revision_number,
                    trusted_height.revision_height,
                )?;

                let trusted_consensus_state = state
                    .get_verified_consensus_state(&trusted_height, &self.client_id)
                    .await?;

                // Verify the header (BLS signature, trusting period, height monotonicity)
                let host_ts = HI::get_block_timestamp(&state).await?;
                let host_ts_secs = host_ts.unix_timestamp() as u64;

                let provider = BankdProvider;
                provider.verify_header(
                    &trusted_client_state,
                    &trusted_consensus_state,
                    &any_header,
                    host_ts_secs,
                )?;

                // Get latest client state and compute state transition (equivocation checks)
                let client_state = state
                    .get_client_state(&self.client_id)
                    .await
                    .context("unable to get client state")?;

                let (next_client_state, next_consensus_state) = state
                    .next_client_state(self.client_id.clone(), client_state, any_header.clone())
                    .await?;

                let header_height = bankd_header
                    .height
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("bankd header missing height"))?;
                let header_height = ibc_types::core::client::Height::new(
                    header_height.revision_number,
                    header_height.revision_height,
                )?;

                state.put_client(&self.client_id, next_client_state);
                state
                    .put_verified_consensus_state::<HI>(
                        header_height,
                        self.client_id.clone(),
                        next_consensus_state,
                    )
                    .await?;

                state.record(
                    UpdateClient {
                        client_id: self.client_id.clone(),
                        client_type: ibc_types::core::client::ClientType(
                            "08-commonware-bls".to_string(),
                        ),
                        consensus_height: header_height,
                        header: prost::Message::encode_to_vec(bankd_header),
                    }
                    .into(),
                );
            }
        }

        Ok(())
    }
}

async fn update_is_already_committed<S: StateRead>(
    state: S,
    msg: &MsgUpdateClient,
) -> anyhow::Result<bool> {
    let any_header = AnyHeader::try_from(msg.client_message.clone())?;
    let client_id = msg.client_id.clone();

    match any_header {
        AnyHeader::Tendermint(untrusted_header) => {
            // check if we already have a consensus state for this height, if we do, check that it is
            // the same as this update, if it is, return early.
            let height = untrusted_header.height();
            let untrusted_consensus_state = TendermintConsensusState::from(untrusted_header);
            if let Ok(stored_consensus_state) = state
                .get_verified_consensus_state(&height, &client_id)
                .await
            {
                match stored_consensus_state {
                    AnyConsensusState::Tendermint(stored_tm) => {
                        Ok(stored_tm == untrusted_consensus_state)
                    }
                    _ => Ok(false),
                }
            } else {
                Ok(false)
            }
        }
        AnyHeader::Bankd(ref bankd_header) => {
            let height_proto = bankd_header
                .height
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("bankd header missing height"))?;
            let height = ibc_types::core::client::Height::new(
                height_proto.revision_number,
                height_proto.revision_height,
            )?;
            if let Ok(stored_consensus_state) = state
                .get_verified_consensus_state(&height, &client_id)
                .await
            {
                match stored_consensus_state {
                    AnyConsensusState::Bankd(stored_bankd) => Ok(stored_bankd.root
                        == bankd_header.new_root
                        && stored_bankd.timestamp == bankd_header.timestamp),
                    _ => Ok(false),
                }
            } else {
                Ok(false)
            }
        }
    }
}

async fn client_is_not_expired<S: StateRead, HI: HostInterface>(
    state: S,
    client_id: &ClientId,
    client_state: &AnyClientState,
) -> anyhow::Result<()> {
    let latest_height = client_state
        .latest_height()
        .context("unable to get latest height from client state")?;

    let latest_consensus_state = state
        .get_verified_consensus_state(&latest_height, client_id)
        .await?;

    match (client_state, &latest_consensus_state) {
        (AnyClientState::Tendermint(tm_cs), AnyConsensusState::Tendermint(tm_cons)) => {
            let now = HI::get_block_timestamp(&state).await?;
            let time_elapsed = now.duration_since(tm_cons.timestamp)?;
            if tm_cs.expired(time_elapsed) {
                Err(anyhow::anyhow!("client is expired"))
            } else {
                Ok(())
            }
        }
        (AnyClientState::Bankd(_), AnyConsensusState::Bankd(_)) => {
            // Bankd clients don't expire from time alone
            Ok(())
        }
        _ => Err(anyhow::anyhow!(
            "mismatched client state and consensus state types"
        )),
    }
}

async fn client_is_present<S: StateRead>(
    state: S,
    msg: &MsgUpdateClient,
) -> anyhow::Result<AnyClientState> {
    state.get_client_type(&msg.client_id).await?;

    state.get_client_state(&msg.client_id).await
}

fn client_is_not_frozen(client: &AnyClientState) -> anyhow::Result<()> {
    if client.is_frozen() {
        Err(anyhow::anyhow!("client is frozen"))
    } else {
        Ok(())
    }
}

fn header_revision_matches_client_state(
    trusted_client_state: &TendermintClientState,
    untrusted_header: &TendermintHeader,
) -> anyhow::Result<()> {
    if untrusted_header.height().revision_number() != trusted_client_state.chain_id.version() {
        Err(anyhow::anyhow!(
            "client update revision number does not match client state"
        ))
    } else {
        Ok(())
    }
}

fn header_height_is_consistent(untrusted_header: &TendermintHeader) -> anyhow::Result<()> {
    if untrusted_header.height() <= untrusted_header.trusted_height {
        Err(anyhow::anyhow!(
            "client update height is not greater than trusted height"
        ))
    } else {
        Ok(())
    }
}

pub fn verify_header_validator_set<'h>(
    untrusted_header: &'h TendermintHeader,
    last_trusted_consensus_state: &TendermintConsensusState,
) -> anyhow::Result<&'h validator::Set> {
    if untrusted_header.trusted_validator_set.hash()
        != last_trusted_consensus_state.next_validators_hash
    {
        Err(anyhow::anyhow!(
            "client update validator set hash does not match trusted consensus state"
        ))
    } else {
        Ok(&untrusted_header.trusted_validator_set)
    }
}
