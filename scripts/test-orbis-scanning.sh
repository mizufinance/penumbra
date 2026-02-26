#!/bin/bash
# Orbis Scanning & Progressive Disclosure Demo
#
# Read-only demo showing selective progressive disclosure:
#   STATE 1: Detection-only (flagged transactions decrypted via issuer DK)
#   STATE 2: Core PRE for Alice & Bob (amounts + self-addresses via Orbis)
#   STATE 3: Extension PRE for Alice & Bob (counterparty addresses via Orbis)
#   Charlie is never audited — his non-flagged transactions stay encrypted.
#
# This script is fully rerunnable — it reads the chain and re-does the
# analysis from scratch each time. No chain writes, no new key generation.
#
# Prerequisites:
#   - setup-penumbra.sh running (pd + cometbft + wallets)
#   - setup-orbis.sh running (SourceHub + 3 Orbis nodes)
#   - setup-tx.sh completed (chain has data + keys in tmp/)
#   - cargo build --release -p orbis-audit
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"
source "$SCRIPT_DIR/lib/transactions.sh"
load_env

REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
ORBIS_AUDIT="${ORBIS_AUDIT:-$REPO_ROOT/target/release/orbis-audit}"

ISSUER_DB="$COMPLIANCE_TMP/issuer-ledger.db"
DETECTED="$COMPLIANCE_TMP/detected_txs.json"
ORBIS_ENDPOINT="http://127.0.0.1:50051"

# ═══════════════════════════════════════════════════════════════════════
print_banner "Compliance Demo: Progressive Disclosure" \
    "3-tier decryption: Detection -> Core PRE -> Extension PRE"

# ═══════════════════════════════════════════════════════════════════════
print_phase "Prerequisites"

log_info "Checking Penumbra..."
if ! nc -z -w1 127.0.0.1 8080 2>/dev/null; then
    log_error "Penumbra pd not reachable on port 8080."
    log_error "Run: ./scripts/setup-penumbra.sh"
    exit 1
fi
log_success "pd ready (port 8080)"

log_info "Checking Orbis nodes..."
for port in 50051 50052 50053; do
    if ! nc -z -w1 127.0.0.1 "$port" 2>/dev/null; then
        log_error "Orbis node on port $port not reachable."
        log_error "Run: ./scripts/setup-orbis.sh"
        exit 1
    fi
done
log_success "3 Orbis nodes ready (ports 50051-50053)"

if [ ! -f "$ORBIS_AUDIT" ]; then
    log_info "Building orbis-audit..."
    cargo build --release -p orbis-audit --manifest-path "$REPO_ROOT/Cargo.toml"
fi
log_success "orbis-audit ready"

# Load issuer DK from setup-tx.sh
ISSUER_DK_FILE="$COMPLIANCE_TMP/issuer-dk.env"
if [ ! -f "$ISSUER_DK_FILE" ]; then
    log_error "Issuer DK not found: $ISSUER_DK_FILE"
    log_error "Run: ./scripts/setup-tx.sh first"
    exit 1
fi
source "$ISSUER_DK_FILE"
log_success "Issuer DK loaded (${REGULATED_DK:0:16}...)"

# Load ring info from setup-tx.sh
RING_INFO="$COMPLIANCE_TMP/ring-info.env"
if [ ! -f "$RING_INFO" ]; then
    log_error "Ring info not found: $RING_INFO"
    log_error "Run: ./scripts/setup-tx.sh first"
    exit 1
fi
source "$RING_INFO"

