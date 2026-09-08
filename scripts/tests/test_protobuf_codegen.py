import pathlib
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]


class ProtobufEntrypointTests(unittest.TestCase):
    def test_entrypoint_resolves_its_source_outside_the_working_tree(self):
        with tempfile.TemporaryDirectory() as directory:
            result = subprocess.run(
                [str(ROOT / "deployments/scripts/protobuf-codegen"), "--help"],
                cwd=directory,
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("compare without changing tracked files", result.stdout)
