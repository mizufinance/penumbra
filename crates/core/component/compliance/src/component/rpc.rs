use cnidarium::Storage;
use penumbra_sdk_asset::asset;
use penumbra_sdk_keys::Address;
use penumbra_sdk_proto::core::component::compliance::v1::{
    query_service_server::QueryService, ComplianceAnchorsRequest, ComplianceAnchorsResponse,
    ComplianceAssetStatusRequest, ComplianceAssetStatusResponse,
    ComplianceBatchMerkleProofsRequest, ComplianceBatchMerkleProofsResponse,
    ComplianceMerkleProofsRequest, ComplianceMerkleProofsResponse, ComplianceUserLeafRequest,
    ComplianceUserLeafResponse, IndexedLeafData, MerklePath, MerklePathLayer,
};
use penumbra_sdk_sct::component::clock::EpochRead;
use tonic::Status;
use tracing::instrument;

use crate::registry::ComplianceRegistryRead;
use penumbra_sdk_tct::StateCommitment;

/// Maximum number of queries allowed in a batch compliance request.
/// This prevents resource exhaustion from excessively large batch requests.
const MAX_BATCH_SIZE: usize = 100;

/// Maximum number of blocks to search backwards for a recorded anchor.
const MAX_ANCHOR_SEARCH_DEPTH: u64 = 10;

/// Find the most recent recorded compliance anchors by searching backwards from current height.
///
/// This handles the case where the RPC is called mid-block before anchors are recorded.
async fn find_most_recent_anchors<S: cnidarium::StateRead + ComplianceRegistryRead>(
    state: &S,
    current_height: u64,
) -> Result<(StateCommitment, StateCommitment), Status> {
    // Search backwards from current height to find recorded anchors
    let search_start = current_height;
    let search_end = current_height.saturating_sub(MAX_ANCHOR_SEARCH_DEPTH);

    for height in (search_end..=search_start).rev() {
        let user_anchor = state.get_user_anchor_by_height(height).await.map_err(|e| {
            Status::internal(format!("failed to get user anchor at height {height}: {e}"))
        })?;

        let asset_anchor = state
            .get_asset_anchor_by_height(height)
            .await
            .map_err(|e| {
                Status::internal(format!(
                    "failed to get asset anchor at height {height}: {e}"
                ))
            })?;

        if let (Some(user), Some(asset)) = (user_anchor, asset_anchor) {
            tracing::debug!(height, "found recorded anchors");
            return Ok((user, asset));
        }
    }

    Err(Status::not_found(format!(
        "no compliance anchors found in last {} blocks (current height: {})",
        MAX_ANCHOR_SEARCH_DEPTH, current_height
    )))
}

/// gRPC server for compliance registry queries.
pub struct Server {
    storage: Storage,
}

impl Server {
    pub fn new(storage: Storage) -> Self {
        Self { storage }
    }
}

#[tonic::async_trait]
impl QueryService for Server {
    #[instrument(skip(self, request))]
    async fn compliance_asset_status(
        &self,
        request: tonic::Request<ComplianceAssetStatusRequest>,
    ) -> Result<tonic::Response<ComplianceAssetStatusResponse>, Status> {
        let state = self.storage.latest_snapshot();

        let request = request.into_inner();
        let asset_id: asset::Id = request
            .asset_id
            .ok_or_else(|| Status::invalid_argument("missing asset_id"))?
            .try_into()
            .map_err(|e| Status::invalid_argument(format!("could not parse asset_id: {e}")))?;

        // Query the IMT for the asset's regulation status
        let proof_data = state
            .get_asset_proof_data(asset_id)
            .await
            .map_err(|e| Status::internal(format!("failed to query asset status: {e}")))?;

        tracing::debug!(
            ?asset_id,
            is_regulated = proof_data.is_regulated,
            "queried asset regulation status"
        );

        // With IMT, all assets are always "queryable" - regulated via membership, unregulated via non-membership
        let response = ComplianceAssetStatusResponse {
            asset_id: Some(asset_id.into()),
            is_registered: true, // With IMT, we can always answer the query
            is_regulated: proof_data.is_regulated,
        };

        Ok(tonic::Response::new(response))
    }

    #[instrument(skip(self, _request))]
    async fn compliance_anchors(
        &self,
        _request: tonic::Request<ComplianceAnchorsRequest>,
    ) -> Result<tonic::Response<ComplianceAnchorsResponse>, Status> {
        let state = self.storage.latest_snapshot();

        // Get the current block height to look up recorded anchors
        let current_height = state
            .get_block_height()
            .await
            .map_err(|e| Status::internal(format!("failed to get block height: {e}")))?;

        // Return the RECORDED anchors (not current tree roots) to ensure
        // they will be accepted by validate_compliance_anchors().
        // Search backwards from current height to find the most recent recorded anchor,
        // since the current height may not have been finalized yet.
        let (user_tree_root, asset_imt_root) =
            find_most_recent_anchors(&state, current_height).await?;

        tracing::debug!(
            current_height,
            ?user_tree_root,
            ?asset_imt_root,
            "returning recorded compliance anchors"
        );

        let response = ComplianceAnchorsResponse {
            user_tree_root: user_tree_root.0.to_bytes().to_vec(),
            asset_tree_root: asset_imt_root.0.to_bytes().to_vec(),
        };

        Ok(tonic::Response::new(response))
    }

