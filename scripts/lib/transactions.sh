#!/bin/bash
# Shared transaction logic for compliance test scripts.
# Source this file after common.sh and load_env.
#
# Requires env vars: PCLI, ALICE_HOME, BOB_HOME, CHARLIE_HOME,
#   UNREGISTERED_HOME, ALICE_ADDRESS, BOB_ADDRESS, CHARLIE_ADDRESS,
#   UNREGISTERED_ADDRESS, PENUMBRA_NODE_PD_URL
#
# Sets globals: REGULATED_DK, REGULATED_DK_PUB, RING_PK, RING_ID, RING_SK

ORBIS_CLI="${ORBIS_CLI:-cli-tool}"
ALICE_ADDRESS_1=""

run_transfer() {
    local home="$1"
    local value="$2"
    local destination="$3"

    run_quiet $PCLI --home "$home" tx transfer --to "$destination" "$value"
}

# Generate issuer detection key.
# Sets: REGULATED_DK, REGULATED_DK_PUB
# Writes: $COMPLIANCE_TMP/issuer-dk.env
generate_issuer_dk() {
    local output
    output=$($PCLI tx compliance generate-dk 2>&1)
    REGULATED_DK=$(echo "$output" | grep "DK (hex):" | sed 's/.*DK (hex): //')
    REGULATED_DK_PUB=$(echo "$output" | grep "DK_pub (hex):" | sed 's/.*DK_pub (hex): //')
    echo "REGULATED_DK=$REGULATED_DK" > "$COMPLIANCE_TMP/issuer-dk.env"
    echo "REGULATED_DK_PUB=$REGULATED_DK_PUB" >> "$COMPLIANCE_TMP/issuer-dk.env"
}

