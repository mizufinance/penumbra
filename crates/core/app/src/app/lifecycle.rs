use super::*;

impl App {
    /// Initializes the Shieldd execution state for a host-owned chain.
    pub async fn init_chain(&mut self, app_state: &AppState) {
        let mut state_tx = self
            .state
            .try_begin_transaction()
            .expect("state Arc should not be referenced elsewhere");
        match app_state {
            AppState::Content(genesis) => {
                crate::app_version::initialize_app_version(&mut state_tx);
                state_tx.put_chain_id(genesis.chain_id.clone());
                state_tx.put_host_withdrawals_enabled(true);
                state_tx.put_ibc_params(genesis.ibc_content.ibc_params.clone());
                Sct::init_chain(&mut state_tx, Some(&genesis.sct_content)).await;
                Compliance::init_chain(&mut state_tx, Some(&genesis.compliance_content)).await;
                ShieldedPool::init_chain(&mut state_tx, Some(&genesis.shielded_pool_content)).await;
                FeeComponent::init_chain(&mut state_tx, Some(&genesis.fee_content)).await;

                state_tx
                    .finish_block()
                    .await
                    .expect("must be able to finish compact block");
            }
            AppState::Checkpoint(_) => {
                ShieldedPool::init_chain(&mut state_tx, None).await;
                FeeComponent::init_chain(&mut state_tx, None).await;
                Compliance::init_chain(&mut state_tx, None).await;
            }
        };

        state_tx.apply();
    }

    /// Runs per-block hooks for execution components only.
    pub async fn begin_block(
        &mut self,
        begin_block: &cnidarium_component::BlockContext,
    ) -> Vec<abci::Event> {
        self.pending_sct_append_log.clear();
        let mut state_tx = StateDelta::new(self.state.clone());

        clear_block_fee_price_cache(&mut state_tx);

        let mut arc_state_tx = Arc::new(state_tx);
        Sct::begin_block(&mut arc_state_tx, begin_block).await;
        ShieldedPool::begin_block(&mut arc_state_tx, begin_block).await;
        FeeComponent::begin_block(&mut arc_state_tx, begin_block).await;

        let state_tx = Arc::try_unwrap(arc_state_tx)
            .expect("components did not retain copies of shared state");

        self.apply(state_tx)
    }

    /// Flushes host transactions and closes execution-component block and epoch state.
    pub async fn end_block(&mut self, height: u64) -> Vec<abci::Event> {
        self.flush_deferred_block_transactions()
            .await
            .expect("must be able to flush deferred block transactions in end_block");
        let mut state_tx = StateDelta::new(self.state.clone());
        self.materialize_pending_sct_append_log(&mut state_tx)
            .await
            .expect("must be able to materialize deferred SCT payloads in end_block");

        tracing::debug!("running host app components' `end_block` hooks");
        let mut arc_state_tx = Arc::new(state_tx);
        Sct::end_block(&mut arc_state_tx, height).await;
        ShieldedPool::end_block(&mut arc_state_tx, height).await;
        FeeComponent::end_block(&mut arc_state_tx, height).await;
        Compliance::end_block(&mut arc_state_tx, height).await;
        let mut state_tx = Arc::try_unwrap(arc_state_tx)
            .expect("components did not retain copies of shared state");
        tracing::debug!("finished host app components' `end_block` hooks");

        let current_height = state_tx
            .get_block_height()
            .await
            .expect("able to get block height in end_block");
        let current_epoch = state_tx
            .get_current_epoch()
            .await
            .expect("able to get current epoch in end_block");

        let is_end_epoch = current_epoch.is_scheduled_epoch_end(
            current_height,
            state_tx
                .get_epoch_duration_parameter()
                .await
                .expect("able to get epoch duration in end_block"),
        ) || state_tx.is_epoch_ending_early().await;

        if is_end_epoch {
            tracing::info!(?current_height, "ending host epoch");

            let mut arc_state_tx = Arc::new(state_tx);

            Sct::end_epoch(&mut arc_state_tx)
                .await
                .expect("able to call end_epoch on Sct component");
            ShieldedPool::end_epoch(&mut arc_state_tx)
                .await
                .expect("able to call end_epoch on shielded pool component");
            FeeComponent::end_epoch(&mut arc_state_tx)
                .await
                .expect("able to call end_epoch on Fee component");

            let mut state_tx = Arc::try_unwrap(arc_state_tx)
                .expect("components did not retain copies of shared state");

            state_tx
                .finish_epoch()
                .await
                .expect("must be able to finish compact block");

            shieldd_sdk_sct::component::clock::EpochManager::put_epoch_by_height(
                &mut state_tx,
                current_height + 1,
                Epoch {
                    index: current_epoch.index + 1,
                    start_height: current_height + 1,
                },
            );

            self.apply(state_tx)
        } else {
            shieldd_sdk_sct::component::clock::EpochManager::put_epoch_by_height(
                &mut state_tx,
                current_height + 1,
                current_epoch,
            );

            state_tx
                .finish_block()
                .await
                .expect("must be able to finish compact block");

            self.apply(state_tx)
        }
    }

    /// Persists host execution state and resets snapshots for the next host call.
    pub async fn commit(&mut self, storage: Storage) -> RootHash {
        self.state
            .ensure_nullifier_block_materialized()
            .expect("cannot commit an open nullifier block");
        let commit_start = Instant::now();
        let flush_start = Instant::now();
        self.flush_deferred_block_transactions()
            .await
            .expect("must be able to flush deferred block transactions before commit");
        let flush_ms = flush_start.elapsed().as_secs_f64() * 1000.0;
        let dummy_state = StateDelta::new(storage.latest_snapshot());
        let state = Arc::try_unwrap(std::mem::replace(&mut self.state, Arc::new(dummy_state)))
            .expect("we have exclusive ownership of the State at commit()");

        let halt_check_ms = 0.0;

        let storage_commit_start = Instant::now();
        let jmt_root = storage
            .commit(state)
            .await
            .expect("must be able to successfully commit to storage");
        let storage_commit_ms = storage_commit_start.elapsed().as_secs_f64() * 1000.0;

        tracing::debug!(?jmt_root, "finished committing host state");

        let snapshot_reset_start = Instant::now();
        let latest_snapshot = storage.latest_snapshot();
        self.snapshot_version = latest_snapshot.version();
        self.committed_snapshot = latest_snapshot.clone();
        self.state = Arc::new(StateDelta::new(latest_snapshot));
        self.pending_sct_append_log.clear();
        let snapshot_reset_ms = snapshot_reset_start.elapsed().as_secs_f64() * 1000.0;
        let total_ms = commit_start.elapsed().as_secs_f64() * 1000.0;
        tracing::info!(
            commit_total_ms = total_ms,
            commit_flush_deferred_ms = flush_ms,
            commit_halt_check_ms = halt_check_ms,
            commit_storage_commit_ms = storage_commit_ms,
            commit_snapshot_reset_ms = snapshot_reset_ms,
            "host_commit_phase_profile"
        );
        jmt_root
    }
}
