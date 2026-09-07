use std::{pin::Pin, sync::Arc};

use futures::{stream, Stream};
use shieldd_sdk_proto::execution_client::v1::{
    execution_client_service_server::ExecutionClientService, AppParametersRequest,
    AppParametersResponse, ApplyComplianceActionRequest, ApplyComplianceActionResponse,
    ArchivedNullifierProofRequest, ArchivedNullifierProofResponse, AssetMetadataByIdRequest,
    AssetMetadataByIdResponse, BeginBlockRequest, BeginBlockResponse, CheckTxRequest,
    CheckTxResponse, CommitRequest, CommitResponse, CompactBlockRangeRequest,
    CompactBlockRangeResponse, ComplianceAssetStatusRequest, ComplianceAssetStatusResponse,
    ComplianceBatchMerkleProofsRequest, ComplianceBatchMerkleProofsResponse,
    ComplianceUserLeafRequest, ComplianceUserLeafResponse, DeliverTxRequest, DeliverTxResponse,
    DepositRequest, DepositResponse, EndBlockRequest, EndBlockResponse, ExportGenesisRequest,
    ExportGenesisResponse, GetCommittedStateRequest, GetCommittedStateResponse, InitGenesisRequest,
    InitGenesisResponse, KeyValueRequest, KeyValueResponse, NullifierWindowRequest,
    NullifierWindowResponse, RollbackRequest, RollbackResponse, SeizeNoteRequest,
    SeizeNoteResponse,
};
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};

use crate::{ErrorKind, ExecutionService, ServiceError};

#[derive(Clone)]
pub struct GrpcExecutionClient {
    service: Arc<RwLock<ExecutionService>>,
}

impl GrpcExecutionClient {
    pub fn new(service: ExecutionService) -> Self {
        Self {
            service: Arc::new(RwLock::new(service)),
        }
    }

    pub async fn close(&self) -> std::result::Result<(), ServiceError> {
        self.service.write().await.close().await
    }
}

#[tonic::async_trait]
impl ExecutionClientService for GrpcExecutionClient {
    async fn init_genesis(
        &self,
        request: Request<InitGenesisRequest>,
    ) -> std::result::Result<Response<InitGenesisResponse>, Status> {
        self.service
            .write()
            .await
            .init_genesis(request.into_inner())
            .await
            .map(Response::new)
            .map_err(status)
    }

    async fn begin_block(
        &self,
        request: Request<BeginBlockRequest>,
    ) -> std::result::Result<Response<BeginBlockResponse>, Status> {
        self.service
            .write()
            .await
            .begin_block(request.into_inner())
            .await
            .map(Response::new)
            .map_err(status)
    }

    async fn deposit(
        &self,
        request: Request<DepositRequest>,
    ) -> std::result::Result<Response<DepositResponse>, Status> {
        self.service
            .write()
            .await
            .deposit(request.into_inner())
            .await
            .map(Response::new)
            .map_err(status)
    }

    async fn apply_compliance_action(
        &self,
        request: Request<ApplyComplianceActionRequest>,
    ) -> std::result::Result<Response<ApplyComplianceActionResponse>, Status> {
        self.service
            .write()
            .await
            .apply_compliance_action(request.into_inner())
            .await
            .map(Response::new)
            .map_err(status)
    }

    async fn seize_note(
        &self,
        request: Request<SeizeNoteRequest>,
    ) -> std::result::Result<Response<SeizeNoteResponse>, Status> {
        self.service
            .write()
            .await
            .seize_note(request.into_inner())
            .await
            .map(Response::new)
            .map_err(status)
    }

    async fn check_tx(
        &self,
        request: Request<CheckTxRequest>,
    ) -> std::result::Result<Response<CheckTxResponse>, Status> {
        self.service
            .read()
            .await
            .check_tx(request.into_inner())
            .await
            .map(Response::new)
            .map_err(status)
    }

