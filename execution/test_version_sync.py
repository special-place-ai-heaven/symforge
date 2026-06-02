from __future__ import annotations

import json
import unittest
import uuid
from pathlib import Path

import version_sync


class VersionSyncTests(unittest.TestCase):
    def make_repo(self) -> Path:
        temp_root = Path(__file__).resolve().parent.parent / ".tmp" / "execution-tests"
        temp_root.mkdir(parents=True, exist_ok=True)
        root = temp_root / f"repo-{uuid.uuid4().hex}"
        root.mkdir()
        (root / ".github").mkdir()
        (root / "npm").mkdir()
        (root / ".github" / ".release-please-manifest.json").write_text(
            '{\n  ".": "0.3.12"\n}\n',
            encoding="utf-8",
        )
        (root / "Cargo.toml").write_text(
            '[package]\nname = "symforge"\nversion = "0.3.12"\n\n'
            '[dependencies]\nserde = { version = "1.0", features = ["derive"] }\n',
            encoding="utf-8",
        )
        (root / "npm" / "package.json").write_text(
            json.dumps({"name": "symforge", "version": "0.3.12"}, indent=2) + "\n",
            encoding="utf-8",
        )
        return root

    def test_check_versions_accepts_aligned_state(self) -> None:
        root = self.make_repo()
        self.assertEqual(version_sync.check_versions(root, tag="v0.3.12"), [])

    def test_check_versions_reports_manifest_and_tag_drift(self) -> None:
        root = self.make_repo()
        (root / "npm" / "package.json").write_text(
            json.dumps({"name": "symforge", "version": "0.3.11"}, indent=2) + "\n",
            encoding="utf-8",
        )

        problems = version_sync.check_versions(root, tag="v0.3.10")

        self.assertIn(
            "npm version '0.3.11' does not match release manifest '0.3.12'.",
            problems,
        )
        self.assertIn(
            "tag version '0.3.10' does not match release manifest '0.3.12'.",
            problems,
        )

    def test_check_versions_reports_optional_dependency_pin_drift(self) -> None:
        root = self.make_repo()
        (root / "npm" / "package.json").write_text(
            json.dumps(
                {
                    "name": "symforge",
                    "version": "0.3.12",
                    "optionalDependencies": {
                        "symforge-linux-x64": "0.3.12",
                        "symforge-windows-x64": "0.3.11",
                    },
                },
                indent=2,
            )
            + "\n",
            encoding="utf-8",
        )

        problems = version_sync.check_versions(root)

        self.assertIn(
            "npm optionalDependencies 'symforge-windows-x64' pin '0.3.11' does not match "
            "release manifest '0.3.12'.",
            problems,
        )
        # The aligned pin must not be reported.
        self.assertFalse(
            any("symforge-linux-x64" in problem for problem in problems),
            problems,
        )

    def test_check_versions_rejects_noncanonical_tag_shape(self) -> None:
        root = self.make_repo()

        problems = version_sync.check_versions(root, tag="symforge-v0.3.12")

        self.assertIn(
            "canonical release tags must use plain vX.Y.Z format, got 'symforge-v0.3.12'.",
            problems,
        )

    def test_set_version_updates_manifest_and_publishable_packages(self) -> None:
        root = self.make_repo()

        changed = version_sync.set_version(root, "0.3.13")

        self.assertEqual(
            {path.relative_to(root).as_posix() for path in changed},
            {
                ".github/.release-please-manifest.json",
                "Cargo.toml",
                "npm/package.json",
            },
        )
        versions = version_sync.collect_versions(root)
        self.assertEqual(versions["manifest"], "0.3.13")
        self.assertEqual(versions["cargo"], "0.3.13")
        self.assertEqual(versions["npm"], "0.3.13")

    def test_set_version_rejects_invalid_semver(self) -> None:
        root = self.make_repo()

        with self.assertRaises(version_sync.VersionSyncError):
            version_sync.set_version(root, "next-release")

    def test_repo_cargo_and_npm_versions_agree(self) -> None:
        """Cross-check against the real repo files version_sync syncs.

        Reads the actual Cargo.toml and npm/package.json at the repo root
        and asserts their versions match. A drift between these two at
        release time is a ship-blocker, and this test catches it directly
        against the files on disk rather than synthetic fixtures.
        """
        root = version_sync.repo_root()
        cargo_version = version_sync.read_cargo_version(root)
        npm_version = version_sync.read_npm_version(root)
        self.assertEqual(
            cargo_version,
            npm_version,
            msg=(
                f"Cargo.toml version '{cargo_version}' does not match "
                f"npm/package.json version '{npm_version}'. "
                "A drift between these is a ship-blocker."
            ),
        )


if __name__ == "__main__":
    unittest.main()
