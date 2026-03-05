# Scanner Benchmarks

Scanner category measures detection/decryption scanning workload.

## Benches

| Bench name | Command | Output |
|---|---|---|
| `scanner_flow` | `cargo bench --bench scanner_flow` | `scanner/scanner.csv` + `scanner/sections.csv` + `scanner/sections/{detect,decrypt}.csv` |
| `scanner_decryption` | `cargo bench --bench scanner_decryption` | `scanner/decryption.csv` |
| `scanner_trees` | `cargo bench --bench scanner_trees` | `scanner/trees.csv` |

## Flow Outputs

`scanner.csv` contains top-level scanner KPIs (`full`, and `full_per_tx` in complete suite).
`sections.csv` contains the section overview rows for `detect` and `decrypt`.

Per-section files contain section-level rows:
- `scanner/sections/detect.csv`
- `scanner/sections/decrypt.csv`

Cross-category flow overview:
- `flows.csv` (contains `category=scanner` rows from this bench)

`BENCH_SUITE=regression` uses one canonical variant per section.
`BENCH_SUITE=complete` adds per-tx flow variants and wider scenario coverage.
