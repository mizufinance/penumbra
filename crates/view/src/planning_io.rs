//! The storage and RPC reads required to complete a wallet plan.
use crate::SpendableNoteRecord;
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
    async fn latest_block_timestamp(&mut self) -> Result<u64>;
    async fn volume_accumulator_recovery(
        &mut self,
        subject: decaf377::Fq,
        day_start: u64,
    ) -> Result<crate::storage::VolumeAccumulatorRecovery>;
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
