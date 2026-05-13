# External Integrations

**Analysis Date:** 2026-05-12

## APIs & External Services

**Consensus / Node Networking:**
- CometBFT / Tendermint - drives the Penumbra application over ABCI and exposes JSON-RPC/P2P endpoints.
  - SDK/Client: `tendermint`, `tendermint-rpc`, `tendermint-proto`, `tower-abci` in `Cargo.toml`; proxy implementation in `crates/util/tendermint-proxy/src/lib.rs`.
  - Auth: Not detected for local node RPC; bind/endpoints are configured by `PENUMBRA_PD_ABCI_BIND`, `PENUMBRA_PD_COMETBFT_PROXY_URL`, `PENUMBRA_PD_TM_RPC_BIND`, `PENUMBRA_PD_TM_P2P_BIND`, `PENUMBRA_NODE_CMT_URL`, and `PENUMBRA_COMETBFT_*_PORT`.
- Penumbra gRPC / grpc-web - public node query surface, local view service, custody service, and CLI client transport.
  - SDK/Client: `tonic`, `tonic-web`, `prost`, `axum`, `tower` in `Cargo.toml`; node route assembly in `crates/core/app/src/rpc.rs`; daemon service assembly in `crates/bin/pclientd/src/lib.rs`; client connection handling in `crates/view/src/service.rs`.
  - Auth: No network auth provider detected; wallet signing authority is handled by local custody backends in `crates/custody/**` and `crates/bin/pcli/src/config.rs`.
- Cosmos / IBC / ICS23 - protocol integration for IBC messages, transfer queries, light-client verification, and vendored proto dependencies.
  - SDK/Client: `ibc-types`, `ibc-proto`, `ics23`, Cosmos/IBC modules in `proto/buf.yaml`; IBC component code under `crates/core/component/ibc`.
  - Auth: Chain-level transaction authorization through Penumbra custody keys; no separate external auth provider detected.

**Compliance / PRE:**
- Orbis runtime and SourceHub - policy, DKG, secret storage, proxy re-encryption, bulletin, and ACP operations for compliance/audit workflows.
  - SDK/Client: `penumbra-orbis-client` in `crates/util/orbis-client/Cargo.toml`; pinned `orbis-rs` git crates at rev `d5889bd777bbac7bf97a8e89a2556116f2740ceb`; local compose contract in `deployments/orbis/README.md` and `deployments/orbis/docker-compose.yml`.
  - Auth: Orbis JWT signing and DID key generation in `crates/util/orbis-client/src/auth.rs`; SourceHub endpoint configuration via `ORBIS_SOURCEHUB_RPC`, `ORBIS_SOURCEHUB_REST`, `ORBIS_SOURCEHUB_GRPC`, `ORBIS_SOURCEHUB_*_PORT`, `ORBIS_SOURCEHUB_CHAIN_ID`, and `ORBIS_SOURCEHUB_DENOM`.
- Orbis audit CLI - fetches Penumbra transactions, stores encrypted seed packages, registers objects/relationships, starts PRE requests, and writes audit output.
  - SDK/Client: `OrbisClient` in `crates/util/orbis-client/src/client.rs`; audit flow in `crates/bin/orbis-audit/src/main.rs`; integration orchestration in `crates/bin/orbis-integration/src/main.rs`.
  - Auth: `PENUMBRA_NODE_PD_URL` for Penumbra node access; `ORBIS_NODE*_ENDPOINT` / `ORBIS_ENDPOINT` for Orbis gRPC access; Orbis request auth is JWT-based in `crates/util/orbis-client/src/client.rs`.

**Proof Artifacts / Cryptography:**
- Git LFS for proving keys - optional build-time proving-key download when `download-proving-keys` is enabled.
  - SDK/Client: blocking `reqwest` build dependency in `crates/crypto/proof-params/Cargo.toml`; Git LFS Batch API client in `crates/crypto/proof-params/build.rs`.
  - Auth: Not detected in code; downloads target the repository's Git LFS batch endpoint.
