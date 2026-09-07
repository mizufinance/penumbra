use anyhow::Result;
use futures::FutureExt;

use cnidarium::{Snapshot, Storage};

use tendermint::v0_37::abci::{
    request::CheckTx as CheckTxReq, request::CheckTxKind, response::CheckTx as CheckTxRsp,
    MempoolRequest as Request, MempoolResponse as Response,
};
use tokio::sync::mpsc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tower_actor::Message;
use tracing::Instrument;

use std::sync::Arc;
use std::{any::Any, panic::AssertUnwindSafe};

use crate::{
    app::{App, MAX_TRANSACTION_SIZE_BYTES},
    block_tx_indexing::BlockTxIndexingMode,
    metrics,
    stateless_cache::StatelessCache,
};

const DEFAULT_MAX_IN_FLIGHT_CHECKTX: usize = 8;
const MAX_IN_FLIGHT_CHECKTX: usize = 64;
const MAX_IN_FLIGHT_CHECKTX_HEAVYWORK: usize = 32;

fn bounded_checktx_concurrency(available_parallelism: usize) -> usize {
    available_parallelism.clamp(DEFAULT_MAX_IN_FLIGHT_CHECKTX, MAX_IN_FLIGHT_CHECKTX)
}

fn bounded_checktx_heavywork_concurrency(available_parallelism: usize) -> usize {
    (available_parallelism / 2).clamp(1, MAX_IN_FLIGHT_CHECKTX_HEAVYWORK)
}

fn available_parallelism() -> usize {
    std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1)
}

fn max_in_flight_checktx() -> usize {
    bounded_checktx_concurrency(available_parallelism())
}

fn max_in_flight_checktx_heavywork() -> usize {
    bounded_checktx_heavywork_concurrency(available_parallelism())
}

fn oversized_checktx_response(tx_size_bytes: usize) -> Option<Response> {
    (tx_size_bytes > MAX_TRANSACTION_SIZE_BYTES).then(|| {
        Response::CheckTx(CheckTxRsp {
            code: 1.into(),
            log: format!(
                "transaction size {tx_size_bytes} exceeds maximum {MAX_TRANSACTION_SIZE_BYTES}"
            ),
            ..Default::default()
        })
    })
}

/// A mempool service that applies transaction checks against an isolated application fork.
pub struct Mempool {
    queue: mpsc::Receiver<Message<Request, Response, tower::BoxError>>,
    storage: Storage,
    stateless_cache: Arc<StatelessCache>,
}

