//! Transaction validation and execution against the current App state.

use super::*;

pub(super) struct DecodedTransaction<'a> {
    bytes: &'a [u8],
    tx: Arc<Transaction>,
}

impl<'a> DecodedTransaction<'a> {
    pub(super) fn decode(bytes: &'a [u8]) -> Result<Self> {
        anyhow::ensure!(
            transaction_size_allowed(bytes.len()),
            "transaction size {} exceeds maximum {}",
            bytes.len(),
            MAX_TRANSACTION_SIZE_BYTES
        );
        Ok(Self {
            bytes,
            tx: Arc::new(
                Transaction::decode_canonical(bytes).context("decoding host transaction")?,
            ),
        })
    }

    pub(super) fn tx(&self) -> &Transaction {
        &self.tx
    }
}

impl App {
    /// Verify and execute one transaction, reusing byte-bound cached proof results.
    pub async fn deliver_tx_bytes(
        &mut self,
        tx_bytes: &[u8],
        cache: Option<&StatelessCache>,
    ) -> Result<Vec<abci::Event>> {
        self.deliver_tx_input(tx_bytes, None, cache).await
    }

    pub(super) async fn deliver_decoded_tx(
        &mut self,
        decoded: DecodedTransaction<'_>,
        cache: Option<&StatelessCache>,
    ) -> Result<Vec<abci::Event>> {
        self.deliver_tx_input(decoded.bytes, Some(decoded.tx), cache)
            .await
    }

    async fn deliver_tx_input(
        &mut self,
        tx_bytes: &[u8],
        decoded: Option<Arc<Transaction>>,
        cache: Option<&StatelessCache>,
    ) -> Result<Vec<abci::Event>> {
        anyhow::ensure!(
            transaction_size_allowed(tx_bytes.len()),
            "transaction size {} exceeds maximum {}",
            tx_bytes.len(),
            MAX_TRANSACTION_SIZE_BYTES
        );
        if let Some(cache) = cache {
            let hash: [u8; 32] = sha2::Sha256::digest(tx_bytes).into();
            let artifact = match cache.get(&hash, tx_bytes) {
                Some(CacheEntry::FullyVerified(artifact)) => {
                    Self::record_artifact_reuse("checktx");
                    Some(artifact)
                }
                Some(CacheEntry::Extracted(extracted)) => {
                    let mut verified = match Self::verify_tx_artifacts_for_stage(
                        "checktx_cache_upgrade",
                        std::slice::from_ref(&extracted),
                    )
                    .await
                    {
                        Ok(verified) => verified,
                        Err(error) => {
                            cache.insert_invalid(tx_bytes)?;
                            return Err(error);
                        }
                    };
                    let artifact = verified
                        .pop()
                        .context("verified cache-upgrade artifact missing")?;
                    cache.insert_fully_verified(tx_bytes, artifact.clone())?;
                    Some(artifact)
                }
                Some(CacheEntry::Invalid) => {
                    anyhow::bail!("transaction previously failed stateless checks")
                }
                None => None,
            };
            if let Some(artifact) = artifact {
                let skip_historical =
                    artifact.has_matching_historical_validation(self.snapshot_version);
                let events = if supports_parallel_prepare(artifact.tx())
                    && self.checktx_shared_context.is_some()
                {
                    self.execute_checktx_fast(artifact, skip_historical).await?
                } else {
                    self.deliver_tx_with_verified_stateless(artifact, None)
                        .await?
                };
                return Ok(events);
            }
        }

        let tx = match decoded {
            Some(tx) => tx,
            None => {
                Arc::new(Transaction::decode_canonical(tx_bytes).context("decoding transaction")?)
            }
        };
        Self::ensure_user_tx_has_no_internal_actions(&tx)?;
        let fast = supports_parallel_prepare(tx.as_ref()) && self.checktx_shared_context.is_some();
        let tx_for_extract = tx.clone();
        let handle = tokio::runtime::Handle::current();
        let span = tracing::Span::current();
        let stage = if cache.is_some() {
            "checktx"
        } else {
            "checktx_uncached"
        };
        let stateless = tokio::task::spawn_blocking(move || {
            span.in_scope(|| {
                handle.block_on(Self::build_tx_artifact_for_stage(stage, tx_for_extract))
            })
        });
        let prepared = if fast {
            let context = self
                .checktx_shared_context
                .as_ref()
                .expect("checked shared context")
                .historical_check_context
                .as_ref()
                .clone();
            let snapshot = Arc::new(self.committed_snapshot.clone());
            let tx = tx.clone();
            Some(tokio::spawn(
                async move { prepare_candidate_read(tx, snapshot, context, false).await }
                    .instrument(tracing::Span::current()),
            ))
        } else {
            None
        };
        let historical = if !fast {
            let state = self.state.clone();
            Some(tokio::spawn(
                async move { tx.check_historical(state).await }
                    .instrument(tracing::Span::current()),
            ))
        } else {
            None
        };

        // Stateless rejection wins before any prepared effects can be applied.
        let artifact_result = stateless.await.context("waiting for extraction task")?;
        if let Some(cache) = cache {
            match &artifact_result {
                Ok(artifact) => cache.insert_fully_verified(tx_bytes, artifact.clone())?,
                Err(_) => cache.insert_invalid(tx_bytes)?,
            }
        }
        let artifact = match artifact_result {
            Ok(artifact) => artifact,
            Err(error) => {
                if let Some(task) = prepared {
                    task.abort();
                }
                if let Some(task) = historical {
                    task.abort();
                }
                return Err(error).context("extract stateless failed");
            }
        };
        if let Some(prepared) = prepared {
            let prepared = prepared.await.context("waiting for prepared candidate")??;
            let stamp = self.current_historical_validation_stamp(artifact.tx());
            let artifact = artifact.with_historical_validation_owned(stamp);
            if let Some(cache) = cache {
                cache.insert_fully_verified(tx_bytes, artifact.clone())?;
            }
            let events = self.apply_prepared_checktx(artifact, prepared).await?;
            Ok(events)
        } else {
            historical
                .expect("standard path has a historical task")
                .await
                .context("waiting for historical checks")??;
            let stamp = self.current_historical_validation_stamp(artifact.tx());
            let artifact = artifact.with_historical_validation_owned(stamp);
            if let Some(cache) = cache {
                cache.insert_fully_verified(tx_bytes, artifact.clone())?;
            }
            let events = self.execute_tx_checked_historical(artifact).await?;
            Ok(events)
        }
    }

