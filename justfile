set export
export CARGO_BUILD_JOBS := "2"
export RAYON_NUM_THREADS := "2"
export GOMAXPROCS := "2"
export GOFLAGS := "-p=2"

# Prints the list of recipes.
default:
    @just --list

# Formats the rust files in the project.
fmt:
    cargo fmt --all

# warms the rust cache by building all targets
build:
    cargo build --release --all-features --all-targets

# Runs 'cargo check' on all rust files in the project.
check:
    just tooling-test
    just snarkpack-invariants
    # check, failing on warnings
    RUSTFLAGS="-D warnings" cargo check --release --all-targets --all-features --target-dir=target/check
    # fmt dry-run, failing on any suggestions
    cargo fmt --all -- --check

tooling-test:
    python3 -m unittest discover -s scripts/tests
    python3 -m unittest discover -s scripts/ci -p 'test_*.py'
    python3 -m unittest discover -s deployments/scripts/tests

# Go formatting check for the gnark runtime.
go-fmt-check:
    #!/usr/bin/env bash
    set -euo pipefail
    cd tools/gnark
    files="$(gofmt -l .)"
    if [[ -n "$files" ]]; then
        printf 'Run just go-fmt to format:\n%s\n' "$files" >&2
        exit 1
    fi

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
    cargo test -p shieldd-sdk-shielded-pool gnark:: --lib
    cargo test -p shieldd-sdk-shielded-pool public_input_hash:: --lib

# Exercise the note-seizure daemon, consensus verifier, and host state transition.
note-seizure-proof-tests:
    mkdir -p target/gnark-test
    cd tools/gnark && go build -o ../../target/gnark-test/proverdaemon ./cmd/proverdaemon
    SHIELDD_GNARK_NOTE_SEIZURE_DAEMON="$PWD/target/gnark-test/proverdaemon" SHIELDD_GNARK_NOTE_SEIZURE_ARTIFACT_DIR="$PWD/tools/gnark/artifacts/note_seizure" cargo test --release -p shieldd-sdk-shielded-pool gnark::note_seizure::tests::gnark_daemon_proof_note_seizure_roundtrip --lib -- --exact --ignored --test-threads=1
    SHIELDD_GNARK_NOTE_SEIZURE_DAEMON="$PWD/target/gnark-test/proverdaemon" SHIELDD_GNARK_NOTE_SEIZURE_ARTIFACT_DIR="$PWD/tools/gnark/artifacts/note_seizure" cargo test --release -p shieldd-sdk-app app::host::tests::note_seizure_verifies_capsule_release_and_commits_once --lib -- --exact --ignored --test-threads=1

# Run the slow end-to-end gnark proof-generation suite.
gnark-proof-tests-slow:
    python3 scripts/proof_artifacts.py materialize --bundle runtime
    just note-seizure-proof-tests
    bash scripts/gnark-proof-tests-slow.sh

# Run ignored slow SnarkPack parity tests.
snarkpack-slow:
    just snarkpack-slow-one oracle
    just snarkpack-slow-one interop

# Run one ignored SnarkPack runtime test.
snarkpack-slow-one test:
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{test}}" in
      oracle)
        package=shieldd-sdk-proof-aggregation
        filter=snarkpack_matches_single_and_batch_groth16_oracles_slow
        ;;
      interop)
        package=shieldd-sdk-proof-aggregation-reference
        filter=slow_two_way_interop_band
        ;;
      *)
        echo "unknown SnarkPack slow test: {{test}}" >&2
        exit 2
        ;;
    esac
    cargo test --release -p "$package" "$filter" --lib -- --ignored --test-threads=1

# Run the exact ordinary tests anchoring the bounded challenge sampler and its
# public prover/verifier exhaustion mappings.
snarkpack-challenge-boundaries:
    cargo test -p ark-ip-proofs bounded_challenge_sampler_ --lib
    cargo test -p ark-ip-proofs shipping_nonce_exhaustion_maps_exact_public_error --lib

# Run bounded SnarkPack fuzz harness smoke tests.
snarkpack-fuzz-smoke:
    bash -lc 'set -euo pipefail; unset ROCKSDB_LIB_DIR ROCKSDB_INCLUDE_DIR; toolchain="${SNARKPACK_FUZZ_TOOLCHAIN:-nightly-2025-09-30}"; export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH" RUSTUP_TOOLCHAIN="$toolchain"; runs="${SNARKPACK_FUZZ_RUNS:-16}"; fuzz_dir="crates/crypto/proof-aggregation-fuzz"; tmp="$(mktemp -d)"; trap "rm -rf \"$tmp\"" EXIT; cargo fuzz build --fuzz-dir "$fuzz_dir"; for target in deserialize_aggregate_proof; do mkdir -p "$tmp/$target"; cp "$fuzz_dir"/corpus/"$target"/* "$tmp/$target"/; cargo fuzz run --fuzz-dir "$fuzz_dir" "$target" "$tmp/$target" -- -runs="$runs"; done'

