import unittest
from unittest import mock

import release_ops


class ReleaseOpsTests(unittest.TestCase):
    def test_normalize_release_tag_adds_prefix(self) -> None:
        self.assertEqual(release_ops.normalize_release_tag("0.3.12"), "v0.3.12")

    def test_normalize_release_tag_preserves_prefix(self) -> None:
        self.assertEqual(release_ops.normalize_release_tag("v0.3.12"), "v0.3.12")

    def test_normalize_release_tag_rejects_noncanonical_shape(self) -> None:
        with self.assertRaises(release_ops.ReleaseOpsError):
            release_ops.normalize_release_tag("symforge-v0.3.12")

    def test_normalize_release_tag_rejects_blank_input(self) -> None:
        with self.assertRaises(release_ops.ReleaseOpsError):
            release_ops.normalize_release_tag("   ")

    def test_guide_text_mentions_canonical_commands(self) -> None:
        text = release_ops.guide_text()
        self.assertIn("python execution/release_ops.py preflight", text)
        self.assertIn("python execution/release_ops.py push-main", text)
        self.assertIn("python execution/release_ops.py rebuild --tag v0.3.12", text)
        self.assertIn("GitHub Actions workflow permissions", text)

    def test_parse_github_repo_slug_supports_https_and_ssh(self) -> None:
        self.assertEqual(
            release_ops.parse_github_repo_slug(
                "https://github.com/special-place-administrator/symforge.git"
            ),
            "special-place-administrator/symforge",
        )
        self.assertEqual(
            release_ops.parse_github_repo_slug(
                "git@github.com:special-place-administrator/symforge.git"
            ),
            "special-place-administrator/symforge",
        )

    def test_resolve_executable_prefers_shutil_lookup(self) -> None:
        with mock.patch("release_ops.shutil.which", return_value="C:/Tools/npm.cmd"):
            self.assertEqual(release_ops.resolve_executable("npm"), "C:/Tools/npm.cmd")

    def test_recommended_next_steps_dirty_tree_blocks_push(self) -> None:
        steps = release_ops.recommended_next_steps("main", clean=False)
        self.assertTrue(any("dirty" in step for step in steps))

    def test_preflight_steps_include_version_sync(self) -> None:
        root = release_ops.repo_root()
        rendered = [" ".join(args) for _, args, _ in release_ops.preflight_steps(root)]
        self.assertTrue(any("version_sync.py check" in command for command in rendered))

    def test_npm_version_exists_returns_true_for_matching_registry_version(self) -> None:
        root = release_ops.repo_root()
        completed = mock.Mock(returncode=0, stdout="4.9.8\n")
        with mock.patch("release_ops.subprocess.run", return_value=completed):
            self.assertTrue(release_ops.npm_version_exists(root, "symforge", "4.9.8"))

    def test_publish_npm_tarball_skips_when_version_already_exists(self) -> None:
        root = release_ops.repo_root()
        with mock.patch("release_ops.npm_version_exists", return_value=True):
            with mock.patch("release_ops.run_checked") as run_checked:
                result = release_ops.publish_npm_tarball(
                    root,
                    "./dist/symforge-4.9.8.tgz",
                    package_name="symforge",
                    version="4.9.8",
                )

        self.assertEqual(result, "skipped")
        run_checked.assert_not_called()

    def test_publish_npm_tarball_runs_publish_when_version_missing(self) -> None:
        root = release_ops.repo_root()
        with mock.patch("release_ops.npm_version_exists", return_value=False):
            with mock.patch("release_ops.run_checked") as run_checked:
                result = release_ops.publish_npm_tarball(
                    root,
                    "./dist/symforge-4.9.8.tgz",
                    package_name="symforge",
                    version="4.9.8",
                )

        self.assertEqual(result, "published")
        run_checked.assert_called_once_with(
            ["npm", "publish", "./dist/symforge-4.9.8.tgz", "--access", "public"],
            cwd=root,
        )

    def test_release_workflow_publishes_platform_npm_packages_before_root(self) -> None:
        root = release_ops.repo_root()
        workflow = (root / ".github" / "workflows" / "release.yml").read_text(
            encoding="utf-8"
        )

        for package_name in (
            "symforge-windows-x64",
            "symforge-linux-x64",
            "symforge-macos-x64",
            "symforge-macos-arm64",
        ):
            self.assertIn(package_name, workflow)
        self.assertIn("Download built binary artifacts", workflow)
        self.assertIn("Publish platform packages to npm", workflow)
        self.assertIn("Publish root package to npm", workflow)


if __name__ == "__main__":
    unittest.main()
