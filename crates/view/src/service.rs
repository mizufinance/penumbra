use shieldd_sdk_compliance::ComplianceQuery;
use std::{
    collections::{BTreeMap, BTreeSet},
    pin::Pin,
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, Context};
use async_stream::try_stream;
use camino::Utf8Path;
use futures::stream::{StreamExt, TryStreamExt};
use rand::Rng;
use rand_core::OsRng;
use tap::{Tap, TapFallible};
use tokio::sync::{watch, RwLock};
use tokio_stream::wrappers::WatchStream;
use tonic::transport::channel::ClientTlsConfig;
use tonic::transport::channel::Endpoint;
use tonic::{async_trait, transport::Channel, Request, Response, Status};
use tracing::{instrument, Instrument};
use url::Url;

use shieldd_sdk_asset::{asset, asset::Metadata, Value};
use shieldd_sdk_keys::{
    keys::WalletId,
    keys::{AddressIndex, FullViewingKey},
    Address, AddressView,
};
use shieldd_sdk_num::Amount;
use shieldd_sdk_proto::{
    core::component::compliance::v1 as compliance_pb,
    util::tendermint_proxy::v1::{
        tendermint_proxy_service_client::TendermintProxyServiceClient, BroadcastTxSyncRequest,
        GetStatusRequest, GetStatusResponse, SyncInfo, BROADCAST_OUTCOME_METADATA_KEY,
        BROADCAST_OUTCOME_NOT_SUBMITTED, BROADCAST_OUTCOME_UNKNOWN,
    },
    view::v1::{
        self as pb,
        broadcast_transaction_response::{BroadcastSuccess, Confirmed, Status as BroadcastStatus},
        view_service_server::ViewService,
        AppParametersResponse, AssetMetadataByIdRequest, AssetMetadataByIdResponse,
        BroadcastTransactionResponse, DiscoveryParametersResponse, GasPricesResponse,
        NoteByCommitmentResponse, NullifierWindowResponse, StatusResponse,
        TransactionPlannerResponse, WalletIdRequest, WalletIdResponse, WitnessResponse,
    },
    DomainType,
};
use shieldd_sdk_tct::{Proof, StateCommitment};
use shieldd_sdk_transaction::{
    plan::ActionPlan, txhash::TransactionId, AuthorizationData, Transaction,
    TransactionPerspective, TransactionPlan, WitnessData,
};

use crate::{
    compliance_tree::ComplianceSnapshot, historical_proof_worker::HistoricalProofWorker,
    worker::Worker, AddressPurpose, HistoricalProofProvider, IssuedAddress, NoteManager,
    NoteManagerPlanningResult, Storage,
};

/// A [`futures::Stream`] of broadcast transaction responses.
///
/// See [`ViewService::broadcast_transaction()`].
type BroadcastTransactionStream = Pin<
    Box<dyn futures::Stream<Item = Result<pb::BroadcastTransactionResponse, tonic::Status>> + Send>,
>;

const BROADCAST_NULLIFIER_DETECTION_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(180);
const BROADCAST_AMBIGUITY_RECONCILIATION_TIMEOUT: std::time::Duration =
    BROADCAST_NULLIFIER_DETECTION_TIMEOUT;

fn broadcast_outcome_unknown_status(message: impl Into<String>) -> tonic::Status {
    let mut status = tonic::Status::unavailable(message.into());
    status.metadata_mut().insert(
        BROADCAST_OUTCOME_METADATA_KEY,
        tonic::metadata::MetadataValue::from_static(BROADCAST_OUTCOME_UNKNOWN),
    );
    status
}

fn broadcast_outcome_is_unknown(status: &tonic::Status) -> bool {
    status.code() == tonic::Code::Unavailable
        && status
            .metadata()
            .get(BROADCAST_OUTCOME_METADATA_KEY)
            .and_then(|value| value.to_str().ok())
            != Some(BROADCAST_OUTCOME_NOT_SUBMITTED)
}

fn resolve_broadcast_detection(
    broadcast_error: tonic::Status,
    submitted_id: TransactionId,
    detected: Option<(u64, TransactionId)>,
) -> Result<u64, tonic::Status> {
    if !broadcast_outcome_is_unknown(&broadcast_error) {
        return Err(broadcast_error);
    }

    match detected {
        Some((height, detected_id)) if detected_id == submitted_id => Ok(height),
        Some((_, detected_id)) => Err(broadcast_outcome_unknown_status(format!(
            "{}; detected transaction {} did not match submitted transaction {}",
            broadcast_error.message(),
            detected_id,
            submitted_id,
        ))),
        None => Err(broadcast_outcome_unknown_status(format!(
            "{}; submitted transaction {} was not detected",
            broadcast_error.message(),
            submitted_id,
        ))),
    }
}

#[cfg(test)]
mod broadcast_detection_tests {
    use super::{
        broadcast_outcome_is_unknown, resolve_broadcast_detection, TransactionId,
        BROADCAST_OUTCOME_METADATA_KEY, BROADCAST_OUTCOME_NOT_SUBMITTED, BROADCAST_OUTCOME_UNKNOWN,
    };
    use tonic::{Code, Status};

    fn unknown_outcome(message: &'static str) -> Status {
        let mut status = Status::unavailable(message);
        status.metadata_mut().insert(
            BROADCAST_OUTCOME_METADATA_KEY,
            tonic::metadata::MetadataValue::from_static(BROADCAST_OUTCOME_UNKNOWN),
        );
        status
    }

    #[test]
    fn unavailable_with_matching_transaction_recovers_height() {
        let submitted_id = TransactionId([7; 32]);
        let height = resolve_broadcast_detection(
            unknown_outcome("response lost"),
            submitted_id,
            Some((42, submitted_id)),
        )
        .expect("matching transaction should reconcile an unavailable response");

        assert_eq!(height, 42);
    }

    #[test]
    fn unavailable_without_matching_transaction_remains_unavailable() {
        let submitted_id = TransactionId([7; 32]);
        let missing =
            resolve_broadcast_detection(unknown_outcome("response lost"), submitted_id, None)
                .expect_err("missing transaction must not reconcile");
        assert_eq!(missing.code(), Code::Unavailable);
        assert!(broadcast_outcome_is_unknown(&missing));
        let expected_missing =
            format!("response lost; submitted transaction {submitted_id} was not detected");
        assert_eq!(missing.message(), expected_missing.as_str());

        let detected_id = TransactionId([8; 32]);
        let different = resolve_broadcast_detection(
            unknown_outcome("response lost"),
            submitted_id,
            Some((42, detected_id)),
        )
        .expect_err("different transaction must not reconcile");
        assert_eq!(different.code(), Code::Unavailable);
        assert!(broadcast_outcome_is_unknown(&different));
        let expected_different = format!(
            "response lost; detected transaction {detected_id} did not match submitted transaction {submitted_id}"
        );
        assert_eq!(different.message(), expected_different.as_str());
    }

    #[test]
    fn only_known_not_submitted_or_non_unavailable_status_skips_reconciliation() {
        let submitted_id = TransactionId([7; 32]);
        let unmarked = Status::unavailable("status trailers lost");
        assert!(
            broadcast_outcome_is_unknown(&unmarked),
            "missing outcome metadata after invocation remains ambiguous"
        );

        let mut not_submitted = Status::unavailable("connection refused before submission");
        not_submitted.metadata_mut().insert(
            BROADCAST_OUTCOME_METADATA_KEY,
            tonic::metadata::MetadataValue::from_static(BROADCAST_OUTCOME_NOT_SUBMITTED),
        );
        assert!(!broadcast_outcome_is_unknown(&not_submitted));
        let error =
            resolve_broadcast_detection(not_submitted, submitted_id, Some((42, submitted_id)))
                .expect_err("known pre-submission failure must not reconcile");
        assert_eq!(error.code(), Code::Unavailable);
        assert_eq!(error.message(), "connection refused before submission");

        let error = resolve_broadcast_detection(
            Status::invalid_argument("definitive rejection"),
            submitted_id,
            Some((42, submitted_id)),
        )
        .expect_err("definitive status must not reconcile");

        assert_eq!(error.code(), Code::InvalidArgument);
        assert_eq!(error.message(), "definitive rejection");
    }
}

/// A service that synchronizes private chain state and responds to queries
/// about it.
///
/// The [`ViewServer`] implements the Tonic-derived [`ViewService`] trait,
/// so it can be used as a gRPC server, or called directly.  It spawns a task
/// internally that performs synchronization and scanning.  The
/// [`ViewServer`] can be cloned; each clone will read from the same shared
/// state, but there will only be a single scanning task.
#[derive(Clone)]
pub struct ViewServer {
    storage: Storage,
    // A shared error slot for errors bubbled up by the worker. This is a regular Mutex
    // rather than a Tokio Mutex because it should be uncontended.
    error_slot: Arc<Mutex<Option<anyhow::Error>>>,
    // A copy of the SCT used by the worker task.
    state_commitment_tree: Arc<RwLock<shieldd_sdk_tct::Tree>>,
    // The Url for the pd gRPC endpoint on remote node.
    node: Url,
    /// Used to watch for changes to the sync height.
    sync_height_rx: watch::Receiver<u64>,
    /// One height-consistent compliance snapshot shared with the worker.
    compliance_snapshot: Arc<parking_lot::RwLock<Arc<ComplianceSnapshot>>>,
}

