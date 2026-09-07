//! The storage and RPC reads required to complete a wallet plan.
use crate::{SpendableNoteRecord, ViewClient};
use anyhow::Result;
use async_trait::async_trait;
use shieldd_sdk_compliance::BatchComplianceData;
use shieldd_sdk_compliance::ComplianceQuery;
use shieldd_sdk_keys::{keys::AddressIndex, Address};
use shieldd_sdk_proto::view::v1::NotesRequest;
use shieldd_sdk_sct::nullifier_generation::NullifierWindow;
use shieldd_sdk_shielded_pool::discovery::Parameters;

#[async_trait]
pub trait PlanningIo: Send {
    async fn chain_id(&mut self) -> Result<String>;
    async fn nullifier_window(&mut self) -> Result<NullifierWindow>;
    async fn discovery_parameters(&mut self) -> Result<Parameters>;
    async fn notes(&mut self, request: NotesRequest) -> Result<Vec<SpendableNoteRecord>>;
    async fn address_by_index(&mut self, index: AddressIndex) -> Result<Address>;
    async fn index_by_address(&mut self, address: Address) -> Result<Option<AddressIndex>>;
    async fn compliance_data(
        &mut self,
        queries: Vec<ComplianceQuery>,
    ) -> Result<BatchComplianceData>;
}

#[async_trait]
impl<V: ViewClient + Send + ?Sized> PlanningIo for V {
    async fn chain_id(&mut self) -> Result<String> {
        Ok(ViewClient::app_params(self).await?.chain_id)
    }
    async fn nullifier_window(&mut self) -> Result<NullifierWindow> {
        ViewClient::nullifier_window(self).await
    }
    async fn discovery_parameters(&mut self) -> Result<Parameters> {
        ViewClient::discovery_parameters(self).await
    }
    async fn notes(&mut self, request: NotesRequest) -> Result<Vec<SpendableNoteRecord>> {
        ViewClient::notes(self, request).await
    }
    async fn address_by_index(&mut self, index: AddressIndex) -> Result<Address> {
        ViewClient::address_by_index(self, index).await
    }
    async fn index_by_address(&mut self, address: Address) -> Result<Option<AddressIndex>> {
        ViewClient::index_by_address(self, address).await
    }
    async fn compliance_data(
        &mut self,
        queries: Vec<ComplianceQuery>,
    ) -> Result<BatchComplianceData> {
        let batch = self.compliance_batch_merkle_proofs(queries.clone()).await?;
        anyhow::ensure!(
            batch.results.len() == queries.len(),
            "compliance batch result count mismatch"
        );
        let assets = queries
            .iter()
            .zip(&batch.results)
            .filter_map(|(query, result)| result.is_regulated.then_some(query.asset_id))
            .collect::<std::collections::BTreeSet<_>>();
        let mut policies = std::collections::BTreeMap::new();
        for asset in assets {
            let policy = self
                .compliance_asset_policy(asset)
                .await?
                .asset_policy
                .ok_or_else(|| anyhow::anyhow!("missing regulated asset policy"))?;
            policies.insert(asset, policy.try_into()?);
        }
        crate::client_compliance::parse_batch_compliance(&queries, batch, policies)
    }
}
