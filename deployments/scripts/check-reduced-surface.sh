#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "${repo_root}"

fail() {
  echo "ERROR: reduced-surface acceptance check failed: $1" >&2
  exit 1
}

check_no_match() {
  local description="$1"
  shift
  if rg -n "$@" > /tmp/penumbra-reduced-surface-check.txt; then
    cat /tmp/penumbra-reduced-surface-check.txt >&2
    fail "$description"
  fi
}

check_no_match \
  "legacy tx command surface is still present" \
  'SendMulti|Sweep|TxCmd::Send\b|TxCmd::Withdraw\b|InitTopSubCmd::Spend|InitType::SpendKey|min_validator_funding|Invalid swap commitment|swap claim outputs|note_reshape_2x2' \
  crates/bin/pcli crates/core/app crates/core/component/stake crates/core/component/shielded-pool crates/view proto/penumbra poc \
  -g '!crates/bin/pcli/tests/cli_surface.rs'

check_no_match \
  "legacy command examples are still present in tests/docs/scripts" \
  'tx send\b|tx sweep\b|send-multi\b' \
  crates/bin/pcli/tests deployments/scripts docs deployments/000-localnet \
  -g '!crates/bin/pcli/tests/cli_surface.rs' \
  -g '!deployments/scripts/check-reduced-surface.sh'

if [ -e deployments/config/grafana/dashboards/DEX.json ]; then
  fail "DEX dashboard residue is still present"
fi

echo "Reduced-surface acceptance checks passed."
