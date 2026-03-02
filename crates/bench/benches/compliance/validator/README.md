# Validator Benchmarks

Measures validator-side TX verification performance: proof verification, ciphertext parsing, and block-level throughput.

## Benchmarks

| Bench name | Command | CSV output |
|------------|---------|------------|
| `validator_proofs` | `cargo bench --bench validator_proofs` | `results/proofs.csv` |
| `validator_verification` | `cargo bench --bench validator_verification` | `results/verification.csv` |
| `validator_flow` | `cargo bench --bench validator_flow` | `results/flow.csv` |
| `validator_block` | `cargo bench --bench validator_block` | `results/block_tps.csv` |

## Result Files

### proofs.csv

Proof generation and verification with circuit constraint counts. Compares v0 vs v0.1 circuit sizes and their impact on prove/verify time.

| Column | Values | Description |
|--------|--------|-------------|
| `circuit` | `spend`, `output` | Which ZK circuit |
| `operation` | `prove`, `verify` | Proof generation or verification |

The `constraints` column shows circuit size:
- v0 spend: ~36K, v0.1 spend: ~122K (3.4x larger)
- v0 output: ~14K, v0.1 output: ~175K (12.6x larger)

Proving time scales roughly with constraint count. Verification time scales weakly (pairing-dominated).

### verification.csv

Validator verification pipeline broken into stages. Isolates how much time each step adds.

| Column | Values | Description |
|--------|--------|-------------|
| `circuit` | `spend`, `output`, *(empty)* | Action type |
| `stage` | `verify`, `ct_deserialize`, `full_verify`, `dleq_parse` | Pipeline stage |

- `verify`: Groth16 proof verification only (`ark_groth16::verify_with_processed_vk`)
- `ct_deserialize`: parse compliance ciphertext bytes into struct + extract circuit public inputs
- `full_verify`: complete pipeline (ciphertext parse + proof verify)
- `dleq_parse`: parse DLEQ proof field elements from bytes

### flow.csv

Batch verification of 100 transactions, matching the production `ProcessProposal` pattern: transactions processed sequentially, actions within each TX verified in parallel.

| Column | Values | Description |
|--------|--------|-------------|
| `batch_size` | `100`, `per_tx` | Batch total or per-TX average |
| `mode` | `serial`, `parallel` | Execution strategy |

- `serial`: all actions verified sequentially (total CPU cost baseline)
- `parallel`: actions within each TX verified via `JoinSet` + `spawn_blocking` (matches production `check_stateless`)
- `per_tx`: batch time / 100 (amortized per-TX cost)
- Each TX is 1 spend + 1 output (1S1O)
- Only measures `check_stateless` (proof + sig + DLEQ). No state I/O.

### block_tps.csv

Full block-level throughput through the real `App::deliver_tx` pipeline. Uses `TestNode` with `TempStorage` to run N transactions through `begin_block` -> N x `deliver_tx` -> `end_block` -> `commit`.

| Column | Values | Description |
|--------|--------|-------------|
| `block_size` | `1`, `5`, `10`, `25` | Number of TXs in the block |
| `metric` | `block_total_ms`, `per_tx_ms`, `tps` | What the value represents |

- `block_total_ms`: wall-clock time for the entire block
- `per_tx_ms`: block time / N
- `tps`: N / (block_time_seconds)
- v0.1 is measured directly. v0 rows are labeled "estimated" -- for real v0 numbers, run on `release/v2.1.x`
- Single sample per block size (each run consumes nullifiers, requiring full re-setup)
- Includes everything: Groth16 verify, stateful checks (nullifier, anchor), state writes, JMT commit
