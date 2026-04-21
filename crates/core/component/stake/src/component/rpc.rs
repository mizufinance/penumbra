use std::pin::Pin;

use cnidarium::Storage;
use futures::{StreamExt, TryStreamExt};
use penumbra_sdk_proto::{
    core::component::validator::v1::{
        self as pb_validator, query_service_server::QueryService as ValidatorQueryService,
    },
    DomainType,
};
use tap::{TapFallible, TapOptional};
use tonic::Status;
use tracing::{error_span, instrument, Instrument, Span};

use super::{validator_handler::ValidatorDataRead, ConsensusIndexRead};
use crate::validator::{Info, State};

// TODO: Hide this and only expose a Router?
pub struct Server {
    storage: Storage,
}

impl Server {
    pub fn new(storage: Storage) -> Self {
        Self { storage }
    }

    async fn get_validator_info_inner(
        &self,
        identity_key: crate::IdentityKey,
    ) -> Result<Info, tonic::Status> {
        let state = self.storage.latest_snapshot();
        state
            .get_validator_info(&identity_key)
            .await
            .tap_err(|error| tracing::error!(?error, %identity_key, "failed to get validator info"))
            .map_err(|_| Status::invalid_argument("failed to get validator info"))?
            .tap_none(|| tracing::debug!(%identity_key, "validator info was not found"))
            .ok_or_else(|| Status::not_found("validator info was not found"))
    }

    fn validator_info_stream_inner(
        &self,
        show_inactive: bool,
    ) -> Result<
        Pin<Box<dyn futures::Stream<Item = Result<Info, tonic::Status>> + Send>>,
        tonic::Status,
    > {
        use futures::TryStreamExt;

        let snapshot = self.storage.latest_snapshot();

        let filter_inactive = move |info: &Info| {
            let should = match info.status.state {
                State::Active => true,
                _ if show_inactive => true,
                _ => false,
            };
            futures::future::ready(should)
        };

        let make_span = |identity_key| -> Span {
            let span = error_span!("fetching validator information", %identity_key);
            let current = Span::current();
            span.follows_from(current);
            span
        };

        let consensus_set = snapshot
            .consensus_set_stream()
            .map_err(|e| format!("error getting consensus set: {e}"))
            .map_err(Status::unavailable)?;

        let validators = async_stream::try_stream! {
            for await identity_key in consensus_set {
                let identity_key = identity_key?;
                let span = make_span(identity_key);
                yield snapshot
                    .get_validator_info(&identity_key)
                    .instrument(span)
                    .await?
                    .expect("known validator must be present");
            }
        };

        Ok(validators
            .try_filter(filter_inactive)
            .map_err(|e: anyhow::Error| format!("error getting validator info: {e}"))
            .map_err(Status::unavailable)
            .into_stream()
            .boxed())
    }

    async fn validator_status_inner(
        &self,
        identity_key: crate::IdentityKey,
    ) -> Result<crate::validator::Status, tonic::Status> {
        let state = self.storage.latest_snapshot();
        state
            .get_validator_status(&identity_key)
            .await
            .map_err(|e| Status::unavailable(format!("error getting validator status: {e}")))?
            .ok_or_else(|| Status::not_found("validator not found"))
    }

    async fn validator_uptime_inner(
        &self,
        identity_key: crate::IdentityKey,
    ) -> Result<crate::Uptime, tonic::Status> {
        let state = self.storage.latest_snapshot();
        state
            .get_validator_uptime(&identity_key)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("validator uptime not found"))
    }
}

#[tonic::async_trait]
impl ValidatorQueryService for Server {
    #[instrument(skip(self, request))]
    async fn get_validator_info(
        &self,
        request: tonic::Request<pb_validator::GetValidatorInfoRequest>,
    ) -> Result<tonic::Response<pb_validator::GetValidatorInfoResponse>, tonic::Status> {
        let identity_key = request
            .into_inner()
            .identity_key
            .ok_or_else(|| Status::invalid_argument("an identity key must be provided"))?
            .try_into()
            .tap_err(|error| tracing::debug!(?error, "request contained an invalid identity key"))
            .map_err(|_| Status::invalid_argument("invalid identity key"))?;

        Ok(tonic::Response::new(
            pb_validator::GetValidatorInfoResponse {
                validator_info: Some(
                    self.get_validator_info_inner(identity_key)
                        .await?
                        .to_proto(),
                ),
            },
        ))
    }

    type ValidatorInfoStream = Pin<
        Box<
            dyn futures::Stream<Item = Result<pb_validator::ValidatorInfoResponse, tonic::Status>>
                + Send,
        >,
    >;

    #[instrument(skip(self, request), fields(show_inactive = request.get_ref().show_inactive))]
    async fn validator_info(
        &self,
        request: tonic::Request<pb_validator::ValidatorInfoRequest>,
    ) -> Result<tonic::Response<Self::ValidatorInfoStream>, Status> {
        let pb_validator::ValidatorInfoRequest { show_inactive } = request.into_inner();
        let stream = self
            .validator_info_stream_inner(show_inactive)?
            .map_ok(|info| pb_validator::ValidatorInfoResponse {
                validator_info: Some(info.to_proto()),
            })
            .boxed();

        Ok(tonic::Response::new(stream))
    }

    #[instrument(skip(self, request))]
    async fn validator_status(
        &self,
        request: tonic::Request<pb_validator::ValidatorStatusRequest>,
    ) -> Result<tonic::Response<pb_validator::ValidatorStatusResponse>, Status> {
        let identity_key = request
            .into_inner()
            .identity_key
            .ok_or_else(|| Status::invalid_argument("missing identity key"))?
            .try_into()
            .map_err(|_| Status::invalid_argument("invalid identity key"))?;

        Ok(tonic::Response::new(
            pb_validator::ValidatorStatusResponse {
                status: Some(self.validator_status_inner(identity_key).await?.into()),
            },
        ))
    }

    #[instrument(skip(self, request))]
    async fn validator_uptime(
        &self,
        request: tonic::Request<pb_validator::ValidatorUptimeRequest>,
    ) -> Result<tonic::Response<pb_validator::ValidatorUptimeResponse>, Status> {
        let identity_key = request
            .into_inner()
            .identity_key
            .ok_or_else(|| tonic::Status::invalid_argument("empty message"))?
            .try_into()
            .map_err(|_| tonic::Status::invalid_argument("invalid identity key"))?;

        Ok(tonic::Response::new(
            pb_validator::ValidatorUptimeResponse {
                uptime: Some(self.validator_uptime_inner(identity_key).await?.into()),
            },
        ))
    }
}
