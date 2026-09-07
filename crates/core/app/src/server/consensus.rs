use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use sha2::Digest as _;

use cnidarium::Storage;
use tendermint::abci::Event;
use tendermint::v0_37::abci::{
    request, response, ConsensusRequest as Request, ConsensusResponse as Response,
};
use tokio::sync::mpsc;
use tower::BoxError;
use tower_actor::Message;
use tracing::Instrument;

use crate::app::App;
use crate::block_tx_indexing::BlockTxIndexingMode;
use crate::metrics;
use crate::stateless_cache::StatelessCache;

pub struct Consensus {
    queue: mpsc::Receiver<Message<Request, Response, tower::BoxError>>,
    storage: Storage,
    app: App,
    stateless_cache: Arc<StatelessCache>,
    last_commit_finished_at: Option<Instant>,
    current_block_delivered_txs: usize,
    prepared_proposal_height: Option<u64>,
    prepared_proposal_digests: HashSet<[u8; 32]>,
    aggregate_retry_cache: Option<crate::app::CachedProposalAggregate>,
}

pub type ConsensusService = tower_actor::Actor<Request, Response, BoxError>;

fn trace_events(events: &[Event]) {
    for event in events {
        let span = tracing::debug_span!("event", kind = ?event.kind);
        span.in_scope(|| {
            for attr in &event.attributes {
                tracing::debug!(
                    k = %String::from_utf8_lossy(attr.key_bytes()),
                    v = %String::from_utf8_lossy(attr.value_bytes()),
                );
            }
        })
    }
}

fn can_reuse_prepared_proposal(
    prepared_height: Option<u64>,
    prepared_digests: &HashSet<[u8; 32]>,
    proposal_height: u64,
    proposal_digest: &[u8; 32],
) -> bool {
    prepared_height == Some(proposal_height) && prepared_digests.contains(proposal_digest)
}

impl Consensus {
    const QUEUE_SIZE: usize = 10;

    pub fn new(storage: Storage) -> ConsensusService {
        Self::new_with_cache(storage, Arc::new(StatelessCache::new()))
    }

    pub fn new_with_cache(
        storage: Storage,
        stateless_cache: Arc<StatelessCache>,
    ) -> ConsensusService {
        tower_actor::Actor::new(Self::QUEUE_SIZE, |queue: _| {
            Consensus::new_inner(storage, stateless_cache, queue).run()
        })
    }

    fn new_inner(
        storage: Storage,
        stateless_cache: Arc<StatelessCache>,
        queue: mpsc::Receiver<Message<Request, Response, tower::BoxError>>,
    ) -> Self {
        let mut app = App::new(storage.latest_snapshot());
        app.set_block_tx_indexing_mode(BlockTxIndexingMode::DeferredBatch);
        Self {
            queue,
            storage,
            app,
            stateless_cache,
            last_commit_finished_at: None,
            current_block_delivered_txs: 0,
            prepared_proposal_height: None,
            prepared_proposal_digests: HashSet::new(),
            aggregate_retry_cache: None,
        }
    }

    fn record_phase_duration(phase: &'static str, started: Instant) {
        metrics::histogram!(metrics::CONSENSUS_PHASE_DURATION, "phase" => phase)
            .record(started.elapsed().as_secs_f64());
    }

    fn record_block_tx_count(phase: &'static str, tx_count: usize) {
        metrics::histogram!(metrics::CONSENSUS_BLOCK_TX_COUNT, "phase" => phase)
            .record(tx_count as f64);
    }

    fn proposal_digest<T: AsRef<[u8]>>(txs: &[T]) -> [u8; 32] {
        let mut hasher = sha2::Sha256::new();
        hasher.update((txs.len() as u64).to_le_bytes());
        for tx in txs {
            let bytes = tx.as_ref();
            hasher.update((bytes.len() as u64).to_le_bytes());
            hasher.update(bytes);
        }
        hasher.finalize().into()
    }

    async fn run(mut self) -> Result<(), tower::BoxError> {
        while let Some(Message {
            req,
            rsp_sender,
            span,
        }) = self.queue.recv().await
        {
            // The send only fails if the receiver was dropped, which happens
            // if the caller didn't propagate the message back to tendermint
            // for some reason -- but that's not our problem.
            let _ = rsp_sender.send(Ok(match req {
                Request::InitChain(init_chain) => Response::InitChain(
                    self.init_chain(init_chain)
                        .instrument(span)
                        .await
                        .expect("init_chain must succeed"),
                ),
                Request::PrepareProposal(proposal) => Response::PrepareProposal(
                    match self.prepare_proposal(proposal).instrument(span).await {
                        Ok(rsp) => rsp,
                        Err(e) => {
                            tracing::error!(
                                ?e,
                                "prepare_proposal failed; returning empty proposal"
                            );
                            response::PrepareProposal { txs: vec![] }
                        }
                    },
                ),
                Request::ProcessProposal(proposal) => Response::ProcessProposal(
                    match self.process_proposal(proposal).instrument(span).await {
                        Ok(rsp) => rsp,
                        Err(e) => {
                            tracing::error!(?e, "process_proposal failed; rejecting proposal");
                            response::ProcessProposal::Reject
                        }
                    },
                ),
                Request::BeginBlock(begin_block) => Response::BeginBlock(
                    self.begin_block(begin_block)
                        .instrument(span)
                        .await
                        .expect("begin_block must succeed"),
                ),
                Request::DeliverTx(deliver_tx) => {
                    Response::DeliverTx(self.deliver_tx(deliver_tx).instrument(span.clone()).await)
                }
                Request::EndBlock(end_block) => {
                    Response::EndBlock(self.end_block(end_block).instrument(span).await)
                }
                Request::Commit => Response::Commit(
                    self.commit()
                        .instrument(span)
                        .await
                        .expect("commit must succeed"),
                ),
            }));
        }
        Ok(())
    }