    async fn deliver_tx(
        &self,
        request: Request<DeliverTxRequest>,
    ) -> std::result::Result<Response<DeliverTxResponse>, Status> {
        self.service
            .write()
            .await
            .deliver_tx(request.into_inner())
            .await
            .map(Response::new)
            .map_err(status)
    }

    async fn end_block(
        &self,
        request: Request<EndBlockRequest>,
    ) -> std::result::Result<Response<EndBlockResponse>, Status> {
        self.service
            .write()
            .await
            .end_block(request.into_inner())
            .await
            .map(Response::new)
            .map_err(status)
    }

    async fn commit(
        &self,
        request: Request<CommitRequest>,
    ) -> std::result::Result<Response<CommitResponse>, Status> {
        self.service
            .write()
            .await
            .commit(request.into_inner())
            .await
            .map(Response::new)
            .map_err(status)
    }

    async fn get_committed_state(
        &self,
        request: Request<GetCommittedStateRequest>,
    ) -> std::result::Result<Response<GetCommittedStateResponse>, Status> {
        self.service
            .read()
            .await
            .get_committed_state(request.into_inner())
            .await
            .map(Response::new)
            .map_err(status)
    }

    async fn rollback(
        &self,
        request: Request<RollbackRequest>,
    ) -> std::result::Result<Response<RollbackResponse>, Status> {
        self.service
            .write()
            .await
            .rollback(request.into_inner())
            .await
            .map(Response::new)
            .map_err(status)
    }

    async fn export_genesis(
        &self,
        request: Request<ExportGenesisRequest>,
    ) -> std::result::Result<Response<ExportGenesisResponse>, Status> {
        self.service
            .read()
            .await
            .export_genesis(request.into_inner())
            .await
            .map(Response::new)
            .map_err(status)
    }

    async fn archived_nullifier_proof(
        &self,
        request: Request<ArchivedNullifierProofRequest>,
    ) -> std::result::Result<Response<ArchivedNullifierProofResponse>, Status> {
        let request = request
            .into_inner()
            .request
            .ok_or_else(|| Status::invalid_argument("missing archived nullifier proof request"))?;
        self.service
            .read()
            .await
            .archived_nullifier_proof(request)
            .await
            .map(|response| {
                Response::new(ArchivedNullifierProofResponse {
                    response: Some(response),
                })
            })
            .map_err(status)
    }

    async fn nullifier_window(
        &self,
        request: Request<NullifierWindowRequest>,
    ) -> std::result::Result<Response<NullifierWindowResponse>, Status> {
        let request = request
            .into_inner()
            .request
            .ok_or_else(|| Status::invalid_argument("missing nullifier window request"))?;
        self.service
            .read()
            .await
            .nullifier_window(request)
            .await
            .map(|response| {
                Response::new(NullifierWindowResponse {
                    response: Some(response),
                })
            })
            .map_err(status)
    }

    async fn app_parameters(
        &self,
        request: Request<AppParametersRequest>,
    ) -> std::result::Result<Response<AppParametersResponse>, Status> {
        let request = request
            .into_inner()
            .request
            .ok_or_else(|| Status::invalid_argument("missing app parameters request"))?;
        self.service
            .read()
            .await
            .app_parameters(request)
            .await
            .map(|response| {
                Response::new(AppParametersResponse {
                    response: Some(response),
                })
            })
            .map_err(status)
    }

    async fn asset_metadata_by_id(
        &self,
        request: Request<AssetMetadataByIdRequest>,
    ) -> std::result::Result<Response<AssetMetadataByIdResponse>, Status> {
        let request = request
            .into_inner()
            .request
            .ok_or_else(|| Status::invalid_argument("missing asset metadata request"))?;
        self.service
            .read()
            .await
            .asset_metadata_by_id(request)
            .await
            .map(|response| {
                Response::new(AssetMetadataByIdResponse {
                    response: Some(response),
                })
            })
            .map_err(status)
    }

