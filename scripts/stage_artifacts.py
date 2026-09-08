#!/usr/bin/env python3
"""Build and verify the relocatable native, prover, and audit deliverables."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
GROUPS = {
    "native": ["shieldd"],
    "provers": ["bankd-e2e-spend-builder", "bankd-e2e-host-withdrawal-builder"],
    "audit": ["orbis-audit", "orbis-integration"],
}


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def revision(value):
    if not value:
        top = subprocess.check_output(["git", "-C", str(ROOT), "rev-parse", "--show-toplevel"], text=True).strip()
        if Path(top).resolve() != ROOT.resolve():
            raise ValueError("nested source requires --revision or SHIELDD_REVISION")
        value = subprocess.check_output(["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True).strip()
    if not re.fullmatch(r"[0-9a-f]{40}", value):
        raise ValueError("an exact source revision is required")
    return value


def verify(directory, expected, target=None):
    manifest = json.loads((directory / "manifest.json").read_text())
    if manifest["source_revision"] != expected:
        raise ValueError("artifact source revision does not match the selected Shieldd commit")
    if target and manifest["target"] != target:
        raise ValueError("artifact target does not match the requested platform")
    if not manifest["groups"] or any(group not in GROUPS for group in manifest["groups"]):
        raise ValueError("artifact manifest has no recognized deliverables")
    required = set()
    for group in manifest["groups"]:
        required.update({"include/shieldd.h", "lib/libshieldd.a"} if group == "native"
                        else {f"bin/{name}" for name in GROUPS[group]})
    if not required.issubset(manifest["files"]):
        raise ValueError("artifact manifest omits required deliverables")
    if "provers" in manifest["groups"] and not any(name.startswith("lib/gnark/") for name in manifest["files"]):
        raise ValueError("artifact manifest omits gnark libraries")
    for name, checksum in manifest["files"].items():
        path = (directory / name).resolve()
        if not path.is_relative_to(directory.resolve()) or not path.is_file() or digest(path) != checksum:
            raise ValueError(f"missing or corrupt artifact: {name}")
    return manifest


def build(group, output, source_revision, target, profile="release"):
    command = ["cargo", "build", "--profile", profile, "--locked", "--message-format=json-render-diagnostics"]
    if target:
        command += ["--target", target]
    for package in GROUPS[group]:
        command += ["--package", package]
    if group == "native":
        command += ["--lib"]
    env = dict(os.environ)
    for name in ("CARGO_BUILD_JOBS", "RAYON_NUM_THREADS", "GOMAXPROCS"):
        env.setdefault(name, "2")
    copies = {}
    process = subprocess.Popen(command, cwd=ROOT, env=env, stdout=subprocess.PIPE, text=True)
    for line in process.stdout:
        event = json.loads(line)
        if event.get("reason") == "compiler-artifact":
            name = event["target"]["name"]
            if name not in GROUPS[group]:
                continue
            if group == "native":
                for filename in event["filenames"]:
                    if filename.endswith(".a"):
                        copies["lib/libshieldd.a"] = Path(filename)
            elif event.get("executable"):
                copies[f"bin/{name}"] = Path(event["executable"])
        if event.get("reason") == "build-script-executed" and "shieldd-sdk-proof-params" in event["package_id"]:
            for library in (Path(event["out_dir"]) / "gnark").glob("*/*"):
                if library.suffix in (".so", ".dylib", ".dll"):
                    copies[f"lib/gnark/{library.name}"] = library
    if process.wait():
        raise RuntimeError("artifact build failed")
    expected = {"lib/libshieldd.a"} if group == "native" else {f"bin/{name}" for name in GROUPS[group]}
    if not expected.issubset(copies):
        raise ValueError(f"build did not produce {sorted(expected - copies.keys())}")
    if group == "provers" and not any(name.startswith("lib/gnark/") for name in copies):
        raise ValueError("prover build did not produce its native gnark libraries")
    if group == "native":
        copies["include/shieldd.h"] = ROOT / "crates/bin/shieldd/include/shieldd.h"
    if not target:
        info = subprocess.check_output(["rustc", "-vV"], text=True)
        target = next(line.removeprefix("host: ") for line in info.splitlines() if line.startswith("host: "))
    manifest = {"source_revision": source_revision, "target": target, "groups": [], "files": {}}
    if (output / "manifest.json").exists():
        previous = json.loads((output / "manifest.json").read_text())
        if previous["source_revision"] == source_revision and previous["target"] == target:
            manifest = verify(output, source_revision, target)
        else:
            # Remove only previously verified build outputs, preserving unrelated files.
            verify(output, previous["source_revision"], previous["target"])
            for name in previous["files"]:
                (output / name).unlink()
            (output / "manifest.json").unlink()
    for name, source in copies.items():
        destination = output / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)
        manifest["files"][name] = digest(destination)
    manifest["groups"] = sorted(set(manifest["groups"]) | {group})
    (output / "manifest.json").write_text(json.dumps(manifest, sort_keys=True, indent=2) + "\n")
    verify(output, source_revision, target)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("group", choices=[*GROUPS, "verify"])
    parser.add_argument("--output", type=Path, default=ROOT / "target/shieldd")
    parser.add_argument("--revision", default=os.environ.get("SHIELDD_REVISION"))
    parser.add_argument("--target")
    parser.add_argument("--profile", default="release")
    args = parser.parse_args()
    source_revision = revision(args.revision)
    if args.group != "verify":
        build(args.group, args.output, source_revision, args.target, args.profile)
    manifest = verify(args.output, source_revision, args.target)
    total = sum((args.output / name).stat().st_size for name in manifest["files"])
    print(f"Verified {len(manifest['files'])} artifacts ({total} bytes) for {source_revision} on {manifest['target']}")


if __name__ == "__main__":
    main()
