#!/usr/bin/env python
"""Prove that a release-please skip was not a silent drop of feat/fix commits.

release-please skipping is silent. This repo now fails the prepare-release job
when user-facing commits since the last *completed* release produced no PR.

A merged release PR with no git tag yet is not that failure. With
`skip-github-release: true`, release-please opens the version PR, then aborts
with "untagged, merged release PRs outstanding" until this workflow creates
the tag. Counting feat/fix since the *previous* tag on that path blocks
tagging. That is how v10.2.0 never landed after #561 merged.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Sequence

USER_FACING_SUBJECT = re.compile(r"^(feat|fix)(\([^)]*\))?!?: ")
RELEASE_SUBJECT = re.compile(
    r"^chore\([^)]*\): release (\d+\.\d+\.\d+)(?:\s+\(#\d+\))?\s*$"
)
MANIFEST_RELATIVE = Path(".github") / ".release-please-manifest.json"


class ProveError(RuntimeError):
    """Raised when the skip cannot be proven safe."""


def repo_root(path: str | None = None) -> Path:
    if path is not None:
        return Path(path).resolve()
    return Path(__file__).resolve().parent.parent


def run_git(root: Path, args: Sequence[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args],
        cwd=root,
        check=False,
        capture_output=True,
        text=True,
    )


def git_text(root: Path, args: Sequence[str]) -> str:
    result = run_git(root, args)
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip() or "git command failed"
        raise ProveError(detail)
    return result.stdout


def list_v_tags(root: Path) -> list[str]:
    result = run_git(root, ["tag", "-l", "v*"])
    if result.returncode != 0:
        raise ProveError(result.stderr.strip() or "git tag failed")
    return [line.strip() for line in result.stdout.splitlines() if line.strip()]


def tag_exists(root: Path, tag: str) -> bool:
    result = run_git(root, ["rev-parse", "-q", "--verify", f"refs/tags/{tag}"])
    return result.returncode == 0


def commit_subject(root: Path, rev: str) -> str:
    return git_text(root, ["log", "-1", "--format=%s", rev]).strip()


def has_second_parent(root: Path, rev: str) -> bool:
    result = run_git(root, ["rev-parse", "-q", "--verify", f"{rev}^2"])
    return result.returncode == 0


def release_subject_version(subject: str) -> str | None:
    match = RELEASE_SUBJECT.match(subject.strip())
    if match is None:
        return None
    return match.group(1)


def is_release_head(root: Path, version: str, head: str = "HEAD") -> bool:
    if release_subject_version(commit_subject(root, head)) == version:
        return True
    if not has_second_parent(root, head):
        return False
    return release_subject_version(commit_subject(root, f"{head}^2")) == version


def find_release_commit(root: Path, version: str) -> str | None:
    log = git_text(root, ["log", "--format=%H\t%s"])
    for line in log.splitlines():
        sha, separator, subject = line.partition("\t")
        if not separator:
            continue
        if release_subject_version(subject) == version:
            return sha
    return None


def find_release_tag_commit(root: Path, version: str, head: str = "HEAD") -> str | None:
    """SHA a GitHub release tag should point at: the merge onto main, else the chore."""
    chore = find_release_commit(root, version)
    if chore is None:
        return None
    first_parent = git_text(root, ["log", "--first-parent", "--format=%H", head])
    for sha in (line.strip() for line in first_parent.splitlines()):
        if not sha:
            continue
        if sha == chore:
            return chore
        if has_second_parent(root, sha):
            second = git_text(root, ["rev-parse", f"{sha}^2"]).strip()
            if second == chore:
                return sha
    return chore


def manifest_version(root: Path) -> str:
    path = root / MANIFEST_RELATIVE
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError, UnicodeDecodeError) as error:
        raise ProveError("release manifest is unavailable or not JSON") from error
    value = payload.get(".") if isinstance(payload, dict) else None
    if not isinstance(value, str) or not value:
        raise ProveError("release manifest version is unavailable")
    return value


def owed_subjects(root: Path, since: str, head: str = "HEAD") -> list[str]:
    log = git_text(root, ["log", "--no-merges", "--format=%s", f"{since}..{head}"])
    return [
        subject
        for subject in (line.strip() for line in log.splitlines())
        if subject and USER_FACING_SUBJECT.match(subject)
    ]


def owed_lines(root: Path, since: str, head: str = "HEAD") -> list[str]:
    log = git_text(root, ["log", "--no-merges", "--format=%h %s", f"{since}..{head}"])
    lines = []
    for line in log.splitlines():
        stripped = line.strip()
        if not stripped:
            continue
        _, separator, subject = stripped.partition(" ")
        if separator and USER_FACING_SUBJECT.match(subject):
            lines.append(stripped)
    return lines


def prove(root: Path, head: str = "HEAD") -> str:
    """Return a success message, or raise ProveError if a skip is not proven."""
    if not list_v_tags(root):
        return "No release tag yet; nothing to prove."

    version = manifest_version(root)
    expected_tag = f"v{version}"
    if not tag_exists(root, expected_tag):
        if is_release_head(root, version, head):
            return (
                f"Manifest is {version} with no tag {expected_tag}; "
                "HEAD is that release commit. Tagging is the next step."
            )
        release_sha = find_release_tag_commit(root, version, head)
        where = f" at {release_sha}" if release_sha is not None else ""
        raise ProveError(
            f"Manifest is {version} with no tag {expected_tag}, but HEAD is not "
            f"that release commit.\n"
            f"Create GitHub release {expected_tag}{where} "
            f"(chore(main): release {version}) and label the merged release PR "
            "autorelease: tagged.\n"
            "Do not tag HEAD; it has moved past the untagged release."
        )

    owed = owed_subjects(root, expected_tag, head)
    if owed:
        listed = "\n".join(f"  {line}" for line in owed_lines(root, expected_tag, head))
        raise ProveError(
            f"{len(owed)} user-facing commit(s) since {expected_tag} produced no "
            "release PR.\n"
            "release-please parsed them away silently. Inspect the step log above "
            "for 'commit could not be parsed' before assuming there was nothing "
            "to release.\n"
            f"{listed}"
        )
    return f"No user-facing commits since {expected_tag}; skipping is correct."


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Fail when a release-please skip dropped feat/fix commits, but allow "
            "the untagged merged release-PR tagging path."
        )
    )
    parser.add_argument(
        "--root",
        default=None,
        help="Repository root. Defaults to the current repository.",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    root = repo_root(args.root)
    try:
        message = prove(root)
    except ProveError as error:
        for line in str(error).splitlines():
            print(f"::error::{line}", file=sys.stderr)
            print(line, file=sys.stderr)
        return 1
    print(message)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