impl ViewServer {
    async fn query_compliance_batch(
        &self,
        request_inner: pb::ComplianceBatchMerkleProofsRequest,
    ) -> Result<pb::ComplianceBatchMerkleProofsResponse, Status> {
        let snapshot = self.compliance_snapshot.read().clone();
        let user_tree = &snapshot.user_tree;
        let asset_tree = &snapshot.asset_tree;

        let user_anchor = user_tree.root();
        let asset_anchor = asset_tree.root();

        // Debug: log anchors at read time
        tracing::debug!(
            user_anchor = ?user_anchor.0.to_bytes(),
            asset_anchor = ?asset_anchor.0.to_bytes(),
            num_queries = request_inner.queries.len(),
            "compliance_batch_merkle_proofs: read anchors from local trees"
        );

        let mut results = Vec::with_capacity(request_inner.queries.len());

        // Lazy gRPC client - only created if we have cache misses
        use shieldd_sdk_proto::core::component::compliance::v1::{
            query_service_client::QueryServiceClient as ComplianceQueryServiceClient,
            ComplianceMerkleProofsRequest,
        };
        let mut grpc_client: Option<ComplianceQueryServiceClient<tonic::transport::Channel>> = None;

        for query in request_inner.queries {
            // Parse address and asset_id
            let address: shieldd_sdk_keys::Address = query
                .address
                .clone()
                .ok_or_else(|| tonic::Status::invalid_argument("missing address in query"))?
                .try_into()
                .map_err(|e| tonic::Status::invalid_argument(format!("invalid address: {e}")))?;

            let asset_id: shieldd_sdk_asset::asset::Id = query
                .asset_id
                .clone()
                .ok_or_else(|| tonic::Status::invalid_argument("missing asset_id in query"))?
                .try_into()
                .map_err(|e| tonic::Status::invalid_argument(format!("invalid asset_id: {e}")))?;

            // Get asset proof from local tree (using held reference)
            let (asset_position, indexed_leaf, asset_path, is_regulated) = asset_tree
                .get_proof_data(asset_id)
                .map_err(|e| tonic::Status::internal(format!("failed to get asset proof: {e}")))?;

            // Debug: log proof data
            let leaf_commitment = indexed_leaf.commit();
            tracing::debug!(
                asset_id = ?asset_id.0.to_bytes(),
                position = asset_position,
                is_regulated,
                leaf_value = ?indexed_leaf.value.to_bytes(),
                leaf_next_index = indexed_leaf.next_index,
                leaf_threshold = indexed_leaf.params.threshold,
                leaf_dk_pub_first_byte = indexed_leaf.params.dk_pub.vartime_compress().0[0],
                leaf_commitment = ?leaf_commitment.0.to_bytes(),
                "compliance_batch_merkle_proofs: asset proof data"
            );

            // Returns (user_registered, compliance_position, compliance_path, compliance_leaf).
            // Prefer real user proofs whenever a leaf exists, even for unregulated assets.
            let local_leaf_data = self
                .storage
                .get_compliance_leaf_data(&address, &asset_id)
                .await
                .map_err(|e| tonic::Status::internal(format!("storage error: {e}")))?;

            let (user_registered, compliance_position, compliance_path, compliance_leaf) =
                match local_leaf_data {
                    Some(leaf_data) => {
                        let position = leaf_data.position;
                        if user_tree.commitment(position) != Some(leaf_data.commitment) {
                            return Err(tonic::Status::unavailable(
                                "compliance projection advanced while building the proof; retry",
                            ));
                        }
                        let path = user_tree.witness(position).map_err(|e| {
                            tonic::Status::internal(format!("failed to compute path: {e}"))
                        })?;

                        let leaf_proto = compliance_pb::ComplianceLeaf {
                            address: Some(address.clone().into()),
                            asset_id: Some(asset_id.into()),
                            d: leaf_data.d.to_vec(),
                            status: compliance_pb::UserAssetStatus::from(leaf_data.status) as i32,
                        };

                        tracing::debug!(
                            ?address,
                            ?asset_id,
                            position,
                            is_regulated,
                            "using local storage for batch user proof"
                        );
                        (true, position, path, Some(leaf_proto))
                    }
                    None => {
                        tracing::debug!(
                            ?address,
                            ?asset_id,
                            is_regulated,
                            "local storage miss, fetching from pd for batch"
                        );

                        if grpc_client.is_none() {
                            let endpoint =
                                get_pd_endpoint(self.node.clone()).await.map_err(|e| {
                                    tonic::Status::internal(format!("failed to connect to pd: {e}"))
                                })?;
                            let channel = endpoint.connect().await.map_err(|e| {
                                tonic::Status::internal(format!("failed to connect to pd: {e}"))
                            })?;
                            grpc_client = Some(ComplianceQueryServiceClient::new(channel));
                        }
                        let client = grpc_client
                            .as_mut()
                            .expect("gRPC client is initialized above");

                        let proof_request = ComplianceMerkleProofsRequest {
                            address: query.address.clone(),
                            asset_id: query.asset_id.clone(),
                        };
                        let proof_response = client
                            .compliance_merkle_proofs(tonic::Request::new(proof_request))
                            .await?
                            .into_inner();
                        if proof_response.compliance_anchor.as_slice() != user_anchor.0.to_bytes()
                            || proof_response.asset_anchor.as_slice() != asset_anchor.0.to_bytes()
                        {
                            return Err(tonic::Status::unavailable(
                                "remote compliance proof is newer than the local snapshot; retry",
                            ));
                        }

                        if !proof_response.user_registered {
                            (
                                false,
                                0,
                                shieldd_sdk_compliance::structs::MerklePath::default(),
                                None,
                            )
                        } else {
                            let path = proof_response
                                .compliance_path
                                .map(shieldd_sdk_compliance::structs::MerklePath::try_from)
                                .transpose()
                                .map_err(|error| {
                                    tonic::Status::internal(format!(
                                        "invalid compliance_path in pd response: {error}"
                                    ))
                                })?
                                .ok_or_else(|| {
                                    tonic::Status::internal(
                                        "compliance_path missing from pd response",
                                    )
                                })?;

                            (
                                proof_response.user_registered,
                                proof_response.compliance_position,
                                path,
                                proof_response.compliance_leaf,
                            )
                        }
                    }
                };

            // Convert local types to proto types
            let compliance_path_proto = compliance_pb::MerklePath {
                layers: compliance_path
                    .layers
                    .into_iter()
                    .map(|layer| compliance_pb::MerklePathLayer {
                        siblings: layer.siblings,
                    })
                    .collect(),
            };

            let asset_path_proto = compliance_pb::MerklePath {
                layers: asset_path
                    .layers
                    .into_iter()
                    .map(|layer| compliance_pb::MerklePathLayer {
                        siblings: layer.siblings,
                    })
                    .collect(),
            };

            let asset_indexed_leaf_proto: compliance_pb::IndexedLeafData =
                indexed_leaf.clone().into();

            results.push(pb::ComplianceMerkleProofsResponse {
                user_registered,
                asset_registered: true, // Always true with IMT (membership or non-membership)
                is_regulated,
                compliance_path: Some(compliance_path_proto),
                compliance_position,
                asset_path: Some(asset_path_proto),
                asset_position,
                compliance_anchor: user_anchor.0.to_bytes().to_vec(),
                asset_anchor: asset_anchor.0.to_bytes().to_vec(),
                asset_indexed_leaf: Some(asset_indexed_leaf_proto),
                compliance_leaf,
            });
        }

        Ok(pb::ComplianceBatchMerkleProofsResponse {
            compliance_anchor: user_anchor.0.to_bytes().to_vec(),
            asset_anchor: asset_anchor.0.to_bytes().to_vec(),
            results,
        })
    }

    fn address_purpose(purpose: Option<pb::AddressPurpose>) -> Result<AddressPurpose, Status> {
        match purpose.and_then(|purpose| purpose.regulated_asset_id) {
            Some(asset_id) => Ok(AddressPurpose::Regulated {
                asset_id: asset_id.try_into().map_err(|error| {
                    Status::invalid_argument(format!("invalid regulated asset ID: {error:#}"))
                })?,
            }),
            None => Ok(AddressPurpose::General),
        }
    }

    async fn persist_issued_address(
        &self,
        address_index: AddressIndex,
        address: Address,
        purpose: AddressPurpose,
    ) -> Result<Address, Status> {
        let birth_height = self
            .storage
            .last_sync_height()
            .await
            .map_err(|error| Status::internal(format!("could not read sync height: {error:#}")))?
            .unwrap_or(0);
        self.storage
            .record_issued_address(IssuedAddress {
                address_index,
                address: address.clone(),
                purpose,
                birth_height,
                retired_height: None,
            })
            .await
            .map_err(|error| {
                Status::internal(format!("could not persist issued address: {error:#}"))
            })?;
        Ok(address)
    }

    /// Convenience method that calls [`Storage::load_or_initialize`] and then [`Self::new`].
    pub async fn load_or_initialize(
        storage_path: Option<impl AsRef<Utf8Path>>,
        registry_path: Option<impl AsRef<Utf8Path>>,
        fvk: &FullViewingKey,
        node: Url,
    ) -> anyhow::Result<Self> {
        let storage = Storage::load_or_initialize(storage_path, fvk, node.clone())
            .tap(|_| tracing::trace!("loading or initializing storage"))
            .await?
            .tap(|_| tracing::debug!("storage is ready"));

        if let Some(registry_path) = registry_path {
            storage.load_asset_metadata(registry_path).await?;
        }

        Self::new(storage, node)
            .tap(|_| tracing::trace!("constructing view server"))
            .await
            .tap(|_| tracing::debug!("constructed view server"))
    }

    /// Constructs a new [`ViewService`], spawning a sync task internally.
    ///
    /// The sync task uses the provided `client` to sync with the chain.
    ///
    /// To create multiple [`ViewService`]s, clone the [`ViewService`] returned
    /// by this method, rather than calling it multiple times.  That way, each clone
    /// will be backed by the same scanning task, rather than each spawning its own.
    pub async fn new(storage: Storage, node: Url) -> anyhow::Result<Self> {
        Self::new_inner(storage, node, None).await
    }

    /// Construct a view service with a local historical-proof backend.
    pub async fn new_with_historical_prover(
        storage: Storage,
        node: Url,
        historical_prover: Arc<dyn HistoricalProofProvider>,
    ) -> anyhow::Result<Self> {
        Self::new_inner(storage, node, Some(historical_prover)).await
    }

    async fn new_inner(
        storage: Storage,
        node: Url,
        historical_prover: Option<Arc<dyn HistoricalProofProvider>>,
    ) -> anyhow::Result<Self> {
        let span = tracing::error_span!(parent: None, "view");
        let channel = Self::get_pd_channel(node.clone()).await?;

        let (worker, state_commitment_tree, error_slot, sync_height_rx, compliance_snapshot) =
            Worker::new(storage.clone(), channel.clone())
                .instrument(span.clone())
                .tap(|_| tracing::trace!("constructing view server worker"))
                .await?
                .tap(|_| tracing::debug!("constructed view server worker"));

        tokio::spawn(worker.run().instrument(span))
            .tap(|_| tracing::debug!("spawned view server worker"));
        tokio::spawn(
            HistoricalProofWorker::new(
                storage.clone(),
                channel,
                historical_prover,
                sync_height_rx.clone(),
            )
            .run(),
        );

        Ok(Self {
            storage,
            error_slot,
            sync_height_rx,
            state_commitment_tree,
            node,
            compliance_snapshot,
        })
    }

    /// Obtain a Tonic [Channel] to a remote `pd` endpoint.
    ///
    /// Provided as a convenience method for bootstrapping a connection.
    /// Handles configuring TLS if the URL is HTTPS. Also adds a tracing span
    /// to the working [Channel].
    pub async fn get_pd_channel(node: Url) -> anyhow::Result<Channel> {
        let endpoint = get_pd_endpoint(node).await?;
        let span = tracing::error_span!(parent: None, "view");
        let c: Channel = endpoint
            .connect()
            .instrument(span.clone())
            .await
            .with_context(|| "could not connect to grpc server")
            .tap_err(|error| tracing::error!(?error, "could not connect to grpc server"))?;

        Ok(c)
    }

    /// Checks if the view server worker has encountered an error.
    ///
    /// This function returns a gRPC [`tonic::Status`] containing the view server worker error if
    /// any exists, otherwise it returns `Ok(())`.
    #[instrument(level = "debug", skip_all)]
    async fn check_worker(&self) -> Result<(), tonic::Status> {
        // If the shared error slot is set, then an error has occurred in the worker
        // that we should bubble up.
        tracing::debug!("checking view server worker");
        if let Some(error) = self
            .error_slot
            .lock()
            .tap_err(|error| tracing::error!(?error, "unable to lock worker error slot"))
            .map_err(|e| {
                tonic::Status::unavailable(format!("unable to lock worker error slot {:#}", e))
            })?
            .as_ref()
        {
            return Err(tonic::Status::new(
                tonic::Code::Internal,
                format!("Worker failed: {error}"),
            ));
        }

        // TODO: check whether the worker is still alive, else fail, when we have a way to do that
        // (if the worker is to crash without setting the error_slot, the service should die as well)

        Ok(()).tap(|_| tracing::trace!("view server worker is healthy"))
    }