    pub(super) async fn deliver_tx_with_verified_stateless(
        &mut self,
        artifact: Arc<VerifiedTxArtifact>,
        historical_context: Option<&HistoricalCheckContext>,
    ) -> Result<Vec<abci::Event>> {
        let tx = artifact.tx().clone();

        match historical_context {
            Some(context) => {
                check_historical_with_context(Arc::as_ref(&tx), self.state.clone(), context)
                    .await
                    .context("check_stateful failed")?
            }
            None => tx
                .check_historical(self.state.clone())
                .await
                .context("check_stateful failed")?,
        }

        let events = self.execute_tx_checked_historical(artifact).await?;

        Ok(events)
    }

    pub(super) async fn execute_checktx_fast(
        &mut self,
        artifact: Arc<VerifiedTxArtifact>,
        skip_historical: bool,
    ) -> Result<Vec<abci::Event>> {
        let context = self
            .checktx_shared_context
            .as_ref()
            .map(|context| context.historical_check_context.as_ref().clone())
            .context("missing CheckTxSharedContext for fast CheckTx path")?;
        let tx = artifact.tx().clone();
        let snapshot = self.committed_snapshot.clone();
        let handle = tokio::runtime::Handle::current();
        let prepared = tokio::task::spawn_blocking(move || {
            prepare_candidate_read_blocking(tx, snapshot, context, skip_historical, handle)
        })
        .await
        .context("joining fast CheckTx prepare task")??;
        self.apply_prepared_checktx(artifact, prepared).await
    }

