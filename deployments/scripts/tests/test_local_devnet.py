import os
from pathlib import Path
import subprocess
import sys
import tempfile
import tomllib
import unittest


ROOT = Path(__file__).resolve().parents[3]


class LocalDevnetTest(unittest.TestCase):
    def test_base_devnet_keeps_local_indexing(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            pd = root / "pd"
            pd.write_text(
                f"#!{sys.executable}\n"
                "import pathlib, sys\n"
                "network = pathlib.Path(sys.argv[sys.argv.index('--network-dir') + 1])\n"
                "config = network / 'node0/cometbft/config/config.toml'\n"
                "config.parent.mkdir(parents=True)\n"
                "config.write_text('[tx_index]\\nindexer = \"kv\"\\n')\n"
            )
            compose = root / "process-compose"
            compose.write_text("#!/bin/sh\nexit 0\n")
            pd.chmod(0o755)
            compose.chmod(0o755)
            environment = {
                **os.environ,
                "PATH": f"{root}:{os.environ['PATH']}",
                "SHIELDD_PD_BIN": str(pd),
                "SHIELDD_DEVNET_HOME": str(root / "state"),
                "COMPLIANCE_TMP": str(root / "compliance"),
                "SHIELDD_PD_INTEGRATION_DEV_SRS": "0",
                "SHIELDD_POSTGRES_PORT": "15432",
            }
            subprocess.run(
                ["bash", "deployments/scripts/run-local-devnet.sh", "--no-server"],
                cwd=ROOT,
                env=environment,
                check=True,
            )
            config = root / "state/network_data/node0/cometbft/config/config.toml"
            self.assertEqual(tomllib.loads(config.read_text())["tx_index"]["indexer"], "kv")
            pg_ctl = root / "pg_ctl"
            pg_ctl.write_text("#!/bin/sh\nexit 0\n")
            pg_ctl.chmod(0o755)
            for _ in range(2):
                subprocess.run(
                    ["bash", "deployments/scripts/prep-postgres-env"],
                    cwd=ROOT,
                    env=environment,
                    check=True,
                )
                index = tomllib.loads(config.read_text())["tx_index"]
                self.assertEqual(index["indexer"], "psql")
                self.assertIn("127.0.0.1:15432/", index["psql-conn"])
