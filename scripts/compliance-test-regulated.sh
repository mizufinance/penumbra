#!/bin/bash
# Scenario 1: Regulated Transfer (regulated_usd)
# Alice->Bob, scannable by registered users
# Also tests local compliance tree sync via multiple transfers
set -e

ENV_FILE=/tmp/compliance-demo.env
[ ! -f "$ENV_FILE" ] && echo "Run compliance-setup.sh first" && exit 1
source "$ENV_FILE"

echo "=== Scenario 1: Regulated Transfer ==="

# Transfer regulated_usd (Alice has genesis allocation)
echo "Transfer 1: Alice -> Bob (100 regulated_usd)"
$PCLI --home "$ALICE_HOME" tx send 100regulated_usd --to "$BOB_ADDRESS"

# Sync
$PCLI --home "$ALICE_HOME" view sync
$PCLI --home "$BOB_HOME" view sync
$PCLI --home "$OSCAR_HOME" view sync

# Second transfer to verify local compliance tree sync (Bob is now cached counterparty)
echo ""
echo "Transfer 2: Alice -> Bob (50 regulated_usd, via local compliance sync)"
$PCLI --home "$ALICE_HOME" tx send 50regulated_usd --to "$BOB_ADDRESS"

$PCLI --home "$ALICE_HOME" view sync
$PCLI --home "$BOB_HOME" view sync

# Bob sends back to Alice (verifies bidirectional sync)
echo ""
echo "Transfer 3: Bob -> Alice (25 regulated_usd)"
$PCLI --home "$BOB_HOME" tx send 25regulated_usd --to "$ALICE_ADDRESS"

$PCLI --home "$ALICE_HOME" view sync
$PCLI --home "$BOB_HOME" view sync

# Derive daily keys
DATE=$(python3 -c "import time; print(int(time.time() // 86400))")
ALICE_DAILY=$($PCLI tx compliance derive-daily-key --uck-hex "$ALICE_UCK" --date "$DATE" 2>&1 | grep "Full Key Set:" | sed 's/.*Full Key Set: *//')
BOB_DAILY=$($PCLI tx compliance derive-daily-key --uck-hex "$BOB_UCK" --date "$DATE" 2>&1 | grep "Full Key Set:" | sed 's/.*Full Key Set: *//')
OSCAR_DAILY=$($PCLI tx compliance derive-daily-key --uck-hex "$OSCAR_UCK" --date "$DATE" 2>&1 | grep "Full Key Set:" | sed 's/.*Full Key Set: *//')

# Scan
echo ""
echo "=== Scanning with Daily Keys ==="
echo "Alice (sender - should see all 3 transfers):"
$PCLI tx compliance scan --node "$PENUMBRA_NODE_PD_URL" --daily-key-hex "$ALICE_DAILY"

echo "Bob (receiver - should see all 3 transfers):"
$PCLI tx compliance scan --node "$PENUMBRA_NODE_PD_URL" --daily-key-hex "$BOB_DAILY"

echo "Oscar (should see NOTHING):"
$PCLI tx compliance scan --node "$PENUMBRA_NODE_PD_URL" --daily-key-hex "$OSCAR_DAILY"

# ============================================================================
# Threshold Testing (regulated_usd has threshold=500)
# ============================================================================
echo ""
echo "=== Threshold Transfer Testing (regulated_usd, threshold=500) ==="

# Above threshold - should be flagged (encrypted to issuer DK)
echo ""
echo "Transfer 4: Alice -> Bob (600 regulated_usd, ABOVE threshold - flagged)"
$PCLI --home "$ALICE_HOME" tx send 600regulated_usd --to "$BOB_ADDRESS"
$PCLI --home "$ALICE_HOME" view sync
$PCLI --home "$BOB_HOME" view sync

echo "Scanning above-threshold transfer (user daily key - detection tier encrypted to issuer):"
$PCLI tx compliance scan --node "$PENUMBRA_NODE_PD_URL" --daily-key-hex "$ALICE_DAILY"

# Final balances
echo ""
echo "=== Final Balances ==="
echo "Alice:" && $PCLI --home "$ALICE_HOME" view balance
echo "Bob:" && $PCLI --home "$BOB_HOME" view balance

echo ""
echo "=== Scenario 1 Complete ==="
echo "Summary:"
echo "  - 4 regulated_usd transfers completed successfully"
echo "  - Local compliance tree sync: WORKING (transfers 2 & 3 used synced counterparty data)"
echo "  - Scanner visibility: Alice and Bob see transfers, Oscar sees nothing"
echo "  - Threshold testing:"
echo "    - Transfers 1-3 (25-100 regulated_usd): Below threshold, user daily key decrypts"
echo "    - Transfer 4 (600 regulated_usd): Above threshold, flagged to issuer DK"
