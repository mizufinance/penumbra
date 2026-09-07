#![allow(clippy::clone_on_copy)]
#![deny(clippy::unwrap_used)]
#![recursion_limit = "512"]

#[cfg(not(feature = "benchmark-system-allocator"))]
#[global_allocator]
static GLOBAL_ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::io::IsTerminal as _;
use std::{error::Error, process::exit};

use metrics_tracing_context::{MetricsLayer, TracingContextLayer};
use metrics_util::layers::Stack;

use anyhow::{anyhow, Context};
use cnidarium::Storage;
use decaf377_rdsa::{SpendAuth, VerificationKey};
use metrics_exporter_prometheus::PrometheusBuilder;
use pd::{
    cli::{NetworkCommand, Opt, RootCommand},
    network::{
        config::{get_network_dir, parse_tm_address, url_has_necessary_parts},
        generate::NetworkConfig,
        join::network_join,
    },
};
use rand::Rng;
use rand_core::OsRng;
use rustls::crypto::aws_lc_rs;
use shieldd_sdk_app::app_version::check_app_version;
use shieldd_sdk_app::{APP_VERSION, SUBSTORE_PREFIXES};
use shieldd_sdk_keys::ensure_nonidentity_spend_auth_key;
use shieldd_sdk_tower_trace::remote_addr;
use tendermint_config::net::Address as TendermintAddress;
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{prelude::*, EnvFilter};
use url::Url;

fn default_grpc_bind(
    grpc_bind: Option<std::net::SocketAddr>,
    grpc_auto_https: Option<&str>,
) -> std::net::SocketAddr {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    const HTTP_DEFAULT: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
    const HTTPS_DEFAULT: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 443);

    grpc_bind.unwrap_or_else(|| {
        if grpc_auto_https.is_some() {
            HTTPS_DEFAULT
        } else {
            HTTP_DEFAULT
        }
    })
}

