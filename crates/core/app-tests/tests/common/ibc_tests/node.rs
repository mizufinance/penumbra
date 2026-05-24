use {
    crate::common::BuilderExt as _,
    anyhow::{anyhow, Context as _, Result},
    cnidarium::Storage,
    decaf377_rdsa::{
        SigningKey as RdsaSigningKey, SpendAuth, VerificationKey as RdsaVerificationKey,
    },
    ed25519_consensus::{SigningKey, VerificationKey},
    ibc_proto::ibc::core::{
        channel::v1::query_client::QueryClient as IbcChannelQueryClient,
        client::v1::query_client::QueryClient as IbcClientQueryClient,
        connection::v1::query_client::QueryClient as IbcConnectionQueryClient,
    },
    ibc_types::{
        core::{
            channel::{ChannelEnd, ChannelId, PortId, Version as ChannelVersion},
            client::{ClientId, ClientType, Height},
            connection::{
                ChainId, ConnectionEnd, ConnectionId, Counterparty, Version as ConnectionVersion,
            },
        },
        lightclients::tendermint::{
            consensus_state::ConsensusState, header::Header as TendermintHeader,
        },
    },
    penumbra_sdk_app::{
        genesis::{self, AppState},
        server::consensus::Consensus,
        SUBSTORE_PREFIXES,
    },
    penumbra_sdk_asset::{asset, BASE_ASSET_DENOM},
    penumbra_sdk_compliance::{
        structs::{AssetRegistrationGrant, UserRegistrationGrant, UserRegistrationGrantBody},
        ComplianceLeaf, IbcAssetOrigin, IbcRoute, MsgRegisterAsset, MsgRegisterUser,
    },
    penumbra_sdk_ibc::{component::ClientStateReadExt as _, IBC_COMMITMENT_PREFIX},
    penumbra_sdk_keys::{test_keys, Address},
    penumbra_sdk_mock_client::MockClient,
    penumbra_sdk_mock_consensus::{NodeResumeState, TestNode},
    penumbra_sdk_proto::{
        util::tendermint_proxy::v1::{
            tendermint_proxy_service_client::TendermintProxyServiceClient, GetStatusRequest,
        },
        DomainType as _,
    },
    penumbra_sdk_transaction::{Action, Transaction, TransactionBody, TransactionParameters},
    rand::SeedableRng as _,
    std::{
        error::Error,
        fs,
        net::TcpListener,
        ops::Deref,
        path::{Path, PathBuf},
    },
    tap::{Tap, TapFallible},
    tendermint::{
        v0_37::abci::{ConsensusRequest, ConsensusResponse},
        vote::Power,
        Time,
    },
    tokio::time,
    tonic::transport::Channel,
    tower_actor::Actor,
    tracing::info,
};

pub struct TestStorage {
    inner: Storage,
    root: PathBuf,
    _temp_dir: Option<tempfile::TempDir>,
}

impl TestStorage {
    pub async fn new_with_penumbra_prefixes() -> Result<Self> {
        let dir = tempfile::tempdir()?;
        Self::load_from_root(dir.path().to_path_buf(), Some(dir)).await
    }

    pub async fn new_persistent(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)?;
        Self::load_from_root(root, None).await
    }

    pub async fn copied_from(source: impl AsRef<Path>) -> Result<Self> {
        let dir = tempfile::tempdir()?;
        copy_dir_contents(source.as_ref(), dir.path())?;
        Self::load_from_root(dir.path().to_path_buf(), Some(dir)).await
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    async fn load_from_root(root: PathBuf, temp_dir: Option<tempfile::TempDir>) -> Result<Self> {
        let inner = Storage::load(root.join("storage.db"), SUBSTORE_PREFIXES.to_vec()).await?;
        Ok(Self {
            inner,
            root,
            _temp_dir: temp_dir,
        })
    }
}

