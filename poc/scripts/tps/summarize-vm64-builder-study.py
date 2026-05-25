#!/usr/bin/env python3
import argparse
import csv
import json
from pathlib import Path


def read_csv(path: Path):
    if not path.exists():
        return []
    with path.open(newline="") as f:
        return list(csv.DictReader(f))


def to_float(row, key, default=0.0):
    try:
        return float(row.get(key, default) or default)
    except ValueError:
        return default


def to_bool(row, key):
    return str(row.get(key, "")).lower() == "true"


def rank_rows(rows):
    ranked = list(rows)
    ranked.sort(
        key=lambda r: (
            not to_bool(r, "sustainable"),
            -to_float(r, "effective_built_tps"),
            -to_float(r, "candidate_ready_turn_ratio"),
            -to_float(r, "candidate_tx_coverage_ratio"),
            to_float(r, "background_build_candidate_ms_mean"),
        )
    )
    return ranked


def write_top_configs(rows, path: Path):
    fieldnames = [
        "mode",
        "num_validators",
        "block_interval_ms",
        "max_block_txs",
        "segment_tx_count",
        "ready_guard_ms",
        "offered_tps",
        "sustainable",
        "effective_built_tps",
        "candidate_ready_turn_ratio",
        "candidate_tx_coverage_ratio",
        "background_build_candidate_ms_mean",
    ]
    with path.open("w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        for row in rows[:10]:
            writer.writerow({k: row.get(k, "") for k in fieldnames})


def write_machine_profile(run_dir: Path, out_path: Path):
    preflight = run_dir / "preflight.txt"
    metadata = run_dir / "metadata.json"
    with out_path.open("w") as f:
        f.write("# VM64 Machine Profile\n\n")
        if metadata.exists():
            data = json.loads(metadata.read_text())
            f.write("## Metadata\n\n")
            for key in ("run_id", "bench_git_rev", "bench_source_dirty", "bench_host_label"):
                f.write(f"- `{key}`: `{data.get(key, 'unknown')}`\n")
            f.write("\n")
        if preflight.exists():
            f.write("## Preflight\n\n```text\n")
            f.write(preflight.read_text())
            f.write("\n```\n")


def write_summary(run_dir: Path, out_path: Path):
    phase_a_confirmation = read_csv(run_dir / "phaseA" / "confirmation.csv")
    phase_b_summary = read_csv(run_dir / "phaseB" / "coarse_summary.csv")
    phase_c_summary = read_csv(run_dir / "phaseC" / "tuned_summary.csv")
    phase_d_large = read_csv(run_dir / "phaseD" / "large_block_summary.csv")
    phase_d_final = read_csv(run_dir / "phaseD" / "final_lab.csv")

    ranked_final = rank_rows(phase_d_final or phase_d_large or phase_c_summary or phase_b_summary)

    with out_path.open("w") as f:
        f.write("# VM64 Builder Study Summary\n\n")

        f.write("## Phase A One-Shot Highlights\n\n")
        if phase_a_confirmation:
            by_block = {}
            for row in phase_a_confirmation:
                key = row["max_block_txs"]
                score = to_float(row, "build_wall_ms")
                cur = by_block.get(key)
                if cur is None or score < cur[0]:
                    by_block[key] = (score, row)
            for max_block_txs in sorted(by_block, key=lambda x: int(x)):
                row = by_block[max_block_txs][1]
                f.write(
                    f"- `{max_block_txs}` tx: best `segment_tx_count={row['segment_tx_count']}`, "
                    f"`build_wall_ms~={row['build_wall_ms']}`, "
                    f"`aggregate_total_ms~={row['aggregate_total_ms']}`\n"
                )
        else:
            f.write("- no Phase A confirmation results found\n")

        f.write("\n## Best Cadence-Aware Configs\n\n")
        if ranked_final:
            for row in ranked_final[:5]:
                f.write(
                    f"- `{row.get('mode','?')}`, validators `{row.get('num_validators','?')}`, "
                    f"interval `{row.get('block_interval_ms','?')} ms`, block `{row.get('max_block_txs','?')}`, "
                    f"segment `{row.get('segment_tx_count','?')}`, guard `{row.get('ready_guard_ms','?')}`, "
                    f"`effective_built_tps~={row.get('effective_built_tps','?')}`, "
                    f"`sustainable={row.get('sustainable','?')}`\n"
                )
        else:
            f.write("- no cadence-aware results found\n")

        f.write("\n## Available Result Sets\n\n")
        for rel in [
            "phase0/smoke-builder-one-shot.csv",
            "phase0/smoke-lookahead.csv",
            "phaseA/screening.csv",
            "phaseA/confirmation.csv",
            "phaseB/coarse_raw.csv",
            "phaseB/coarse_summary.csv",
            "phaseC/tuned_raw.csv",
            "phaseC/tuned_summary.csv",
            "phaseD/large_block_raw.csv",
            "phaseD/final_lab.csv",
            "phaseD/final_one_shot.csv",
        ]:
            if (run_dir / rel).exists():
                f.write(f"- `{rel}`\n")


def main():
    parser = argparse.ArgumentParser(description="Summarize the VM64 strict builder study")
    parser.add_argument("--run-dir", required=True)
    parser.add_argument("--out-dir", required=True)
    args = parser.parse_args()

    run_dir = Path(args.run_dir).resolve()
    out_dir = Path(args.out_dir).resolve()
    out_dir.mkdir(parents=True, exist_ok=True)

    ranked_source = (
        read_csv(run_dir / "phaseD" / "final_lab.csv")
        or read_csv(run_dir / "phaseD" / "large_block_summary.csv")
        or read_csv(run_dir / "phaseC" / "tuned_summary.csv")
        or read_csv(run_dir / "phaseB" / "coarse_summary.csv")
    )

    ranked = rank_rows(ranked_source)
    write_top_configs(ranked, out_dir / "top-configs.csv")
    write_summary(run_dir, out_dir / "summary.md")
    write_machine_profile(run_dir, out_dir / "machine-profile.md")


if __name__ == "__main__":
    main()
