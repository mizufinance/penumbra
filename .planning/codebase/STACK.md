# Technology Stack

**Analysis Date:** 2026-05-12

## Languages

**Primary:**
- Rust 1.89, edition 2021 - Production workspace in `Cargo.toml`, with crates under `crates/**` and workspace binaries in `crates/bin/pd`, `crates/bin/pcli`, `crates/bin/pclientd`, `crates/bin/pindexer`, `crates/bin/orbis-audit`, and `crates/bin/orbis-integration`.

**Secondary:**
- Go 1.25.4 - gnark proving runtime and circuit tooling in `tools/gnark/go.mod`; the production container also installs Go 1.25.4 in `deployments/containerfiles/Dockerfile`.
- JavaScript - D3-based TCT visualization helper in `crates/misc/tct-visualize/package.json`.
- Protocol Buffers - API and domain wire contracts in `proto/penumbra/**`; generated Rust bindings live in `crates/proto/src/gen`.
- Nix - reproducible dev/build shells in `flake.nix` and pinned inputs in `flake.lock`.
- Bash/YAML/TOML - local orchestration and deployment tooling in `justfile`, `scripts/**`, `deployments/**`, `.github/workflows/**`, and `dist-workspace.toml`.

## Runtime

**Environment:**
- Rust async services run on Tokio 1.39.0 via `tokio` workspace dependency in `Cargo.toml`.
- `pd` is the full node daemon; it serves ABCI, gRPC/grpc-web, status frontend assets, and Prometheus metrics from `crates/bin/pd/src/main.rs`.
- `pclientd` is a persistent view/custody daemon; it serves tonic/grpc-web services and proxies selected node queries from `crates/bin/pclientd/src/lib.rs`.
- `pcli` is the CLI wallet/client; it embeds local custody and local or remote view clients from `crates/bin/pcli/src/opt.rs`.
- `pindexer` is the Postgres indexer; it runs `cometindex` app views from `crates/bin/pindexer/src/main.rs`.
- `orbis-audit` and `orbis-integration` are compliance/PRE workflow binaries in `crates/bin/orbis-audit` and `crates/bin/orbis-integration`.

**Package Manager:**
- Cargo - Rust workspace package manager, configured by `Cargo.toml` and locked by `Cargo.lock`.
- Go modules - gnark tooling in `tools/gnark/go.mod`, locked by `tools/gnark/go.sum`.
- npm - visualization helper dependencies in `crates/misc/tct-visualize/package.json`, locked by `crates/misc/tct-visualize/package-lock.json`.
- Nix flakes - dev shell and package builds in `flake.nix`, locked by `flake.lock`.
- Lockfile: present for Rust, Go, npm, and Nix.

## Frameworks

**Core:**
- Tokio 1.39.0 - async runtime for services, workers, tests, and CLI flows (`Cargo.toml`).
- Tonic 0.12.3, tonic-web 0.12.3, prost 0.13.4 - gRPC/grpc-web service and protobuf stack (`Cargo.toml`, `crates/core/app/src/rpc.rs`, `crates/proto/Cargo.toml`).
- Axum 0.7.9, axum-server 0.7.1, tower, tower-http - HTTP/grpc-web serving, CORS, tracing layers, and TLS acceptors (`crates/bin/pd/Cargo.toml`, `crates/bin/pd/src/main.rs`).
- tower-abci 0.18 and tower-actor 0.1.0 - ABCI server and actor-backed consensus/mempool services (`crates/core/app/Cargo.toml`, `crates/core/app/src/server.rs`).
- CometBFT/Tendermint 0.37.15 runtime with Rust tendermint crates 0.40.3 - consensus driver, RPC proxying, and IBC light-client types (`flake.nix`, `Cargo.toml`, `crates/util/tendermint-proxy/Cargo.toml`).
- Cnidarium 0.83, RocksDB 0.21.0, JMT 0.11 - durable chain state storage (`Cargo.toml`, `crates/bin/pd/src/main.rs`, `crates/core/app/Cargo.toml`).
- SQLx 0.8 with Postgres - event indexing and `pindexer` storage (`Cargo.toml`, `crates/util/cometindex/Cargo.toml`, `crates/bin/pindexer/Cargo.toml`).
- rusqlite 0.32 and r2d2_sqlite 0.25 - wallet/view and compliance scanner SQLite storage (`crates/view/Cargo.toml`, `crates/core/component/compliance/Cargo.toml`).
- Arkworks 0.5, decaf377, poseidon377, Groth16, SnarkPack/proof aggregation - cryptography and proof system libraries (`Cargo.toml`, `crates/crypto/**`).
- gnark 0.14.0 and gnark-crypto 0.19.0 - Go proving runtime for supported shielded proof families (`tools/gnark/go.mod`, `tools/gnark/README.md`).
- IBC crates (`ibc-types` 0.15.1, `ibc-proto` 0.51.1, `ics23` 0.12.0) - Cosmos/IBC protocol support (`Cargo.toml`, `proto/buf.yaml`).