- gnark runtime - Go-based Groth16 prover boundary through C-shared libraries or a prover daemon.
  - SDK/Client: Go module `tools/gnark/go.mod`; Rust transport loader in `crates/core/component/shielded-pool/src/gnark/transport.rs`; artifacts under `tools/gnark/artifacts/**`.
  - Auth: Not applicable; local library/daemon paths are selected by `PENUMBRA_GNARK_*` environment variables.

**External Registries / Package Services:**
- Buf Schema Registry - protobuf dependency source and published Penumbra module.
  - SDK/Client: `buf` configs in `proto/buf.yaml` and `proto/buf.gen.yaml`; generation script in `deployments/scripts/protobuf-codegen`; publish workflow in `.github/workflows/buf-push.yml`.
  - Auth: `BUF_TOKEN` GitHub Actions secret for publishing.
- crates.io - Rust crate publishing for workspace releases.
  - SDK/Client: cargo-release metadata in `Cargo.toml`; publish script `deployments/scripts/publish-crates`; workflow `.github/workflows/crates.yml`.
  - Auth: `CARGO_REGISTRY_TOKEN` GitHub Actions secret.
- GitHub Container Registry - OCI image publishing.
  - SDK/Client: Docker Buildx workflow in `.github/workflows/containers.yml`; image build in `deployments/containerfiles/Dockerfile`.
  - Auth: `GITHUB_TOKEN` in GitHub Actions.
- GitHub Releases / cargo-dist - binary release hosting.
  - SDK/Client: `dist-workspace.toml` and `.github/workflows/release.yml`.
  - Auth: `GITHUB_TOKEN` in GitHub Actions.
- Firebase Hosting configs - docs/protobuf/rustdoc hosting configuration files.
  - SDK/Client: `docs/protocol/firebase.json`, `docs/protobuf/firebase.json`, `docs/rustdoc/firebase.json`.
  - Auth: Deployment credentials are not detected in repo files.

**HTTP Data Sources:**
- Prax wallet asset registry - optional `pclientd load-registry` source for asset metadata.
  - SDK/Client: `reqwest::get` in `crates/bin/pclientd/src/lib.rs`; default registry URL is built in `Command::LoadRegistry`.
  - Auth: Not detected.
- Node archive/bootstrap URLs - `pd network join` can fetch a remote archive and query a remote CometBFT node.
  - SDK/Client: `reqwest` and `tendermint-rpc` in `crates/bin/pd/src/network/join.rs`.
  - Auth: Not detected; configured by `PENUMBRA_PD_JOIN_URL` and `PENUMBRA_PD_ARCHIVE_URL`.
- Let's Encrypt ACME - optional automatic HTTPS certificate provisioning for `pd`.
  - SDK/Client: `rustls-acme` in `crates/util/auto-https/Cargo.toml`; CLI flags in `crates/bin/pd/src/cli.rs`; wiring in `crates/bin/pd/src/main.rs`.
  - Auth: ACME account/certificate cache stored under the `pd` home directory by `rustls-acme`; no static repo secret detected.

**Hardware:**
- Ledger USB devices - optional hardware custody backend for `pcli`.
  - SDK/Client: `ledger-lib`, `ledger-proto` in `Cargo.toml`; APDU implementation in `crates/custody-ledger-usb/src/device.rs`; config enum in `crates/bin/pcli/src/config.rs`.
  - Auth: User approval/PIN happens on the device; no repo secret detected.

## Data Storage

**Databases:**
- RocksDB/Cnidarium state store
  - Connection: local filesystem under `PENUMBRA_PD_HOME` / generated network data; `pd` opens `rocksdb` under the node home in `crates/bin/pd/src/main.rs`.
  - Client: `cnidarium::Storage`, `rocksdb`, and `jmt` from `Cargo.toml`.
- SQLite view database
  - Connection: local pcli/pclientd home paths; `pclientd` uses `pclientd-db.sqlite` in `crates/bin/pclientd/src/lib.rs`; view storage opens SQLite in `crates/view/src/storage.rs`.
  - Client: `r2d2_sqlite`, `rusqlite`, and WAL-mode connection setup in `crates/view/src/storage.rs`.
