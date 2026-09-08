#!/usr/bin/env python3
"""Keep runtime proof artifacts independent of Git LFS."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

from proof_artifacts import (
    ArtifactError,
    BUNDLE_BYTE_BUDGETS,
    FAMILIES,
    Bundle,
    POINTER_VERSION,
    cache_info,
    parse_lfs_pointer,
)


ROOT = Path(__file__).resolve().parents[1]
LFS_ATTRIBUTES = "filter=lfs diff=lfs merge=lfs -text"
ALLOWED_LFS_PATHS = {
    f"tools/gnark/artifacts/{family}/{family}.sr1cs"
    for family in FAMILIES
}
EXPECTED_WORKFLOW_RESTORES: dict[str, int] = {}


def fail(message: str) -> None:
    print(f"LFS policy: {message}", file=sys.stderr)
    raise SystemExit(1)


def run_git(*args: str) -> str:
    try:
        return subprocess.run(
            ["git", *args],
            cwd=ROOT,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        ).stdout
    except (FileNotFoundError, subprocess.CalledProcessError) as error:
        fail(f"could not run git {' '.join(args)}: {error}")


def event_base() -> str | None:
    event_path = os.environ.get("GITHUB_EVENT_PATH")
    if not event_path:
        return None
    try:
        event = json.loads(Path(event_path).read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"could not read GitHub event payload: {error}")
    pull_request = event.get("pull_request")
    if isinstance(pull_request, dict):
        base = pull_request.get("base")
        if isinstance(base, dict) and isinstance(base.get("sha"), str):
            return base["sha"]
    merge_group = event.get("merge_group")
    if isinstance(merge_group, dict) and isinstance(
        merge_group.get("base_sha"), str
    ):
        return merge_group["base_sha"]
    return None


def enforce_attributes() -> None:
    attribute_rules = {
        line.split(maxsplit=1)[0]: line.split(maxsplit=1)[1]
        for line in (ROOT / ".gitattributes").read_text().splitlines()
        if line.strip()
        and not line.lstrip().startswith("#")
        and "filter=lfs" in line
    }
    if set(attribute_rules) != ALLOWED_LFS_PATHS:
        fail(
            ".gitattributes LFS paths differ from the current proof artifacts: "
            f"expected {sorted(ALLOWED_LFS_PATHS)}, got {sorted(attribute_rules)}"
        )
    for path, attributes in attribute_rules.items():
        if attributes != LFS_ATTRIBUTES:
            fail(f"unexpected LFS attributes for {path}: {attributes}")


def enforce_workflow_fanout() -> None:
    workflows = ROOT / ".github" / "workflows"
    observed: dict[str, int] = {}
    for workflow in workflows.glob("*.yml"):
        text = workflow.read_text(encoding="utf-8")
        relative = workflow.relative_to(ROOT)
        if re.search(r"(?m)^\s+lfs:\s*true\s*$", text):
            fail(f"{relative} hydrates every LFS object during checkout")
        if "materialize-proof-artifacts" in text:
            fail(f"{relative} uses a self-fetching artifact action")
        if re.search(r"proof_artifacts\.py[\"']?\s+materialize(?:\s|$)", text):
            fail(f"{relative} can fetch LFS objects outside the preparer action")

        prepare_count = text.count("/prepare-proof-artifacts")
        restore_count = text.count("/restore-proof-artifacts")
        if prepare_count > 1:
            fail(f"{relative} has {prepare_count} proof-artifact preparers")
        if restore_count and prepare_count != 1:
            fail(f"{relative} restores proof artifacts without one preparer")
        if restore_count:
            observed[workflow.name] = restore_count

    if observed != EXPECTED_WORKFLOW_RESTORES:
        fail(
            "workflow proof-artifact fan-out changed without a policy update: "
            f"expected {EXPECTED_WORKFLOW_RESTORES}, got {observed}"
        )

    prepare = (
        ROOT / ".github/actions/prepare-proof-artifacts/action.yml"
    ).read_text(encoding="utf-8")
    restore = (
        ROOT / ".github/actions/restore-proof-artifacts/action.yml"
    ).read_text(encoding="utf-8")
    if prepare.count("materialize --bundle") != 1:
        fail("the preparer must contain exactly one scoped materialization")
    if "actions/cache/save@" not in prepare:
        fail("the preparer does not publish its cache before consumers start")
    if "proving_key.bin" in prepare or "proving_key.bin" in restore:
        fail("workflow caches must not duplicate Git-backed runtime proving keys")
    if "materialize" in restore or "git lfs pull" in restore.lower():
        fail("the restore-only action contains an LFS fallback")
    for name, action in (("prepare", prepare), ("restore", restore)):
        if "proof-constraints-lfs-v3-" not in action:
            fail(f"the {name} action does not use deterministic v3 cache keys")
        if "restore-keys:" in action:
            fail(f"the {name} action permits inexact proof-artifact cache restores")

    self_fetching = ROOT / ".github/actions/materialize-proof-artifacts/action.yml"
    if self_fetching.exists():
        fail("self-fetching proof-artifact actions are forbidden")

    def extract_job(text: str, name: str) -> str:
        body = text.partition(f"\n  {name}:")[2]
        boundary = re.search(r"(?m)^  [a-zA-Z0-9_-]+:\s*$", body)
        return body[: boundary.start()] if boundary else body

    rust = (workflows / "rust.yml").read_text(encoding="utf-8")
    checks = extract_job(rust, "checks")
    if "proof-artifacts" in checks:
        fail("the Rust checks job depends on proof-artifact hydration")
    tests = extract_job(rust, "tests")
    if "/restore-proof-artifacts" in tests:
        fail("the Rust tests job restores proof artifacts on candidate runs")
    if (
        tests.count("/.github/actions/prepare-proof-artifacts") != 1
        or "bundle: full" not in tests
        or "if: github.event_name != 'pull_request' && github.event_name != 'merge_group'"
        not in tests
    ):
        fail("manual Rust proof replay is not isolated from candidate runs")

    for name in ("containers.yml", "docs-lint.yml", "orbis-integration.yml", "release.yml"):
        text = (workflows / name).read_text(encoding="utf-8")
        if "proof-artifacts" in text:
            fail(f"the {name} workflow depends on proof-artifact hydration")


def enforce_proof_scheduling() -> None:
    workflows = ROOT / ".github" / "workflows"
    for relative in ("rust.yml",):
        workflow = workflows / relative
        if re.search(r"(?m)^\s+schedule:\s*$", workflow.read_text()):
            fail(f"{workflow.relative_to(ROOT)} schedules proof work")


def enforce_bundle_budgets() -> None:
    expected_objects = {
        Bundle.RUNTIME: len(FAMILIES),
        Bundle.CONSTRAINTS: len(FAMILIES),
        Bundle.FULL: 2 * len(FAMILIES),
    }
    for bundle, object_count in expected_objects.items():
        try:
            info = cache_info(bundle)
        except ArtifactError as error:
            fail(str(error))
        if info.object_count != object_count:
            fail(
                f"{bundle.value} bundle has {info.object_count} objects; "
                f"expected {object_count}"
            )
        if info.size_bytes > BUNDLE_BYTE_BUDGETS[bundle.value]:
            fail(
                f"{bundle.value} bundle exceeds its byte budget: "
                f"{info.size_bytes} > {BUNDLE_BYTE_BUDGETS[bundle.value]}"
            )


def committed_content_hash(revision: str, path: str) -> str | None:
    exists = subprocess.run(
        ["git", "cat-file", "-e", f"{revision}:{path}"],
        cwd=ROOT, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    if exists.returncode:
        return None
    with subprocess.Popen(
        ["git", "cat-file", "blob", f"{revision}:{path}"],
        cwd=ROOT, stdout=subprocess.PIPE,
    ) as process:
        assert process.stdout is not None
        prefix = process.stdout.read(1024)
        digest = hashlib.sha256(prefix)
        for chunk in iter(lambda: process.stdout.read(1024 * 1024), b""):
            digest.update(chunk)
        if process.wait():
            fail(f"could not read committed artifact {revision}:{path}")
    if prefix.startswith(f"version {POINTER_VERSION}\n".encode()):
        try:
            return parse_lfs_pointer(prefix.decode(), path).oid
        except (ArtifactError, UnicodeDecodeError) as error:
            fail(str(error))
    return digest.hexdigest()


def enforce_paired_rotations(base: str | None) -> None:
    if base is None:
        return
    changed = set(run_git("diff", "--name-only", f"{base}...HEAD").splitlines())
    for family in FAMILIES:
        root = f"tools/gnark/artifacts/{family}"
        def content_changed(path: str) -> bool:
            return path in changed and committed_content_hash(base, path) != committed_content_hash("HEAD", path)

        sr1cs_changed = content_changed(f"{root}/{family}.sr1cs")
        proving_key_changed = content_changed(f"{root}/proving_key.bin")
        if sr1cs_changed != proving_key_changed:
            fail(
                f"{family} rotates only one setup object; SR1CS and proving key "
                "must rotate together"
            )
        if not sr1cs_changed:
            continue
        required = {
            f"{root}/circuit_metadata.json",
            f"{root}/setup_provenance.json",
            f"{root}/verifying_key.bin",
            f"{root}/verifying_key.json",
        }
        missing = required - changed
        if missing:
            fail(
                f"{family} setup rotation is missing committed evidence updates: "
                f"{sorted(missing)}"
            )


def enforce_tracked_files() -> None:
    tracked_paths = set(
        run_git(
            "grep",
            "--cached",
            "-l",
            "-I",
            "-e",
            f"^version {POINTER_VERSION}$",
            "--",
            ".",
        ).splitlines()
    )
    if tracked_paths != ALLOWED_LFS_PATHS:
        fail(
            "tracked LFS files differ from the current proof artifacts: "
            f"expected {sorted(ALLOWED_LFS_PATHS)}, got {sorted(tracked_paths)}"
        )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--base",
        help="Compare artifact rotations with this base commit (auto-detected in CI).",
    )
    args = parser.parse_args()
    enforce_attributes()
    enforce_workflow_fanout()
    enforce_proof_scheduling()
    enforce_bundle_budgets()
    enforce_paired_rotations(args.base or event_base())
    enforce_tracked_files()
    print(
        "LFS policy: release artifacts are Git-backed, constraint hydration is "
        "bounded, cached by content, and formal replay is external"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