impl Deref for TestStorage {
    type Target = Storage;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl AsRef<Storage> for TestStorage {
    fn as_ref(&self) -> &Storage {
        &self.inner
    }
}

fn copy_dir_contents(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_contents(&source_path, &destination_path)?;
        } else {
            if entry.file_name() == "LOCK" {
                continue;
            }
            fs::copy(&source_path, &destination_path).with_context(|| {
                format!(
                    "copying {} to {}",
                    source_path.display(),
                    destination_path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn bind_rpc_listener() -> Result<(TcpListener, url::Url)> {
    let listener = TcpListener::bind("127.0.0.1:0").context("binding test gRPC listener")?;
    let local_addr = listener
        .local_addr()
        .context("reading test gRPC listener address")?;
    listener
        .set_nonblocking(true)
        .context("setting test gRPC listener nonblocking")?;
    let grpc_url = format!("http://{local_addr}")
        .parse::<url::Url>()?
        .tap(|url| tracing::debug!(%url, "parsed grpc url"));
    Ok((listener, grpc_url))
}

// Contains some data from a single IBC connection + client for test usage.
// This might be better off as an extension trait or additional impl on the TestNode struct.
#[allow(unused)]
pub struct TestNodeWithIBC {
    pub connection_id: ConnectionId,
    pub channel_id: ChannelId,
    pub client_id: ClientId,
    pub port_id: PortId,
    pub chain_id: String,
    pub counterparty: Counterparty,
    pub connection_version: ConnectionVersion,
    pub channel_version: ChannelVersion,
    pub signer: String,
    pub connection: Option<ConnectionEnd>,
    pub channel: Option<ChannelEnd>,
    pub node: TestNode<Actor<ConsensusRequest, ConsensusResponse, Box<dyn Error + Send + Sync>>>,
    pub storage: TestStorage,
    pub ibc_client_query_client: IbcClientQueryClient<Channel>,
    pub ibc_connection_query_client: IbcConnectionQueryClient<Channel>,
    pub ibc_channel_query_client: IbcChannelQueryClient<Channel>,
    pub tendermint_proxy_service_client: TendermintProxyServiceClient<Channel>,
    rpc_server: tokio::task::JoinHandle<()>,
}

#[allow(unused)]
/// This interacts with a node similarly to how a relayer would. We intentionally call
/// against the external gRPC interfaces to get the most comprehensive test coverage.
impl TestNodeWithIBC {
    pub async fn new(
        suffix: &str,
        start_time: Time,
        keys: (SigningKey, VerificationKey),
    ) -> Result<Self, anyhow::Error> {
        Self::new_with_genesis_content(suffix, start_time, keys, genesis::Content::default()).await
    }

    pub async fn new_with_allocations_and_registrar(
        suffix: &str,
        start_time: Time,
        keys: (SigningKey, VerificationKey),
        allocation_count: usize,
        registrar_vk: RdsaVerificationKey<SpendAuth>,
    ) -> Result<Self, anyhow::Error> {
        Self::new_with_allocations_and_registrar_at_root(
            suffix,
            start_time,
            keys,
            allocation_count,
            registrar_vk,
            None,
        )
        .await
    }

    pub async fn new_with_allocations_and_registrar_at_root(
        suffix: &str,
        start_time: Time,
        keys: (SigningKey, VerificationKey),
        allocation_count: usize,
        registrar_vk: RdsaVerificationKey<SpendAuth>,
        storage_root: Option<&Path>,
    ) -> Result<Self, anyhow::Error> {
        let allocations = std::iter::repeat(penumbra_sdk_shielded_pool::genesis::Allocation {
            raw_amount: 1_000_000u128.into(),
            raw_denom: BASE_ASSET_DENOM.deref().base_denom().denom,
            address: test_keys::ADDRESS_0.to_owned(),
        })
        .take(allocation_count)
        .collect();
        let content = genesis::Content {
            compliance_content: penumbra_sdk_compliance::genesis::Content {
                compliance_registrar_vk: vec![registrar_vk],
                ..Default::default()
            },
            shielded_pool_content: penumbra_sdk_shielded_pool::genesis::Content {
                allocations,
                ..Default::default()
            },
            ..Default::default()
        };
        Self::new_with_genesis_content_at_root(suffix, start_time, keys, content, storage_root)
            .await
    }

    pub async fn new_with_genesis_content(
        suffix: &str,
        start_time: Time,
        keys: (SigningKey, VerificationKey),
        content: genesis::Content,
    ) -> Result<Self, anyhow::Error> {
        Self::new_with_genesis_content_at_root(suffix, start_time, keys, content, None).await
    }

    pub async fn new_with_genesis_content_at_root(
        suffix: &str,
        start_time: Time,
        keys: (SigningKey, VerificationKey),
        content: genesis::Content,
        storage_root: Option<&Path>,
    ) -> Result<Self, anyhow::Error> {
        let chain_id = format!("{}-{}", TestNode::<()>::CHAIN_ID, suffix);
        // Use the correct substores
        let storage = match storage_root {
            Some(root) => TestStorage::new_persistent(root).await?,
            None => TestStorage::new_with_penumbra_prefixes().await?,
        };
        // Instantiate a mock tendermint proxy, which we will connect to the test node.
        let proxy = penumbra_sdk_mock_tendermint_proxy::TestNodeProxy::new::<Consensus>();

        let node = {
            let app_state = AppState::Content(content.with_chain_id(chain_id.clone()));
            let consensus = Consensus::new(storage.as_ref().clone());
            TestNode::builder()
                .with_keys(vec![keys])
                .single_validator()
                .with_initial_timestamp(start_time)
                .with_penumbra_auto_app_state(app_state)?
                .on_block(proxy.on_block_callback())
                .init_chain(consensus)
                .await
                .tap_ok(|e| tracing::info!(hash = %e.last_app_hash_hex(), "finished init chain"))?
        };

        let (listener, grpc_url) = bind_rpc_listener()?;

        tracing::info!("spawning gRPC...");
        // Spawn the node's RPC server.
        let rpc_server = {
            let make_svc = penumbra_sdk_app::rpc::routes(
                storage.as_ref(),
                proxy,
                false, /*enable_expensive_rpc*/
            )?
            .into_axum_router()
            .layer(tower_http::cors::CorsLayer::permissive())
            .into_make_service()
            .tap(|_| tracing::info!("initialized rpc service"));
            let server = axum_server::from_tcp(listener).serve(make_svc);
            tokio::spawn(async { server.await.expect("grpc server returned an error") })
                .tap(|_| tracing::info!("grpc server is running"))
        };

        time::sleep(time::Duration::from_secs(1)).await;
        // Create an RPC server for each chain to respond to IBC-related queries.
        let channel = Channel::from_shared(grpc_url.to_string())
            .with_context(|| "could not parse node URI")?
            .connect()
            .await
            .with_context(|| "could not connect to grpc server")
            .tap_err(|error| tracing::error!(?error, "could not connect to grpc server"))?;

        let ibc_connection_query_client = IbcConnectionQueryClient::new(channel.clone());
        let ibc_channel_query_client = IbcChannelQueryClient::new(channel.clone());
        let ibc_client_query_client = IbcClientQueryClient::new(channel.clone());
        let tendermint_proxy_service_client = TendermintProxyServiceClient::new(channel.clone());

        Self::from_parts(
            chain_id,
            node,
            storage,
            ibc_connection_query_client,
            ibc_channel_query_client,
            ibc_client_query_client,
            tendermint_proxy_service_client,
            rpc_server,
        )
    }

    pub async fn new_from_cached_storage(
        suffix: &str,
        keys: (SigningKey, VerificationKey),
        storage: TestStorage,
        resume_state: NodeResumeState,
    ) -> Result<Self, anyhow::Error> {
        let chain_id = format!("{}-{}", TestNode::<()>::CHAIN_ID, suffix);
        let proxy = penumbra_sdk_mock_tendermint_proxy::TestNodeProxy::new::<Consensus>();
        let consensus = Consensus::new(storage.as_ref().clone());
        let node = TestNode::builder()
            .with_keys(vec![keys])
            .single_validator()
            .on_block(proxy.on_block_callback())
            .resume_chain(consensus, resume_state)?;

        let (listener, grpc_url) = bind_rpc_listener()?;

        let rpc_server = {
            let make_svc = penumbra_sdk_app::rpc::routes(
                storage.as_ref(),
                proxy,
                false, /*enable_expensive_rpc*/
            )?
            .into_axum_router()
            .layer(tower_http::cors::CorsLayer::permissive())
            .into_make_service()
            .tap(|_| tracing::info!("initialized rpc service"));
            let server = axum_server::from_tcp(listener).serve(make_svc);
            tokio::spawn(async { server.await.expect("grpc server returned an error") })
                .tap(|_| tracing::info!("grpc server is running"))
        };

        time::sleep(time::Duration::from_secs(1)).await;
        let channel = Channel::from_shared(grpc_url.to_string())
            .with_context(|| "could not parse node URI")?
            .connect()
            .await
            .with_context(|| "could not connect to grpc server")
            .tap_err(|error| tracing::error!(?error, "could not connect to grpc server"))?;

        let ibc_connection_query_client = IbcConnectionQueryClient::new(channel.clone());
        let ibc_channel_query_client = IbcChannelQueryClient::new(channel.clone());
        let ibc_client_query_client = IbcClientQueryClient::new(channel.clone());
        let tendermint_proxy_service_client = TendermintProxyServiceClient::new(channel.clone());

        Self::from_parts(
            chain_id,
            node,
            storage,
            ibc_connection_query_client,
            ibc_channel_query_client,
            ibc_client_query_client,
            tendermint_proxy_service_client,
            rpc_server,
        )
    }

    fn from_parts(
        chain_id: String,
        node: TestNode<Actor<ConsensusRequest, ConsensusResponse, Box<dyn Error + Send + Sync>>>,
        storage: TestStorage,
        ibc_connection_query_client: IbcConnectionQueryClient<Channel>,
        ibc_channel_query_client: IbcChannelQueryClient<Channel>,
        ibc_client_query_client: IbcClientQueryClient<Channel>,
        tendermint_proxy_service_client: TendermintProxyServiceClient<Channel>,
        rpc_server: tokio::task::JoinHandle<()>,
    ) -> Result<Self, anyhow::Error> {
        let pk = node
            .keyring()
            .iter()
            .next()
            .expect("validator key in keyring")
            .0;
        let proposer_address = tendermint::account::Id::new(
            <sha2::Sha256 as sha2::Digest>::digest(pk).as_slice()[0..20]
                .try_into()
                .expect(""),
        );

        Ok(Self {
            // the test relayer supports only a single connection on each chain as of now
            connection_id: ConnectionId::new(0),
            // the test relayer supports only a single channel per connection on each chain as of now
            channel_id: ChannelId::new(0),
            // Only ICS20 transfers are supported
            port_id: PortId::transfer(),
            node,
            storage,
            client_id: ClientId::new(ClientType::new("07-tendermint".to_string()), 0)?,
            chain_id: chain_id.clone(),
            counterparty: Counterparty {
                client_id: ClientId::new(ClientType::new("07-tendermint".to_string()), 0)?,
                connection_id: None,
                prefix: IBC_COMMITMENT_PREFIX.to_owned(),
            },
            connection_version: ConnectionVersion::default(),
            channel_version: ChannelVersion::new("ics20-1".to_string()),
            signer: hex::encode_upper(proposer_address),
            connection: None,
            channel: None,
            ibc_connection_query_client,
            ibc_channel_query_client,
            ibc_client_query_client,
            tendermint_proxy_service_client,
            rpc_server,
        })
    }

    pub async fn client(&mut self) -> Result<MockClient, anyhow::Error> {
        // Sync the mock client, using the test wallet's spend key, to the latest snapshot.
        Ok(MockClient::new(test_keys::SPEND_KEY.clone())
            .with_sync_to_storage(&self.storage)
            .await?
            .tap(|c| info!(client.notes = %c.notes.len(), "mock client synced to test storage")))
    }

    /// Execute a block containing compliance registration actions.
    /// This commits compliance data to chain state so subsequent transactions
    /// will have matching compliance anchors.
    pub async fn execute_compliance_setup(
        &mut self,
        addresses: &[Address],
        asset_ids: &[asset::Id],
    ) -> Result<()> {
        let mut actions: Vec<Action> = Vec::new();

        // Create MsgRegisterAsset for each asset (unregulated)
        for &asset_id in asset_ids {
            let msg = MsgRegisterAsset {
                asset_id,
                is_regulated: false,
                dk_pub: None,
                threshold: None,
                slot_count: 0,
                allowed_ibc_routes: vec![],
                ibc_origin: None,
                ring_pk: None,
                ring_id: String::new(),
                policy_id: String::new(),
                permission: String::new(),
                resource: String::new(),
                registration_authority_vk: None,
                asset_registration_grant: None,
            };
            actions.push(Action::ComplianceRegisterAsset(msg));
        }

        // Create MsgRegisterUser for each (address, asset) pair
        for address in addresses {
            for &asset_id in asset_ids {
                let b_d_fq = address.diversified_generator().vartime_compress_to_field();
                let leaf = ComplianceLeaf::new(address.clone(), asset_id, b_d_fq);
                let msg = MsgRegisterUser { leaf, grant: None };
                actions.push(Action::ComplianceRegisterUser(msg));
            }
        }

        // Build minimal transaction directly (no shielded input plan = no witness data needed)
        let transaction_body = TransactionBody {
            actions,
            transaction_parameters: TransactionParameters {
                expiry_height: 0,
                chain_id: self.chain_id.clone(),
                fee: Default::default(),
            },
            fee_funding: None,
            detection_data: None,
            memo: None,
        };

        let tx = Transaction {
            transaction_body,
            binding_sig: [0u8; 64].into(), // No binding sig needed (no value balance)
            anchor: penumbra_sdk_tct::Tree::new().root(), // Default empty tree anchor
        };

        self.node
            .block()
            .with_data(vec![tx.encode_to_vec()])
            .execute()
            .await?;

        Ok(())
    }

    pub async fn execute_regulated_compliance_setup(
        &mut self,
        addresses: &[Address],
        asset: asset::Id,
        allowed_ibc_routes: Vec<IbcRoute>,
        ibc_origin: Option<IbcAssetOrigin>,
        registrar_sk: &RdsaSigningKey<SpendAuth>,
        authority_sk: &RdsaSigningKey<SpendAuth>,
    ) -> Result<()> {
        const VALID_UNTIL_UNIX: u64 = 4_102_444_800;
        let authority_vk = RdsaVerificationKey::from(authority_sk);
        let mut actions: Vec<Action> = Vec::new();

        let mut asset_msg = MsgRegisterAsset {
            asset_id: asset,
            is_regulated: true,
            dk_pub: Some(decaf377::Element::GENERATOR),
            threshold: None,
            slot_count: penumbra_sdk_compliance::DEFAULT_COMPLIANCE_SLOT_COUNT,
            allowed_ibc_routes,
            ibc_origin,
            ring_pk: None,
            ring_id: String::new(),
            policy_id: "benchmark-policy".to_string(),
            permission: String::new(),
            resource: String::new(),
            registration_authority_vk: Some(authority_vk),
            asset_registration_grant: None,
        };
        let body = asset_msg.registration_grant_body(VALID_UNTIL_UNIX);
        asset_msg.asset_registration_grant = Some(AssetRegistrationGrant {
            registrar_vk: RdsaVerificationKey::from(registrar_sk),
            signature: registrar_sk.sign(
                rand_chacha::ChaChaRng::seed_from_u64(1),
                &body.signing_bytes(),
            ),
            body,
        });
        actions.push(Action::ComplianceRegisterAsset(asset_msg));

        for address in addresses {
            let b_d_fq = address.diversified_generator().vartime_compress_to_field();
            let leaf = ComplianceLeaf::new(address.clone(), asset, b_d_fq);
            let body = UserRegistrationGrantBody {
                leaf: leaf.clone(),
                policy_id: "benchmark-policy".to_string(),
                valid_until_unix: VALID_UNTIL_UNIX,
                nonce: address.to_string().into_bytes(),
            };
            actions.push(Action::ComplianceRegisterUser(MsgRegisterUser {
                leaf,
                grant: Some(UserRegistrationGrant {
                    signature: authority_sk.sign(
                        rand_chacha::ChaChaRng::seed_from_u64(2 + actions.len() as u64),
                        &body.signing_bytes(),
                    ),
                    body,
                }),
            }));
        }

        let transaction_body = TransactionBody {
            actions,
            transaction_parameters: TransactionParameters {
                expiry_height: 0,
                chain_id: self.chain_id.clone(),
                fee: Default::default(),
            },
            fee_funding: None,
            detection_data: None,
            memo: None,
        };

        let tx = Transaction {
            transaction_body,
            binding_sig: [0u8; 64].into(),
            anchor: penumbra_sdk_tct::Tree::new().root(),
        };

        self.node
            .block()
            .with_data(vec![tx.encode_to_vec()])
            .execute()
            .await?;

        Ok(())
    }

    pub async fn get_latest_height(&mut self) -> Result<Height, anyhow::Error> {
        let status: penumbra_sdk_proto::util::tendermint_proxy::v1::GetStatusResponse = self
            .tendermint_proxy_service_client
            .get_status(GetStatusRequest {})
            .await?
            .into_inner();
        Ok(Height::new(
            ChainId::chain_version(&self.chain_id),
            status
                .sync_info
                .ok_or(anyhow!("no sync info"))?
                .latest_block_height,
        )?)
    }

    // TODO: maybe move to an IBC extension trait for TestNode?
    // or maybe the Block has everything it needs to produce this?
    pub fn create_tendermint_header(
        &self,
        trusted_height: Option<Height>,
        penumbra_sdk_proto::util::tendermint_proxy::v1::GetBlockByHeightResponse {
            block_id: _,
            block,
        }: penumbra_sdk_proto::util::tendermint_proxy::v1::GetBlockByHeightResponse,
    ) -> Result<TendermintHeader> {
        let pk = self
            .node
            .keyring()
            .iter()
            .next()
            .expect("validator key in keyring")
            .0;
        let block = block.ok_or(anyhow!("no block"))?;
        let header = block.header.ok_or(anyhow!("no header"))?;

        // the tendermint SignedHeader is non_exhaustive so we
        // can't use struct syntax to instantiate it and have to do
        // some annoying manual construction of the pb type instead.
        let h: tendermint::block::Header = header.clone().try_into().expect("bad header");
        use tendermint_proto::v0_37::types::SignedHeader as RawSignedHeader;
        // The SignedHeader is the header accompanied by the commit to prove it.
        let rsh: RawSignedHeader = RawSignedHeader {
            header: Some(tendermint_proto::v0_37::types::Header {
                version: Some(tendermint_proto::v0_37::version::Consensus {
                    block: header.version.as_ref().expect("version").block,
                    app: header.version.expect("version").app,
                }),
                chain_id: header.chain_id,
                height: header.height.into(),
                time: Some(tendermint_proto::google::protobuf::Timestamp {
                    seconds: header.time.as_ref().expect("time").seconds,
                    nanos: header.time.expect("time").nanos,
                }),
                last_block_id: header.last_block_id.clone().map(|a| {
                    tendermint_proto::v0_37::types::BlockId {
                        hash: a.hash,
                        part_set_header: a.part_set_header.map(|b| {
                            tendermint_proto::v0_37::types::PartSetHeader {
                                total: b.total,
                                hash: b.hash,
                            }
                        }),
                    }
                }),
                last_commit_hash: header.last_commit_hash.into(),
                data_hash: header.data_hash.into(),
                validators_hash: header.validators_hash.into(),
                next_validators_hash: header.next_validators_hash.into(),
                consensus_hash: header.consensus_hash.into(),
                app_hash: header.app_hash.into(),
                last_results_hash: header.last_results_hash.into(),
                evidence_hash: header.evidence_hash.into(),
                proposer_address: header.proposer_address.into(),
            }),
            commit: Some(tendermint_proto::v0_37::types::Commit {
                // The commit is for the current height
                height: header.height.into(),
                round: 0.into(),
                block_id: Some(tendermint_proto::v0_37::types::BlockId {
                    hash: h.hash().into(),
                    part_set_header: Some(tendermint_proto::v0_37::types::PartSetHeader {
                        total: 0,
                        hash: vec![],
                    }),
                }),
                // signatures for this block
                signatures: self
                    .node
                    .last_commit()
                    .unwrap()
                    .signatures
                    .clone()
                    .into_iter()
                    .map(Into::into)
                    .collect::<Vec<_>>(),
            }),
        };

        let signed_header = rsh.clone().try_into()?;

        // now get a SignedHeader
        let pub_key =
            tendermint::PublicKey::from_raw_ed25519(pk.as_bytes()).expect("pub key present");
        let proposer_address = tendermint::account::Id::new(
            <sha2::Sha256 as sha2::Digest>::digest(pk).as_slice()[0..20]
                .try_into()
                .expect(""),
        );
        // TODO: don't hardcode these
        let validator_set = tendermint::validator::Set::new(
            vec![tendermint::validator::Info {
                address: proposer_address.try_into()?,
                pub_key,
                power: Power::try_from(25_000 * 10i64.pow(6))?,
                name: Some("test validator".to_string()),
                proposer_priority: 1i64.try_into()?,
            }],
            // Same validator as proposer?
            Some(tendermint::validator::Info {
                address: proposer_address.try_into()?,
                pub_key,
                power: Power::try_from(25_000 * 10i64.pow(6))?,
                name: Some("test validator".to_string()),
                proposer_priority: 1i64.try_into()?,
            }),
        );

        // now we can make the Header
        let header = TendermintHeader {
            signed_header,
            validator_set: validator_set.clone(),
            trusted_validator_set: validator_set.clone(),
            trusted_height: trusted_height.unwrap_or_else(|| ibc_types::core::client::Height {
                revision_number: 0,
                revision_height: 0,
            }),
        };
        Ok(header)
    }
}

impl Drop for TestNodeWithIBC {
    fn drop(&mut self) {
        self.rpc_server.abort();
    }
}