    /// Initializes the chain based on the genesis data.
    ///
    /// The genesis data is provided by tendermint, and is used to initialize
    /// the database.
    async fn init_chain(&mut self, init_chain: request::InitChain) -> Result<response::InitChain> {
        // Note that errors cannot be handled in InitChain, the application must crash.
        let app_state: crate::genesis::AppState =
            serde_json::from_slice(&init_chain.app_state_bytes)
                .expect("can parse app_state in genesis file");

        self.app.init_chain(&app_state).await;

        let app_hash = match &app_state {
            crate::genesis::AppState::Checkpoint(h) => {
                tracing::info!(?h, "genesis state is a checkpoint");
                // If we're starting from a checkpoint, we just need to forward the app hash
                // back to CometBFT.
                self.storage.latest_snapshot().root_hash().await?
            }
            crate::genesis::AppState::Content(_) => {
                tracing::info!("genesis state is a full configuration");
                // Check that we haven't got a duplicated InitChain message for some reason:
                if self.storage.latest_version() != u64::MAX {
                    anyhow::bail!("database already initialized");
                }
                // Note: App::commit resets internal components, so we don't need to do that ourselves.
                self.app.commit(self.storage.clone()).await
            }
        };

        tracing::info!(
            consensus_params = ?init_chain.consensus_params,
            app_hash = ?app_hash,
            "finished init_chain"
        );

        Ok(response::InitChain {
            consensus_params: Some(init_chain.consensus_params),
            validators: Vec::new(),
            app_hash: app_hash.0.to_vec().try_into()?,
        })
    }

    async fn prepare_proposal(
        &mut self,
        proposal: request::PrepareProposal,
    ) -> Result<response::PrepareProposal> {
        let started = Instant::now();
        let proposal_height = proposal.height.value() as u64;
        let candidate_tx_count = proposal.txs.len();
        let candidate_tx_bytes = proposal.txs.iter().map(|tx| tx.len()).sum::<usize>();
        tracing::info!(
            height = proposal_height,
            proposer = ?proposal.proposer_address,
            candidate_tx_count,
            candidate_tx_bytes,
            "prepare_proposal_start"
        );
        if let Some(last_commit_finished_at) = self.last_commit_finished_at {
            metrics::histogram!(metrics::CONSENSUS_BLOCK_IDLE_GAP)
                .record(last_commit_finished_at.elapsed().as_secs_f64());
        }
        Self::record_block_tx_count("proposed", proposal.txs.len());
        if self.prepared_proposal_height != Some(proposal_height) {
            self.prepared_proposal_height = Some(proposal_height);
            self.prepared_proposal_digests.clear();
            self.aggregate_retry_cache = None;
        }
        // We prepare a proposal against an isolated fork of the application state.
        let mut tmp_app = App::new(self.storage.latest_snapshot());
        tmp_app.set_block_tx_indexing_mode(BlockTxIndexingMode::NoIndex);
        tmp_app.set_aggregate_retry_cache(self.aggregate_retry_cache.clone());
        // Once we are done, we discard it so that the application state doesn't get corrupted
        // if another round of consensus is required because the proposal fails to finalize.
        let (response, _) = tmp_app
            .prepare_proposal(proposal, Some(self.stateless_cache.as_ref()), false)
            .await;
        self.aggregate_retry_cache = tmp_app.aggregate_retry_cache();
        let response_digest = Self::proposal_digest(&response.txs);
        self.prepared_proposal_digests.insert(response_digest);
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        let included_tx_count = response.txs.len();
        let included_tx_bytes = response.txs.iter().map(|tx| tx.len()).sum::<usize>();
        tracing::info!(
            height = proposal_height,
            candidate_tx_count,
            included_tx_count,
            included_tx_bytes,
            elapsed_ms,
            "prepare_proposal_finish"
        );
        Self::record_phase_duration("prepare_proposal", started);
        Ok(response)
    }