# Check durable SnarkPack runtime invariants.
snarkpack-invariants:
    bash scripts/check-snarkpack-runtime-invariants.sh

# Enforce SnarkPack valid-vs-adversarial DoS latency and size thresholds.
snarkpack-dos-gate:
    cargo test --release -p shieldd-sdk-proof-aggregation snarkpack_dos_gate_valid_and_adversarial_paths_hold_thresholds --lib -- --ignored --nocapture

# Run the default gnark validation suite.
gnark-proof-tests: gnark-proof-tests-fast

# CI wrapper for `check`.
ci-check:
    if command -v nix >/dev/null 2>&1; then \
      nix develop .#ci --command just check; \
    else \
      just check; \
    fi

# CI wrapper for `test`.
ci-test:
    python3 scripts/stage_artifacts.py provers --profile ci
    if command -v cargo-nextest >/dev/null 2>&1; then \
      SHIELDD_ARTIFACT_ROOT="$PWD/target/shieldd" cargo nextest run --cargo-profile ci --no-fail-fast -j 2; \
    else \
      echo "warning: cargo-nextest not found; falling back to 'cargo test --release --no-fail-fast'"; \
      SHIELDD_ARTIFACT_ROOT="$PWD/target/shieldd" cargo test --release --no-fail-fast -- --test-threads=2; \
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
      nix develop .#ci --command ./deployments/scripts/check-crate-feature-sets; \
    elif command -v cargo-hack >/dev/null 2>&1; then \
      ./deployments/scripts/check-crate-feature-sets; \
    else \
      echo "warning: nix and cargo-hack not found; falling back to 'cargo check --workspace --all-targets --all-features --release'"; \
      cargo check --workspace --all-targets --all-features --release; \
    fi
    if command -v nix >/dev/null 2>&1; then \
      nix develop .#ci --command ./deployments/scripts/check-wasm-compat.sh; \
    else \
      ./deployments/scripts/check-wasm-compat.sh; \
    fi
    just ci-test
    just ci-go-check
    just ci-gnark-proof-tests

# Validate local dependencies for the Orbis integration flow.
orbis-integration-preflight:
    ./scripts/orbis-integration-preflight.sh

# Validate binaries required by the Orbis integration flow.
orbis-integration-preflight-binaries:
    ./scripts/orbis-integration-preflight.sh --require-binaries

# Build the binaries required by the Orbis integration flow.
orbis-integration-build:
    cargo build --release -p orbis-audit -p orbis-integration

# Bring up the Orbis stack for use with Bankd.
orbis-integration-up:
    just orbis-integration-build
    just orbis-integration-preflight-binaries
    ./scripts/orbis-stack.sh up

# Create a ring and policy against an already running Orbis/Vera stack.
orbis-integration-setup-ring output_json:
    just orbis-integration-preflight
    test -x ./target/release/orbis-integration || (echo "Run cargo build --release -p orbis-integration first" >&2; exit 1)
    ./target/release/orbis-integration setup-ring --output-json {{output_json}}

# Tear down the Orbis integration stack.
orbis-integration-down:
    ./scripts/orbis-stack.sh down

# Print Docker logs for the Orbis stack.
orbis-integration-logs:
    ./scripts/orbis-stack.sh logs

# Generate code for Rust & Go from proto definitions.
proto:
    ./deployments/scripts/protobuf-codegen

proto-check:
    ./deployments/scripts/protobuf-codegen --check

features-check:
    ./deployments/scripts/check-crate-feature-sets

wasm-check:
    ./deployments/scripts/check-wasm-compat.sh

# Rebuild Rust crate documentation
rustdocs:
    ./deployments/scripts/rust-docs

# Run rust unit tests, via cargo-nextest
test:
    python3 scripts/stage_artifacts.py provers
    SHIELDD_ARTIFACT_ROOT="$PWD/target/shieldd" cargo nextest run --release -j 2

# Stage relocatable artifacts for embedded hosts and proof tools.
artifacts-native:
    python3 scripts/stage_artifacts.py native

artifacts-provers:
    python3 scripts/stage_artifacts.py provers

artifacts-audit:
    python3 scripts/stage_artifacts.py audit

# Opt-in circuit sizing diagnostics; no correctness assertions.
gnark-profile:
    cd tools/gnark && GOMAXPROCS=2 go test -p 2 -tags diagnostics ./internal/circuits -run '^TestConstraintProfiles$' -v

# Explicitly regenerate the frozen seizure witness fixture.
gnark-bless-seizure:
    cd tools/gnark && GOMAXPROCS=2 go test -p 2 -tags fixtures ./internal/abi -run '^TestBlessNoteSeizureWitness$' -v