# Setup Orbis ring via DKG or fallback to hardcoded test key.
# Sets: RING_PK, RING_ID, RING_SK (empty unless hardcoded)
# Writes: $COMPLIANCE_TMP/ring-info.env (on DKG success)
setup_ring() {
    RING_SK=""
    local ring_info="$COMPLIANCE_TMP/ring-info.env"

    if [ -f "$ring_info" ]; then
        source "$ring_info"
        RING_SK=""
        return 0
    fi

    if command -v "$ORBIS_CLI" &>/dev/null && nc -z -w1 127.0.0.1 50051 2>/dev/null; then
        local peer1 peer2 peer3 ring_output
        peer1=$($ORBIS_CLI info --endpoint http://127.0.0.1:50051 2>&1 | grep "Peer ID:" | awk '{print $NF}')
        peer2=$($ORBIS_CLI info --endpoint http://127.0.0.1:50052 2>&1 | grep "Peer ID:" | awk '{print $NF}')
        peer3=$($ORBIS_CLI info --endpoint http://127.0.0.1:50053 2>&1 | grep "Peer ID:" | awk '{print $NF}')
        $ORBIS_CLI dkg --endpoint http://127.0.0.1:50051 --threshold 2 --peer-ids "$peer1" "$peer2" "$peer3" 2>&1
        sleep 10
        ring_output=$($ORBIS_CLI get-latest-ring 2>&1) || true
        RING_PK=$(echo "$ring_output" | grep "RING_PK=" | sed 's/RING_PK=//')
        RING_ID=$(echo "$ring_output" | grep "RING_ID=" | sed 's/RING_ID=//')
        if [ -n "$RING_PK" ] && [ -n "$RING_ID" ]; then
            echo "RING_PK=$RING_PK" > "$ring_info"
            echo "RING_ID=$RING_ID" >> "$ring_info"
            return 0
        fi
    fi

    # Fallback: hardcoded test key
    RING_SK="0100000000000000000000000000000000000000000000000000000000000000"
    RING_PK="0800000000000000000000000000000000000000000000000000000000000000"
    RING_ID=""
}

# Register regulated_usd with threshold=500 display units.
# Requires: REGULATED_DK_PUB, RING_PK set
register_regulated_asset() {
    local threshold="${1:-500000000000000000000}"
    $PCLI --home "$ALICE_HOME" tx compliance register-asset regulated_usd --regulated \
        --dk-pub-hex "$REGULATED_DK_PUB" --threshold "$threshold" \
        --ring-pk-hex "$RING_PK"
    run_quiet $PCLI --home "$ALICE_HOME" view sync
    run_quiet $PCLI --home "$BOB_HOME" view sync
    run_quiet $PCLI --home "$CHARLIE_HOME" view sync
}

# Register Alice[0,1], Bob, Charlie for regulated_usd.
# Sets: ALICE_ADDRESS_1
register_users() {
    $PCLI --home "$ALICE_HOME" tx compliance register-user regulated_usd 2>&1 || true
    $PCLI --home "$ALICE_HOME" tx compliance register-user regulated_usd --address-index 1 || true
    ALICE_ADDRESS_1=$($PCLI --home "$ALICE_HOME" view address 1)
    $PCLI --home "$BOB_HOME" tx compliance register-user regulated_usd 2>&1 || true
    $PCLI --home "$CHARLIE_HOME" tx compliance register-user regulated_usd 2>&1 || true

    sleep 5
    run_quiet $PCLI --home "$ALICE_HOME" view sync
    run_quiet $PCLI --home "$BOB_HOME" view sync
    run_quiet $PCLI --home "$CHARLIE_HOME" view sync
    run_quiet $PCLI --home "$UNREGISTERED_HOME" view sync
}

# Execute the full regulated flow:
#   1 split, 9 transfers, 1 consolidate
execute_regulated_transfers() {
    # One split replaces the old unsupported multi-recipient transfer.
    # This devnet uses nonzero base-asset fees, but split fee funding is now
    # handled separately from the regulated asset notes.
    log_info "Alice split regulated_usd into exact send notes: 400, 300, 600, 998700"
    local alice_split_note
    alice_split_note=$($PCLI --home "$ALICE_HOME" view notes --asset regulated_usd --largest --commitment-only)
    run_quiet $PCLI --home "$ALICE_HOME" tx split \
        --note-commitment "$alice_split_note" \
        400regulated_usd \
        300regulated_usd \
        600regulated_usd \
        998700regulated_usd
    run_quiet $PCLI --home "$ALICE_HOME" view sync
    run_quiet $PCLI --home "$BOB_HOME" view sync
    run_quiet $PCLI --home "$CHARLIE_HOME" view sync

    log_info "Alice->Bob:400"
    run_transfer "$ALICE_HOME" 400regulated_usd "$BOB_ADDRESS"
    run_quiet $PCLI --home "$ALICE_HOME" view sync
    run_quiet $PCLI --home "$BOB_HOME" view sync

    log_info "Alice->Charlie:300"
    run_transfer "$ALICE_HOME" 300regulated_usd "$CHARLIE_ADDRESS"
    run_quiet $PCLI --home "$ALICE_HOME" view sync
    run_quiet $PCLI --home "$CHARLIE_HOME" view sync

    log_info "Bob->Alice:50"
    run_transfer "$BOB_HOME" 50regulated_usd "$ALICE_ADDRESS"
    run_quiet $PCLI --home "$BOB_HOME" view sync
    run_quiet $PCLI --home "$ALICE_HOME" view sync

    log_info "Charlie->Alice:40"
    run_transfer "$CHARLIE_HOME" 40regulated_usd "$ALICE_ADDRESS"
    run_quiet $PCLI --home "$CHARLIE_HOME" view sync
    run_quiet $PCLI --home "$ALICE_HOME" view sync

    log_info "Bob->Charlie:100"
    run_transfer "$BOB_HOME" 100regulated_usd "$CHARLIE_ADDRESS"
    run_quiet $PCLI --home "$BOB_HOME" view sync
    run_quiet $PCLI --home "$CHARLIE_HOME" view sync

    log_info "Charlie->Bob:80"
    run_transfer "$CHARLIE_HOME" 80regulated_usd "$BOB_ADDRESS"
    run_quiet $PCLI --home "$CHARLIE_HOME" view sync
    run_quiet $PCLI --home "$BOB_HOME" view sync

    log_info "Bob->Alice:60"
    run_transfer "$BOB_HOME" 60regulated_usd "$ALICE_ADDRESS"
    run_quiet $PCLI --home "$BOB_HOME" view sync
    run_quiet $PCLI --home "$ALICE_HOME" view sync

    log_info "Charlie->Alice:30"
    run_transfer "$CHARLIE_HOME" 30regulated_usd "$ALICE_ADDRESS"
    run_quiet $PCLI --home "$CHARLIE_HOME" view sync
    run_quiet $PCLI --home "$ALICE_HOME" view sync

    # Flagged transfer (above threshold=500)
    log_info "Alice->Bob:600 (FLAGGED, above threshold=500)"
    run_transfer "$ALICE_HOME" 600regulated_usd "$BOB_ADDRESS"
    run_quiet $PCLI --home "$ALICE_HOME" view sync
    run_quiet $PCLI --home "$BOB_HOME" view sync

    log_info "Bob consolidates remaining regulated_usd notes"
    run_quiet $PCLI --home "$BOB_HOME" tx consolidate regulated_usd --family 2x1
    run_quiet $PCLI --home "$ALICE_HOME" view sync
    run_quiet $PCLI --home "$BOB_HOME" view sync
    run_quiet $PCLI --home "$CHARLIE_HOME" view sync
}

# Test that transfers to unregistered users are rejected.
test_unregistered_rejection() {
    log_info "Alice -> Unregistered:10 (must fail)"
    if $PCLI --home "$ALICE_HOME" tx transfer --to "$UNREGISTERED_ADDRESS" 10regulated_usd 2>&1; then
        fail "Transfer to unregistered user should have failed"
    else
        pass "Unregistered user correctly rejected"
    fi
    run_quiet $PCLI --home "$ALICE_HOME" view sync
}

# Test that unregulated assets can be sent without compliance (BLACK_HOLE ACK).
test_unregulated_transfer() {
    log_info "Alice -> Bob:1000 test_usd (unregulated, BLACK_HOLE ACK)"
    run_transfer "$ALICE_HOME" 1000test_usd "$BOB_ADDRESS"
    run_quiet $PCLI --home "$ALICE_HOME" view sync
    run_quiet $PCLI --home "$BOB_HOME" view sync
    pass "Unregulated transfer succeeded"
}

# Scan chain for regulated_usd and write detected transactions.
# Args: $1=output_file (default: $COMPLIANCE_TMP/detected_txs.json)
run_scan() {
    local output_file="${1:-$COMPLIANCE_TMP/detected_txs.json}"
    run_quiet $PCLI tx compliance scan \
        --dk-hex "$REGULATED_DK" \
        --scan-asset-id regulated_usd \
        --node "$PENUMBRA_NODE_PD_URL" \
        --output "$output_file"
}

# Initialize issuer DB: create, import scan results, add aliases.
# Args: $1=db_file, $2=scan_file
init_issuer_db() {
    local db_file="$1"
    local scan_file="$2"
    rm -f "$db_file"
    run_quiet $PCLI tx compliance issuer-db init --db "$db_file"
    run_quiet $PCLI tx compliance issuer-db import \
        --db "$db_file" \
        --scan-output "$scan_file" \
        --dk-hex "$REGULATED_DK" \
        --node "$PENUMBRA_NODE_PD_URL"
    run_quiet $PCLI tx compliance issuer-db alias --db "$db_file" --address "$ALICE_ADDRESS" --name "Alice"
    run_quiet $PCLI tx compliance issuer-db alias --db "$db_file" --address "$BOB_ADDRESS" --name "Bob"
    run_quiet $PCLI tx compliance issuer-db alias --db "$db_file" --address "$CHARLIE_ADDRESS" --name "Charlie"
}
