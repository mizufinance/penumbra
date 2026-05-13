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
    cargo test --release -p penumbra-sdk-shielded-pool --features bundled-proving-keys transfer_proof_roundtrip --lib
    cargo test --release -p penumbra-sdk-shielded-pool --lib

# Run the default gnark validation suite.
gnark-proof-tests: gnark-proof-tests-fast

# CI wrapper for `check`.
ci-check:
    if command -v nix >/dev/null 2>&1; then \
      nix develop --command just check; \
    else \
      just check; \
    fi

# CI wrapper for `test`.
ci-test:
    if command -v cargo-nextest >/dev/null 2>&1; then \
      cargo nextest run --cargo-profile ci; \
    else \
      echo "warning: cargo-nextest not found; falling back to 'cargo test --release --no-fail-fast'"; \
      cargo test --release --no-fail-fast; \
    fi

# CI wrapper for `go-check`.
ci-go-check:
    if command -v nix >/dev/null 2>&1; then \
      nix develop --command just go-check; \
    else \
      just go-check; \
    fi

# CI wrapper for `gnark-proof-tests`.
ci-gnark-proof-tests:
    if command -v nix >/dev/null 2>&1; then \
      nix develop --command just gnark-proof-tests-slow; \
    else \
      just gnark-proof-tests-slow; \
    fi

# Run the Rust and gnark CI surfaces locally, using the same commands as GitHub Actions.
ci-preflight:
    if command -v nix >/dev/null 2>&1; then \
      nix develop --command ./deployments/scripts/check-crate-feature-sets; \
    elif command -v cargo-hack >/dev/null 2>&1; then \
      ./deployments/scripts/check-crate-feature-sets; \
    else \
      echo "warning: nix and cargo-hack not found; falling back to 'cargo check --workspace --all-targets --all-features --release'"; \
      cargo check --workspace --all-targets --all-features --release; \
    fi
    if command -v nix >/dev/null 2>&1; then \
      nix develop --command ./deployments/scripts/check-wasm-compat.sh; \
    else \
      ./deployments/scripts/check-wasm-compat.sh; \
    fi
    just ci-test
    just ci-go-check
    just ci-gnark-proof-tests
    if command -v nix >/dev/null 2>&1; then \
      nix develop --command just smoke; \
    else \
      just smoke; \
    fi

# Bring up Penumbra infra for the Orbis compliance flow.
penumbra-up:
    ./scripts/penumbra-up.sh

# Validate local dependencies for the Orbis integration flow.
orbis-integration-preflight:
    ./scripts/orbis-integration-preflight.sh

# Validate binaries required by the Orbis integration flow.
orbis-integration-preflight-binaries:
    ./scripts/orbis-integration-preflight.sh --require-binaries

# Validate binaries and local ports before bringing up the stack.
orbis-integration-preflight-bringup:
    ./scripts/orbis-integration-preflight.sh --require-binaries --check-ports-free

# Build the binaries required by the Orbis integration flow.
orbis-integration-build:
    cargo build --release -p pcli -p pclientd --features bundled-proving-keys
    cargo build --release -p pd -p orbis-audit -p orbis-integration

# Run the full Orbis integration flow assuming release binaries already exist.
orbis-integration-run:
    just orbis-integration-preflight-bringup
    ./target/release/orbis-integration run

# Keep the stack running on failure for local debugging.
orbis-integration-debug:
    just orbis-integration-build
    just orbis-integration-preflight-bringup
    ./target/release/orbis-integration run --keep-on-fail

# Build and run the full one-shot Penumbra + Orbis integration flow.
orbis-integration:
    just orbis-integration-build
    just orbis-integration-run

# Bring up Penumbra and Orbis for phased local debugging.
orbis-integration-up:
    just orbis-integration-build
    just orbis-integration-preflight-bringup
    ./scripts/penumbra-up.sh
    ./scripts/orbis-stack.sh up

# Run the seed phase against an already running Penumbra + Orbis stack.
orbis-integration-seed:
    just orbis-integration-preflight-binaries
    ./target/release/orbis-integration seed

# Run the read-only verify phase against an existing seeded stack.
orbis-integration-verify:
    just orbis-integration-preflight-binaries
    ./target/release/orbis-integration verify

# Tear down the Orbis integration stack.
orbis-integration-down:
    ./scripts/orbis-stack.sh down
    ./scripts/penumbra-down.sh

# Print Docker logs for the Orbis stack.
orbis-integration-logs:
    ./scripts/orbis-stack.sh logs

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

# Run smoke test suite, via process-compose config.
smoke:
    ./deployments/scripts/check-nix-shell
    ./deployments/scripts/smoke-test.sh

reduced-surface-check:
    bash ./deployments/scripts/check-reduced-surface.sh

# Run integration tests for pclientd. Assumes specific dev env is already running.
integration-pclientd:
    cargo test --release --features bundled-proving-keys,download-proving-keys,sct-divergence-check --package pclientd --test network_integration -- \
      --ignored --test-threads 1 --nocapture

# Run integration tests for pcli. Assumes specific dev env is already running.
integration-pcli:
    cargo test --release --features bundled-proving-keys,download-proving-keys,sct-divergence-check --package pcli --test network_integration -- \
      --ignored --test-threads 1 --nocapture
    cargo test --release --features bundled-proving-keys,download-proving-keys,sct-divergence-check --package pcli --test compliance_network -- \
      --ignored --test-threads 1 --nocapture

# Run integration tests for pindexer. Assumes specific dev env is already running.
integration-pindexer:
    if cargo nextest --version >/dev/null 2>&1; then \
      cargo nextest run --release -p pindexer --features network-integration --run-ignored ignored-only --test-threads 1; \
    else \
      cargo test --release -p pindexer --features network-integration -- --ignored --test-threads 1 --nocapture; \
    fi

# Run integration tests for pd. Assumes specific dev env is already running.
integration-pd:
    cargo test --release --package pd --test network_integration -- --ignored --test-threads 1 --nocapture

# Build the container image locally
container:
    docker build -t ghcr.io/mizufinance/penumbra:local -f ./deployments/containerfiles/Dockerfile .

# Run the testnet locally entirely
testnet:
    just --justfile {{ justfile() }} testnet-clean
    docker compose -f deployments/compose/docker-compose.yml up

# clean up the testnet, removing all volumes
testnet-clean:
    docker compose -f deployments/compose/docker-compose.yml down --volumes
    docker volume rm compose_penumbra-pd-node0 --force || true
