import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

SCRIPT = Path(__file__).resolve().parents[1] / "stage_artifacts.py"
SPEC = importlib.util.spec_from_file_location("stage_artifacts", SCRIPT)
stage = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(stage)


class StagedArtifactsTests(unittest.TestCase):
    def test_prover_stage_excludes_retired_libraries_from_build_cache(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            out = root / "build"
            libraries = out / "gnark" / "test-target"
            libraries.mkdir(parents=True)
            current = {f"libshieldd_gnark_{family}.dylib" for family in
                       ("transfer", "note_reshape", "shielded_withdrawal")}
            for name in current | {"libshieldd_gnark_retired.dylib"}:
                (libraries / name).write_bytes(name.encode())
            events = [{"reason": "build-script-executed",
                       "package_id": "shieldd-sdk-proof-params", "out_dir": str(out)}]
            for name in stage.GROUPS["provers"]:
                binary = root / name
                binary.write_bytes(name.encode())
                events.append({"reason": "compiler-artifact", "target": {"name": name},
                               "executable": str(binary)})
            process = Mock(stdout=[json.dumps(event) for event in events])
            process.wait.return_value = 0
            destination = root / "stage"
            with patch.object(stage.subprocess, "Popen", return_value=process):
                stage.build("provers", destination, "a" * 40, "test-target")
            self.assertEqual({p.name for p in (destination / "lib/gnark").iterdir()}, current)

    def test_revision_platform_integrity_and_required_outputs(self):
        for case in ["valid", "revision", "platform", "corrupt", "unlisted", "missing"]:
            with self.subTest(case=case), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                files = {}
                for name in ("lib/libshieldd.a", "include/shieldd.h"):
                    path = root / name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(name.encode())
                    files[name] = stage.digest(path)
                manifest = {"source_revision": "a" * 40, "target": "test-target", "groups": ["native"], "files": files}
                revision, target = "a" * 40, "test-target"
                if case == "revision": revision = "b" * 40
                if case == "platform": target = "other-target"
                if case == "corrupt": (root / "lib/libshieldd.a").write_bytes(b"corrupt")
                if case == "unlisted": manifest["files"] = {}
                if case == "missing": (root / "lib/libshieldd.a").unlink()
                (root / "manifest.json").write_text(json.dumps(manifest))
                if case == "valid":
                    stage.verify(root, revision, target)
                else:
                    with self.assertRaises((ValueError, FileNotFoundError)):
                        stage.verify(root, revision, target)


if __name__ == "__main__":
    unittest.main()
