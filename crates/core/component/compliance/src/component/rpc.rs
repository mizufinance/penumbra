use cnidarium::Storage;
use shieldd_sdk_asset::asset;
use shieldd_sdk_keys::Address;
use shieldd_sdk_proto::core::component::compliance::v1::{
    query_service_server::QueryService, ComplianceAnchorsRequest, ComplianceAnchorsResponse,
    ComplianceAssetStatusRequest, ComplianceAssetStatusResponse,
    ComplianceBatchMerkleProofsRequest, ComplianceBatchMerkleProofsResponse,
    ComplianceMerkleProofsRequest, ComplianceMerkleProofsResponse, ComplianceUserLeafRequest,
    ComplianceUserLeafResponse, IndexedLeafData, MerklePath, MerklePathLayer,
};
use tonic::Status;
use tracing::instrument;

use crate::registry::ComplianceRegistryRead;
use shieldd_sdk_tct::StateCommitment;

/// Maximum number of queries allowed in a batch compliance request.
/// This prevents resource exhaustion from excessively large batch requests.
const MAX_BATCH_SIZE: usize = 100;

/// Return the exact mutable roots required for new proofs.
async fn current_anchors<S: cnidarium::StateRead + ComplianceRegistryRead>(
    state: &S,
) -> Result<(StateCommitment, StateCommitment), Status> {
    let user_anchor = state
        .get_user_tree_root()
        .await
        .map_err(|e| Status::internal(format!("failed to get current user root: {e}")))?;
    let asset_anchor = state
        .get_asset_imt_root()
        .await
        .map_err(|e| Status::internal(format!("failed to get current asset root: {e}")))?;
    Ok((user_anchor, asset_anchor))
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

        let policy = state
            .get_asset_policy(asset_id)
            .await
            .map_err(|e| Status::internal(format!("failed to query asset policy: {e}")))?;
        let (dk_pub, daily_volume_limit) = match &policy {
            Some(policy) => (
                policy.params.dk_pub.vartime_compress().0.to_vec(),
                policy.params.daily_volume_limit.to_le_bytes().to_vec(),
            ),
            None => (vec![], vec![]),
        };

        // With IMT, all assets are always "queryable" - regulated via membership, unregulated via non-membership.
        let response = ComplianceAssetStatusResponse {
            asset_id: Some(asset_id.into()),
            is_registered: true, // With IMT, we can always answer the query
            is_regulated: proof_data.is_regulated,
            dk_pub,
            daily_volume_limit,
            asset_policy: policy.map(Into::into),
        };

        Ok(tonic::Response::new(response))
    }

    #[instrument(skip(self, _request))]
    async fn compliance_anchors(
        &self,
        _request: tonic::Request<ComplianceAnchorsRequest>,
    ) -> Result<tonic::Response<ComplianceAnchorsResponse>, Status> {
        let state = self.storage.latest_snapshot();

        let (user_tree_root, asset_imt_root) = current_anchors(&state).await?;

        tracing::debug!(
            ?user_tree_root,
            ?asset_imt_root,
            "returning current compliance roots"
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

        let (compliance_anchor, asset_anchor) = current_anchors(&state).await?;

        // Get asset proof data from IMT (handles both regulated and unregulated)
        let asset_proof_data = state
            .get_asset_proof_data(asset_id)
            .await
            .map_err(|e| Status::internal(format!("failed to get asset proof data: {e}")))?;

        // Load the compact user index once; it carries both position and leaf.
        let user_record = state
            .get_user_leaf_record(&address, asset_id)
            .await
            .map_err(|e| Status::internal(format!("failed to look up user record: {e}")))?;

        // Build the response based on what was found
        // Also fetch the user leaf if registered
        let (user_registered, compliance_path, compliance_position, compliance_leaf) =
            match user_record {
                Some(record) => {
                    let path = state
                        .get_user_auth_path(record.position)
                        .await
                        .map_err(|e| {
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

                    (
                        true,
                        Some(proto_path),
                        record.position,
                        Some(record.leaf.into()),
                    )
                }
                None => (false, None, 0, None),
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
        let asset_indexed_leaf: IndexedLeafData = asset_proof_data.indexed_leaf.clone().into();
        let asset_indexed_leaf = Some(asset_indexed_leaf);

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
            compliance_leaf,
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
                ComplianceUserLeafResponse {
                    is_registered: true,
                    leaf: Some(leaf.into()),
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

        let (compliance_anchor, asset_anchor) = current_anchors(&state).await?;

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

            // Load the compact user index once; it carries both position and leaf.
            let user_record = state
                .get_user_leaf_record(&address, asset_id)
                .await
                .map_err(|e| Status::internal(format!("failed to look up user record: {e}")))?;

            // Get asset proof data from IMT
            let asset_proof_data = state
                .get_asset_proof_data(asset_id)
                .await
                .map_err(|e| Status::internal(format!("failed to get asset proof data: {e}")))?;

            // Build the result for this query, including the user leaf if registered
            let (user_registered, compliance_path, compliance_position, compliance_leaf) =
                match user_record {
                    Some(record) => {
                        let path =
                            state
                                .get_user_auth_path(record.position)
                                .await
                                .map_err(|e| {
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

                        (
                            true,
                            Some(proto_path),
                            record.position,
                            Some(record.leaf.into()),
                        )
                    }
                    None => (false, None, 0, None),
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
            let asset_indexed_leaf: IndexedLeafData = asset_proof_data.indexed_leaf.clone().into();
            let asset_indexed_leaf = Some(asset_indexed_leaf);

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
                compliance_leaf,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::ComplianceRegistryWrite as _;
    use crate::structs::{AssetPolicy, ComplianceLeaf};
    use cnidarium::TempStorage;
    use decaf377::Fq;
    use shieldd_sdk_asset::asset;
    use shieldd_sdk_keys::Address;

    #[tokio::test]
    async fn current_anchors_returns_both_mutable_roots() {
        let storage = TempStorage::new().await.unwrap();
        let snapshot = storage.latest_snapshot();
        let mut state = cnidarium::StateDelta::new(snapshot);
        let stale_user_root = state.get_user_tree_root().await.unwrap();
        let stale_asset_root = state.get_asset_imt_root().await.unwrap();
        state
            .test_only_register_asset(
                asset::Id(Fq::from(55u64)),
                AssetPolicy::for_test(
                    decaf377::Element::GENERATOR,
                    u128::MAX,
                    decaf377::Element::GENERATOR,
                ),
                true,
            )
            .await
            .unwrap();
        let current_asset_root = state.get_asset_imt_root().await.unwrap();
        assert_ne!(stale_asset_root, current_asset_root);
        state
            .test_only_add_compliance_leaf(ComplianceLeaf::registered_for_test(
                Address::dummy(&mut rand::thread_rng()),
                asset::Id(Fq::from(55u64)),
            ))
            .await
            .unwrap();
        let current_user_root = state.get_user_tree_root().await.unwrap();
        assert_ne!(stale_user_root, current_user_root);

        let found = current_anchors(&state).await.unwrap();
        assert_eq!(found, (current_user_root, current_asset_root));
    }
}