    /// Get a reference to the storage.
    ///
    /// This is useful for getting leaf data for local compliance proof generation.
    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    #[instrument(skip(self, transaction), fields(id = %transaction.id()))]
    fn broadcast_transaction(
        &self,
        transaction: Transaction,
        await_detection: bool,
    ) -> BroadcastTransactionStream {
        let self2 = self.clone();
        try_stream! {
                let transaction_id = transaction.id();
                let spent_nullifier = if await_detection {
                    transaction.spent_nullifiers().next()
                } else {
                    None
                };

                // 1. Broadcast the transaction to the network.
                // Note that "synchronous" here means "wait for the tx to be accepted by
                // the fullnode", not "wait for the tx to be included on chain.
                let mut fullnode_client = self2.tendermint_proxy_client().await
                            .map_err(|e| {
                                tonic::Status::unavailable(format!(
                                    "couldn't connect to fullnode: {:#?}",
                                    e
                                ))
                            })?
                        ;
                let node_rsp = fullnode_client
                    .broadcast_tx_sync(BroadcastTxSyncRequest {
                        params: transaction.encode_to_vec(),
                        req_id: OsRng.gen(),
                    })
                    .await;

                // A dropped HTTP response is ambiguous: CometBFT may have accepted
                // the transaction before the proxy observed the disconnect. Never
                // resubmit in that case. For transactions relevant to this view,
                // reconcile the exact transaction hash after its nullifier is
                // detected; otherwise preserve the original broadcast failure.
                let reconciled_detection_height = match node_rsp {
                    Ok(node_rsp) => {
                        let node_rsp = node_rsp.into_inner();
                        tracing::info!(?node_rsp);
                        match node_rsp.code {
                            0 => Ok(()),
                            _ => Err(tonic::Status::new(
                                tonic::Code::Internal,
                                format!(
                                    "Error submitting transaction: code {}, log: {}",
                                    node_rsp.code,
                                    node_rsp.log,
                                ),
                            )),
                        }?;
                        None
                    }
                    Err(broadcast_error) if broadcast_outcome_is_unknown(&broadcast_error) => {
                        match spent_nullifier {
                            Some(nullifier) => {
                                let original_error = format!(
                                    "error broadcasting tx: {:#?}",
                                    broadcast_error
                                );
                                tracing::warn!(
                                    ?transaction_id,
                                    ?broadcast_error,
                                    "broadcast response was ambiguous; awaiting exact transaction detection without resubmitting"
                                );
                                let detection = self2.storage.nullifier_status(nullifier, true);
                                tokio::time::timeout(
                                    BROADCAST_AMBIGUITY_RECONCILIATION_TIMEOUT,
                                    detection,
                                )
                                .await
                                .map_err(|_| {
                                    broadcast_outcome_unknown_status(format!(
                                        "{original_error}; transaction was not detected before the ambiguity timeout"
                                    ))
                                })?
                                .map_err(|error| {
                                    broadcast_outcome_unknown_status(format!(
                                        "{original_error}; error while reconciling the ambiguous broadcast: {error:#}"
                                    ))
                                })?;

                                let detected = self2.storage
                                    .transaction_by_hash(&transaction_id.0)
                                    .await
                                    .map_err(|error| {
                                        broadcast_outcome_unknown_status(format!(
                                            "{original_error}; error querying the reconciled transaction: {error:#}"
                                        ))
                                    })?
                                    .map(|(height, transaction)| (height, transaction.id()));
                                let height = resolve_broadcast_detection(
                                    broadcast_error,
                                    transaction_id,
                                    detected,
                                )?;
                                tracing::info!(
                                    ?transaction_id,
                                    height,
                                    "reconciled ambiguous broadcast from exact transaction detection"
                                );
                                Some(height)
                            }
                            None => resolve_broadcast_detection(
                                broadcast_error,
                                transaction_id,
                                None,
                            )
                            .map(Some)?,
                        }
                    }
                    Err(broadcast_error) => resolve_broadcast_detection(
                        broadcast_error,
                        transaction_id,
                        None,
                    )
                    .map(Some)?,
                };

                // The transaction was submitted so we provide a status update
                yield BroadcastTransactionResponse{ status: Some(BroadcastStatus::BroadcastSuccess(BroadcastSuccess{id:Some(transaction_id.into())}))};

                // 2. Optionally wait for the transaction to be detected by the view service.
                if reconciled_detection_height.is_none() {
                    if let Some(nullifier) = spent_nullifier {
                        tracing::info!(?nullifier, "waiting for detection of nullifier");
                        let detection = self2.storage.nullifier_status(nullifier, true);
                        tokio::time::timeout(BROADCAST_NULLIFIER_DETECTION_TIMEOUT, detection)
                            .await
                            .map_err(|_| {
                                tonic::Status::unavailable(
                                    "timeout waiting to detect nullifier of submitted transaction"
                                )
                            })?
                            .map_err(|_| {
                                tonic::Status::unavailable(
                                    "error while waiting for detection of submitted transaction"
                                )
                            })?;
                    }
                }

                let detection_height = match reconciled_detection_height {
                    Some(height) => height,
                    None => self2.storage
                        .transaction_by_hash(&transaction_id.0)
                        .await
                        .map_err(|e| tonic::Status::internal(format!("error querying storage: {:#}", e)))?
                        .map(|(height, _tx)| height)
                        // If we didn't find it for some reason, return 0 for unknown.
                        // TODO: how does this change if we detach extended transaction fetch from scanning?
                        .unwrap_or(0),
                };
                yield BroadcastTransactionResponse{ status: Some(BroadcastStatus::Confirmed(Confirmed{id:Some(transaction_id.into()), detection_height}))};
            }.boxed()
    }

    #[instrument(level = "trace", skip(self))]
    async fn tendermint_proxy_client(
        &self,
    ) -> anyhow::Result<TendermintProxyServiceClient<Channel>> {
        TendermintProxyServiceClient::connect(self.node.to_string())
            .tap(|_| tracing::debug!("connecting to tendermint proxy"))
            .await
            .tap_err(|error| tracing::error!(?error, "failed to connect to tendermint proxy"))
            .map_err(anyhow::Error::from)
    }

    /// Return the latest block height known by the fullnode or its peers, as
    /// well as whether the fullnode is caught up with that height.
    #[instrument(skip(self))]
    pub async fn latest_known_block_height(&self) -> anyhow::Result<(u64, bool)> {
        let mut client = self.tendermint_proxy_client().await?;

        let GetStatusResponse { sync_info, .. } = client
            .get_status(GetStatusRequest {})
            .tap(|_| tracing::debug!("querying current status"))
            .await
            .tap_err(|error| tracing::debug!(?error, "failed to query current status"))?
            .into_inner();

        let SyncInfo {
            latest_block_height,
            catching_up,
            ..
        } = sync_info
            .ok_or_else(|| anyhow::anyhow!("could not parse sync_info in gRPC response"))?;

        // There is a `max_peer_block_height` available in TM 0.35, however it should not be used
        // as it does not seem to reflect the consensus height. Since clients use `latest_known_block_height`
        // to determine the height to attempt syncing to, a validator reporting a non-consensus height
        // can cause a DoS to clients attempting to sync if `max_peer_block_height` is used.
        let latest_known_block_height = latest_block_height;

        tracing::debug!(
            ?latest_block_height,
            ?catching_up,
            ?latest_known_block_height,
            "found latest known block height"
        );

        Ok((latest_known_block_height, catching_up))
    }

    #[instrument(skip(self))]
    pub async fn status(&self) -> anyhow::Result<StatusResponse> {
        let sync_height = self.storage.last_sync_height().await?.unwrap_or(0);

        let (latest_known_block_height, node_catching_up) =
            self.latest_known_block_height().await?;

        let height_diff = latest_known_block_height
            .checked_sub(sync_height)
            .ok_or_else(|| anyhow!("sync height ahead of node height"))?;

        let catching_up = match (node_catching_up, height_diff) {
            // We're synced to the same height as the node
            (false, 0) => false,
            // We're one block behind, and will learn about it soon, close enough
            (false, 1) => false,
            // We're behind the node
            (false, _) => true,
            // The node is behind the network
            (true, _) => true,
        };

        Ok(StatusResponse {
            sync_height,
            catching_up,
        })
    }
}

#[async_trait]
impl ViewService for ViewServer {
    type NotesStream =
        Pin<Box<dyn futures::Stream<Item = Result<pb::NotesResponse, tonic::Status>> + Send>>;
    type AssetsStream =
        Pin<Box<dyn futures::Stream<Item = Result<pb::AssetsResponse, tonic::Status>> + Send>>;
    type StatusStreamStream = Pin<
        Box<dyn futures::Stream<Item = Result<pb::StatusStreamResponse, tonic::Status>> + Send>,
    >;
    type TransactionInfoStream = Pin<
        Box<dyn futures::Stream<Item = Result<pb::TransactionInfoResponse, tonic::Status>> + Send>,
    >;
    type BalancesStream =
        Pin<Box<dyn futures::Stream<Item = Result<pb::BalancesResponse, tonic::Status>> + Send>>;
    type BroadcastTransactionStream = BroadcastTransactionStream;
    type WitnessAndBuildStream = Pin<
        Box<dyn futures::Stream<Item = Result<pb::WitnessAndBuildResponse, tonic::Status>> + Send>,
    >;
    type AuthorizeAndBuildStream = Pin<
        Box<
            dyn futures::Stream<Item = Result<pb::AuthorizeAndBuildResponse, tonic::Status>> + Send,
        >,
    >;

    #[instrument(skip_all, level = "trace")]
    async fn broadcast_transaction(
        &self,
        request: tonic::Request<pb::BroadcastTransactionRequest>,
    ) -> Result<tonic::Response<Self::BroadcastTransactionStream>, tonic::Status> {
        let pb::BroadcastTransactionRequest {
            transaction,
            await_detection,
        } = request.into_inner();

        let transaction: Transaction = transaction
            .ok_or_else(|| tonic::Status::invalid_argument("missing transaction"))?
            .try_into()
            .map_err(|e: anyhow::Error| e.context("could not decode transaction"))
            .map_err(|e| tonic::Status::invalid_argument(format!("{:#}", e)))?;

        let stream = self.broadcast_transaction(transaction, await_detection);

        Ok(tonic::Response::new(stream))
    }

