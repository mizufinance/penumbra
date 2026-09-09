#!/usr/bin/env python3
"""Check local links in first-party documentation, including new working-tree docs."""
from pathlib import Path
import re
import subprocess
import sys
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parents[1]


def anchors(text: str) -> set[str]:
    text = re.sub(r"```.*?```", "", text, flags=re.DOTALL)
    found = set(re.findall(r'(?:id|name)=["\']([^"\']+)["\']', text))
    counts: dict[str, int] = {}
    for heading in re.findall(r"^#{1,6}\s+(.+?)\s*#*\s*$", text, re.MULTILINE):
        slug = re.sub(r"[^\w\- ]", "", heading.lower()).replace(" ", "-")
        count = counts.get(slug, 0)
        counts[slug] = count + 1
        found.add(f"{slug}-{count}" if count else slug)
    return found


def main() -> int:
    errors: list[str] = []
    names = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=ROOT
    ).decode().split("\0")
    for name in sorted(set(names)):
        path = ROOT / name
        if path.suffix != ".md" or name.startswith("third_party/") or not path.is_file():
            continue
        text = path.read_text(encoding="utf-8")
        # Code examples may show placeholder links; they are not navigation.
        text = re.sub(r"```.*?```", "", text, flags=re.DOTALL)
        targets = re.findall(r"\[[^\]]*\]\(([^)]+)\)", text)
        targets += re.findall(r"^\[[^\]]+\]:\s*(\S+)", text, re.MULTILINE)
        for target in targets:
            target = target.strip().split(' "', 1)[0].strip("<>")
            if re.match(r"[a-zA-Z][a-zA-Z0-9+.-]*:", target) or target.startswith("//"):
                continue
            filename, _, fragment = unquote(target).partition("#")
            resolved = (path.parent / filename).resolve() if filename else path
            if not resolved.exists():
                errors.append(f"{name}: broken link {target!r}")
            elif fragment and resolved.is_file() and resolved.suffix == ".md":
                if fragment not in anchors(resolved.read_text(encoding="utf-8")):
                    errors.append(f"{name}: missing anchor {target!r}")
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
