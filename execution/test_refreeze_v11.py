from __future__ import annotations

import hashlib
import io
import json
import os
import shutil
import subprocess
import unittest
import uuid
from copy import deepcopy
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest.mock import patch

import refreeze_v11


FEATURE_ROOT = "specs/020-repository-knowledge-index"
MANIFEST_PATH = f"{FEATURE_ROOT}/REFREEZE-MANIFEST-v11.md"
ATTESTATION_PATH = "docs/reviews/FEATURE-020-REFREEZE-ATTESTATION-v11.md"
DESIGN_PATH = (
    "docs/superpowers/specs/2026-08-11-project-index-lifecycle-prevention-design.md"
)
API_PATH = f"{FEATURE_ROOT}/contracts/public-api-v11.json"
CONTEXT_PATH = "CONTEXT.md"
MANIFEST_START = "<!-- SYMFORGE FEATURE020 REFREEZE V11 JSON START -->"
MANIFEST_END = "<!-- SYMFORGE FEATURE020 REFREEZE V11 JSON END -->"
ATTESTATION_START = "<!-- SYMFORGE FEATURE020 ATTESTATION V11 JSON START -->"
ATTESTATION_END = "<!-- SYMFORGE FEATURE020 ATTESTATION V11 JSON END -->"
AMENDMENT_DOMAIN = b"symforge.feature-020.amendment-set.v11\0"


def system_executable(name: str) -> Path:
    discovered = shutil.which(name)
    if discovered is None:
        raise RuntimeError(f"required test executable unavailable: {name}")
    return Path(discovered).resolve()


SYSTEM_GIT_EXECUTABLE = system_executable("git")
SYSTEM_SSH_KEYGEN_EXECUTABLE = system_executable("ssh-keygen")

REQUIRED_NORMATIVE_PATHS = [
    CONTEXT_PATH,
    f"{FEATURE_ROOT}/GOAL.md",
    f"{FEATURE_ROOT}/checklists/requirements.md",
    f"{FEATURE_ROOT}/contracts/knowledge-authority-hygiene.md",
    f"{FEATURE_ROOT}/contracts/lifecycle-acceptance-oracles-v11.md",
    f"{FEATURE_ROOT}/contracts/lifecycle-oracle-traceability-v11.md",
    API_PATH,
    f"{FEATURE_ROOT}/contracts/repository-mental-model.md",
    f"{FEATURE_ROOT}/contracts/search-knowledge.md",
    f"{FEATURE_ROOT}/contracts/source-binding-and-state.md",
    f"{FEATURE_ROOT}/contracts/v10-authority-retirement-v11.md",
    f"{FEATURE_ROOT}/data-model.md",
    f"{FEATURE_ROOT}/plan.md",
    f"{FEATURE_ROOT}/quickstart.md",
    f"{FEATURE_ROOT}/spec.md",
    f"{FEATURE_ROOT}/tasks.md",
]


def canonical_json(value: object) -> bytes:
    return json.dumps(
        value,
        ensure_ascii=False,
        allow_nan=False,
        separators=(",", ":"),
        sort_keys=True,
    ).encode("utf-8")


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sentinel_document(start: str, end: str, payload: dict[str, object]) -> bytes:
    rendered = json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True)
    return f"# Test fixture\n\n{start}\n```json\n{rendered}\n```\n{end}\n".encode()


def compute_amendment_set_id(amendments: list[dict[str, object]]) -> str:
    amendment_records = [
        {
            "amendment_id": item["amendment_id"],
            "contract_clause_ids": list(item["contract_clause_ids"]),
            "plan_task_ids": list(item["plan_task_ids"]),
            "regression_ids": list(item["regression_ids"]),
            "replaced": sorted(item["replaced"], key=canonical_json),
            "replacements": sorted(item["replacements"], key=canonical_json),
            "requirement_ids": list(item["requirement_ids"]),
        }
        for item in amendments
    ]
    amendment_records.sort(key=lambda item: item["amendment_id"])
    return sha256(AMENDMENT_DOMAIN + canonical_json(amendment_records))


class RefreezeFixture:
    def __init__(self) -> None:
        temp_root = Path(__file__).resolve().parent.parent / ".tmp" / "execution-tests"
        temp_root.mkdir(parents=True, exist_ok=True)
        self.root = temp_root / f"refreeze-{uuid.uuid4().hex}"
        self.root.mkdir()
        self.git("init")
        self.git("config", "user.email", "refreeze-test@example.invalid")
        self.git("config", "user.name", "Refreeze Test")
        self.git("config", "core.autocrlf", "false")

        baseline_lines = [f"baseline clause {index}\n" for index in range(1, 20)]
        self.write(f"{FEATURE_ROOT}/spec.md", "".join(baseline_lines).encode())
        self.write(DESIGN_PATH, b"cleared lifecycle design\n")
        self.write(CONTEXT_PATH, b"bound Feature 020 context\n")
        self.git("add", "--all")
        self.git("commit", "-m", "baseline")
        self.baseline_commit = self.git_text("rev-parse", "HEAD")
        self.baseline_tree = self.git_text("rev-parse", "HEAD^{tree}")

        target_lines = []
        for index in range(1, 20):
            amendment_id = f"F020-V11-A{index:02}"
            regressions = refreeze_v11.EXPECTED_AMENDMENT_MAPPINGS[amendment_id][
                "regression_ids"
            ]
            target_lines.append(
                f"**{amendment_id}** — replacement clause {index}; regressions "
                f"{', '.join(regressions)}.\n"
            )
        self.write(f"{FEATURE_ROOT}/spec.md", "".join(target_lines).encode())
        for path in REQUIRED_NORMATIVE_PATHS:
            if path in {CONTEXT_PATH, API_PATH, f"{FEATURE_ROOT}/spec.md"}:
                continue
            self.write(path, f"V11 normative artifact: {path}\n".encode())
        requirement_ids = sorted(
            {
                requirement_id
                for mapping in refreeze_v11.EXPECTED_AMENDMENT_MAPPINGS.values()
                for requirement_id in mapping["requirement_ids"]
                if not requirement_id.startswith("F020-V11-A")
            }
        )
        self.write(
            f"{FEATURE_ROOT}/GOAL.md",
            "".join(
                f"**{requirement_id}**: fixture definition.\n"
                for requirement_id in requirement_ids
            ).encode(),
        )
        contract_slugs: dict[str, set[str]] = {}
        task_regressions: dict[str, set[str]] = {}
        for mapping in refreeze_v11.EXPECTED_AMENDMENT_MAPPINGS.values():
            for contract_reference in mapping["contract_clause_ids"]:
                relative_path, slug = contract_reference.split("#", 1)
                contract_slugs.setdefault(
                    f"{FEATURE_ROOT}/{relative_path}", set()
                ).add(slug)
            for task_id in mapping["plan_task_ids"]:
                task_regressions.setdefault(task_id, set()).update(
                    mapping["regression_ids"]
                )
        for contract_path, slugs in contract_slugs.items():
            self.write(
                contract_path,
                "".join(f"# {slug}\n\n" for slug in sorted(slugs)).encode(),
            )
        self.write(
            f"{FEATURE_ROOT}/tasks.md",
            "".join(
                f"- [ ] {task_id} Verify {' '.join(sorted(regressions))}.\n"
                for task_id, regressions in sorted(task_regressions.items())
            ).encode(),
        )

        repository_root = Path(__file__).resolve().parent.parent
        api_bytes = (repository_root / API_PATH).read_bytes()
        self.api = json.loads(api_bytes)
        for corpus_entry in self.api["configuration_domain"]["cover"][
            "input_corpus"
        ]:
            corpus_path = corpus_entry["path"]
            self.write(corpus_path, (repository_root / corpus_path).read_bytes())
        self.write(API_PATH, api_bytes)

        amendments: list[dict[str, object]] = []
        for index, (baseline_line, target_line) in enumerate(
            zip(baseline_lines, target_lines, strict=True), start=1
        ):
            amendment_id = f"F020-V11-A{index:02}"
            expected_mapping = refreeze_v11.EXPECTED_AMENDMENT_MAPPINGS[
                amendment_id
            ]
            amendments.append(
                {
                    "amendment_id": amendment_id,
                    "contract_clause_ids": list(
                        expected_mapping["contract_clause_ids"]
                    ),
                    "plan_task_ids": list(expected_mapping["plan_task_ids"]),
                    "regression_ids": list(expected_mapping["regression_ids"]),
                    "replaced": [
                        {
                            "clause_id": f"BASE-{index:02}",
                            "end_line": index,
                            "path": f"{FEATURE_ROOT}/spec.md",
                            "sha256": sha256(baseline_line.encode()),
                            "source": "baseline",
                            "start_line": index,
                        }
                    ],
                    "replacements": [
                        {
                            "clause_id": f"V11-{index:02}",
                            "end_line": index,
                            "path": f"{FEATURE_ROOT}/spec.md",
                            "sha256": sha256(target_line.encode()),
                            "source": "target",
                            "start_line": index,
                        }
                    ],
                    "requirement_ids": list(expected_mapping["requirement_ids"]),
                }
            )

        amendment_set_id = compute_amendment_set_id(amendments)

        feature_files = [
            path.relative_to(self.root).as_posix()
            for path in (self.root / FEATURE_ROOT).rglob("*")
            if path.is_file()
        ]
        inventory_paths = sorted({*feature_files, MANIFEST_PATH, CONTEXT_PATH})
        inventory = []
        for path in inventory_paths:
            self_excluded = path == MANIFEST_PATH
            inventory.append(
                {
                    "classification": "normative",
                    "hash_policy": "self_excluded" if self_excluded else "raw_bytes",
                    "path": path,
                    "scope": "bound" if path == CONTEXT_PATH else "feature",
                    "sha256": None if self_excluded else sha256(self.read(path)),
                    "superseded_by": [],
                }
            )

        self.manifest = {
            "amendment_set_id": amendment_set_id,
            "amendments": amendments,
            "baseline": {
                "commit": self.baseline_commit,
                "tree": self.baseline_tree,
            },
            "context": {"path": CONTEXT_PATH, "sha256": sha256(self.read(CONTEXT_PATH))},
            "design": {"path": DESIGN_PATH, "sha256": sha256(self.read(DESIGN_PATH))},
            "detached_attestation_path": ATTESTATION_PATH,
            "feature_root": FEATURE_ROOT,
            "inventory": inventory,
            "kind": "symforge-feature-020-refreeze",
            "public_api": {
                "canonical_sha256": sha256(canonical_json(self.api)),
                "path": API_PATH,
                "raw_sha256": sha256(api_bytes),
            },
            "required_normative_paths": REQUIRED_NORMATIVE_PATHS,
            "schema_version": 1,
            "self_path": MANIFEST_PATH,
        }
        manifest_bytes = sentinel_document(MANIFEST_START, MANIFEST_END, self.manifest)
        self.write(MANIFEST_PATH, manifest_bytes)

        self.attestation = {
            "amendment_set_id": amendment_set_id,
            "baseline": self.manifest["baseline"],
            "context": self.manifest["context"],
            "design": self.manifest["design"],
            "external_approval": {
                "purpose": "implementation_start",
                "required": True,
                "signature_namespace": "symforge-feature-020-refreeze-v11",
            },
            "kind": "symforge-feature-020-refreeze-attestation",
            "manifest": {"path": MANIFEST_PATH, "sha256": sha256(manifest_bytes)},
            "public_api": self.manifest["public_api"],
            "schema_version": 1,
        }
        self.write(
            ATTESTATION_PATH,
            sentinel_document(ATTESTATION_START, ATTESTATION_END, self.attestation),
        )
        self.git("add", "--all")
        self.git("commit", "-m", "refreeze")
        self.target_commit = self.git_text("rev-parse", "HEAD")
        self.target_tree = self.git_text("rev-parse", "HEAD^{tree}")

    def commit_all(self, message: str) -> str:
        self.git("add", "--all")
        self.git("commit", "-m", message)
        self.target_commit = self.git_text("rev-parse", "HEAD")
        self.target_tree = self.git_text("rev-parse", "HEAD^{tree}")
        return self.target_commit

    def reseal_manifest(self, manifest: dict[str, object]) -> str:
        self.manifest = deepcopy(manifest)
        manifest_bytes = sentinel_document(MANIFEST_START, MANIFEST_END, self.manifest)
        self.write(MANIFEST_PATH, manifest_bytes)
        for field in (
            "baseline",
            "design",
            "context",
            "public_api",
            "amendment_set_id",
        ):
            self.attestation[field] = deepcopy(self.manifest[field])
        self.attestation["manifest"] = {
            "path": MANIFEST_PATH,
            "sha256": sha256(manifest_bytes),
        }
        self.write(
            ATTESTATION_PATH,
            sentinel_document(ATTESTATION_START, ATTESTATION_END, self.attestation),
        )
        return self.commit_all("mutate refreeze evidence")

    def replace_public_api(self, api: dict[str, object]) -> str:
        self.api = deepcopy(api)
        api_bytes = json.dumps(self.api, indent=2, sort_keys=True).encode() + b"\n"
        self.write(API_PATH, api_bytes)
        manifest = deepcopy(self.manifest)
        manifest["public_api"] = {
            "canonical_sha256": sha256(canonical_json(self.api)),
            "path": API_PATH,
            "raw_sha256": sha256(api_bytes),
        }
        api_entry = next(
            item for item in manifest["inventory"] if item["path"] == API_PATH
        )
        api_entry["sha256"] = sha256(api_bytes)
        return self.reseal_manifest(manifest)

    def make_external_approval(
        self,
    ) -> tuple[dict[str, object], Path, Path, Path, str]:
        trust_root = self.root.parent / f"refreeze-trust-{uuid.uuid4().hex}"
        trust_root.mkdir()
        release_identity = "release-ci@example.invalid"
        approval = {
            "approved_at": "2026-08-11T12:00:00Z",
            "attestation": {
                "path": ATTESTATION_PATH,
                "sha256": sha256(self.read(ATTESTATION_PATH)),
            },
            "kind": "symforge-feature-020-refreeze-approval",
            "predecessor_digest": None,
            "purpose": "implementation_start",
            "release_identity": release_identity,
            "repository": "special-place-ai-heaven/symforge",
            "schema_version": 1,
            "sequence": 1,
            "signature_namespace": "symforge-feature-020-refreeze-v11",
            "store_locator": "append-only://refreeze-test/1",
            "store_version": 1,
            "target_commit": self.target_commit,
            "target_tree": self.target_tree,
        }
        approval_path = trust_root / "approval.json"
        signature_path = trust_root / "approval.json.sshsig"
        allowed_signers_path = trust_root / "allowed_signers"
        approval_path.write_bytes(canonical_json(approval))
        signature_path.write_bytes(b"test signature boundary\n")
        allowed_signers_path.write_bytes(b"test allowed-signers boundary\n")
        return (
            approval,
            approval_path,
            signature_path,
            allowed_signers_path,
            release_identity,
        )

    def write(self, path: str, data: bytes) -> None:
        destination = self.root / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)

    def read(self, path: str) -> bytes:
        return (self.root / path).read_bytes()

    def git(self, *args: str) -> subprocess.CompletedProcess[bytes]:
        result = subprocess.run(
            [str(SYSTEM_GIT_EXECUTABLE), *args],
            cwd=self.root,
            check=False,
            capture_output=True,
        )
        if result.returncode != 0:
            self.fail_git(args, result)
        return result

    def git_text(self, *args: str) -> str:
        return self.git(*args).stdout.decode("ascii").strip()

    def fail_git(
        self, args: tuple[str, ...], result: subprocess.CompletedProcess[bytes]
    ) -> None:
        raise AssertionError(f"git command failed: {args!r}; exit={result.returncode}")


