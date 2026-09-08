import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "stage_artifacts.py"
SPEC = importlib.util.spec_from_file_location("stage_artifacts", SCRIPT)
stage = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(stage)


class StagedArtifactsTests(unittest.TestCase):
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