impl Mempool {
    fn panic_payload_message(payload: &(dyn Any + Send)) -> String {
        if let Some(message) = payload.downcast_ref::<&'static str>() {
            (*message).to_string()
        } else if let Some(message) = payload.downcast_ref::<String>() {
            message.clone()
        } else {
            "non-string panic payload".to_string()
        }
    }

    pub fn new(
        storage: Storage,
        stateless_cache: Arc<StatelessCache>,
        queue: mpsc::Receiver<Message<Request, Response, tower::BoxError>>,
    ) -> Self {
        Self {
            queue,
            storage,
            stateless_cache,
        }
    }

    async fn check_tx_with_state(
        snapshot: Snapshot,
        stateless_cache: Arc<StatelessCache>,
        req: Request,
    ) -> Result<Response, tower::BoxError> {
        let Request::CheckTx(CheckTxReq {
            tx: tx_bytes, kind, ..
        }) = req;

        let start = tokio::time::Instant::now();
        let kind_str = match kind {
            CheckTxKind::New => "new",
            CheckTxKind::Recheck => "recheck",
        };

        let mut app = App::new(snapshot);
        app.set_block_tx_indexing_mode(BlockTxIndexingMode::NoIndex);

        match app
            .deliver_tx_bytes(tx_bytes.as_ref(), Some(stateless_cache.as_ref()))
            .await
        {
            Ok(events) => {
                let elapsed = start.elapsed();
                tracing::info!(?elapsed, "tx accepted");
                metrics::histogram!(
                    metrics::MEMPOOL_CHECKTX_DURATION,
                    "kind" => kind_str,
                    "code" => "0"
                )
                .record(elapsed);
                metrics::counter!(metrics::MEMPOOL_CHECKTX_TOTAL, "kind" => kind_str, "code" => "0").increment(1);
                Ok(Response::CheckTx(CheckTxRsp {
                    events,
                    ..Default::default()
                }))
            }
            Err(e) => {
                let elapsed = start.elapsed();
                tracing::info!(?e, ?elapsed, "tx rejected");
                metrics::histogram!(
                    metrics::MEMPOOL_CHECKTX_DURATION,
                    "kind" => kind_str,
                    "code" => "1"
                )
                .record(elapsed);
                metrics::counter!(metrics::MEMPOOL_CHECKTX_TOTAL, "kind" => kind_str, "code" => "1").increment(1);
                Ok(Response::CheckTx(CheckTxRsp {
                    code: 1.into(),
                    // Use the alternate format specifier to include the chain of error causes.
                    log: format!("{e:#}"),
                    ..Default::default()
                }))
            }
        }
    }

    pub async fn run(mut self) -> Result<(), tower::BoxError> {
        let max_in_flight = max_in_flight_checktx();
        let max_in_flight_heavywork = max_in_flight_checktx_heavywork();
        tracing::info!(
            max_in_flight,
            max_in_flight_heavywork,
            "mempool service started"
        );
        let mut in_flight = JoinSet::new();
        let permits = Arc::new(Semaphore::new(max_in_flight));
        let heavywork_permits = Arc::new(Semaphore::new(max_in_flight_heavywork));
        let storage = self.storage.clone();
        let stateless_cache = self.stateless_cache.clone();
        let mut queue_closed = false;

        loop {
            tokio::select! {
                Some(joined) = in_flight.join_next(), if !in_flight.is_empty() => {
                    if let Err(join_error) = joined {
                        tracing::error!(?join_error, "check_tx task terminated unexpectedly after panic handling");
                    }
                }
                message = self.queue.recv(), if !queue_closed => {
                    match message {
                        Some(Message { req, rsp_sender, span }) => {
                            let received_at = tokio::time::Instant::now();
                            let (tx_size_bytes, kind) = match &req {
                                Request::CheckTx(CheckTxReq { tx, kind, .. }) => {
                                    (tx.len(), *kind)
                                }
                            };
                            tracing::info!(
                                parent: &span,
                                tx_size_bytes,
                                "checktx_frontdoor_received"
                            );
                            if let Some(response) = oversized_checktx_response(tx_size_bytes) {
                                let kind_str = match kind {
                                    CheckTxKind::New => "new",
                                    CheckTxKind::Recheck => "recheck",
                                };
                                tracing::info!(
                                    parent: &span,
                                    tx_size_bytes,
                                    "checktx_frontdoor_rejected_oversized"
                                );
                                metrics::histogram!(
                                    metrics::MEMPOOL_CHECKTX_DURATION,
                                    "kind" => kind_str,
                                    "code" => "1"
                                )
                                .record(received_at.elapsed());
                                metrics::counter!(
                                    metrics::MEMPOOL_CHECKTX_TOTAL,
                                    "kind" => kind_str,
                                    "code" => "1"
                                )
                                .increment(1);
                                let _ = rsp_sender.send(Ok(response));
                                continue;
                            }
                            metrics::gauge!(metrics::MEMPOOL_CHECKTX_PENDING).increment(1.0);
                            let permit_wait_started = tokio::time::Instant::now();
                            let permit = permits
                                .clone()
                                .acquire_owned()
                                .await
                                .expect("mempool semaphore should not be closed");
                            tracing::info!(
                                parent: &span,
                                permit_wait_ms = permit_wait_started.elapsed().as_secs_f64() * 1000.0,
                                "checktx_frontdoor_permit_acquired"
                            );
                            let heavywork_permits = heavywork_permits.clone();
                            let snapshot = storage.latest_snapshot();
                            let stateless_cache = stateless_cache.clone();
                            let stateless_cache_for_check = stateless_cache.clone();
                            in_flight.spawn(async move {
                                let _permit = permit;
                                let heavywork_wait_started = tokio::time::Instant::now();
                                let _heavywork_permit = heavywork_permits
                                    .acquire_owned()
                                    .await
                                    .expect("mempool heavywork semaphore should not be closed");
                                tracing::info!(
                                    heavywork_wait_ms = heavywork_wait_started.elapsed().as_secs_f64() * 1000.0,
                                    frontdoor_elapsed_ms = received_at.elapsed().as_secs_f64() * 1000.0,
                                    "checktx_frontdoor_heavywork_acquired"
                                );
                                metrics::gauge!(metrics::MEMPOOL_CHECKTX_IN_FLIGHT).increment(1.0);
                                let execute_started = tokio::time::Instant::now();
                                let result = AssertUnwindSafe(Self::check_tx_with_state(
                                        snapshot,
                                        stateless_cache_for_check,
                                        req,
                                    ))
                                    .catch_unwind()
                                    .await;
                                let result = match result {
                                    Ok(result) => result,
                                    Err(panic_payload) => {
                                        let panic_message = Self::panic_payload_message(&*panic_payload);
                                        tracing::error!(
                                            tx_size_bytes,
                                            %panic_message,
                                            "checktx task panicked; rejecting transaction instead of terminating mempool actor"
                                        );
                                        Ok(Response::CheckTx(CheckTxRsp {
                                            code: 1.into(),
                                            log: format!(
                                                "internal CheckTx panic: {panic_message}"
                                            ),
                                            ..Default::default()
                                        }))
                                    }
                                };
                                tracing::info!(
                                    execute_wall_ms = execute_started.elapsed().as_secs_f64() * 1000.0,
                                    frontdoor_total_ms = received_at.elapsed().as_secs_f64() * 1000.0,
                                    response_ok = result.is_ok(),
                                    "checktx_frontdoor_finished"
                                );
                                metrics::gauge!(metrics::MEMPOOL_CHECKTX_IN_FLIGHT).decrement(1.0);
                                metrics::gauge!(metrics::MEMPOOL_CHECKTX_PENDING).decrement(1.0);
                                let _ = rsp_sender.send(result);
                            }.instrument(span));
                        }
                        None => {
                            tracing::warn!("mempool request queue closed");
                            queue_closed = true;
                        }
                    }
                }
                else => break,
            }

            if queue_closed && in_flight.is_empty() {
                tracing::warn!(
                    "mempool service loop exiting after queue closure and draining inflight work"
                );
                break;
            }
        }
        tracing::info!("mempool service stopped");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        bounded_checktx_concurrency, bounded_checktx_heavywork_concurrency,
        oversized_checktx_response, MAX_TRANSACTION_SIZE_BYTES,
    };
    use tendermint::v0_37::abci::MempoolResponse;

    #[test]
    fn oversized_checktx_frontdoor_rejects_at_fixed_limit() {
        assert!(oversized_checktx_response(MAX_TRANSACTION_SIZE_BYTES).is_none());

        let response = oversized_checktx_response(MAX_TRANSACTION_SIZE_BYTES + 1)
            .expect("oversized CheckTx must reject at the mempool frontdoor");
        let MempoolResponse::CheckTx(response) = response;
        assert_ne!(response.code.value(), 0);
        assert_eq!(
            response.log,
            format!(
                "transaction size {} exceeds maximum {}",
                MAX_TRANSACTION_SIZE_BYTES + 1,
                MAX_TRANSACTION_SIZE_BYTES
            )
        );
    }

    #[test]
    fn checktx_concurrency_is_bounded_for_all_hardware_sizes() {
        assert_eq!(bounded_checktx_concurrency(0), 8);
        assert_eq!(bounded_checktx_concurrency(1), 8);
        assert_eq!(bounded_checktx_concurrency(32), 32);
        assert_eq!(bounded_checktx_concurrency(usize::MAX), 64);

        assert_eq!(bounded_checktx_heavywork_concurrency(0), 1);
        assert_eq!(bounded_checktx_heavywork_concurrency(1), 1);
        assert_eq!(bounded_checktx_heavywork_concurrency(64), 32);
        assert_eq!(bounded_checktx_heavywork_concurrency(usize::MAX), 32);
    }
}