fn parse_spend_vk_hex(value: &str) -> anyhow::Result<VerificationKey<SpendAuth>> {
    let bytes = hex::decode(value).context("invalid --compliance-registrar-vk-hex")?;
    if bytes.len() != 32 {
        anyhow::bail!("--compliance-registrar-vk-hex must be exactly 64 hex chars");
    }
    let key = VerificationKey::<SpendAuth>::try_from(bytes.as_slice())
        .map_err(|_| anyhow!("invalid --compliance-registrar-vk-hex encoding"))?;
    ensure_nonidentity_spend_auth_key(&key, "compliance registrar key")?;
    Ok(key)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Validate options immediately.
    let Opt { cmd } = <Opt as clap::Parser>::parse();

    let metrics_layer = MetricsLayer::new();
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_ansi(std::io::stdout().is_terminal())
        .with_target(true);
    let filter_layer = EnvFilter::try_from_default_env().or_else(|_| EnvFilter::try_new("info"))?;

    let registry = tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .with(metrics_layer);
    registry.init();

    // Initialize HTTPS support
    // We log the error and continue, so the node is operational.
    if let Err(e) = aws_lc_rs::default_provider().install_default() {
        tracing::error!("failed to initialize rustls support: {:?}", e);
    }

    tracing::info!(?cmd, version = env!("CARGO_PKG_VERSION"), "running command");
    match cmd {
        RootCommand::Start {
            home,
            abci_bind,
            grpc_bind,
            grpc_auto_https,
            acme_staging,
            metrics_bind,
            cometbft_addr,
            enable_expensive_rpc,
        } => {
            let grpc_bind = default_grpc_bind(grpc_bind, grpc_auto_https.as_deref());

            // Ensure we have all necessary parts in the URL
            if !url_has_necessary_parts(&cometbft_addr) {
                anyhow::bail!(
                    "Failed to parse '--cometbft-addr' as URL: {}",
                    cometbft_addr
                )
            }

            // Unpack home directory. Accept an explicit path, but default
            // to a sane value if unspecified.
            let pd_home = match home {
                Some(h) => h,
                None => get_network_dir(None).join("node0").join("pd"),
            };
            let rocksdb_home = pd_home.join("rocksdb");

            let mut storage = Storage::load(rocksdb_home.clone(), SUBSTORE_PREFIXES.to_vec())
                .await
                .context(
                    "Unable to initialize RocksDB storage - is there another `pd` process running?",
                )?;
            check_app_version(&storage).await?;
            let generation_packs = shieldd_sdk_sct::generation_pack::GenerationPackRepository::new(
                pd_home.join(pd::nullifier_generation_packs::DIRECTORY),
                1,
            )?;
            if pd::nullifier_generation_packs::prepare(&storage, &generation_packs)
                .await
                .context("could not prepare retired nullifier generation packs")?
            {
                storage.release().await;
                storage = Storage::load(rocksdb_home, SUBSTORE_PREFIXES.to_vec())
                    .await
                    .context("could not reopen storage after generation pack maintenance")?;
            }

            tracing::info!(
                APP_VERSION,
                ?abci_bind,
                ?grpc_bind,
                ?grpc_auto_https,
                ?acme_staging,
                ?metrics_bind,
                %cometbft_addr,
                ?enable_expensive_rpc,
                "starting pd"
            );

            if shieldd_sdk_app::app::App::is_ready(storage.latest_snapshot()).await {
                tracing::info!("application ready to start");
            } else {
                tracing::warn!("application is halted, refusing to start");
                exit(0)
            }

            let abci_server = tokio::task::spawn(
                shieldd_sdk_app::server::new(storage.clone()).listen_tcp(abci_bind),
            );

            let tm_proxy = shieldd_sdk_tendermint_proxy::TendermintProxy::new(cometbft_addr);

            let _generation_pack_worker = pd::nullifier_generation_packs::spawn_worker(
                storage.clone(),
                generation_packs.clone(),
            );
            let grpc_routes = shieldd_sdk_app::rpc::routes_with_generation_packs(
                &storage,
                tm_proxy,
                enable_expensive_rpc,
                generation_packs,
            )?
            .into_axum_router()
            .layer(ServiceBuilder::new().layer(
                TraceLayer::new_for_grpc().make_span_with(
                    |req: &http::Request<_>| match remote_addr(req) {
                        Some(remote_addr) => {
                            tracing::error_span!("grpc", ?remote_addr)
                        }
                        None => tracing::error_span!("grpc"),
                    },
                ),
            ));

            // Create Axum routes for the frontend app.
            let frontend = pd::zipserve::router("/app/", pd::MINIFRONT_ARCHIVE_BYTES);
            let node_status = pd::zipserve::router("/", pd::NODE_STATUS_ARCHIVE_BYTES);

            // Now we drop down a layer of abstraction, from tonic to axum, and merge handlers.
            let router = grpc_routes
                .merge(frontend)
                .merge(node_status)
                // Set rather permissive CORS headers for pd's gRPC: the service
                // should be accessible from arbitrary web contexts, such as localhost,
                // or any FQDN that wants to reference its data.
                .layer(CorsLayer::permissive());

            let make_svc = router.into_make_service();

            // Now start the GRPC server, initializing an ACME client to use as a certificate
            // resolver if auto-https has been enabled. if auto-https is not enabled, we will
            // instead spawn a future that will never return.

            // TODO(janis): Is `axum_server::bind` sufficient to accept http1 (for grpc-web) and
            // http2 (for grpc) requests?
            //
            // See also this (about axum::serve, suggesting that it just works out of the box; does that
            // apply to axum_server::bind as well?)
            // https://github.com/tokio-rs/axum/blob/c596deafe48ed608775e312eef7d12ddbb0fd424/examples/websockets-http2/src/main.rs#L57-L59
            let grpc_server = axum_server::bind(grpc_bind);
            let (grpc_server, acme_worker) = match grpc_auto_https {
                Some(domain) => {
                    let (acceptor, acme_worker) =
                        shieldd_sdk_auto_https::axum_acceptor(pd_home, domain, !acme_staging);
                    let acme_worker = tokio::spawn(acme_worker);
                    let grpc_server =
                        tokio::task::spawn(grpc_server.acceptor(acceptor).serve(make_svc));
                    (grpc_server, acme_worker)
                }
                None => {
                    let acme_worker = tokio::task::spawn(futures::future::pending());
                    let grpc_server = tokio::task::spawn(grpc_server.serve(make_svc));
                    (grpc_server, acme_worker)
                }
            };

            // Configure a Prometheus recorder and exporter.
            let (recorder, exporter) = PrometheusBuilder::new()
                .with_http_listener(metrics_bind)
                .build()
                .map_err(|e| {
                    let msg = format!(
                        "failed to build prometheus recorder; make sure {} is available",
                        &metrics_bind
                    );
                    tracing::error!(?e, ?msg);
                    anyhow!(msg)
                })?;

            Stack::new(recorder)
                // Adding the `TracingContextLayer` will add labels from the tracing span to metrics.
                // The only labels to be included are "chain_id" and "role".
                .push(TracingContextLayer::only_allow(["chain_id", "role"]))
                .install()
                .expect("global recorder already installed");

            // Spawn the HTTP service that lets Prometheus pull metrics from `pd`, and then
            // register pd's metrics with the exporter.
            tokio::spawn(exporter);
            pd::register_metrics();
            tokio::spawn(pd::metrics::sleep_worker::run());
            tokio::spawn(pd::metrics::cpu_worker::run());

            // We error out if a service errors, rather than keep running.
            // A special attempt is made to detect whether binding to target socket failed;
            // if so, we report that error explicitly, otherwise we fall back to reporting
            // whatever the error was.
            tokio::select! {
                x = abci_server => x?.map_err(|e| {
                    // The display impl on the ABCI error is sufficiently informative,
                    // so we don't need special handling of the failed-to-bind case.
                    let msg = format!("abci server on {} failed: {}", abci_bind, e);
                    tracing::error!("{}", msg);
                    anyhow::anyhow!(msg)
                }
                )?,

                x = grpc_server => x?.map_err(|e| {
                    let mut msg = format!("grpc server on {} failed: {}", grpc_bind, e);
                    // Detect if we have a bind error. We need to unpack nested errors, from
                    // tonic -> hyper -> std. Otherwise, only "transport error" is reported,
                    // which isn't informative enough to take action.
                    if let Some(e) = e.source() {
                        if let Some(e) = e.source() {
                            if let Some(e) = e.downcast_ref::<std::io::Error>() {
                                if e.kind().to_string().contains("address in use") {
                                    msg = format!("grpc bind socket already in use: {}", grpc_bind);
                                }
                            }
                        }
                    }
                    tracing::error!("{}", msg);
                    anyhow::anyhow!(msg)
                }
                )?,

                // if the acme worker returns an error, let's propagate it!
                x = acme_worker => x?.map_err(|error| {
                    let msg = format!("acme worker failed: {error}");
                    tracing::error!("{}", msg);
                    anyhow::anyhow!(msg)
                })?,
            };
        }

        RootCommand::Network {
            net_cmd: NetworkCommand::UnsafeResetAll {},
            network_dir,
        } => {
            let network_dir = get_network_dir(network_dir);
            if network_dir.exists() {
                tracing::info!("removing network directory: {}", network_dir.display());
                std::fs::remove_dir_all(network_dir)?;
            } else {
                tracing::info!(
                    "network directory does not exist, so not removing: {}",
                    network_dir.display()
                );
            }
        }

        RootCommand::Network {
            net_cmd:
                NetworkCommand::Join {
                    node,
                    archive_url,
                    moniker,
                    external_address,
                    tendermint_rpc_bind,
                    tendermint_p2p_bind,
                    leave_archive,
                },
            network_dir,
        } => {
            let output_dir = get_network_dir(network_dir);

            // If the output directory already exists, bail out, rather than overwriting.
            if output_dir.exists() {
                anyhow::bail!(
                    "output directory {:?} already exists, refusing to overwrite it",
                    output_dir
                );
            }

            // Check whether an external address was set, and parse as TendermintAddress.
            let external_address: Option<TendermintAddress> = match external_address {
                Some(a) => {
                    let u = Url::parse(format!("tcp://{}", a).as_str())?;
                    parse_tm_address(None, &u).ok()
                }
                None => None,
            };

            // Set custom moniker, or default to random string suffix.
            let node_name = match moniker {
                Some(m) => m,
                None => format!("node-{}", hex::encode(OsRng.gen::<u32>().to_le_bytes())),
            };

            // Join the target network, looking up network info and writing
            // local configs for pd and cometbft.
            network_join(
                output_dir.clone(),
                node,
                &node_name,
                external_address,
                tendermint_rpc_bind,
                tendermint_p2p_bind,
            )
            .await?;

            // Download and extract archive URL, if set.
            if let Some(archive_url) = archive_url {
                pd::network::join::unpack_state_archive(archive_url, output_dir, leave_archive)
                    .await?;
            }
        }

        RootCommand::Network {
            net_cmd:
                NetworkCommand::Generate {
                    peer_address_template,
                    timeout_commit,
                    epoch_duration,
                    allocations_input_file,
                    allocation_address,
                    validators_input_file,
                    chain_id,
                    gas_price_simple,
                    compliance_registrar_vk_hex,
                    preserve_chain_id,
                    external_addresses,
                    tendermint_rpc_bind,
                    tendermint_p2p_bind,
                },
            network_dir,
        } => {
            // Build script computes the latest testnet name and sets it as an env variable
            let chain_id = match preserve_chain_id {
                true => chain_id.unwrap_or_else(|| "shieldd-localnet".to_string()),
                false => {
                    // If preserve_chain_id is false, we append a random suffix to avoid collisions
                    let randomizer = OsRng.gen::<u32>();
                    let chain_id = chain_id.unwrap_or_else(|| "shieldd-localnet".to_string());
                    // We insert an 'x' in the randomized hex string to ensure it's not parsed as a
                    // revision id.
                    format!("{}-x{}", chain_id, hex::encode(randomizer.to_le_bytes()))
                }
            };

            let output_dir = get_network_dir(network_dir);
            // If the output directory already exists, bail out, rather than overwriting.
            if output_dir.exists() {
                anyhow::bail!(
                    "output directory {:?} already exists, refusing to overwrite it",
                    output_dir
                );
            }

            // Unpack external address information into a vec, since there could be multiple
            // values. We don't yet know how many validators will be in the genesis, but the
            // Testnet::generate constructor will assert that the number of external addresses,
            // if Some, is equal to the number of validators.
            let external_addresses: anyhow::Result<Vec<TendermintAddress>> =
                match external_addresses {
                    Some(a) => a
                        .split(',')
                        .map(|x| {
                            x.parse()
                                .context(format!("Failed to parse external address: {x}"))
                        })
                        .collect(),
                    None => Ok(Vec::new()),
                };

            let external_addresses = external_addresses?;
            let compliance_registrar_vk = compliance_registrar_vk_hex
                .iter()
                .map(|vk| parse_spend_vk_hex(vk))
                .collect::<anyhow::Result<Vec<_>>>()?;

            // Build and write local configs based on input flags.
            tracing::info!(?chain_id, "Generating network config");
            let t = NetworkConfig::generate(
                &chain_id,
                Some(output_dir),
                peer_address_template,
                Some(external_addresses),
                allocations_input_file,
                allocation_address,
                validators_input_file,
                timeout_commit,
                epoch_duration,
                gas_price_simple,
                compliance_registrar_vk,
                tendermint_rpc_bind,
                tendermint_p2p_bind,
            )?;
            tracing::info!(
                n_validators = t.network_validators.len(),
                chain_id = %t.genesis.chain_id,
                "Writing config files for network"
            );
            t.write_configs()?;
        }
        RootCommand::Export {
            home,
            export_directory,
            export_archive,
        } => {
            use fs_extra;

            // Export state as directory.
            let src_rocksdb_dir = home.join("rocksdb");
            tracing::info!(
                "copying node state {} -> {}",
                src_rocksdb_dir.display(),
                export_directory.display()
            );
            std::fs::create_dir_all(&export_directory)?;
            let copy_opts = fs_extra::dir::CopyOptions::new();
            fs_extra::copy_items(
                &[src_rocksdb_dir.as_path()],
                export_directory.as_path(),
                &copy_opts,
            )?;
            let src_pack_dir = home.join("nullifier-generation-packs");
            if src_pack_dir.is_dir() {
                fs_extra::copy_items(
                    &[src_pack_dir.as_path()],
                    export_directory.as_path(),
                    &copy_opts,
                )?;
            }
            tracing::info!("finished copying node state");

            // Compress to tarball if requested.
            if let Some(archive_filepath) = export_archive {
                pd::export::archive_directory(export_directory.clone(), archive_filepath.clone())?;
                tracing::info!("export complete: {}", archive_filepath.display());
            } else {
                // Provide friendly "OK" message that's still accurate without archiving.
                tracing::info!("export complete: {}", export_directory.display());
            }
        }
        RootCommand::NullifierGenerationPack {
            home,
            generation,
            prune,
        } => {
            use cnidarium::StateDelta;
            use shieldd_sdk_sct::generation_pack::GenerationPackRepository;
            use shieldd_sdk_sct::nullifier_tree::{
                build_generation_pack, prune_packed_generation, record_generation_pack_completion,
            };

            let storage = Storage::load(home.join("rocksdb"), SUBSTORE_PREFIXES.to_vec()).await?;
            let mut delta = StateDelta::new(storage.latest_snapshot());
            let pack = build_generation_pack(&delta, generation).await?;
            let repository =
                GenerationPackRepository::new(home.join("nullifier-generation-packs"), 1)?;
            let receipt = repository.write(&pack)?;
            let archived =
                shieldd_sdk_sct::nullifier_tree::archived_generation(&delta, generation).await?;
            anyhow::ensure!(
                repository.verify(archived)? == receipt,
                "generation pack changed after its durable write"
            );
            record_generation_pack_completion(&mut delta, &receipt).await?;
            let deleted = if prune {
                prune_packed_generation(&mut delta, &receipt).await?
            } else {
                0
            };
            storage.commit_in_place(delta).await?;
            tracing::info!(
                generation,
                pack = %repository.path(generation).display(),
                sha256 = %hex::encode(receipt.pack_sha256),
                leaves = receipt.leaf_count,
                bytes = receipt.byte_length,
                deleted,
                "nullifier generation pack complete"
            );
        }
        RootCommand::NullifierGenerationPackVerify { home, generation } => {
            use shieldd_sdk_sct::generation_pack::GenerationPackRepository;

            let storage = Storage::load(home.join("rocksdb"), SUBSTORE_PREFIXES.to_vec()).await?;
            let archived = shieldd_sdk_sct::nullifier_tree::archived_generation(
                &storage.latest_snapshot(),
                generation,
            )
            .await?;
            let repository = GenerationPackRepository::new(
                home.join(pd::nullifier_generation_packs::DIRECTORY),
                1,
            )?;
            let receipt = repository.verify(archived)?;
            tracing::info!(
                generation,
                pack = %repository.path(generation).display(),
                sha256 = %hex::encode(receipt.pack_sha256),
                leaves = receipt.leaf_count,
                bytes = receipt.byte_length,
                "nullifier generation pack verified"
            );
        }
    }
    Ok(())
}