    async fn process_proposal(
        &mut self,
        proposal: request::ProcessProposal,
    ) -> Result<response::ProcessProposal> {
        let started = Instant::now();
        let proposal_height = proposal.height.value() as u64;
        let proposal_tx_count = proposal.txs.len();
        let proposal_tx_bytes = proposal.txs.iter().map(|tx| tx.len()).sum::<usize>();
        tracing::info!(
            height = proposal_height,
            proposer = ?proposal.proposer_address,
            proposal_hash = %proposal.hash,
            proposal_tx_count,
            proposal_tx_bytes,
            "process_proposal_start"
        );
        Self::record_block_tx_count("processed", proposal_tx_count);
        let proposal_digest = Self::proposal_digest(&proposal.txs);
        if can_reuse_prepared_proposal(
            self.prepared_proposal_height,
            &self.prepared_proposal_digests,
            proposal_height,
            &proposal_digest,
        ) {
            let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
            tracing::info!(
                height = proposal_height,
                proposal_hash = %proposal.hash,
                verdict = "accept",
                cache_reuse = true,
                elapsed_ms,
                "process_proposal_finish"
            );
            Self::record_phase_duration("process_proposal", started);
            return Ok(response::ProcessProposal::Accept);
        }
        // Proposal validation applies effects to an isolated state fork.
        let mut tmp_app = App::new(self.storage.latest_snapshot());
        tmp_app.set_block_tx_indexing_mode(BlockTxIndexingMode::NoIndex);
        let response = tmp_app
            .process_proposal(proposal, Some(self.stateless_cache.as_ref()), None, false)
            .await;
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        let verdict = match response {
            response::ProcessProposal::Accept => "accept",
            response::ProcessProposal::Reject => "reject",
            response::ProcessProposal::Unknown => "unknown",
        };
        tracing::info!(
            height = proposal_height,
            verdict,
            cache_reuse = false,
            elapsed_ms,
            "process_proposal_finish"
        );
        Self::record_phase_duration("process_proposal", started);
        Ok(response)
    }

    async fn begin_block(
        &mut self,
        begin_block: request::BeginBlock,
    ) -> Result<response::BeginBlock> {
        let started = Instant::now();
        // We don't need to print the block height, because it will already be
        // included in the span modeling the abci request handling.
        tracing::info!(time = ?begin_block.header.time, "beginning block");
        self.current_block_delivered_txs = 0;
        self.prepared_proposal_height = None;
        self.prepared_proposal_digests.clear();
        self.aggregate_retry_cache = None;

        let events = self.app.begin_block(&begin_block).await;
        Self::record_phase_duration("begin_block", started);

        Ok(response::BeginBlock { events })
    }

    async fn deliver_tx(&mut self, deliver_tx: request::DeliverTx) -> response::DeliverTx {
        // Unlike the other messages, DeliverTx is fallible, so
        // inspect the response to report errors.
        let rsp = self
            .app
            .deliver_tx_bytes(deliver_tx.tx.as_ref(), Some(self.stateless_cache.as_ref()))
            .await;

        match rsp {
            Ok(events) => {
                self.current_block_delivered_txs += 1;
                trace_events(&events);
                response::DeliverTx {
                    events,
                    ..Default::default()
                }
            }
            Err(e) => {
                tracing::info!(?e, "deliver_tx failed");
                response::DeliverTx {
                    code: 1.into(),
                    // Use the alternate format specifier to include the chain of error causes.
                    log: format!("{e:#}"),
                    ..Default::default()
                }
            }
        }
    }

    async fn end_block(&mut self, end_block: request::EndBlock) -> response::EndBlock {
        let started = Instant::now();
        let latest_state_version = self.storage.latest_version();
        tracing::info!(height = ?end_block.height, ?latest_state_version, "ending block");
        if latest_state_version >= end_block.height as u64 {
            tracing::warn!(
                %latest_state_version,
                %end_block.height,
                "chain state version is ahead of the block height, this is an unexpected corruption of chain state"
            );
        }
        let events = self.app.end_block(&end_block).await;
        trace_events(&events);

        Self::record_phase_duration("end_block", started);

        response::EndBlock {
            validator_updates: Vec::new(),
            consensus_param_updates: None,
            events,
        }
    }

    async fn commit(&mut self) -> Result<response::Commit> {
        let started = Instant::now();
        let app_hash = self.app.commit(self.storage.clone()).await;
        tracing::info!(?app_hash, "committed block");
        Self::record_phase_duration("commit", started);
        Self::record_block_tx_count("committed", self.current_block_delivered_txs);
        self.last_commit_finished_at = Some(Instant::now());
        self.prepared_proposal_height = None;
        self.prepared_proposal_digests.clear();
        self.aggregate_retry_cache = None;

        Ok(response::Commit {
            data: app_hash.0.to_vec().into(),
            retain_height: 0u32.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::can_reuse_prepared_proposal;

    #[test]
    fn prepared_proposal_reuse_requires_same_height_exact_digest() {
        let exact_digest = [7u8; 32];
        let prepared_digests = HashSet::from([exact_digest]);

        assert!(can_reuse_prepared_proposal(
            Some(42),
            &prepared_digests,
            42,
            &exact_digest,
        ));
        assert!(!can_reuse_prepared_proposal(
            Some(42),
            &prepared_digests,
            43,
            &exact_digest,
        ));
        assert!(!can_reuse_prepared_proposal(
            Some(42),
            &prepared_digests,
            42,
            &[8u8; 32],
        ));
        assert!(!can_reuse_prepared_proposal(
            None,
            &prepared_digests,
            42,
            &exact_digest,
        ));
    }
}
