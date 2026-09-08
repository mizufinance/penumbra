#!/usr/bin/env python3
"""Materialize and validate scoped proof-artifact bundles."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from enum import Enum
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys


REPO_ROOT = Path(__file__).resolve().parents[1]
ARTIFACT_ROOT = REPO_ROOT / "tools" / "gnark" / "artifacts"
FAMILIES = (
    "note_seizure",
    "note_reshape1x8",
    "note_reshape8x1",
    "shielded_ics20_withdrawal",
    "transfer",
)
POINTER_VERSION = "https://git-lfs.github.com/spec/v1"
CACHE_IDENTITY_SCHEMA = "shieldd.proof-artifact-cache"
BUNDLE_BYTE_BUDGETS = {
    "runtime": 145_000_000,
    "constraints": 825_000_000,
    "full": 970_000_000,
}


class ArtifactError(RuntimeError):
    pass


class ArtifactKind(Enum):
    PROVING_KEY = "proving-key"
    SR1CS = "sr1cs"


class Bundle(Enum):
    RUNTIME = "runtime"
    CONSTRAINTS = "constraints"
    FULL = "full"

    @property
    def kinds(self) -> frozenset[ArtifactKind]:
        if self is Bundle.RUNTIME:
            return frozenset({ArtifactKind.PROVING_KEY})
        if self is Bundle.CONSTRAINTS:
            return frozenset({ArtifactKind.SR1CS})
        return frozenset(ArtifactKind)


@dataclass(frozen=True)
class ArtifactFile:
    family: str
    kind: ArtifactKind
    path: Path
    sha256_hex: str
    size_bytes: int | None


@dataclass(frozen=True)
class LfsPointer:
    oid: str
    size_bytes: int


@dataclass(frozen=True)
class CacheInfo:
    identity: str
    object_count: int
    size_bytes: int


def parse_bundle(value: str | Bundle) -> Bundle:
    if isinstance(value, Bundle):
        return value
    try:
        return Bundle(value)
    except ValueError as error:
        raise ArtifactError(f"unsupported proof-artifact bundle: {value}") from error


def required_hash(metadata: dict[str, object], field: str, family: str) -> str:
    value = metadata.get(field)
    if not isinstance(value, str) or len(value) != 64 or any(
        char not in "0123456789abcdef" for char in value
    ):
        raise ArtifactError(f"invalid {field} for {family}")
    return value


def required_size(metadata: dict[str, object], field: str, family: str) -> int:
    value = metadata.get(field)
    if not isinstance(value, int) or isinstance(value, bool) or value <= 0:
        raise ArtifactError(f"invalid {field} for {family}")
    return value


def artifact_files(bundle: str | Bundle = Bundle.FULL) -> list[ArtifactFile]:
    selected = parse_bundle(bundle)
    files: list[ArtifactFile] = []
    for family in FAMILIES:
        directory = ARTIFACT_ROOT / family
        metadata = json.loads((directory / "circuit_metadata.json").read_text())
        if metadata.get("schema") != "shieldd.gnark.circuit_metadata":
            raise ArtifactError(f"unsupported circuit metadata for {family}")
        if metadata.get("circuit") != family:
            raise ArtifactError(f"circuit metadata identity mismatch for {family}")
        candidates = (
            ArtifactFile(
                family=family,
                kind=ArtifactKind.SR1CS,
                path=directory / f"{family}.sr1cs",
                sha256_hex=required_hash(metadata, "sr1cs_sha256_hex", family),
                size_bytes=None,
            ),
            ArtifactFile(
                family=family,
                kind=ArtifactKind.PROVING_KEY,
                path=directory / "proving_key.bin",
                sha256_hex=required_hash(metadata, "proving_key_sha256_hex", family),
                size_bytes=required_size(
                    metadata, "proving_key_size_bytes", family
                ),
            ),
        )
        files.extend(candidate for candidate in candidates if candidate.kind in selected.kinds)
    return files


def lfs_paths(bundle: str | Bundle = Bundle.FULL) -> list[str]:
    return [
        artifact.path.relative_to(REPO_ROOT).as_posix()
        for artifact in artifact_files(bundle)
        if artifact.kind is ArtifactKind.SR1CS
    ]


def parse_lfs_pointer(text: str, path: str) -> LfsPointer:
    fields: dict[str, str] = {}
    for line in text.splitlines():
        key, separator, value = line.partition(" ")
        if not separator or key in fields:
            raise ArtifactError(f"invalid committed Git LFS pointer for {path}")
        fields[key] = value
    if set(fields) != {"version", "oid", "size"}:
        raise ArtifactError(f"invalid committed Git LFS pointer fields for {path}")
    if fields["version"] != POINTER_VERSION:
        raise ArtifactError(f"unsupported committed Git LFS pointer for {path}")
    algorithm, separator, oid = fields["oid"].partition(":")
    if (
        not separator
        or algorithm != "sha256"
        or len(oid) != 64
        or any(char not in "0123456789abcdef" for char in oid)
    ):
        raise ArtifactError(f"invalid committed Git LFS object id for {path}")
    try:
        size_bytes = int(fields["size"])
    except ValueError as error:
        raise ArtifactError(f"invalid committed Git LFS size for {path}") from error
    if size_bytes <= 0 or str(size_bytes) != fields["size"]:
        raise ArtifactError(f"invalid committed Git LFS size for {path}")
    return LfsPointer(oid=oid, size_bytes=size_bytes)


def committed_lfs_pointer(path: str, ref: str = "HEAD") -> LfsPointer:
    result = subprocess.run(
        ["git", "show", f"{ref}:{path}"],
        cwd=REPO_ROOT,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    if result.returncode != 0:
        raise ArtifactError(
            f"could not read committed Git LFS pointer {ref}:{path}: "
            f"{result.stderr.strip()}"
        )
    return parse_lfs_pointer(result.stdout, path)


def cache_info(bundle: str | Bundle, ref: str = "HEAD") -> CacheInfo:
    selected = parse_bundle(bundle)
    digest = hashlib.sha256()
    digest.update(f"{CACHE_IDENTITY_SCHEMA}\n{selected.value}\n".encode())
    total_size = 0
    artifacts = artifact_files(selected)
    for artifact in artifacts:
        path = artifact.path.relative_to(REPO_ROOT).as_posix()
        if artifact.kind is ArtifactKind.SR1CS:
            pointer = committed_lfs_pointer(path, ref)
            if pointer.oid != artifact.sha256_hex:
                raise ArtifactError(
                    f"committed Git LFS object id for {path} does not match circuit metadata"
                )
            oid = pointer.oid
            size_bytes = pointer.size_bytes
        else:
            if artifact.size_bytes is None:
                raise ArtifactError(f"missing committed size for {path}")
            oid = artifact.sha256_hex
            size_bytes = artifact.size_bytes
        digest.update(f"{path}\0{oid}\0{size_bytes}\n".encode())
        total_size += size_bytes
    budget = BUNDLE_BYTE_BUDGETS[selected.value]
    if total_size > budget:
        raise ArtifactError(
            f"{selected.value} proof-artifact bundle is {total_size} bytes; "
            f"budget is {budget} bytes"
        )
    return CacheInfo(
        identity=digest.hexdigest(),
        object_count=len(artifacts),
        size_bytes=total_size,
    )


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verify_file(artifact: ArtifactFile) -> None:
    path = artifact.path
    if not path.is_file() or path.is_symlink():
        raise ArtifactError(f"missing regular proof artifact: {path}")
    actual_size = path.stat().st_size
    if artifact.size_bytes is not None and actual_size != artifact.size_bytes:
        raise ArtifactError(
            f"size mismatch for {path}: expected {artifact.size_bytes}, got {actual_size}"
        )
    actual_hash = sha256(path)
    if actual_hash != artifact.sha256_hex:
        raise ArtifactError(
            f"hash mismatch for {path}: expected {artifact.sha256_hex}, got {actual_hash}"
        )


def verify(bundle: str | Bundle = Bundle.FULL) -> None:
    for artifact in artifact_files(bundle):
        verify_file(artifact)


def artifact_cache_root() -> Path:
    configured = os.environ.get("SHIELDD_PROOF_ARTIFACT_CACHE")
    return Path(configured).expanduser() if configured else REPO_ROOT / ".cache" / "proof-artifacts"


def artifact_cache_path(artifact: ArtifactFile) -> Path:
    return artifact_cache_root() / "sha256" / artifact.sha256_hex[:2] / artifact.sha256_hex


def restore_cached_artifact(artifact: ArtifactFile) -> bool:
    if artifact.kind is not ArtifactKind.SR1CS:
        return False
    cached = artifact_cache_path(artifact)
    if not cached.is_file() or cached.is_symlink():
        return False
    if sha256(cached) != artifact.sha256_hex:
        return False
    artifact.path.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(cached, artifact.path)
    verify_file(artifact)
    return True


def cache_artifact(artifact: ArtifactFile) -> None:
    if artifact.kind is not ArtifactKind.SR1CS:
        return
    verify_file(artifact)
    destination = artifact_cache_path(artifact)
    if destination.is_file() and not destination.is_symlink():
        if sha256(destination) == artifact.sha256_hex:
            return
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_name(f".{destination.name}.{os.getpid()}.tmp")
    try:
        shutil.copy2(artifact.path, temporary)
        if sha256(temporary) != artifact.sha256_hex:
            raise ArtifactError(f"failed to populate proof-artifact cache for {artifact.path}")
        os.replace(temporary, destination)
    finally:
        temporary.unlink(missing_ok=True)


def restore_cached_bundle(bundle: str | Bundle) -> None:
    for artifact in artifact_files(bundle):
        try:
            verify_file(artifact)
        except ArtifactError:
            restore_cached_artifact(artifact)


def cache_bundle(bundle: str | Bundle) -> None:
    for artifact in artifact_files(bundle):
        cache_artifact(artifact)


def install_lfs_filters() -> None:
    command = ["git", "lfs", "install", "--local", "--skip-smudge"]
    result = subprocess.run(command, cwd=REPO_ROOT, check=False)
    if result.returncode != 0:
        raise ArtifactError(
            "Git LFS could not install repository-local filters; install Git LFS "
            "and retry"
        )


def refresh_git_index(bundle: str | Bundle = Bundle.FULL) -> None:
    paths = lfs_paths(bundle)
    if not paths:
        return
    diff_command = ["git", "diff", "--cached", "--quiet", "--", *paths]
    if subprocess.run(diff_command, cwd=REPO_ROOT, check=False).returncode != 0:
        raise ArtifactError("proof artifacts already have staged changes")

    add_command = ["git", "add", "--", *paths]
    if subprocess.run(add_command, cwd=REPO_ROOT, check=False).returncode != 0:
        raise ArtifactError(
            "materialized proof artifacts do not match their committed Git LFS pointers"
        )

    if subprocess.run(diff_command, cwd=REPO_ROOT, check=False).returncode != 0:
        subprocess.run(
            ["git", "restore", "--staged", "--source=HEAD", "--", *paths],
            cwd=REPO_ROOT,
            check=False,
        )
        raise ArtifactError(
            "materialized proof artifacts do not match their committed Git LFS pointers"
        )


def restore(bundle: str | Bundle = Bundle.FULL) -> None:
    selected = parse_bundle(bundle)
    if lfs_paths(selected):
        install_lfs_filters()
    restore_cached_bundle(selected)
    verify(selected)
    cache_bundle(selected)
    refresh_git_index(selected)


def materialize(bundle: str | Bundle = Bundle.FULL) -> None:
    selected = parse_bundle(bundle)
    if lfs_paths(selected):
        install_lfs_filters()
    restore_cached_bundle(selected)
    missing_or_invalid: list[ArtifactFile] = []
    for artifact in artifact_files(selected):
        try:
            verify_file(artifact)
        except ArtifactError:
            missing_or_invalid.append(artifact)
    if missing_or_invalid:
        unavailable_git_artifacts = [
            artifact
            for artifact in missing_or_invalid
            if artifact.kind is ArtifactKind.PROVING_KEY
        ]
        if unavailable_git_artifacts:
            paths = ", ".join(
                artifact.path.relative_to(REPO_ROOT).as_posix()
                for artifact in unavailable_git_artifacts
            )
            raise ArtifactError(
                f"committed runtime artifacts are missing or invalid: {paths}; "
                "restore them from Git"
            )
        include = ",".join(
            artifact.path.relative_to(REPO_ROOT).as_posix()
            for artifact in missing_or_invalid
        )
        command = [
            "git",
            "lfs",
            "pull",
            f"--include={include}",
            "--exclude=",
        ]
        result = subprocess.run(command, cwd=REPO_ROOT, check=False)
        if result.returncode != 0:
            raise ArtifactError(
                "Git LFS could not fetch the current proof artifacts; install Git "
                "LFS, authenticate to the repository, and retry"
            )
        verify(selected)
    cache_bundle(selected)
    refresh_git_index(selected)


def add_bundle_argument(parser: argparse.ArgumentParser) -> None:
    parser.add_argument(
        "--bundle",
        choices=[bundle.value for bundle in Bundle],
        default=Bundle.FULL.value,
    )


def write_github_output(path: Path, selected: Bundle, info: CacheInfo) -> None:
    with path.open("a", encoding="utf-8") as output:
        output.write(f"bundle={selected.value}\n")
        output.write(f"identity={info.identity}\n")
        output.write(f"objects={info.object_count}\n")
        output.write(f"bytes={info.size_bytes}\n")


def main() -> int:
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("verify", "restore", "materialize"):
        add_bundle_argument(commands.add_parser(name))
    cache = commands.add_parser("cache-info")
    add_bundle_argument(cache)
    cache.add_argument("--ref", default="HEAD")
    cache.add_argument("--github-output", type=Path)
    args = parser.parse_args()
    try:
        selected = parse_bundle(args.bundle)
        if args.command == "verify":
            verify(selected)
        elif args.command == "restore":
            restore(selected)
        elif args.command == "materialize":
            materialize(selected)
        else:
            info = cache_info(selected, args.ref)
            if args.github_output is not None:
                write_github_output(args.github_output, selected, info)
            else:
                print(
                    json.dumps(
                        {
                            "bundle": selected.value,
                            "identity": info.identity,
                            "objects": info.object_count,
                            "bytes": info.size_bytes,
                        },
                        sort_keys=True,
                    )
                )
    except (ArtifactError, OSError, json.JSONDecodeError) as error:
        print(f"proof-artifacts: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
