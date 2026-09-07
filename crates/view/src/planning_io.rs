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

#[async_trait]
impl<V: ViewClient + Send + ?Sized> PlanningIo for V {
    async fn latest_block_timestamp(&mut self) -> Result<u64> {
        Ok(ViewClient::status(self).await?.latest_block_timestamp)
    }
    async fn volume_accumulator_recovery(
        &mut self,
        subject: decaf377::Fq,
        day_start: u64,
    ) -> Result<crate::storage::VolumeAccumulatorRecovery> {
        ViewClient::volume_accumulator_recovery(self, subject, day_start).await
    }

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

pub(crate) fn decode_volume_recovery(
    response: shieldd_sdk_proto::view::v1::VolumeAccumulatorRecoveryResponse,
    subject: decaf377::Fq,
    day_start: u64,
) -> Result<crate::storage::VolumeAccumulatorRecovery> {
    use crate::storage::{ConfirmedVolumeAccumulator, VolumeAccumulatorRecovery};
    use shieldd_sdk_proto::view::v1::volume_accumulator_recovery_response::Outcome;
    Ok(
        match response
            .outcome
            .ok_or_else(|| anyhow::anyhow!("missing volume recovery outcome"))?
        {
            Outcome::Absent(_) => VolumeAccumulatorRecovery::Absent,
            Outcome::Incomplete(_) => VolumeAccumulatorRecovery::Incomplete,
            Outcome::Complete(complete) => {
                let decode_field = |bytes: Vec<u8>| -> Result<decaf377::Fq> {
                    let bytes: [u8; 32] = bytes
                        .try_into()
                        .map_err(|_| anyhow::anyhow!("invalid volume recovery field length"))?;
                    decaf377::Fq::from_bytes_checked(&bytes)
                        .map_err(|e| anyhow::anyhow!("invalid volume recovery field: {e}"))
                };
                let state = shieldd_sdk_shielded_pool::VolumeAccumulatorState {
                    subject: decode_field(complete.subject)?,
                    day_start: complete.day_start,
                    undisclosed_volume: u128::from_le_bytes(
                        complete
                            .undisclosed_volume
                            .try_into()
                            .map_err(|_| anyhow::anyhow!("invalid recovered volume length"))?,
                    ),
                    blinding: decode_field(complete.blinding)?,
                };
                let commitment = complete
                    .commitment
                    .ok_or_else(|| anyhow::anyhow!("missing recovered commitment"))?
                    .try_into()?;
                anyhow::ensure!(
                    state.subject == subject
                        && state.day_start == day_start
                        && state.commitment() == commitment,
                    "volume recovery does not match requested subject/day or commitment"
                );
                let position = shieldd_sdk_tct::Position::from(complete.position);
                anyhow::ensure!(
                    u64::from(position) == complete.position,
                    "recovered position exceeds the SCT range"
                );
                VolumeAccumulatorRecovery::Complete(ConfirmedVolumeAccumulator {
                    state,
                    commitment,
                    position,
                })
            }
        },
    )
}

pub(crate) fn encode_volume_recovery(
    recovery: crate::storage::VolumeAccumulatorRecovery,
) -> shieldd_sdk_proto::view::v1::VolumeAccumulatorRecoveryResponse {
    use crate::storage::VolumeAccumulatorRecovery;
    use shieldd_sdk_proto::view::v1::{
        self as pb,
        volume_accumulator_recovery_response::{self as response, Outcome},
    };
    let outcome = match recovery {
        VolumeAccumulatorRecovery::Absent => Outcome::Absent(response::Absent {}),
        VolumeAccumulatorRecovery::Incomplete => Outcome::Incomplete(response::Incomplete {}),
        VolumeAccumulatorRecovery::Complete(confirmed) => Outcome::Complete(response::Complete {
            subject: confirmed.state.subject.to_bytes().to_vec(),
            day_start: confirmed.state.day_start,
            undisclosed_volume: confirmed.state.undisclosed_volume.to_le_bytes().to_vec(),
            blinding: confirmed.state.blinding.to_bytes().to_vec(),
            commitment: Some(confirmed.commitment.into()),
            position: confirmed.position.into(),
        }),
    };
    pb::VolumeAccumulatorRecoveryResponse {
        outcome: Some(outcome),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{ConfirmedVolumeAccumulator, VolumeAccumulatorRecovery};
    use shieldd_sdk_shielded_pool::VolumeAccumulatorState;

    #[test]
    fn volume_recovery_rpc_preserves_local_results_and_rejects_invalid_context() {
        let subject = decaf377::Fq::from(17u64);
        let day_start = 86_400;
        let state = VolumeAccumulatorState {
            subject,
            day_start,
            undisclosed_volume: 42,
            blinding: decaf377::Fq::from(23u64),
        };
        let complete = VolumeAccumulatorRecovery::Complete(ConfirmedVolumeAccumulator {
            commitment: state.commitment(),
            state,
            position: 7u64.into(),
        });
        for recovery in [
            VolumeAccumulatorRecovery::Absent,
            VolumeAccumulatorRecovery::Incomplete,
            complete.clone(),
        ] {
            let encoded = encode_volume_recovery(recovery);
            let decoded = decode_volume_recovery(encoded.clone(), subject, day_start).unwrap();
            assert_eq!(encode_volume_recovery(decoded), encoded);
        }
        assert!(decode_volume_recovery(Default::default(), subject, day_start).is_err());
        let encoded = encode_volume_recovery(complete);
        assert!(
            decode_volume_recovery(encoded.clone(), decaf377::Fq::from(99u64), day_start).is_err()
        );
        assert!(decode_volume_recovery(encoded.clone(), subject, day_start + 86_400).is_err());
        use shieldd_sdk_proto::view::v1::volume_accumulator_recovery_response::Outcome;
        for field in 0..4 {
            let mut malformed = encoded.clone();
            let Some(Outcome::Complete(value)) = &mut malformed.outcome else {
                unreachable!()
            };
            match field {
                0 => value.undisclosed_volume.push(0),
                1 => value.blinding = vec![255; 32],
                2 => value.commitment = None,
                _ => value.position = u64::MAX,
            }
            assert!(decode_volume_recovery(malformed, subject, day_start).is_err());
        }
    }
}
