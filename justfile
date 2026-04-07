# Prints the list of recipes.
default:
    @just --list

# Creates and runs a local devnet with solo validator. Includes ancillary services
# like metrics, postgres for storing ABCI events, and pindexer for munging those events.
dev:
    ./deployments/scripts/check-nix-shell && \
        ./deployments/scripts/run-local-devnet.sh \
        --keep-project \
        --config ./deployments/compose/process-compose-postgres.yml \
        --config ./deployments/compose/process-compose-metrics.yml \
        --config ./deployments/compose/process-compose-dev-tooling.yml

# Formats the rust files in the project.
fmt:
    cargo fmt --all

# warms the rust cache by building all targets
build:
    cargo build --release --all-features --all-targets

# Runs 'cargo check' on all rust files in the project.
check:
  # check, failing on warnings
  RUSTFLAGS="-D warnings" cargo check --release --all-targets --all-features --target-dir=target/check
  # fmt dry-run, failing on any suggestions
  cargo fmt --all -- --check

# Go formatting check for the gnark runtime.
go-fmt-check:
  bash -lc 'cd tools/gnark && \
    files="$(gofmt -l .)"; \
    if test -z "$files"; then \
      exit 0; \
    fi; \
    echo "unformatted Go files:"; \
    printf "%s\n" "$files"; \
    if test -n "$CI"; then \
      echo "run: cd tools/gnark && gofmt -w $files"; \
      exit 1; \
    fi; \
    echo "auto-fixing with gofmt -w"; \
    gofmt -w $files; \
    remaining="$(gofmt -l .)"; \
    if test -n "$remaining"; then \
      echo "still unformatted after gofmt:"; \
      printf "%s\n" "$remaining"; \
      exit 1; \
    fi'

# Format the gnark Go module.
go-fmt:
  cd tools/gnark && gofmt -w .

# Compile the gnark Go module.
go-build:
  cd tools/gnark && go build ./...

# Run gnark Go tests.
go-test:
  cd tools/gnark && go test ./...

# Run gnark Go static checks.
go-vet:
  cd tools/gnark && go vet ./...

# Run the full gnark Go verification suite.
go-check: go-fmt-check go-build go-test go-vet

# Run the fast inner-loop gnark validation suite.
gnark-proof-tests-fast:
  just go-check
  cargo test -p penumbra-sdk-shielded-pool gnark:: --lib
  cargo test -p penumbra-sdk-shielded-pool public_input_hash:: --lib

# Run the slow end-to-end gnark proof-generation suite.
gnark-proof-tests-slow:
  cargo test --release -p pcli --test proof
  cargo test --release -p penumbra-sdk-shielded-pool --features bundled-proving-keys transfer_proof_roundtrip --lib
  cargo test --release -p penumbra-sdk-shielded-pool --lib
  cargo test --release -p penumbra-sdk-app-tests --test compliance_full_flow

# Run the default gnark validation suite.
gnark-proof-tests: gnark-proof-tests-fast

# CI wrapper for `check`.
ci-check:
  nix develop --command just check

# CI wrapper for `test`.
ci-test:
  cargo nextest run --cargo-profile ci

# CI wrapper for `go-check`.
ci-go-check:
  nix develop --command just go-check

# CI wrapper for `gnark-proof-tests`.
ci-gnark-proof-tests:
  nix develop --command just gnark-proof-tests-slow

# Run Orbis crypto integration tests (DKG, DLEQ, PRE against real nodes).
orbis-test:
    ./scripts/test-orbis-primitives.sh

# Run compliance transaction setup (DKG, registrations, transfers — run once).
compliance-setup-tx:
    ./scripts/setup-tx.sh

# Run scanning + progressive disclosure demo (rerunnable).
compliance-scanning:
    ./scripts/test-orbis-scanning.sh

# Render livereload environment for editing the Protocol documentation.
protocol-docs:
    # Access local docs at http://127.0.0.1:3002
    cd docs/protocol && \
        mdbook serve -n 127.0.0.1 --port 3002

# Generate code for Rust & Go from proto definitions.
proto:
    ./deployments/scripts/protobuf-codegen

# Run a local prometheus/grafana setup, to scrape a local node.
metrics:
    ./deployments/scripts/check-nix-shell && \
        process-compose --no-server --config ./deployments/compose/process-compose-metrics.yml up --keep-tui

# Rebuild Rust crate documentation
rustdocs:
    ./deployments/scripts/rust-docs

# Run rust unit tests, via cargo-nextest
test:
  cargo nextest run --release

# Run integration tests against the testnet, for validating HTTPS support
integration-testnet:
  cargo nextest run --release ${CARGO_FEATURE_ARGS:-} --features integration-testnet -E 'test(/_testnet$/)'

# Run integration tests for pmonitor tool
integration-pmonitor:
  ./deployments/scripts/warn-about-pd-state
  rm -rf /tmp/pmonitor-integration-test
  # Prebuild binaries, so they're available inside the tests without blocking.
  cargo build --release -p pcli --features bundled-proving-keys,download-proving-keys
  cargo build --release -p pd
  cargo -q run --release --bin pd -- --help > /dev/null
  cargo nextest run --release -p pmonitor --features bundled-proving-keys,download-proving-keys,network-integration --no-capture --no-fail-fast

# Run smoke test suite, via process-compose config.
smoke:
  ./deployments/scripts/check-nix-shell
  ./deployments/scripts/warn-about-pd-state
  ./deployments/scripts/smoke-test.sh

# Run integration tests for pclientd. Assumes specific dev env is already running.
integration-pclientd:
  cargo test --release --features bundled-proving-keys,download-proving-keys,sct-divergence-check --package pclientd -- \
    --ignored --test-threads 1 --nocapture

# Run integration tests for pcli. Assumes specific dev env is already running.
integration-pcli:
  cargo test --release --features bundled-proving-keys,download-proving-keys,sct-divergence-check --package pcli -- \
    --ignored --test-threads 1 --nocapture

# Run integration tests for pindexer. Assumes specific dev env is already running.
integration-pindexer:
  cargo nextest run --release -p pindexer --features network-integration

# Run integration tests for pd. Assumes specific dev env is already running.
integration-pd:
  cargo test --release --package pd -- --ignored --test-threads 1 --nocapture

# Build the container image locally
container:
  docker build -t ghcr.io/penumbra-zone/penumbra:local -f ./deployments/containerfiles/Dockerfile .

# Run the testnet locally entirely
testnet:
  just --justfile {{justfile()}} testnet-clean
  docker compose -f deployments/compose/docker-compose.yml up

# clean up the testnet, removing all volumes
testnet-clean:
  docker compose -f deployments/compose/docker-compose.yml down --volumes
  docker volume rm compose_penumbra-pd-node0 --force || true