- SQLite compliance scanner store
  - Connection: caller-supplied scanner database path; `SqliteScannerStore::new` accepts a local path in `crates/core/component/compliance/src/scanner/storage.rs`.
  - Client: optional `rusqlite` feature in `crates/core/component/compliance/Cargo.toml`.
- PostgreSQL event/index databases
  - Connection: `--src-database-url` and `--dst-database-url` CLI args in `crates/util/cometindex/src/opt.rs`; local port controlled by `PENUMBRA_POSTGRES_PORT` in `deployments/compose/process-compose-postgres.yml`.
  - Client: `sqlx::PgPool` in `crates/util/cometindex/src/database.rs`; schema seed in `crates/util/cometindex/vendor/schema.sql`.

**File Storage:**
- Local filesystem homes for node, wallet, and view daemons in `crates/bin/pd/src/cli.rs`, `crates/bin/pcli/src/opt.rs`, and `crates/bin/pclientd/src/lib.rs`.
- Local generated devnet/testnet files under `testnets/**`, `deployments/000-localnet/**`, and `${PENUMBRA_DEVNET_HOME}/network_data` from `deployments/scripts/run-local-devnet.sh` and `scripts/penumbra-up.sh`.
- Bundled static archives served by `pd` through `crates/bin/pd/src/main.rs` and `crates/bin/pd/src/zipserve.rs`.
- gnark artifacts and proving/verifying keys under `tools/gnark/artifacts/**`.
- Orbis/Penumbra integration artifacts under `tmp/**` and `COMPLIANCE_TMP` from `scripts/lib/common.sh`; `tmp/` is local/generated.

**Caching:**
- ACME certificate cache via `rustls-acme` DirCache in `crates/util/auto-https/src/lib.rs`.
- Nix, Rust, and build caches in CI through `.github/actions/setup-nix-rust/action.yml`.
- Cargo-dist artifact cache in `.github/workflows/release.yml`.
- No Redis/Memcached/application cache service detected.

## Authentication & Identity

**Auth Provider:**
- Penumbra custody is custom/local:
  - Implementation: `SoftKms`, encrypted custody, threshold custody, view-only custody, and optional Ledger custody in `crates/custody/**`, `crates/custody-ledger-usb/**`, and `crates/bin/pcli/src/config.rs`.
- Orbis request authentication:
  - Implementation: JWT signer and deterministic DID key helper in `crates/util/orbis-client/src/auth.rs`; authenticated request construction in `crates/util/orbis-client/src/client.rs`.
- CI/service auth:
  - Implementation: GitHub Actions secrets referenced by workflows: `BUF_TOKEN` in `.github/workflows/buf-push.yml`, `CARGO_REGISTRY_TOKEN` in `.github/workflows/crates.yml`, and `GITHUB_TOKEN` in `.github/workflows/containers.yml` and `.github/workflows/release.yml`.
- Public node gRPC:
  - Implementation: no auth middleware detected in `crates/core/app/src/rpc.rs` or `crates/bin/pd/src/main.rs`; access is controlled by bind address, TLS, and deployment networking.

## Monitoring & Observability

**Error Tracking:**
- External error tracking service: None detected.

**Logs:**
- Rust services use `tracing` and `tracing-subscriber` with `RUST_LOG` / `EnvFilter` in `crates/bin/pd/src/main.rs`, `crates/bin/pclientd/src/main.rs`, and `crates/bin/pcli/src/opt.rs`.
- Local process-compose logs are combined in `deployments/logs/dev-env-combined.log` by `deployments/compose/process-compose.yml`.
- Orbis integration failure logs are captured in `.github/workflows/orbis-integration.yml`.

**Metrics:**
- `pd` exports Prometheus metrics through `metrics-exporter-prometheus` on `PENUMBRA_PD_METRICS_BIND` from `crates/bin/pd/src/main.rs`.
- Local Prometheus/Grafana stack is configured by `deployments/compose/process-compose-metrics.yml`, `deployments/config/prometheus/prometheus.yml`, and `deployments/config/grafana/**`.
- Orbis compose exposes node metrics ports through `deployments/orbis/docker-compose.yml`.

## CI/CD & Deployment

