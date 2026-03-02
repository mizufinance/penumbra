# Scanner Benchmarks

Measures issuer-side scanning performance: detection, decryption tiers, tree operations, and block-level throughput.

All scanner benchmarks are v0.1 only. Compliance scanning did not exist in v0.

## Benchmarks

| Bench name | Command | CSV output |
|------------|---------|------------|
| `scanner_decryption` | `cargo bench --bench scanner_decryption` | `results/decryption.csv` |
| `scanner_trees` | `cargo bench --bench scanner_trees` | `results/trees.csv` |
| `scanner_flow` | `cargo bench --bench scanner_flow` | `results/flow.csv` |

## Result Files

### decryption.csv

Individual decryption tier performance and batch scanning. 50 samples.

| Column | Values | Description |
|--------|--------|-------------|
| `operation` | *(see below)* | Decryption operation |
| `batch_size` | `10`, `100`, `1000`, *(empty)* | Batch size (empty for single ops) |

Single operations:
- `decrypt_core`: core tier (amount + self address)
- `decrypt_extension`: extension tier (counterparty address)
- `decrypt_full`: core + extension combined
- `spend_decrypt_core`: spend ciphertext core tier
- `flagged_decrypt_*`: flagged variant (amount >= threshold, decrypted via issuer DK)
- `detection_decrypt`: detection tier only (asset_id + flag check)

Batch operations (throughput measurement):
- `decrypt` with batch_size: batch full decryption
- `detection` with batch_size: batch detection-only scanning at 50% match rate

### trees.csv

QuadTree and IMT operations at various tree sizes. 10 samples.

| Column | Values | Description |
|--------|--------|-------------|
| `tree` | `quad`, `imt` | Tree type |
| `operation` | `insert`, `auth_path`, `verify`, `root`, `membership`, `non_membership` | Tree operation |
| `size` | `100`/`1000`/`10000` (quad), `50`/`500`/`5000` (imt) | Number of leaves |

- **QuadTree**: user registrations (address + asset pairs). Ops: `insert`, `auth_path`, `verify`, `root`
- **IMT** (Indexed Merkle Tree): asset registrations with Policy-in-Leaf. Ops: `insert`, `membership`, `non_membership`, `root`

### flow.csv

Block-level scanning simulation. Measures detection-only vs full scan at different block sizes and match rates.

| Column | Values | Description |
|--------|--------|-------------|
| `block_size` | `10`, `100` | TXs in simulated block |
| `match_rate` | `10%`, `100%` | Percentage of TXs matching the scanner's DK |
| `stage` | `detect`, `full` | Scan depth |

- `detect`: detection tier only (try DK on every ciphertext)
- `full`: detection + core + extension decryption for matches
- Each TX has 1 spend + 1 output (2 ciphertexts per TX)
