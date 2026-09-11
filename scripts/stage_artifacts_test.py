"""Production staging rejects development and missing build provenance."""
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("stage", os.environ.get("STAGE_SCRIPT", str(Path(__file__).with_name("stage_artifacts.py"))))
stage = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stage)

class StagingTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        files = {}
        for name in ["include/shieldd.h", "lib/libshieldd.a"]:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"fixture")
            files[name] = stage.digest(path)
        self.record = {"profile": "release", "debug_assertions": False,
                       "proof_parameters": {"approved": True, "debug_assertions": False}}
        self.manifest = {"source_revision": "a" * 40, "target": "test", "groups": ["native"],
                         "files": files, "provenance": {"native": self.record}}
    def write(self):
        (self.root / "manifest.json").write_text(json.dumps(self.manifest))
    def reject(self):
        self.write()
        with self.assertRaises(ValueError): stage.verify(self.root, "a" * 40, "test")
    def test_production_accepts_complete_provenance(self):
        self.write()
        stage.verify(self.root, "a" * 40, "test")
    def test_development_rejected_by_production(self):
        self.record["profile"] = "dev"
        self.record["debug_assertions"] = True
        self.record["proof_parameters"] = {"approved": False, "debug_assertions": True}
        self.reject()
    def test_missing_provenance_rejected(self):
        del self.manifest["provenance"]
        self.reject()
    def test_release_label_cannot_hide_debug_or_unapproved_keys(self):
        for field in ["approved", "debug_assertions"]:
            with self.subTest(field=field):
                self.record["proof_parameters"] = {"approved": True, "debug_assertions": False}
                self.record["proof_parameters"][field] = field != "approved"
                self.reject()
    def test_explicit_development_verification(self):
        self.record.update(profile="dev", debug_assertions=True)
        self.record["proof_parameters"] = {"approved": False, "debug_assertions": True}
        self.write()
        stage.verify(self.root, "a" * 40, "test", profile="dev")

if __name__ == "__main__": unittest.main()