**Testing:**
- cargo-nextest - default Rust test runner in `justfile` and CI workflows (`justfile`, `.github/workflows/rust.yml`).
- cargo test - targeted integration/proof tests and fallback runner in `justfile`.
- proptest 1.6, rstest 0.24, assert_cmd 2.0, predicates - Rust testing support (`Cargo.toml`, crate manifests under `crates/**/Cargo.toml`).
- Go `go test`, `go vet`, and `gofmt` - gnark module verification (`justfile`, `.github/workflows/rust.yml`, `tools/gnark/go.mod`).
- process-compose smoke/devnet suites - local integration environments in `deployments/compose/*.yml` and `deployments/scripts/smoke-test.sh`.

**Build/Dev:**
- Nix dev shells - `flake.nix` supplies Rust toolchain, CometBFT, Go, buf, cargo-nextest, cargo-hack, cargo-release, PostgreSQL, Prometheus, Grafana, mdBook, protobuf, RocksDB, SQLite, OpenSSL, clang, and system libraries.
- just - task runner for build, test, devnet, metrics, smoke, Orbis, docs, and container workflows (`justfile`).
- cargo-dist 0.28.0 - release artifacts configured by `dist-workspace.toml` and `.github/workflows/release.yml`.
- Docker/Compose - production/runtime images in `deployments/containerfiles/Dockerfile`; local Penumbra and Orbis stacks in `deployments/compose/*.yml` and `deployments/orbis/docker-compose.yml`.
- process-compose - local multi-process devnet, Postgres, metrics, and tooling configs in `deployments/compose/process-compose*.yml`.
- Buf - protobuf dependency management and Go code generation in `proto/buf.yaml`, `proto/buf.gen.yaml`, and `deployments/scripts/protobuf-codegen`.
- mdBook - protocol docs in `docs/protocol/book.toml`.
- Repo-local Codex/GSD automation - workflow skills under `.codex/skills/*/SKILL.md`; these are planning/development automation, not production runtime dependencies.

## Key Dependencies

**Critical:**
- `penumbra-sdk-app` 2.1.0 - component stack implementing the Penumbra protocol (`crates/core/app/Cargo.toml`).
- `pd` 2.1.0 - node daemon and service wiring (`crates/bin/pd/Cargo.toml`, `crates/bin/pd/src/main.rs`).
- `pcli` 2.1.0 - wallet CLI and local custody/view wiring (`crates/bin/pcli/Cargo.toml`, `crates/bin/pcli/src/opt.rs`).
- `pclientd` 2.1.0 - persistent view/custody daemon (`crates/bin/pclientd/Cargo.toml`, `crates/bin/pclientd/src/lib.rs`).
- `pindexer` 2.1.0 and `cometindex` 2.1.0 - CometBFT event indexing to Postgres (`crates/bin/pindexer/Cargo.toml`, `crates/util/cometindex/Cargo.toml`).
- `penumbra-sdk-proto` 2.1.0 - generated protobuf bindings and gRPC types (`crates/proto/Cargo.toml`).
- `cnidarium` 0.83 - state storage abstraction used by the app and RPC services (`Cargo.toml`, `crates/core/app/Cargo.toml`).
- `ark-*` 0.5, `decaf377`, `decaf377-rdsa`, `poseidon377` - proof, curve, signature, and hash primitives (`Cargo.toml`).
- `penumbra-sdk-proof-params` 2.1.0 and `tools/gnark/artifacts/**` - proving/verifying key registries and gnark artifact boundary (`crates/crypto/proof-params/Cargo.toml`, `tools/gnark/README.md`).
- `penumbra-orbis-client` 2.1.0 plus pinned `orbis-rs` git crates - Orbis PRE and SourceHub integration (`crates/util/orbis-client/Cargo.toml`).
- `ledger-lib` and `ledger-proto` pinned to `ledger-community/rust-ledger` rev `510bb3ca30639af4bdb12a918b6bbbdb75fa5f52` - optional Ledger USB custody support (`Cargo.toml`, `crates/custody-ledger-usb/Cargo.toml`).

**Infrastructure:**
- `metrics`, `metrics-exporter-prometheus`, `metrics-tracing-context`, `tracing`, `tracing-subscriber` - metrics and structured logs (`Cargo.toml`, `crates/bin/pd/src/main.rs`).
- `rustls`, `rustls-acme`, `axum-server` TLS support - HTTPS/ACME management (`crates/util/auto-https/Cargo.toml`, `crates/bin/pd/src/cli.rs`).
- `reqwest` 0.12.9 - HTTP clients for network join, proof-key download, registry loading, smoke tests, and app helpers (`Cargo.toml`, `crates/bin/pd/src/network/join.rs`, `crates/bin/pclientd/src/lib.rs`, `crates/crypto/proof-params/build.rs`).
- `serde`, `serde_json`, `toml`, `serde_with` - config and data serialization (`Cargo.toml`, `crates/bin/pcli/src/config.rs`, `crates/bin/pclientd/src/lib.rs`).
- `clap` 3.2 with env support - CLI and env-var binding (`Cargo.toml`, `crates/bin/pd/src/cli.rs`, `crates/bin/pcli/src/opt.rs`, `crates/util/cometindex/src/opt.rs`).
- `mimalloc` - default allocator for `pd` unless `benchmark-system-allocator` is enabled (`crates/bin/pd/src/main.rs`).