    #[instrument(skip_all, level = "trace")]
    async fn transaction_planner(
        &self,
        request: tonic::Request<pb::TransactionPlannerRequest>,
    ) -> Result<tonic::Response<pb::TransactionPlannerResponse>, tonic::Status> {
        let prq = request.into_inner();

        let gas_prices =
            self.storage.gas_prices().await.map_err(|e| {
                tonic::Status::internal(format!("could not get gas prices: {:#}", e))
            })?;

        let source = prq
            .source
            .as_ref()
            .map(|addr_index| addr_index.account)
            .unwrap_or(0u32);

        if !prq.outputs.is_empty() {
            if !prq.ibc_relay_actions.is_empty()
                || !prq.ics20_withdrawals.is_empty()
                || !prq.host_withdrawals.is_empty()
            {
                return Err(tonic::Status::invalid_argument(
                    "wallet-facing shielded transfer intents cannot be mixed with other action types in transaction_planner",
                ));
            }
            if prq.outputs.len() != 1 {
                return Err(tonic::Status::invalid_argument(
                    "wallet-facing shielded transfer supports exactly one external recipient",
                ));
            }

            let output = prq
                .outputs
                .into_iter()
                .next()
                .expect("checked exactly one output");
            let address: Address = output
                .address
                .ok_or_else(|| tonic::Status::invalid_argument("Missing address"))?
                .try_into()
                .map_err(|e| {
                    tonic::Status::invalid_argument(format!("Could not parse address: {e:#}"))
                })?;
            let value: Value = output
                .value
                .ok_or_else(|| tonic::Status::invalid_argument("Missing value"))?
                .try_into()
                .map_err(|e| {
                    tonic::Status::invalid_argument(format!("Could not parse value: {e:#}"))
                })?;

            let mut note_manager = NoteManager::new(OsRng);
            note_manager
                .set_gas_prices(gas_prices)
                .expiry_height(prq.expiry_height);
            if let Some(memo) = prq.memo {
                note_manager.memo(memo.text);
                if let Some(return_address) = memo.return_address {
                    note_manager.memo_return_address(return_address.try_into().map_err(|e| {
                        tonic::Status::invalid_argument(format!(
                            "Could not parse memo return address: {e:#}"
                        ))
                    })?);
                }
            }

            let mut planning_io = LocalPlanningIo(self);
            let planning_result = note_manager
                .plan_transfer(&mut planning_io, source.into(), value, address)
                .await
                .context("could not plan wallet-facing shielded transfer")
                .map_err(|e| tonic::Status::invalid_argument(format!("{e:#}")))?;

            let transaction_plan = planning_result_to_rpc(
                planning_result,
                "wallet-facing shielded transfer requires note maintenance first",
                "insufficient balance for requested transfer",
            )?;

            return Ok(tonic::Response::new(TransactionPlannerResponse {
                plan: Some(transaction_plan.into()),
            }));
        }

        if !prq.ics20_withdrawals.is_empty() {
            if !prq.ibc_relay_actions.is_empty()
                || !prq.outputs.is_empty()
                || !prq.host_withdrawals.is_empty()
            {
                return Err(tonic::Status::invalid_argument(
                    "wallet-facing ICS-20 withdrawal intents cannot be mixed with other action types in transaction_planner",
                ));
            }
            if prq.ics20_withdrawals.len() != 1 {
                return Err(tonic::Status::invalid_argument(
                    "wallet-facing ICS-20 withdrawal planner supports exactly one outbound withdrawal",
                ));
            }

            let withdrawal: shieldd_sdk_shielded_pool::Ics20Withdrawal = prq
                .ics20_withdrawals
                .into_iter()
                .next()
                .expect("checked exactly one ICS-20 withdrawal")
                .try_into()
                .map_err(|e| {
                    tonic::Status::invalid_argument(format!(
                        "Could not parse ICS-20 withdrawal: {e:#}"
                    ))
                })?;

            let mut note_manager = NoteManager::new(OsRng);
            note_manager
                .set_gas_prices(gas_prices)
                .expiry_height(prq.expiry_height);

            let mut planning_io = LocalPlanningIo(self);
            let planning_result = note_manager
                .plan_ics20_withdrawal(&mut planning_io, source.into(), withdrawal)
                .await
                .context("could not plan wallet-facing ICS-20 withdrawal")
                .map_err(|e| tonic::Status::invalid_argument(format!("{e:#}")))?;

            let transaction_plan = planning_result_to_rpc(
                planning_result,
                "wallet-facing ICS-20 withdrawal requires note maintenance first",
                "insufficient balance for requested ICS-20 withdrawal",
            )?;

            return Ok(tonic::Response::new(TransactionPlannerResponse {
                plan: Some(transaction_plan.into()),
            }));
        }

        if !prq.host_withdrawals.is_empty() {
            if !prq.ibc_relay_actions.is_empty()
                || !prq.outputs.is_empty()
                || !prq.ics20_withdrawals.is_empty()
            {
                return Err(tonic::Status::invalid_argument(
                    "wallet-facing host withdrawal intents cannot be mixed with other action types in transaction_planner",
                ));
            }
            if prq.host_withdrawals.len() != 1 {
                return Err(tonic::Status::invalid_argument(
                    "wallet-facing host withdrawal planner supports exactly one outbound withdrawal",
                ));
            }

            let withdrawal: shieldd_sdk_shielded_pool::HostWithdrawal = prq
                .host_withdrawals
                .into_iter()
                .next()
                .expect("checked exactly one host withdrawal")
                .try_into()
                .map_err(|e| {
                    tonic::Status::invalid_argument(format!(
                        "Could not parse host withdrawal: {e:#}"
                    ))
                })?;

            let mut note_manager = NoteManager::new(OsRng);
            note_manager
                .set_gas_prices(gas_prices)
                .expiry_height(prq.expiry_height);

            let mut planning_io = LocalPlanningIo(self);
            let planning_result = note_manager
                .plan_host_withdrawal(&mut planning_io, source.into(), withdrawal)
                .await
                .context("could not plan wallet-facing host withdrawal")
                .map_err(|e| tonic::Status::invalid_argument(format!("{e:#}")))?;

            let transaction_plan = planning_result_to_rpc(
                planning_result,
                "wallet-facing host withdrawal requires note maintenance first",
                "insufficient balance for requested host withdrawal",
            )?;

            return Ok(tonic::Response::new(TransactionPlannerResponse {
                plan: Some(transaction_plan.into()),
            }));
        }

        if !prq.ibc_relay_actions.is_empty() && prq.outputs.is_empty() {
            let mut note_manager = NoteManager::new(OsRng);
            note_manager
                .set_gas_prices(gas_prices)
                .expiry_height(prq.expiry_height);
            if let Some(memo) = prq.memo {
                note_manager.memo(memo.text);
                if let Some(return_address) = memo.return_address {
                    note_manager.memo_return_address(return_address.try_into().map_err(|e| {
                        tonic::Status::invalid_argument(format!(
                            "Could not parse memo return address: {e:#}"
                        ))
                    })?);
                }
            }

            let actions = prq
                .ibc_relay_actions
                .into_iter()
                .map(|ibc_action| {
                    ibc_action
                        .try_into()
                        .map(ActionPlan::IbcAction)
                        .map_err(|e| tonic::Status::invalid_argument(format!("{e:#}")))
                })
                .collect::<Result<Vec<_>, _>>()?;

            let mut planning_io = LocalPlanningIo(self);
            let planning_result = note_manager
                .plan_actions_with_transfer_funding(&mut planning_io, source.into(), actions)
                .await
                .context("could not plan wallet-facing IBC relay transaction")
                .map_err(|e| tonic::Status::invalid_argument(format!("{e:#}")))?;

            let transaction_plan = planning_result_to_rpc(
                planning_result,
                "wallet-facing IBC relay transaction requires note maintenance first",
                "insufficient balance for requested IBC relay transaction",
            )?;

            return Ok(tonic::Response::new(TransactionPlannerResponse {
                plan: Some(transaction_plan.into()),
            }));
        }

        Err(tonic::Status::invalid_argument(
            "transaction_planner only supports wallet-facing transfer, ICS-20 withdrawal, host withdrawal, and transfer-funded IBC relay intents",
        ))
    }

    #[instrument(skip_all, level = "trace")]
    async fn address_by_index(
        &self,
        request: tonic::Request<pb::AddressByIndexRequest>,
    ) -> Result<tonic::Response<pb::AddressByIndexResponse>, tonic::Status> {
        let fvk =
            self.storage.full_viewing_key().await.map_err(|_| {
                tonic::Status::failed_precondition("Error retrieving full viewing key")
            })?;

        let request = request.into_inner();
        let purpose = Self::address_purpose(request.purpose)?;
        let address_index = request
            .address_index
            .ok_or_else(|| tonic::Status::invalid_argument("Missing address index"))?
            .try_into()
            .map_err(|e| {
                tonic::Status::invalid_argument(format!("Could not parse address index: {e:#}"))
            })?;

        let address = self
            .persist_issued_address(address_index, fvk.payment_address(address_index), purpose)
            .await?;
        Ok(tonic::Response::new(pb::AddressByIndexResponse {
            address: Some(address.into()),
        }))
    }

    #[instrument(skip_all, level = "trace")]
    async fn index_by_address(
        &self,
        request: tonic::Request<pb::IndexByAddressRequest>,
    ) -> Result<tonic::Response<pb::IndexByAddressResponse>, tonic::Status> {
        let fvk =
            self.storage.full_viewing_key().await.map_err(|_| {
                tonic::Status::failed_precondition("Error retrieving full viewing key")
            })?;

        let address: Address = request
            .into_inner()
            .address
            .ok_or_else(|| tonic::Status::invalid_argument("Missing address"))?
            .try_into()
            .map_err(|e| {
                tonic::Status::invalid_argument(format!("Could not parse address: {e:#}"))
            })?;

        Ok(tonic::Response::new(pb::IndexByAddressResponse {
            address_index: fvk.address_index(&address).map(Into::into),
        }))
    }
    async fn transparent_address(
        &self,
        _request: tonic::Request<pb::TransparentAddressRequest>,
    ) -> Result<tonic::Response<pb::TransparentAddressResponse>, tonic::Status> {
        let fvk =
            self.storage.full_viewing_key().await.map_err(|_| {
                tonic::Status::failed_precondition("Error retrieving full viewing key")
            })?;

        let encoding = fvk.incoming().transparent_address();
        let address: Address = encoding
            .parse()
            .map_err(|_| tonic::Status::internal("could not parse newly generated address"))?;
        let address_index = fvk.address_index(&address).ok_or_else(|| {
            tonic::Status::internal("could not recover transparent address index")
        })?;
        let address = self
            .persist_issued_address(address_index, address, AddressPurpose::General)
            .await?;

        Ok(tonic::Response::new(pb::TransparentAddressResponse {
            address: Some(address.into()),
            encoding,
        }))
    }

    #[instrument(skip_all, level = "trace")]
    async fn ephemeral_address(
        &self,
        request: tonic::Request<pb::EphemeralAddressRequest>,
    ) -> Result<tonic::Response<pb::EphemeralAddressResponse>, tonic::Status> {
        let fvk =
            self.storage.full_viewing_key().await.map_err(|_| {
                tonic::Status::failed_precondition("Error retrieving full viewing key")
            })?;

        let request = request.into_inner();
        let purpose = Self::address_purpose(request.purpose)?;
        let account_index: AddressIndex = request
            .address_index
            .ok_or_else(|| tonic::Status::invalid_argument("Missing address index"))?
            .try_into()
            .map_err(|e| {
                tonic::Status::invalid_argument(format!("Could not parse address index: {e:#}"))
            })?;

        let address_index = AddressIndex::new_ephemeral(account_index.account, OsRng);
        let address = self
            .persist_issued_address(address_index, fvk.payment_address(address_index), purpose)
            .await?;
        Ok(tonic::Response::new(pb::EphemeralAddressResponse {
            address: Some(address.into()),
        }))
    }

