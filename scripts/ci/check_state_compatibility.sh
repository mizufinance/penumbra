#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
baseline=76a5a03dc83d9253bec3048a2d1e3e8ec8efe8e7
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/baseline" "$work/bankd/components/shieldd"
git -C "$root" archive "$baseline" | tar -x -C "$work/baseline"
git -C "$root" archive HEAD | tar -x -C "$work/bankd/components/shieldd"
export CARGO_BUILD_JOBS=2 RAYON_NUM_THREADS=2 GOMAXPROCS=2
export CARGO_TARGET_DIR="$root/target"
for source in "$work/baseline" "$work/bankd/components/shieldd"; do
    mkdir -p "$source/crates/bin/shieldd/examples"
    cp "$root/scripts/ci/fixtures/state_compatibility.rs" "$source/crates/bin/shieldd/examples/state_compatibility.rs"
done
run_fixture() {
    local source="$1"
    shift
    (cd "$source" && cargo run --locked --release -p shieldd --example state_compatibility -- "$@")
}
run_fixture "$work/baseline" create "$work/database" "$work/created"
cp -a "$work/database" "$work/control"
cp "$work/database.snapshot" "$work/control.snapshot"
cp "$work/database.history" "$work/control.history"
run_fixture "$work/baseline" continue "$work/control" "$work/control-result"
run_fixture "$work/bankd/components/shieldd" continue "$work/database" "$work/candidate-result"
cmp "$work/control-result" "$work/candidate-result"
echo 'Pre-change database queries, replay, spent markers, and next committed root match.'