## Configuration

**Environment:**
- Rust toolchain is pinned in `rust-toolchain.toml` with `rustfmt`, `rust-analyzer`, and `wasm32-unknown-unknown`.
- Nix shell exports build-time paths and `RUST_LOG` defaults from `flake.nix`; native build inputs include clang, pkg-config, SQLite, OpenSSL, RocksDB, and platform frameworks on Darwin.
- Node/fullnode runtime flags are Clap env-bound in `crates/bin/pd/src/cli.rs`: use `PENUMBRA_PD_HOME`, `PENUMBRA_PD_ABCI_BIND`, `PENUMBRA_PD_GRPC_BIND`, `PENUMBRA_PD_METRICS_BIND`, `PENUMBRA_PD_COMETBFT_PROXY_URL`, `PENUMBRA_PD_TM_RPC_BIND`, `PENUMBRA_PD_TM_P2P_BIND`, `PENUMBRA_PD_JOIN_URL`, and `PENUMBRA_PD_ARCHIVE_URL`.
- Wallet/view runtime flags are configured by `PENUMBRA_PCLI_HOME`, `PENUMBRA_NODE_PD_URL`, and `PENUMBRA_PCLIENTD_HOME` in `crates/bin/pcli/src/opt.rs`, `crates/bin/pcli/src/command/init.rs`, and `crates/bin/pclientd/src/lib.rs`.
- Local devnet/compliance scripts centralize port and endpoint vars in `scripts/lib/common.sh`: `COMPLIANCE_TMP`, `PENUMBRA_DEVNET_HOME`, `PENUMBRA_PD_GRPC_PORT`, `PENUMBRA_COMETBFT_RPC_PORT`, `PENUMBRA_COMETBFT_P2P_PORT`, `PENUMBRA_POSTGRES_PORT`, `PENUMBRA_PCLIENTD_PORT_BASE`, `PENUMBRA_NODE_PD_URL`, and `PENUMBRA_NODE_CMT_URL`.
- gnark runtime selection uses `PENUMBRA_GNARK_*_ARTIFACT_DIR`, `PENUMBRA_GNARK_*_LIB`, and `PENUMBRA_GNARK_*_DAEMON` variables documented in `tools/gnark/README.md` and consumed by `crates/core/component/shielded-pool/src/gnark/transport.rs`.
- No `.env` file was detected; `.envrc.example` is present as environment-helper material only.

**Build:**
- Rust workspace and dependency versions: `Cargo.toml`, `Cargo.lock`.
- Rust compiler/channel: `rust-toolchain.toml`.
- Cargo linker flags for Linux: `.cargo/config.toml`.
- Lint policy: `clippy.toml`.
- Nix dev and build shell: `flake.nix`, `flake.lock`.
- Task runner: `justfile`.
- Release packaging: `dist-workspace.toml`, `.github/workflows/release.yml`.
- Container builds: `deployments/containerfiles/Dockerfile`.
- Protobuf generation: `proto/buf.yaml`, `proto/buf.gen.yaml`, `tools/proto-compiler/Cargo.toml`, `deployments/scripts/protobuf-codegen`.
- Local orchestration: `deployments/compose/process-compose.yml`, `deployments/compose/process-compose-postgres.yml`, `deployments/compose/process-compose-metrics.yml`, `deployments/orbis/docker-compose.yml`.

## Platform Requirements

**Development:**
- Use `nix develop` from `flake.nix` for the complete local toolchain.
- Required core tools outside Nix: Rust/Cargo, Go for `tools/gnark`, clang/pkg-config/OpenSSL/SQLite/RocksDB/protobuf compiler for native builds, and Docker for Orbis/container flows.
- Use `just check` for focused Rust check + formatting, `just test` for nextest, `just go-check` for gnark Go validation, and `just ci-preflight` for the broad local CI surface (`justfile`).
- Use `just dev`, `just smoke`, `just metrics`, and `just orbis-integration` for local full-stack environments (`justfile`, `deployments/scripts/run-local-devnet.sh`, `scripts/orbis-stack.sh`).

**Production:**
- Release binaries are `pd`, `pcli`, `pclientd`, `pindexer`, `orbis-audit`, and `orbis-integration` from the Cargo workspace (`Cargo.toml`, `deployments/containerfiles/Dockerfile`).
- OCI images publish to `ghcr.io/mizufinance/penumbra` via `.github/workflows/containers.yml`.
- Multi-platform binary archives/installers target macOS and Linux for `aarch64` and `x86_64` via `dist-workspace.toml`.
- Example systemd units for fullnode operation are in `deployments/systemd/penumbra.service` and `deployments/systemd/cometbft.service`.
- CometBFT is a required node-side process; the Nix build pins CometBFT 0.37.15 in `flake.nix`.

---

*Stack analysis: 2026-05-12*