    #[instrument(skip_all, level = "trace")]
    async fn transaction_info_by_hash(
        &self,
        request: tonic::Request<pb::TransactionInfoByHashRequest>,
    ) -> Result<tonic::Response<pb::TransactionInfoByHashResponse>, tonic::Status> {
        self.check_worker().await?;

        let request = request.into_inner();

        let fvk =
            self.storage.full_viewing_key().await.map_err(|_| {
                tonic::Status::failed_precondition("Error retrieving full viewing key")
            })?;

        let maybe_tx = self
            .storage
            .transaction_by_hash(
                &request
                    .id
                    .clone()
                    .ok_or_else(|| {
                        tonic::Status::invalid_argument(
                            "missing transaction ID in TransactionInfoByHashRequest",
                        )
                    })?
                    .inner,
            )
            .await
            .map_err(|_| {
                tonic::Status::failed_precondition(format!(
                    "Error retrieving transaction by hash {}",
                    hex::encode(request.id.expect("transaction id is present").inner)
                ))
            })?;

        let Some((height, tx)) = maybe_tx else {
            return Ok(tonic::Response::new(
                pb::TransactionInfoByHashResponse::default(),
            ));
        };

        // First, create a TxP with the payload keys visible to our FVK and no other data.
        let mut txp = TransactionPerspective {
            payload_keys: tx
                .payload_keys(&fvk)
                .map_err(|_| tonic::Status::failed_precondition("Error generating payload keys"))?,
            ..Default::default()
        };

        // Next, extend the TxP with the openings of commitments known to our view server
        // but not included in the transaction body, for instance spent notes.
        for action in tx.actions() {
            use shieldd_sdk_transaction::Action;
            match action {
                Action::Transfer(transfer) => {
                    for input in &transfer.body.inputs {
                        let nullifier = input.nullifier;
                        if let Ok(spendable_note_record) =
                            self.storage.note_by_nullifier(nullifier, false).await
                        {
                            txp.spend_nullifiers
                                .insert(nullifier, spendable_note_record.note);
                        }
                    }
                }
                Action::NoteReshape(note_reshape) => {
                    for input in &note_reshape.body.inputs {
                        let nullifier = input.nullifier;
                        if let Ok(spendable_note_record) =
                            self.storage.note_by_nullifier(nullifier, false).await
                        {
                            txp.spend_nullifiers
                                .insert(nullifier, spendable_note_record.note);
                        }
                    }
                }
                Action::ShieldedIcs20Withdrawal(withdrawal) => {
                    for input in &withdrawal.body.inputs {
                        let nullifier = input.nullifier;
                        if let Ok(spendable_note_record) =
                            self.storage.note_by_nullifier(nullifier, false).await
                        {
                            txp.spend_nullifiers
                                .insert(nullifier, spendable_note_record.note);
                        }
                    }
                }
                Action::ShieldedHostWithdrawal(withdrawal) => {
                    for input in &withdrawal.body.inputs {
                        let nullifier = input.nullifier;
                        if let Ok(spendable_note_record) =
                            self.storage.note_by_nullifier(nullifier, false).await
                        {
                            txp.spend_nullifiers
                                .insert(nullifier, spendable_note_record.note);
                        }
                    }
                }
                _ => {}
            }
        }

        // Now, generate a stub TxV from our minimal TxP, and inspect it to see what data we should
        // augment the minimal TxP with to provide additional context (e.g., filling in denoms for
        // visible asset IDs).
        let min_view = tx.view_from_perspective(&txp);
        let mut address_views = BTreeMap::new();
        let mut asset_ids = BTreeSet::new();
        for action_view in min_view.action_views() {
            use shieldd_sdk_transaction::view::action_view::ActionView;
            match action_view {
                ActionView::Transfer(
                    shieldd_sdk_transaction::view::action_view::TransferView::Visible {
                        spent_notes,
                        created_notes,
                        ..
                    },
                )
                | ActionView::NoteReshape(
                    shieldd_sdk_transaction::view::action_view::NoteReshapeView::Visible {
                        spent_notes,
                        created_notes,
                        ..
                    },
                ) => {
                    for note in spent_notes.iter().chain(created_notes.iter()) {
                        let address = note.address();
                        address_views.insert(address.clone(), fvk.view_address(address));
                        asset_ids.insert(note.asset_id());
                    }
                    if let Ok(memo) = tx.decrypt_memo(&fvk) {
                        let return_address = memo.return_address();
                        address_views
                            .insert(return_address.clone(), fvk.view_address(return_address));
                    }
                }
                ActionView::ShieldedIcs20Withdrawal(
                    shieldd_sdk_shielded_pool::ShieldedIcs20WithdrawalView::Visible {
                        spent_notes,
                        change_note,
                        ..
                    },
                ) => {
                    for note in spent_notes.iter().chain(change_note.iter()) {
                        let address = note.address();
                        address_views.insert(address.clone(), fvk.view_address(address));
                        asset_ids.insert(note.asset_id());
                    }
                    if let Ok(memo) = tx.decrypt_memo(&fvk) {
                        let return_address = memo.return_address();
                        address_views
                            .insert(return_address.clone(), fvk.view_address(return_address));
                    }
                }
                ActionView::ShieldedHostWithdrawal(
                    shieldd_sdk_shielded_pool::ShieldedHostWithdrawalView::Visible {
                        spent_notes,
                        change_note,
                        ..
                    },
                ) => {
                    for note in spent_notes
                        .iter()
                        .chain(std::slice::from_ref(change_note).iter())
                    {
                        let address = note.address();
                        address_views.insert(address.clone(), fvk.view_address(address));
                        asset_ids.insert(note.asset_id());
                    }
                    if let Ok(memo) = tx.decrypt_memo(&fvk) {
                        let return_address = memo.return_address();
                        address_views
                            .insert(return_address.clone(), fvk.view_address(return_address));
                    }
                }
                _ => {}
            }
        }

        // Now, extend the TxV with information helpful to understand the data it can view:

        let mut denoms = Vec::new();

        for id in asset_ids {
            if let Some(asset) = self.storage.asset_by_id(&id).await.map_err(|e| {
                tonic::Status::internal(format!("Error retrieving asset by id: {:#}", e))
            })? {
                denoms.push(asset);
            }
        }

        txp.denoms.extend(denoms);

        txp.address_views = address_views.into_values().collect();

        // Finally, compute the full TxV from the full TxP:
        let txv = tx.view_from_perspective(&txp);
        let summary = txv.summary();

        let response = pb::TransactionInfoByHashResponse {
            tx_info: Some(pb::TransactionInfo {
                height,
                id: Some(tx.id().into()),
                perspective: Some(txp.into()),
                transaction: Some(tx.into()),
                view: Some(txv.into()),
                summary: Some(summary.into()),
            }),
        };

        Ok(tonic::Response::new(response))
    }

    #[allow(deprecated)]
    #[instrument(skip(self, request))]
    async fn balances(
        &self,
        request: tonic::Request<pb::BalancesRequest>,
    ) -> Result<tonic::Response<Self::BalancesStream>, tonic::Status> {
        let request = request.into_inner();

        let account_filter = request.account_filter.and_then(|x| {
            AddressIndex::try_from(x)
                .map_err(|_| {
                    tonic::Status::failed_precondition("invalid account filter in request")
                })
                .map_or(None, |x| x.into())
        });

        let asset_id_filter = request.asset_id_filter.and_then(|x| {
            asset::Id::try_from(x)
                .map_err(|_| tonic::Status::failed_precondition("invalid asset filter in request"))
                .map_or(None, |x| x.into())
        });

        let result = self
            .storage
            .balances(account_filter, asset_id_filter)
            .await
            .map_err(|e| tonic::Status::internal(format!("error: {e}")))?;

        tracing::debug!(?account_filter, ?asset_id_filter, ?result);

        let self2 = self.clone();
        let stream = try_stream! {
            // retrieve balance and address views
            for element in result {
                let metadata: Metadata = self2
                    .asset_metadata_by_id(Request::new(pb::AssetMetadataByIdRequest {
                        asset_id: Some(element.id.into()),
                    }))
                    .await?
                    .into_inner()
                    .denom_metadata
                    .context("denom metadata not found")?
                    .try_into()?;

                 let value = Value {
                    asset_id: element.id,
                    amount: element.amount.into(),
                };

                let value_view = value.view_with_denom(metadata)?;

                let address: Address = self2
                  .address_by_index(Request::new(pb::AddressByIndexRequest {
                       address_index: account_filter.map(Into::into),
                       purpose: None,
                   }))
                   .await?
                    .into_inner()
                    .address
                    .context("address not found")?
                    .try_into()?;

                 let wallet_id: WalletId = self2
                            .wallet_id(Request::new(pb::WalletIdRequest {}))
                            .await?
                            .into_inner()
                            .wallet_id
                            .context("wallet id not found")?
                            .try_into()?;

                let address_view = AddressView::Decoded {
                    address,
                    index: element.address_index,
                    wallet_id,
                };

                yield pb::BalancesResponse {
                    account_address: Some(address_view.into()),
                    balance_view: Some(value_view.into()),
                    balance: None,
                    account: None,
                }
            }
        };

        Ok(tonic::Response::new(
            stream
                .map_err(|e: anyhow::Error| {
                    tonic::Status::unavailable(format!("error getting balances: {e}"))
                })
                .boxed(),
        ))
    }

    #[instrument(skip_all, level = "trace")]
    async fn note_by_commitment(
        &self,
        request: tonic::Request<pb::NoteByCommitmentRequest>,
    ) -> Result<tonic::Response<pb::NoteByCommitmentResponse>, tonic::Status> {
        self.check_worker().await?;

        let request = request.into_inner();

        let note_commitment = request
            .note_commitment
            .ok_or_else(|| {
                tonic::Status::failed_precondition("Missing note commitment in request")
            })?
            .try_into()
            .map_err(|_| {
                tonic::Status::failed_precondition("Invalid note commitment in request")
            })?;

        let spendable_note = pb::SpendableNoteRecord::from(
            self.storage
                .note_by_commitment(note_commitment, request.await_detection)
                .await
                .map_err(|e| tonic::Status::internal(format!("error: {e}")))?,
        );

        Ok(tonic::Response::new(NoteByCommitmentResponse {
            spendable_note: Some(spendable_note),
        }))
    }

    #[instrument(skip_all, level = "trace")]
    async fn nullifier_status(
        &self,
        request: tonic::Request<pb::NullifierStatusRequest>,
    ) -> Result<tonic::Response<pb::NullifierStatusResponse>, tonic::Status> {
        self.check_worker().await?;

        let request = request.into_inner();

        let nullifier = request
            .nullifier
            .ok_or_else(|| tonic::Status::failed_precondition("Missing nullifier in request"))?
            .try_into()
            .map_err(|_| tonic::Status::failed_precondition("Invalid nullifier in request"))?;

        Ok(tonic::Response::new(pb::NullifierStatusResponse {
            spent: self
                .storage
                .nullifier_status(nullifier, request.await_detection)
                .await
                .map_err(|e| tonic::Status::internal(format!("error: {e}")))?,
        }))
    }

    #[instrument(skip_all, level = "trace")]
    async fn status(
        &self,
        _: tonic::Request<pb::StatusRequest>,
    ) -> Result<tonic::Response<pb::StatusResponse>, tonic::Status> {
        self.check_worker().await?;

        Ok(tonic::Response::new(self.status().await.map_err(|e| {
            tonic::Status::internal(format!("error: {e}"))
        })?))
    }

    #[instrument(skip_all, level = "trace")]
    async fn status_stream(
        &self,
        _: tonic::Request<pb::StatusStreamRequest>,
    ) -> Result<tonic::Response<Self::StatusStreamStream>, tonic::Status> {
        self.check_worker().await?;

        let (latest_known_block_height, _) = self
            .latest_known_block_height()
            .await
            .tap_err(|error| {
                tracing::debug!(
                    ?error,
                    "unable to fetch latest known block height from fullnode"
                )
            })
            .map_err(|e| {
                tonic::Status::unknown(format!(
                    "unable to fetch latest known block height from fullnode: {e}"
                ))
            })?;

        // Create a stream of sync height updates from our worker, and send them to the client
        // until we've reached the latest known block height at the time the request was made.
        let mut sync_height_stream = WatchStream::new(self.sync_height_rx.clone());
        let stream = try_stream! {
            while let Some(sync_height) = sync_height_stream.next().await {
                yield pb::StatusStreamResponse {
                    latest_known_block_height,
                    sync_height: sync_height,
                };
                if sync_height >= latest_known_block_height {
                    break;
                }
            }
        };

        Ok(tonic::Response::new(stream.boxed()))
    }

