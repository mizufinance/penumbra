#!/bin/bash
# Scenario 3: Transfer TO Unregistered User (regulated asset)
# Should FAIL - receiver not registered for regulated asset
set -e

ENV_FILE=/tmp/compliance-demo.env
[ ! -f "$ENV_FILE" ] && echo "Run compliance-setup.sh first" && exit 1
source "$ENV_FILE"

echo "=== Scenario 3: Transfer TO Unregistered User ==="
echo "Testing that registered user (Alice) cannot send regulated_usd to unregistered address."
echo ""

# Show unregistered wallet has no balance
echo "Unregistered wallet balance (should be empty or no regulated_usd):"
$PCLI --home "$UNREGISTERED_HOME" view balance

# Show Alice has regulated_usd to send
echo ""
echo "Alice wallet balance:"
$PCLI --home "$ALICE_HOME" view balance

# Attempt transfer TO unregistered wallet
# Should FAIL because receiver is not registered for this regulated asset
echo ""
echo "Attempting transfer TO unregistered wallet (should FAIL)..."
set +e
TRANSFER_OUTPUT=$($PCLI --home "$ALICE_HOME" tx send 10regulated_usd --to "$UNREGISTERED_ADDRESS" 2>&1)
TRANSFER_EXIT_CODE=$?
set -e

if [ $TRANSFER_EXIT_CODE -ne 0 ]; then
    echo "Transfer failed as expected:"
    echo "$TRANSFER_OUTPUT" | grep -i "not registered\|compliance\|circuit\|receiver" || echo "$TRANSFER_OUTPUT"
    echo ""
    echo "=== Scenario 3 Complete ==="
    echo "Summary: Cannot send regulated assets TO unregistered users"
else
    echo "ERROR: Transfer succeeded unexpectedly!"
    echo "Regulated assets should not be sendable to unregistered recipients."
    exit 1
fi
