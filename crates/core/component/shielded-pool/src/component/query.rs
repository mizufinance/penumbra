use super::AssetRegistryRead;
use cnidarium_component::QueryError as Status;
use shieldd_sdk_asset::asset;
use shieldd_sdk_proto::core::component::shielded_pool::v1::{
    AssetMetadataByIdRequest, AssetMetadataByIdResponse,
};
use tracing::instrument;

#[instrument(skip(state, request))]
pub async fn asset_metadata_by_id(
    state: &impl cnidarium::StateRead,
    request: AssetMetadataByIdRequest,
) -> Result<AssetMetadataByIdResponse, Status> {
    let id: asset::Id = request
        .asset_id
        .ok_or_else(|| Status::invalid_argument("missing asset_id"))?
        .try_into()
        .map_err(|e| Status::invalid_argument(format!("could not parse asset_id: {e}")))?;

    let denom_metadata = state.denom_metadata_by_asset(&id).await;

    let rsp = match denom_metadata {
        Some(denom_metadata) => {
            tracing::debug!(?id, ?denom_metadata, "found denom metadata");
            AssetMetadataByIdResponse {
                denom_metadata: Some(denom_metadata.into()),
            }
        }
        None => {
            tracing::debug!(?id, "unknown asset id");
            Default::default()
        }
    };

    Ok(rsp)
}
