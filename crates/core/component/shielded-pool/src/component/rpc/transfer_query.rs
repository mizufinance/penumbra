use async_trait::async_trait;
use futures::StreamExt;
use ibc_proto::ibc::applications::transfer::v1::query_server::Query as TransferQuery;
use ibc_proto::ibc::apps::transfer::v1::{
    DenomTrace, QueryDenomHashRequest, QueryDenomHashResponse, QueryDenomTraceRequest,
    QueryDenomTraceResponse, QueryDenomTracesRequest, QueryDenomTracesResponse,
    QueryEscrowAddressRequest, QueryEscrowAddressResponse, QueryParamsRequest, QueryParamsResponse,
    QueryTotalEscrowForDenomRequest, QueryTotalEscrowForDenomResponse,
};
use penumbra_sdk_asset::asset::Metadata;
use penumbra_sdk_proto::StateReadProto as _;

use crate::state_key;

use super::Server;

fn denom_trace_item(
    item: anyhow::Result<(String, Metadata)>,
) -> Option<anyhow::Result<DenomTrace>> {
    let (_key, denom) = match item {
        Ok(item) => item,
        Err(error) => return Some(Err(error.context("bad denom in state"))),
    };

    denom
        .best_effort_ibc_transfer_parse()
        .map(|(path, base_denom)| Ok(DenomTrace { path, base_denom }))
}

#[async_trait]
impl TransferQuery for Server {
    async fn total_escrow_for_denom(
        &self,
        _: tonic::Request<QueryTotalEscrowForDenomRequest>,
    ) -> std::result::Result<tonic::Response<QueryTotalEscrowForDenomResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("not implemented"))
    }

    async fn escrow_address(
        &self,
        _: tonic::Request<QueryEscrowAddressRequest>,
    ) -> std::result::Result<tonic::Response<QueryEscrowAddressResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("not implemented"))
    }

    async fn denom_hash(
        &self,
        _: tonic::Request<QueryDenomHashRequest>,
    ) -> std::result::Result<tonic::Response<QueryDenomHashResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("not implemented"))
    }

    async fn params(
        &self,
        _: tonic::Request<QueryParamsRequest>,
    ) -> std::result::Result<tonic::Response<QueryParamsResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("not implemented"))
    }

    async fn denom_trace(
        &self,
        _: tonic::Request<QueryDenomTraceRequest>,
    ) -> std::result::Result<tonic::Response<QueryDenomTraceResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("not implemented"))
    }

    async fn denom_traces(
        &self,
        _: tonic::Request<QueryDenomTracesRequest>,
    ) -> std::result::Result<tonic::Response<QueryDenomTracesResponse>, tonic::Status> {
        // TODO: Currently pagination is ignored and all denom traces are returned at once.
        // Since this API isn't streaming, this may be something useful to implement later.
        let snapshot = self.storage.latest_snapshot();
        let s = snapshot.prefix(state_key::denom_metadata_by_asset::prefix());
        let denom_traces = s
            .filter_map(
                move |i: anyhow::Result<(String, Metadata)>| async move { denom_trace_item(i) },
            )
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<anyhow::Result<Vec<_>>>()
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(QueryDenomTracesResponse {
            denom_traces,
            // pagination disabled for now
            pagination: None,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_query_stream_errors_propagate() {
        let err = denom_trace_item(Err(anyhow::anyhow!("storage failed")))
            .expect("stream item should be retained")
            .expect_err("storage error should propagate");
        assert!(
            err.to_string().contains("bad denom in state"),
            "unexpected error: {err:#}"
        );
    }
}