    #[instrument(skip(self, request))]
    async fn compliance_merkle_proofs(
        &self,
        request: tonic::Request<ComplianceMerkleProofsRequest>,
    ) -> Result<tonic::Response<ComplianceMerkleProofsResponse>, Status> {
        let state = self.storage.latest_snapshot();
        let request = request.into_inner();

        // Parse address (Address)
        let address: Address = request
            .address
            .ok_or_else(|| Status::invalid_argument("missing address"))?
            .try_into()
            .map_err(|e| Status::invalid_argument(format!("could not parse address: {e}")))?;

        // Parse asset_id
        let asset_id: asset::Id = request
            .asset_id
            .ok_or_else(|| Status::invalid_argument("missing asset_id"))?
            .try_into()
            .map_err(|e| Status::invalid_argument(format!("could not parse asset_id: {e}")))?;

        // Get recorded anchors (not current tree roots) for validation compatibility.
        // Search backwards from current height to find the most recent recorded anchor.
        let current_height = state
            .get_block_height()
            .await
            .map_err(|e| Status::internal(format!("failed to get block height: {e}")))?;

        let (compliance_anchor, asset_anchor) =
            find_most_recent_anchors(&state, current_height).await?;

        // Get asset proof data from IMT (handles both regulated and unregulated)
        let asset_proof_data = state
            .get_asset_proof_data(asset_id)
            .await
            .map_err(|e| Status::internal(format!("failed to get asset proof data: {e}")))?;

        // Look up user's position in compliance tree
        let user_position = state
            .get_user_leaf_position(&address, asset_id)
            .await
            .map_err(|e| Status::internal(format!("failed to look up user position: {e}")))?;

        // Build the response based on what was found
        let (user_registered, compliance_path, compliance_position) = match user_position {
            Some(pos) => {
                let path = state
                    .get_user_auth_path(pos)
                    .await
                    .map_err(|e| Status::internal(format!("failed to get user auth path: {e}")))?;

                let proto_path = MerklePath {
                    layers: path
                        .into_iter()
                        .map(|siblings| MerklePathLayer {
                            siblings: siblings.iter().map(|c| c.0.to_bytes().to_vec()).collect(),
                        })
                        .collect(),
                };

                (true, Some(proto_path), pos)
            }
            None => (false, None, 0),
        };

        // Build asset proof response from IMT proof data
        // auth_path.layers[].siblings are already Vec<Vec<u8>>
        let asset_path = Some(MerklePath {
            layers: asset_proof_data
                .auth_path
                .layers
                .iter()
                .map(|layer| MerklePathLayer {
                    siblings: layer.siblings.clone(),
                })
                .collect(),
        });
        let is_regulated = asset_proof_data.is_regulated;
        let asset_pos = asset_proof_data.position;
        // With IMT, asset is always "registered" - either as regulated (membership) or unregulated (non-membership)
        let asset_registered = true;

        tracing::debug!(
            ?address,
            ?asset_id,
            user_registered,
            asset_registered,
            is_regulated,
            compliance_position,
            asset_position = asset_pos,
            "returning compliance merkle proofs"
        );

        // Convert indexed_leaf to proto
        let asset_indexed_leaf = Some(IndexedLeafData {
            value: asset_proof_data.indexed_leaf.value.to_bytes().to_vec(),
            next_index: asset_proof_data.indexed_leaf.next_index,
            next_value: asset_proof_data.indexed_leaf.next_value.to_bytes().to_vec(),
        });

        let response = ComplianceMerkleProofsResponse {
            user_registered,
            asset_registered,
            is_regulated,
            compliance_path,
            compliance_position,
            asset_path,
            asset_position: asset_pos,
            compliance_anchor: compliance_anchor.0.to_bytes().to_vec(),
            asset_anchor: asset_anchor.0.to_bytes().to_vec(),
            asset_indexed_leaf,
        };

        Ok(tonic::Response::new(response))
    }

    #[instrument(skip(self, request))]
    async fn compliance_user_leaf(
        &self,
        request: tonic::Request<ComplianceUserLeafRequest>,
    ) -> Result<tonic::Response<ComplianceUserLeafResponse>, Status> {
        let state = self.storage.latest_snapshot();
        let request = request.into_inner();

        // Parse address (Address)
        let address: Address = request
            .address
            .ok_or_else(|| Status::invalid_argument("missing address"))?
            .try_into()
            .map_err(|e| Status::invalid_argument(format!("could not parse address: {e}")))?;

        // Parse asset_id
        let asset_id: asset::Id = request
            .asset_id
            .ok_or_else(|| Status::invalid_argument("missing asset_id"))?
            .try_into()
            .map_err(|e| Status::invalid_argument(format!("could not parse asset_id: {e}")))?;

        // Look up the user's full leaf from state
        let leaf_opt = state
            .get_user_leaf(&address, asset_id)
            .await
            .map_err(|e| Status::internal(format!("failed to get user leaf: {e}")))?;

        let response = match leaf_opt {
            Some(leaf) => {
                tracing::debug!(?address, ?asset_id, "found user leaf");
                use penumbra_sdk_proto::core::component::compliance::v1 as pb;
                ComplianceUserLeafResponse {
                    is_registered: true,
                    leaf: Some(pb::ComplianceLeaf {
                        address: Some(leaf.address.into()),
                        key: Some(pb::ComplianceViewingKey {
                            inner: leaf.key.0.vartime_compress().0.to_vec(),
                        }),
                        asset_id: Some(leaf.asset_id.into()),
                    }),
                }
            }
            None => {
                tracing::debug!(?address, ?asset_id, "user not registered");
                ComplianceUserLeafResponse {
                    is_registered: false,
                    leaf: None,
                }
            }
        };

        Ok(tonic::Response::new(response))
    }