    async fn compliance_asset_status(
        &self,
        request: Request<ComplianceAssetStatusRequest>,
    ) -> std::result::Result<Response<ComplianceAssetStatusResponse>, Status> {
        let request = request
            .into_inner()
            .request
            .ok_or_else(|| Status::invalid_argument("missing compliance asset status request"))?;
        self.service
            .read()
            .await
            .compliance_asset_status(request)
            .await
            .map(|response| {
                Response::new(ComplianceAssetStatusResponse {
                    response: Some(response),
                })
            })
            .map_err(status)
    }

    async fn compliance_batch_merkle_proofs(
        &self,
        request: Request<ComplianceBatchMerkleProofsRequest>,
    ) -> std::result::Result<Response<ComplianceBatchMerkleProofsResponse>, Status> {
        let request = request.into_inner().request.ok_or_else(|| {
            Status::invalid_argument("missing compliance batch Merkle proofs request")
        })?;
        self.service
            .read()
            .await
            .compliance_batch_merkle_proofs(request)
            .await
            .map(|response| {
                Response::new(ComplianceBatchMerkleProofsResponse {
                    response: Some(response),
                })
            })
            .map_err(status)
    }

    async fn compliance_user_leaf(
        &self,
        request: Request<ComplianceUserLeafRequest>,
    ) -> std::result::Result<Response<ComplianceUserLeafResponse>, Status> {
        let request = request
            .into_inner()
            .request
            .ok_or_else(|| Status::invalid_argument("missing compliance user leaf request"))?;
        self.service
            .read()
            .await
            .compliance_user_leaf(request)
            .await
            .map(|response| {
                Response::new(ComplianceUserLeafResponse {
                    response: Some(response),
                })
            })
            .map_err(status)
    }

    async fn key_value(
        &self,
        request: Request<KeyValueRequest>,
    ) -> std::result::Result<Response<KeyValueResponse>, Status> {
        let request = request
            .into_inner()
            .request
            .ok_or_else(|| Status::invalid_argument("missing key-value request"))?;
        self.service
            .read()
            .await
            .key_value(request)
            .await
            .map(|response| {
                Response::new(KeyValueResponse {
                    response: Some(response),
                })
            })
            .map_err(status)
    }

    type CompactBlockRangeStream =
        Pin<Box<dyn Stream<Item = std::result::Result<CompactBlockRangeResponse, Status>> + Send>>;

    async fn compact_block_range(
        &self,
        request: Request<CompactBlockRangeRequest>,
    ) -> std::result::Result<Response<Self::CompactBlockRangeStream>, Status> {
        let request = request
            .into_inner()
            .request
            .ok_or_else(|| Status::invalid_argument("missing compact block range request"))?;
        let responses = self
            .service
            .read()
            .await
            .compact_block_range(request)
            .await
            .map_err(status)?;
        Ok(Response::new(Box::pin(stream::iter(
            responses.into_iter().map(|response| {
                Ok(CompactBlockRangeResponse {
                    response: Some(response),
                })
            }),
        ))))
    }
}

