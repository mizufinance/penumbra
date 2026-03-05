# Benchmark Runbook: Real `base` vs `dev` vs `local`

This runbook runs each version from its real codebase (no synthetic baseline) and merges results into your current branch CSVs.

## Safety rules

- Keep your current branch committed before starting.
- Never push temporary worktree branches.
- Use local-only branch names:
  - `bench-dev-local-only`
  - `bench-base-local-only`

## 1) Define paths

```bash
# Update this path to your local repository checkout.
cd /path/to/your/penumbra

ROOT="$(pwd)"
DEV_WT="/private/tmp/penumbra-dev-bench"
BASE_WT="/private/tmp/penumbra-base-bench"
RESULTS_REL="crates/bench/benches/compliance"
```

## 2) Create worktrees (once)

```bash
git worktree add "$DEV_WT" origin/dev
git worktree add "$BASE_WT" release/v2.1.x

git -C "$DEV_WT" checkout -b bench-dev-local-only
git -C "$BASE_WT" checkout -b bench-base-local-only

# Verify they have no upstream:
git -C "$DEV_WT" rev-parse --abbrev-ref --symbolic-full-name @{u} 2>/dev/null || echo no-upstream
git -C "$BASE_WT" rev-parse --abbrev-ref --symbolic-full-name @{u} 2>/dev/null || echo no-upstream
```

## 3) Sync benchmark architecture into worktrees

Run this whenever benchmark architecture changes on your local branch.

```bash
[[ -n "${ROOT:-}" ]] || { echo "ROOT is empty"; exit 1; }
[[ -n "${DEV_WT:-}" && -n "${BASE_WT:-}" ]] || { echo "DEV_WT/BASE_WT must be set"; exit 1; }
[[ "$DEV_WT" != "/" && "$BASE_WT" != "/" ]] || { echo "worktree path must not be /"; exit 1; }
[[ "$DEV_WT" != "$ROOT" && "$BASE_WT" != "$ROOT" ]] || { echo "worktree path must not equal ROOT"; exit 1; }
mkdir -p "$DEV_WT/crates/bench" "$BASE_WT/crates/bench"

rsync -a --delete "$ROOT/crates/bench/" "$DEV_WT/crates/bench/"
rsync -a --delete "$ROOT/crates/bench/" "$BASE_WT/crates/bench/"
```

## 4) Validate compilation and patch compatibility in worktrees only

`dev` usually compiles directly. `base` may need small compatibility edits in `crates/bench` due to API drift.

```bash
cd "$DEV_WT"
BENCH_VERSION=dev cargo bench -p penumbra-sdk-bench --bench client_flow --no-run
BENCH_VERSION=dev cargo bench -p penumbra-sdk-bench --bench scanner_flow --no-run
BENCH_VERSION=dev cargo bench -p penumbra-sdk-bench --bench validator_flow --no-run
BENCH_VERSION=dev cargo bench -p penumbra-sdk-bench --bench node_abci_flow --no-run

cd "$BASE_WT"
BENCH_VERSION=base cargo bench -p penumbra-sdk-bench --bench client_flow --no-run
BENCH_VERSION=base cargo bench -p penumbra-sdk-bench --bench scanner_flow --no-run
BENCH_VERSION=base cargo bench -p penumbra-sdk-bench --bench validator_flow --no-run
BENCH_VERSION=base cargo bench -p penumbra-sdk-bench --bench node_abci_flow --no-run
```

If `base` fails to compile, patch only files under `$BASE_WT/crates/bench` until these 4 commands pass.

## 5) Run each version (single-version mode)