class RefreezeV11Tests(unittest.TestCase):
    def test_verify_internal_accepts_exact_committed_refreeze(self) -> None:
        fixture = RefreezeFixture()

        status = refreeze_v11.main(
            [
                "--root",
                str(fixture.root),
                "verify-internal",
                "--target-ref",
                fixture.target_commit,
            ]
        )

        self.assertEqual(status, 0)

    def test_verify_internal_rejects_an_unclassified_feature_file(self) -> None:
        fixture = RefreezeFixture()
        fixture.write(f"{FEATURE_ROOT}/unclassified.md", b"not in the inventory\n")
        target = fixture.commit_all("add unclassified artifact")

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_noncanonical_inventory_order(self) -> None:
        fixture = RefreezeFixture()
        manifest = deepcopy(fixture.manifest)
        manifest["inventory"] = list(reversed(manifest["inventory"]))
        target = fixture.reseal_manifest(manifest)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_ignores_uncommitted_worktree_drift(self) -> None:
        fixture = RefreezeFixture()
        fixture.write(f"{FEATURE_ROOT}/spec.md", b"uncommitted replacement bytes\n")

        status = refreeze_v11.main(
            [
                "--root",
                str(fixture.root),
                "verify-internal",
                "--target-ref",
                fixture.target_commit,
            ]
        )

        self.assertEqual(status, 0)

    def test_verify_internal_reads_literal_objects_despite_git_replace(self) -> None:
        fixture = RefreezeFixture()
        replacement_commit = fixture.git_text(
            "commit-tree",
            fixture.baseline_tree,
            "-p",
            fixture.baseline_commit,
            "-m",
            "replacement object",
        )
        fixture.git("replace", fixture.target_commit, replacement_commit)

        status = refreeze_v11.main(
            [
                "--root",
                str(fixture.root),
                "verify-internal",
                "--target-ref",
                fixture.target_commit,
            ]
        )

        self.assertEqual(status, 0)

    def test_verify_internal_rejects_inherited_git_identity_overrides(self) -> None:
        fixture = RefreezeFixture()
        overrides = {
            "GIT_ALTERNATE_OBJECT_DIRECTORIES": str(fixture.root / "objects"),
            "GIT_CONFIG_COUNT": "1",
            "GIT_DIR": str(fixture.root / ".git"),
            "GIT_OBJECT_DIRECTORY": str(fixture.root / "objects"),
            "GIT_WORK_TREE": str(fixture.root),
        }
        for name, value in overrides.items():
            with self.subTest(name=name), patch.dict(
                os.environ, {name: value}, clear=False
            ):
                status = refreeze_v11.main(
                    [
                        "--root",
                        str(fixture.root),
                        "verify-internal",
                        "--target-ref",
                        fixture.target_commit,
                    ]
                )
                self.assertEqual(status, 1)

    def test_verify_internal_never_executes_repo_local_git_from_path(self) -> None:
        fixture = RefreezeFixture()
        discovered_git = shutil.which("git")
        self.assertIsNotNone(discovered_git)
        trusted_git = Path(discovered_git).resolve()
        marker = fixture.root / "repo-local-git-executed"
        if os.name == "nt":
            shim_name = "git.cmd"
            shim = (
                "@echo off\r\n"
                f">\"{marker}\" echo executed\r\n"
                f"@\"{trusted_git}\" %*\r\n"
            ).encode()
        else:
            shim_name = "git"
            shim = (
                "#!/bin/sh\n"
                f"printf executed > '{marker}'\n"
                f"exec '{trusted_git}' \"$@\"\n"
            ).encode()
        fixture.write(shim_name, shim)
        (fixture.root / shim_name).chmod(0o755)
        path = f"{fixture.root}{os.pathsep}{os.environ.get('PATH', '')}"
        stderr = io.StringIO()

        with patch.dict(os.environ, {"PATH": path}), redirect_stderr(stderr):
            status = refreeze_v11.main(
                [
                    "--root",
                    str(fixture.root),
                    "verify-internal",
                    "--target-ref",
                    fixture.target_commit,
                ]
            )

        self.assertEqual(status, 1)
        self.assertFalse(marker.exists())
        self.assertIn("EXECUTABLE_PROVENANCE_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_explicit_repo_local_git_executable(self) -> None:
        fixture = RefreezeFixture()
        executable = fixture.root / ("git.exe" if os.name == "nt" else "git")
        fixture.write(executable.name, b"not a trusted executable")
        executable.chmod(0o755)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                [
                    "--root",
                    str(fixture.root),
                    "--git-executable",
                    str(executable),
                    "verify-internal",
                    "--target-ref",
                    fixture.target_commit,
                ]
            )

        self.assertEqual(status, 1)
        self.assertIn("EXECUTABLE_PROVENANCE_INVALID", stderr.getvalue())

    def test_verify_approval_rejects_repo_local_ssh_keygen_executable(self) -> None:
        fixture = RefreezeFixture()
        (
            _approval,
            approval_path,
            signature_path,
            allowed_signers_path,
            release_identity,
        ) = fixture.make_external_approval()
        discovered_git = shutil.which("git")
        self.assertIsNotNone(discovered_git)
        ssh_keygen = fixture.root / (
            "ssh-keygen.exe" if os.name == "nt" else "ssh-keygen"
        )
        fixture.write(ssh_keygen.name, b"not a trusted executable")
        ssh_keygen.chmod(0o755)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                [
                    "--root",
                    str(fixture.root),
                    "--git-executable",
                    str(Path(discovered_git).resolve()),
                    "verify-approval",
                    "--target-ref",
                    fixture.target_commit,
                    "--ssh-keygen-executable",
                    str(ssh_keygen),
                    "--approval-record",
                    str(approval_path),
                    "--approval-signature",
                    str(signature_path),
                    "--allowed-signers",
                    str(allowed_signers_path),
                    "--expected-release-identity",
                    release_identity,
                    "--expected-repository",
                    "special-place-ai-heaven/symforge",
                ]
            )

        self.assertEqual(status, 1)
        self.assertIn("EXECUTABLE_PROVENANCE_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_committed_raw_byte_drift(self) -> None:
        fixture = RefreezeFixture()
        fixture.write(f"{FEATURE_ROOT}/GOAL.md", b"drifted after refreeze\n")
        target = fixture.commit_all("drift a pinned artifact")

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_symlink_mode_for_design_blob(self) -> None:
        fixture = RefreezeFixture()
        blob = fixture.git_text("rev-parse", f"HEAD:{DESIGN_PATH}")
        fixture.git(
            "update-index",
            "--add",
            "--cacheinfo",
            f"120000,{blob},{DESIGN_PATH}",
        )
        fixture.git("commit", "-m", "make design entry a symlink")
        target = fixture.git_text("rev-parse", "HEAD")
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("GIT_TREE_ENTRY_UNSUPPORTED", stderr.getvalue())

    def test_verify_internal_rejects_symlink_mode_for_attestation_blob(self) -> None:
        fixture = RefreezeFixture()
        blob = fixture.git_text("rev-parse", f"HEAD:{ATTESTATION_PATH}")
        fixture.git(
            "update-index",
            "--add",
            "--cacheinfo",
            f"120000,{blob},{ATTESTATION_PATH}",
        )
        fixture.git("commit", "-m", "make attestation entry a symlink")
        target = fixture.git_text("rev-parse", "HEAD")
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("GIT_TREE_ENTRY_UNSUPPORTED", stderr.getvalue())

    def test_verify_internal_accepts_executable_regular_file_mode(self) -> None:
        fixture = RefreezeFixture()
        blob = fixture.git_text("rev-parse", f"HEAD:{DESIGN_PATH}")
        fixture.git(
            "update-index",
            "--add",
            "--cacheinfo",
            f"100755,{blob},{DESIGN_PATH}",
        )
        fixture.git("commit", "-m", "make design entry executable")
        target = fixture.git_text("rev-parse", "HEAD")

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 0)

    def test_verify_internal_rejects_a_hash_for_the_manifest_itself(self) -> None:
        fixture = RefreezeFixture()
        manifest = deepcopy(fixture.manifest)
        self_entry = next(
            item for item in manifest["inventory"] if item["path"] == MANIFEST_PATH
        )
        self_entry["hash_policy"] = "raw_bytes"
        self_entry["sha256"] = "0" * 64
        target = fixture.reseal_manifest(manifest)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_missing_amendment(self) -> None:
        fixture = RefreezeFixture()
        manifest = deepcopy(fixture.manifest)
        manifest["amendments"] = manifest["amendments"][:-1]
        target = fixture.reseal_manifest(manifest)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_a_baseline_outside_target_history(self) -> None:
        fixture = RefreezeFixture()
        unrelated_commit = fixture.git_text(
            "commit-tree", fixture.baseline_tree, "-m", "unrelated baseline"
        )
        manifest = deepcopy(fixture.manifest)
        manifest["baseline"]["commit"] = unrelated_commit
        target = fixture.reseal_manifest(manifest)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_empty_amendment_mapping_lane(self) -> None:
        fixture = RefreezeFixture()
        manifest = deepcopy(fixture.manifest)
        manifest["amendments"][0]["regression_ids"] = []
        target = fixture.reseal_manifest(manifest)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_clause_bytes_not_owned_by_the_selected_commit(
        self,
    ) -> None:
        fixture = RefreezeFixture()
        manifest = deepcopy(fixture.manifest)
        manifest["amendments"][0]["replacements"][0]["sha256"] = "0" * 64
        target = fixture.reseal_manifest(manifest)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_clause_mapping_outside_normative_artifacts(
        self,
    ) -> None:
        fixture = RefreezeFixture()
        manifest = deepcopy(fixture.manifest)
        replacement = manifest["amendments"][0]["replacements"][0]
        replacement["path"] = DESIGN_PATH
        replacement["start_line"] = 1
        replacement["end_line"] = 1
        replacement["sha256"] = sha256(b"cleared lifecycle design\n")
        manifest["amendment_set_id"] = compute_amendment_set_id(
            manifest["amendments"]
        )
        target = fixture.reseal_manifest(manifest)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_operator_chosen_amendment_set_id(self) -> None:
        fixture = RefreezeFixture()
        manifest = deepcopy(fixture.manifest)
        manifest["amendment_set_id"] = "0" * 64
        target = fixture.reseal_manifest(manifest)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_amendment_digest_binds_requirement_association(self) -> None:
        fixture = RefreezeFixture()
        manifest = deepcopy(fixture.manifest)
        manifest["amendments"][0]["requirement_ids"] = ["F020-V11-A02"]
        target = fixture.reseal_manifest(manifest)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_swapped_amendment_semantics(self) -> None:
        fixture = RefreezeFixture()
        manifest = deepcopy(fixture.manifest)
        first, second = manifest["amendments"][0:2]
        for field in (
            "requirement_ids",
            "contract_clause_ids",
            "plan_task_ids",
            "regression_ids",
        ):
            first[field], second[field] = second[field], first[field]
        manifest["amendment_set_id"] = compute_amendment_set_id(
            manifest["amendments"]
        )
        target = fixture.reseal_manifest(manifest)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn(
            "AMENDMENT_SEMANTIC_ASSOCIATION_INVALID", stderr.getvalue()
        )

    def test_verify_internal_binds_regressions_to_replacement_bytes(self) -> None:
        fixture = RefreezeFixture()
        manifest = deepcopy(fixture.manifest)
        manifest["amendments"][0]["regression_ids"] = ["F020-V11-R02"]
        manifest["amendment_set_id"] = compute_amendment_set_id(
            manifest["amendments"]
        )
        target = fixture.reseal_manifest(manifest)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn(
            "AMENDMENT_SEMANTIC_ASSOCIATION_INVALID", stderr.getvalue()
        )

    def test_verify_internal_binds_exact_amendment_crosswalk(self) -> None:
        fixture = RefreezeFixture()
        manifest = deepcopy(fixture.manifest)
        manifest["amendments"][0]["plan_task_ids"] = deepcopy(
            manifest["amendments"][1]["plan_task_ids"]
        )
        manifest["amendment_set_id"] = compute_amendment_set_id(
            manifest["amendments"]
        )
        target = fixture.reseal_manifest(manifest)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn(
            "AMENDMENT_SEMANTIC_ASSOCIATION_INVALID", stderr.getvalue()
        )

    def test_amendment_digest_binds_full_clause_location(self) -> None:
        fixture = RefreezeFixture()
        manifest = deepcopy(fixture.manifest)
        target_line = fixture.read(f"{FEATURE_ROOT}/spec.md").splitlines(
            keepends=True
        )[0]
        quickstart_path = f"{FEATURE_ROOT}/quickstart.md"
        fixture.write(quickstart_path, target_line)
        quickstart_entry = next(
            item for item in manifest["inventory"] if item["path"] == quickstart_path
        )
        quickstart_entry["sha256"] = sha256(target_line)
        manifest["amendments"][0]["replacements"][0]["path"] = quickstart_path
        target = fixture.reseal_manifest(manifest)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_amendment_clause_arrays_require_canonical_order(self) -> None:
        fixture = RefreezeFixture()
        manifest = deepcopy(fixture.manifest)
        second_clause = deepcopy(manifest["amendments"][1]["replaced"][0])
        second_clause["clause_id"] = "AAA-SECOND-CLAUSE"
        manifest["amendments"][0]["replaced"].append(second_clause)
        manifest["amendment_set_id"] = compute_amendment_set_id(
            manifest["amendments"]
        )
        target = fixture.reseal_manifest(manifest)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_duplicate_clause_location(self) -> None:
        fixture = RefreezeFixture()
        manifest = deepcopy(fixture.manifest)
        clauses = manifest["amendments"][0]["replaced"]
        duplicate = deepcopy(clauses[0])
        duplicate["clause_id"] = "BASE-01-DUPLICATE"
        clauses.append(duplicate)
        clauses.sort(key=canonical_json)
        manifest["amendment_set_id"] = compute_amendment_set_id(
            manifest["amendments"]
        )
        target = fixture.reseal_manifest(manifest)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("CLAUSE_LOCATION_DUPLICATE", stderr.getvalue())

    def test_verify_internal_rejects_overlapping_clause_ranges(self) -> None:
        fixture = RefreezeFixture()
        manifest = deepcopy(fixture.manifest)
        clause = manifest["amendments"][0]["replaced"][0]
        clause["end_line"] = 2
        clause["sha256"] = sha256(
            b"baseline clause 1\nbaseline clause 2\n"
        )
        manifest["amendment_set_id"] = compute_amendment_set_id(
            manifest["amendments"]
        )
        target = fixture.reseal_manifest(manifest)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("CLAUSE_RANGE_OVERLAP", stderr.getvalue())

    def test_verify_internal_rejects_duplicate_manifest_json_keys(self) -> None:
        fixture = RefreezeFixture()
        raw = fixture.read(MANIFEST_PATH)
        duplicated = raw.replace(
            b'  "kind": "symforge-feature-020-refreeze",',
            b'  "kind": "symforge-feature-020-refreeze",\n'
            b'  "kind": "symforge-feature-020-refreeze",',
            1,
        )
        self.assertNotEqual(duplicated, raw)
        fixture.write(MANIFEST_PATH, duplicated)
        target = fixture.commit_all("duplicate a manifest key")

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_requires_one_json_fence_between_sentinels(self) -> None:
        fixture = RefreezeFixture()
        raw = fixture.read(MANIFEST_PATH)
        unfenced = raw.replace(b"```json\n", b"", 1).replace(b"\n```\n", b"\n", 1)
        fixture.write(MANIFEST_PATH, unfenced)
        target = fixture.commit_all("remove the manifest fence")

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_reversed_sentinels_fail_with_typed_order_error(self) -> None:
        reversed_markers = f"{MANIFEST_END}\n{MANIFEST_START}\n".encode()

        with self.assertRaisesRegex(
            refreeze_v11.RefreezeError, "SENTINEL_ORDER_INVALID"
        ):
            refreeze_v11._parse_sentinel_json(
                reversed_markers, MANIFEST_START, MANIFEST_END
            )

    def test_verify_internal_rejects_public_api_canonical_digest_drift(self) -> None:
        fixture = RefreezeFixture()
        manifest = deepcopy(fixture.manifest)
        manifest["public_api"]["canonical_sha256"] = "0" * 64
        target = fixture.reseal_manifest(manifest)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_minimal_vacuous_public_api(self) -> None:
        fixture = RefreezeFixture()
        target = fixture.replace_public_api(
            {
                "canonicalization": "jcs+symforge-api-v1",
                "kind": "symforge-rust-public-api",
                "schema_version": 1,
            }
        )

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_unknown_public_api_field(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["unexpected_field"] = True
        target = fixture.replace_public_api(api)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_dangling_supported_feature_vector(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["configuration_domain"]["targets"][0][
            "supported_feature_vectors"
        ] = ["definitely-missing-vector"]
        target = fixture.replace_public_api(api)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_configuration_cell_coverage_gap(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["configuration_domain"]["cells"].pop()
        target = fixture.replace_public_api(api)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_incomplete_feature_partition(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["configuration_domain"]["feature_vectors"][0]["disabled"].pop()
        target = fixture.replace_public_api(api)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_input_corpus_path_traversal(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["configuration_domain"]["cover"]["input_corpus"][0]["path"] = (
            "../outside"
        )
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                [
                    "--root",
                    str(fixture.root),
                    "verify-internal",
                    "--target-ref",
                    target,
                ]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_INPUT_CORPUS_PATH_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_unsorted_input_corpus(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        corpus = api["configuration_domain"]["cover"]["input_corpus"]
        corpus[0], corpus[1] = corpus[1], corpus[0]
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                [
                    "--root",
                    str(fixture.root),
                    "verify-internal",
                    "--target-ref",
                    target,
                ]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_INPUT_CORPUS_ORDER_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_unknown_input_corpus_role(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["configuration_domain"]["cover"]["input_corpus"][0]["role"] = (
            "unknown-role"
        )
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                [
                    "--root",
                    str(fixture.root),
                    "verify-internal",
                    "--target-ref",
                    target,
                ]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_INPUT_CORPUS_ROLE_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_wrong_input_corpus_role_cardinality(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        corpus = api["configuration_domain"]["cover"]["input_corpus"]
        corpus.pop()
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                [
                    "--root",
                    str(fixture.root),
                    "verify-internal",
                    "--target-ref",
                    target,
                ]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_INPUT_CORPUS_ROLE_CARDINALITY_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_input_corpus_role_path_swap(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        corpus = api["configuration_domain"]["cover"]["input_corpus"]
        corpus[0]["role"], corpus[1]["role"] = corpus[1]["role"], corpus[0]["role"]
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_INPUT_CORPUS_ROLE_PATH_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_missing_input_corpus_blob(self) -> None:
        fixture = RefreezeFixture()
        missing_path = fixture.api["configuration_domain"]["cover"]["input_corpus"][
            -1
        ]["path"]
        (fixture.root / missing_path).unlink()
        target = fixture.commit_all("remove input corpus blob")
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                [
                    "--root",
                    str(fixture.root),
                    "verify-internal",
                    "--target-ref",
                    target,
                ]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_INPUT_CORPUS_BLOB_MISSING", stderr.getvalue())

    def test_verify_internal_rejects_input_corpus_raw_hash_mismatch(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["configuration_domain"]["cover"]["input_corpus"][0]["sha256"] = (
            "0" * 64
        )
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                [
                    "--root",
                    str(fixture.root),
                    "verify-internal",
                    "--target-ref",
                    target,
                ]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_INPUT_CORPUS_HASH_MISMATCH", stderr.getvalue())

    def test_verify_internal_rejects_dangling_export_item(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["expected_graph"]["exports"][0]["item"] = "type:missing:NeverDefined"
        target = fixture.replace_public_api(api)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_function_relabelled_as_type(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        functions = api["expected_graph"]["functions"]
        function = next(
            item for item in functions if item["id"] == "function:embed:engine_info"
        )
        function["id"] = "type:embed:engine_info"
        functions.sort(key=lambda item: item["id"])
        export = next(
            item
            for item in api["expected_graph"]["exports"]
            if item["path"] == "symforge::embed::engine_info"
        )
        export["item"] = "type:embed:engine_info"
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_DECLARATION_ID_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_underscore_function_name(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        functions = api["expected_graph"]["functions"]
        function = next(
            item for item in functions if item["id"] == "function:embed:engine_info"
        )
        function["id"] = "function:embed:_"
        functions.sort(key=lambda item: item["id"])
        export = next(
            item
            for item in api["expected_graph"]["exports"]
            if item["path"] == "symforge::embed::engine_info"
        )
        export["item"] = "function:embed:_"
        export["path"] = "symforge::embed::_"
        api["expected_graph"]["exports"].sort(key=lambda item: item["path"])
        category = next(
            item
            for item in api["migration_v10"]["categories"]
            if item["id"] == "v10-03-engine-info"
        )
        for field in ("atoms", "v11_atoms"):
            category[field][category[field].index("symforge::embed::engine_info")] = (
                "symforge::embed::_"
            )
            category[field].sort()
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_DECLARATION_ID_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_reserved_function_name(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        functions = api["expected_graph"]["functions"]
        function = next(
            item for item in functions if item["id"] == "function:embed:engine_info"
        )
        function["id"] = "function:embed:type"
        functions.sort(key=lambda item: item["id"])
        export = next(
            item
            for item in api["expected_graph"]["exports"]
            if item["path"] == "symforge::embed::engine_info"
        )
        export["item"] = "function:embed:type"
        export["path"] = "symforge::embed::type"
        api["expected_graph"]["exports"].sort(key=lambda item: item["path"])
        category = next(
            item
            for item in api["migration_v10"]["categories"]
            if item["id"] == "v10-03-engine-info"
        )
        for field in ("atoms", "v11_atoms"):
            category[field][category[field].index("symforge::embed::engine_info")] = (
                "symforge::embed::type"
            )
            category[field].sort()
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_DECLARATION_ID_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_dangling_projection_module(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["expected_graph"]["graph_projections"][0]["included_modules"][0] = (
            "symforge::missing"
        )
        target = fixture.replace_public_api(api)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_unsorted_core_named_sets(self) -> None:
        cases = (
            (("configuration_domain", "cells"), "id"),
            (("configuration_domain", "cfg_keys"), "name"),
            (("configuration_domain", "feature_vectors"), "id"),
            (("configuration_domain", "targets"), "id"),
            (("expected_graph", "exports"), "path"),
            (("expected_graph", "functions"), "id"),
            (("expected_graph", "graph_projections"), "id"),
            (("expected_graph", "impls"), "id"),
            (("expected_graph", "impls", 0, "associated_items"), "id"),
            (("expected_graph", "items"), "id"),
            (("expected_graph", "modules"), "path"),
        )
        for path, _key in cases:
            with self.subTest(path=path):
                fixture = RefreezeFixture()
                api = deepcopy(fixture.api)
                collection = api
                for segment in path:
                    collection = collection[segment]
                collection[0], collection[1] = collection[1], collection[0]
                target = fixture.replace_public_api(api)
                stderr = io.StringIO()

                with redirect_stderr(stderr):
                    status = refreeze_v11.main(
                        [
                            "--root",
                            str(fixture.root),
                            "verify-internal",
                            "--target-ref",
                            target,
                        ]
                    )

                self.assertEqual(status, 1)
                self.assertIn("API_NAMED_SET_ORDER_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_item_kind_definition_mismatch(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["expected_graph"]["items"][0]["kind"] = "enum"
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_ITEM_DEFINITION_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_reserved_type_name(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["expected_graph"]["items"][0]["id"] = "type:embed:Self"
        api["expected_graph"]["items"].sort(key=lambda item: item["id"])
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_DECLARATION_ID_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_unsorted_struct_fields(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        item = next(
            item
            for item in api["expected_graph"]["items"]
            if item["id"] == "type:embed:EngineInfo"
        )
        fields = item["definition"]["fields"]
        fields[0], fields[1] = fields[1], fields[0]
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_STRUCT_FIELD_ORDER_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_invalid_struct_field_identifier(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        item = next(
            candidate
            for candidate in api["expected_graph"]["items"]
            if candidate["kind"] == "struct"
            and candidate["definition"]["fields"]
        )
        item["definition"]["fields"][0]["name"] = "_"
        item["definition"]["fields"].sort(key=lambda field: field["name"])
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_RUST_IDENTIFIER_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_reserved_struct_field_identifier(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        item = next(
            candidate
            for candidate in api["expected_graph"]["items"]
            if candidate["kind"] == "struct"
            and candidate["definition"]["fields"]
        )
        item["definition"]["fields"][0]["name"] = "type"
        item["definition"]["fields"].sort(key=lambda field: field["name"])
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_RUST_IDENTIFIER_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_duplicate_declared_enum_variant(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        item = next(
            item
            for item in api["expected_graph"]["items"]
            if item["id"] == "type:embed:OperationKind"
        )
        item["definition"]["variants"][1] = item["definition"]["variants"][0]
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_ITEM_DEFINITION_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_invalid_enum_variant_identifier(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        item = next(
            candidate
            for candidate in api["expected_graph"]["items"]
            if candidate["kind"] == "enum"
        )
        item["definition"]["variants"][0] = "_"
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_RUST_IDENTIFIER_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_reserved_enum_variant_identifier(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        item = next(
            candidate
            for candidate in api["expected_graph"]["items"]
            if candidate["kind"] == "enum"
        )
        item["definition"]["variants"][0] = "Self"
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_RUST_IDENTIFIER_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_invalid_semantic_variant_identifier(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        variants = api["expected_graph"]["semantic_algebras"][
            "AtomicAuthority"
        ]["variants"]
        variants[0]["name"] = "_"
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_RUST_IDENTIFIER_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_reserved_semantic_field_identifier(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        fields = api["expected_graph"]["semantic_algebras"]["Claim"]["fields"]
        fields[0] = "type"
        fields.sort()
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_RUST_IDENTIFIER_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_undeclared_retry_advice_variant(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        variants = api["expected_graph"]["semantic_algebras"][
            "UnavailableCause"
        ]["legal_variants"]
        variants[0]["retry"][0] = "Later"
        variants[0]["retry"].sort()
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn(
            "API_SEMANTIC_ALGEBRA_REFERENCE_INVALID", stderr.getvalue()
        )

    def test_verify_internal_rejects_undeclared_source_refusal_kind(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        variants = api["expected_graph"]["semantic_algebras"]["SourceRefusal"][
            "variants"
        ]
        variants[0]["name"] = "OtherRefusal"
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn(
            "API_SEMANTIC_ALGEBRA_REFERENCE_INVALID", stderr.getvalue()
        )

    def test_verify_internal_rejects_unsorted_representative_lexical_sets(
        self,
    ) -> None:
        paths = (
            ("configuration_domain", "cfg_keys", 0, "allowed_values"),
            ("configuration_domain", "targets", 0, "atomic_widths"),
            ("expected_graph", "semantic_algebras", "Claim", "fields"),
            ("policy", "associated_item_signatures", "required_fields"),
        )
        for path in paths:
            with self.subTest(path=path):
                fixture = RefreezeFixture()
                api = deepcopy(fixture.api)
                values = api
                for segment in path:
                    values = values[segment]
                values[0], values[1] = values[1], values[0]
                target = fixture.replace_public_api(api)
                stderr = io.StringIO()

                with redirect_stderr(stderr):
                    status = refreeze_v11.main(
                        [
                            "--root",
                            str(fixture.root),
                            "verify-internal",
                            "--target-ref",
                            target,
                        ]
                    )

                self.assertEqual(status, 1)
                self.assertIn("API_LEXICAL_SET_ORDER_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_deeply_nested_dangling_api_type_id(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["expected_graph"]["impls"][3]["associated_items"][3]["output"][
            "arguments"
        ][0]["arguments"][0]["id"] = "type:embed:DefinitelyMissing"
        target = fixture.replace_public_api(api)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_type_ast_discriminator_shape_swap(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["expected_graph"]["impls"][3]["associated_items"][3]["output"][
            "arguments"
        ][0]["arguments"][0]["kind"] = "path"
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                [
                    "--root",
                    str(fixture.root),
                    "verify-internal",
                    "--target-ref",
                    target,
                ]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_TYPE_AST_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_undeclared_type_ast_generic_binder(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["expected_graph"]["impls"][1]["associated_items"][4]["output"][
            "target"
        ]["binder"] = "T999"
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_TYPE_AST_BINDER_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_impl_generic_not_owned_by_type(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        implementation = api["expected_graph"]["impls"][1]
        implementation["generics"][0]["binder"] = "T1"
        implementation["associated_items"][4]["output"]["target"]["binder"] = "T1"
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_IMPL_GENERIC_BINDING_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_impl_id_not_bound_to_owner(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["expected_graph"]["impls"][0]["id"] = "impl:AtomicAuthorityX"
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_IMPL_ID_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_reserved_generic_binder(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        item = next(
            candidate
            for candidate in api["expected_graph"]["items"]
            if candidate["id"] == "type:embed:Claim"
        )
        implementation = next(
            candidate
            for candidate in api["expected_graph"]["impls"]
            if candidate["for"] == "type:embed:Claim"
        )
        item["generics"][0]["binder"] = "Self"
        implementation["generics"][0]["binder"] = "Self"
        implementation["associated_items"][4]["output"]["target"]["binder"] = (
            "Self"
        )
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_RUST_IDENTIFIER_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_invalid_function_input_identifier(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        function = next(
            item
            for item in api["expected_graph"]["functions"]
            if item["id"] == "function:server_api:run"
        )
        function["inputs"][0]["name"] = "_"
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_RUST_IDENTIFIER_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_invalid_method_input_identifier(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        implementation = next(
            candidate
            for candidate in api["expected_graph"]["impls"]
            if candidate["id"] == "impl:EmbeddedSourceHandle"
        )
        method = next(
            candidate
            for candidate in implementation["associated_items"]
            if candidate["id"] == "method:EmbeddedSourceHandle:search_symbols"
        )
        method["inputs"][1]["name"] = "_"
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_RUST_IDENTIFIER_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_api_type_without_required_arguments(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["expected_graph"]["functions"][0]["output"]["id"] = (
            "type:embed:Claim"
        )
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_TYPE_AST_ARITY_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_generic_arguments_for_nongeneric_api_type(
        self,
    ) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["expected_graph"]["impls"][3]["associated_items"][3]["output"][
            "arguments"
        ][0]["path"] = "symforge::embed::AtomicAuthority"
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_TYPE_AST_ARITY_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_local_api_type_encoded_as_path(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        function = next(
            item
            for item in api["expected_graph"]["functions"]
            if item["id"] == "function:server_api:run"
        )
        function["inputs"][0]["type"]["arguments"][0]["path"] = (
            "symforge::embed::Claim"
        )
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_TYPE_REFERENCE_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_relative_local_type_paths(self) -> None:
        for local_root in ("crate", "self", "super"):
            with self.subTest(local_root=local_root):
                fixture = RefreezeFixture()
                api = deepcopy(fixture.api)
                function = next(
                    item
                    for item in api["expected_graph"]["functions"]
                    if item["id"] == "function:server_api:run"
                )
                function["inputs"][0]["type"]["arguments"][0]["path"] = (
                    f"{local_root}::embed::Claim"
                )
                target = fixture.replace_public_api(api)
                stderr = io.StringIO()

                with redirect_stderr(stderr):
                    status = refreeze_v11.main(
                        [
                            "--root",
                            str(fixture.root),
                            "verify-internal",
                            "--target-ref",
                            target,
                        ]
                    )

                self.assertEqual(status, 1)
                self.assertIn("API_TYPE_REFERENCE_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_reserved_external_type_path_component(
        self,
    ) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        function = next(
            item
            for item in api["expected_graph"]["functions"]
            if item["id"] == "function:server_api:run"
        )
        function["inputs"][0]["type"]["arguments"][0]["path"] = (
            "std::ffi::type"
        )
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_RUST_PATH_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_local_value_encoded_as_generic_type(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["expected_graph"]["impls"][3]["associated_items"][3]["output"][
            "arguments"
        ][0]["path"] = "symforge::server_api::run"
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_TYPE_REFERENCE_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_relative_local_generic_paths(self) -> None:
        for local_root in ("crate", "self", "super"):
            with self.subTest(local_root=local_root):
                fixture = RefreezeFixture()
                api = deepcopy(fixture.api)
                api["expected_graph"]["impls"][3]["associated_items"][3][
                    "output"
                ]["arguments"][0]["path"] = f"{local_root}::embed::Claim"
                target = fixture.replace_public_api(api)
                stderr = io.StringIO()

                with redirect_stderr(stderr):
                    status = refreeze_v11.main(
                        [
                            "--root",
                            str(fixture.root),
                            "verify-internal",
                            "--target-ref",
                            target,
                        ]
                    )

                self.assertEqual(status, 1)
                self.assertIn("API_TYPE_REFERENCE_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_export_with_nonexistent_parent_module(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["expected_graph"]["exports"][0]["path"] = (
            "symforge::definitely_missing::AtomicAuthority"
        )
        target = fixture.replace_public_api(api)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_export_path_not_owned_by_item(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["expected_graph"]["exports"][0]["path"] = (
            "symforge::embed::DefinitelyMissing"
        )
        target = fixture.replace_public_api(api)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_export_availability_mismatch(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["expected_graph"]["exports"][0]["availability"] = "feature=server"
        target = fixture.replace_public_api(api)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_export_namespace_mismatch(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["expected_graph"]["exports"][0]["namespace"] = "value"
        target = fixture.replace_public_api(api)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_dangling_trait_impl_subject(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["expected_graph"]["trait_impls"][0]["for"] = (
            "symforge::embed::DefinitelyMissing"
        )
        target = fixture.replace_public_api(api)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_duplicate_direct_trait_edge(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["expected_graph"]["trait_impls"][1] = deepcopy(
            api["expected_graph"]["trait_impls"][0]
        )
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_TRAIT_IMPL_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_opposing_direct_trait_polarities(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        trait_impls = api["expected_graph"]["trait_impls"]
        opposing = deepcopy(trait_impls[0])
        opposing["polarity"] = "negative"
        trait_impls.append(opposing)
        trait_impls.sort(
            key=lambda item: (item["for"], item["trait"], item["polarity"])
        )
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_TRAIT_IMPL_CONTRADICTION", stderr.getvalue())

    def test_verify_internal_rejects_direct_auto_trait_contradiction(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        trait_impls = api["expected_graph"]["trait_impls"]
        trait_impls.append(
            {
                "for": "symforge::embed::EmbeddedSourceHandle",
                "polarity": "positive",
                "trait": "std::panic::RefUnwindSafe",
            }
        )
        trait_impls.sort(
            key=lambda item: (item["for"], item["trait"], item["polarity"])
        )
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_TRAIT_IMPL_CONTRADICTION", stderr.getvalue())

    def test_verify_internal_rejects_noncanonical_direct_trait_path(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        trait_impls = api["expected_graph"]["trait_impls"]
        trait_impls.append(
            {
                "for": "symforge::embed::EmbeddedSourceHandle",
                "polarity": "positive",
                "trait": "::std::panic::RefUnwindSafe",
            }
        )
        trait_impls.sort(
            key=lambda item: (item["for"], item["trait"], item["polarity"])
        )
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_TRAIT_PATH_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_undeclared_upstream_trait(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        trait_impls = api["expected_graph"]["trait_impls"]
        trait_impls.append(
            {
                "for": "symforge::embed::EmbeddedSourceHandle",
                "polarity": "positive",
                "trait": "core::hash::Hash",
            }
        )
        trait_impls.sort(
            key=lambda item: (item["for"], item["trait"], item["polarity"])
        )
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_TRAIT_REFERENCE_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_reserved_direct_trait_path_component(
        self,
    ) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        trait_impls = api["expected_graph"]["trait_impls"]
        trait_impls.append(
            {
                "for": "symforge::embed::EmbeddedSourceHandle",
                "polarity": "positive",
                "trait": "std::type::RefUnwindSafe",
            }
        )
        trait_impls.sort(
            key=lambda item: (item["for"], item["trait"], item["polarity"])
        )
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_TRAIT_PATH_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_unsorted_direct_trait_edges(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        trait_impls = api["expected_graph"]["trait_impls"]
        trait_impls[0], trait_impls[1] = trait_impls[1], trait_impls[0]
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_TRAIT_IMPL_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_unknown_trait_polarity(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["expected_graph"]["trait_impls"][0]["polarity"] = "unknown"
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_TRAIT_IMPL_INVALID", stderr.getvalue())

    def test_verify_internal_binds_complete_direct_trait_expectations(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        trait_impls = api["expected_graph"]["trait_impls"]
        copy_edge = next(
            edge
            for edge in trait_impls
            if edge["for"] == "symforge::embed::EngineInfo"
            and edge["trait"] == "core::marker::Copy"
        )
        copy_edge["trait"] = "core::default::Default"
        trait_impls.sort(
            key=lambda item: (item["for"], item["trait"], item["polarity"])
        )
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_TRAIT_IMPL_EXPECTATION_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_dangling_auto_trait_subject(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["expected_graph"]["auto_traits"]["expectation_groups"][0][
            "subjects"
        ][0] = "symforge::embed::DefinitelyMissing"
        target = fixture.replace_public_api(api)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_unknown_auto_trait_state(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        states = api["expected_graph"]["auto_traits"]["expectation_groups"][0][
            "states"
        ]
        states[next(iter(states))] = "maybe"
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_AUTO_TRAIT_STATE_INVALID", stderr.getvalue())

    def test_verify_internal_binds_complete_auto_trait_expectations(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["expected_graph"]["auto_traits"]["expectation_groups"][0]["states"][
            "core::marker::Send"
        ] = "negative"
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_AUTO_TRAIT_EXPECTATION_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_conditional_auto_trait_for_nongeneric_type(
        self,
    ) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["expected_graph"]["auto_traits"]["expectation_groups"][0]["states"][
            "core::marker::Send"
        ] = "conditional:T"
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_AUTO_TRAIT_CONDITIONAL_BINDING_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_noncanonical_auto_trait_path(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        universe = api["expected_graph"]["auto_traits"]["universe"]
        universe[universe.index("core::marker::Send")] = "::core::marker::Send"
        universe.sort()
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_TRAIT_PATH_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_dangling_negative_assertion_subject(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["negative_assertions"][0]["subjects"][0] = (
            "symforge::embed::DefinitelyMissing"
        )
        target = fixture.replace_public_api(api)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_noncanonical_negative_generic_placeholder(
        self,
    ) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        assertion = next(
            candidate
            for candidate in api["negative_assertions"]
            if candidate["id"] == "authority-types-01-not-deserialize"
        )
        claim = "symforge::embed::Claim<T>"
        assertion["subjects"][assertion["subjects"].index(claim)] = (
            "symforge::embed::Claim<U>"
        )
        assertion["subjects"].sort()
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_NEGATIVE_ASSERTION_SUBJECT_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_nonreflexive_negative_assertion_exception(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        assertion = next(
            item
            for item in api["negative_assertions"]
            if item["id"] == "authority-types-03-not-from"
        )
        assertion["permitted_source"] = "Anything"
        target = fixture.replace_public_api(api)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_reflexive_exception_on_wrong_assertion_kind(
        self,
    ) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        assertion = next(
            item
            for item in api["negative_assertions"]
            if item["id"] == "authority-types-03-not-from"
        )
        assertion["kind"] = "impl-family-absent"
        target = fixture.replace_public_api(api)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_duplicate_negative_assertion_inventory(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        assertions = api["negative_assertions"]
        source = next(
            item for item in assertions if item["id"] == "authority-types-01-not-deserialize"
        )
        target_index = next(
            index
            for index, item in enumerate(assertions)
            if item["id"] == "embedded-source-handle-not-clone"
        )
        assertions[target_index] = deepcopy(source)
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_NEGATIVE_ASSERTION_INVENTORY_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_unknown_negative_assertion_kind(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["negative_assertions"][0]["kind"] = "unknown-assertion"
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_NEGATIVE_ASSERTION_KIND_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_subjectless_assertion_for_subject_id(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        assertions = api["negative_assertions"]
        graph_assertion = next(
            item
            for item in assertions
            if item["id"] == "cbm-spike-no-public-graph-delta"
        )
        target_index = next(
            index
            for index, item in enumerate(assertions)
            if item["id"] == "authority-types-01-not-deserialize"
        )
        assertions[target_index] = deepcopy(graph_assertion)
        assertions[target_index]["id"] = "authority-types-01-not-deserialize"
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_NEGATIVE_ASSERTION_FIELDS_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_forbidden_prefix_that_is_public(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        assertion = next(
            item
            for item in api["negative_assertions"]
            if item["id"] == "no-rust-health-api"
        )
        assertion["forbidden_prefixes"][0] = "symforge::embed"
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_NEGATIVE_ASSERTION_CONTRADICTION", stderr.getvalue())

    def test_verify_internal_rejects_noncanonical_forbidden_prefix(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        assertion = next(
            candidate
            for candidate in api["negative_assertions"]
            if candidate["id"] == "no-rust-health-api"
        )
        assertion["forbidden_prefixes"][0] = "::symforge::health"
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_NEGATIVE_ASSERTION_FIELDS_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_negative_trait_with_positive_edge(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        trait_impls = api["expected_graph"]["trait_impls"]
        trait_impls.append(
            {
                "for": "symforge::embed::ProcessIndexRuntime",
                "polarity": "positive",
                "trait": "core::ops::Deref",
            }
        )
        trait_impls.sort(
            key=lambda item: (item["for"], item["trait"], item["polarity"])
        )
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_NEGATIVE_ASSERTION_CONTRADICTION", stderr.getvalue())

    def test_verify_internal_binds_negative_assertion_id_to_exact_trait(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        assertion = next(
            candidate
            for candidate in api["negative_assertions"]
            if candidate["id"] == "embedded-source-handle-not-clone"
        )
        assertion["trait"] = "core::hash::Hash"
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn(
            "API_NEGATIVE_ASSERTION_SEMANTICS_INVALID", stderr.getvalue()
        )

    def test_verify_internal_binds_negative_assertion_id_to_exact_subjects(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        assertion = next(
            candidate
            for candidate in api["negative_assertions"]
            if candidate["id"] == "authority-types-01-not-deserialize"
        )
        assertion["subjects"][0] = "symforge::embed::EngineInfo"
        assertion["subjects"].sort()
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn(
            "API_NEGATIVE_ASSERTION_SEMANTICS_INVALID", stderr.getvalue()
        )

    def test_verify_internal_binds_negative_assertion_id_to_exact_prefixes(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        assertion = next(
            candidate
            for candidate in api["negative_assertions"]
            if candidate["id"] == "no-rust-health-api"
        )
        assertion["forbidden_prefixes"][0] = "symforge::future_health"
        assertion["forbidden_prefixes"].sort()
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn(
            "API_NEGATIVE_ASSERTION_SEMANTICS_INVALID", stderr.getvalue()
        )

    def test_verify_internal_rejects_negative_trait_with_positive_auto_edge(
        self,
    ) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        assertion = next(
            item
            for item in api["negative_assertions"]
            if item["id"] == "embedded-source-handle-not-clone"
        )
        assertion["trait"] = "core::marker::Send"
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn(
            "API_NEGATIVE_ASSERTION_SEMANTICS_INVALID", stderr.getvalue()
        )

    def test_verify_internal_rejects_noncanonical_negative_trait_path(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        assertion = next(
            item
            for item in api["negative_assertions"]
            if item["id"] == "embedded-source-handle-not-clone"
        )
        assertion["trait"] = "::core::marker::Send"
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn(
            "API_NEGATIVE_ASSERTION_SEMANTICS_INVALID", stderr.getvalue()
        )

    def test_verify_internal_rejects_unknown_graph_equivalence_feature_vector(
        self,
    ) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        assertion = next(
            item
            for item in api["negative_assertions"]
            if item["id"] == "cbm-spike-no-public-graph-delta"
        )
        assertion["pairs"][0][1] = "definitely-missing-vector"
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_NEGATIVE_ASSERTION_PAIR_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_false_known_graph_equivalence_pair(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        assertion = next(
            item
            for item in api["negative_assertions"]
            if item["id"] == "cbm-spike-no-public-graph-delta"
        )
        assertion["pairs"][0] = ["embed", "server"]
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn(
            "API_NEGATIVE_ASSERTION_PAIR_COVERAGE_INVALID", stderr.getvalue()
        )

    def test_verify_internal_rejects_dishonest_pre_activation_api_state(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["observed_graph"]["status"] = "ready"
        target = fixture.replace_public_api(api)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_unpinned_associated_method_signature(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["expected_graph"]["impls"][0]["associated_items"][0][
            "effective_signature"
        ]["async"] = True
        target = fixture.replace_public_api(api)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_mismatched_associated_method_id(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["expected_graph"]["impls"][0]["associated_items"][0]["id"] = (
            "method:AtomicAuthority:identity_x"
        )
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_ASSOCIATED_ITEM_ID_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_invalid_associated_method_name(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        implementation = api["expected_graph"]["impls"][0]
        associated_item = implementation["associated_items"][0]
        associated_item["id"] = "method:AtomicAuthority:identity-hyphen"
        associated_item["name"] = "identity-hyphen"
        implementation["associated_items"].sort(key=lambda item: item["id"])
        introduced = api["migration_v10"]["introduced_v11_atoms"]
        introduced[introduced.index("symforge::embed::AtomicAuthority::identity")] = (
            "symforge::embed::AtomicAuthority::identity-hyphen"
        )
        introduced.sort()
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_ASSOCIATED_ITEM_NAME_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_duplicate_method_name_for_owner(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        implementation = api["expected_graph"]["impls"][0]
        implementation["associated_items"][0]["name"] = implementation[
            "associated_items"
        ][1]["name"]
        api["migration_v10"]["introduced_v11_atoms"].remove(
            "symforge::embed::AtomicAuthority::identity"
        )
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_ASSOCIATED_ITEM_DUPLICATE", stderr.getvalue())

    def test_verify_internal_requires_v10_crate_root_mapping(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["migration_v10"]["categories"] = [
            item
            for item in api["migration_v10"]["categories"]
            if item["id"] != "v10-00-crate-root"
        ]
        target = fixture.replace_public_api(api)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_overlapping_v10_migration_atoms(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["migration_v10"]["categories"][2]["atoms"][0] = "symforge"
        target = fixture.replace_public_api(api)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_dangling_v11_migration_atom(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["migration_v10"]["categories"][3]["v11_atoms"][0] = (
            "symforge::embed::DefinitelyMissing"
        )
        target = fixture.replace_public_api(api)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_missing_introduced_v11_atom(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["migration_v10"]["introduced_v11_atoms"].pop()
        target = fixture.replace_public_api(api)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_missing_introduced_associated_method(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        api["migration_v10"]["introduced_v11_atoms"].remove(
            "symforge::embed::AtomicAuthority::identity"
        )
        target = fixture.replace_public_api(api)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_keep_relabel_without_atom_identity(self) -> None:
        fixture = RefreezeFixture()
        api = deepcopy(fixture.api)
        category = next(
            item
            for item in api["migration_v10"]["categories"]
            if item["id"] == "v10-04-raw-runtime-state"
        )
        category["decision"] = "keep"
        replacement_atoms = set(category["v11_atoms"])
        api["migration_v10"]["introduced_v11_atoms"] = [
            atom
            for atom in api["migration_v10"]["introduced_v11_atoms"]
            if atom not in replacement_atoms
        ]
        target = fixture.replace_public_api(api)
        stderr = io.StringIO()

        with redirect_stderr(stderr):
            status = refreeze_v11.main(
                [
                    "--root",
                    str(fixture.root),
                    "verify-internal",
                    "--target-ref",
                    target,
                ]
            )

        self.assertEqual(status, 1)
        self.assertIn("API_MIGRATION_KEEP_IDENTITY_INVALID", stderr.getvalue())

    def test_verify_internal_rejects_detached_attestation_inconsistency(self) -> None:
        fixture = RefreezeFixture()
        attestation = deepcopy(fixture.attestation)
        attestation["design"]["sha256"] = "0" * 64
        fixture.write(
            ATTESTATION_PATH,
            sentinel_document(ATTESTATION_START, ATTESTATION_END, attestation),
        )
        target = fixture.commit_all("drift detached attestation")

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_requires_normative_classification_for_required_paths(
        self,
    ) -> None:
        fixture = RefreezeFixture()
        manifest = deepcopy(fixture.manifest)
        goal = next(
            item
            for item in manifest["inventory"]
            if item["path"] == f"{FEATURE_ROOT}/GOAL.md"
        )
        goal["classification"] = "supporting_evidence"
        target = fixture.reseal_manifest(manifest)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_fails_closed_on_wrong_typed_classification(self) -> None:
        fixture = RefreezeFixture()
        manifest = deepcopy(fixture.manifest)
        goal = next(
            item
            for item in manifest["inventory"]
            if item["path"] == f"{FEATURE_ROOT}/GOAL.md"
        )
        goal["classification"] = []
        target = fixture.reseal_manifest(manifest)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_inventory_path_traversal(self) -> None:
        fixture = RefreezeFixture()
        manifest = deepcopy(fixture.manifest)
        manifest["inventory"][0]["path"] = "../CONTEXT.md"
        target = fixture.reseal_manifest(manifest)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_unresolved_requirement_mapping(self) -> None:
        fixture = RefreezeFixture()
        manifest = deepcopy(fixture.manifest)
        manifest["amendments"][0]["requirement_ids"] = ["FR-999"]
        target = fixture.reseal_manifest(manifest)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_ambiguous_requirement_definition(self) -> None:
        fixture = RefreezeFixture()
        manifest = deepcopy(fixture.manifest)
        goal_path = f"{FEATURE_ROOT}/GOAL.md"
        fixture.write(goal_path, b"**F020-V11-A01** duplicate definition\n")
        goal_entry = next(
            item for item in manifest["inventory"] if item["path"] == goal_path
        )
        goal_entry["sha256"] = sha256(fixture.read(goal_path))
        target = fixture.reseal_manifest(manifest)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_unresolved_contract_heading(self) -> None:
        fixture = RefreezeFixture()
        manifest = deepcopy(fixture.manifest)
        manifest["amendments"][0]["contract_clause_ids"] = [
            "contracts/source-binding-and-state.md#missing-heading"
        ]
        target = fixture.reseal_manifest(manifest)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_ambiguous_contract_heading(self) -> None:
        fixture = RefreezeFixture()
        manifest = deepcopy(fixture.manifest)
        contract_path = f"{FEATURE_ROOT}/contracts/source-binding-and-state.md"
        fixture.write(
            contract_path,
            fixture.read(contract_path) + b"\n# v11-lifecycle-amendment\n",
        )
        contract_entry = next(
            item for item in manifest["inventory"] if item["path"] == contract_path
        )
        contract_entry["sha256"] = sha256(fixture.read(contract_path))
        target = fixture.reseal_manifest(manifest)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_unresolved_plan_task(self) -> None:
        fixture = RefreezeFixture()
        manifest = deepcopy(fixture.manifest)
        manifest["amendments"][0]["plan_task_ids"] = ["T999"]
        target = fixture.reseal_manifest(manifest)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_internal_rejects_unresolved_regression_id(self) -> None:
        fixture = RefreezeFixture()
        manifest = deepcopy(fixture.manifest)
        manifest["amendments"][0]["regression_ids"] = ["F020-V11-R99"]
        manifest["amendment_set_id"] = compute_amendment_set_id(
            manifest["amendments"]
        )
        target = fixture.reseal_manifest(manifest)

        status = refreeze_v11.main(
            ["--root", str(fixture.root), "verify-internal", "--target-ref", target]
        )

        self.assertEqual(status, 1)

    def test_verify_approval_rejects_an_old_record_for_a_successor_target(self) -> None:
        fixture = RefreezeFixture()
        (
            _approval,
            approval_path,
            signature_path,
            allowed_signers_path,
            release_identity,
        ) = fixture.make_external_approval()
        manifest = deepcopy(fixture.manifest)
        fixture.write(DESIGN_PATH, b"coordinated successor design\n")
        manifest["design"]["sha256"] = sha256(fixture.read(DESIGN_PATH))
        successor = fixture.reseal_manifest(manifest)
        signature_called = False

        def accept_signature(*_args: object, **_kwargs: object) -> bool:
            nonlocal signature_called
            signature_called = True
            return True

        with self.assertRaises(refreeze_v11.RefreezeError):
            refreeze_v11.verify_approval(
                fixture.root,
                target_ref=successor,
                approval_record=approval_path,
                approval_signature=signature_path,
                allowed_signers=allowed_signers_path,
                git_executable=SYSTEM_GIT_EXECUTABLE,
                ssh_keygen_executable=SYSTEM_SSH_KEYGEN_EXECUTABLE,
                expected_release_identity=release_identity,
                expected_repository="special-place-ai-heaven/symforge",
                signature_verifier=accept_signature,
            )
        self.assertFalse(signature_called)

    def test_verify_approval_accepts_exact_external_record_with_mocked_signature(
        self,
    ) -> None:
        fixture = RefreezeFixture()
        (
            _approval,
            approval_path,
            signature_path,
            allowed_signers_path,
            release_identity,
        ) = fixture.make_external_approval()
        calls: list[tuple[bytes, dict[str, object]]] = []

        def accept_signature(message: bytes, **kwargs: object) -> bool:
            calls.append((message, kwargs))
            return True

        result = refreeze_v11.verify_approval(
            fixture.root,
            target_ref=fixture.target_commit,
            approval_record=approval_path,
            approval_signature=signature_path,
            allowed_signers=allowed_signers_path,
            git_executable=SYSTEM_GIT_EXECUTABLE,
            ssh_keygen_executable=SYSTEM_SSH_KEYGEN_EXECUTABLE,
            expected_release_identity=release_identity,
            expected_repository="special-place-ai-heaven/symforge",
            signature_verifier=accept_signature,
        )

        self.assertEqual(result.target_commit, fixture.target_commit)
        self.assertEqual(len(calls), 1)
        self.assertEqual(calls[0][0], approval_path.read_bytes())
        self.assertEqual(calls[0][1]["signature"], signature_path.resolve())
        self.assertEqual(
            calls[0][1]["allowed_signers"], allowed_signers_path.resolve()
        )
        self.assertEqual(calls[0][1]["release_identity"], release_identity)

    def test_verify_approval_rejects_noncanonical_bytes_before_signature(self) -> None:
        fixture = RefreezeFixture()
        (
            _approval,
            approval_path,
            signature_path,
            allowed_signers_path,
            release_identity,
        ) = fixture.make_external_approval()
        approval_path.write_bytes(approval_path.read_bytes() + b"\n")
        signature_called = False

        def accept_signature(*_args: object, **_kwargs: object) -> bool:
            nonlocal signature_called
            signature_called = True
            return True

        with self.assertRaisesRegex(
            refreeze_v11.RefreezeError, "APPROVAL_NOT_CANONICAL"
        ):
            refreeze_v11.verify_approval(
                fixture.root,
                target_ref=fixture.target_commit,
                approval_record=approval_path,
                approval_signature=signature_path,
                allowed_signers=allowed_signers_path,
                git_executable=SYSTEM_GIT_EXECUTABLE,
                ssh_keygen_executable=SYSTEM_SSH_KEYGEN_EXECUTABLE,
                expected_release_identity=release_identity,
                expected_repository="special-place-ai-heaven/symforge",
                signature_verifier=accept_signature,
            )
        self.assertFalse(signature_called)

    def test_verify_approval_rejects_trust_material_inside_repository(self) -> None:
        fixture = RefreezeFixture()
        approval, *_rest, release_identity = fixture.make_external_approval()
        approval_path = fixture.root / "approval.json"
        signature_path = fixture.root / "approval.json.sshsig"
        allowed_signers_path = fixture.root / "allowed_signers"
        approval_path.write_bytes(canonical_json(approval))
        signature_path.write_bytes(b"inside repository\n")
        allowed_signers_path.write_bytes(b"inside repository\n")

        with self.assertRaisesRegex(
            refreeze_v11.RefreezeError, "TRUST_FILE_INSIDE_REPOSITORY"
        ):
            refreeze_v11.verify_approval(
                fixture.root,
                target_ref=fixture.target_commit,
                approval_record=approval_path,
                approval_signature=signature_path,
                allowed_signers=allowed_signers_path,
                git_executable=SYSTEM_GIT_EXECUTABLE,
                ssh_keygen_executable=SYSTEM_SSH_KEYGEN_EXECUTABLE,
                expected_release_identity=release_identity,
                expected_repository="special-place-ai-heaven/symforge",
                signature_verifier=lambda *_args, **_kwargs: True,
            )

    def test_verify_approval_rejects_tree_or_attestation_drift_before_signature(
        self,
    ) -> None:
        fixture = RefreezeFixture()
        (
            approval,
            approval_path,
            signature_path,
            allowed_signers_path,
            release_identity,
        ) = fixture.make_external_approval()
        signature_called = False

        def accept_signature(*_args: object, **_kwargs: object) -> bool:
            nonlocal signature_called
            signature_called = True
            return True

        for field in ("tree", "attestation"):
            with self.subTest(field=field):
                drifted = deepcopy(approval)
                if field == "tree":
                    drifted["target_tree"] = "0" * len(fixture.target_tree)
                else:
                    drifted["attestation"]["sha256"] = "0" * 64
                approval_path.write_bytes(canonical_json(drifted))
                with self.assertRaises(refreeze_v11.RefreezeError):
                    refreeze_v11.verify_approval(
                        fixture.root,
                        target_ref=fixture.target_commit,
                        approval_record=approval_path,
                        approval_signature=signature_path,
                        allowed_signers=allowed_signers_path,
                        git_executable=SYSTEM_GIT_EXECUTABLE,
                        ssh_keygen_executable=SYSTEM_SSH_KEYGEN_EXECUTABLE,
                        expected_release_identity=release_identity,
                        expected_repository="special-place-ai-heaven/symforge",
                        signature_verifier=accept_signature,
                    )
        self.assertFalse(signature_called)

    def test_verify_approval_rejects_broken_append_only_predecessor_rule(self) -> None:
        fixture = RefreezeFixture()
        (
            approval,
            approval_path,
            signature_path,
            allowed_signers_path,
            release_identity,
        ) = fixture.make_external_approval()
        approval["sequence"] = 2
        approval_path.write_bytes(canonical_json(approval))

        with self.assertRaisesRegex(
            refreeze_v11.RefreezeError, "APPROVAL_PREDECESSOR_INVALID"
        ):
            refreeze_v11.verify_approval(
                fixture.root,
                target_ref=fixture.target_commit,
                approval_record=approval_path,
                approval_signature=signature_path,
                allowed_signers=allowed_signers_path,
                git_executable=SYSTEM_GIT_EXECUTABLE,
                ssh_keygen_executable=SYSTEM_SSH_KEYGEN_EXECUTABLE,
                expected_release_identity=release_identity,
                expected_repository="special-place-ai-heaven/symforge",
                signature_verifier=lambda *_args, **_kwargs: True,
            )

    def test_verify_approval_rejects_forged_zero_predecessor_without_history(
        self,
    ) -> None:
        fixture = RefreezeFixture()
        (
            approval,
            approval_path,
            signature_path,
            allowed_signers_path,
            release_identity,
        ) = fixture.make_external_approval()
        approval["sequence"] = 2
        approval["predecessor_digest"] = "0" * 64
        approval_path.write_bytes(canonical_json(approval))

        with self.assertRaises(refreeze_v11.RefreezeError):
            refreeze_v11.verify_approval(
                fixture.root,
                target_ref=fixture.target_commit,
                approval_record=approval_path,
                approval_signature=signature_path,
                allowed_signers=allowed_signers_path,
                git_executable=SYSTEM_GIT_EXECUTABLE,
                ssh_keygen_executable=SYSTEM_SSH_KEYGEN_EXECUTABLE,
                expected_release_identity=release_identity,
                expected_repository="special-place-ai-heaven/symforge",
                signature_verifier=lambda *_args, **_kwargs: True,
            )

    def test_verify_approval_accepts_complete_signed_append_only_history(self) -> None:
        fixture = RefreezeFixture()
        (
            approval,
            approval_path,
            signature_path,
            allowed_signers_path,
            release_identity,
        ) = fixture.make_external_approval()
        predecessor_bytes = approval_path.read_bytes()
        predecessor_digest = sha256(predecessor_bytes)
        history_dir = approval_path.parent / "history"
        history_dir.mkdir()
        (history_dir / f"{predecessor_digest}.json").write_bytes(predecessor_bytes)
        (history_dir / f"{predecessor_digest}.json.sig").write_bytes(
            b"predecessor signature boundary\n"
        )
        approval["approved_at"] = "2026-08-11T12:01:00Z"
        approval["sequence"] = 2
        approval["predecessor_digest"] = predecessor_digest
        current_bytes = canonical_json(approval)
        approval_path.write_bytes(current_bytes)
        signed_messages: list[bytes] = []

        def accept_signature(message: bytes, **_kwargs: object) -> bool:
            signed_messages.append(message)
            return True

        result = refreeze_v11.verify_approval(
            fixture.root,
            target_ref=fixture.target_commit,
            approval_record=approval_path,
            approval_signature=signature_path,
            approval_history_dir=history_dir,
            allowed_signers=allowed_signers_path,
            git_executable=SYSTEM_GIT_EXECUTABLE,
            ssh_keygen_executable=SYSTEM_SSH_KEYGEN_EXECUTABLE,
            expected_release_identity=release_identity,
            expected_repository="special-place-ai-heaven/symforge",
            signature_verifier=accept_signature,
        )

        self.assertEqual(result.target_commit, fixture.target_commit)
        self.assertEqual(signed_messages, [current_bytes, predecessor_bytes])

    def test_verify_approval_rejects_forged_zero_predecessor_file(self) -> None:
        fixture = RefreezeFixture()
        (
            approval,
            approval_path,
            signature_path,
            allowed_signers_path,
            release_identity,
        ) = fixture.make_external_approval()
        predecessor_bytes = approval_path.read_bytes()
        history_dir = approval_path.parent / "history"
        history_dir.mkdir()
        zero_digest = "0" * 64
        (history_dir / f"{zero_digest}.json").write_bytes(predecessor_bytes)
        (history_dir / f"{zero_digest}.json.sig").write_bytes(b"forged signature\n")
        approval["sequence"] = 2
        approval["predecessor_digest"] = zero_digest
        approval_path.write_bytes(canonical_json(approval))

        with self.assertRaisesRegex(
            refreeze_v11.RefreezeError, "APPROVAL_HISTORY_DIGEST_MISMATCH"
        ):
            refreeze_v11.verify_approval(
                fixture.root,
                target_ref=fixture.target_commit,
                approval_record=approval_path,
                approval_signature=signature_path,
                approval_history_dir=history_dir,
                allowed_signers=allowed_signers_path,
                git_executable=SYSTEM_GIT_EXECUTABLE,
                ssh_keygen_executable=SYSTEM_SSH_KEYGEN_EXECUTABLE,
                expected_release_identity=release_identity,
                expected_repository="special-place-ai-heaven/symforge",
                signature_verifier=lambda *_args, **_kwargs: True,
            )

    def test_verify_approval_rejects_history_store_discontinuity(self) -> None:
        fixture = RefreezeFixture()
        (
            approval,
            approval_path,
            signature_path,
            allowed_signers_path,
            release_identity,
        ) = fixture.make_external_approval()
        predecessor = deepcopy(approval)
        predecessor["store_locator"] = "append-only://different-store/1"
        predecessor_bytes = canonical_json(predecessor)
        predecessor_digest = sha256(predecessor_bytes)
        history_dir = approval_path.parent / "history"
        history_dir.mkdir()
        (history_dir / f"{predecessor_digest}.json").write_bytes(predecessor_bytes)
        (history_dir / f"{predecessor_digest}.json.sig").write_bytes(
            b"predecessor signature boundary\n"
        )
        approval["sequence"] = 2
        approval["predecessor_digest"] = predecessor_digest
        approval_path.write_bytes(canonical_json(approval))

        with self.assertRaisesRegex(
            refreeze_v11.RefreezeError, "APPROVAL_HISTORY_CONTINUITY_MISMATCH"
        ):
            refreeze_v11.verify_approval(
                fixture.root,
                target_ref=fixture.target_commit,
                approval_record=approval_path,
                approval_signature=signature_path,
                approval_history_dir=history_dir,
                allowed_signers=allowed_signers_path,
                git_executable=SYSTEM_GIT_EXECUTABLE,
                ssh_keygen_executable=SYSTEM_SSH_KEYGEN_EXECUTABLE,
                expected_release_identity=release_identity,
                expected_repository="special-place-ai-heaven/symforge",
                signature_verifier=lambda *_args, **_kwargs: True,
            )

    def test_verify_approval_rejects_history_directory_inside_repository(self) -> None:
        fixture = RefreezeFixture()
        (
            approval,
            approval_path,
            signature_path,
            allowed_signers_path,
            release_identity,
        ) = fixture.make_external_approval()
        approval["sequence"] = 2
        approval["predecessor_digest"] = "0" * 64
        approval_path.write_bytes(canonical_json(approval))
        history_dir = fixture.root / "approval-history"
        history_dir.mkdir()

        with self.assertRaisesRegex(
            refreeze_v11.RefreezeError, "TRUST_FILE_INSIDE_REPOSITORY"
        ):
            refreeze_v11.verify_approval(
                fixture.root,
                target_ref=fixture.target_commit,
                approval_record=approval_path,
                approval_signature=signature_path,
                approval_history_dir=history_dir,
                allowed_signers=allowed_signers_path,
                git_executable=SYSTEM_GIT_EXECUTABLE,
                ssh_keygen_executable=SYSTEM_SSH_KEYGEN_EXECUTABLE,
                expected_release_identity=release_identity,
                expected_repository="special-place-ai-heaven/symforge",
                signature_verifier=lambda *_args, **_kwargs: True,
            )

    def test_verify_approval_rejects_control_characters_in_release_identity(self) -> None:
        fixture = RefreezeFixture()
        (
            approval,
            approval_path,
            signature_path,
            allowed_signers_path,
            _release_identity,
        ) = fixture.make_external_approval()
        invalid_identity = "release-ci\ninvalid"
        approval["release_identity"] = invalid_identity
        approval_path.write_bytes(canonical_json(approval))

        with self.assertRaisesRegex(
            refreeze_v11.RefreezeError, "APPROVAL_IDENTITY_INVALID"
        ):
            refreeze_v11.verify_approval(
                fixture.root,
                target_ref=fixture.target_commit,
                approval_record=approval_path,
                approval_signature=signature_path,
                allowed_signers=allowed_signers_path,
                git_executable=SYSTEM_GIT_EXECUTABLE,
                ssh_keygen_executable=SYSTEM_SSH_KEYGEN_EXECUTABLE,
                expected_release_identity=invalid_identity,
                expected_repository="special-place-ai-heaven/symforge",
                signature_verifier=lambda *_args, **_kwargs: True,
            )

    def test_verify_approval_rejects_a_failed_signature(self) -> None:
        fixture = RefreezeFixture()
        (
            _approval,
            approval_path,
            signature_path,
            allowed_signers_path,
            release_identity,
        ) = fixture.make_external_approval()

        with self.assertRaisesRegex(
            refreeze_v11.RefreezeError, "APPROVAL_SIGNATURE_INVALID"
        ):
            refreeze_v11.verify_approval(
                fixture.root,
                target_ref=fixture.target_commit,
                approval_record=approval_path,
                approval_signature=signature_path,
                allowed_signers=allowed_signers_path,
                git_executable=SYSTEM_GIT_EXECUTABLE,
                ssh_keygen_executable=SYSTEM_SSH_KEYGEN_EXECUTABLE,
                expected_release_identity=release_identity,
                expected_repository="special-place-ai-heaven/symforge",
                signature_verifier=lambda *_args, **_kwargs: False,
            )

    def test_sshsig_invocation_uses_argv_stdin_and_no_shell(self) -> None:
        completed = subprocess.CompletedProcess(
            [str(SYSTEM_SSH_KEYGEN_EXECUTABLE)], 0
        )
        with patch.object(refreeze_v11.subprocess, "run", return_value=completed) as run:
            accepted = refreeze_v11.verify_sshsig(
                b"canonical approval bytes",
                ssh_keygen_executable=SYSTEM_SSH_KEYGEN_EXECUTABLE,
                signature=Path("external.sshsig"),
                allowed_signers=Path("external.allowed_signers"),
                release_identity="release-ci@example.invalid",
            )

        self.assertTrue(accepted)
        argv = run.call_args.args[0]
        kwargs = run.call_args.kwargs
        self.assertEqual(
            argv[0:4],
            [str(SYSTEM_SSH_KEYGEN_EXECUTABLE), "-Y", "verify", "-f"],
        )
        self.assertEqual(kwargs["input"], b"canonical approval bytes")
        self.assertIs(kwargs["shell"], False)
        self.assertIs(kwargs["stdout"], subprocess.DEVNULL)
        self.assertIs(kwargs["stderr"], subprocess.DEVNULL)

    def test_cli_failure_never_echoes_external_record_material(self) -> None:
        fixture = RefreezeFixture()
        (
            approval,
            approval_path,
            signature_path,
            allowed_signers_path,
            _release_identity,
        ) = fixture.make_external_approval()
        marker = "EXTERNAL_RECORD_MATERIAL_MARKER"
        approval["release_identity"] = marker
        approval["store_locator"] = f"append-only://{marker}"
        approval_path.write_bytes(canonical_json(approval))
        stdout = io.StringIO()
        stderr = io.StringIO()

        with redirect_stdout(stdout), redirect_stderr(stderr):
            status = refreeze_v11.main(
                [
                    "--root",
                    str(fixture.root),
                    "--git-executable",
                    str(SYSTEM_GIT_EXECUTABLE),
                    "verify-approval",
                    "--target-ref",
                    fixture.target_commit,
                    "--approval-record",
                    str(approval_path),
                    "--approval-signature",
                    str(signature_path),
                    "--allowed-signers",
                    str(allowed_signers_path),
                    "--ssh-keygen-executable",
                    str(SYSTEM_SSH_KEYGEN_EXECUTABLE),
                    "--expected-release-identity",
                    "different-release-identity@example.invalid",
                    "--expected-repository",
                    "special-place-ai-heaven/symforge",
                ]
            )

        self.assertEqual(status, 1)
        self.assertNotIn(marker, stdout.getvalue())
        self.assertNotIn(marker, stderr.getvalue())


if __name__ == "__main__":
    unittest.main()