fn status(error: ServiceError) -> Status {
    match error.kind() {
        ErrorKind::InvalidArgument => Status::invalid_argument(error.to_string()),
        ErrorKind::FailedPrecondition => Status::failed_precondition(error.to_string()),
        ErrorKind::NotFound => Status::not_found(error.to_string()),
        ErrorKind::Internal => Status::internal(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use cnidarium::TempStorage;
    use futures::TryStreamExt as _;
    use shieldd_sdk_app::{
        genesis::{AppState, Content},
        SUBSTORE_PREFIXES,
    };
    use shieldd_sdk_asset::asset;
    use shieldd_sdk_keys::test_keys::ADDRESS_0;
    use shieldd_sdk_proto::cnidarium::v1::KeyValueRequest as ComponentKeyValueRequest;
    use shieldd_sdk_proto::core::app::v1::AppParametersRequest as ComponentAppParametersRequest;
    use shieldd_sdk_proto::core::component::{
        compact_block::v1::CompactBlockRangeRequest as ComponentCompactBlockRangeRequest,
        compliance::v1::{
            ComplianceAssetStatusRequest as ComponentComplianceAssetStatusRequest,
            ComplianceBatchMerkleProofsRequest as ComponentComplianceBatchMerkleProofsRequest,
            ComplianceBatchQuery, ComplianceUserLeafRequest as ComponentComplianceUserLeafRequest,
        },
        sct::v1::NullifierWindowRequest as ComponentNullifierWindowRequest,
        shielded_pool::v1::AssetMetadataByIdRequest as ComponentAssetMetadataByIdRequest,
    };
    use std::ops::Deref as _;

    async fn initialized_client() -> Result<(TempStorage, GrpcExecutionClient)> {
        let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
        let client = GrpcExecutionClient::new(ExecutionService::new(storage.deref().clone()));
        ExecutionClientService::init_genesis(
            &client,
            Request::new(InitGenesisRequest {
                genesis: Some(
                    AppState::Content(
                        Content::default().with_chain_id("shieldd-grpc-test".to_owned()),
                    )
                    .into(),
                ),
            }),
        )
        .await?;
        ExecutionClientService::commit(&client, Request::new(CommitRequest {})).await?;
        Ok((storage, client))
    }

    #[tokio::test]
    async fn grpc_execution_check_tx_rejects_invalid_transaction() -> Result<()> {
        let (_storage, client) = initialized_client().await?;

        let response = ExecutionClientService::check_tx(
            &client,
            Request::new(CheckTxRequest {
                tx: b"not a shieldd transaction".to_vec(),
            }),
        )
        .await?
        .into_inner();

        assert_eq!(response.code, 1);
        assert!(response.log.contains("decoding transaction"));
        Ok(())
    }

    #[tokio::test]
    async fn grpc_execution_deliver_tx_rejects_invalid_transaction() -> Result<()> {
        let (_storage, client) = initialized_client().await?;
        let mut begin_block = BeginBlockRequest {
            height: 1,
            time: Some(Default::default()),
        };
        begin_block
            .time
            .as_mut()
            .expect("test begin-block time")
            .seconds = 1_700_000_000;
        ExecutionClientService::begin_block(&client, Request::new(begin_block)).await?;

        let response = ExecutionClientService::deliver_tx(
            &client,
            Request::new(DeliverTxRequest {
                tx: b"not a shieldd transaction".to_vec(),
            }),
        )
        .await?
        .into_inner();

        assert_eq!(response.code, 1);
        assert!(response.log.contains("decoding transaction"));
        Ok(())
    }

    #[tokio::test]
    async fn grpc_execution_exposes_embedded_frontend_queries() -> Result<()> {
        let (_storage, client) = initialized_client().await?;
        let asset_id = asset::REGISTRY.parse_unit("shieldd").id();
        let asset_id_proto: shieldd_sdk_proto::core::asset::v1::AssetId = asset_id.into();
        let address: shieldd_sdk_proto::core::keys::v1::Address = ADDRESS_0.clone().into();

        let parameters = ExecutionClientService::app_parameters(
            &client,
            Request::new(AppParametersRequest {
                request: Some(ComponentAppParametersRequest {}),
            }),
        )
        .await?
        .into_inner()
        .response
        .expect("execution response contains app parameters response")
        .app_parameters
        .expect("app parameters response contains parameters");
        assert_eq!(parameters.chain_id, "shieldd-grpc-test");

        let nullifier_window = ExecutionClientService::nullifier_window(
            &client,
            Request::new(NullifierWindowRequest {
                request: Some(ComponentNullifierWindowRequest {}),
            }),
        )
        .await?
        .into_inner()
        .response
        .expect("execution response contains nullifier window response")
        .window
        .expect("initialized state contains a nullifier window");
        assert_eq!(
            nullifier_window.protocol_version,
            shieldd_sdk_sct::nullifier_generation::PROTOCOL_VERSION
        );
        assert_eq!(nullifier_window.current_generation, 0);

        let metadata = ExecutionClientService::asset_metadata_by_id(
            &client,
            Request::new(AssetMetadataByIdRequest {
                request: Some(ComponentAssetMetadataByIdRequest {
                    asset_id: Some(asset_id_proto.clone()),
                }),
            }),
        )
        .await?
        .into_inner()
        .response
        .expect("execution response contains asset metadata response")
        .denom_metadata
        .expect("known genesis asset metadata");
        assert_eq!(metadata.base, "ushieldd");

        let status = ExecutionClientService::compliance_asset_status(
            &client,
            Request::new(ComplianceAssetStatusRequest {
                request: Some(ComponentComplianceAssetStatusRequest {
                    asset_id: Some(asset_id_proto.clone()),
                }),
            }),
        )
        .await?
        .into_inner()
        .response
        .expect("execution response contains compliance asset status response");
        assert!(status.is_registered);
        assert!(!status.is_regulated);

        let user = ExecutionClientService::compliance_user_leaf(
            &client,
            Request::new(ComplianceUserLeafRequest {
                request: Some(ComponentComplianceUserLeafRequest {
                    address: Some(address.clone()),
                    asset_id: Some(asset_id_proto.clone()),
                }),
            }),
        )
        .await?
        .into_inner()
        .response
        .expect("execution response contains compliance user leaf response");
        assert!(!user.is_registered);
        assert!(user.leaf.is_none());

        let batch = ExecutionClientService::compliance_batch_merkle_proofs(
            &client,
            Request::new(ComplianceBatchMerkleProofsRequest {
                request: Some(ComponentComplianceBatchMerkleProofsRequest {
                    queries: vec![ComplianceBatchQuery {
                        address: Some(address),
                        asset_id: Some(asset_id_proto),
                    }],
                }),
            }),
        )
        .await?
        .into_inner()
        .response
        .expect("execution response contains compliance batch response");
        assert_eq!(batch.results.len(), 1);
        assert!(batch.results[0].asset_registered);
        assert!(!batch.results[0].user_registered);

        let key = ExecutionClientService::key_value(
            &client,
            Request::new(KeyValueRequest {
                request: Some(ComponentKeyValueRequest {
                    key: shieldd_sdk_sct::state_key::tree::anchor_by_height(0),
                    proof: false,
                }),
            }),
        )
        .await?
        .into_inner()
        .response
        .expect("execution response contains key-value response");
        assert!(key.value.is_some());
        assert!(key.proof.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn grpc_execution_streams_bounded_compact_block_ranges() -> Result<()> {
        let (_storage, client) = initialized_client().await?;
        for height in 1..=2 {
            let mut begin = BeginBlockRequest {
                height,
                time: Some(Default::default()),
            };
            begin.time.as_mut().expect("test begin-block time").seconds = 1_700_000_000 + height;
            ExecutionClientService::begin_block(&client, Request::new(begin)).await?;
            ExecutionClientService::end_block(&client, Request::new(EndBlockRequest { height }))
                .await?;
            ExecutionClientService::commit(&client, Request::new(CommitRequest {})).await?;
        }

        let responses = ExecutionClientService::compact_block_range(
            &client,
            Request::new(CompactBlockRangeRequest {
                request: Some(ComponentCompactBlockRangeRequest {
                    start_height: 0,
                    end_height: 2,
                    keep_alive: false,
                }),
            }),
        )
        .await?
        .into_inner()
        .try_collect::<Vec<_>>()
        .await?;
        let heights = responses
            .into_iter()
            .map(|response| {
                response
                    .response
                    .expect("execution stream contains compact block response")
                    .compact_block
                    .expect("range response contains a compact block")
                    .height
            })
            .collect::<Vec<_>>();
        assert_eq!(heights, vec![0, 1, 2]);
        Ok(())
    }
}