    #[instrument(skip_all, level = "trace")]
    async fn notes(
        &self,
        request: tonic::Request<pb::NotesRequest>,
    ) -> Result<tonic::Response<Self::NotesStream>, tonic::Status> {
        self.check_worker().await?;

        let request = request.into_inner();

        let include_spent = request.include_spent;
        let asset_id = request
            .asset_id
            .to_owned()
            .map(asset::Id::try_from)
            .map_or(Ok(None), |v| v.map(Some))
            .map_err(|_| tonic::Status::invalid_argument("invalid asset id"))?;
        let address_index = request
            .address_index
            .to_owned()
            .map(AddressIndex::try_from)
            .map_or(Ok(None), |v| v.map(Some))
            .map_err(|_| tonic::Status::invalid_argument("invalid address index"))?;

        let amount_to_spend = request
            .amount_to_spend
            .map(Amount::try_from)
            .map_or(Ok(None), |v| v.map(Some))
            .map_err(|_| tonic::Status::invalid_argument("invalid amount to spend"))?;

        let notes = self
            .storage
            .notes(include_spent, asset_id, address_index, amount_to_spend)
            .await
            .map_err(|e| tonic::Status::unavailable(format!("error fetching notes: {e}")))?;

        let stream = try_stream! {
            for note in notes {
                yield pb::NotesResponse {
                    note_record: Some(note.into()),
                }
            }
        };

        Ok(tonic::Response::new(
            stream
                .map_err(|e: anyhow::Error| {
                    tonic::Status::unavailable(format!("error getting notes: {e}"))
                })
                .boxed(),
        ))
    }

    #[instrument(skip_all, level = "trace")]
    async fn assets(
        &self,
        request: tonic::Request<pb::AssetsRequest>,
    ) -> Result<tonic::Response<Self::AssetsStream>, tonic::Status> {
        self.check_worker().await?;

        let pb::AssetsRequest {
            filtered,
            include_specific_denominations,
            include_proposal_nfts,
            include_voting_receipt_tokens,
        } = request.get_ref();

        // Fetch assets from storage.
        let assets = if !filtered {
            self.storage
                .all_assets()
                .await
                .map_err(|e| tonic::Status::unavailable(format!("error fetching assets: {e}")))?
        } else {
            let mut assets = vec![];
            for denom in include_specific_denominations {
                if let Some(denom) = asset::REGISTRY.parse_denom(&denom.denom) {
                    assets.push(denom);
                }
            }
            for (include, pattern) in [
                (include_proposal_nfts, "proposal\\_%"),
                (include_voting_receipt_tokens, "voted\\_on\\_%"),
            ] {
                if *include {
                    assets.extend(
                        self.storage
                            .assets_matching(pattern.to_string())
                            .await
                            .map_err(|e| {
                                tonic::Status::unavailable(format!("error fetching assets: {e}"))
                            })?,
                    );
                }
            }
            assets
        };

        let stream = try_stream! {
            for asset in assets {
                yield
                    pb::AssetsResponse {
                        denom_metadata: Some(asset.into()),
                    }
            }
        };

        Ok(tonic::Response::new(
            stream
                .map_err(|e: anyhow::Error| {
                    tonic::Status::unavailable(format!("error getting assets: {e}"))
                })
                .boxed(),
        ))
    }

    #[instrument(skip_all, level = "trace")]
    async fn transaction_info(
        &self,
        request: tonic::Request<pb::TransactionInfoRequest>,
    ) -> Result<tonic::Response<Self::TransactionInfoStream>, tonic::Status> {
        self.check_worker().await?;
        // Unpack optional start/end heights.
        let start_height = if request.get_ref().start_height == 0 {
            None
        } else {
            Some(request.get_ref().start_height)
        };
        let end_height = if request.get_ref().end_height == 0 {
            None
        } else {
            Some(request.get_ref().end_height)
        };

        // Fetch transactions from storage.
        let txs = self
            .storage
            .transactions(start_height, end_height)
            .await
            .map_err(|e| tonic::Status::unavailable(format!("error fetching transactions: {e}")))?;

        let self2 = self.clone();
        let stream = try_stream! {
            for tx in txs {

                let rsp = self2.transaction_info_by_hash(tonic::Request::new(pb::TransactionInfoByHashRequest {
                    id: Some(tx.2.id().into()),
                })).await?.into_inner();

                yield pb::TransactionInfoResponse {
                    tx_info: rsp.tx_info,
                }
            }
        };

        Ok(tonic::Response::new(
            stream
                .map_err(|e: anyhow::Error| {
                    tonic::Status::unavailable(format!("error getting transactions: {e}"))
                })
                .boxed(),
        ))
    }

    #[instrument(skip_all, level = "trace")]
    async fn witness(
        &self,
        request: tonic::Request<pb::WitnessRequest>,
    ) -> Result<tonic::Response<WitnessResponse>, tonic::Status> {
        self.check_worker().await?;

        // Acquire a read lock for the SCT that will live for the entire request,
        // so that all auth paths are relative to the same SCT root.
        let sct = self.state_commitment_tree.read().await;

        // Read the SCT root
        let anchor = sct.root();

        // Obtain an auth path for each requested note commitment
        let tx_plan: TransactionPlan = request
            .get_ref()
            .to_owned()
            .transaction_plan
            .map(TryInto::try_into)
            .transpose()
            .map_err(|error| {
                tonic::Status::invalid_argument(format!("invalid transaction plan: {error}"))
            })?
            .unwrap_or_default();

        let zero_amount = 0u64.into();
        let all_spend_notes = || {
            tx_plan
                .actions
                .iter()
                .flat_map(|action| action.spends())
                .chain(tx_plan.fee_funding.iter().flat_map(|f| &f.transfer.spends))
        };

        let real_spend_count = all_spend_notes()
            .filter(|spend| spend.note.amount() != zero_amount)
            .count();
        let mut historical_nullifier_proofs = Vec::new();
        if real_spend_count > 0 {
            let plan_window = tx_plan.nullifier_window.ok_or_else(|| {
                tonic::Status::invalid_argument("spend-bearing plan is missing nullifier window")
            })?;
            let current_window = self.storage.nullifier_window().await.map_err(|error| {
                tonic::Status::unavailable(format!("error getting nullifier window: {error}"))
            })?;
            if plan_window != current_window {
                return Err(tonic::Status::failed_precondition(
                    "transaction plan nullifier window is stale",
                ));
            }
            let fvk = self.storage.full_viewing_key().await.map_err(|error| {
                tonic::Status::unavailable(format!("error getting full viewing key: {error}"))
            })?;
            for spend in all_spend_notes().filter(|spend| {
                spend.note.amount() != zero_amount
                    && u64::from(spend.position) < plan_window.recent_position_floor
            }) {
                let nullifier = spend.nullifier(&fvk);
                let cache = self
                    .storage
                    .historical_proof_cache(nullifier)
                    .await
                    .map_err(|error| {
                        tonic::Status::unavailable(format!(
                            "error loading historical proof cache: {error}"
                        ))
                    })?
                    .ok_or_else(|| {
                        tonic::Status::failed_precondition(format!(
                            "historical proof cache is missing for {nullifier}"
                        ))
                    })?;
                historical_nullifier_proofs.push(cache.bundle_for(plan_window).map_err(
                    |error| {
                        tonic::Status::failed_precondition(format!(
                            "historical proof cache is not ready for {nullifier}: {error}"
                        ))
                    },
                )?);
            }
        }

        let requested_note_commitments: Vec<StateCommitment> = all_spend_notes()
            .filter(|spend| spend.note.amount() != zero_amount)
            .map(|spend| spend.note.commit().into())
            .collect();

        tracing::debug!(?requested_note_commitments);

        let auth_paths: Vec<Proof> = requested_note_commitments
            .iter()
            .map(|nc| {
                sct.witness(*nc).ok_or_else(|| {
                    tonic::Status::new(tonic::Code::InvalidArgument, "Note commitment missing")
                })
            })
            .collect::<Result<Vec<Proof>, tonic::Status>>()?;

        // Release the read lock on the SCT
        drop(sct);

        let mut witness_data = WitnessData {
            anchor,
            state_commitment_proofs: auth_paths
                .into_iter()
                .map(|proof| (proof.commitment(), proof))
                .collect(),
            historical_nullifier_proofs,
        };

        tracing::debug!(?witness_data);

        // Now we need to augment the witness data with dummy proofs such that
        // note commitments corresponding to dummy spends also have proofs.
        for nc in all_spend_notes()
            .filter(|spend| spend.note.amount() == zero_amount)
            .map(|spend| spend.note.commit())
        {
            witness_data.add_proof(nc, Proof::dummy(&mut OsRng, nc));
        }

        let witness_response = WitnessResponse {
            witness_data: Some(witness_data.into()),
        };
        Ok(tonic::Response::new(witness_response))
    }

    #[instrument(skip_all, level = "trace")]
    async fn witness_and_build(
        &self,
        request: tonic::Request<pb::WitnessAndBuildRequest>,
    ) -> Result<tonic::Response<Self::WitnessAndBuildStream>, tonic::Status> {
        let pb::WitnessAndBuildRequest {
            transaction_plan,
            authorization_data,
        } = request.into_inner();

        let transaction_plan: TransactionPlan = transaction_plan
            .ok_or_else(|| tonic::Status::invalid_argument("missing transaction plan"))?
            .try_into()
            .map_err(|e: anyhow::Error| e.context("could not decode transaction plan"))
            .map_err(|e| tonic::Status::invalid_argument(format!("{:#}", e)))?;

        let authorization_data: AuthorizationData = authorization_data
            .ok_or_else(|| tonic::Status::invalid_argument("missing authorization data"))?
            .try_into()
            .map_err(|e: anyhow::Error| e.context("could not decode authorization data"))
            .map_err(|e| tonic::Status::invalid_argument(format!("{:#}", e)))?;

        let witness_request = pb::WitnessRequest {
            transaction_plan: Some(transaction_plan.clone().into()),
        };

        let witness_data: WitnessData = self
            .witness(tonic::Request::new(witness_request))
            .await?
            .into_inner()
            .witness_data
            .ok_or_else(|| tonic::Status::invalid_argument("missing witness data"))?
            .try_into()
            .map_err(|e: anyhow::Error| e.context("could not decode witness data"))
            .map_err(|e| tonic::Status::invalid_argument(format!("{:#}", e)))?;

        let fvk =
            self.storage.full_viewing_key().await.map_err(|_| {
                tonic::Status::failed_precondition("Error retrieving full viewing key")
            })?;

        // Extract destination addresses before building (for counterparty tracking)
        let dest_addresses = transaction_plan.dest_addresses();

        let transaction = Some(
            transaction_plan
                // TODO: calling `.build` should provide some mechanism to get progress
                // updates
                .build(&fvk, &witness_data, &authorization_data)
                .map_err(|e| {
                    tonic::Status::failed_precondition(format!("Error building transaction: {}", e))
                })?
                .into(),
        );

        // Track counterparties after successful build
        // This enables offline compliance lookups for future transactions to these addresses
        for address in dest_addresses {
            // Skip self-sends (change outputs back to our own addresses)
            if fvk.incoming().views_address(&address) {
                continue;
            }
            // Record the counterparty (height 0 = pending, will be updated when TX is confirmed)
            if let Err(e) = self.storage.record_counterparty(&address, 0).await {
                tracing::warn!(?address, ?e, "failed to record counterparty");
            } else {
                tracing::debug!(
                    ?address,
                    "recorded counterparty for future offline compliance"
                );
            }
        }

        let stream = try_stream! {
            yield pb::WitnessAndBuildResponse {
                status: Some(pb::witness_and_build_response::Status::Complete(
                    pb::witness_and_build_response::Complete { transaction },
                )),
            }
        };

        Ok(tonic::Response::new(
            stream
                .map_err(|e: anyhow::Error| {
                    tonic::Status::unavailable(format!("error witnessing transaction: {e}"))
                })
                .boxed(),
        ))
    }

