#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
export CARGO_BUILD_JOBS=2 RAYON_NUM_THREADS=2 GOMAXPROCS=2
export CARGO_TARGET_DIR="$root/target"
# Include the current working tree so local checks exercise the proposed changes.
python3 - "$root" "$work" <<'COPY'
import pathlib, shutil, subprocess, sys
root, work = map(pathlib.Path, sys.argv[1:])
names = subprocess.check_output(["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=root).decode().split("\0")
for relative in set(names):
    source = root / relative
    if not source.is_file():
        continue
    for location in ("source", "bankd/components/shieldd"):
        target = work / location / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)
for location in ("source", "bankd/components/shieldd"):
    target = work / location / "crates/bin/shieldd/examples/state_compatibility.rs"
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(root / "scripts/ci/fixtures/state_compatibility.rs", target)
COPY
for location in source bankd/components/shieldd; do
    source_dir="$work/$location"
    result_name="$(basename "$location")"
    (cd "$source_dir" && cargo run --locked --release -p shieldd --example state_compatibility -- "$work/db-$result_name" "$work/result-$result_name")
done
cmp "$work/result-source" "$work/result-shieldd"
echo 'Current-version reopen, checkpoint, history, replay and next-root parity passed at both source locations.'
