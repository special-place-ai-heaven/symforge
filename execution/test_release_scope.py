"""Which commits can never make release-please cut a release."""

from __future__ import annotations

import unittest

import release_scope


class NeverReleases(unittest.TestCase):
    def test_never_releasing_types(self) -> None:
        for kind in release_scope.NEVER_RELEASING_TYPES:
            with self.subTest(kind=kind):
                self.assertTrue(release_scope.message_never_releases(f"{kind}: a"))
                self.assertTrue(release_scope.message_never_releases(f"{kind}(scope): a\n\nplain body text"))
        self.assertEqual(
            set(release_scope.NEVER_RELEASING_TYPES),
            {"ci", "docs", "chore", "test", "style", "build", "refactor"},
        )

    def test_releasable_or_unknown_types(self) -> None:
        for subject in ("fix: a", "feat(x): a", "perf: a", "revert: a", "deps: a", "wip: a", "feature: a"):
            with self.subTest(subject=subject):
                self.assertFalse(release_scope.message_never_releases(subject))

    def test_breaking_markers_and_release_as(self) -> None:
        for message in (
            "chore!: a",
            "chore(x)!: a",
            "chore: a\n\nBREAKING CHANGE: b",
            "chore: a\n\nBREAKING-CHANGE: b",
            "chore: a\n\nbreaking change: b",
            "chore: a\n\nRelease-As: 2.0.0",
            "chore: a\n\nrelease-as: 2.0.0",
        ):
            with self.subTest(message=message):
                self.assertFalse(release_scope.message_never_releases(message))

    def test_body_lines_parsed_as_commits(self) -> None:
        self.assertFalse(release_scope.message_never_releases("chore: a\n\nfix(x): b"))
        self.assertFalse(release_scope.message_never_releases("chore: a\n\n  feat: b"))
        self.assertTrue(release_scope.message_never_releases("chore: a\n\ndocs: b"))
        self.assertFalse(release_scope.message_never_releases("chore: a\n\ndocs!: b"))

    def test_unparsable_subjects(self) -> None:
        for subject in ("", "Update readme", 'Revert "ci: a"', "ci:a", "ci(): a", "CI: a", "ci: ", "Merge pull request #1"):
            with self.subTest(subject=subject):
                self.assertFalse(release_scope.message_never_releases(subject))


if __name__ == "__main__":
    unittest.main()
