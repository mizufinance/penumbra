from __future__ import annotations

import json
import subprocess
import sys
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
ROOT = SCRIPTS.parents[1]
sys.path.insert(0, str(SCRIPTS))

import rust_doc_packages as PACKAGES


def package(
    name: str,
    *,
    features: dict[str, list[str]] | None = None,
    dependencies: list[dict[str, object]] | None = None,
    doc: bool = True,
) -> dict[str, object]:
    return {
        "name": name,
        "features": features or {},
        "dependencies": dependencies or [],
        "targets": [{"doc": doc}],
    }


def dependency(
    name: str,
    *,
    features: list[str] | None = None,
    kind: str | None = None,
    optional: bool = False,
    uses_default_features: bool = True,
) -> dict[str, object]:
    return {
        "name": name,
        "rename": None,
        "features": features or [],
        "kind": kind,
        "optional": optional,
        "uses_default_features": uses_default_features,
    }


class RustDocPackagesTest(unittest.TestCase):
    def test_repository_selection_keeps_proof_libraries_without_proving_roots(
        self,
    ) -> None:
        metadata = json.loads(
            subprocess.check_output(
                ["cargo", "metadata", "--format-version=1", "--no-deps"],
                cwd=ROOT,
                text=True,
            )
        )

        selected = set(PACKAGES.workspace_package_specs(metadata))

        self.assertTrue(
            {"shieldd-sdk-proof-params", "shieldd-sdk-shielded-pool", "pcli"}
            <= selected
        )
        self.assertTrue(
            {
                "bankd-e2e-host-withdrawal-builder",
                "bankd-e2e-spend-builder",
                "shieldd-sdk-app",
                "shieldd-sdk-app-tests",
                "shieldd-sdk-bench",
                "shieldd-sdk-bench-support",
                "shieldd-sdk-mock-client",
            }.isdisjoint(selected)
        )

    def test_excludes_package_with_direct_proving_key_dependency_feature(self) -> None:
        metadata = {
            "packages": [
                package(
                    "proof-params",
                    features={"default": [], "bundled-proving-keys": []},
                ),
                package(
                    "bench",
                    dependencies=[
                        dependency("proof-params", features=["bundled-proving-keys"])
                    ],
                ),
            ]
        }

        self.assertEqual(
            PACKAGES.workspace_package_specs(metadata),
            ["proof-params"],
        )

    def test_excludes_package_whose_default_feature_forwards_proving_keys(self) -> None:
        metadata = {
            "packages": [
                package(
                    "proof-params",
                    features={"default": [], "bundled-proving-keys": []},
                ),
                package(
                    "prover",
                    features={
                        "default": ["runtime"],
                        "runtime": ["proof-params/bundled-proving-keys"],
                    },
                    dependencies=[dependency("proof-params", features=[])],
                ),
            ]
        }

        self.assertEqual(
            PACKAGES.workspace_package_specs(metadata),
            ["proof-params"],
        )

    def test_excludes_transitive_default_activation(self) -> None:
        metadata = {
            "packages": [
                package(
                    "proof-params",
                    features={"default": [], "bundled-proving-keys": []},
                ),
                package(
                    "prover",
                    features={
                        "default": ["bundled-proving-keys"],
                        "bundled-proving-keys": [
                            "proof-params/bundled-proving-keys"
                        ],
                    },
                    dependencies=[dependency("proof-params")],
                ),
                package("api", dependencies=[dependency("prover")]),
            ]
        }

        self.assertEqual(
            PACKAGES.workspace_package_specs(metadata),
            ["proof-params"],
        )

    def test_keeps_package_with_inactive_optional_proving_dependency(self) -> None:
        metadata = {
            "packages": [
                package(
                    "proof-params",
                    features={"default": [], "bundled-proving-keys": []},
                ),
                package(
                    "optional-prover",
                    features={"default": []},
                    dependencies=[
                        dependency(
                            "proof-params",
                            features=["bundled-proving-keys"],
                            optional=True,
                        )
                    ],
                ),
            ]
        }

        self.assertEqual(
            PACKAGES.workspace_package_specs(metadata),
            ["optional-prover", "proof-params"],
        )

    def test_ignores_transitive_development_dependencies(self) -> None:
        metadata = {
            "packages": [
                package(
                    "proof-params",
                    features={"default": [], "bundled-proving-keys": []},
                ),
                package(
                    "library",
                    dependencies=[
                        dependency(
                            "proof-params",
                            features=["bundled-proving-keys"],
                            kind="dev",
                        )
                    ],
                ),
                package("api", dependencies=[dependency("library")]),
            ]
        }

        self.assertEqual(
            PACKAGES.workspace_package_specs(metadata),
            ["api", "proof-params"],
        )

    def test_excludes_package_with_proving_key_development_dependency(self) -> None:
        metadata = {
            "packages": [
                package(
                    "proof-params",
                    features={"default": [], "bundled-proving-keys": []},
                ),
                package(
                    "cli",
                    dependencies=[
                        dependency(
                            "proof-params",
                            features=["bundled-proving-keys"],
                            kind="dev",
                        )
                    ],
                ),
            ]
        }

        self.assertEqual(
            PACKAGES.workspace_package_specs(metadata),
            ["proof-params"],
        )


if __name__ == "__main__":
    unittest.main()
