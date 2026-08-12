#!/usr/bin/env python
"""Verify the externally anchored Feature 020 V11 refreeze gate."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import unicodedata
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path, PurePosixPath
from typing import Callable, Sequence


FEATURE_ROOT = "specs/020-repository-knowledge-index"
MANIFEST_PATH = f"{FEATURE_ROOT}/REFREEZE-MANIFEST-v11.md"
ATTESTATION_PATH = "docs/reviews/FEATURE-020-REFREEZE-ATTESTATION-v11.md"
DESIGN_PATH = (
    "docs/superpowers/specs/2026-08-11-project-index-lifecycle-prevention-design.md"
)
API_PATH = f"{FEATURE_ROOT}/contracts/public-api-v11.json"
FIXTURE_MANIFEST_PATH = (
    "tests/fixtures/public-api-v11-consumer/fixture-manifest.json"
)
COMPILE_FAIL_CASES_PATH = (
    "tests/fixtures/public-api-v11-consumer/compile-fail/cases.json"
)
RELEASE_EVIDENCE_REQUIREMENTS_PATH = (
    ".github/release-evidence-requirements-v11.json"
)
CONTEXT_PATH = "CONTEXT.md"

MANIFEST_START = "<!-- SYMFORGE FEATURE020 REFREEZE V11 JSON START -->"
MANIFEST_END = "<!-- SYMFORGE FEATURE020 REFREEZE V11 JSON END -->"
ATTESTATION_START = "<!-- SYMFORGE FEATURE020 ATTESTATION V11 JSON START -->"
ATTESTATION_END = "<!-- SYMFORGE FEATURE020 ATTESTATION V11 JSON END -->"

AMENDMENT_DOMAIN = b"symforge.feature-020.amendment-set.v11\0"
API_SCHEMA_SHAPE_DOMAIN = b"symforge.public-api-v11.schema-shape.v1\0"
API_SCHEMA_SHAPE_SHA256 = "07358647b547b05e5f28c51768b51740f0941f83d6b81d37c39397e9ca054516"
SIGNATURE_NAMESPACE = "symforge-feature-020-refreeze-v11"
APPROVAL_PURPOSE = "implementation_start"
CANONICAL_REPOSITORY = "special-place-ai-heaven/symforge"
MAX_APPROVAL_CHAIN_LENGTH = 128

SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
GIT_OID_RE = re.compile(r"^(?:[0-9a-f]{40}|[0-9a-f]{64})$")
AMENDMENT_ID_RE = re.compile(r"^F020-V11-A(?:0[1-9]|1[0-9])$")
REQUIREMENT_ID_RE = re.compile(
    r"^(?:(?:FR|SC)-[0-9]{3}|F020-V11-A(?:0[1-9]|1[0-9]))$"
)
PLAN_TASK_ID_RE = re.compile(r"^T[0-9]{3}$")
REGRESSION_ID_RE = re.compile(r"^F020-V11-R[0-9]{2}[A-Z]?$")

_ACCEPTANCE_CONTRACT = (
    "contracts/lifecycle-acceptance-oracles-v11.md"
    "#lifecycle-acceptance-oracles-v11"
)
_SOURCE_BINDING_CONTRACT = (
    "contracts/source-binding-and-state.md#v11-lifecycle-amendment"
)
_COMMON_LIFECYCLE_CONTRACTS = (
    _ACCEPTANCE_CONTRACT,
    _SOURCE_BINDING_CONTRACT,
)
EXPECTED_AMENDMENT_MAPPINGS = {
    "F020-V11-A01": {
        "requirement_ids": ("F020-V11-A01", "FR-009"),
        "contract_clause_ids": _COMMON_LIFECYCLE_CONTRACTS,
        "plan_task_ids": ("T003", "T053", "T063"),
        "regression_ids": ("F020-V11-R01",),
    },
    "F020-V11-A02": {
        "requirement_ids": ("F020-V11-A02", "FR-004"),
        "contract_clause_ids": _COMMON_LIFECYCLE_CONTRACTS,
        "plan_task_ids": ("T003", "T056"),
        "regression_ids": ("F020-V11-R02",),
    },
    "F020-V11-A03": {
        "requirement_ids": ("F020-V11-A03", "FR-017"),
        "contract_clause_ids": _COMMON_LIFECYCLE_CONTRACTS,
        "plan_task_ids": ("T003", "T041", "T063"),
        "regression_ids": ("F020-V11-R03",),
    },
    "F020-V11-A04": {
        "requirement_ids": (
            "F020-V11-A04",
            "FR-003",
            "FR-004",
            "FR-011",
        ),
        "contract_clause_ids": _COMMON_LIFECYCLE_CONTRACTS,
        "plan_task_ids": ("T003", "T053"),
        "regression_ids": ("F020-V11-R04",),
    },
    "F020-V11-A05": {
        "requirement_ids": ("F020-V11-A05", "FR-039"),
        "contract_clause_ids": _COMMON_LIFECYCLE_CONTRACTS,
        "plan_task_ids": ("T003", "T053"),
        "regression_ids": ("F020-V11-R05",),
    },
    "F020-V11-A06": {
        "requirement_ids": ("F020-V11-A06", "FR-007"),
        "contract_clause_ids": _COMMON_LIFECYCLE_CONTRACTS,
        "plan_task_ids": ("T003", "T053", "T059"),
        "regression_ids": ("F020-V11-R06",),
    },
    "F020-V11-A07": {
        "requirement_ids": ("F020-V11-A07", "FR-007"),
        "contract_clause_ids": _COMMON_LIFECYCLE_CONTRACTS,
        "plan_task_ids": ("T003", "T053", "T063"),
        "regression_ids": ("F020-V11-R07",),
    },
    "F020-V11-A08": {
        "requirement_ids": ("F020-V11-A08", "FR-021"),
        "contract_clause_ids": _COMMON_LIFECYCLE_CONTRACTS,
        "plan_task_ids": ("T003", "T056", "T059", "T063"),
        "regression_ids": ("F020-V11-R08",),
    },
    "F020-V11-A09": {
        "requirement_ids": ("F020-V11-A09", "FR-022"),
        "contract_clause_ids": _COMMON_LIFECYCLE_CONTRACTS,
        "plan_task_ids": ("T003", "T053"),
        "regression_ids": ("F020-V11-R09",),
    },
    "F020-V11-A10": {
        "requirement_ids": ("F020-V11-A10", "FR-031", "FR-039"),
        "contract_clause_ids": _COMMON_LIFECYCLE_CONTRACTS,
        "plan_task_ids": ("T003", "T053", "T060", "T064"),
        "regression_ids": ("F020-V11-R10",),
    },
    "F020-V11-A11": {
        "requirement_ids": ("F020-V11-A11", "FR-039"),
        "contract_clause_ids": _COMMON_LIFECYCLE_CONTRACTS,
        "plan_task_ids": ("T003", "T041", "T053", "T056"),
        "regression_ids": ("F020-V11-R11",),
    },
    "F020-V11-A12": {
        "requirement_ids": ("F020-V11-A12", "SC-019"),
        "contract_clause_ids": _COMMON_LIFECYCLE_CONTRACTS,
        "plan_task_ids": ("T003", "T030", "T056"),
        "regression_ids": ("F020-V11-R12",),
    },
    "F020-V11-A13": {
        "requirement_ids": ("F020-V11-A13", "FR-017"),
        "contract_clause_ids": (
            _ACCEPTANCE_CONTRACT,
            "contracts/search-knowledge.md#v11-lifecycle-acquisition",
        ),
        "plan_task_ids": ("T003", "T056", "T063"),
        "regression_ids": ("F020-V11-R13",),
    },
    "F020-V11-A14": {
        "requirement_ids": ("F020-V11-A14", "SC-011"),
        "contract_clause_ids": (
            _ACCEPTANCE_CONTRACT,
            "contracts/repository-mental-model.md#v11-lifecycle-amendment",
        ),
        "plan_task_ids": ("T003", "T056", "T063"),
        "regression_ids": ("F020-V11-R14",),
    },
    "F020-V11-A15": {
        "requirement_ids": ("F020-V11-A15", "SC-002"),
        "contract_clause_ids": _COMMON_LIFECYCLE_CONTRACTS,
        "plan_task_ids": ("T003", "T056", "T063"),
        "regression_ids": ("F020-V11-R15",),
    },
    "F020-V11-A16": {
        "requirement_ids": ("F020-V11-A16", "FR-021"),
        "contract_clause_ids": _COMMON_LIFECYCLE_CONTRACTS,
        "plan_task_ids": ("T003", "T056", "T059", "T063"),
        "regression_ids": ("F020-V11-R16",),
    },
    "F020-V11-A17": {
        "requirement_ids": ("F020-V11-A17", "FR-049"),
        "contract_clause_ids": _COMMON_LIFECYCLE_CONTRACTS,
        "plan_task_ids": ("T003", "T016", "T064"),
        "regression_ids": ("F020-V11-R17",),
    },
    "F020-V11-A18": {
        "requirement_ids": ("F020-V11-A18", "SC-024"),
        "contract_clause_ids": (
            _ACCEPTANCE_CONTRACT,
            "contracts/lifecycle-oracle-traceability-v11.md"
            "#lifecycle-oracle-traceability-contract-v11",
        ),
        "plan_task_ids": ("T003", "T068", "T069", "T070"),
        "regression_ids": (
            "F020-V11-R18A",
            "F020-V11-R18B",
            "F020-V11-R18C",
        ),
    },
    "F020-V11-A19": {
        "requirement_ids": ("F020-V11-A19", "FR-017", "FR-033"),
        "contract_clause_ids": (
            "contracts/knowledge-authority-hygiene.md"
            "#v11-lifecycle-acquisition-and-voice-filtering",
            _ACCEPTANCE_CONTRACT,
            "contracts/repository-mental-model.md#v11-lifecycle-amendment",
            "contracts/search-knowledge.md#v11-lifecycle-acquisition",
            _SOURCE_BINDING_CONTRACT,
        ),
        "plan_task_ids": ("T003", "T041", "T063", "T084"),
        "regression_ids": ("F020-V11-R19A", "F020-V11-R19B"),
    },
}

EXPECTED_DIRECT_TRAIT_IMPLS = (
    ("symforge::embed::EmbeddedSourceHandle", "core::ops::Drop", "positive"),
    ("symforge::embed::EngineInfo", "core::clone::Clone", "positive"),
    ("symforge::embed::EngineInfo", "core::cmp::Eq", "positive"),
    ("symforge::embed::EngineInfo", "core::cmp::PartialEq", "positive"),
    ("symforge::embed::EngineInfo", "core::fmt::Debug", "positive"),
    ("symforge::embed::EngineInfo", "core::marker::Copy", "positive"),
    ("symforge::embed::ProcessIndexRuntime", "core::clone::Clone", "positive"),
    ("symforge::embed::ProcessIndexRuntime", "core::ops::Drop", "positive"),
    ("symforge::embed::ReceiptWaitError", "core::fmt::Debug", "positive"),
    ("symforge::embed::ReceiptWaitError", "core::fmt::Display", "positive"),
    ("symforge::embed::ReceiptWaitError", "std::error::Error", "positive"),
    ("symforge::embed::SourceRefusal", "core::fmt::Debug", "positive"),
    ("symforge::embed::SourceRefusal", "core::fmt::Display", "positive"),
    ("symforge::embed::SourceRefusal", "std::error::Error", "positive"),
    ("symforge::server_api::ServerBootstrapError", "core::fmt::Debug", "positive"),
    ("symforge::server_api::ServerBootstrapError", "core::fmt::Display", "positive"),
    ("symforge::server_api::ServerBootstrapError", "std::error::Error", "positive"),
)

EXPECTED_AUTO_TRAITS = {
    "expansion": (
        "Every subject is checked against every trait in universe; groups are an "
        "exact cartesian compression, not a wildcard."
    ),
    "expectation_groups": (
        {
            "states": {
                "core::marker::Send": "positive",
                "core::marker::Sync": "positive",
                "core::marker::Unpin": "positive",
                "std::panic::RefUnwindSafe": "positive",
                "std::panic::UnwindSafe": "positive",
            },
            "subjects": (
                "symforge::embed::EngineInfo",
                "symforge::embed::ReceiptWaitError",
                "symforge::embed::RetryAdvice",
                "symforge::embed::ShutdownReport",
                "symforge::embed::SourceCloseReport",
                "symforge::embed::SourceRefusalKind",
                "symforge::embed::SourceRuntimePhase",
                "symforge::embed::SourceRuntimeView",
                "symforge::embed::SymbolMatch",
                "symforge::embed::SymbolSearchRequest",
                "symforge::embed::SymbolSearchResult",
                "symforge::embed::TextMatch",
                "symforge::embed::TextSearchRequest",
                "symforge::embed::TextSearchResult",
                "symforge::server_api::ServerExit",
            ),
        },
        {
            "states": {
                "core::marker::Send": "positive",
                "core::marker::Sync": "positive",
                "core::marker::Unpin": "positive",
                "std::panic::RefUnwindSafe": "negative",
                "std::panic::UnwindSafe": "negative",
            },
            "subjects": (
                "symforge::embed::EmbeddedSourceHandle",
                "symforge::embed::ProcessIndexRuntime",
                "symforge::embed::RefreshTicket",
                "symforge::embed::ShutdownReceipt",
                "symforge::embed::SourceCloseReceipt",
            ),
        },
        {
            "states": {
                "core::marker::Send": "conditional:T",
                "core::marker::Sync": "conditional:T",
                "core::marker::Unpin": "conditional:T",
                "std::panic::RefUnwindSafe": "conditional:T",
                "std::panic::UnwindSafe": "conditional:T",
            },
            "subjects": ("symforge::embed::Claim",),
        },
        {
            "states": {
                "core::marker::Send": "positive",
                "core::marker::Sync": "positive",
                "core::marker::Unpin": "positive",
                "std::panic::RefUnwindSafe": "positive",
                "std::panic::UnwindSafe": "positive",
            },
            "subjects": (
                "symforge::embed::AtomicAuthority",
                "symforge::embed::ClaimProvenance",
                "symforge::embed::EmbeddedSourceSpec",
                "symforge::embed::EvaluationProvenance",
                "symforge::embed::OperationKind",
                "symforge::embed::OperationReceipt",
                "symforge::embed::SourceRefusal",
                "symforge::server_api::ServerBootstrapError",
            ),
        },
    ),
    "universe": (
        "core::marker::Send",
        "core::marker::Sync",
        "core::marker::Unpin",
        "std::panic::RefUnwindSafe",
        "std::panic::UnwindSafe",
    ),
}

_AUTHORITY_NEGATIVE_SUBJECTS = (
    "symforge::embed::AtomicAuthority",
    "symforge::embed::Claim<T>",
    "symforge::embed::ClaimProvenance",
    "symforge::embed::EvaluationProvenance",
    "symforge::embed::OperationReceipt",
    "symforge::embed::SourceRefusal",
)
_RUNTIME_NEGATIVE_SUBJECTS = (
    "symforge::embed::EmbeddedSourceHandle",
    "symforge::embed::ProcessIndexRuntime",
)
EXPECTED_NEGATIVE_ASSERTIONS = (
    {
        "id": "authority-types-01-not-deserialize",
        "kind": "impl-absent",
        "subjects": _AUTHORITY_NEGATIVE_SUBJECTS,
        "trait": "serde::Deserialize",
    },
    {
        "id": "authority-types-02-not-default",
        "kind": "impl-absent",
        "subjects": _AUTHORITY_NEGATIVE_SUBJECTS,
        "trait": "core::default::Default",
    },
    {
        "id": "authority-types-03-not-from",
        "kind": "impl-family-absent-except-reflexive",
        "permitted_source": "Self",
        "subjects": _AUTHORITY_NEGATIVE_SUBJECTS,
        "trait_family": "core::convert::From<_>",
    },
    {
        "id": "cbm-spike-no-public-graph-delta",
        "kind": "graph-equivalence",
        "pairs": (
            ("embed", "embed-cbm-spike"),
            ("server", "server-cbm-spike"),
            ("server-embed", "server-embed-cbm-spike"),
        ),
    },
    {
        "id": "embedded-source-handle-not-clone",
        "kind": "impl-absent",
        "subjects": ("symforge::embed::EmbeddedSourceHandle",),
        "trait": "core::clone::Clone",
    },
    {
        "forbidden_prefixes": (
            "symforge::analytics",
            "symforge::capability",
            "symforge::cli",
            "symforge::daemon",
            "symforge::discovery",
            "symforge::domain",
            "symforge::edit_safety",
            "symforge::git",
            "symforge::gitignore_hygiene",
            "symforge::hash",
            "symforge::idempotency",
            "symforge::knowledge",
            "symforge::live_index",
            "symforge::observability",
            "symforge::parsing",
            "symforge::path_shadow",
            "symforge::paths",
            "symforge::process_util",
            "symforge::protocol",
            "symforge::server",
            "symforge::sidecar",
            "symforge::stel",
            "symforge::stel_core",
            "symforge::version_registry",
            "symforge::watcher",
            "symforge::watcher_state",
            "symforge::worktree",
        ),
        "id": "no-raw-crate-root-modules",
        "kind": "export-prefix-absent",
    },
    {
        "forbidden_prefixes": (
            "symforge::embed::domain",
            "symforge::embed::git",
            "symforge::embed::live_index",
            "symforge::embed::parsing",
        ),
        "id": "no-raw-deep-embed-reexports",
        "kind": "export-prefix-absent",
    },
    {
        "forbidden_prefixes": (
            "symforge::health",
            "symforge::server_api::health",
        ),
        "id": "no-rust-health-api",
        "kind": "export-prefix-absent",
    },
    {
        "forbidden_prefixes": (
            "symforge::embed::stel",
            "symforge::stel",
            "symforge::stel_core",
        ),
        "id": "no-stel-public-api",
        "kind": "export-prefix-absent",
    },
    {
        "id": "runtime-handle-no-as-ref",
        "kind": "impl-family-absent",
        "subjects": _RUNTIME_NEGATIVE_SUBJECTS,
        "trait_family": "core::convert::AsRef<_>",
    },
    {
        "id": "runtime-handle-no-borrow",
        "kind": "impl-family-absent-except-reflexive",
        "permitted_source": "Self",
        "subjects": _RUNTIME_NEGATIVE_SUBJECTS,
        "trait_family": "core::borrow::Borrow<_>",
    },
    {
        "id": "runtime-handle-no-deref",
        "kind": "impl-absent",
        "subjects": _RUNTIME_NEGATIVE_SUBJECTS,
        "trait": "core::ops::Deref",
    },
)

REQUIRED_NORMATIVE_PATHS = frozenset(
    {
        CONTEXT_PATH,
        f"{FEATURE_ROOT}/GOAL.md",
        f"{FEATURE_ROOT}/checklists/requirements.md",
        f"{FEATURE_ROOT}/contracts/knowledge-authority-hygiene.md",
        f"{FEATURE_ROOT}/contracts/lifecycle-acceptance-oracles-v11.md",
        f"{FEATURE_ROOT}/contracts/lifecycle-oracle-traceability-v11.md",
        f"{FEATURE_ROOT}/contracts/repository-mental-model.md",
        f"{FEATURE_ROOT}/contracts/search-knowledge.md",
        f"{FEATURE_ROOT}/contracts/source-binding-and-state.md",
        f"{FEATURE_ROOT}/contracts/v10-authority-retirement-v11.md",
        API_PATH,
        f"{FEATURE_ROOT}/data-model.md",
        f"{FEATURE_ROOT}/plan.md",
        f"{FEATURE_ROOT}/quickstart.md",
        f"{FEATURE_ROOT}/spec.md",
        f"{FEATURE_ROOT}/tasks.md",
    }
)
CLAUSE_PATHS = REQUIRED_NORMATIVE_PATHS - {API_PATH}

MANIFEST_FIELDS = frozenset(
    {
        "kind",
        "schema_version",
        "feature_root",
        "self_path",
        "baseline",
        "inventory",
        "required_normative_paths",
        "amendments",
        "amendment_set_id",
        "public_api",
        "design",
        "context",
        "detached_attestation_path",
    }
)
INVENTORY_FIELDS = frozenset(
    {
        "path",
        "scope",
        "classification",
        "hash_policy",
        "sha256",
        "superseded_by",
    }
)
AMENDMENT_FIELDS = frozenset(
    {
        "amendment_id",
        "replaced",
        "replacements",
        "requirement_ids",
        "contract_clause_ids",
        "plan_task_ids",
        "regression_ids",
    }
)
CLAUSE_FIELDS = frozenset(
    {"clause_id", "source", "path", "start_line", "end_line", "sha256"}
)
ATTESTATION_FIELDS = frozenset(
    {
        "kind",
        "schema_version",
        "manifest",
        "baseline",
        "design",
        "context",
        "public_api",
        "amendment_set_id",
        "external_approval",
    }
)
APPROVAL_FIELDS = frozenset(
    {
        "kind",
        "schema_version",
        "repository",
        "purpose",
        "target_commit",
        "target_tree",
        "attestation",
        "release_identity",
        "approved_at",
        "sequence",
        "store_locator",
        "store_version",
        "predecessor_digest",
        "signature_namespace",
    }
)

FORBIDDEN_INHERITED_GIT_ENV = frozenset(
    {
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_COMMON_DIR",
        "GIT_CONFIG",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_GLOBAL",
        "GIT_CONFIG_NOSYSTEM",
        "GIT_CONFIG_PARAMETERS",
        "GIT_CONFIG_SYSTEM",
        "GIT_DIR",
        "GIT_INDEX_FILE",
        "GIT_NAMESPACE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_REPLACE_REF_BASE",
        "GIT_WORK_TREE",
    }
)
FORBIDDEN_INHERITED_GIT_ENV_PREFIXES = ("GIT_CONFIG_KEY_", "GIT_CONFIG_VALUE_")


class RefreezeError(RuntimeError):
    """A fail-closed refreeze verification result with a non-sensitive code."""

    def __init__(self, code: str) -> None:
        super().__init__(code)
        self.code = code


@dataclass(frozen=True)
class InternalVerification:
    target_commit: str
    target_tree: str
    attestation_path: str
    attestation_sha256: str


class GitObjects:
    """Small raw-object reader; verification never trusts working-tree bytes."""

    def __init__(
        self, root: Path, *, git_executable: str | Path | None = None
    ) -> None:
        _reject_inherited_git_overrides()
        self.root = root.resolve()
        if git_executable is None:
            git_executable = shutil.which("git")
            if git_executable is None:
                raise RefreezeError("GIT_UNAVAILABLE")
        self.git_executable = _trusted_executable_path(
            git_executable, root=self.root
        )
        self._blob_cache: dict[tuple[str, str], bytes] = {}
        self._git_environment = _neutral_git_environment()
        top = self._run(["rev-parse", "--show-toplevel"]).stdout
        try:
            discovered = Path(top.decode("utf-8").strip()).resolve()
        except (UnicodeDecodeError, OSError) as error:
            raise RefreezeError("GIT_REPOSITORY_INVALID") from error
        if discovered != self.root:
            raise RefreezeError("GIT_ROOT_MISMATCH")

    def _run(
        self,
        args: Sequence[str],
        *,
        input_bytes: bytes | None = None,
        allowed_returncodes: frozenset[int] = frozenset({0}),
    ) -> subprocess.CompletedProcess[bytes]:
        try:
            result = subprocess.run(
                [str(self.git_executable), "--no-replace-objects", *args],
                cwd=self.root,
                check=False,
                capture_output=True,
                env=self._git_environment,
                input=input_bytes,
                shell=False,
            )
        except OSError as error:
            raise RefreezeError("GIT_UNAVAILABLE") from error
        if result.returncode not in allowed_returncodes:
            raise RefreezeError("GIT_OBJECT_READ_FAILED")
        return result

    def resolve_commit(self, target_ref: str) -> str:
        if not isinstance(target_ref, str) or not target_ref or "\x00" in target_ref:
            raise RefreezeError("TARGET_REF_INVALID")
        output = self._run(
            ["rev-parse", "--verify", "--end-of-options", f"{target_ref}^{{commit}}"]
        ).stdout
        value = _decode_ascii_line(output, "TARGET_COMMIT_INVALID")
        if not GIT_OID_RE.fullmatch(value):
            raise RefreezeError("TARGET_COMMIT_INVALID")
        return value

    def resolve_tree(self, commit: str) -> str:
        output = self._run(
            ["rev-parse", "--verify", "--end-of-options", f"{commit}^{{tree}}"]
        ).stdout
        value = _decode_ascii_line(output, "TARGET_TREE_INVALID")
        if not GIT_OID_RE.fullmatch(value):
            raise RefreezeError("TARGET_TREE_INVALID")
        return value

    def is_ancestor(self, older: str, newer: str) -> bool:
        result = self._run(
            ["merge-base", "--is-ancestor", older, newer],
            allowed_returncodes=frozenset({0, 1}),
        )
        return result.returncode == 0

    def path_history(self, commit: str, path: str) -> list[str]:
        _validate_repo_path(path)
        # A truncated clone hides ancestors, so an absence answer derived from this
        # walk would assert history the walk never saw.
        if (
            _decode_ascii_line(
                self._run(["rev-parse", "--is-shallow-repository"]).stdout.strip(),
                "GIT_HISTORY_SHALLOW",
            )
            != "false"
        ):
            raise RefreezeError("GIT_HISTORY_SHALLOW")
        output = self._run(
            ["rev-list", "--full-history", commit, "--", path]
        ).stdout
        commits: list[str] = []
        for raw_line in output.splitlines():
            value = _decode_ascii_line(raw_line, "GIT_HISTORY_INVALID")
            if GIT_OID_RE.fullmatch(value) is None:
                raise RefreezeError("GIT_HISTORY_INVALID")
            commits.append(value)
        return commits

    def blob_exists(self, commit: str, path: str) -> bool:
        _validate_repo_path(path)
        tree_output = self._run(
            ["ls-tree", "-z", "--full-tree", commit, "--", path]
        ).stdout
        raw_entries = [entry for entry in tree_output.split(b"\0") if entry]
        if not raw_entries:
            return False
        if len(raw_entries) != 1:
            raise RefreezeError("GIT_TREE_ENTRY_INVALID")
        mode, object_type, _oid, discovered_path = self._parse_tree_entry(
            raw_entries[0]
        )
        if (
            discovered_path != path
            or object_type != b"blob"
            or mode not in {b"100644", b"100755"}
        ):
            raise RefreezeError("GIT_TREE_ENTRY_UNSUPPORTED")
        return True

    @staticmethod
    def _parse_tree_entry(raw_entry: bytes) -> tuple[bytes, bytes, str, str]:
        metadata, separator, raw_path = raw_entry.partition(b"\t")
        fields = metadata.split(b" ")
        if not separator or len(fields) != 3:
            raise RefreezeError("GIT_TREE_ENTRY_INVALID")
        try:
            path = raw_path.decode("utf-8")
            oid = fields[2].decode("ascii")
        except UnicodeDecodeError as error:
            raise RefreezeError("GIT_TREE_PATH_INVALID") from error
        _validate_repo_path(path)
        if GIT_OID_RE.fullmatch(oid) is None:
            raise RefreezeError("GIT_TREE_ENTRY_INVALID")
        return fields[0], fields[1], oid, path

    def read_blob(self, commit: str, path: str) -> bytes:
        _validate_repo_path(path)
        cache_key = (commit, path)
        cached = self._blob_cache.get(cache_key)
        if cached is not None:
            return cached
        tree_output = self._run(
            ["ls-tree", "-z", "--full-tree", commit, "--", path]
        ).stdout
        raw_entries = [entry for entry in tree_output.split(b"\0") if entry]
        if len(raw_entries) != 1:
            raise RefreezeError("GIT_TREE_ENTRY_INVALID")
        mode, object_type, oid, discovered_path = self._parse_tree_entry(
            raw_entries[0]
        )
        if discovered_path != path:
            raise RefreezeError("GIT_TREE_ENTRY_INVALID")
        if object_type != b"blob" or mode not in {b"100644", b"100755"}:
            raise RefreezeError("GIT_TREE_ENTRY_UNSUPPORTED")
        value = self._run(["cat-file", "blob", oid]).stdout
        self._blob_cache[cache_key] = value
        return value

    def inventory_paths(self, commit: str, feature_root: str, context_path: str) -> set[str]:
        output = self._run(
            [
                "ls-tree",
                "-r",
                "-z",
                "--full-tree",
                commit,
                "--",
                feature_root,
                context_path,
            ]
        ).stdout
        paths: set[str] = set()
        for raw_entry in output.split(b"\0"):
            if not raw_entry:
                continue
            mode, object_type, _oid, path = self._parse_tree_entry(raw_entry)
            if object_type != b"blob" or mode not in {
                b"100644",
                b"100755",
            }:
                raise RefreezeError("GIT_TREE_ENTRY_UNSUPPORTED")
            if path in paths:
                raise RefreezeError("GIT_TREE_PATH_DUPLICATE")
            paths.add(path)
        return paths


def _trusted_executable_path(value: str | Path, *, root: Path) -> Path:
    candidate = Path(value)
    if not candidate.is_absolute():
        raise RefreezeError("EXECUTABLE_PROVENANCE_INVALID")
    try:
        resolved = candidate.resolve(strict=True)
    except OSError as error:
        raise RefreezeError("EXECUTABLE_PROVENANCE_INVALID") from error
    if (
        candidate != resolved
        or candidate.is_symlink()
        or not resolved.is_file()
        or not os.access(resolved, os.X_OK)
        or resolved == root
        or root in resolved.parents
    ):
        raise RefreezeError("EXECUTABLE_PROVENANCE_INVALID")
    return resolved


def _reject_inherited_git_overrides() -> None:
    for name in os.environ:
        if name in FORBIDDEN_INHERITED_GIT_ENV or name.startswith(
            FORBIDDEN_INHERITED_GIT_ENV_PREFIXES
        ):
            raise RefreezeError("GIT_ENVIRONMENT_OVERRIDE_REJECTED")


def _neutral_git_environment() -> dict[str, str]:
    environment = {
        name: value for name, value in os.environ.items() if not name.startswith("GIT_")
    }
    environment.update(
        {
            "GIT_CONFIG_GLOBAL": os.devnull,
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_CONFIG_SYSTEM": os.devnull,
            "GIT_NO_REPLACE_OBJECTS": "1",
            "GIT_TERMINAL_PROMPT": "0",
        }
    )
    return environment


def _decode_ascii_line(value: bytes, code: str) -> str:
    try:
        decoded = value.decode("ascii").strip()
    except UnicodeDecodeError as error:
        raise RefreezeError(code) from error
    if "\n" in decoded or "\r" in decoded:
        raise RefreezeError(code)
    return decoded


def _validate_repo_path(path: object) -> str:
    if not isinstance(path, str) or not path or "\\" in path or "\x00" in path:
        raise RefreezeError("PATH_INVALID")
    parsed = PurePosixPath(path)
    if parsed.is_absolute() or any(part in {"", ".", ".."} for part in parsed.parts):
        raise RefreezeError("PATH_INVALID")
    if parsed.as_posix() != path:
        raise RefreezeError("PATH_INVALID")
    return path


def _sha256(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def _canonical_json(value: object) -> bytes:
    try:
        return json.dumps(
            value,
            ensure_ascii=False,
            allow_nan=False,
            separators=(",", ":"),
            sort_keys=True,
        ).encode("utf-8")
    except (TypeError, ValueError) as error:
        raise RefreezeError("JSON_CANONICALIZATION_FAILED") from error


def _reject_duplicate_pairs(pairs: list[tuple[str, object]]) -> dict[str, object]:
    value: dict[str, object] = {}
    for key, item in pairs:
        if key in value:
            raise RefreezeError("JSON_DUPLICATE_KEY")
        value[key] = item
    return value


def _reject_json_constant(_value: str) -> object:
    raise RefreezeError("JSON_NONFINITE_NUMBER")


def _parse_json_text(text: str) -> dict[str, object]:
    try:
        value = json.loads(
            text,
            object_pairs_hook=_reject_duplicate_pairs,
            parse_constant=_reject_json_constant,
        )
    except RefreezeError:
        raise
    except (json.JSONDecodeError, UnicodeError) as error:
        raise RefreezeError("JSON_INVALID") from error
    if not isinstance(value, dict):
        raise RefreezeError("JSON_ROOT_INVALID")
    return value


def _parse_json_bytes(value: bytes) -> dict[str, object]:
    try:
        text = value.decode("utf-8")
    except UnicodeDecodeError as error:
        raise RefreezeError("JSON_UTF8_REQUIRED") from error
    return _parse_json_text(text)


def _parse_sentinel_json(value: bytes, start: str, end: str) -> dict[str, object]:
    try:
        text = value.decode("utf-8")
    except UnicodeDecodeError as error:
        raise RefreezeError("SENTINEL_UTF8_REQUIRED") from error
    if text.count(start) != 1 or text.count(end) != 1:
        raise RefreezeError("SENTINEL_COUNT_INVALID")
    if text.index(start) > text.index(end):
        raise RefreezeError("SENTINEL_ORDER_INVALID")
    before, payload_and_end = text.split(start, 1)
    payload, after = payload_and_end.split(end, 1)
    if end in before or start in after:
        raise RefreezeError("SENTINEL_ORDER_INVALID")
    match = re.fullmatch(r"\s*```json\r?\n(.*)\r?\n```\s*", payload, re.DOTALL)
    if match is None:
        raise RefreezeError("SENTINEL_FENCE_INVALID")
    return _parse_json_text(match.group(1))


def _closed_object(value: object, fields: frozenset[str], code: str) -> dict[str, object]:
    if not isinstance(value, dict) or set(value) != fields:
        raise RefreezeError(code)
    return value


def _exact(value: object, expected: object, code: str) -> None:
    if type(value) is not type(expected) or value != expected:
        raise RefreezeError(code)


def _string(value: object, code: str) -> str:
    if not isinstance(value, str) or not value:
        raise RefreezeError(code)
    return value


def _release_identity(value: object) -> str:
    identity = _string(value, "APPROVAL_IDENTITY_INVALID")
    if (
        len(identity) > 256
        or identity.startswith("-")
        or any(ord(character) < 0x21 or ord(character) > 0x7E for character in identity)
    ):
        raise RefreezeError("APPROVAL_IDENTITY_INVALID")
    return identity


def _integer(value: object, code: str, *, minimum: int = 0) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < minimum:
        raise RefreezeError(code)
    return value


def _sha256_value(value: object, code: str) -> str:
    if not isinstance(value, str) or SHA256_RE.fullmatch(value) is None:
        raise RefreezeError(code)
    return value


def _git_oid(value: object, code: str) -> str:
    if not isinstance(value, str) or GIT_OID_RE.fullmatch(value) is None:
        raise RefreezeError(code)
    return value


def _sorted_unique_strings(
    value: object,
    code: str,
    *,
    nonempty: bool = True,
    allow_empty_items: bool = False,
) -> list[str]:
    if not isinstance(value, list) or (nonempty and not value):
        raise RefreezeError(code)
    if any(
        not isinstance(item, str) or (not allow_empty_items and not item)
        for item in value
    ):
        raise RefreezeError(code)
    if value != sorted(set(value)):
        raise RefreezeError(code)
    return value


def _validate_baseline(
    git: GitObjects, value: object, *, target_commit: str
) -> dict[str, object]:
    baseline = _closed_object(value, frozenset({"commit", "tree"}), "BASELINE_SHAPE_INVALID")
    commit = _git_oid(baseline["commit"], "BASELINE_COMMIT_INVALID")
    tree = _git_oid(baseline["tree"], "BASELINE_TREE_INVALID")
    if git.resolve_commit(commit) != commit or git.resolve_tree(commit) != tree:
        raise RefreezeError("BASELINE_IDENTITY_MISMATCH")
    if not git.is_ancestor(commit, target_commit):
        raise RefreezeError("BASELINE_NOT_ANCESTOR")
    return baseline


def _validate_path_hash(
    git: GitObjects,
    commit: str,
    value: object,
    *,
    expected_path: str | None,
    code: str,
) -> dict[str, object]:
    item = _closed_object(value, frozenset({"path", "sha256"}), f"{code}_SHAPE")
    path = _validate_repo_path(item["path"])
    if expected_path is not None and path != expected_path:
        raise RefreezeError(f"{code}_PATH")
    digest = _sha256_value(item["sha256"], f"{code}_DIGEST")
    if _sha256(git.read_blob(commit, path)) != digest:
        raise RefreezeError(f"{code}_HASH_MISMATCH")
    return item


def _validate_compile_fail_cases(
    git: GitObjects,
    commit: str,
) -> tuple[int, int]:
    case_catalog = _closed_object(
        _parse_json_bytes(git.read_blob(commit, COMPILE_FAIL_CASES_PATH)),
        frozenset(
            {
                "schema_version",
                "kind",
                "status",
                "materialization_rule",
                "trait_absent_groups",
                "impl_family_absent_groups",
                "path_absent_groups",
                "graph_only_assertions",
                "expected_results_sha256",
            }
        ),
        "API_COMPILE_FAIL_CASE_CATALOG_INVALID",
    )
    _exact(case_catalog["schema_version"], 1, "API_COMPILE_FAIL_CASE_CATALOG_INVALID")
    _exact(
        case_catalog["kind"],
        "symforge.public_api_v11_compile_fail_cases",
        "API_COMPILE_FAIL_CASE_CATALOG_INVALID",
    )
    _exact(
        case_catalog["status"],
        "pre_activation_inputs_only",
        "API_COMPILE_FAIL_CASE_CATALOG_INVALID",
    )
    _exact(
        case_catalog["expected_results_sha256"],
        None,
        "API_COMPILE_FAIL_EXPECTED_RESULTS_INVALID",
    )
    _exact(
        case_catalog["materialization_rule"],
        "Expand every subject/path in every group to an independent temporary "
        "dependent crate; one compiler invocation per atomic case; accept a "
        "failure only when its primary Rust diagnostic code is listed in that "
        "group's expected_error_codes.",
        "API_COMPILE_FAIL_MATERIALIZATION_RULE_INVALID",
    )

    atomic_probe_count = 0
    for field, subject_field, expected_codes in (
        ("trait_absent_groups", "subjects", ["E0277"]),
        ("impl_family_absent_groups", "subjects", ["E0277"]),
        ("path_absent_groups", "paths", ["E0432", "E0603"]),
    ):
        groups = _api_nonempty_list(
            case_catalog[field], "API_COMPILE_FAIL_CASE_CATALOG_INVALID"
        )
        for group in groups:
            if (
                not isinstance(group, dict)
                or group.get("expected_error_codes") != expected_codes
            ):
                raise RefreezeError("API_COMPILE_FAIL_EXPECTED_ERROR_CODES_INVALID")
            subjects = group.get(subject_field)
            if (
                not isinstance(subjects, list)
                or not subjects
                or any(not isinstance(subject, str) or not subject for subject in subjects)
            ):
                raise RefreezeError("API_COMPILE_FAIL_CASE_CATALOG_INVALID")
            atomic_probe_count += len(subjects)

    graph_equivalence_pair_count = 0
    graph_assertions = _api_nonempty_list(
        case_catalog["graph_only_assertions"],
        "API_COMPILE_FAIL_CASE_CATALOG_INVALID",
    )
    for assertion in graph_assertions:
        pairs = assertion.get("pairs") if isinstance(assertion, dict) else None
        if (
            not isinstance(pairs, list)
            or not pairs
            or any(
                not isinstance(pair, list)
                or len(pair) != 2
                or any(not isinstance(item, str) or not item for item in pair)
                for pair in pairs
            )
        ):
            raise RefreezeError("API_COMPILE_FAIL_CASE_CATALOG_INVALID")
        graph_equivalence_pair_count += len(pairs)
    return atomic_probe_count, graph_equivalence_pair_count


def _validate_fixture_manifest(
    git: GitObjects,
    commit: str,
    *,
    api: dict[str, object],
    api_raw_digest: str,
) -> None:
    fixture_manifest = _closed_object(
        _parse_json_bytes(git.read_blob(commit, FIXTURE_MANIFEST_PATH)),
        frozenset(
            {
                "schema_version",
                "kind",
                "status",
                "source_contract",
                "coverage",
                "closed_evidence_mapping",
                "inputs",
                "pre_activation_facts",
                "limitations",
            }
        ),
        "FIXTURE_MANIFEST_SHAPE_INVALID",
    )
    source_contract = _closed_object(
        fixture_manifest["source_contract"],
        frozenset({"path", "schema_version", "sha256"}),
        "FIXTURE_MANIFEST_SOURCE_CONTRACT_INVALID",
    )
    if source_contract != {
        "path": API_PATH,
        "schema_version": 1,
        "sha256": api_raw_digest,
    }:
        raise RefreezeError("FIXTURE_MANIFEST_SOURCE_CONTRACT_INVALID")
    _exact(fixture_manifest["schema_version"], 1, "FIXTURE_MANIFEST_SHAPE_INVALID")
    _exact(
        fixture_manifest["kind"],
        "symforge.public_api_v11_consumer_fixture",
        "FIXTURE_MANIFEST_SHAPE_INVALID",
    )
    _exact(
        fixture_manifest["status"],
        "pre_activation_inputs_only",
        "FIXTURE_MANIFEST_SHAPE_INVALID",
    )
    atomic_probe_count, graph_equivalence_pair_count = _validate_compile_fail_cases(
        git,
        commit,
    )
    configuration = api["configuration_domain"]
    graph = api["expected_graph"]
    expected_coverage = {
        "supported_cells": len(configuration["cells"]),
        "supported_targets": len(configuration["targets"]),
        "supported_feature_vectors": len(configuration["feature_vectors"]),
        "graph_projections": len(graph["graph_projections"]),
        "public_modules": len(graph["modules"]),
        "public_exports": len(graph["exports"]),
        "public_items": len(graph["items"]),
        "inherent_associated_items": sum(
            len(item["associated_items"])
            for item in graph["impls"]
            if item["trait"] is None
        ),
        "direct_trait_impls": len(graph["trait_impls"]),
        "negative_assertions": len(api["negative_assertions"]),
        "atomic_compile_fail_probes": atomic_probe_count,
        "graph_equivalence_pairs": graph_equivalence_pair_count,
    }
    coverage = _closed_object(
        fixture_manifest["coverage"],
        frozenset(expected_coverage),
        "FIXTURE_MANIFEST_COVERAGE_INVALID",
    )
    if coverage != expected_coverage:
        raise RefreezeError("FIXTURE_MANIFEST_COVERAGE_INVALID")
    pre_activation_facts = _closed_object(
        fixture_manifest["pre_activation_facts"],
        frozenset(
            {
                "all_cfg_inventory_sha256",
                "assignment_proof_sha256",
                "dependent_positive_compile_result",
                "generated_rustdoc_graph_set_sha256",
                "negative_case_results_sha256",
                "observed_v10_atom_set_sha256",
                "predicate_inventory_sha256",
                "rustc_cfg_sha256_by_target",
            }
        ),
        "FIXTURE_MANIFEST_PREACTIVATION_FACTS_INVALID",
    )
    if any(value is not None for value in pre_activation_facts.values()):
        raise RefreezeError("FIXTURE_MANIFEST_PREACTIVATION_FACTS_INVALID")
    _exact(
        fixture_manifest["closed_evidence_mapping"],
        {
            "case_catalog": "compile-fail/cases.json",
            "closed": True,
            "non_exhaustive_assertions": [
                {
                    "assertion_id": "authority-types-01-not-deserialize",
                    "required_exhaustive_evidence": "public_item_graph",
                    "required_probe_completeness": "non_exhaustive",
                }
            ],
            "unlisted_non_exhaustive_assertion": "reject",
            "unsupported_exhaustive_evidence": "reject",
        },
        "FIXTURE_MANIFEST_CLOSED_EVIDENCE_MAPPING_INVALID",
    )
    _exact(
        fixture_manifest["inputs"],
        [
            "README.md",
            "all-cfg/Cargo.toml",
            "all-cfg/src/lib.rs",
            "compile-fail/Cargo.toml",
            "compile-fail/cases.json",
            "compile-fail/src/lib.rs",
            "compile-fail/templates/impl_family_absent.rs.in",
            "compile-fail/templates/path_absent.rs.in",
            "compile-fail/templates/trait_absent.rs.in",
            "dependent-positive/Cargo.toml",
            "dependent-positive/src/lib.rs",
            "graph-cover.json",
        ],
        "FIXTURE_MANIFEST_INPUTS_INVALID",
    )
    _exact(
        fixture_manifest["limitations"],
        [
            "The current product crate does not expose the V11 API; these are "
            "inputs, not passing compiler evidence.",
            "Impl-family rules are exhaustive only in the future public-item "
            "graph comparison; compile-fail templates use a distinct local Probe "
            "type as a non-reflexive external-consumer witness.",
            "The checked-in corpus has no self-authorizing generator or verifier; "
            "the frozen contract and externally approved target remain authoritative.",
        ],
        "FIXTURE_MANIFEST_LIMITATIONS_INVALID",
    )


def _validate_api(
    git: GitObjects, commit: str, value: object
) -> dict[str, object]:
    item = _closed_object(
        value, frozenset({"path", "raw_sha256", "canonical_sha256"}), "API_PIN_SHAPE"
    )
    path = _validate_repo_path(item["path"])
    if path != API_PATH:
        raise RefreezeError("API_PATH_INVALID")
    raw = git.read_blob(commit, path)
    raw_digest = _sha256_value(item["raw_sha256"], "API_RAW_DIGEST_INVALID")
    canonical_digest = _sha256_value(
        item["canonical_sha256"], "API_CANONICAL_DIGEST_INVALID"
    )
    if _sha256(raw) != raw_digest:
        raise RefreezeError("API_RAW_HASH_MISMATCH")
    api = _parse_json_bytes(raw)
    _exact(api.get("kind"), "symforge-rust-public-api", "API_KIND_INVALID")
    _exact(api.get("schema_version"), 1, "API_SCHEMA_INVALID")
    _exact(
        api.get("canonicalization"),
        "jcs+symforge-api-v1",
        "API_CANONICALIZATION_INVALID",
    )
    _validate_api_contract(api)
    input_corpus_paths: list[str] = []
    expected_input_corpus_paths_by_role = {
        "all-cfg-manifest": frozenset(
            {"tests/fixtures/public-api-v11-consumer/all-cfg/Cargo.toml"}
        ),
        "all-cfg-source": frozenset(
            {"tests/fixtures/public-api-v11-consumer/all-cfg/src/lib.rs"}
        ),
        "compile-fail-case-catalog": frozenset(
            {"tests/fixtures/public-api-v11-consumer/compile-fail/cases.json"}
        ),
        "compile-fail-manifest": frozenset(
            {"tests/fixtures/public-api-v11-consumer/compile-fail/Cargo.toml"}
        ),
        "compile-fail-placeholder": frozenset(
            {"tests/fixtures/public-api-v11-consumer/compile-fail/src/lib.rs"}
        ),
        "compile-fail-template": frozenset(
            {
                "tests/fixtures/public-api-v11-consumer/compile-fail/templates/impl_family_absent.rs.in",
                "tests/fixtures/public-api-v11-consumer/compile-fail/templates/path_absent.rs.in",
                "tests/fixtures/public-api-v11-consumer/compile-fail/templates/trait_absent.rs.in",
            }
        ),
        "dependent-positive-manifest": frozenset(
            {"tests/fixtures/public-api-v11-consumer/dependent-positive/Cargo.toml"}
        ),
        "dependent-positive-source": frozenset(
            {"tests/fixtures/public-api-v11-consumer/dependent-positive/src/lib.rs"}
        ),
        "graph-cover": frozenset(
            {"tests/fixtures/public-api-v11-consumer/graph-cover.json"}
        ),
    }
    expected_input_corpus_role_counts = {
        role: len(paths)
        for role, paths in expected_input_corpus_paths_by_role.items()
    }
    input_corpus_role_counts: dict[str, int] = {}
    for raw_entry in api["configuration_domain"]["cover"]["input_corpus"]:
        entry = _closed_object(
            raw_entry,
            frozenset({"path", "role", "sha256"}),
            "API_INPUT_CORPUS_SHAPE_INVALID",
        )
        try:
            corpus_path = _validate_repo_path(entry["path"])
        except RefreezeError as exc:
            raise RefreezeError("API_INPUT_CORPUS_PATH_INVALID") from exc
        role = entry["role"]
        if role not in expected_input_corpus_role_counts:
            raise RefreezeError("API_INPUT_CORPUS_ROLE_INVALID")
        if corpus_path not in expected_input_corpus_paths_by_role[role]:
            raise RefreezeError("API_INPUT_CORPUS_ROLE_PATH_INVALID")
        try:
            corpus_blob = git.read_blob(commit, corpus_path)
        except RefreezeError as exc:
            raise RefreezeError("API_INPUT_CORPUS_BLOB_MISSING") from exc
        corpus_digest = _sha256_value(
            entry["sha256"], "API_INPUT_CORPUS_DIGEST_INVALID"
        )
        if _sha256(corpus_blob) != corpus_digest:
            raise RefreezeError("API_INPUT_CORPUS_HASH_MISMATCH")
        input_corpus_role_counts[role] = input_corpus_role_counts.get(role, 0) + 1
        input_corpus_paths.append(corpus_path)
    if (
        input_corpus_paths != sorted(input_corpus_paths)
        or len(set(input_corpus_paths)) != len(input_corpus_paths)
    ):
        raise RefreezeError("API_INPUT_CORPUS_ORDER_INVALID")
    if input_corpus_role_counts != expected_input_corpus_role_counts:
        raise RefreezeError("API_INPUT_CORPUS_ROLE_CARDINALITY_INVALID")
    _validate_fixture_manifest(
        git,
        commit,
        api=api,
        api_raw_digest=raw_digest,
    )
    if _sha256(_canonical_json(api)) != canonical_digest:
        raise RefreezeError("API_CANONICAL_HASH_MISMATCH")
    return item


def _json_schema_shape(value: object) -> object:
    if isinstance(value, dict):
        return {
            "object": {
                key: _json_schema_shape(item) for key, item in sorted(value.items())
            }
        }
    if isinstance(value, list):
        encoded_shapes = {
            _canonical_json(_json_schema_shape(item)).decode("utf-8") for item in value
        }
        return {"array": [json.loads(item) for item in sorted(encoded_shapes)]}
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "boolean"
    if isinstance(value, int):
        return "integer"
    if isinstance(value, float):
        return "number"
    if isinstance(value, str):
        return "string"
    raise RefreezeError("API_SCHEMA_VALUE_INVALID")


def _api_nonempty_list(value: object, code: str) -> list[object]:
    if not isinstance(value, list) or not value:
        raise RefreezeError(code)
    return value


def _api_named_set(value: object, key: str) -> list[object]:
    items = _api_nonempty_list(value, "API_NAMED_SET_ORDER_INVALID")
    keys: list[str] = []
    for item in items:
        if not isinstance(item, dict) or not isinstance(item.get(key), str):
            raise RefreezeError("API_NAMED_SET_ORDER_INVALID")
        keys.append(item[key])
    if keys != sorted(set(keys)):
        raise RefreezeError("API_NAMED_SET_ORDER_INVALID")
    return items


def _validate_api_contract(api: dict[str, object]) -> None:
    shape_digest = _sha256(
        API_SCHEMA_SHAPE_DOMAIN + _canonical_json(_json_schema_shape(api))
    )
    if shape_digest != API_SCHEMA_SHAPE_SHA256:
        raise RefreezeError("API_CLOSED_SCHEMA_MISMATCH")

    policy = api["policy"]
    for field in (
        "closed_world",
        "doc_hidden_counts_as_public",
        "pin_upstream_blanket_impls",
    ):
        _exact(policy[field], True, "API_POLICY_NOT_CLOSED")
    for field in (
        "unknown_cfg_name",
        "unknown_cfg_value",
        "unknown_feature_vector",
        "unknown_target",
        "unlisted_export",
        "unlisted_impl",
        "unlisted_item",
        "unlisted_macro",
        "unlisted_module",
    ):
        _exact(policy[field], "reject", "API_POLICY_NOT_CLOSED")
    _exact(
        policy["array_ordering"],
        {
            "algebra_variant_arrays": "declared_semantic_order",
            "generic_argument_arrays": "positional",
            "named_set_arrays": "lexicographic_by_id_name_or_path",
            "struct_field_arrays": "lexicographic_by_name",
            "trait_impl_arrays": "canonical_tuple_by_subject_trait_polarity",
        },
        "API_ARRAY_ORDERING_INVALID",
    )
    associated_signature_fields = _sorted_unique_strings(
        policy["associated_item_signatures"]["required_fields"],
        "API_LEXICAL_SET_ORDER_INVALID",
    )
    if set(associated_signature_fields) != {
        "abi",
        "async",
        "availability",
        "cfg",
        "const",
        "generics",
        "unsafe",
        "where_predicates",
    }:
        raise RefreezeError("API_ASSOCIATED_METHOD_SIGNATURE_INVALID")

    crate = api["crate"]
    _exact(crate["package"], "symforge", "API_CRATE_IDENTITY_INVALID")
    _exact(crate["lib_crate"], "symforge", "API_CRATE_IDENTITY_INVALID")
    _exact(crate["api_major"], 11, "API_CRATE_IDENTITY_INVALID")
    _exact(crate["edition"], "2024", "API_CRATE_IDENTITY_INVALID")
    _exact(crate["identity_status"], "pre_activation_required", "API_PREACTIVATION_INVALID")
    if crate["source_commit"] is not None or crate["cargo_lock_sha256"] is not None:
        raise RefreezeError("API_PREACTIVATION_INVALID")
    toolchain = crate["toolchain"]
    _exact(toolchain["status"], "pre_activation_required", "API_PREACTIVATION_INVALID")
    for field in ("extractor_version", "rustc_commit", "rustdoc_format_version"):
        if toolchain[field] is not None:
            raise RefreezeError("API_PREACTIVATION_INVALID")
    _exact(toolchain["normalizer_version"], 1, "API_NORMALIZER_VERSION_INVALID")

    configuration = api["configuration_domain"]
    targets = _api_named_set(configuration["targets"], "id")
    feature_vectors = _api_named_set(configuration["feature_vectors"], "id")
    cfg_keys = _api_named_set(configuration["cfg_keys"], "name")
    cells = _api_named_set(configuration["cells"], "id")
    for cfg_key in cfg_keys:
        _sorted_unique_strings(
            cfg_key["allowed_values"],
            "API_LEXICAL_SET_ORDER_INVALID",
            allow_empty_items=True,
        )
    features = configuration["features"]
    _exact(
        features["cbm_spike_public_graph_effect"],
        "forbidden",
        "API_CONFIGURATION_FEATURE_INVALID",
    )
    declared_features = set(
        _sorted_unique_strings(
            features["declared"], "API_CONFIGURATION_FEATURE_INVALID"
        )
    )
    for target in targets:
        _exact(
            target["rustc_cfg_status"],
            "pre_activation_required",
            "API_PREACTIVATION_INVALID",
        )
        if target["rustc_cfg_sha256"] is not None:
            raise RefreezeError("API_PREACTIVATION_INVALID")
    cover = configuration["cover"]
    _exact(cover["status"], "pre_activation_required", "API_PREACTIVATION_INVALID")
    _exact(
        cover["mode"],
        "explicit-cells-or-bdd-equivalence",
        "API_CONFIGURATION_COVER_INVALID",
    )
    if (
        cover["assignment_proof_sha256"] is not None
        or cover["predicate_inventory_sha256"] is not None
    ):
        raise RefreezeError("API_PREACTIVATION_INVALID")
    target_ids = {target["id"] for target in targets}
    feature_vector_ids = {vector["id"] for vector in feature_vectors}
    feature_vectors_by_id = {vector["id"]: vector for vector in feature_vectors}
    if len(target_ids) != len(targets) or len(feature_vector_ids) != len(feature_vectors):
        raise RefreezeError("API_CONFIGURATION_ID_INVALID")
    default_features = set(
        _sorted_unique_strings(
            features["default"],
            "API_CONFIGURATION_FEATURE_INVALID",
            nonempty=False,
        )
    )
    if not default_features.issubset(declared_features):
        raise RefreezeError("API_CONFIGURATION_FEATURE_REFERENCE_INVALID")
    rejected_vectors: set[tuple[str, ...]] = set()
    rejected_vector_order: list[tuple[str, ...]] = []
    if not isinstance(features["rejected_vectors"], list):
        raise RefreezeError("API_CONFIGURATION_FEATURE_INVALID")
    for raw_rejected in features["rejected_vectors"]:
        rejected = tuple(
            _sorted_unique_strings(
                raw_rejected,
                "API_CONFIGURATION_FEATURE_INVALID",
                nonempty=False,
            )
        )
        if not set(rejected).issubset(declared_features):
            raise RefreezeError("API_CONFIGURATION_FEATURE_REFERENCE_INVALID")
        if rejected in rejected_vectors:
            raise RefreezeError("API_CONFIGURATION_FEATURE_INVALID")
        rejected_vectors.add(rejected)
        rejected_vector_order.append(rejected)
    if rejected_vector_order != sorted(rejected_vector_order):
        raise RefreezeError("API_CONFIGURATION_FEATURE_INVALID")
    for vector in feature_vectors:
        requested = set(
            _sorted_unique_strings(
                vector["requested"], "API_CONFIGURATION_FEATURE_INVALID"
            )
        )
        resolved = set(
            _sorted_unique_strings(
                vector["resolved"], "API_CONFIGURATION_FEATURE_INVALID"
            )
        )
        disabled = set(
            _sorted_unique_strings(
                vector["disabled"],
                "API_CONFIGURATION_FEATURE_INVALID",
                nonempty=False,
            )
        )
        if not (requested | resolved | disabled).issubset(declared_features):
            raise RefreezeError("API_CONFIGURATION_FEATURE_REFERENCE_INVALID")
        if (
            not requested.issubset(resolved)
            or resolved & disabled
            or resolved | disabled != declared_features
            or (
                vector["default_features"] is True
                and not default_features.issubset(resolved)
            )
            or (
                vector["default_features"] is False
                and not default_features.issubset(disabled)
            )
        ):
            raise RefreezeError("API_CONFIGURATION_FEATURE_INVALID")
    for profile_vector in configuration["profiles"].values():
        if profile_vector not in feature_vector_ids:
            raise RefreezeError("API_CONFIGURATION_REFERENCE_INVALID")
    supported_cells: set[tuple[str, str]] = set()
    for target in targets:
        _sorted_unique_strings(
            target["atomic_widths"], "API_LEXICAL_SET_ORDER_INVALID"
        )
        supported = _sorted_unique_strings(
            target["supported_feature_vectors"],
            "API_CONFIGURATION_REFERENCE_INVALID",
        )
        if any(vector not in feature_vector_ids for vector in supported):
            raise RefreezeError("API_CONFIGURATION_REFERENCE_INVALID")
        supported_cells.update((target["id"], vector) for vector in supported)

    expected = api["expected_graph"]
    rust_value_identifier_re = re.compile(r"^(?!_$)[a-z_][a-z0-9_]*$")
    rust_type_identifier_re = re.compile(r"^[A-Z][A-Za-z0-9]*$")
    rust_identifier_component_re = re.compile(r"^(?!_$)[A-Za-z_][A-Za-z0-9_]*$")
    rust_reserved_identifiers = frozenset(
        {
            "Self",
            "abstract",
            "as",
            "async",
            "await",
            "become",
            "box",
            "break",
            "const",
            "continue",
            "crate",
            "do",
            "dyn",
            "else",
            "enum",
            "extern",
            "false",
            "final",
            "fn",
            "for",
            "gen",
            "if",
            "impl",
            "in",
            "let",
            "loop",
            "macro",
            "match",
            "mod",
            "move",
            "mut",
            "override",
            "priv",
            "pub",
            "ref",
            "return",
            "self",
            "static",
            "struct",
            "super",
            "trait",
            "true",
            "try",
            "type",
            "typeof",
            "unsafe",
            "unsized",
            "use",
            "virtual",
            "where",
            "while",
            "yield",
        }
    )

    def is_rust_value_identifier(value: object) -> bool:
        return (
            isinstance(value, str)
            and rust_value_identifier_re.fullmatch(value) is not None
            and value not in rust_reserved_identifiers
        )

    def is_rust_type_identifier(value: object) -> bool:
        return (
            isinstance(value, str)
            and rust_type_identifier_re.fullmatch(value) is not None
            and value not in rust_reserved_identifiers
        )

    def has_canonical_rust_path_components(path: str) -> bool:
        return all(
            rust_identifier_component_re.fullmatch(component) is not None
            and component not in rust_reserved_identifiers
            for component in path.removeprefix("::").split("::")
        )

    _exact(expected["status"], "normative_expected_graph", "API_EXPECTED_GRAPH_INVALID")
    for field in (
        "exports",
        "functions",
        "graph_projections",
        "impls",
        "items",
        "modules",
        "trait_impls",
    ):
        _api_nonempty_list(expected[field], "API_EXPECTED_GRAPH_VACUOUS")
    _api_nonempty_list(
        expected["auto_traits"]["universe"], "API_EXPECTED_GRAPH_VACUOUS"
    )
    _api_nonempty_list(
        expected["auto_traits"]["expectation_groups"],
        "API_EXPECTED_GRAPH_VACUOUS",
    )
    if not expected["semantic_algebras"]:
        raise RefreezeError("API_EXPECTED_GRAPH_VACUOUS")
    semantic_algebras = expected["semantic_algebras"]
    for algebra_name in ("Claim", "EvaluationProvenance", "OperationReceipt"):
        fields = _sorted_unique_strings(
            semantic_algebras[algebra_name]["fields"],
            "API_LEXICAL_SET_ORDER_INVALID",
        )
        if any(not is_rust_value_identifier(field) for field in fields):
            raise RefreezeError("API_RUST_IDENTIFIER_INVALID")
    for algebra_name in ("AtomicAuthority", "ClaimProvenance", "SourceRefusal"):
        variant_names: list[str] = []
        for variant in _api_nonempty_list(
            semantic_algebras[algebra_name]["variants"],
            "API_ALGEBRA_VARIANT_INVALID",
        ):
            if not is_rust_type_identifier(variant["name"]):
                raise RefreezeError("API_RUST_IDENTIFIER_INVALID")
            variant_names.append(variant["name"])
            fields = _sorted_unique_strings(
                variant["fields"], "API_LEXICAL_SET_ORDER_INVALID"
            )
            if any(not is_rust_value_identifier(field) for field in fields):
                raise RefreezeError("API_RUST_IDENTIFIER_INVALID")
        if len(set(variant_names)) != len(variant_names):
            raise RefreezeError("API_ALGEBRA_VARIANT_INVALID")
    unavailable_variant_names: list[str] = []
    for variant in _api_nonempty_list(
        semantic_algebras["UnavailableCause"]["legal_variants"],
        "API_ALGEBRA_VARIANT_INVALID",
    ):
        if not is_rust_type_identifier(variant["name"]):
            raise RefreezeError("API_RUST_IDENTIFIER_INVALID")
        unavailable_variant_names.append(variant["name"])
        retry_variants = _sorted_unique_strings(
            variant["retry"], "API_LEXICAL_SET_ORDER_INVALID"
        )
        if (
            not is_rust_type_identifier(variant["basis"])
            or any(
                not is_rust_type_identifier(retry_variant)
                for retry_variant in retry_variants
            )
        ):
            raise RefreezeError("API_RUST_IDENTIFIER_INVALID")
    if len(set(unavailable_variant_names)) != len(unavailable_variant_names):
        raise RefreezeError("API_ALGEBRA_VARIANT_INVALID")
    _api_nonempty_list(
        expected["type_ast"]["closed_kinds"], "API_EXPECTED_GRAPH_VACUOUS"
    )
    _exact(
        expected["type_ast"]["rendered_rust_strings_are_normative"],
        False,
        "API_TYPE_AST_INVALID",
    )
    _exact(expected["type_ast"]["version"], 1, "API_TYPE_AST_INVALID")
    graph_projections = _api_named_set(expected["graph_projections"], "id")
    projection_ids = {projection["id"] for projection in graph_projections}
    cell_pairs: list[tuple[str, str]] = []
    cell_graph_by_pair: dict[tuple[str, str], str] = {}
    for cell in cells:
        if (
            cell["target"] not in target_ids
            or cell["feature_vector"] not in feature_vector_ids
            or cell["expected_graph"] not in projection_ids
        ):
            raise RefreezeError("API_CONFIGURATION_REFERENCE_INVALID")
        cell_pair = (cell["target"], cell["feature_vector"])
        cell_pairs.append(cell_pair)
        cell_graph_by_pair[cell_pair] = cell["expected_graph"]
    if len(set(cell_pairs)) != len(cell_pairs) or set(cell_pairs) != supported_cells:
        raise RefreezeError("API_CONFIGURATION_COVERAGE_INVALID")

    modules = _api_named_set(expected["modules"], "path")
    modules_by_path = {module["path"]: module for module in modules}
    module_paths = set(modules_by_path)
    if len(module_paths) != len(modules):
        raise RefreezeError("API_EXPECTED_GRAPH_REFERENCE_INVALID")
    for module_path in module_paths:
        components = module_path.split("::")
        if (
            components[0] != "symforge"
            or any(not is_rust_value_identifier(component) for component in components)
            or (
                len(components) > 1
                and module_path.rpartition("::")[0] not in module_paths
            )
        ):
            raise RefreezeError("API_MODULE_PATH_INVALID")
    type_declaration_id_re = re.compile(
        r"^type:(?P<module>[a-z][a-z0-9_]*):(?P<name>[A-Z][A-Za-z0-9]*)$"
    )
    function_declaration_id_re = re.compile(
        r"^function:(?P<module>[a-z][a-z0-9_]*):"
        r"(?P<name>(?:[a-z][a-z0-9_]*|_[a-z0-9_]+))$"
    )
    items = _api_named_set(expected["items"], "id")
    item_ids = {item["id"] for item in items}
    items_by_id = {item["id"]: item for item in items}
    items_by_public_path: dict[str, dict[str, object]] = {}
    for item in items:
        item_id_match = type_declaration_id_re.fullmatch(item["id"])
        if (
            item_id_match is None
            or not is_rust_value_identifier(item_id_match["module"])
            or not is_rust_type_identifier(item_id_match["name"])
        ):
            raise RefreezeError("API_DECLARATION_ID_INVALID")
        items_by_public_path[
            f"symforge::{item_id_match['module']}::{item_id_match['name']}"
        ] = item
    for item in items:
        definition = item["definition"]
        if item["kind"] == "enum":
            if not isinstance(definition, dict) or frozenset(definition) != {
                "variants"
            }:
                raise RefreezeError("API_ITEM_DEFINITION_INVALID")
            variants = _api_nonempty_list(
                definition["variants"], "API_ITEM_DEFINITION_INVALID"
            )
            if not all(isinstance(variant, str) and variant for variant in variants):
                raise RefreezeError("API_ITEM_DEFINITION_INVALID")
            if any(not is_rust_type_identifier(variant) for variant in variants):
                raise RefreezeError("API_RUST_IDENTIFIER_INVALID")
            if len(set(variants)) != len(variants):
                raise RefreezeError("API_ITEM_DEFINITION_INVALID")
        elif item["kind"] == "struct":
            if (
                not isinstance(definition, dict)
                or frozenset(definition)
                != {"fields", "has_nonpublic_fields", "shape"}
                or definition["shape"] != "struct"
                or not isinstance(definition["has_nonpublic_fields"], bool)
                or not isinstance(definition["fields"], list)
            ):
                raise RefreezeError("API_ITEM_DEFINITION_INVALID")
            field_names: list[str] = []
            for field in definition["fields"]:
                if (
                    not isinstance(field, dict)
                    or frozenset(field) != {"name", "type", "visibility"}
                    or not isinstance(field["name"], str)
                    or not field["name"]
                    or field["visibility"] != "public"
                ):
                    raise RefreezeError("API_ITEM_DEFINITION_INVALID")
                if not is_rust_value_identifier(field["name"]):
                    raise RefreezeError("API_RUST_IDENTIFIER_INVALID")
                field_names.append(field["name"])
            if field_names != sorted(set(field_names)):
                raise RefreezeError("API_STRUCT_FIELD_ORDER_INVALID")
        else:
            raise RefreezeError("API_ITEM_DEFINITION_INVALID")
    retry_advice_item = items_by_id.get("type:embed:RetryAdvice")
    if retry_advice_item is None or retry_advice_item["kind"] != "enum":
        raise RefreezeError("API_SEMANTIC_ALGEBRA_REFERENCE_INVALID")
    declared_retry_advice = set(retry_advice_item["definition"]["variants"])
    semantic_retry_advice = {
        retry
        for variant in semantic_algebras["UnavailableCause"]["legal_variants"]
        for retry in variant["retry"]
    }
    if semantic_retry_advice != declared_retry_advice:
        raise RefreezeError("API_SEMANTIC_ALGEBRA_REFERENCE_INVALID")
    source_refusal_kind_item = items_by_id.get("type:embed:SourceRefusalKind")
    if (
        source_refusal_kind_item is None
        or source_refusal_kind_item["kind"] != "enum"
    ):
        raise RefreezeError("API_SEMANTIC_ALGEBRA_REFERENCE_INVALID")
    declared_source_refusal_kinds = set(
        source_refusal_kind_item["definition"]["variants"]
    )
    semantic_source_refusal_kinds = {
        variant["name"]
        for variant in semantic_algebras["SourceRefusal"]["variants"]
    }
    if semantic_source_refusal_kinds != declared_source_refusal_kinds:
        raise RefreezeError("API_SEMANTIC_ALGEBRA_REFERENCE_INVALID")
    functions = _api_named_set(expected["functions"], "id")
    function_ids = {function["id"] for function in functions}
    function_id_matches = [
        function_declaration_id_re.fullmatch(function["id"])
        for function in functions
    ]
    if (
        len(item_ids) != len(items)
        or len(function_ids) != len(functions)
        or item_ids & function_ids
        or any(
            match is None
            or not is_rust_value_identifier(match["module"])
            or not is_rust_value_identifier(match["name"])
            for match in function_id_matches
        )
    ):
        raise RefreezeError("API_DECLARATION_ID_INVALID")
    if len(items_by_public_path) != len(items):
        raise RefreezeError("API_EXPECTED_GRAPH_REFERENCE_INVALID")
    type_ast_fields = {
        "api": frozenset({"id", "kind"}),
        "generic": frozenset({"arguments", "kind", "path"}),
        "generic-parameter": frozenset({"binder", "kind"}),
        "path": frozenset({"kind", "path"}),
        "primitive": frozenset({"kind", "name"}),
        "receiver": frozenset({"kind", "mode"}),
        "reference": frozenset({"kind", "lifetime", "mutable", "target"}),
        "slice": frozenset({"element", "kind"}),
    }
    _exact(
        expected["type_ast"]["closed_kinds"],
        sorted(type_ast_fields),
        "API_TYPE_AST_INVALID",
    )
    rust_primitives = frozenset(
        {
            "bool",
            "char",
            "f32",
            "f64",
            "i8",
            "i16",
            "i32",
            "i64",
            "i128",
            "isize",
            "str",
            "u8",
            "u16",
            "u32",
            "u64",
            "u128",
            "usize",
        }
    )
    rust_path_re = re.compile(
        r"^(?:::)?[A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)*$"
    )
    canonical_trait_path_re = re.compile(
        r"^[A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)*$"
    )
    local_type_path_roots = frozenset({"crate", "self", "super", "symforge"})
    def validate_type_ast(
        node: object,
        *,
        type_binders: frozenset[str] = frozenset(),
        allow_receiver: bool = False,
    ) -> None:
        if not isinstance(node, dict):
            raise RefreezeError("API_TYPE_AST_INVALID")
        kind = node.get("kind")
        if kind not in type_ast_fields or frozenset(node) != type_ast_fields[kind]:
            raise RefreezeError("API_TYPE_AST_INVALID")
        if kind == "api":
            if not isinstance(node["id"], str) or node["id"] not in item_ids:
                raise RefreezeError("API_TYPE_REFERENCE_INVALID")
            if items_by_id[node["id"]]["generics"]:
                raise RefreezeError("API_TYPE_AST_ARITY_INVALID")
            return
        if kind == "generic":
            if not isinstance(node["path"], str) or not rust_path_re.fullmatch(
                node["path"]
            ):
                raise RefreezeError("API_TYPE_AST_INVALID")
            arguments = _api_nonempty_list(
                node["arguments"], "API_TYPE_AST_INVALID"
            )
            normalized_path = node["path"].removeprefix("::")
            local_item = items_by_public_path.get(normalized_path)
            local_root = normalized_path.partition("::")[0]
            if local_root in local_type_path_roots and (
                local_root != "symforge" or local_item is None
            ):
                raise RefreezeError("API_TYPE_REFERENCE_INVALID")
            if not has_canonical_rust_path_components(node["path"]):
                raise RefreezeError("API_RUST_PATH_INVALID")
            if local_item is not None and len(arguments) != len(local_item["generics"]):
                raise RefreezeError("API_TYPE_AST_ARITY_INVALID")
            for argument in arguments:
                validate_type_ast(argument, type_binders=type_binders)
            return
        if kind == "generic-parameter":
            if node["binder"] not in type_binders:
                raise RefreezeError("API_TYPE_AST_BINDER_INVALID")
            return
        if kind == "path":
            if not isinstance(node["path"], str) or not rust_path_re.fullmatch(
                node["path"]
            ):
                raise RefreezeError("API_TYPE_AST_INVALID")
            normalized_path = node["path"].removeprefix("::")
            if normalized_path.partition("::")[0] in local_type_path_roots:
                raise RefreezeError("API_TYPE_REFERENCE_INVALID")
            if not has_canonical_rust_path_components(node["path"]):
                raise RefreezeError("API_RUST_PATH_INVALID")
            return
        if kind == "primitive":
            if node["name"] not in rust_primitives:
                raise RefreezeError("API_TYPE_AST_INVALID")
            return
        if kind == "receiver":
            if not allow_receiver or node["mode"] != "shared-reference":
                raise RefreezeError("API_TYPE_AST_INVALID")
            return
        if kind == "reference":
            if not isinstance(node["mutable"], bool) or node["lifetime"] not in {
                "elided",
                "static",
            }:
                raise RefreezeError("API_TYPE_AST_INVALID")
            validate_type_ast(node["target"], type_binders=type_binders)
            return
        validate_type_ast(node["element"], type_binders=type_binders)

    def declared_type_binders(owner: dict[str, object]) -> frozenset[str]:
        raw_generics = owner["generics"]
        if not isinstance(raw_generics, list):
            raise RefreezeError("API_TYPE_AST_BINDER_INVALID")
        binders: list[str] = []
        for generic in raw_generics:
            if (
                not isinstance(generic, dict)
                or frozenset(generic) != {"binder", "kind"}
                or generic["kind"] != "type"
                or not isinstance(generic["binder"], str)
                or rust_type_identifier_re.fullmatch(generic["binder"]) is None
            ):
                raise RefreezeError("API_TYPE_AST_BINDER_INVALID")
            if not is_rust_type_identifier(generic["binder"]):
                raise RefreezeError("API_RUST_IDENTIFIER_INVALID")
            binders.append(generic["binder"])
        if len(set(binders)) != len(binders):
            raise RefreezeError("API_TYPE_AST_BINDER_INVALID")
        return frozenset(binders)

    def validate_named_inputs(
        raw_inputs: object, type_binders: frozenset[str]
    ) -> None:
        if not isinstance(raw_inputs, list):
            raise RefreezeError("API_TYPE_AST_INVALID")
        input_names: set[str] = set()
        for input_item in raw_inputs:
            if (
                not isinstance(input_item, dict)
                or frozenset(input_item) != {"name", "type"}
                or not isinstance(input_item["name"], str)
                or not input_item["name"]
                or input_item["name"] in input_names
            ):
                raise RefreezeError("API_TYPE_AST_INVALID")
            if not is_rust_value_identifier(input_item["name"]):
                raise RefreezeError("API_RUST_IDENTIFIER_INVALID")
            input_names.add(input_item["name"])
            validate_type_ast(input_item["type"], type_binders=type_binders)

    for item in items:
        item_binders = declared_type_binders(item)
        if item["kind"] == "struct":
            for field in item["definition"]["fields"]:
                validate_type_ast(field["type"], type_binders=item_binders)
    for function in functions:
        validate_named_inputs(function["inputs"], frozenset())
        validate_type_ast(function["output"])
    for implementation in _api_nonempty_list(
        expected["impls"], "API_EXPECTED_GRAPH_VACUOUS"
    ):
        owner_item = items_by_id.get(implementation["for"])
        if owner_item is None:
            raise RefreezeError("API_EXPECTED_GRAPH_REFERENCE_INVALID")
        if implementation["generics"] != owner_item["generics"]:
            raise RefreezeError("API_IMPL_GENERIC_BINDING_INVALID")
        implementation_binders = declared_type_binders(implementation)
        for associated_item in _api_nonempty_list(
            implementation["associated_items"], "API_ASSOCIATED_METHODS_VACUOUS"
        ):
            inputs = associated_item["inputs"]
            if not isinstance(inputs, list):
                raise RefreezeError("API_TYPE_AST_INVALID")
            if (
                inputs
                and isinstance(inputs[0], dict)
                and inputs[0].get("kind") == "receiver"
            ):
                validate_type_ast(
                    inputs[0],
                    type_binders=implementation_binders,
                    allow_receiver=True,
                )
                validate_named_inputs(inputs[1:], implementation_binders)
            else:
                validate_named_inputs(inputs, implementation_binders)
            validate_type_ast(
                associated_item["output"], type_binders=implementation_binders
            )
    public_item_ids = item_ids | function_ids
    public_items_by_id = {
        entry["id"]: entry for entry in [*items, *functions]
    }
    exports = _api_named_set(expected["exports"], "path")
    export_paths = {export["path"] for export in exports}
    export_items = [export["item"] for export in exports]
    export_paths_by_item = {export["item"]: export["path"] for export in exports}
    if (
        len(export_paths) != len(exports)
        or len(set(export_items)) != len(export_items)
        or set(export_items) != public_item_ids
    ):
        raise RefreezeError("API_EXPECTED_GRAPH_REFERENCE_INVALID")
    for export in exports:
        parent_path, separator, _ = export["path"].rpartition("::")
        if not separator or parent_path not in module_paths:
            raise RefreezeError("API_EXPORT_PARENT_INVALID")
        item_id_parts = export["item"].split(":")
        if (
            len(item_id_parts) != 3
            or item_id_parts[0] not in {"function", "type"}
            or export["path"]
            != f"symforge::{item_id_parts[1]}::{item_id_parts[2]}"
        ):
            raise RefreezeError("API_EXPORT_PATH_INVALID")
        if (
            export["availability"] != modules_by_path[parent_path]["availability"]
            or export["availability"]
            != public_items_by_id[export["item"]]["availability"]
        ):
            raise RefreezeError("API_EXPORT_AVAILABILITY_INVALID")
        expected_namespace = "value" if export["item"] in function_ids else "type"
        if export["namespace"] != expected_namespace:
            raise RefreezeError("API_EXPORT_NAMESPACE_INVALID")
    type_export_paths = {
        export["path"] for export in exports if export["namespace"] == "type"
    }
    type_items_by_export_path = {
        export["path"]: items_by_id[export["item"]]
        for export in exports
        if export["namespace"] == "type"
    }
    upstream_trait_universe = frozenset(
        {
            "core::borrow::Borrow",
            "core::clone::Clone",
            "core::cmp::Eq",
            "core::cmp::PartialEq",
            "core::convert::AsRef",
            "core::convert::From",
            "core::default::Default",
            "core::fmt::Debug",
            "core::fmt::Display",
            "core::marker::Copy",
            "core::marker::Send",
            "core::marker::Sync",
            "core::marker::Unpin",
            "core::ops::Deref",
            "core::ops::Drop",
            "serde::Deserialize",
            "std::error::Error",
            "std::panic::RefUnwindSafe",
            "std::panic::UnwindSafe",
        }
    )
    trait_impl_keys: list[tuple[str, str, str]] = []
    for trait_impl in expected["trait_impls"]:
        if (
            trait_impl["for"] not in type_export_paths
            or trait_impl["polarity"] not in {"negative", "positive"}
        ):
            raise RefreezeError("API_TRAIT_IMPL_INVALID")
        if (
            not isinstance(trait_impl["trait"], str)
            or not canonical_trait_path_re.fullmatch(trait_impl["trait"])
            or not has_canonical_rust_path_components(trait_impl["trait"])
        ):
            raise RefreezeError("API_TRAIT_PATH_INVALID")
        if trait_impl["trait"] not in upstream_trait_universe:
            raise RefreezeError("API_TRAIT_REFERENCE_INVALID")
        trait_impl_keys.append(
            (trait_impl["for"], trait_impl["trait"], trait_impl["polarity"])
        )
    if trait_impl_keys != sorted(set(trait_impl_keys)):
        raise RefreezeError("API_TRAIT_IMPL_INVALID")
    trait_impl_pairs = [(subject, trait) for subject, trait, _ in trait_impl_keys]
    if len(set(trait_impl_pairs)) != len(trait_impl_pairs):
        raise RefreezeError("API_TRAIT_IMPL_CONTRADICTION")
    positive_trait_edges = {
        (subject, trait)
        for subject, trait, polarity in trait_impl_keys
        if polarity == "positive"
    }
    auto_traits = expected["auto_traits"]
    auto_trait_paths = _sorted_unique_strings(
        auto_traits["universe"], "API_AUTO_TRAIT_UNIVERSE_INVALID"
    )
    if any(
        canonical_trait_path_re.fullmatch(trait) is None
        or not has_canonical_rust_path_components(trait)
        for trait in auto_trait_paths
    ):
        raise RefreezeError("API_TRAIT_PATH_INVALID")
    auto_trait_universe = set(auto_trait_paths)
    auto_trait_subjects: list[str] = []
    auto_trait_states: dict[tuple[str, str], str] = {}
    for expectation_group in auto_traits["expectation_groups"]:
        subjects = _sorted_unique_strings(
            expectation_group["subjects"], "API_AUTO_TRAIT_SUBJECT_INVALID"
        )
        if set(expectation_group["states"]) != auto_trait_universe:
            raise RefreezeError("API_AUTO_TRAIT_UNIVERSE_INVALID")
        if any(
            state not in {"conditional:T", "negative", "positive"}
            for state in expectation_group["states"].values()
        ):
            raise RefreezeError("API_AUTO_TRAIT_STATE_INVALID")
        auto_trait_subjects.extend(subjects)
        for subject in subjects:
            for trait, state in expectation_group["states"].items():
                auto_trait_states[(subject, trait)] = state
    if (
        len(set(auto_trait_subjects)) != len(auto_trait_subjects)
        or set(auto_trait_subjects) != type_export_paths
    ):
        raise RefreezeError("API_AUTO_TRAIT_SUBJECT_INVALID")
    for (subject, _), state in auto_trait_states.items():
        if state != "conditional:T":
            continue
        subject_generics = type_items_by_export_path[subject]["generics"]
        if len(subject_generics) != 1 or subject_generics[0]["kind"] != "type":
            raise RefreezeError("API_AUTO_TRAIT_CONDITIONAL_BINDING_INVALID")
    for subject, trait, polarity in trait_impl_keys:
        auto_state = auto_trait_states.get((subject, trait))
        if auto_state is not None and auto_state != polarity:
            raise RefreezeError("API_TRAIT_IMPL_CONTRADICTION")
    for projection in graph_projections:
        included = set(
            _sorted_unique_strings(
                projection["included_modules"],
                "API_EXPECTED_GRAPH_REFERENCE_INVALID",
            )
        )
        excluded = set(
            _sorted_unique_strings(
                projection["excluded_modules"],
                "API_EXPECTED_GRAPH_REFERENCE_INVALID",
                nonempty=False,
            )
        )
        if included & excluded or included | excluded != module_paths:
            raise RefreezeError("API_EXPECTED_GRAPH_REFERENCE_INVALID")
    implementation_ids: set[str] = set()
    associated_item_ids: set[str] = set()
    associated_method_keys: set[tuple[str, str]] = set()
    associated_atom_paths: set[str] = set()
    implementations = _api_named_set(expected["impls"], "id")
    for implementation in implementations:
        if implementation["for"] not in item_ids:
            raise RefreezeError("API_EXPECTED_GRAPH_REFERENCE_INVALID")
        for associated_item in _api_named_set(
            implementation["associated_items"], "id"
        ):
            method_name = associated_item["name"]
            if not is_rust_value_identifier(method_name):
                raise RefreezeError("API_ASSOCIATED_ITEM_NAME_INVALID")
            method_key = (implementation["for"], method_name)
            if method_key in associated_method_keys:
                raise RefreezeError("API_ASSOCIATED_ITEM_DUPLICATE")
            associated_method_keys.add(method_key)
    for implementation in implementations:
        implementation_id = implementation["id"]
        if implementation_id in implementation_ids or implementation["for"] not in item_ids:
            raise RefreezeError("API_EXPECTED_GRAPH_REFERENCE_INVALID")
        implementation_ids.add(implementation_id)
        owner_path = export_paths_by_item[implementation["for"]]
        owner_name = owner_path.rsplit("::", 1)[-1]
        owner_generics = items_by_id[implementation["for"]]["generics"]
        if not owner_generics:
            expected_implementation_id = f"impl:{owner_name}"
        elif len(owner_generics) == 1 and owner_generics[0]["kind"] == "type":
            expected_implementation_id = f"impl:{owner_name}<T>"
        else:
            raise RefreezeError("API_IMPL_ID_INVALID")
        if implementation_id != expected_implementation_id:
            raise RefreezeError("API_IMPL_ID_INVALID")
        associated_items = _api_named_set(implementation["associated_items"], "id")
        for associated_item in associated_items:
            associated_id = associated_item["id"]
            if associated_id in associated_item_ids:
                raise RefreezeError("API_EXPECTED_GRAPH_REFERENCE_INVALID")
            method_name = associated_item["name"]
            if associated_id != f"method:{owner_name}:{method_name}":
                raise RefreezeError("API_ASSOCIATED_ITEM_ID_INVALID")
            associated_item_ids.add(associated_id)
            associated_atom_paths.add(f"{owner_path}::{method_name}")
            _exact(
                associated_item["effective_signature"],
                {
                    "abi": "Rust",
                    "async": False,
                    "availability": "inherited_from_owning_export",
                    "cfg": "inherited_from_owning_export",
                    "const": False,
                    "generics": [],
                    "unsafe": False,
                    "where_predicates": [],
                },
                "API_ASSOCIATED_METHOD_SIGNATURE_INVALID",
            )

    negative_assertions = _api_nonempty_list(
        api["negative_assertions"], "API_NEGATIVE_ASSERTIONS_VACUOUS"
    )
    negative_assertion_specs = {
        "authority-types-01-not-deserialize": (
            "impl-absent",
            frozenset({"id", "kind", "subjects", "trait"}),
        ),
        "authority-types-02-not-default": (
            "impl-absent",
            frozenset({"id", "kind", "subjects", "trait"}),
        ),
        "authority-types-03-not-from": (
            "impl-family-absent-except-reflexive",
            frozenset(
                {"id", "kind", "permitted_source", "subjects", "trait_family"}
            ),
        ),
        "cbm-spike-no-public-graph-delta": (
            "graph-equivalence",
            frozenset({"id", "kind", "pairs"}),
        ),
        "embedded-source-handle-not-clone": (
            "impl-absent",
            frozenset({"id", "kind", "subjects", "trait"}),
        ),
        "no-raw-crate-root-modules": (
            "export-prefix-absent",
            frozenset({"forbidden_prefixes", "id", "kind"}),
        ),
        "no-raw-deep-embed-reexports": (
            "export-prefix-absent",
            frozenset({"forbidden_prefixes", "id", "kind"}),
        ),
        "no-rust-health-api": (
            "export-prefix-absent",
            frozenset({"forbidden_prefixes", "id", "kind"}),
        ),
        "no-stel-public-api": (
            "export-prefix-absent",
            frozenset({"forbidden_prefixes", "id", "kind"}),
        ),
        "runtime-handle-no-as-ref": (
            "impl-family-absent",
            frozenset({"id", "kind", "subjects", "trait_family"}),
        ),
        "runtime-handle-no-borrow": (
            "impl-family-absent-except-reflexive",
            frozenset(
                {"id", "kind", "permitted_source", "subjects", "trait_family"}
            ),
        ),
        "runtime-handle-no-deref": (
            "impl-absent",
            frozenset({"id", "kind", "subjects", "trait"}),
        ),
    }
    negative_assertion_traits = {
        "authority-types-01-not-deserialize": ("trait", "serde::Deserialize"),
        "authority-types-02-not-default": ("trait", "core::default::Default"),
        "authority-types-03-not-from": (
            "trait_family",
            "core::convert::From<_>",
        ),
        "embedded-source-handle-not-clone": ("trait", "core::clone::Clone"),
        "runtime-handle-no-as-ref": (
            "trait_family",
            "core::convert::AsRef<_>",
        ),
        "runtime-handle-no-borrow": (
            "trait_family",
            "core::borrow::Borrow<_>",
        ),
        "runtime-handle-no-deref": ("trait", "core::ops::Deref"),
    }
    assertion_ids = [assertion["id"] for assertion in negative_assertions]
    if assertion_ids != sorted(negative_assertion_specs):
        raise RefreezeError("API_NEGATIVE_ASSERTION_INVENTORY_INVALID")
    allowed_assertion_kinds = {
        expected_kind for expected_kind, _ in negative_assertion_specs.values()
    }
    for assertion in negative_assertions:
        assertion_id = assertion["id"]
        kind = assertion["kind"]
        if kind not in allowed_assertion_kinds:
            raise RefreezeError("API_NEGATIVE_ASSERTION_KIND_INVALID")
        expected_kind, expected_fields = negative_assertion_specs[assertion_id]
        if kind != expected_kind or frozenset(assertion) != expected_fields:
            raise RefreezeError("API_NEGATIVE_ASSERTION_FIELDS_INVALID")
        expected_trait = negative_assertion_traits.get(assertion_id)
        if expected_trait is not None:
            trait_field, trait_value = expected_trait
            if assertion[trait_field] != trait_value:
                raise RefreezeError(
                    "API_NEGATIVE_ASSERTION_SEMANTICS_INVALID"
                )
        if kind == "impl-family-absent-except-reflexive":
            if (
                assertion["permitted_source"] != "Self"
                or assertion["trait_family"]
                not in {"core::borrow::Borrow<_>", "core::convert::From<_>"}
            ):
                raise RefreezeError("API_NEGATIVE_ASSERTION_EXCEPTION_INVALID")
        if kind == "graph-equivalence":
            pairs: list[tuple[str, str]] = []
            for raw_pair in _api_nonempty_list(
                assertion["pairs"], "API_NEGATIVE_ASSERTION_PAIR_INVALID"
            ):
                if (
                    not isinstance(raw_pair, list)
                    or len(raw_pair) != 2
                    or not all(isinstance(value, str) for value in raw_pair)
                ):
                    raise RefreezeError("API_NEGATIVE_ASSERTION_PAIR_INVALID")
                pair = (raw_pair[0], raw_pair[1])
                if (
                    pair[0] == pair[1]
                    or pair[0] not in feature_vector_ids
                    or pair[1] not in feature_vector_ids
                ):
                    raise RefreezeError("API_NEGATIVE_ASSERTION_PAIR_INVALID")
                pairs.append(pair)
            if pairs != sorted(set(pairs)):
                raise RefreezeError("API_NEGATIVE_ASSERTION_PAIR_INVALID")
            endpoints = [endpoint for pair in pairs for endpoint in pair]
            if (
                len(set(endpoints)) != len(endpoints)
                or set(endpoints) != feature_vector_ids
            ):
                raise RefreezeError(
                    "API_NEGATIVE_ASSERTION_PAIR_COVERAGE_INVALID"
                )
            for left_id, right_id in pairs:
                if right_id != f"{left_id}-cbm-spike":
                    raise RefreezeError(
                        "API_NEGATIVE_ASSERTION_PAIR_COVERAGE_INVALID"
                    )
                left = feature_vectors_by_id[left_id]
                right = feature_vectors_by_id[right_id]
                if (
                    set(right["requested"])
                    != set(left["requested"]) | {"cbm-spike"}
                    or set(right["resolved"])
                    != set(left["resolved"]) | {"cbm-spike"}
                    or set(right["disabled"])
                    != set(left["disabled"]) - {"cbm-spike"}
                    or right["default_features"] != left["default_features"]
                ):
                    raise RefreezeError(
                        "API_NEGATIVE_ASSERTION_PAIR_COVERAGE_INVALID"
                    )
                for target_id in target_ids:
                    left_key = (target_id, left_id)
                    right_key = (target_id, right_id)
                    if (left_key in cell_graph_by_pair) != (
                        right_key in cell_graph_by_pair
                    ) or (
                        left_key in cell_graph_by_pair
                        and cell_graph_by_pair[left_key]
                        != cell_graph_by_pair[right_key]
                    ):
                        raise RefreezeError(
                            "API_NEGATIVE_ASSERTION_PAIR_COVERAGE_INVALID"
                        )
            continue
        if kind == "export-prefix-absent":
            prefixes = _sorted_unique_strings(
                assertion["forbidden_prefixes"],
                "API_NEGATIVE_ASSERTION_FIELDS_INVALID",
            )
            if any(
                not canonical_trait_path_re.fullmatch(prefix)
                or not has_canonical_rust_path_components(prefix)
                for prefix in prefixes
            ):
                raise RefreezeError("API_NEGATIVE_ASSERTION_FIELDS_INVALID")
            public_paths = module_paths | export_paths
            if any(
                public_path == prefix or public_path.startswith(f"{prefix}::")
                for prefix in prefixes
                for public_path in public_paths
            ):
                raise RefreezeError("API_NEGATIVE_ASSERTION_CONTRADICTION")
            continue
        trait_field = "trait" if kind == "impl-absent" else "trait_family"
        trait_name = assertion[trait_field]
        if not isinstance(trait_name, str):
            raise RefreezeError("API_NEGATIVE_ASSERTION_FIELDS_INVALID")
        if trait_field == "trait":
            if not canonical_trait_path_re.fullmatch(
                trait_name
            ) or not has_canonical_rust_path_components(trait_name):
                raise RefreezeError("API_TRAIT_PATH_INVALID")
        else:
            trait_family_match = re.fullmatch(
                r"([A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)*)<_>",
                trait_name,
            )
            if trait_family_match is None or not has_canonical_rust_path_components(
                trait_family_match.group(1)
            ):
                raise RefreezeError("API_TRAIT_PATH_INVALID")
        subjects = _sorted_unique_strings(
            assertion["subjects"], "API_NEGATIVE_ASSERTION_SUBJECT_INVALID"
        )
        for subject in subjects:
            subject_path = subject
            generic_arguments: list[str] = []
            generic_match = re.fullmatch(
                r"([^<>]+)<([A-Z](?:,[A-Z])*)>", subject
            )
            if generic_match is not None:
                subject_path = generic_match.group(1)
                generic_arguments = generic_match.group(2).split(",")
            if subject_path not in type_export_paths:
                raise RefreezeError("API_NEGATIVE_ASSERTION_SUBJECT_INVALID")
            subject_item = public_items_by_id[
                next(
                    export["item"]
                    for export in exports
                    if export["path"] == subject_path
                )
            ]
            generic_parameters = subject_item["generics"]
            canonical_generic_arguments = list("TUVWXYZ"[: len(generic_parameters)])
            if (
                len(generic_parameters) > 7
                or generic_arguments != canonical_generic_arguments
                or any(
                    parameter["kind"] != "type" for parameter in generic_parameters
                )
            ):
                raise RefreezeError("API_NEGATIVE_ASSERTION_SUBJECT_INVALID")
            if kind == "impl-absent":
                trait_key = (subject_path, trait_name)
                if trait_key in positive_trait_edges or auto_trait_states.get(
                    trait_key
                ) in {"conditional:T", "positive"}:
                    raise RefreezeError("API_NEGATIVE_ASSERTION_CONTRADICTION")
    if tuple(trait_impl_keys) != EXPECTED_DIRECT_TRAIT_IMPLS:
        raise RefreezeError("API_TRAIT_IMPL_EXPECTATION_INVALID")
    if _canonical_json(auto_traits) != _canonical_json(EXPECTED_AUTO_TRAITS):
        raise RefreezeError("API_AUTO_TRAIT_EXPECTATION_INVALID")
    if _canonical_json(negative_assertions) != _canonical_json(
        EXPECTED_NEGATIVE_ASSERTIONS
    ):
        raise RefreezeError("API_NEGATIVE_ASSERTION_SEMANTICS_INVALID")
    observed = api["observed_graph"]
    _exact(
        observed,
        {
            "cell_graphs": None,
            "graph_set_sha256": None,
            "required_equality": "exact",
            "status": "pre_activation_required",
        },
        "API_OBSERVED_GRAPH_NOT_PREACTIVATION",
    )

    migration = api["migration_v10"]
    categories = _api_nonempty_list(
        migration["categories"], "API_MIGRATION_MAPPING_VACUOUS"
    )
    introduced_v11_atoms = _sorted_unique_strings(
        migration["introduced_v11_atoms"], "API_MIGRATION_MAPPING_VACUOUS"
    )
    baseline = migration["baseline"]
    _exact(baseline["status"], "pre_activation_required", "API_PREACTIVATION_INVALID")
    for field in ("commit", "configuration_domain_sha256", "graph_set_sha256"):
        if baseline[field] is not None:
            raise RefreezeError("API_PREACTIVATION_INVALID")
    atomization = migration["atomization"]
    _exact(
        atomization["status"], "pre_activation_required", "API_PREACTIVATION_INVALID"
    )
    if atomization["observed_atom_set_sha256"] is not None:
        raise RefreezeError("API_PREACTIVATION_INVALID")
    category_ids: list[str] = []
    old_atoms: list[str] = []
    kept_v11_atoms: list[str] = []
    v11_atom_universe = module_paths | export_paths | associated_atom_paths
    for category in categories:
        category_ids.append(category["id"])
        category_atoms = _sorted_unique_strings(
            category["atoms"], "API_MIGRATION_ATOM_PARTITION_INVALID"
        )
        old_atoms.extend(category_atoms)
        decision = category["decision"]
        has_v11_atoms = "v11_atoms" in category
        if decision not in {"keep", "remove", "replace"} or (
            has_v11_atoms != (decision in {"keep", "replace"})
        ):
            raise RefreezeError("API_MIGRATION_DECISION_INVALID")
        if has_v11_atoms:
            category_v11_atoms = _sorted_unique_strings(
                category["v11_atoms"], "API_MIGRATION_V11_ATOM_INVALID"
            )
            if any(atom not in v11_atom_universe for atom in category_v11_atoms):
                raise RefreezeError("API_MIGRATION_V11_ATOM_INVALID")
            if decision == "keep":
                if category_v11_atoms != category_atoms:
                    raise RefreezeError("API_MIGRATION_KEEP_IDENTITY_INVALID")
                kept_v11_atoms.extend(category_v11_atoms)
    if (
        category_ids != sorted(category_ids)
        or len(set(category_ids)) != len(category_ids)
        or len(set(old_atoms)) != len(old_atoms)
    ):
        raise RefreezeError("API_MIGRATION_ATOM_PARTITION_INVALID")
    expected_introduced_v11_atoms = (
        v11_atom_universe
        - set(kept_v11_atoms)
    )
    if set(introduced_v11_atoms) != expected_introduced_v11_atoms:
        raise RefreezeError("API_MIGRATION_INTRODUCED_ATOMS_INVALID")
    root_categories = [item for item in categories if item["id"] == "v10-00-crate-root"]
    if len(root_categories) != 1:
        raise RefreezeError("API_V10_ROOT_MAPPING_INVALID")
    root_category = root_categories[0]
    if (
        root_category["atoms"] != ["symforge"]
        or root_category["decision"] != "keep"
        or root_category["covers_descendants"] is not False
        or root_category["v11_atoms"] != ["symforge"]
        or not root_category["rationale"]
    ):
        raise RefreezeError("API_V10_ROOT_MAPPING_INVALID")


def _validate_inventory(
    git: GitObjects,
    commit: str,
    value: object,
    *,
    amendment_ids: set[str],
) -> None:
    if not isinstance(value, list) or not value:
        raise RefreezeError("INVENTORY_INVALID")
    expected_paths = git.inventory_paths(commit, FEATURE_ROOT, CONTEXT_PATH)
    entries: dict[str, dict[str, object]] = {}
    listed_paths: list[str] = []
    classifications = {"normative", "supporting_evidence", "historical", "superseded"}
    for raw_entry in value:
        entry = _closed_object(raw_entry, INVENTORY_FIELDS, "INVENTORY_ENTRY_SHAPE")
        path = _validate_repo_path(entry["path"])
        if path in entries:
            raise RefreezeError("INVENTORY_PATH_DUPLICATE")
        listed_paths.append(path)
        expected_scope = "bound" if path == CONTEXT_PATH else "feature"
        _exact(entry["scope"], expected_scope, "INVENTORY_SCOPE_INVALID")
        if path != CONTEXT_PATH and not path.startswith(f"{FEATURE_ROOT}/"):
            raise RefreezeError("INVENTORY_SCOPE_ESCAPE")
        if (
            not isinstance(entry["classification"], str)
            or entry["classification"] not in classifications
        ):
            raise RefreezeError("INVENTORY_CLASSIFICATION_INVALID")
        superseded_by = _sorted_unique_strings(
            entry["superseded_by"], "INVENTORY_SUPERSESSION_INVALID", nonempty=False
        )
        if any(item not in amendment_ids for item in superseded_by):
            raise RefreezeError("INVENTORY_SUPERSESSION_UNKNOWN")
        if entry["classification"] == "superseded" and not superseded_by:
            raise RefreezeError("INVENTORY_SUPERSESSION_MISSING")
        if path == MANIFEST_PATH:
            _exact(entry["hash_policy"], "self_excluded", "INVENTORY_SELF_POLICY")
            if entry["sha256"] is not None:
                raise RefreezeError("INVENTORY_SELF_HASH_PRESENT")
        else:
            _exact(entry["hash_policy"], "raw_bytes", "INVENTORY_HASH_POLICY")
            digest = _sha256_value(entry["sha256"], "INVENTORY_DIGEST_INVALID")
            if _sha256(git.read_blob(commit, path)) != digest:
                raise RefreezeError("INVENTORY_HASH_MISMATCH")
        entries[path] = entry
    if listed_paths != sorted(listed_paths):
        raise RefreezeError("INVENTORY_ORDER_INVALID")
    if set(entries) != expected_paths:
        raise RefreezeError("INVENTORY_SET_MISMATCH")
    if MANIFEST_PATH not in entries or CONTEXT_PATH not in entries:
        raise RefreezeError("INVENTORY_REQUIRED_ENTRY_MISSING")


def _clause_bytes(blob: bytes, start_line: int, end_line: int) -> bytes:
    lines = blob.splitlines(keepends=True)
    if start_line < 1 or end_line < start_line or end_line > len(lines):
        raise RefreezeError("CLAUSE_RANGE_INVALID")
    return b"".join(lines[start_line - 1 : end_line])


def _validate_clause(
    git: GitObjects,
    value: object,
    *,
    baseline_commit: str,
    target_commit: str,
    expected_source: str,
    seen_clause_ids: set[str],
    seen_clause_locations: set[tuple[str, str, int, int]],
    clause_intervals: dict[tuple[str, str], list[tuple[int, int]]],
) -> dict[str, object]:
    clause = _closed_object(value, CLAUSE_FIELDS, "CLAUSE_SHAPE_INVALID")
    clause_id = _string(clause["clause_id"], "CLAUSE_ID_INVALID")
    if clause_id in seen_clause_ids:
        raise RefreezeError("CLAUSE_ID_DUPLICATE")
    seen_clause_ids.add(clause_id)
    _exact(clause["source"], expected_source, "CLAUSE_SOURCE_INVALID")
    path = _validate_repo_path(clause["path"])
    if path not in CLAUSE_PATHS:
        raise RefreezeError("CLAUSE_PATH_INVALID")
    start_line = _integer(clause["start_line"], "CLAUSE_START_INVALID", minimum=1)
    end_line = _integer(clause["end_line"], "CLAUSE_END_INVALID", minimum=1)
    location = (expected_source, path, start_line, end_line)
    if location in seen_clause_locations:
        raise RefreezeError("CLAUSE_LOCATION_DUPLICATE")
    seen_clause_locations.add(location)
    clause_intervals.setdefault((expected_source, path), []).append(
        (start_line, end_line)
    )
    digest = _sha256_value(clause["sha256"], "CLAUSE_DIGEST_INVALID")
    commit = baseline_commit if expected_source == "baseline" else target_commit
    selected = _clause_bytes(git.read_blob(commit, path), start_line, end_line)
    if _sha256(selected) != digest:
        raise RefreezeError("CLAUSE_HASH_MISMATCH")
    return clause


def _utf8_markdown(value: bytes, code: str) -> str:
    try:
        return value.decode("utf-8")
    except UnicodeDecodeError as error:
        raise RefreezeError(code) from error


def _definition_count(documents: Sequence[str], token: str) -> int:
    pattern = re.compile(r"\*\*" + re.escape(token) + r"(?=\*\*|[\s—:])")
    return sum(len(pattern.findall(document)) for document in documents)


def _token_present(documents: Sequence[str], token: str) -> bool:
    pattern = re.compile(
        r"(?<![A-Za-z0-9_-])" + re.escape(token) + r"(?![A-Za-z0-9_-])"
    )
    return any(pattern.search(document) is not None for document in documents)


def _github_heading_slug(heading: str) -> str:
    slug: list[str] = []
    for character in heading.strip().lower():
        category = unicodedata.category(character)
        if character in {"-", "_"} or character.isalnum():
            slug.append(character)
        elif character.isspace():
            slug.append("-")
        elif category.startswith(("P", "S")):
            continue
    return "".join(slug)


def _markdown_heading_slugs(document: str) -> list[str]:
    slugs: list[str] = []
    fence_character: str | None = None
    fence_length = 0
    for line in document.splitlines():
        fence = re.match(r"^[ \t]{0,3}(`{3,}|~{3,})", line)
        if fence is not None:
            marker = fence.group(1)
            if fence_character is None:
                fence_character = marker[0]
                fence_length = len(marker)
            elif marker[0] == fence_character and len(marker) >= fence_length:
                fence_character = None
                fence_length = 0
            continue
        if fence_character is not None:
            continue
        heading = re.match(r"^[ \t]{0,3}#{1,6}[ \t]+(.+?)[ \t]*$", line)
        if heading is None:
            continue
        text = re.sub(r"[ \t]+#+[ \t]*$", "", heading.group(1))
        slug = _github_heading_slug(text)
        if slug:
            slugs.append(slug)
    return slugs


def _validate_mapping_references(
    git: GitObjects, target_commit: str, amendments: Sequence[dict[str, object]]
) -> None:
    requirement_documents = [
        _utf8_markdown(
            git.read_blob(target_commit, path), "REQUIREMENT_DOCUMENT_UTF8_REQUIRED"
        )
        for path in (f"{FEATURE_ROOT}/spec.md", f"{FEATURE_ROOT}/GOAL.md")
    ]
    task_document = _utf8_markdown(
        git.read_blob(target_commit, f"{FEATURE_ROOT}/tasks.md"),
        "TASK_DOCUMENT_UTF8_REQUIRED",
    )
    task_definitions = re.findall(
        r"(?m)^[ \t]*-[ \t]*\[[ xX]\][ \t]+(T[0-9]{3})(?![0-9])",
        task_document,
    )
    regression_documents = [
        _utf8_markdown(
            git.read_blob(target_commit, path), "REGRESSION_DOCUMENT_UTF8_REQUIRED"
        )
        for path in (
            f"{FEATURE_ROOT}/contracts/lifecycle-acceptance-oracles-v11.md",
        )
    ]
    heading_cache: dict[str, list[str]] = {}

    for amendment in amendments:
        for requirement_id in amendment["requirement_ids"]:
            if (
                REQUIREMENT_ID_RE.fullmatch(requirement_id) is None
                or _definition_count(requirement_documents, requirement_id) != 1
            ):
                raise RefreezeError("AMENDMENT_REQUIREMENT_UNRESOLVED")
        for contract_id in amendment["contract_clause_ids"]:
            if contract_id.count("#") != 1:
                raise RefreezeError("AMENDMENT_CONTRACT_REFERENCE_INVALID")
            relative_path, slug = contract_id.split("#", 1)
            if not relative_path.startswith("contracts/") or not relative_path.endswith(
                ".md"
            ):
                raise RefreezeError("AMENDMENT_CONTRACT_REFERENCE_INVALID")
            path = _validate_repo_path(f"{FEATURE_ROOT}/{relative_path}")
            if not slug or _github_heading_slug(slug) != slug:
                raise RefreezeError("AMENDMENT_CONTRACT_REFERENCE_INVALID")
            headings = heading_cache.get(path)
            if headings is None:
                headings = _markdown_heading_slugs(
                    _utf8_markdown(
                        git.read_blob(target_commit, path),
                        "CONTRACT_DOCUMENT_UTF8_REQUIRED",
                    )
                )
                heading_cache[path] = headings
            if headings.count(slug) != 1:
                raise RefreezeError("AMENDMENT_CONTRACT_REFERENCE_UNRESOLVED")
        for task_id in amendment["plan_task_ids"]:
            if PLAN_TASK_ID_RE.fullmatch(task_id) is None or task_definitions.count(task_id) != 1:
                raise RefreezeError("AMENDMENT_TASK_UNRESOLVED")
        for regression_id in amendment["regression_ids"]:
            if (
                REGRESSION_ID_RE.fullmatch(regression_id) is None
                or not _token_present(regression_documents, regression_id)
            ):
                raise RefreezeError("AMENDMENT_REGRESSION_UNRESOLVED")


def _validate_amendments(
    git: GitObjects,
    value: object,
    *,
    baseline_commit: str,
    target_commit: str,
) -> tuple[set[str], str]:
    if not isinstance(value, list):
        raise RefreezeError("AMENDMENTS_INVALID")
    expected_ids = [f"F020-V11-A{index:02}" for index in range(1, 20)]
    if len(value) != len(expected_ids):
        raise RefreezeError("AMENDMENT_COUNT_INVALID")
    records: list[dict[str, object]] = []
    seen_clause_ids: set[str] = set()
    seen_clause_locations: set[tuple[str, str, int, int]] = set()
    clause_intervals: dict[tuple[str, str], list[tuple[int, int]]] = {}
    actual_ids: list[str] = []
    for raw_amendment in value:
        amendment = _closed_object(raw_amendment, AMENDMENT_FIELDS, "AMENDMENT_SHAPE")
        amendment_id = _string(amendment["amendment_id"], "AMENDMENT_ID_INVALID")
        if AMENDMENT_ID_RE.fullmatch(amendment_id) is None:
            raise RefreezeError("AMENDMENT_ID_INVALID")
        actual_ids.append(amendment_id)
        replaced_raw = amendment["replaced"]
        replacements_raw = amendment["replacements"]
        if not isinstance(replaced_raw, list) or not replaced_raw:
            raise RefreezeError("AMENDMENT_REPLACED_EMPTY")
        if not isinstance(replacements_raw, list) or not replacements_raw:
            raise RefreezeError("AMENDMENT_REPLACEMENTS_EMPTY")
        replaced = [
            _validate_clause(
                git,
                clause,
                baseline_commit=baseline_commit,
                target_commit=target_commit,
                expected_source="baseline",
                seen_clause_ids=seen_clause_ids,
                seen_clause_locations=seen_clause_locations,
                clause_intervals=clause_intervals,
            )
            for clause in replaced_raw
        ]
        for clause in replaced:
            replaced_bytes = _clause_bytes(
                git.read_blob(baseline_commit, clause["path"]),
                clause["start_line"],
                clause["end_line"],
            )
            if replaced_bytes in git.read_blob(target_commit, clause["path"]):
                raise RefreezeError("AMENDMENT_REPLACED_CLAUSE_STILL_PRESENT")
        replacements = [
            _validate_clause(
                git,
                clause,
                baseline_commit=baseline_commit,
                target_commit=target_commit,
                expected_source="target",
                seen_clause_ids=seen_clause_ids,
                seen_clause_locations=seen_clause_locations,
                clause_intervals=clause_intervals,
            )
            for clause in replacements_raw
        ]
        if replaced != sorted(replaced, key=_canonical_json):
            raise RefreezeError("AMENDMENT_REPLACED_ORDER_INVALID")
        if replacements != sorted(replacements, key=_canonical_json):
            raise RefreezeError("AMENDMENT_REPLACEMENTS_ORDER_INVALID")
        for field in (
            "requirement_ids",
            "contract_clause_ids",
            "plan_task_ids",
            "regression_ids",
        ):
            _sorted_unique_strings(amendment[field], f"AMENDMENT_{field.upper()}_INVALID")
        expected_mapping = EXPECTED_AMENDMENT_MAPPINGS[amendment_id]
        if any(
            tuple(amendment[field]) != expected_mapping[field]
            for field in (
                "requirement_ids",
                "contract_clause_ids",
                "plan_task_ids",
                "regression_ids",
            )
        ):
            raise RefreezeError("AMENDMENT_SEMANTIC_ASSOCIATION_INVALID")
        if amendment_id not in amendment["requirement_ids"]:
            raise RefreezeError("AMENDMENT_SEMANTIC_ASSOCIATION_INVALID")
        replacement_documents: list[str] = []
        for clause in replacements:
            replacement_bytes = _clause_bytes(
                git.read_blob(target_commit, clause["path"]),
                clause["start_line"],
                clause["end_line"],
            )
            try:
                replacement_documents.append(replacement_bytes.decode("utf-8"))
            except UnicodeDecodeError as error:
                raise RefreezeError(
                    "AMENDMENT_SEMANTIC_ASSOCIATION_INVALID"
                ) from error
        if any(
            not _token_present(replacement_documents, token)
            for token in [amendment_id, *amendment["regression_ids"]]
        ):
            raise RefreezeError("AMENDMENT_SEMANTIC_ASSOCIATION_INVALID")
        records.append(
            {
                "amendment_id": amendment_id,
                "contract_clause_ids": amendment["contract_clause_ids"],
                "plan_task_ids": amendment["plan_task_ids"],
                "regression_ids": amendment["regression_ids"],
                "replaced": replaced,
                "replacements": replacements,
                "requirement_ids": amendment["requirement_ids"],
            }
        )
    if actual_ids != expected_ids:
        raise RefreezeError("AMENDMENT_ID_SET_INVALID")
    for intervals in clause_intervals.values():
        maximum_end = 0
        for start_line, end_line in sorted(intervals):
            if start_line <= maximum_end:
                raise RefreezeError("CLAUSE_RANGE_OVERLAP")
            maximum_end = max(maximum_end, end_line)
    _validate_mapping_references(git, target_commit, value)
    records.sort(key=lambda record: record["amendment_id"])
    digest = _sha256(AMENDMENT_DOMAIN + _canonical_json(records))
    return set(actual_ids), digest


def _validate_required_paths(value: object, inventory: object) -> None:
    paths = _sorted_unique_strings(value, "REQUIRED_PATHS_INVALID")
    if set(paths) != REQUIRED_NORMATIVE_PATHS:
        raise RefreezeError("REQUIRED_PATHS_SET_INVALID")
    if not isinstance(inventory, list):
        raise RefreezeError("INVENTORY_INVALID")
    by_path = {
        item.get("path"): item
        for item in inventory
        if isinstance(item, dict) and isinstance(item.get("path"), str)
    }
    for path in paths:
        item = by_path.get(path)
        if not isinstance(item, dict) or item.get("classification") != "normative":
            raise RefreezeError("REQUIRED_PATH_NOT_NORMATIVE")


def _validate_attestation(
    git: GitObjects,
    target_commit: str,
    manifest: dict[str, object],
    manifest_bytes: bytes,
) -> tuple[str, str]:
    attestation_path = _validate_repo_path(manifest["detached_attestation_path"])
    if attestation_path != ATTESTATION_PATH:
        raise RefreezeError("ATTESTATION_PATH_INVALID")
    raw = git.read_blob(target_commit, attestation_path)
    attestation = _closed_object(
        _parse_sentinel_json(raw, ATTESTATION_START, ATTESTATION_END),
        ATTESTATION_FIELDS,
        "ATTESTATION_SHAPE_INVALID",
    )
    _exact(
        attestation["kind"],
        "symforge-feature-020-refreeze-attestation",
        "ATTESTATION_KIND_INVALID",
    )
    _exact(attestation["schema_version"], 1, "ATTESTATION_SCHEMA_INVALID")
    manifest_pin = _closed_object(
        attestation["manifest"], frozenset({"path", "sha256"}), "ATTESTATION_MANIFEST"
    )
    _exact(manifest_pin["path"], MANIFEST_PATH, "ATTESTATION_MANIFEST_PATH")
    _exact(
        manifest_pin["sha256"],
        _sha256(manifest_bytes),
        "ATTESTATION_MANIFEST_HASH",
    )
    for field in ("baseline", "design", "context", "public_api", "amendment_set_id"):
        if attestation[field] != manifest[field]:
            raise RefreezeError("ATTESTATION_BINDING_MISMATCH")
    external = _closed_object(
        attestation["external_approval"],
        frozenset({"required", "purpose", "signature_namespace"}),
        "ATTESTATION_EXTERNAL_APPROVAL",
    )
    _exact(external["required"], True, "ATTESTATION_APPROVAL_REQUIRED")
    _exact(external["purpose"], APPROVAL_PURPOSE, "ATTESTATION_PURPOSE")
    _exact(
        external["signature_namespace"],
        SIGNATURE_NAMESPACE,
        "ATTESTATION_NAMESPACE",
    )
    return attestation_path, _sha256(raw)


def _release_evidence_phase(git: GitObjects, commit: str) -> str:
    requirements = _closed_object(
        _parse_json_bytes(
            git.read_blob(commit, RELEASE_EVIDENCE_REQUIREMENTS_PATH)
        ),
        frozenset(
            {
                "kind",
                "phase",
                "required_oracle_receipts",
                "required_review_documents",
                "required_task_receipts",
                "schema_version",
            }
        ),
        "RELEASE_EVIDENCE_REQUIREMENTS_INVALID",
    )
    _exact(
        requirements["kind"],
        "symforge.lifecycle_release_evidence_requirements.v11",
        "RELEASE_EVIDENCE_REQUIREMENTS_INVALID",
    )
    _exact(
        requirements["schema_version"],
        1,
        "RELEASE_EVIDENCE_REQUIREMENTS_INVALID",
    )
    phase = requirements["phase"]
    if phase not in {"active", "pre_activation"}:
        raise RefreezeError("RELEASE_EVIDENCE_REQUIREMENTS_INVALID")
    requirement_fields = (
        "required_oracle_receipts",
        "required_review_documents",
        "required_task_receipts",
    )
    for field in requirement_fields:
        _sorted_unique_strings(
            requirements[field],
            "RELEASE_EVIDENCE_REQUIREMENTS_INVALID",
            nonempty=False,
        )
    if phase == "pre_activation" and any(
        requirements[field] for field in requirement_fields
    ):
        raise RefreezeError("RELEASE_EVIDENCE_REQUIREMENTS_INVALID")
    return phase


def _validate_release_evidence_phase_history(
    git: GitObjects,
    target_commit: str,
) -> None:
    if _release_evidence_phase(git, target_commit) != "pre_activation":
        return
    for ancestor in git.path_history(
        target_commit,
        RELEASE_EVIDENCE_REQUIREMENTS_PATH,
    ):
        if ancestor == target_commit or not git.blob_exists(
            ancestor,
            RELEASE_EVIDENCE_REQUIREMENTS_PATH,
        ):
            continue
        if _release_evidence_phase(git, ancestor) == "active":
            raise RefreezeError("RELEASE_EVIDENCE_PHASE_REGRESSION")


def verify_internal(
    root: Path,
    target_ref: str,
    *,
    git_executable: str | Path | None = None,
) -> InternalVerification:
    git = GitObjects(root, git_executable=git_executable)
    target_commit = git.resolve_commit(target_ref)
    target_tree = git.resolve_tree(target_commit)
    _validate_release_evidence_phase_history(git, target_commit)
    manifest_bytes = git.read_blob(target_commit, MANIFEST_PATH)
    manifest = _closed_object(
        _parse_sentinel_json(manifest_bytes, MANIFEST_START, MANIFEST_END),
        MANIFEST_FIELDS,
        "MANIFEST_SHAPE_INVALID",
    )
    _exact(manifest["kind"], "symforge-feature-020-refreeze", "MANIFEST_KIND_INVALID")
    _exact(manifest["schema_version"], 1, "MANIFEST_SCHEMA_INVALID")
    _exact(manifest["feature_root"], FEATURE_ROOT, "MANIFEST_FEATURE_ROOT_INVALID")
    _exact(manifest["self_path"], MANIFEST_PATH, "MANIFEST_SELF_PATH_INVALID")
    baseline = _validate_baseline(git, manifest["baseline"], target_commit=target_commit)
    amendment_ids, amendment_digest = _validate_amendments(
        git,
        manifest["amendments"],
        baseline_commit=baseline["commit"],
        target_commit=target_commit,
    )
    _exact(
        manifest["amendment_set_id"],
        amendment_digest,
        "AMENDMENT_SET_DIGEST_MISMATCH",
    )
    _validate_inventory(
        git,
        target_commit,
        manifest["inventory"],
        amendment_ids=amendment_ids,
    )
    _validate_required_paths(manifest["required_normative_paths"], manifest["inventory"])
    _validate_path_hash(
        git,
        target_commit,
        manifest["design"],
        expected_path=DESIGN_PATH,
        code="DESIGN",
    )
    _validate_path_hash(
        git,
        target_commit,
        manifest["context"],
        expected_path=CONTEXT_PATH,
        code="CONTEXT",
    )
    _validate_api(git, target_commit, manifest["public_api"])
    attestation_path, attestation_digest = _validate_attestation(
        git, target_commit, manifest, manifest_bytes
    )
    return InternalVerification(
        target_commit=target_commit,
        target_tree=target_tree,
        attestation_path=attestation_path,
        attestation_sha256=attestation_digest,
    )


def _outside_repository(root: Path, path: Path, code: str) -> Path:
    try:
        resolved = path.resolve(strict=True)
    except OSError as error:
        raise RefreezeError(code) from error
    if not resolved.is_file():
        raise RefreezeError(code)
    try:
        resolved.relative_to(root.resolve())
    except ValueError:
        return resolved
    raise RefreezeError("TRUST_FILE_INSIDE_REPOSITORY")


def _outside_repository_directory(root: Path, path: Path) -> Path:
    try:
        resolved = path.resolve(strict=True)
    except OSError as error:
        raise RefreezeError("APPROVAL_HISTORY_DIRECTORY_INVALID") from error
    if not resolved.is_dir():
        raise RefreezeError("APPROVAL_HISTORY_DIRECTORY_INVALID")
    try:
        resolved.relative_to(root.resolve())
    except ValueError:
        return resolved
    raise RefreezeError("TRUST_FILE_INSIDE_REPOSITORY")


def _history_file(
    root: Path, history_directory: Path, digest: str, suffix: str
) -> Path:
    _sha256_value(digest, "APPROVAL_PREDECESSOR_INVALID")
    candidate = history_directory / f"{digest}{suffix}"
    try:
        resolved = candidate.resolve(strict=True)
        resolved.relative_to(history_directory)
    except (OSError, ValueError) as error:
        raise RefreezeError("APPROVAL_HISTORY_FILE_INVALID") from error
    return _outside_repository(root, resolved, "APPROVAL_HISTORY_FILE_INVALID")


def _validate_approval_record(
    value: dict[str, object], *, expected_release_identity: str, expected_repository: str
) -> None:
    _closed_object(value, APPROVAL_FIELDS, "APPROVAL_SHAPE_INVALID")
    _exact(value["kind"], "symforge-feature-020-refreeze-approval", "APPROVAL_KIND_INVALID")
    _exact(value["schema_version"], 1, "APPROVAL_SCHEMA_INVALID")
    _exact(expected_repository, CANONICAL_REPOSITORY, "APPROVAL_REPOSITORY_INVALID")
    _exact(value["repository"], expected_repository, "APPROVAL_REPOSITORY_INVALID")
    _exact(value["purpose"], APPROVAL_PURPOSE, "APPROVAL_PURPOSE_INVALID")
    _git_oid(value["target_commit"], "APPROVAL_TARGET_COMMIT_INVALID")
    _git_oid(value["target_tree"], "APPROVAL_TARGET_TREE_INVALID")
    attestation = _closed_object(
        value["attestation"], frozenset({"path", "sha256"}), "APPROVAL_ATTESTATION_INVALID"
    )
    _validate_repo_path(attestation["path"])
    _sha256_value(attestation["sha256"], "APPROVAL_ATTESTATION_DIGEST_INVALID")
    _release_identity(value["release_identity"])
    if value["release_identity"] != expected_release_identity:
        raise RefreezeError("APPROVAL_IDENTITY_MISMATCH")
    approved_at = _string(value["approved_at"], "APPROVAL_TIME_INVALID")
    try:
        parsed_time = datetime.fromisoformat(approved_at.replace("Z", "+00:00"))
    except ValueError as error:
        raise RefreezeError("APPROVAL_TIME_INVALID") from error
    if parsed_time.tzinfo is None:
        raise RefreezeError("APPROVAL_TIME_INVALID")
    sequence = _integer(value["sequence"], "APPROVAL_SEQUENCE_INVALID", minimum=1)
    _string(value["store_locator"], "APPROVAL_STORE_LOCATOR_INVALID")
    _integer(value["store_version"], "APPROVAL_STORE_VERSION_INVALID", minimum=1)
    predecessor = value["predecessor_digest"]
    if sequence == 1:
        if predecessor is not None:
            raise RefreezeError("APPROVAL_PREDECESSOR_INVALID")
    else:
        _sha256_value(predecessor, "APPROVAL_PREDECESSOR_INVALID")
    _exact(
        value["signature_namespace"],
        SIGNATURE_NAMESPACE,
        "APPROVAL_NAMESPACE_INVALID",
    )


def verify_sshsig(
    approval_bytes: bytes,
    *,
    ssh_keygen_executable: Path,
    signature: Path,
    allowed_signers: Path,
    release_identity: str,
) -> bool:
    try:
        result = subprocess.run(
            [
                str(ssh_keygen_executable),
                "-Y",
                "verify",
                "-f",
                str(allowed_signers),
                "-I",
                release_identity,
                "-n",
                SIGNATURE_NAMESPACE,
                "-s",
                str(signature),
            ],
            input=approval_bytes,
            check=False,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            shell=False,
            timeout=15,
        )
    except (OSError, subprocess.TimeoutExpired):
        return False
    return result.returncode == 0


SignatureVerifier = Callable[..., bool]


def _verify_approval_history(
    root: Path,
    *,
    current: dict[str, object],
    current_bytes: bytes,
    history_directory: Path | None,
    allowed_signers: Path,
    ssh_keygen_executable: Path,
    expected_release_identity: str,
    expected_repository: str,
    signature_verifier: SignatureVerifier,
) -> None:
    sequence = current["sequence"]
    if sequence > MAX_APPROVAL_CHAIN_LENGTH:
        raise RefreezeError("APPROVAL_HISTORY_BOUND_EXCEEDED")
    if sequence == 1:
        return
    if history_directory is None:
        raise RefreezeError("APPROVAL_HISTORY_REQUIRED")

    continuity = (
        current["repository"],
        current["purpose"],
        current["signature_namespace"],
        current["release_identity"],
        current["store_locator"],
        current["store_version"],
    )
    seen = {_sha256(current_bytes)}
    cursor = current
    while cursor["sequence"] > 1:
        predecessor_digest = cursor["predecessor_digest"]
        if predecessor_digest in seen:
            raise RefreezeError("APPROVAL_HISTORY_CYCLE")
        record_path = _history_file(
            root, history_directory, predecessor_digest, ".json"
        )
        signature_path = _history_file(
            root, history_directory, predecessor_digest, ".json.sig"
        )
        try:
            predecessor_bytes = record_path.read_bytes()
        except OSError as error:
            raise RefreezeError("APPROVAL_HISTORY_FILE_INVALID") from error
        if _sha256(predecessor_bytes) != predecessor_digest:
            raise RefreezeError("APPROVAL_HISTORY_DIGEST_MISMATCH")
        predecessor = _parse_json_bytes(predecessor_bytes)
        _validate_approval_record(
            predecessor,
            expected_release_identity=expected_release_identity,
            expected_repository=expected_repository,
        )
        if predecessor_bytes != _canonical_json(predecessor):
            raise RefreezeError("APPROVAL_HISTORY_NOT_CANONICAL")
        if predecessor["sequence"] != cursor["sequence"] - 1:
            raise RefreezeError("APPROVAL_HISTORY_SEQUENCE_MISMATCH")
        predecessor_continuity = (
            predecessor["repository"],
            predecessor["purpose"],
            predecessor["signature_namespace"],
            predecessor["release_identity"],
            predecessor["store_locator"],
            predecessor["store_version"],
        )
        if predecessor_continuity != continuity:
            raise RefreezeError("APPROVAL_HISTORY_CONTINUITY_MISMATCH")
        if not signature_verifier(
            predecessor_bytes,
            signature=signature_path,
            allowed_signers=allowed_signers,
            ssh_keygen_executable=ssh_keygen_executable,
            release_identity=expected_release_identity,
        ):
            raise RefreezeError("APPROVAL_HISTORY_SIGNATURE_INVALID")
        seen.add(predecessor_digest)
        cursor = predecessor


def verify_approval(
    root: Path,
    *,
    target_ref: str,
    approval_record: Path,
    approval_signature: Path,
    approval_history_dir: Path | None = None,
    allowed_signers: Path,
    git_executable: str | Path,
    ssh_keygen_executable: str | Path,
    expected_release_identity: str,
    expected_repository: str,
    signature_verifier: SignatureVerifier = verify_sshsig,
) -> InternalVerification:
    root = root.resolve()
    trusted_git = _trusted_executable_path(git_executable, root=root)
    trusted_ssh_keygen = _trusted_executable_path(
        ssh_keygen_executable, root=root
    )
    _release_identity(expected_release_identity)
    approval_path = _outside_repository(root, approval_record, "APPROVAL_FILE_INVALID")
    signature_path = _outside_repository(
        root, approval_signature, "SIGNATURE_FILE_INVALID"
    )
    allowed_signers_path = _outside_repository(
        root, allowed_signers, "ALLOWED_SIGNERS_FILE_INVALID"
    )
    history_directory = (
        _outside_repository_directory(root, approval_history_dir)
        if approval_history_dir is not None
        else None
    )
    try:
        approval_bytes = approval_path.read_bytes()
    except OSError as error:
        raise RefreezeError("APPROVAL_FILE_INVALID") from error
    approval = _parse_json_bytes(approval_bytes)
    _validate_approval_record(
        approval,
        expected_release_identity=expected_release_identity,
        expected_repository=expected_repository,
    )
    if approval_bytes != _canonical_json(approval):
        raise RefreezeError("APPROVAL_NOT_CANONICAL")
    expected_target = GitObjects(
        root, git_executable=trusted_git
    ).resolve_commit(target_ref)
    if approval["target_commit"] != expected_target:
        raise RefreezeError("APPROVAL_COMMIT_MISMATCH")
    internal = verify_internal(
        root, expected_target, git_executable=trusted_git
    )
    if internal.target_commit != approval["target_commit"]:
        raise RefreezeError("APPROVAL_COMMIT_MISMATCH")
    if internal.target_tree != approval["target_tree"]:
        raise RefreezeError("APPROVAL_TREE_MISMATCH")
    attestation = approval["attestation"]
    if attestation["path"] != internal.attestation_path:
        raise RefreezeError("APPROVAL_ATTESTATION_PATH_MISMATCH")
    if attestation["sha256"] != internal.attestation_sha256:
        raise RefreezeError("APPROVAL_ATTESTATION_HASH_MISMATCH")
    if approval["sequence"] > 1 and history_directory is None:
        raise RefreezeError("APPROVAL_HISTORY_REQUIRED")
    if not signature_verifier(
        approval_bytes,
        signature=signature_path,
        allowed_signers=allowed_signers_path,
        ssh_keygen_executable=trusted_ssh_keygen,
        release_identity=expected_release_identity,
    ):
        raise RefreezeError("APPROVAL_SIGNATURE_INVALID")
    _verify_approval_history(
        root,
        current=approval,
        current_bytes=approval_bytes,
        history_directory=history_directory,
        allowed_signers=allowed_signers_path,
        ssh_keygen_executable=trusted_ssh_keygen,
        expected_release_identity=expected_release_identity,
        expected_repository=expected_repository,
        signature_verifier=signature_verifier,
    )
    return internal


def repo_root(path: str | None = None) -> Path:
    if path is not None:
        return Path(path).resolve()
    return Path(__file__).resolve().parent.parent


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Verify the Feature 020 V11 refreeze gate.")
    parser.add_argument(
        "--root",
        default=None,
        help="Repository root. Defaults to the repository containing this script.",
    )
    parser.add_argument(
        "--git-executable",
        default=None,
        help=(
            "Absolute canonical Git executable outside the repository. "
            "Release verification must pin this explicitly."
        ),
    )
    subparsers = parser.add_subparsers(dest="command", required=True)
    internal = subparsers.add_parser(
        "verify-internal", help="Verify the committed in-repository refreeze evidence."
    )
    internal.add_argument(
        "--target-ref",
        required=True,
        help="Exact committed target to verify (CI uses HEAD).",
    )
    approval = subparsers.add_parser(
        "verify-approval", help="Verify internal evidence and an external signed approval."
    )
    approval.add_argument(
        "--target-ref",
        required=True,
        help="Exact activation target independently selected by CI (normally HEAD).",
    )
    approval.add_argument("--approval-record", required=True)
    approval.add_argument("--approval-signature", required=True)
    approval.add_argument("--approval-history-dir", default=None)
    approval.add_argument("--allowed-signers", required=True)
    approval.add_argument("--ssh-keygen-executable", required=True)
    approval.add_argument("--expected-release-identity", required=True)
    approval.add_argument("--expected-repository", required=True)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    root = repo_root(args.root)
    try:
        if args.command == "verify-internal":
            verify_internal(
                root, args.target_ref, git_executable=args.git_executable
            )
            print("Feature 020 V11 internal refreeze verification passed.")
            return 0
        if args.command == "verify-approval":
            if args.git_executable is None:
                raise RefreezeError("EXECUTABLE_PROVENANCE_INVALID")
            verify_approval(
                root,
                target_ref=args.target_ref,
                approval_record=Path(args.approval_record),
                approval_signature=Path(args.approval_signature),
                approval_history_dir=(
                    Path(args.approval_history_dir)
                    if args.approval_history_dir is not None
                    else None
                ),
                allowed_signers=Path(args.allowed_signers),
                git_executable=args.git_executable,
                ssh_keygen_executable=args.ssh_keygen_executable,
                expected_release_identity=args.expected_release_identity,
                expected_repository=args.expected_repository,
            )
            print("Feature 020 V11 external approval verification passed.")
            return 0
    except RefreezeError as error:
        print(f"Feature 020 V11 verification failed: {error.code}", file=sys.stderr)
        return 1
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