    #[instrument(skip_all, level = "trace")]
    async fn app_parameters(
        &self,
        _request: tonic::Request<pb::AppParametersRequest>,
    ) -> Result<tonic::Response<pb::AppParametersResponse>, tonic::Status> {
        self.check_worker().await?;

        let parameters =
            self.storage.app_params().await.map_err(|e| {
                tonic::Status::unavailable(format!("error getting app params: {e}"))
            })?;

        let response = AppParametersResponse {
            parameters: Some(parameters.into()),
        };

        Ok(tonic::Response::new(response))
    }

    #[instrument(skip_all, level = "trace")]
    async fn nullifier_window(
        &self,
        _request: tonic::Request<pb::NullifierWindowRequest>,
    ) -> Result<tonic::Response<pb::NullifierWindowResponse>, tonic::Status> {
        self.check_worker().await?;
        let window = self.storage.nullifier_window().await.map_err(|error| {
            tonic::Status::unavailable(format!("error getting nullifier window: {error}"))
        })?;
        Ok(tonic::Response::new(NullifierWindowResponse {
            window: Some(window.into()),
        }))
    }

    #[instrument(skip_all, level = "trace")]
    async fn gas_prices(
        &self,
        _request: tonic::Request<pb::GasPricesRequest>,
    ) -> Result<tonic::Response<pb::GasPricesResponse>, tonic::Status> {
        self.check_worker().await?;

        let gas_prices =
            self.storage.gas_prices().await.map_err(|e| {
                tonic::Status::unavailable(format!("error getting gas prices: {e}"))
            })?;

        let response = GasPricesResponse {
            gas_prices: Some(gas_prices.into()),
            alt_gas_prices: Vec::new(),
        };

        Ok(tonic::Response::new(response))
    }

    #[instrument(skip_all, level = "trace")]
    async fn discovery_parameters(
        &self,
        _request: tonic::Request<pb::DiscoveryParametersRequest>,
    ) -> Result<tonic::Response<pb::DiscoveryParametersResponse>, tonic::Status> {
        self.check_worker().await?;

        let parameters = self.storage.discovery_parameters().await.map_err(|e| {
            tonic::Status::unavailable(format!("error getting discovery parameters: {e}"))
        })?;

        let response = DiscoveryParametersResponse {
            parameters: Some(parameters.into()),
        };

        Ok(tonic::Response::new(response))
    }

    #[instrument(skip_all, level = "trace")]
    async fn authorize_and_build(
        &self,
        request: tonic::Request<pb::AuthorizeAndBuildRequest>,
    ) -> Result<tonic::Response<Self::AuthorizeAndBuildStream>, tonic::Status> {
        let pb::AuthorizeAndBuildRequest { transaction_plan } = request.into_inner();

        let transaction_plan: TransactionPlan = transaction_plan
            .ok_or_else(|| tonic::Status::invalid_argument("missing transaction plan"))?
            .try_into()
            .map_err(|e: anyhow::Error| e.context("could not decode transaction plan"))
            .map_err(|e| tonic::Status::invalid_argument(format!("{:#}", e)))?;

        let witness_request = pb::WitnessRequest {
            transaction_plan: Some(transaction_plan.clone().into()),
        };

        let witness_data: WitnessData = self
            .witness(tonic::Request::new(witness_request))
            .await?
            .into_inner()
            .witness_data
            .ok_or_else(|| tonic::Status::invalid_argument("missing witness data"))?
            .try_into()
            .map_err(|e: anyhow::Error| e.context("could not decode witness data"))
            .map_err(|e| tonic::Status::invalid_argument(format!("{:#}", e)))?;

        let fvk = self.storage.full_viewing_key().await.map_err(|e| {
            tonic::Status::failed_precondition(format!("error retrieving full viewing key: {e}"))
        })?;

        let transaction = transaction_plan
            .build(&fvk, &witness_data, &AuthorizationData::default())
            .map_err(|e| {
                tonic::Status::failed_precondition(format!(
                    "unable to authorize and build transaction from the view service alone: {e}"
                ))
            })?;

        let stream = try_stream! {
            yield pb::AuthorizeAndBuildResponse {
                status: Some(pb::authorize_and_build_response::Status::Complete(
                    pb::authorize_and_build_response::Complete {
                        transaction: Some(transaction.into()),
                    },
                )),
            }
        };

        Ok(tonic::Response::new(
            stream
                .map_err(|e: anyhow::Error| {
                    tonic::Status::unavailable(format!("error authorizing transaction: {e}"))
                })
                .boxed(),
        ))
    }

    #[instrument(skip_all, level = "trace")]
    async fn wallet_id(
        &self,
        _: Request<WalletIdRequest>,
    ) -> Result<Response<WalletIdResponse>, Status> {
        let fvk = self.storage.full_viewing_key().await.map_err(|e| {
            Status::failed_precondition(format!("Error retrieving full viewing key: {e}"))
        })?;

        Ok(Response::new(WalletIdResponse {
            wallet_id: Some(fvk.wallet_id().into()),
        }))
    }

    #[instrument(skip_all, level = "trace")]
    async fn asset_metadata_by_id(
        &self,
        request: Request<AssetMetadataByIdRequest>,
    ) -> Result<Response<AssetMetadataByIdResponse>, Status> {
        let asset_id = request
            .into_inner()
            .asset_id
            .ok_or_else(|| Status::invalid_argument("missing asset id"))?
            .try_into()
            .map_err(|e| Status::invalid_argument(format!("{e:#}")))?;

        let metadata = self
            .storage
            .asset_by_id(&asset_id)
            .await
            .map_err(|e| Status::internal(format!("Error retrieving asset by id: {e:#}")))?;

        Ok(Response::new(AssetMetadataByIdResponse {
            denom_metadata: metadata.map(Into::into),
        }))
    }

    #[instrument(skip_all, level = "trace")]
    async fn compliance_asset_status(
        &self,
        request: tonic::Request<pb::ComplianceAssetStatusRequest>,
    ) -> Result<tonic::Response<pb::ComplianceAssetStatusResponse>, tonic::Status> {
        let asset_id_proto = request
            .into_inner()
            .asset_id
            .ok_or_else(|| tonic::Status::invalid_argument("missing asset_id"))?;

        // Parse asset_id to check against local tree
        let asset_id: shieldd_sdk_asset::asset::Id = asset_id_proto
            .clone()
            .try_into()
            .map_err(|e| tonic::Status::invalid_argument(format!("invalid asset_id: {e}")))?;

        // Get asset policy if it exists
        let policy = self
            .storage
            .get_asset_policy(&asset_id)
            .await
            .map_err(|e| tonic::Status::internal(format!("failed to get asset policy: {e}")))?;
        let is_regulated = policy.is_some();

        let (dk_pub, threshold, has_policy) = match &policy {
            Some(p) => (
                p.params.dk_pub.vartime_compress().0.to_vec(),
                p.params.threshold.to_le_bytes().to_vec(),
                true,
            ),
            None => (vec![], vec![], false),
        };

        tracing::debug!(
            ?asset_id,
            is_regulated,
            has_policy,
            "using local tree for asset status"
        );

        // With IMT, we can always answer the query (regulated = membership, unregulated = non-membership)
        Ok(tonic::Response::new(pb::ComplianceAssetStatusResponse {
            asset_id: Some(asset_id_proto),
            is_registered: true,
            is_regulated,
            dk_pub,
            threshold,
            asset_policy: policy.map(Into::into),
        }))
    }

    #[instrument(skip_all, level = "trace")]
    async fn compliance_anchors(
        &self,
        _request: tonic::Request<pb::ComplianceAnchorsRequest>,
    ) -> Result<tonic::Response<pb::ComplianceAnchorsResponse>, tonic::Status> {
        // Use local tree roots
        let snapshot = self.compliance_snapshot.read().clone();
        let user_root = snapshot.user_tree.root();
        let asset_root = snapshot.asset_tree.root();

        tracing::debug!(
            ?user_root,
            ?asset_root,
            "using local tree roots for anchors"
        );

        Ok(tonic::Response::new(pb::ComplianceAnchorsResponse {
            user_tree_root: user_root.0.to_bytes().to_vec(),
            asset_tree_root: asset_root.0.to_bytes().to_vec(),
        }))
    }

