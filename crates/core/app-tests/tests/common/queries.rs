//! Bankd query routes backed exclusively by the execution host's committed storage.
use shieldd_sdk_app::app::StateReadExt;
use shieldd_sdk_proto::core::{app::v1 as pb, component::compliance::v1 as cpb};
use shieldd_sdk_sct::component::clock::EpochRead;
use std::marker::PhantomData;
use tonic::codegen::*;

#[derive(Clone)]
pub struct CommittedQueries(pub cnidarium::Storage);

struct Query<F, R>(F, PhantomData<R>);
impl<T, R, F, Fut> tonic::server::UnaryService<T> for Query<F, R>
where
    F: Fn(T) -> Fut,
    Fut: std::future::Future<Output = Result<R, tonic::Status>> + Send + 'static,
    R: Send + 'static,
{
    type Response = R;
    type Future = BoxFuture<tonic::Response<R>, tonic::Status>;
    fn call(&mut self, request: tonic::Request<T>) -> Self::Future {
        let future = (self.0)(request.into_inner());
        Box::pin(async move { future.await.map(tonic::Response::new) })
    }
}
impl tonic::server::NamedService for CommittedQueries {
    const NAME: &'static str = "mizufinance.shieldd.v1.Query";
}
impl<B> Service<http::Request<B>> for CommittedQueries
where
    B: Body + Send + 'static,
    B::Error: Into<StdError> + Send + 'static,
{
    type Response = http::Response<tonic::body::BoxBody>;
    type Error = std::convert::Infallible;
    type Future = BoxFuture<Self::Response, Self::Error>;
    fn poll_ready(
        &mut self,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }
    fn call(&mut self, request: http::Request<B>) -> Self::Future {
        let snapshot = self.0.latest_snapshot();
        Box::pin(async move {
            macro_rules! respond {
                ($input:ty, $output:ty, $handler:expr) => {
                    tonic::server::Grpc::new(tonic::codec::ProstCodec::<$output, $input>::default())
                        .unary(Query($handler, PhantomData), request)
                        .await
                };
            }
            let response = match request.uri().path() {
                "/mizufinance.shieldd.v1.Query/AppParameters" => respond!(
                    pb::AppParametersRequest,
                    pb::AppParametersResponse,
                    move |_: pb::AppParametersRequest| {
                        let state = snapshot.clone();
                        async move {
                            let parameters = state
                                .get_app_params()
                                .await
                                .map_err(|e| tonic::Status::internal(e.to_string()))?;
                            Ok(pb::AppParametersResponse {
                                app_parameters: Some(parameters.into()),
                            })
                        }
                    }
                ),
                "/mizufinance.shieldd.v1.Query/TransactionsByHeight" => respond!(
                    pb::TransactionsByHeightRequest,
                    pb::TransactionsByHeightResponse,
                    move |input: pb::TransactionsByHeightRequest| {
                        let state = snapshot.clone();
                        async move {
                            let last = state
                                .get_block_height()
                                .await
                                .map_err(|e| tonic::Status::internal(e.to_string()))?;
                            if input.block_height > last {
                                return Err(tonic::Status::not_found("uncommitted block"));
                            }
                            state
                                .transactions_by_height(input.block_height)
                                .await
                                .map_err(|e| tonic::Status::internal(e.to_string()))
                        }
                    }
                ),
                "/mizufinance.shieldd.v1.Query/ComplianceAssetStatus" => respond!(
                    cpb::ComplianceAssetStatusRequest,
                    cpb::ComplianceAssetStatusResponse,
                    move |input: cpb::ComplianceAssetStatusRequest| {
                        let state = snapshot.clone();
                        async move {
                            shieldd_sdk_compliance::component::query::compliance_asset_status(
                                &state, input,
                            )
                            .await
                            .map_err(|e| tonic::Status::internal(e.to_string()))
                        }
                    }
                ),
                _ => http::Response::builder()
                    .status(200)
                    .header("grpc-status", "12")
                    .header("content-type", "application/grpc")
                    .body(tonic::body::empty_body())
                    .unwrap(),
            };
            Ok(response)
        })
    }
}