# Validate ring is not stale (nodes may have restarted with new peer IDs)
ORBIS_CLI="${ORBIS_CLI:-cli-tool}"
if command -v "$ORBIS_CLI" &>/dev/null && [ -n "${RING_PEERS:-}" ]; then
    CURRENT_PEER1=$($ORBIS_CLI info --endpoint http://127.0.0.1:50051 2>&1 | grep "Peer ID:" | awk '{print $NF}') || true
    CURRENT_PEER2=$($ORBIS_CLI info --endpoint http://127.0.0.1:50052 2>&1 | grep "Peer ID:" | awk '{print $NF}') || true
    CURRENT_PEER3=$($ORBIS_CLI info --endpoint http://127.0.0.1:50053 2>&1 | grep "Peer ID:" | awk '{print $NF}') || true
    CURRENT_PEERS="${CURRENT_PEER1},${CURRENT_PEER2},${CURRENT_PEER3}"
    if [ "$RING_PEERS" != "$CURRENT_PEERS" ]; then
        log_error "Ring is stale: Orbis nodes have new peer IDs since last setup-tx.sh"
        log_error "Re-run: ./scripts/setup-tx.sh"
        exit 1
    fi
fi
log_success "Ring loaded (PK: ${RING_PK:0:16}..., ID: $RING_ID)"

# ═══════════════════════════════════════════════════════════════════════
print_phase "Chain Scan (Detection Layer)"

echo "  The issuer scans the chain using their detection key (DK)."
echo "  Flagged transfers (amount >= threshold) are auto-decrypted."
echo "  Non-flagged transfers are detected but remain encrypted."
echo ""
run_scan "$DETECTED"

TOTAL=$(python3 -c "import json; d=json.load(open('$DETECTED')); print(len(d['detected']))")
FLAGGED=$(python3 -c "import json; d=json.load(open('$DETECTED')); print(sum(1 for t in d['detected'] if t['is_flagged']))")
SPENDS=$(python3 -c "import json; d=json.load(open('$DETECTED')); print(sum(1 for t in d['detected'] if t.get('is_spend', False)))")
OUTPUTS=$((TOTAL - SPENDS))
echo "  Detected: $TOTAL actions ($OUTPUTS outputs, $SPENDS spends)"
echo "  Flagged:  $FLAGGED (auto-decrypted)"
echo "  Encrypted: $((TOTAL - FLAGGED)) (require Orbis PRE)"
pass "Chain scan complete"

# ═══════════════════════════════════════════════════════════════════════
print_phase "Issuer Database"

echo "  Importing scan results into issuer ledger..."
init_issuer_db "$ISSUER_DB" "$DETECTED"
pass "Database initialized with aliases (Alice, Bob, Charlie)"

# ═══════════════════════════════════════════════════════════════════════
#  STATE 1
# ═══════════════════════════════════════════════════════════════════════
print_state_banner "1" "Detection-Only Decryption"
echo "  Flagged transfers (amount >= 500) are auto-decrypted using the issuer's"
echo "  detection key. Non-flagged transfers are visible but encrypted (---)."
echo ""
$PCLI tx compliance issuer-db show --db "$ISSUER_DB"

# ═══════════════════════════════════════════════════════════════════════
print_phase "Orbis PRE: Core Tier"

echo "  Proxy Re-Encryption (PRE): Orbis nodes collaboratively re-encrypt"
echo "  ciphertexts so the issuer can decrypt — without the nodes ever learning"
echo "  the plaintext. The core tier reveals: amount + self-address."
echo ""
echo "  Only Alice and Bob are audited. Charlie's transactions remain encrypted"
echo "  — demonstrating that PRE is selective, not blanket surveillance."
echo ""

for user_info in "Alice:$ALICE_ADDRESS" "Bob:$BOB_ADDRESS"; do
    USER_NAME="${user_info%%:*}"
    USER_ADDR="${user_info#*:}"

    log_info "Requesting PRE for $USER_NAME (core tier)..."
    AUDIT_FILE="$COMPLIANCE_TMP/$(echo "$USER_NAME" | tr '[:upper:]' '[:lower:]')-audit.json"

    $ORBIS_AUDIT \
        --input "$DETECTED" \
        --dk-hex "$REGULATED_DK" \
        --node "$PENUMBRA_NODE_PD_URL" \
        --output "$AUDIT_FILE" \
        --tier default \
        --sender-address "$USER_ADDR" \
        --orbis-endpoint "$ORBIS_ENDPOINT" \
        --ring-pk-hex "$RING_PK" \
        --ring-id "$RING_ID"

    COUNT=$(python3 -c "import json; print(len(json.load(open('$AUDIT_FILE'))))" 2>/dev/null || echo "0")
    echo "  $USER_NAME: $COUNT entries decrypted via core PRE"

    if [ "$COUNT" -gt 0 ]; then
        run_quiet $PCLI tx compliance issuer-db update \
            --db "$ISSUER_DB" \
            --audit-output "$AUDIT_FILE" \
            --audit-subject "$USER_NAME core"
    fi
done
log_info "Charlie: skipped (not audited — transactions remain encrypted)"

pass "Core tier PRE complete"

# ═══════════════════════════════════════════════════════════════════════
#  STATE 2
# ═══════════════════════════════════════════════════════════════════════
print_state_banner "2" "Core PRE — Amounts + Self-Addresses"
echo "  Alice and Bob: non-flagged transfers now reveal amount + owner."
echo "  Charlie: remains encrypted (---) — issuer did not request PRE."
echo "  Counterparty addresses remain hidden until extension tier."
echo ""
$PCLI tx compliance issuer-db show --db "$ISSUER_DB"

# ═══════════════════════════════════════════════════════════════════════
print_phase "Orbis PRE: Extension Tier"

echo "  The extension tier reveals counterparty addresses for audited users,"
echo "  completing their transaction graph: sender -> amount -> receiver."
echo "  Charlie's transactions remain encrypted throughout."
echo ""

for user_info in "Alice:$ALICE_ADDRESS" "Bob:$BOB_ADDRESS"; do
    USER_NAME="${user_info%%:*}"
    USER_ADDR="${user_info#*:}"

    log_info "Requesting PRE for $USER_NAME (extension tier)..."
    AUDIT_FILE="$COMPLIANCE_TMP/$(echo "$USER_NAME" | tr '[:upper:]' '[:lower:]')-ext-audit.json"

    $ORBIS_AUDIT \
        --input "$DETECTED" \
        --dk-hex "$REGULATED_DK" \
        --node "$PENUMBRA_NODE_PD_URL" \
        --output "$AUDIT_FILE" \
        --tier extension \
        --sender-address "$USER_ADDR" \
        --orbis-endpoint "$ORBIS_ENDPOINT" \
        --ring-pk-hex "$RING_PK" \
        --ring-id "$RING_ID"

    COUNT=$(python3 -c "import json; print(len(json.load(open('$AUDIT_FILE'))))" 2>/dev/null || echo "0")
    echo "  $USER_NAME: $COUNT entries decrypted via extension PRE"

    if [ "$COUNT" -gt 0 ]; then
        run_quiet $PCLI tx compliance issuer-db update \
            --db "$ISSUER_DB" \
            --audit-output "$AUDIT_FILE" \
            --audit-subject "$USER_NAME ext"
    fi
done
log_info "Charlie: skipped (not audited)"

pass "Extension tier PRE complete"

# ═══════════════════════════════════════════════════════════════════════
#  STATE 3
# ═══════════════════════════════════════════════════════════════════════
print_state_banner "3" "Extension PRE — Selective Disclosure"
echo "  Alice & Bob: fully decrypted (sender -> amount -> receiver)."
echo "  Charlie: still encrypted (---). The issuer never requested PRE for Charlie,"
echo "  so Orbis never re-encrypted Charlie's ciphertexts. Privacy preserved."
echo ""
$PCLI tx compliance issuer-db show --db "$ISSUER_DB"

# ═══════════════════════════════════════════════════════════════════════
#  SUMMARY
# ═══════════════════════════════════════════════════════════════════════
print_banner "Demo Complete"
echo "  Transactions:  $TOTAL actions ($OUTPUTS outputs, $SPENDS spends)"
echo "  Flagged:       $FLAGGED  (STATE 1: auto-decrypted via issuer DK)"
echo "  Audited:       Alice, Bob (STATE 2+3: decrypted via Orbis PRE)"
echo "  Not audited:   Charlie (transactions remain encrypted)"
echo ""
echo "  Disclosure tiers:"
echo "    STATE 1  Detection     Flagged transactions only (issuer DK)"
echo "    STATE 2  Core PRE      Amounts + self-addresses (Alice & Bob only)"
echo "    STATE 3  Extension PRE Counterparty addresses (Alice & Bob only)"
echo ""
echo "  Charlie's non-flagged transactions stayed encrypted throughout —"
echo "  PRE is selective, not blanket surveillance."
echo ""
echo "  Orbis ring: $RING_ID (threshold 2-of-3)"
echo "  No single node ever held sk_ring — all PRE via threshold MPC."
echo ""
echo "  All run artifacts in: $COMPLIANCE_TMP/"
echo "    Scan:    detected_txs.json"
echo "    DB:      issuer-ledger.db"
echo "    Audit:   {alice,bob}-audit.json, {alice,bob}-ext-audit.json"
echo "    Keys:    issuer-dk.env, ring-info.env"
echo ""

print_results

if [ "$FAILED" -gt 0 ]; then exit 1; fi