    #[instrument(skip_all, level = "trace")]
    async fn compliance_merkle_proofs(
        &self,
        request: tonic::Request<pb::ComplianceMerkleProofsRequest>,
    ) -> Result<tonic::Response<pb::ComplianceMerkleProofsResponse>, tonic::Status> {
        let request_inner = request.into_inner();

        // Parse address and asset_id
        let address: shieldd_sdk_keys::Address = request_inner
            .address
            .clone()
            .ok_or_else(|| tonic::Status::invalid_argument("missing address"))?
            .try_into()
            .map_err(|e| tonic::Status::invalid_argument(format!("invalid address: {e}")))?;

        let asset_id: shieldd_sdk_asset::asset::Id = request_inner
            .asset_id
            .clone()
            .ok_or_else(|| tonic::Status::invalid_argument("missing asset_id"))?
            .try_into()
            .map_err(|e| tonic::Status::invalid_argument(format!("invalid asset_id: {e}")))?;

        let snapshot = self.compliance_snapshot.read().clone();
        let user_tree = &snapshot.user_tree;
        let asset_tree = &snapshot.asset_tree;

        let user_anchor = user_tree.root();
        let asset_anchor = asset_tree.root();

        // Get asset proof from local tree (always available)
        let (asset_position, indexed_leaf, asset_path, is_regulated) = asset_tree
            .get_proof_data(asset_id)
            .map_err(|e| tonic::Status::internal(format!("failed to get asset proof: {e}")))?;

        // Try to get leaf data from local storage
        let local_leaf_data = self
            .storage
            .get_compliance_leaf_data(&address, &asset_id)
            .await
            .map_err(|e| tonic::Status::internal(format!("storage error: {e}")))?;

        // Returns (user_registered, compliance_position, compliance_path, compliance_leaf)
        let (user_registered, compliance_position, compliance_path, compliance_leaf) =
            match local_leaf_data {
                Some(leaf_data) => {
                    // Local storage hit - compute path from held user_tree reference
                    let position = leaf_data.position;
                    if user_tree.commitment(position) != Some(leaf_data.commitment) {
                        return Err(tonic::Status::unavailable(
                            "compliance projection advanced while building the proof; retry",
                        ));
                    }
                    let path = user_tree.witness(position).map_err(|e| {
                        tonic::Status::internal(format!("failed to compute path: {e}"))
                    })?;

                    // Build proto leaf from local storage
                    let leaf_proto = compliance_pb::ComplianceLeaf {
                        address: Some(address.clone().into()),
                        asset_id: Some(asset_id.into()),
                        d: leaf_data.d.to_vec(),
                        status: compliance_pb::UserAssetStatus::from(leaf_data.status) as i32,
                    };

                    tracing::debug!(
                        ?address,
                        ?asset_id,
                        position,
                        "using local storage for user proof"
                    );
                    (true, position, path, Some(leaf_proto))
                }
                None => {
                    // Local storage miss - fetch one root-bound proof response.
                    tracing::debug!(?address, ?asset_id, "local storage miss, fetching from pd");

                    use shieldd_sdk_proto::core::component::compliance::v1::{
                        query_service_client::QueryServiceClient as ComplianceQueryServiceClient,
                        ComplianceMerkleProofsRequest,
                    };

                    let endpoint = get_pd_endpoint(self.node.clone()).await.map_err(|e| {
                        tonic::Status::internal(format!("failed to connect to pd: {e}"))
                    })?;
                    let channel = endpoint.connect().await.map_err(|e| {
                        tonic::Status::internal(format!("failed to connect to pd: {e}"))
                    })?;
                    let mut client = ComplianceQueryServiceClient::new(channel);

                    let proof_request = ComplianceMerkleProofsRequest {
                        address: request_inner.address.clone(),
                        asset_id: request_inner.asset_id.clone(),
                    };
                    let proof_response = client
                        .compliance_merkle_proofs(tonic::Request::new(proof_request))
                        .await?
                        .into_inner();
                    if proof_response.compliance_anchor.as_slice() != user_anchor.0.to_bytes()
                        || proof_response.asset_anchor.as_slice() != asset_anchor.0.to_bytes()
                    {
                        return Err(tonic::Status::unavailable(
                            "remote compliance proof is newer than the local snapshot; retry",
                        ));
                    }

                    if !proof_response.user_registered {
                        // User not registered - return empty proof
                        (
                            false,
                            0,
                            shieldd_sdk_compliance::structs::MerklePath::default(),
                            None,
                        )
                    } else {
                        let path = proof_response
                            .compliance_path
                            .map(shieldd_sdk_compliance::structs::MerklePath::try_from)
                            .transpose()
                            .map_err(|error| {
                                tonic::Status::internal(format!(
                                    "invalid compliance_path in pd response: {error}"
                                ))
                            })?
                            .ok_or_else(|| {
                                tonic::Status::internal("compliance_path missing from pd response")
                            })?;

                        (
                            proof_response.user_registered,
                            proof_response.compliance_position,
                            path,
                            proof_response.compliance_leaf,
                        )
                    }
                }
            };

        // Convert local types to proto types (using compliance_pb for inner types)
        let compliance_path_proto = compliance_pb::MerklePath {
            layers: compliance_path
                .layers
                .into_iter()
                .map(|layer| compliance_pb::MerklePathLayer {
                    siblings: layer.siblings,
                })
                .collect(),
        };

        let asset_path_proto = compliance_pb::MerklePath {
            layers: asset_path
                .layers
                .into_iter()
                .map(|layer| compliance_pb::MerklePathLayer {
                    siblings: layer.siblings,
                })
                .collect(),
        };

        let asset_indexed_leaf_proto: compliance_pb::IndexedLeafData = indexed_leaf.clone().into();

        Ok(tonic::Response::new(pb::ComplianceMerkleProofsResponse {
            user_registered,
            asset_registered: true, // Always true with IMT (membership or non-membership)
            is_regulated,
            compliance_path: Some(compliance_path_proto),
            compliance_position,
            asset_path: Some(asset_path_proto),
            asset_position,
            compliance_anchor: user_anchor.0.to_bytes().to_vec(),
            asset_anchor: asset_anchor.0.to_bytes().to_vec(),
            asset_indexed_leaf: Some(asset_indexed_leaf_proto),
            compliance_leaf,
        }))
    }

    #[instrument(skip_all, level = "trace")]
    async fn compliance_user_leaf(
        &self,
        request: tonic::Request<pb::ComplianceUserLeafRequest>,
    ) -> Result<tonic::Response<pb::ComplianceUserLeafResponse>, tonic::Status> {
        let request_inner = request.into_inner();

        // Parse address and asset_id
        let address: shieldd_sdk_keys::Address = request_inner
            .address
            .clone()
            .ok_or_else(|| tonic::Status::invalid_argument("missing address"))?
            .try_into()
            .map_err(|e| tonic::Status::invalid_argument(format!("invalid address: {e}")))?;

        let asset_id: shieldd_sdk_asset::asset::Id = request_inner
            .asset_id
            .clone()
            .ok_or_else(|| tonic::Status::invalid_argument("missing asset_id"))?
            .try_into()
            .map_err(|e| tonic::Status::invalid_argument(format!("invalid asset_id: {e}")))?;

        // Try to get leaf data from local storage first
        let local_leaf_data = self
            .storage
            .get_compliance_leaf_data(&address, &asset_id)
            .await
            .map_err(|e| tonic::Status::internal(format!("storage error: {e}")))?;

        if let Some(leaf_data) = local_leaf_data {
            // Local storage hit - reconstruct the leaf from stored data.
            tracing::debug!(?address, ?asset_id, "using local storage for user leaf");

            let leaf = compliance_pb::ComplianceLeaf {
                address: request_inner.address,
                asset_id: request_inner.asset_id,
                d: leaf_data.d.to_vec(),
                status: compliance_pb::UserAssetStatus::from(leaf_data.status) as i32,
            };

            return Ok(tonic::Response::new(pb::ComplianceUserLeafResponse {
                is_registered: true,
                leaf: Some(leaf),
            }));
        }

        // Local storage miss - fall back to gRPC
        tracing::debug!(?address, ?asset_id, "local storage miss, fetching from pd");

        use shieldd_sdk_proto::core::component::compliance::v1::{
            query_service_client::QueryServiceClient as ComplianceQueryServiceClient,
            ComplianceUserLeafRequest as ComplianceRequest,
        };

        let endpoint = get_pd_endpoint(self.node.clone())
            .await
            .map_err(|e| tonic::Status::internal(format!("failed to connect to pd: {e}")))?;
        let channel = endpoint
            .connect()
            .await
            .map_err(|e| tonic::Status::internal(format!("failed to connect to pd: {e}")))?;
        let mut client = ComplianceQueryServiceClient::new(channel);

        let compliance_request = ComplianceRequest {
            address: request_inner.address.clone(),
            asset_id: request_inner.asset_id.clone(),
        };
        let response = client
            .compliance_user_leaf(tonic::Request::new(compliance_request))
            .await?
            .into_inner();

        // Convert compliance proto types to view proto types
        let leaf = response.leaf.map(|l| compliance_pb::ComplianceLeaf {
            address: l.address,
            asset_id: l.asset_id,
            d: l.d,
            status: l.status,
        });

        Ok(tonic::Response::new(pb::ComplianceUserLeafResponse {
            is_registered: response.is_registered,
            leaf,
        }))
    }

    async fn compliance_batch_merkle_proofs(
        &self,
        request: Request<pb::ComplianceBatchMerkleProofsRequest>,
    ) -> Result<Response<pb::ComplianceBatchMerkleProofsResponse>, Status> {
        self.query_compliance_batch(request.into_inner())
            .await
            .map(Response::new)
    }
}

/// Convert a pd node URL to a Tonic `Endpoint`.
///
/// Required in order to configure TLS for HTTPS endpoints.
async fn get_pd_endpoint(node: Url) -> anyhow::Result<Endpoint> {
    let endpoint = match node.scheme() {
        "http" => Channel::from_shared(node.to_string())?,
        "https" => Channel::from_shared(node.to_string())?
            .tls_config(ClientTlsConfig::new().with_webpki_roots())?,
        other => anyhow::bail!("unknown url scheme {other}"),
    };
    // HTTP/2 keepalive prevents reuse of stale idle pooled connections, which
    // otherwise surface as tonic Unavailable ("connection closed before message
    // completed") on the long-lived view worker channel.
    let endpoint = endpoint
        .http2_keep_alive_interval(std::time::Duration::from_secs(10))
        .keep_alive_timeout(std::time::Duration::from_secs(20))
        .keep_alive_while_idle(true);
    Ok(endpoint)
}

struct LocalPlanningIo<'a>(&'a ViewServer);

#[async_trait]
impl crate::planning_io::PlanningIo for LocalPlanningIo<'_> {
    async fn chain_id(&mut self) -> anyhow::Result<String> {
        Ok(self.0.storage.app_params().await?.chain_id)
    }
    async fn nullifier_window(
        &mut self,
    ) -> anyhow::Result<shieldd_sdk_sct::nullifier_generation::NullifierWindow> {
        self.0.storage.nullifier_window().await
    }
    async fn discovery_parameters(
        &mut self,
    ) -> anyhow::Result<shieldd_sdk_shielded_pool::discovery::Parameters> {
        self.0.storage.discovery_parameters().await
    }
    async fn notes(
        &mut self,
        request: pb::NotesRequest,
    ) -> anyhow::Result<Vec<crate::SpendableNoteRecord>> {
        self.0.check_worker().await?;
        self.0
            .storage
            .notes(
                request.include_spent,
                request.asset_id.map(TryInto::try_into).transpose()?,
                request.address_index.map(TryInto::try_into).transpose()?,
                request.amount_to_spend.map(TryInto::try_into).transpose()?,
            )
            .await
    }
    async fn address_by_index(&mut self, index: AddressIndex) -> anyhow::Result<Address> {
        let fvk = self.0.storage.full_viewing_key().await?;
        Ok(self
            .0
            .persist_issued_address(index, fvk.payment_address(index), AddressPurpose::General)
            .await?)
    }
    async fn index_by_address(&mut self, address: Address) -> anyhow::Result<Option<AddressIndex>> {
        Ok(self
            .0
            .storage
            .full_viewing_key()
            .await?
            .address_index(&address))
    }
    async fn compliance_data(
        &mut self,
        queries: Vec<ComplianceQuery>,
    ) -> anyhow::Result<shieldd_sdk_compliance::BatchComplianceData> {
        let batch = self
            .0
            .query_compliance_batch(pb::ComplianceBatchMerkleProofsRequest {
                queries: queries
                    .iter()
                    .map(
                        |ComplianceQuery { address, asset_id }| pb::ComplianceBatchQuery {
                            address: Some(address.clone().into()),
                            asset_id: Some((*asset_id).into()),
                        },
                    )
                    .collect(),
            })
            .await?;
        let assets = queries
            .iter()
            .zip(&batch.results)
            .filter_map(|(query, result)| result.is_regulated.then_some(query.asset_id))
            .collect::<BTreeSet<_>>();
        let mut policies = BTreeMap::new();
        for asset in assets {
            let policy = self
                .0
                .storage
                .get_asset_policy(&asset)
                .await?
                .ok_or_else(|| anyhow!("missing regulated asset policy"))?;
            policies.insert(asset, policy);
        }
        crate::client_compliance::parse_batch_compliance(&queries, batch, policies)
    }
}

fn planning_result_to_rpc(
    result: NoteManagerPlanningResult,
    maintenance: &str,
    insufficient: &str,
) -> Result<TransactionPlan, Status> {
    match result {
        NoteManagerPlanningResult::Ready { transaction_plan } => Ok(transaction_plan),
        NoteManagerPlanningResult::NeedsMaintenance { .. } => {
            Err(Status::invalid_argument(maintenance))
        }
        NoteManagerPlanningResult::InsufficientBalance => {
            Err(Status::invalid_argument(insufficient))
        }
        NoteManagerPlanningResult::UnsupportedIntent { reason } => {
            Err(Status::invalid_argument(reason))
        }
    }
}
