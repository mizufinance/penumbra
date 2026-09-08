#!/usr/bin/env bash
# Check each web-facing crate without native component features.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo_root}"

packages=(
    shieldd-sdk-asset
    shieldd-sdk-compact-block
    shieldd-sdk-fee
    shieldd-sdk-keys
    shieldd-sdk-sct
    shieldd-sdk-shielded-pool
    shieldd-sdk-tct
    shieldd-sdk-transaction
    shieldd-sdk-txhash
)

for p in "${packages[@]}" ; do
    echo "Checking package for wasm compat: $p ..."
    if ! cargo check --release --target wasm32-unknown-unknown --no-default-features --package "$p" ; then
        >&2 echo "ERROR: package appears not to be wasm-compatible: '$p'"
        exit 1
    fi
done
