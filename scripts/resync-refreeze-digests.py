#!/usr/bin/env python3
"""Resync the Feature 020 V11 manifest and attestation digests to HEAD.

The manifest pins every corpus file by raw-bytes hash and every amendment clause
by a hash over an exact line range, and the attestation pins the manifest. Any
edit inside the corpus therefore invalidates a known, computable set of hashes.
Recomputing them by hand is how a digest ends up describing a file that no
longer exists in that form, so this reads them from the committed tree.

    python scripts/resync-refreeze-digests.py            # report only
    python scripts/resync-refreeze-digests.py --write    # rewrite the hashes

It reports every hash it changes, and it refuses to touch anything else: it
rewrites exact 64-hex strings in place, so the manifest's formatting, ordering
and content are untouched. Baseline clauses are recomputed against the baseline
commit the manifest names, target clauses against HEAD.

Run `verify-internal` afterwards. This tool makes the digests agree with the
tree; only that verification proves the whole chain does.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

MANIFEST = Path("specs/020-repository-knowledge-index/REFREEZE-MANIFEST-v11.md")
START = "<!-- SYMFORGE FEATURE020 REFREEZE V11 JSON START -->"
END = "<!-- SYMFORGE FEATURE020 REFREEZE V11 JSON END -->"


def blob(root: Path, commit: str, path: str) -> bytes | None:
    result = subprocess.run(
        ["git", "cat-file", "blob", f"{commit}:{path}"],
        cwd=root,
        capture_output=True,
    )
    return result.stdout if result.returncode == 0 else None


def clause_bytes(content: bytes, start_line: int, end_line: int) -> bytes | None:
    lines = content.splitlines(keepends=True)
    if start_line < 1 or end_line < start_line or end_line > len(lines):
        return None
    return b"".join(lines[start_line - 1 : end_line])


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=".")
    parser.add_argument("--write", action="store_true", help="Apply the changes.")
    arguments = parser.parse_args()
    root = Path(arguments.root).resolve()

    manifest_path = root / MANIFEST
    text = manifest_path.read_text(encoding="utf-8")
    body = text[text.index(START) + len(START) : text.index(END)]
    body = body.strip().removeprefix("```json").strip().removesuffix("```").strip()
    manifest = json.loads(body)

    baseline = manifest["baseline"]["commit"]
    replacements: list[tuple[str, str, str]] = []

    for item in manifest["inventory"]:
        if item["sha256"] is None or item["path"] == manifest["self_path"]:
            continue
        content = blob(root, "HEAD", item["path"])
        if content is None:
            print(f"MISSING at HEAD: {item['path']}", file=sys.stderr)
            return 1
        actual = hashlib.sha256(content).hexdigest()
        if actual != item["sha256"]:
            replacements.append((item["sha256"], actual, f"inventory {item['path']}"))

    for amendment in manifest["amendments"]:
        for kind, commit in (("replaced", baseline), ("replacements", "HEAD")):
            for clause in amendment[kind]:
                content = blob(root, commit, clause["path"])
                if content is None:
                    print(f"MISSING at {commit}: {clause['path']}", file=sys.stderr)
                    return 1
                selected = clause_bytes(content, clause["start_line"], clause["end_line"])
                if selected is None:
                    print(
                        f"LINE RANGE OUT OF BOUNDS: {clause['clause_id']} "
                        f"{clause['path']}:{clause['start_line']}-{clause['end_line']}",
                        file=sys.stderr,
                    )
                    return 1
                actual = hashlib.sha256(selected).hexdigest()
                if actual != clause["sha256"]:
                    replacements.append((clause["sha256"], actual, clause["clause_id"]))

    if not replacements:
        print("Manifest digests already agree with the tree.")
    for old, new, label in replacements:
        occurrences = text.count(old)
        if occurrences != 1:
            print(
                f"REFUSED: {label} hash appears {occurrences} times in the manifest",
                file=sys.stderr,
            )
            return 1
        print(f"  {label}\n    {old}\n -> {new}")
        text = text.replace(old, new, 1)

    # The amendment set digest covers every amendment INCLUDING its clause
    # hashes, so it moves whenever a clause does. Computed by the verifier's own
    # code rather than reimplemented here: a second implementation of a digest is
    # a second thing that can disagree with the check it is meant to satisfy.
    sys.path.insert(0, str(root / "execution"))
    import refreeze_v11 as verifier  # noqa: PLC0415

    git_objects = verifier.GitObjects(root, git_executable=None)
    head = git_objects.resolve_commit("HEAD")
    committed = verifier._parse_sentinel_json(
        git_objects.read_blob(head, verifier.MANIFEST_PATH),
        verifier.MANIFEST_START,
        verifier.MANIFEST_END,
    )
    if not replacements:
        _, amendment_digest = verifier._validate_amendments(
            git_objects,
            committed["amendments"],
            baseline_commit=committed["baseline"]["commit"],
            target_commit=head,
        )
        if amendment_digest != manifest["amendment_set_id"]:
            print(
                f"\n  amendment_set_id\n    {manifest['amendment_set_id']}\n"
                f" -> {amendment_digest}"
            )
            if arguments.write:
                text = text.replace(manifest["amendment_set_id"], amendment_digest, 1)
                manifest_path.write_text(text, encoding="utf-8", newline="")
                print("Rewrote the amendment set digest. Commit it, then run this again.")
                return 0

    if replacements and arguments.write:
        manifest_path.write_text(text, encoding="utf-8", newline="")
        print(f"\nRewrote {len(replacements)} hash(es) in {MANIFEST}.")
        print("Commit it, then run this again for the amendment set digest.")
        return 0

    # The attestation pins the manifest, so it can only be repinned once the
    # manifest itself is committed. Reporting that rather than writing a hash
    # for a manifest state that is not in the tree yet.
    # The attestation carries its own copy of the manifest's bindings and the
    # verifier requires them to be equal, so a regenerated amendment set digest
    # has to reach both documents.
    attestation_path = root / manifest["detached_attestation_path"]
    attestation_raw = attestation_path.read_text(encoding="utf-8")
    for field in ("amendment_set_id",):
        expected = committed[field]
        current = re.search(
            rf'"{field}":\s*"([0-9a-f]{{64}})"', attestation_raw
        )
        if current is not None and current.group(1) != expected:
            print(f"\n  attestation {field}\n    {current.group(1)}\n -> {expected}")
            if arguments.write:
                attestation_raw = attestation_raw.replace(current.group(1), expected, 1)
                attestation_path.write_text(attestation_raw, encoding="utf-8", newline="")
                print("Rewrote it. Commit, then run this again for the manifest pin.")
                return 0

    committed_manifest = blob(root, "HEAD", str(MANIFEST).replace("\\", "/"))
    if committed_manifest is None:
        print("The manifest is not committed at HEAD.", file=sys.stderr)
        return 1
    manifest_digest = hashlib.sha256(committed_manifest).hexdigest()
    attestation_text = attestation_path.read_text(encoding="utf-8")
    pinned = re.search(
        r'"manifest":\s*\{[^}]*?"sha256":\s*"([0-9a-f]{64})"', attestation_text, re.S
    )
    if pinned is None:
        print("Could not find the attestation's manifest pin.", file=sys.stderr)
        return 1
    if pinned.group(1) == manifest_digest:
        print("Attestation already pins the committed manifest.")
        return 0
    print(f"\n  attestation manifest pin\n    {pinned.group(1)}\n -> {manifest_digest}")
    if arguments.write:
        attestation_text = attestation_text.replace(pinned.group(1), manifest_digest, 1)
        attestation_path.write_text(attestation_text, encoding="utf-8", newline="")
        print("Rewrote the attestation. Commit it, then run verify-internal.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
