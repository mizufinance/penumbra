import hashlib
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
import check_lfs_policy as policy


class LfsPolicyTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.git('init', '-q')
        self.git('config', 'user.email', 'test@example.invalid')
        self.git('config', 'user.name', 'Test')
        self.family = self.root / 'tools/gnark/artifacts/transfer'
        self.family.mkdir(parents=True)
        self.constraints = b'constraint bytes\x00'
        (self.family / 'transfer.sr1cs').write_bytes(self.constraints)
        (self.family / 'proving_key.bin').write_bytes(b'key bytes')
        self.commit()
        self.base = self.git('rev-parse', 'HEAD').strip()
        patch = mock.patch.multiple(policy, ROOT=self.root, FAMILIES=('transfer',))
        patch.start()
        self.addCleanup(patch.stop)

    def git(self, *args):
        return subprocess.check_output(['git', *args], cwd=self.root, text=True)

    def commit(self):
        self.git('add', '.')
        self.git('commit', '-qm', 'fixture')

    def pointer(self, content):
        return (f'version {policy.POINTER_VERSION}\noid sha256:{hashlib.sha256(content).hexdigest()}\nsize {len(content)}\n')

    def test_unchanged_constraints_can_be_stored_as_lfs(self):
        (self.family / 'transfer.sr1cs').write_text(self.pointer(self.constraints))
        self.commit()
        policy.enforce_paired_rotations(self.base)

    def test_changed_constraints_require_key_rotation(self):
        (self.family / 'transfer.sr1cs').write_text(self.pointer(b'different constraints'))
        self.commit()
        with self.assertRaises(SystemExit):
            policy.enforce_paired_rotations(self.base)

    def test_paired_rotation_requires_setup_metadata(self):
        (self.family / 'transfer.sr1cs').write_bytes(b'new constraints')
        (self.family / 'proving_key.bin').write_bytes(b'new key')
        self.commit()
        with self.assertRaises(SystemExit):
            policy.enforce_paired_rotations(self.base)
