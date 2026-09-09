#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
python3 scripts/stage_artifacts.py provers
export SHIELDD_ARTIFACT_ROOT="$PWD/target/shieldd"

# Each process selects one transport explicitly; family caches never mix artifact directories.
for family in TRANSFER NOTE_RESHAPE SHIELDED_WITHDRAWAL; do
    unset "SHIELDD_GNARK_${family}_LIB" "SHIELDD_GNARK_${family}_DAEMON" "SHIELDD_GNARK_${family}_ARTIFACT_DIR"
done
proof_tests() {
    cargo test --release -p shieldd-sdk-shielded-pool --features bundled-proving-keys "$1" --lib -- --ignored --test-threads=1
}
proof_tests gnark_proof_

mkdir -p target/gnark
runtime_root="$PWD"
(cd tools/gnark && go build -o "$runtime_root/target/gnark/proverdaemon" ./cmd/proverdaemon)
daemon_tests() {
    env "SHIELDD_GNARK_${1}_DAEMON=$runtime_root/target/gnark/proverdaemon" \
        "SHIELDD_GNARK_${1}_ARTIFACT_DIR=$runtime_root/tools/gnark/artifacts/$2" \
        cargo test --release -p shieldd-sdk-shielded-pool --features bundled-proving-keys "$3" --lib -- --ignored --test-threads=1
}
daemon_tests TRANSFER transfer gnark_proof_transfer_proof_roundtrip
daemon_tests NOTE_RESHAPE note_reshape1x8 gnark_proof_note_reshape_1x8_roundtrip
daemon_tests NOTE_RESHAPE note_reshape8x1 gnark_proof_note_reshape_8x1_roundtrip
daemon_tests SHIELDED_WITHDRAWAL shielded_withdrawal gnark_proof_shielded_withdrawal_proof_roundtrip
daemon_tests SHIELDED_WITHDRAWAL shielded_withdrawal gnark_proof_padded_host_withdrawal_proof_roundtrip
