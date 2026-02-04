#!/bin/bash
# Compliance Setup - creates wallets, registers assets and users
set -e

repo_root="$(git rev-parse --show-toplevel)"
PCLI="${PCLI:-$repo_root/target/release/pcli}"
PD="${PD:-$repo_root/target/release/pd}"
PENUMBRA_NODE_PD_URL="${PENUMBRA_NODE_PD_URL:-http://localhost:8080}"
export PENUMBRA_NODE_PD_URL

ALICE_HOME=/tmp/alice-wallet
BOB_HOME=/tmp/bob-wallet
OSCAR_HOME=/tmp/oscar-wallet
UNREGISTERED_HOME=/tmp/unregistered-wallet

echo "=== Compliance Setup ==="

# Cleanup
rm -rf ~/.penumbra/network_data "$ALICE_HOME" "$BOB_HOME" "$OSCAR_HOME" "$UNREGISTERED_HOME"

[ ! -f "$PCLI" ] && echo "ERROR: pcli not found" && exit 1
[ ! -f "$PD" ] && echo "ERROR: pd not found" && exit 1

# Init Alice (need address for genesis)
$PCLI --home "$ALICE_HOME" init soft-kms generate
ALICE_ADDRESS=$($PCLI --home "$ALICE_HOME" view address 0)
echo "Alice: $ALICE_ADDRESS"

echo "Stop pd and cometbft if running, then press Enter to generate new network..."
read -r

$PD network generate \
    --chain-id penumbra-local-devnet \
    --unbonding-delay 302400 \
    --epoch-duration 302400 \
    --proposal-voting-blocks 50 \
    --gas-price-simple 0 \
    --timeout-commit 500ms \
    --validators-input-file "$repo_root/testnets/validators-single.json" \
    --allocation-address "$ALICE_ADDRESS"

echo ""
echo "Network generated. Now start pd and cometbft in separate terminals:"
echo "  Terminal 1: $PD start --home ~/.penumbra/network_data/node0/pd"
echo "  Terminal 2: cometbft start --home ~/.penumbra/network_data/node0/cometbft"
echo ""
echo "Press Enter when both are running..."
read -r

# Init other wallets
$PCLI --home "$BOB_HOME" init soft-kms generate
$PCLI --home "$OSCAR_HOME" init soft-kms generate
$PCLI --home "$UNREGISTERED_HOME" init soft-kms generate

# Sync
$PCLI --home "$ALICE_HOME" view sync
$PCLI --home "$BOB_HOME" view sync
$PCLI --home "$OSCAR_HOME" view sync
$PCLI --home "$UNREGISTERED_HOME" view sync

BOB_ADDRESS=$($PCLI --home "$BOB_HOME" view address 0)
OSCAR_ADDRESS=$($PCLI --home "$OSCAR_HOME" view address 0)
UNREGISTERED_ADDRESS=$($PCLI --home "$UNREGISTERED_HOME" view address 0)

echo "Bob: $BOB_ADDRESS"
echo "Oscar: $OSCAR_ADDRESS"
echo "Unregistered: $UNREGISTERED_ADDRESS"

# Register assets
echo "=== Registering Assets ==="
# penumbra and test_usd are auto-registered as UNREGULATED at genesis

# Generate issuer detection key for regulated_usd
echo "Generating issuer detection key..."
REGULATED_DK_OUTPUT=$($PCLI tx compliance generate-dk 2>&1)
REGULATED_DK=$(echo "$REGULATED_DK_OUTPUT" | grep "DK (hex):" | sed 's/.*DK (hex): //')
REGULATED_DK_PUB=$(echo "$REGULATED_DK_OUTPUT" | grep "DK_pub (hex):" | sed 's/.*DK_pub (hex): //')
echo "regulated_usd DK (private): ${REGULATED_DK:0:16}..."
echo "regulated_usd DK_pub: ${REGULATED_DK_PUB:0:16}..."

# Register regulated_usd as REGULATED with threshold=500 (transfers >= 500 are flagged to issuer)
echo ""
echo "Registering regulated_usd as REGULATED with threshold=500..."
$PCLI --home "$ALICE_HOME" tx compliance register-asset regulated_usd --regulated \
    --dk-pub-hex "$REGULATED_DK_PUB" --threshold 500

$PCLI --home "$ALICE_HOME" view sync
$PCLI --home "$BOB_HOME" view sync
$PCLI --home "$OSCAR_HOME" view sync

