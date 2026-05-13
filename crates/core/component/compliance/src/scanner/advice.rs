use anyhow::{Context, Result};
use async_trait::async_trait;
use penumbra_sdk_asset::asset;
use penumbra_sdk_proto::core::component::compliance::v1::{
    query_service_client::QueryServiceClient as ComplianceQueryServiceClient,
    ComplianceAssetStatusRequest,
};
use tonic::transport::Channel;

use crate::AssetPolicy;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RingInfo {
    pub ring_id: String,
    pub ring_pk: decaf377::Element,
    pub policy_id: String,
    pub resource: String,
    pub permission: String,
}

impl RingInfo {
    pub fn from_asset_policy(policy: &AssetPolicy) -> Self {
        Self {
            ring_id: policy.ring.ring_id.clone(),
            ring_pk: policy.ring.ring_pk,
            policy_id: policy.ring.policy_id.clone(),
            resource: policy.ring.resource.clone(),
            permission: policy.ring.permission.clone(),
        }
    }
}

#[async_trait]
pub trait AuditAdviceProvider: Send + Sync {
    async fn asset_policy(&self, asset_id: asset::Id) -> Result<Option<AssetPolicy>>;
    async fn ring_info(&self, ring_id: &str) -> Result<Option<RingInfo>>;
    async fn known_label(&self, address_key: &[u8]) -> Result<Option<String>>;
}

#[derive(Clone, Debug, Default)]
pub struct NoopAuditAdviceProvider;

#[async_trait]
impl AuditAdviceProvider for NoopAuditAdviceProvider {
    async fn asset_policy(&self, _asset_id: asset::Id) -> Result<Option<AssetPolicy>> {
        Ok(None)
    }

    async fn ring_info(&self, _ring_id: &str) -> Result<Option<RingInfo>> {
        Ok(None)
    }

    async fn known_label(&self, _address_key: &[u8]) -> Result<Option<String>> {
        Ok(None)
    }
}

#[derive(Clone)]
pub struct RpcAuditAdviceProvider {
    channel: Channel,
}

impl RpcAuditAdviceProvider {
    pub fn new(channel: Channel) -> Self {
        Self { channel }
    }
}

#[async_trait]
impl AuditAdviceProvider for RpcAuditAdviceProvider {
    async fn asset_policy(&self, asset_id: asset::Id) -> Result<Option<AssetPolicy>> {
        let mut client = ComplianceQueryServiceClient::new(self.channel.clone());
        let response = client
            .compliance_asset_status(ComplianceAssetStatusRequest {
                asset_id: Some(asset_id.into()),
            })
            .await
            .context("failed to fetch compliance asset policy")?
            .into_inner();

        response
            .asset_policy
            .map(AssetPolicy::try_from)
            .transpose()
            .context("failed to decode compliance asset policy")
    }

    async fn ring_info(&self, ring_id: &str) -> Result<Option<RingInfo>> {
        // The current chain query is keyed by asset id. Ring lookup becomes useful
        // once audit enrichment needs ring-id keyed caches or Orbis metadata.
        let _ = ring_id;
        Ok(None)
    }

    async fn known_label(&self, _address_key: &[u8]) -> Result<Option<String>> {
        Ok(None)
    }
}
