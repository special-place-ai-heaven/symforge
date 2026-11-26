#!/usr/bin/env python
"""Decide whether a range of commits can never make release-please cut a release.

The release workflow skips its verification build only for a push that can neither
publish nor open a release PR. A push cannot open a release PR when every commit
since the last release tag is of a type release-please never releases for. This is
decided conservatively: anything that does not clearly parse as one of those types
counts as releasable, and so does any breaking marker, Release-As footer, merge
commit, or body line release-please could parse as a commit of another type.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

NEVER_RELEASING_TYPES = ("build", "chore", "ci", "docs", "refactor", "style", "test")
SUBJECT_RE = re.compile(r"(?P<type>[a-z]+)(?:\([^()\r\n]+\))?(?P<bang>!)?: \S")
BODY_HEADER_RE = re.compile(r"(?P<type>[A-Za-z][A-Za-z0-9_-]*)(?:\([^)\r\n]*\))?(?P<bang>!)?:\s")
RELEASING_FOOTER_RE = re.compile(r"breaking[ -]change|release-as", re.IGNORECASE)
COMMIT_RE = re.compile(r"[0-9a-f]{40}")


def message_never_releases(message: str) -> bool:
    lines = message.replace("\r\n", "\n").split("\n")
    subject = SUBJECT_RE.match(lines[0])
    if subject is None or subject.group("type") not in NEVER_RELEASING_TYPES or subject.group("bang"):
        return False
    if RELEASING_FOOTER_RE.search(message):
        return False
    for line in lines[1:]:
        header = BODY_HEADER_RE.match(line.strip())
        if header is None:
            continue
        if header.group("type").lower() not in NEVER_RELEASING_TYPES or header.group("bang"):
            return False
    return True


def range_never_releases(root: Path, base: str, head: str) -> bool:
    if not COMMIT_RE.fullmatch(base) or not COMMIT_RE.fullmatch(head) or base == head:
        return False
    result = subprocess.run(
        ["git", "log", "--format=%H%x00%P%x00%B%x1e", f"{base}..{head}"],
        cwd=root,
        stdin=subprocess.DEVNULL,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        return False
    try:
        output = result.stdout.decode("utf-8")
    except UnicodeDecodeError:
        return False
    records = [record.strip("\n") for record in output.split("\x1e") if record.strip("\n")]
    if not records:
        return False
    for record in records:
        fields = record.split("\x00")
        if len(fields) != 3 or not COMMIT_RE.fullmatch(fields[0]):
            return False
        if len(fields[1].split()) != 1:
            return False
        if not message_never_releases(fields[2]):
            return False
    return True


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Exit 0 only if no commit in base..head can cause a release.")
    parser.add_argument("--base", required=True)
    parser.add_argument("--head", required=True)
    parser.add_argument("--root", default=".")
    args = parser.parse_args(argv)
    if range_never_releases(Path(args.root), args.base, args.head):
        print(f"No commit in {args.base}..{args.head} can cause a release.")
        return 0
    print(f"A commit in {args.base}..{args.head} may cause a release.")
    return 1


if __name__ == "__main__":
    sys.exit(main())
