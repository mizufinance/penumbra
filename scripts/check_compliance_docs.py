#!/usr/bin/env python3
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
STALE = {
    "ordinary Orbis PRE share evidence": "Orbis does not return this consensus artifact",
    "asset-seizure-proof-plan.md": "the historical seizure plan was deleted",
    "chain-scope.md": "the ownership boundary moved to enforcement-and-seizure.md",
}


def main() -> int:
    errors: list[str] = []
    markdown = list(ROOT.glob("*.md")) + list((ROOT / "docs").rglob("*.md"))
    for path in markdown:
        text = path.read_text(encoding="utf-8")
        for phrase, reason in STALE.items():
            if phrase in text:
                errors.append(f"{path.relative_to(ROOT)}: stale phrase {phrase!r}: {reason}")
        for target in re.findall(r"\[[^\]]*\]\(([^)]+)\)", text):
            target = target.strip().split("#", 1)[0]
            if not target or "://" in target or target.startswith(("#", "mailto:")):
                continue
            resolved = (path.parent / target).resolve()
            if not resolved.exists():
                errors.append(f"{path.relative_to(ROOT)}: broken link {target!r}")
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
