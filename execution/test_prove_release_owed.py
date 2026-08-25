"""The 10.2.0 tagging stall: prove must not treat an untagged release merge as a skip."""

from __future__ import annotations

import json
import subprocess
import tempfile
import unittest
from pathlib import Path

import prove_release_owed


def _run(root: Path, args: list[str]) -> None:
    subprocess.run(args, cwd=root, check=True, capture_output=True, text=True)


def _git(root: Path, *args: str) -> None:
    _run(root, ["git", *args])


def _init_repo(root: Path) -> None:
    _git(root, "init", "-b", "main")
    _git(root, "config", "user.email", "prove@example.test")
    _git(root, "config", "user.name", "Prove Test")
    _git(root, "config", "commit.gpgsign", "false")


def _write_manifest(root: Path, version: str) -> None:
    path = root / ".github" / ".release-please-manifest.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps({".": version}, indent=2) + "\n", encoding="utf-8")


def _commit(root: Path, message: str, filename: str, content: str) -> None:
    (root / filename).write_text(content, encoding="utf-8")
    _git(root, "add", filename)
    _git(root, "commit", "-m", message)


class ProveReleaseOwedTests(unittest.TestCase):
    def test_release_pr_merged_ahead_of_head_is_the_tagging_path(self) -> None:
        # Observed 2026-08-25 (run 32891474365): an older push run was still
        # queued when release-please, reading origin/main, opened and
        # auto-merged the release PR for the newer version. When the older
        # run finally executed, its HEAD did not contain that merge, so the
        # gate saw owed commits and no release PR and went red -- while the
        # release PR's own push run was already tagging.
        with tempfile.TemporaryDirectory(prefix="prove-release-") as temp:
            root = Path(temp)
            _init_repo(root)
            _write_manifest(root, "1.0.0")
            _git(root, "add", ".github")
            _commit(root, "chore(main): release 1.0.0", "changelog.txt", "1.0.0\n")
            _git(root, "tag", "v1.0.0")
            _commit(root, "fix: owed by an older push", "a.txt", "a\n")
            older_head = subprocess.run(
                ["git", "rev-parse", "HEAD"], cwd=root, check=True,
                capture_output=True, text=True,
            ).stdout.strip()
            _write_manifest(root, "1.0.1")
            _git(root, "add", ".github")
            _commit(root, "chore(main): release 1.0.1", "changelog.txt", "1.0.1\n")
            _git(root, "update-ref", "refs/remotes/origin/main", "HEAD")
            _git(root, "checkout", "--quiet", older_head)

            message = prove_release_owed.prove(root)

            self.assertIn("1.0.1", message)
            self.assertIn("ahead", message)

    def test_no_tags_is_vacuous_success(self) -> None:
        with tempfile.TemporaryDirectory(prefix="prove-release-") as temp:
            root = Path(temp)
            _init_repo(root)
            _write_manifest(root, "1.0.0")
            _commit(root, "feat: first", "a.txt", "a\n")
            self.assertEqual(
                prove_release_owed.prove(root),
                "No release tag yet; nothing to prove.",
            )

    def test_untagged_release_merge_is_tagging_not_a_silent_skip(self) -> None:
        """The 10.2.0 shape: last tag is 1.0.0, manifest is 1.1.0, HEAD is the merge."""
        with tempfile.TemporaryDirectory(prefix="prove-release-") as temp:
            root = Path(temp)
            _init_repo(root)
            _write_manifest(root, "1.0.0")
            _commit(root, "feat: initial", "a.txt", "a\n")
            _git(root, "tag", "v1.0.0")
            _commit(root, "feat(feature-020): Slice 1", "b.txt", "b\n")
            _git(root, "checkout", "-b", "release-please")
            _write_manifest(root, "1.1.0")
            _commit(root, "chore(main): release 1.1.0", "changelog.txt", "1.1.0\n")
            _git(root, "checkout", "main")
            _git(
                root,
                "merge",
                "--no-ff",
                "-m",
                "Merge pull request #561 from org/release-please",
                "release-please",
            )

            message = prove_release_owed.prove(root)
            self.assertIn("Tagging is the next step", message)
            self.assertIn("1.1.0", message)

    def test_head_past_untagged_release_refuses_to_tag_head(self) -> None:
        with tempfile.TemporaryDirectory(prefix="prove-release-") as temp:
            root = Path(temp)
            _init_repo(root)
            _write_manifest(root, "1.0.0")
            _commit(root, "feat: initial", "a.txt", "a\n")
            _git(root, "tag", "v1.0.0")
            _commit(root, "feat: owed", "b.txt", "b\n")
            _git(root, "checkout", "-b", "release-please")
            _write_manifest(root, "1.1.0")
            _commit(root, "chore(main): release 1.1.0", "changelog.txt", "1.1.0\n")
            _git(root, "checkout", "main")
            _git(
                root,
                "merge",
                "--no-ff",
                "-m",
                "Merge pull request #561 from org/release-please",
                "release-please",
            )
            merge_sha = subprocess.run(
                ["git", "rev-parse", "HEAD"],
                cwd=root,
                check=True,
                capture_output=True,
                text=True,
            ).stdout.strip()
            _commit(root, "feat(knowledge): later", "c.txt", "c\n")

            with self.assertRaises(prove_release_owed.ProveError) as raised:
                prove_release_owed.prove(root)
            text = str(raised.exception)
            self.assertIn("Do not tag HEAD", text)
            self.assertIn(merge_sha, text)
            self.assertIn("v1.1.0", text)

    def test_feat_since_manifest_tag_is_owed(self) -> None:
        with tempfile.TemporaryDirectory(prefix="prove-release-") as temp:
            root = Path(temp)
            _init_repo(root)
            _write_manifest(root, "1.1.0")
            _commit(root, "chore(main): release 1.1.0", "a.txt", "a\n")
            _git(root, "tag", "v1.1.0")
            _commit(root, "feat(knowledge): answer-first", "b.txt", "b\n")

            with self.assertRaises(prove_release_owed.ProveError) as raised:
                prove_release_owed.prove(root)
            text = str(raised.exception)
            self.assertIn("1 user-facing commit", text)
            self.assertIn("v1.1.0", text)
            self.assertIn("feat(knowledge): answer-first", text)

    def test_docs_since_manifest_tag_is_not_owed(self) -> None:
        with tempfile.TemporaryDirectory(prefix="prove-release-") as temp:
            root = Path(temp)
            _init_repo(root)
            _write_manifest(root, "1.1.0")
            _commit(root, "chore(main): release 1.1.0", "a.txt", "a\n")
            _git(root, "tag", "v1.1.0")
            _commit(root, "docs: note", "b.txt", "b\n")
            self.assertIn(
                "skipping is correct",
                prove_release_owed.prove(root),
            )


if __name__ == "__main__":
    unittest.main()
