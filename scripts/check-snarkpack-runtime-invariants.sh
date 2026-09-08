#!/usr/bin/env bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
python3 tools/gnark/check_gnark_family_registries.py
cargo test -p shieldd-sdk-proof-params consensus_verifying_keys_are_exact_bundled_keys --lib
cargo test -p shieldd-sdk-proof-params deployed_proof_key_registry_is_exhaustive_and_pairwise_distinct --lib
cargo test -p shieldd-sdk-proof-aggregation preflight_ --lib
cargo test -p shieldd-sdk-proof-aggregation production_registry_ --lib
