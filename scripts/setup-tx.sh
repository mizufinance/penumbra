#!/bin/bash
# Setup compliance transactions: DKG, registrations, and transfers.
# Run once after setup-penumbra.sh + setup-orbis.sh are ready.
#
# This writes to the chain. The scanning/PRE demo (test-orbis-scanning.sh)
# can then be run repeatedly against this state without adding noise.
#
# Prerequisites:
#   - setup-penumbra.sh running (pd + cometbft + wallets)
#   - setup-orbis.sh running (SourceHub + 3 Orbis nodes)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"
source "$SCRIPT_DIR/lib/transactions.sh"
load_env

ORBIS_CLI="${ORBIS_CLI:-cli-tool}"

# ═══════════════════════════════════════════════════════════════════════
print_banner "Compliance Setup: Chain Transactions" \
    "DKG + registrations + transfers (run once)"

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

if ! command -v "$ORBIS_CLI" &>/dev/null; then
    log_error "cli-tool not found in PATH."
    exit 1
fi
log_success "All prerequisites OK"

# ═══════════════════════════════════════════════════════════════════════
print_phase "Distributed Key Generation (DKG)"

echo "  Orbis runs a threshold MPC protocol across 3 nodes."
echo "  DKG establishes a shared ring_pk — no single node holds the full ring_sk."
echo ""

log_info "Getting peer IDs..."
PEER1=$($ORBIS_CLI info --endpoint http://127.0.0.1:50051 2>&1 | grep "Peer ID:" | awk '{print $NF}')
PEER2=$($ORBIS_CLI info --endpoint http://127.0.0.1:50052 2>&1 | grep "Peer ID:" | awk '{print $NF}')
PEER3=$($ORBIS_CLI info --endpoint http://127.0.0.1:50053 2>&1 | grep "Peer ID:" | awk '{print $NF}')
echo "  Node 1: ${PEER1:0:16}..."
echo "  Node 2: ${PEER2:0:16}..."
echo "  Node 3: ${PEER3:0:16}..."

RING_INFO="$COMPLIANCE_TMP/ring-info.env"

# Always run fresh DKG — no caching. This is a "run once" script and caching
# creates failure modes (stale rings, premature file deletion, DKG interference).
log_info "Running DKG (threshold 2-of-3)..."
$ORBIS_CLI dkg \
    --endpoint http://127.0.0.1:50051 \
    --threshold 2 \
    --peer-ids "$PEER1" "$PEER2" "$PEER3" 2>&1
sleep 10

RING_OUTPUT=$($ORBIS_CLI get-latest-ring 2>&1) || true
RING_PK=$(echo "$RING_OUTPUT" | grep "RING_PK=" | sed 's/RING_PK=//')
RING_ID=$(echo "$RING_OUTPUT" | grep "RING_ID=" | sed 's/RING_ID=//')

if [ -z "$RING_PK" ] || [ -z "$RING_ID" ]; then
    log_error "DKG failed. Restart infrastructure and try again."
    exit 1
fi

echo "RING_PK=$RING_PK" > "$RING_INFO"
echo "RING_ID=$RING_ID" >> "$RING_INFO"
echo "RING_PEERS=${PEER1},${PEER2},${PEER3}" >> "$RING_INFO"
RING_SK=""

echo "  Ring PK:    ${RING_PK:0:16}..."
echo "  Ring ID:    $RING_ID"
echo "  Threshold:  2-of-3 (any 2 nodes can participate in PRE)"
pass "DKG complete"

# ═══════════════════════════════════════════════════════════════════════
print_phase "Issuer Setup: Detection Key"

generate_issuer_dk
echo "  DK (private): ${REGULATED_DK:0:16}...  (used for chain scanning)"
echo "  DK_pub:       ${REGULATED_DK_PUB:0:16}...  (bound into asset registration)"
pass "Detection key generated"

# ═══════════════════════════════════════════════════════════════════════
print_phase "Asset Registration"

echo "  Asset:     regulated_usd"
echo "  Threshold: 500 display units (transfers >= 500 are auto-flagged)"
echo "  Ring PK:   bound into compliance commitment"
echo "  DK_pub:    bound into compliance commitment"
echo ""
register_regulated_asset
pass "regulated_usd registered"

# ═══════════════════════════════════════════════════════════════════════
print_phase "User Registration"

echo "  Registering Alice (address 0 and 1), Bob, Charlie..."
echo "  Each registration adds (address, asset) pair to the QuadTree."
echo ""
register_users
pass "Users registered: Alice[0,1], Bob, Charlie"

# ═══════════════════════════════════════════════════════════════════════
print_phase "Regulated Transfers"

echo "  Each transfer builds a ZK proof of compliance and encrypts"
echo "  transaction data in 4 tiers: detection, core, extension, sext."
echo ""
execute_regulated_transfers
pass "8 regulated transfers complete (1 flagged)"

# ═══════════════════════════════════════════════════════════════════════
print_phase "Edge Cases"

echo "  Testing that the compliance system enforces registration..."
echo ""
test_unregistered_rejection

echo ""
echo "  Testing unregulated asset transfer (uses BLACK_HOLE ACK)..."
echo "  Unregulated assets produce undecryptable ciphertext — no issuer can read them."
echo ""
test_unregulated_transfer

# ═══════════════════════════════════════════════════════════════════════
print_banner "Transaction Setup Complete"
echo "  All chain-writing operations done. Artifacts saved for scanning."
echo ""
echo "  All artifacts in: $COMPLIANCE_TMP/"
echo "    Keys: issuer-dk.env, ring-info.env"
echo ""
echo "  Next: run ./scripts/test-orbis-scanning.sh"
echo "  (can be re-run multiple times against the same chain state)"
echo ""

print_results

if [ "$FAILED" -gt 0 ]; then exit 1; fi
