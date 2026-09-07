import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "proof_artifacts", ROOT / "scripts" / "proof_artifacts.py"
)
assert SPEC is not None and SPEC.loader is not None
PROOF_ARTIFACTS = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = PROOF_ARTIFACTS
SPEC.loader.exec_module(PROOF_ARTIFACTS)


class ProofArtifactsTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.artifacts = self.root / "tools" / "gnark" / "artifacts"
        family_dir = self.artifacts / "transfer"
        family_dir.mkdir(parents=True)
        self.sr1cs = b"synthetic sr1cs"
        self.proving_key = b"synthetic proving key"
        metadata = {
            "schema": "shieldd.gnark.circuit_metadata",
            "circuit": "transfer",
            "sr1cs_sha256_hex": hashlib.sha256(self.sr1cs).hexdigest(),
            "proving_key_sha256_hex": hashlib.sha256(self.proving_key).hexdigest(),
            "proving_key_size_bytes": len(self.proving_key),
        }
        (family_dir / "circuit_metadata.json").write_text(json.dumps(metadata))
        (family_dir / "transfer.sr1cs").write_bytes(self.sr1cs)
        (family_dir / "proving_key.bin").write_bytes(self.proving_key)
        self.original_repo_root = PROOF_ARTIFACTS.REPO_ROOT
        self.original_artifact_root = PROOF_ARTIFACTS.ARTIFACT_ROOT
        self.original_families = PROOF_ARTIFACTS.FAMILIES
        PROOF_ARTIFACTS.REPO_ROOT = self.root
        PROOF_ARTIFACTS.ARTIFACT_ROOT = self.artifacts
        PROOF_ARTIFACTS.FAMILIES = ("transfer",)
        self.addCleanup(self.restore_globals)

    def restore_globals(self) -> None:
        PROOF_ARTIFACTS.REPO_ROOT = self.original_repo_root
        PROOF_ARTIFACTS.ARTIFACT_ROOT = self.original_artifact_root
        PROOF_ARTIFACTS.FAMILIES = self.original_families

    def test_verify_accepts_metadata_pinned_files(self) -> None:
        PROOF_ARTIFACTS.verify()

    def test_bundles_select_only_the_required_artifact_kind(self) -> None:
        runtime = PROOF_ARTIFACTS.artifact_files(
            PROOF_ARTIFACTS.Bundle.RUNTIME
        )
        constraints = PROOF_ARTIFACTS.artifact_files(
            PROOF_ARTIFACTS.Bundle.CONSTRAINTS
        )

        self.assertEqual(
            [artifact.path.name for artifact in runtime], ["proving_key.bin"]
        )
        self.assertEqual(
            [artifact.path.name for artifact in constraints], ["transfer.sr1cs"]
        )

    def test_runtime_materialization_never_invokes_lfs(self) -> None:
        with mock.patch.object(PROOF_ARTIFACTS.subprocess, "run") as run:
            PROOF_ARTIFACTS.materialize(PROOF_ARTIFACTS.Bundle.RUNTIME)

        run.assert_not_called()

    def test_runtime_materialization_rejects_missing_git_artifact(self) -> None:
        proving_key = self.artifacts / "transfer" / "proving_key.bin"
        proving_key.unlink()

        with mock.patch.object(PROOF_ARTIFACTS.subprocess, "run") as run:
            with self.assertRaisesRegex(
                PROOF_ARTIFACTS.ArtifactError, "restore them from Git"
            ):
                PROOF_ARTIFACTS.materialize(PROOF_ARTIFACTS.Bundle.RUNTIME)

        run.assert_not_called()

    def test_full_materialization_reuses_an_existing_runtime_subset(self) -> None:
        sr1cs = self.artifacts / "transfer" / "transfer.sr1cs"
        sr1cs.unlink()

        def fake_run(command: list[str], **kwargs: object) -> subprocess.CompletedProcess:
            if command[:3] == ["git", "lfs", "pull"]:
                sr1cs.write_bytes(self.sr1cs)
            return subprocess.CompletedProcess(command, 0)

        with mock.patch.object(
            PROOF_ARTIFACTS.subprocess, "run", side_effect=fake_run
        ) as run:
            PROOF_ARTIFACTS.materialize(PROOF_ARTIFACTS.Bundle.FULL)

        self.assertEqual(
            run.call_args_list[1].args[0],
            [
                "git",
                "lfs",
                "pull",
                "--include=tools/gnark/artifacts/transfer/transfer.sr1cs",
                "--exclude=",
            ],
        )

    def test_materialize_restores_constraint_from_content_cache(self) -> None:
        sr1cs = self.artifacts / "transfer" / "transfer.sr1cs"
        sr1cs.unlink()
        cache = self.root / "shared-cache"
        oid = hashlib.sha256(self.sr1cs).hexdigest()
        cached = cache / "sha256" / oid[:2] / oid
        cached.parent.mkdir(parents=True)
        cached.write_bytes(self.sr1cs)

        completed = subprocess.CompletedProcess([], 0)
        with mock.patch.dict(
            os.environ, {"SHIELDD_PROOF_ARTIFACT_CACHE": str(cache)}
        ), mock.patch.object(
            PROOF_ARTIFACTS.subprocess, "run", return_value=completed
        ) as run:
            PROOF_ARTIFACTS.materialize(PROOF_ARTIFACTS.Bundle.CONSTRAINTS)

        self.assertEqual(sr1cs.read_bytes(), self.sr1cs)
        self.assertFalse(
            any(call.args[0][:3] == ["git", "lfs", "pull"] for call in run.call_args_list)
        )

    def test_materialize_replaces_corrupt_content_cache_after_lfs_pull(self) -> None:
        sr1cs = self.artifacts / "transfer" / "transfer.sr1cs"
        sr1cs.unlink()
        cache = self.root / "shared-cache"
        oid = hashlib.sha256(self.sr1cs).hexdigest()
        cached = cache / "sha256" / oid[:2] / oid
        cached.parent.mkdir(parents=True)
        cached.write_bytes(b"corrupt")

        def fake_run(command: list[str], **kwargs: object) -> subprocess.CompletedProcess:
            if command[:3] == ["git", "lfs", "pull"]:
                sr1cs.write_bytes(self.sr1cs)
            return subprocess.CompletedProcess(command, 0)

        with mock.patch.dict(
            os.environ, {"SHIELDD_PROOF_ARTIFACT_CACHE": str(cache)}
        ), mock.patch.object(
            PROOF_ARTIFACTS.subprocess, "run", side_effect=fake_run
        ):
            PROOF_ARTIFACTS.materialize(PROOF_ARTIFACTS.Bundle.CONSTRAINTS)

        self.assertEqual(cached.read_bytes(), self.sr1cs)

    def test_constraint_cache_identity_rejects_pointer_metadata_mismatch(self) -> None:
        pointer = PROOF_ARTIFACTS.LfsPointer(oid="a" * 64, size_bytes=23)
        with mock.patch.object(
            PROOF_ARTIFACTS, "committed_lfs_pointer", return_value=pointer
        ):
            with self.assertRaisesRegex(
                PROOF_ARTIFACTS.ArtifactError, "does not match circuit metadata"
            ):
                PROOF_ARTIFACTS.cache_info(PROOF_ARTIFACTS.Bundle.CONSTRAINTS)

    def test_restore_only_rejects_a_cache_miss_without_lfs_pull(self) -> None:
        (self.artifacts / "transfer" / "proving_key.bin").unlink()
        with mock.patch.object(PROOF_ARTIFACTS.subprocess, "run") as run:
            with self.assertRaisesRegex(
                PROOF_ARTIFACTS.ArtifactError, "missing regular proof artifact"
            ):
                PROOF_ARTIFACTS.restore(PROOF_ARTIFACTS.Bundle.RUNTIME)

        run.assert_not_called()

    def test_runtime_cache_identity_uses_metadata_hashes_and_sizes(self) -> None:
        with mock.patch.object(
            PROOF_ARTIFACTS, "committed_lfs_pointer"
        ) as committed:
            first = PROOF_ARTIFACTS.cache_info(PROOF_ARTIFACTS.Bundle.RUNTIME)
            second = PROOF_ARTIFACTS.cache_info(PROOF_ARTIFACTS.Bundle.RUNTIME)

        self.assertEqual(first, second)
        self.assertEqual(first.object_count, 1)
        self.assertEqual(first.size_bytes, len(self.proving_key))
        committed.assert_not_called()

        metadata_path = self.artifacts / "transfer" / "circuit_metadata.json"
        metadata = json.loads(metadata_path.read_text())
        metadata["proving_key_sha256_hex"] = "b" * 64
        metadata_path.write_text(json.dumps(metadata))
        changed = PROOF_ARTIFACTS.cache_info(PROOF_ARTIFACTS.Bundle.RUNTIME)
        self.assertNotEqual(first.identity, changed.identity)

    def test_parse_lfs_pointer_rejects_non_pointer_content(self) -> None:
        with self.assertRaisesRegex(
            PROOF_ARTIFACTS.ArtifactError, "invalid committed Git LFS pointer"
        ):
            PROOF_ARTIFACTS.parse_lfs_pointer("not a pointer\n", "artifact")

    def test_materialize_pulls_only_current_paths(self) -> None:
        sr1cs = self.artifacts / "transfer" / "transfer.sr1cs"
        sr1cs.unlink()

        def fake_run(command: list[str], **kwargs: object) -> subprocess.CompletedProcess:
            if command[:3] == ["git", "lfs", "pull"]:
                sr1cs.write_bytes(self.sr1cs)
            return subprocess.CompletedProcess(command, 0)

        with mock.patch.object(PROOF_ARTIFACTS.subprocess, "run", side_effect=fake_run) as run:
            PROOF_ARTIFACTS.materialize()

        self.assertEqual(
            run.call_args_list[0].args[0],
            ["git", "lfs", "install", "--local", "--skip-smudge"],
        )
        command = run.call_args_list[1].args[0]
        self.assertEqual(command[:3], ["git", "lfs", "pull"])
        self.assertEqual(command[-1], "--exclude=")
        self.assertEqual(
            command[-2],
            "--include=tools/gnark/artifacts/transfer/transfer.sr1cs",
        )
        self.assertEqual(
            run.call_args_list[2].args[0],
            [
                "git",
                "diff",
                "--cached",
                "--quiet",
                "--",
                "tools/gnark/artifacts/transfer/transfer.sr1cs",
            ],
        )
        self.assertEqual(
            run.call_args_list[3].args[0],
            [
                "git",
                "add",
                "--",
                "tools/gnark/artifacts/transfer/transfer.sr1cs",
            ],
        )
        self.assertEqual(run.call_args_list[4].args[0], run.call_args_list[2].args[0])

    def test_materialize_installs_filters_without_pull_when_files_are_valid(self) -> None:
        completed = subprocess.CompletedProcess([], 0)
        with mock.patch.object(
            PROOF_ARTIFACTS.subprocess, "run", return_value=completed
        ) as run:
            PROOF_ARTIFACTS.materialize()
        self.assertEqual(
            [call.args[0] for call in run.call_args_list],
            [
                ["git", "lfs", "install", "--local", "--skip-smudge"],
                [
                    "git",
                    "diff",
                    "--cached",
                    "--quiet",
                    "--",
                    "tools/gnark/artifacts/transfer/transfer.sr1cs",
                ],
                [
                    "git",
                    "add",
                    "--",
                    "tools/gnark/artifacts/transfer/transfer.sr1cs",
                ],
                [
                    "git",
                    "diff",
                    "--cached",
                    "--quiet",
                    "--",
                    "tools/gnark/artifacts/transfer/transfer.sr1cs",
                ],
            ],
        )

    def test_materialize_rejects_missing_lfs_filters(self) -> None:
        completed = subprocess.CompletedProcess([], 1)
        with mock.patch.object(
            PROOF_ARTIFACTS.subprocess, "run", return_value=completed
        ):
            with self.assertRaisesRegex(
                PROOF_ARTIFACTS.ArtifactError,
                "repository-local filters",
            ):
                PROOF_ARTIFACTS.materialize()

    def test_materialize_rejects_index_mismatch(self) -> None:
        diff_calls = 0

        def fake_run(command: list[str], **kwargs: object) -> subprocess.CompletedProcess:
            nonlocal diff_calls
            if command[:4] == ["git", "diff", "--cached", "--quiet"]:
                diff_calls += 1
                return subprocess.CompletedProcess(command, 1 if diff_calls == 2 else 0)
            return subprocess.CompletedProcess(command, 0)

        with mock.patch.object(
            PROOF_ARTIFACTS.subprocess,
            "run",
            side_effect=fake_run,
        ) as run:
            with self.assertRaisesRegex(
                PROOF_ARTIFACTS.ArtifactError,
                "committed Git LFS pointers",
            ):
                PROOF_ARTIFACTS.materialize()
        self.assertEqual(
            run.call_args_list[-1].args[0][:4],
            ["git", "restore", "--staged", "--source=HEAD"],
        )

    def test_materialize_rejects_preexisting_staged_artifact(self) -> None:
        def fake_run(command: list[str], **kwargs: object) -> subprocess.CompletedProcess:
            return subprocess.CompletedProcess(
                command,
                1 if command[:4] == ["git", "diff", "--cached", "--quiet"] else 0,
            )

        with mock.patch.object(
            PROOF_ARTIFACTS.subprocess,
            "run",
            side_effect=fake_run,
        ) as run:
            with self.assertRaisesRegex(
                PROOF_ARTIFACTS.ArtifactError,
                "already have staged changes",
            ):
                PROOF_ARTIFACTS.materialize()
        self.assertEqual(len(run.call_args_list), 2)


if __name__ == "__main__":
    unittest.main()