# Register users for regulated_usd
echo "=== Registering Users for regulated_usd ==="
ALICE_OUTPUT=$($PCLI --home "$ALICE_HOME" tx compliance register-user regulated_usd 2>&1) || true
ALICE_UCK=$(echo "$ALICE_OUTPUT" | grep "UCK (hex):" | sed 's/.*UCK (hex): //')

BOB_OUTPUT=$($PCLI --home "$BOB_HOME" tx compliance register-user regulated_usd 2>&1) || true
BOB_UCK=$(echo "$BOB_OUTPUT" | grep "UCK (hex):" | sed 's/.*UCK (hex): //')

OSCAR_OUTPUT=$($PCLI --home "$OSCAR_HOME" tx compliance register-user regulated_usd 2>&1) || true
OSCAR_UCK=$(echo "$OSCAR_OUTPUT" | grep "UCK (hex):" | sed 's/.*UCK (hex): //')

echo "Alice UCK: ${ALICE_UCK:0:16}..."
echo "Bob UCK: ${BOB_UCK:0:16}..."
echo "Oscar UCK: ${OSCAR_UCK:0:16}..."

# Multi-address registration: Alice registers address index 1 for regulated_usd
echo ""
echo "=== Multi-Address Registration (Alice address index 1) ==="
$PCLI --home "$ALICE_HOME" tx compliance register-user regulated_usd --address-index 1 2>&1 || true
ALICE_ADDRESS_1=$($PCLI --home "$ALICE_HOME" view address 1)
echo "Alice Address 1: $ALICE_ADDRESS_1"

# Final sync to ensure local compliance trees are updated
echo ""
echo "=== Syncing Local Compliance Trees ==="
$PCLI --home "$ALICE_HOME" view sync
$PCLI --home "$BOB_HOME" view sync
$PCLI --home "$OSCAR_HOME" view sync
$PCLI --home "$UNREGISTERED_HOME" view sync

echo "Local compliance trees synced via CompactBlock events."
echo "  - User tree: 4 registrations (Alice x2, Bob, Oscar for regulated_usd)"
echo "  - Asset tree: 1 regulated asset (regulated_usd)"

# Save env
cat > /tmp/compliance-demo.env << EOF
export ALICE_HOME="$ALICE_HOME"
export BOB_HOME="$BOB_HOME"
export OSCAR_HOME="$OSCAR_HOME"
export UNREGISTERED_HOME="$UNREGISTERED_HOME"
export ALICE_ADDRESS="$ALICE_ADDRESS"
export ALICE_ADDRESS_1="$ALICE_ADDRESS_1"
export BOB_ADDRESS="$BOB_ADDRESS"
export OSCAR_ADDRESS="$OSCAR_ADDRESS"
export UNREGISTERED_ADDRESS="$UNREGISTERED_ADDRESS"
export ALICE_UCK="$ALICE_UCK"
export BOB_UCK="$BOB_UCK"
export OSCAR_UCK="$OSCAR_UCK"
export REGULATED_DK="$REGULATED_DK"
export REGULATED_DK_PUB="$REGULATED_DK_PUB"
export PCLI="$PCLI"
export PENUMBRA_NODE_PD_URL="$PENUMBRA_NODE_PD_URL"
EOF

echo ""
echo "=== Setup Complete ==="
echo "Env: /tmp/compliance-demo.env"
echo ""
echo "Asset status:"
echo "  - regulated_usd: REGULATED with threshold=500 (transfers >= 500 flagged to issuer)"
echo "  - penumbra: UNREGULATED (not in IMT)"
echo "  - test_usd: UNREGULATED (not in IMT)"
echo ""
echo "User registrations:"
echo "  regulated_usd:"
echo "    - Alice: address 0 and address 1 (multi-address)"
echo "    - Bob: registered"
echo "    - Oscar: registered"
echo "    - Unregistered: NOT registered"
echo ""
echo "Issuer detection key (regulated_usd, threshold=500):"
echo "  - DK: ${REGULATED_DK:0:16}... (use to scan flagged transfers)"
echo "  - DK_pub: ${REGULATED_DK_PUB:0:16}..."
echo ""
echo "Next steps:"
echo "  - compliance-test-regulated.sh: Test regulated asset transfers with scanning, local sync, and threshold"
echo "  - compliance-test-unregulated.sh: Test unregulated assets (BLACK_HOLE encryption)"
echo "  - compliance-test-unregistered.sh: Test that registered users cannot send regulated assets TO unregistered addresses"