**Hosting:**
- GitHub Container Registry for OCI images (`.github/workflows/containers.yml`, `deployments/containerfiles/Dockerfile`).
- GitHub Releases for cargo-dist archives/installers (`.github/workflows/release.yml`, `dist-workspace.toml`).
- crates.io for published Rust crates (`.github/workflows/crates.yml`, `Cargo.toml` release metadata).
- Buf Schema Registry for protobuf modules (`.github/workflows/buf-push.yml`, `proto/buf.yaml`).
- Firebase Hosting configs for documentation outputs (`docs/protocol/firebase.json`, `docs/protobuf/firebase.json`, `docs/rustdoc/firebase.json`).
- Systemd example deployment for `pd` + CometBFT (`deployments/systemd/penumbra.service`, `deployments/systemd/cometbft.service`).

**CI Pipeline:**
- Rust lint/features/test/gnark jobs in `.github/workflows/rust.yml`.
- Orbis integration workflow in `.github/workflows/orbis-integration.yml`.
- Smoke workflow in `.github/workflows/smoke.yml`.
- Container workflow in `.github/workflows/containers.yml`.
- Release workflow in `.github/workflows/release.yml`.
- Crate publishing workflow in `.github/workflows/crates.yml`.
- Protobuf workflows in `.github/workflows/buf-pull-request.yml` and `.github/workflows/buf-push.yml`.
- Docs lint workflow in `.github/workflows/docs-lint.yml`.

## Environment Configuration

**Required env vars:**
- Node daemon: `PENUMBRA_PD_HOME`, `PENUMBRA_PD_ABCI_BIND`, `PENUMBRA_PD_GRPC_BIND`, `PENUMBRA_PD_METRICS_BIND`, `PENUMBRA_PD_COMETBFT_PROXY_URL`, `PENUMBRA_PD_TM_RPC_BIND`, `PENUMBRA_PD_TM_P2P_BIND`, `PENUMBRA_PD_JOIN_URL`, `PENUMBRA_PD_ARCHIVE_URL`.
- Wallet/view clients: `PENUMBRA_NODE_PD_URL`, `PENUMBRA_PCLI_HOME`, `PENUMBRA_PCLIENTD_HOME`, `PENUMBRA_NODE_PD_METRICS_URL`.
- Local devnet/compliance stack: `COMPLIANCE_TMP`, `PENUMBRA_ORBIS_HOME`, `PENUMBRA_DEVNET_HOME`, `PENUMBRA_PD_GRPC_PORT`, `PENUMBRA_COMETBFT_RPC_PORT`, `PENUMBRA_COMETBFT_P2P_PORT`, `PENUMBRA_POSTGRES_PORT`, `PENUMBRA_PCLIENTD_PORT_BASE`, `PENUMBRA_NODE_CMT_URL`.
- gnark proof runtime: `PENUMBRA_GNARK_TRANSFER_ARTIFACT_DIR`, `PENUMBRA_GNARK_TRANSFER_LIB`, `PENUMBRA_GNARK_TRANSFER_DAEMON`, `PENUMBRA_GNARK_SPLIT_ARTIFACT_DIR`, `PENUMBRA_GNARK_SPLIT_LIB`, `PENUMBRA_GNARK_SPLIT_DAEMON`, `PENUMBRA_GNARK_CONSOLIDATE_ARTIFACT_DIR`, `PENUMBRA_GNARK_CONSOLIDATE_LIB`, `PENUMBRA_GNARK_CONSOLIDATE_DAEMON`, `PENUMBRA_GNARK_SHIELDED_ICS20_WITHDRAWAL_ARTIFACT_DIR`, `PENUMBRA_GNARK_SHIELDED_ICS20_WITHDRAWAL_LIB`, `PENUMBRA_GNARK_SHIELDED_ICS20_WITHDRAWAL_DAEMON`.
- Orbis/SourceHub local stack: `ORBIS_COMPOSE_PROJECT_NAME`, `ORBIS_RUNTIME_REPO`, `ORBIS_RUNTIME_REF`, `ORBIS_RUNTIME_CONTEXT`, `ORBIS_SOURCEHUB_RPC`, `ORBIS_SOURCEHUB_REST`, `ORBIS_SOURCEHUB_GRPC`, `ORBIS_SOURCEHUB_*_PORT`, `ORBIS_NODE1_ENDPOINT`, `ORBIS_NODE2_ENDPOINT`, `ORBIS_NODE3_ENDPOINT`, `ORBIS_NODE*_GRPC_PORT`, `ORBIS_NODE*_METRICS_PORT`, `ORBIS_ENDPOINT`.
- Performance/debug knobs: `RUST_LOG`, `RUST_BACKTRACE`, `PENUMBRA_MEMPOOL_CHECKTX_CONCURRENCY`, `PENUMBRA_MEMPOOL_CHECKTX_HEAVYWORK_CONCURRENCY`, `PENUMBRA_MAX_TRANSACTION_SIZE_BYTES`, `PENUMBRA_PREPARE_PROPOSAL_FILTER_CONCURRENCY`, `PENUMBRA_AGGREGATE_DEBUG_DIR`.
- CI secrets: `BUF_TOKEN`, `CARGO_REGISTRY_TOKEN`, and `GITHUB_TOKEN`.