```bash
# local
cd "$ROOT"
BENCH_VERSION=local BENCH_SUITE=regression BENCH_WARMUP=2 BENCH_SAMPLES=10 cargo bench -p penumbra-sdk-bench --bench client_flow
BENCH_VERSION=local BENCH_SUITE=regression BENCH_WARMUP=2 BENCH_SAMPLES=10 cargo bench -p penumbra-sdk-bench --bench scanner_flow
BENCH_VERSION=local BENCH_SUITE=regression BENCH_WARMUP=2 BENCH_SAMPLES=10 cargo bench -p penumbra-sdk-bench --bench validator_flow
BENCH_VERSION=local BENCH_SUITE=regression BENCH_WARMUP=2 BENCH_SAMPLES=10 cargo bench -p penumbra-sdk-bench --bench node_abci_flow

# dev
cd "$DEV_WT"
BENCH_VERSION=dev BENCH_SUITE=regression BENCH_WARMUP=2 BENCH_SAMPLES=10 cargo bench -p penumbra-sdk-bench --bench client_flow
BENCH_VERSION=dev BENCH_SUITE=regression BENCH_WARMUP=2 BENCH_SAMPLES=10 cargo bench -p penumbra-sdk-bench --bench scanner_flow
BENCH_VERSION=dev BENCH_SUITE=regression BENCH_WARMUP=2 BENCH_SAMPLES=10 cargo bench -p penumbra-sdk-bench --bench validator_flow
BENCH_VERSION=dev BENCH_SUITE=regression BENCH_WARMUP=2 BENCH_SAMPLES=10 cargo bench -p penumbra-sdk-bench --bench node_abci_flow

# base
cd "$BASE_WT"
BENCH_VERSION=base BENCH_SUITE=regression BENCH_WARMUP=2 BENCH_SAMPLES=10 cargo bench -p penumbra-sdk-bench --bench client_flow
BENCH_VERSION=base BENCH_SUITE=regression BENCH_WARMUP=2 BENCH_SAMPLES=10 cargo bench -p penumbra-sdk-bench --bench scanner_flow
BENCH_VERSION=base BENCH_SUITE=regression BENCH_WARMUP=2 BENCH_SAMPLES=10 cargo bench -p penumbra-sdk-bench --bench validator_flow
BENCH_VERSION=base BENCH_SUITE=regression BENCH_WARMUP=2 BENCH_SAMPLES=10 cargo bench -p penumbra-sdk-bench --bench node_abci_flow
```

## 6) Merge `base/dev/local` CSV rows into your current branch

```bash
cd "$ROOT"

CUR_ROOT="$ROOT/$RESULTS_REL"
DEV_ROOT="$DEV_WT/$RESULTS_REL"
BASE_ROOT="$BASE_WT/$RESULTS_REL"

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

while IFS= read -r rel; do
  cur="$CUR_ROOT/$rel"
  dev="$DEV_ROOT/$rel"
  base="$BASE_ROOT/$rel"

  existing=()
  [[ -f "$cur" ]] && existing+=("$cur")
  [[ -f "$dev" ]] && existing+=("$dev")
  [[ -f "$base" ]] && existing+=("$base")
  if [[ "${#existing[@]}" -eq 0 ]]; then
    echo "Skipping missing csv in all roots: $rel"
    continue
  fi

  if [[ -f "$cur" ]]; then
    header="$(head -n 1 "$cur")"
  else
    header="$(head -n 1 "${existing[0]}")"
  fi
  body="$tmpdir/body.csv"

  {
    for f in "${existing[@]}"; do
      tail -n +2 "$f"
    done
  } | awk 'NF' \
    | awk '!seen[$0]++' \
    | awk -F',' 'BEGIN{OFS=","} {
        v=$1;
        w=(v=="base"?0:(v=="dev"?1:(v=="local"?2:9)));
        print w,$0
      }' \
    | sort -t',' -k1,1n -k2,2 -k3,3 -k4,4 -k5,5 -k6,6 -k7,7 -k8,8 -k9,9 \
    | cut -d',' -f2- > "$body"

  {
    printf '%s\n' "$header"
    cat "$body"
  } > "$cur"
done < <(cd "$CUR_ROOT" && rg --files -g '*.csv' | sort)

# Include CSVs present only in dev/base worktrees.
while IFS= read -r rel; do
  cur="$CUR_ROOT/$rel"
  dev="$DEV_ROOT/$rel"
  base="$BASE_ROOT/$rel"

  [[ -f "$cur" ]] && continue
  existing=()
  [[ -f "$dev" ]] && existing+=("$dev")
  [[ -f "$base" ]] && existing+=("$base")
  [[ "${#existing[@]}" -gt 0 ]] || continue

  header="$(head -n 1 "${existing[0]}")"
  body="$tmpdir/body.csv"
  {
    for f in "${existing[@]}"; do
      tail -n +2 "$f"
    done
  } | awk 'NF' \
    | awk '!seen[$0]++' \
    | awk -F',' 'BEGIN{OFS=","} {
        v=$1;
        w=(v=="base"?0:(v=="dev"?1:(v=="local"?2:9)));
        print w,$0
      }' \
    | sort -t',' -k1,1n -k2,2 -k3,3 -k4,4 -k5,5 -k6,6 -k7,7 -k8,8 -k9,9 \
    | cut -d',' -f2- > "$body"

  mkdir -p "$(dirname "$cur")"
  {
    printf '%s\n' "$header"
    cat "$body"
  } > "$cur"
done < <(
  {
    (cd "$DEV_ROOT" && rg --files -g '*.csv')
    (cd "$BASE_ROOT" && rg --files -g '*.csv')
  } | sort -u
)
```

## 7) Optional cleanup

```bash
git worktree remove "$DEV_WT"
git worktree remove "$BASE_WT"
```
