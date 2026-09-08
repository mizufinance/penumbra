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
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            files = {}
            for name in ("lib/libshieldd.a", "include/shieldd.h"):
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(name.encode())
                files[name] = stage.digest(path)
            manifest = {"source_revision": "a" * 40, "target": "test-target", "groups": ["native"], "files": files}
            (root / "manifest.json").write_text(json.dumps(manifest))
            stage.verify(root, "a" * 40, "test-target")
            for revision, target in [("b" * 40, "test-target"), ("a" * 40, "other-target")]:
                with self.assertRaises(ValueError):
                    stage.verify(root, revision, target)
            (root / "lib/libshieldd.a").write_bytes(b"corrupt")
            with self.assertRaises(ValueError):
                stage.verify(root, "a" * 40)
            manifest["files"] = {}
            (root / "manifest.json").write_text(json.dumps(manifest))
            with self.assertRaises(ValueError):
                stage.verify(root, "a" * 40)


if __name__ == "__main__":
    unittest.main()