**Secrets location:**
- Runtime wallet keys and custody material are stored in local `pcli`/`pclientd` home config and SQLite files, not in repo source (`crates/bin/pcli/src/config.rs`, `crates/bin/pclientd/src/lib.rs`).
- GitHub Actions secrets are referenced by name in workflow files and are not stored in repo (`.github/workflows/buf-push.yml`, `.github/workflows/crates.yml`, `.github/workflows/containers.yml`, `.github/workflows/release.yml`).
- Orbis local compose includes test-only credential environment entries in `deployments/orbis/docker-compose.yml`; do not reuse them outside local integration.
- `.env` files are not detected; `.envrc.example` exists at repo root.

## Webhooks & Callbacks

**Incoming:**
- ABCI TCP server for CometBFT driving `pd` (`crates/bin/pd/src/cli.rs`, `crates/core/app/src/server.rs`).
- Public `pd` gRPC/grpc-web and reflection routes (`crates/bin/pd/src/main.rs`, `crates/core/app/src/rpc.rs`).
- `pclientd` gRPC/grpc-web view/custody/proxy routes (`crates/bin/pclientd/src/lib.rs`).
- Prometheus scrape endpoint for `pd` metrics (`crates/bin/pd/src/main.rs`, `deployments/config/prometheus/prometheus.yml`).
- Orbis local SourceHub and node gRPC/metrics ports in `deployments/orbis/docker-compose.yml`.
- CometBFT RPC/P2P ports in `deployments/compose/docker-compose.yml`, `deployments/compose/docker-compose.dev.yml`, and `deployments/compose/process-compose.yml`.

**Outgoing:**
- `pd` to CometBFT JSON-RPC through the Tendermint proxy (`crates/bin/pd/src/main.rs`, `crates/util/tendermint-proxy/src/lib.rs`).
- `pcli` / `pclientd` / view worker to Penumbra node gRPC (`crates/bin/pcli/src/opt.rs`, `crates/bin/pclientd/src/lib.rs`, `crates/view/src/service.rs`).
- `pindexer` to Postgres source/destination databases (`crates/util/cometindex/src/opt.rs`, `crates/util/cometindex/src/database.rs`).
- Orbis client to Orbis gRPC services and SourceHub RPC/REST/gRPC (`crates/util/orbis-client/src/client.rs`).
- `pclientd load-registry` to asset registry HTTP/file sources (`crates/bin/pclientd/src/lib.rs`).
- proof-params build script to Git LFS Batch API when `download-proving-keys` is enabled (`crates/crypto/proof-params/build.rs`).
- Buf workflows/scripts to Buf Schema Registry (`proto/buf.yaml`, `deployments/scripts/protobuf-codegen`, `.github/workflows/buf-push.yml`).
- CI release/publish workflows to GitHub Releases, GHCR, crates.io, and artifact upload services (`.github/workflows/release.yml`, `.github/workflows/containers.yml`, `.github/workflows/crates.yml`).

---

*Integration audit: 2026-05-12*