    #[instrument(skip(self, request))]
    async fn compliance_batch_merkle_proofs(
        &self,
        request: tonic::Request<ComplianceBatchMerkleProofsRequest>,
    ) -> Result<tonic::Response<ComplianceBatchMerkleProofsResponse>, Status> {
        let state = self.storage.latest_snapshot();
        let request = request.into_inner();

        // Validate batch size to prevent resource exhaustion
        let query_count = request.queries.len();
        if query_count > MAX_BATCH_SIZE {
            return Err(Status::invalid_argument(format!(
                "batch size {} exceeds maximum allowed {}",
                query_count, MAX_BATCH_SIZE
            )));
        }

        // Get recorded anchors (not current tree roots) for validation compatibility.
        // Search backwards from current height to find the most recent recorded anchor.
        let current_height = state
            .get_block_height()
            .await
            .map_err(|e| Status::internal(format!("failed to get block height: {e}")))?;

        let (compliance_anchor, asset_anchor) =
            find_most_recent_anchors(&state, current_height).await?;

        // Process each query
        let mut results = Vec::with_capacity(query_count);
        for query in request.queries {
            // Parse address
            let address: Address = query
                .address
                .ok_or_else(|| Status::invalid_argument("missing address in query"))?
                .try_into()
                .map_err(|e| Status::invalid_argument(format!("could not parse address: {e}")))?;

            // Parse asset_id
            let asset_id: asset::Id = query
                .asset_id
                .ok_or_else(|| Status::invalid_argument("missing asset_id in query"))?
                .try_into()
                .map_err(|e| Status::invalid_argument(format!("could not parse asset_id: {e}")))?;

            // Look up user's position in compliance tree
            let user_position = state
                .get_user_leaf_position(&address, asset_id)
                .await
                .map_err(|e| Status::internal(format!("failed to look up user position: {e}")))?;

            // Get asset proof data from IMT
            let asset_proof_data = state
                .get_asset_proof_data(asset_id)
                .await
                .map_err(|e| Status::internal(format!("failed to get asset proof data: {e}")))?;

            // Build the result for this query
            let (user_registered, compliance_path, compliance_position) = match user_position {
                Some(pos) => {
                    let path = state.get_user_auth_path(pos).await.map_err(|e| {
                        Status::internal(format!("failed to get user auth path: {e}"))
                    })?;

                    let proto_path = MerklePath {
                        layers: path
                            .into_iter()
                            .map(|siblings| MerklePathLayer {
                                siblings: siblings
                                    .iter()
                                    .map(|c| c.0.to_bytes().to_vec())
                                    .collect(),
                            })
                            .collect(),
                    };

                    (true, Some(proto_path), pos)
                }
                None => (false, None, 0),
            };

            // Build asset proof from IMT proof data
            // auth_path.layers[].siblings are already Vec<Vec<u8>>
            let asset_path = Some(MerklePath {
                layers: asset_proof_data
                    .auth_path
                    .layers
                    .iter()
                    .map(|layer| MerklePathLayer {
                        siblings: layer.siblings.clone(),
                    })
                    .collect(),
            });

            // Convert indexed_leaf to proto
            let asset_indexed_leaf = Some(IndexedLeafData {
                value: asset_proof_data.indexed_leaf.value.to_bytes().to_vec(),
                next_index: asset_proof_data.indexed_leaf.next_index,
                next_value: asset_proof_data.indexed_leaf.next_value.to_bytes().to_vec(),
            });

            results.push(ComplianceMerkleProofsResponse {
                user_registered,
                asset_registered: true, // With IMT, always have proof data
                is_regulated: asset_proof_data.is_regulated,
                compliance_path,
                compliance_position,
                asset_path,
                asset_position: asset_proof_data.position,
                // Individual results don't need anchors - they're in the batch response
                compliance_anchor: vec![],
                asset_anchor: vec![],
                asset_indexed_leaf,
            });
        }

        tracing::debug!(query_count, "returning batch compliance merkle proofs");

        let response = ComplianceBatchMerkleProofsResponse {
            compliance_anchor: compliance_anchor.0.to_bytes().to_vec(),
            asset_anchor: asset_anchor.0.to_bytes().to_vec(),
            results,
        };

        Ok(tonic::Response::new(response))
    }
}
