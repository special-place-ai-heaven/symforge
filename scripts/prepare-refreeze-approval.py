#!/usr/bin/env python3
"""Write the next Feature 020 V11 refreeze approval record.

Every field in that record is volatile state -- a commit id, a tree id, a
digest, a chain position. Typing any of them by hand is how a record ends up
binding a tree that no longer exists, so this reads them from git and from the
committed attestation instead.

    python scripts/prepare-refreeze-approval.py

It refuses rather than guesses:

* the working tree must be clean, because the record names a commit and a
  record that names a commit whose tree is not what you verified is worthless;
* `verify-internal` must pass on that commit;
* an existing record must carry a signature that verifies before it is
  archived, because an unverifiable predecessor breaks the chain the next
  verification walks.

The previous record and its signature move into `history/<digest>.json{,.sig}`,
keyed by the digest the successor names as its predecessor. The stale signature
beside the new record is removed: a signature over the old bytes must never sit
next to new ones.

No key material is read, written, or printed. Signing is a separate step --
`scripts/sign-refreeze-approval.ps1`.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

NAMESPACE = "symforge-feature-020-refreeze-v11"
REPOSITORY = "special-place-ai-heaven/symforge"
RELEASE_IDENTITY = "symforge-release@special-place-ai-heaven"
PURPOSE = "implementation_start"
STORE_LOCATOR = "local-operator-approval-store"
STORE_VERSION = 1
ATTESTATION_PATH = "docs/reviews/FEATURE-020-REFREEZE-ATTESTATION-v11.md"


def fail(message: str) -> "NoReturn":  # type: ignore[valid-type]
    print(f"REFUSED: {message}", file=sys.stderr)
    raise SystemExit(1)


def git(root: Path, *arguments: str) -> str:
    result = subprocess.run(
        ["git", *arguments], cwd=root, capture_output=True, text=True
    )
    if result.returncode != 0:
        fail(f"git {' '.join(arguments)} failed: {result.stderr.strip()}")
    return result.stdout.strip()


def canonical(value: object) -> bytes:
    return json.dumps(
        value, ensure_ascii=False, allow_nan=False, separators=(",", ":"), sort_keys=True
    ).encode("utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=".", help="Repository worktree to bind.")
    parser.add_argument(
        "--approval-dir",
        default=str(Path.home() / "symforge-approval"),
        help="Operator approval store, outside the repository.",
    )
    arguments = parser.parse_args()

    root = Path(arguments.root).resolve()
    store = Path(arguments.approval_dir).resolve()
    if not store.is_dir():
        fail(f"no approval store at {store}")

    if git(root, "status", "--porcelain") != "":
        fail("the working tree is dirty; commit first so the record names a real tree")

    verification = subprocess.run(
        [sys.executable, "execution/refreeze_v11.py", "verify-internal", "--target-ref", "HEAD"],
        cwd=root,
        capture_output=True,
        text=True,
    )
    if verification.returncode != 0:
        fail(
            "internal refreeze verification failed on HEAD, so there is nothing "
            f"worth approving: {verification.stdout.strip()}{verification.stderr.strip()}"
        )

    commit = git(root, "rev-parse", "HEAD")
    tree = git(root, "rev-parse", "HEAD^{tree}")
    attestation_blob = subprocess.run(
        ["git", "cat-file", "blob", f"HEAD:{ATTESTATION_PATH}"],
        cwd=root,
        capture_output=True,
    )
    if attestation_blob.returncode != 0:
        fail(f"{ATTESTATION_PATH} is not committed at HEAD")
    attestation_digest = hashlib.sha256(attestation_blob.stdout).hexdigest()

    record_path = store / "approval.json"
    signature_path = store / "approval.json.sig"
    history = store / "history"
    history.mkdir(exist_ok=True)

    predecessor_digest = None
    sequence = 1
    if record_path.exists():
        previous_bytes = record_path.read_bytes()
        previous = json.loads(previous_bytes.decode("utf-8"))
        if previous_bytes != canonical(previous):
            fail("the existing record is not canonical JSON; refusing to chain from it")
        if not signature_path.exists():
            fail("the existing record has no signature; sign it before superseding it")
        allowed = store / "allowed_signers"
        if not allowed.exists():
            fail(f"no allowed_signers at {allowed}; cannot verify the predecessor")
        verified = subprocess.run(
            [
                "ssh-keygen", "-Y", "verify",
                "-f", str(allowed),
                "-I", RELEASE_IDENTITY,
                "-n", NAMESPACE,
                "-s", str(signature_path),
            ],
            input=previous_bytes,
            capture_output=True,
        )
        if verified.returncode != 0:
            fail(
                "the existing record's signature does not verify, so it cannot become "
                "a predecessor; the chain would be unverifiable from here on"
            )
        predecessor_digest = hashlib.sha256(previous_bytes).hexdigest()
        sequence = int(previous["sequence"]) + 1
        if previous["target_commit"] == commit:
            fail(
                f"sequence {previous['sequence']} already binds {commit[:12]}; "
                "there is nothing new to approve"
            )
        archived = history / f"{predecessor_digest}.json"
        if not archived.exists():
            archived.write_bytes(previous_bytes)
            shutil.copyfile(signature_path, history / f"{predecessor_digest}.json.sig")
        elif archived.read_bytes() != previous_bytes:
            fail(f"history entry {predecessor_digest[:12]} exists with different bytes")

    record = {
        "kind": "symforge-feature-020-refreeze-approval",
        "schema_version": 1,
        "repository": REPOSITORY,
        "purpose": PURPOSE,
        "target_commit": commit,
        "target_tree": tree,
        "attestation": {"path": ATTESTATION_PATH, "sha256": attestation_digest},
        "release_identity": RELEASE_IDENTITY,
        "approved_at": datetime.now(timezone.utc).astimezone().isoformat(timespec="seconds"),
        "sequence": sequence,
        "store_locator": STORE_LOCATOR,
        "store_version": STORE_VERSION,
        "predecessor_digest": predecessor_digest,
        "signature_namespace": NAMESPACE,
    }
    record_bytes = canonical(record)
    record_path.write_bytes(record_bytes)
    # A signature over the previous bytes must not sit beside the new ones.
    signature_path.unlink(missing_ok=True)

    print("APPROVAL RECORD WRITTEN")
    print(f"  path               {record_path}")
    print(f"  sequence           {sequence}", end="")
    print(f"  (predecessor {predecessor_digest[:16]})" if predecessor_digest else "")
    print(f"  target_commit      {commit}")
    print(f"  target_tree        {tree}")
    print(f"  attestation sha256 {attestation_digest}")
    print(f"  record sha256      {hashlib.sha256(record_bytes).hexdigest()}")
    print()
    print("Not signed yet. Sign with: pwsh scripts/sign-refreeze-approval.ps1")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
