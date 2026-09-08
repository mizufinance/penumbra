use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use shieldd_sdk_proto::core::component::sct::v1::{
    query_service_client::QueryServiceClient as SctQueryServiceClient,
    ArchivedNullifierProofRequest,
};
use shieldd_sdk_sct::{
    nullifier_generation::{ArchivedNullifierProof, NullifierWindow},
    Nullifier,
};
use tokio::sync::watch;
use tonic::transport::Channel;

use crate::{
    advance_historical_proof_cache, HistoricalProofCache, HistoricalProofCacheState,
    HistoricalProofProvider, HistoricalProofUpdateError, HistoricalWitnessSource, Storage,
};

pub struct RpcHistoricalWitnessSource {
    pub channel: Channel,
}

#[async_trait]
impl HistoricalWitnessSource for RpcHistoricalWitnessSource {
    async fn nonmembership_proof(
        &self,
        nullifier: Nullifier,
        generation_index: u64,
    ) -> anyhow::Result<ArchivedNullifierProof> {
        let response = SctQueryServiceClient::new(self.channel.clone())
            .archived_nullifier_proof(ArchivedNullifierProofRequest {
                generation_index,
                nullifier: Some(nullifier.into()),
            })
            .await?
            .into_inner();
        let proof: ArchivedNullifierProof = response.try_into()?;
        proof.verify_for(nullifier)?;
        Ok(proof)
    }
}

pub struct HistoricalProofWorker {
    storage: Storage,
    witness_source: Arc<dyn HistoricalWitnessSource>,
    prover: Option<Arc<dyn HistoricalProofProvider>>,
    sync_height_rx: watch::Receiver<u64>,
}

impl HistoricalProofWorker {
    pub fn new(
        storage: Storage,
        witness_source: Arc<dyn HistoricalWitnessSource>,
        prover: Option<Arc<dyn HistoricalProofProvider>>,
        sync_height_rx: watch::Receiver<u64>,
    ) -> Self {
        Self {
            storage,
            witness_source,
            prover,
            sync_height_rx,
        }
    }

    async fn persist_failure(
        &self,
        mut cache: HistoricalProofCache,
        error: HistoricalProofUpdateError,
    ) -> anyhow::Result<()> {
        cache.recover_after_restart()?;
        if cache.state != HistoricalProofCacheState::Updating {
            cache.transition(HistoricalProofCacheState::Updating)?;
        }
        match error {
            HistoricalProofUpdateError::WitnessSource(error) => {
                cache.block_on_witness_source(format!("{error:#}"))?;
            }
            HistoricalProofUpdateError::Prover(error) => {
                cache.block_on_prover(format!("{error:#}"))?;
            }
            HistoricalProofUpdateError::Invalid(error) => {
                cache.transition(HistoricalProofCacheState::Invalid)?;
                cache.set_error(format!("{error:#}"));
            }
        }
        self.storage.put_historical_proof_cache(cache).await
    }

    async fn update_cache(
        &self,
        mut cache: HistoricalProofCache,
        window: NullifierWindow,
    ) -> anyhow::Result<()> {
        cache.recover_after_restart()?;
        if cache.state == HistoricalProofCacheState::Invalid {
            return Ok(());
        }
        if cache.proof.coverage()?.generation_count < window.archived_generation_count
            && self.prover.is_none()
        {
            if cache.state != HistoricalProofCacheState::Updating {
                cache.transition(HistoricalProofCacheState::Updating)?;
            }
            cache.block_on_prover(
                "historical prover is not configured; set ceremony-backed generation and chunk proving keys",
            )?;
            return self.storage.put_historical_proof_cache(cache).await;
        }

        loop {
            let before = cache.clone();
            let prover = self.prover.as_deref().unwrap_or(&NoopHistoricalProver);
            match advance_historical_proof_cache(
                cache,
                window,
                self.witness_source.as_ref(),
                prover,
            )
            .await
            {
                Ok(updated) => {
                    let complete = updated.state == HistoricalProofCacheState::Ready;
                    self.storage
                        .put_historical_proof_cache(updated.clone())
                        .await?;
                    cache = updated;
                    if complete {
                        return Ok(());
                    }
                }
                Err(error) => return self.persist_failure(before, error).await,
            }
        }
    }

    async fn update_all(&self) -> anyhow::Result<()> {
        let Some(window) = self
            .storage
            .nullifier_window_if_initialized()
            .await
            .context("load nullifier window for historical proof worker")?
        else {
            return Ok(());
        };
        for cache in self
            .storage
            .historical_proof_caches_for_unspent_notes()
            .await?
        {
            if let Err(error) = self.update_cache(cache, window).await {
                tracing::warn!(?error, "historical proof cache update failed");
            }
        }
        Ok(())
    }

    pub async fn run(mut self) {
        loop {
            if let Err(error) = self.update_all().await {
                tracing::warn!(?error, "historical proof worker pass failed");
            }
            if self.sync_height_rx.changed().await.is_err() {
                return;
            }
        }
    }
}

struct NoopHistoricalProver;

#[async_trait]
impl HistoricalProofProvider for NoopHistoricalProver {
    async fn prove_generation(
        &self,
        _nullifier: Nullifier,
        _archived: ArchivedNullifierProof,
        _start_history_head: [u8; 32],
        _end_history_head: [u8; 32],
    ) -> anyhow::Result<shieldd_sdk_sct::nullifier_generation::GenerationNonmembershipProof> {
        anyhow::bail!("historical prover is not configured")
    }

    async fn prove_chunk(
        &self,
        _nullifier: Nullifier,
        _chunk_index: u64,
        _start_history_head: [u8; 32],
        _end_history_head: [u8; 32],
        _generation_proofs: Vec<
            shieldd_sdk_sct::nullifier_generation::GenerationNonmembershipProof,
        >,
    ) -> anyhow::Result<shieldd_sdk_sct::nullifier_generation::HistoricalChunkProof> {
        anyhow::bail!("historical prover is not configured")
    }
}
