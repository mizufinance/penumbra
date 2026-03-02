# Client Benchmarks

Measures client-side TX building performance: crypto primitives, compliance enrichment, and end-to-end proof generation.

## Benchmarks

| Bench name | Command | CSV output |
|------------|---------|------------|
| `client_crypto` | `cargo bench --bench client_crypto` | `results/crypto.csv` |
| `client_enrichment` | `cargo bench --bench client_enrichment` | `results/enrichment.csv` |
| `client_flow` | `cargo bench --bench client_flow` | `results/flow.csv` |

## Result Files

### crypto.csv

Individual crypto primitives (v0.1 only, no v0 baseline -- these operations didn't exist pre-compliance). 100 samples each.

| Column | Values | Description |
|--------|--------|-------------|
| `operation` | *(see below)* | The crypto primitive being measured |

Key operations:
- `derive_compliance_scalar`: SHA256-based scalar derivation from diversified generator
- `dleq_compute` / `dleq_verify`: single DLEQ proof generation / verification
- `spend_dleq`: spend-specific DLEQ (1 proof)
- `output_dleqs`: output-specific DLEQs (3 proofs for core/ext/sext tiers)
- `ecdh_shared_secret`: EC point multiplication for shared secret
- `encrypt_spend` / `encrypt_output`: full ciphertext encryption (224B / 544B)
- `encrypt_spend_flagged` / `encrypt_output_flagged`: flagged variant (amount >= threshold)
- `serialize_*` / `deserialize_*`: wire format encoding/decoding
- `leaf_commit` / `indexed_leaf_commit`: Merkle tree leaf commitments

### enrichment.csv

Compliance enrichment overhead per action during TX building (encrypt + DLEQ). 30 samples.

| Column | Values | Description |
|--------|--------|-------------|
| `scenario` | `spend_crypto`, `output_crypto`, `1S1O`, `4S1O` | What's being enriched |

- `spend_crypto`: encrypt + 1 DLEQ for a single spend
- `output_crypto`: encrypt + 3 DLEQs for a single output
- `1S1O`: 1 spend + 1 output (simple transfer)
- `4S1O`: 4 spends + 1 output (multi-input transfer)

### flow.csv

End-to-end TX building: enrichment + authorization + Groth16 proof generation. Compares serial vs concurrent proof generation. 3 samples (slow due to ZK proving).

| Column | Values | Description |
|--------|--------|-------------|
| `scenario` | `1S1O`, `4S1O` | Transaction shape |
| `stage` | `enrich`, `build`, `total` | Pipeline stage |
| `mode` | `serial`, `concurrent`, *(empty)* | Execution strategy |

- `enrich`: compliance crypto only (v0.1 only, v0 is zero)
- `build`: Groth16 proof generation (the dominant cost)
- `total`: full pipeline (enrich + authorize + build)
- `serial`: `plan.build()` single-threaded
- `concurrent`: `plan.build_concurrent()` via tokio (proofs generated in parallel)