    pub(super) async fn apply_prepared_checktx(
        &mut self,
        artifact: Arc<VerifiedTxArtifact>,
        prepared: PreparedCandidateRead,
    ) -> Result<Vec<abci::Event>> {
        let tx = artifact.tx().clone();

        let mut state_tx = self
            .state
            .try_begin_transaction()
            .expect("state Arc should be present and unique");

        let mut deferred_transaction = None;
        match self.block_tx_indexing_mode {
            BlockTxIndexingMode::NoIndex => {}
            BlockTxIndexingMode::PerTx => {
                let height = state_tx.get_block_height().await?;

                let transaction = Arc::as_ref(&tx).clone();

                let proto_transaction = transaction.into();

                Self::append_block_transaction_to_state(&mut state_tx, height, proto_transaction)
                    .await
                    .context("storing transactions")?;
            }
            BlockTxIndexingMode::DeferredBatch => {
                let _height = state_tx.get_block_height().await?;

                let transaction = Arc::as_ref(&tx).clone();

                let proto_transaction = transaction.into();

                deferred_transaction = Some(proto_transaction);
            }
        }

        let tx_id = tx.id();

        state_tx.put_current_source(Some(tx_id.clone()));

        let gas_used = tx.gas_cost();
        let fee = tx.transaction_body.transaction_parameters.fee;
        if let Some(context) = self.checktx_shared_context.as_ref() {
            Self::apply_checktx_fee_with_context(&mut state_tx, gas_used, fee, context)?;
        } else {
            state_tx.pay_fee(gas_used, fee).await?;
        }

        // CheckTx runs against an ephemeral per-transaction app fork. For the
        // supported fast path, committed-state nullifier checks have already
        // run in the read phase, and same-block conflict resolution is a
        // proposer/block concern. However, the fast path still builds an app
        // fork with concrete state for downstream consumers and tests, so the
        // fork should reflect the same semantic spend set as the slow path.
        for scoped in &prepared.volume_nullifiers {
            state_tx
                .record_volume_nullifier(scoped.day_start, scoped.nullifier)
                .await?;
        }

        state_tx
            .nullify_all(&prepared.spend_nullifiers, tx_id.clone().into())
            .await?;

        for nullifier in &prepared.spend_nullifiers {
            state_tx.record_proto(
                shieldd_sdk_shielded_pool::event::EventNullifierSpent {
                    nullifier: *nullifier,
                }
                .to_proto(),
            );
        }

        for payload in &prepared.sct_payloads {
            if let StatePayload::Note { note, .. } = payload {
                state_tx.record_proto(
                    shieldd_sdk_shielded_pool::event::EventNoteCreated {
                        note_commitment: note.note_commitment,
                    }
                    .to_proto(),
                );
            }
        }

        if let Some(context) = self.checktx_shared_context.as_ref() {
            let base_position_u64: u64 = context.sct_base_position.into();
            for (offset, payload) in prepared.sct_payloads.iter().enumerate() {
                let position = shieldd_sdk_tct::Position::from(base_position_u64 + offset as u64);
                state_tx.record_proto(shieldd_sdk_sct::event::commitment(
                    *payload.commitment(),
                    position,
                    payload.source().clone(),
                ));
            }
        } else {
            let positioned_sct_payloads = self
                .pending_sct_append_log
                .reserve_positions(&state_tx, prepared.sct_payloads.clone())
                .await
                .context("reserving deferred SCT positions")?;
            for (position, payload) in &positioned_sct_payloads {
                state_tx.record_proto(shieldd_sdk_sct::event::commitment(
                    *payload.commitment(),
                    *position,
                    payload.source().clone(),
                ));
            }
            self.pending_sct_append_log
                .append_positioned(positioned_sct_payloads);
        }

        state_tx.stage_routing_actions(prepared.routing_actions.clone());
        append_transaction_audit_effects(&mut state_tx, prepared.audit_effects.clone()).await?;

        let events = state_tx.apply().1;

        if let Some(transaction) = deferred_transaction {
            self.deferred_block_transactions.push(transaction);
        }

        Ok(events)
    }

    pub(super) async fn execute_tx_checked_historical(
        &mut self,
        artifact: Arc<VerifiedTxArtifact>,
    ) -> Result<Vec<abci::Event>> {
        let tx = artifact.tx().clone();

        // At this point, the stateful checks should have completed,
        // leaving us with exclusive access to the Arc<State>.

        let tx_id = tx.id();
        let state_arc_strong_count = Arc::strong_count(&self.state);
        let mut state_tx = self
            .state
            .try_begin_transaction()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "CheckTx could not begin state transaction after historical checks: tx_id={}, action_count={}, state_arc_strong_count={}",
                    tx_id,
                    tx.actions().count(),
                    state_arc_strong_count,
                )
            })?;

        // Index the transaction:

        let mut deferred_transaction = None;
        match self.block_tx_indexing_mode {
            BlockTxIndexingMode::NoIndex => {}
            BlockTxIndexingMode::PerTx => {
                let height = state_tx.get_block_height().await?;

                let transaction = Arc::as_ref(&tx).clone();

                let proto_transaction = transaction.into();

                Self::append_block_transaction_to_state(&mut state_tx, height, proto_transaction)
                    .await
                    .context("storing transactions")?;
            }
            BlockTxIndexingMode::DeferredBatch => {
                let _height = state_tx.get_block_height().await?;

                let transaction = Arc::as_ref(&tx).clone();

                let proto_transaction = transaction.into();

                deferred_transaction = Some(proto_transaction);
            }
        }

        check_and_execute(Arc::as_ref(&artifact), &mut state_tx)
            .await
            .context("executing transaction")?;

        // At this point, we've completed execution successfully with no errors,
        // so we can apply the transaction to the State. Otherwise, we'd have
        // bubbled up an error and dropped the StateTransaction.

        let events = state_tx.apply().1;

        if let Some(transaction) = deferred_transaction {
            self.deferred_block_transactions.push(transaction);
        }

        Ok(events)
    }
}
