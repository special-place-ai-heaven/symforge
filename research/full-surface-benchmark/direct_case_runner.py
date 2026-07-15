# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "tiktoken>=0.7.0,<1",
# ]
# ///
"""Deterministic direct-RPC scenario runner for the SymForge surface benchmark.

The runner is deliberately local-only: it materializes disposable clones from
the frozen corpus mirrors, speaks MCP over stdio through :mod:`mcp_harness`,
and sanitizes every persisted record.  It never invokes a paid model API.

Formal runs should pass ``--require-asset-lock`` once ``assets.lock.json`` has
been frozen.  ``validate`` and ``self-test`` remain useful before that freeze;
they still verify all internally pinned corpus, fixture, tokenizer, and SUT
hashes and report the computed cases/campaign hashes.
"""

from __future__ import annotations

import argparse
import difflib
import fnmatch
import hashlib
import json
import math
import os
import pathlib
import platform
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass, field
from typing import Any, Iterable, Sequence

import mcp_harness as harness


SCRIPT_DIR = pathlib.Path(__file__).resolve().parent
PROJECT_ROOT = SCRIPT_DIR.parents[1]
DEFAULT_CASES = SCRIPT_DIR / "cases.json"
DEFAULT_CAMPAIGN = SCRIPT_DIR / "campaign.config.json"
DEFAULT_CORPUS_LOCK = SCRIPT_DIR / "corpus.lock.json"
DEFAULT_ASSET_LOCK = SCRIPT_DIR / "assets.lock.json"
DEFAULT_BASELINE_PATCHES = SCRIPT_DIR / "baseline-patches.json"
DEFAULT_BENCHMARK_ROOT = pathlib.Path(
    os.environ.get(
        "SFBENCH_ROOT",
        r"C:\AI_STUFF\BENCHMARKS\symforge-8.14.0-surface",
    )
)
PLACEHOLDER = re.compile(r"\$\{([^{}]+)\}")
CCR_HANDLE = re.compile(r'\bhash="([0-9a-f]{12})"')
FULL_SHA = re.compile(r"[0-9a-f]{40,64}")
RUNTIME_ALLOWLIST = (".symforge/**",)
SMOKE_CASE_IDS = ("SF-get_repo_map-001",)
ECONOMICS_ROLES = {
    "task",
    "required_prerequisite",
    "comparison_variant",
    "estimate_diagnostic",
    "oracle_reference",
    "replay_diagnostic",
}


class RunnerError(RuntimeError):
    """Expected failure whose message is safe to persist and display."""


class UnsupportedCase(RunnerError):
    """A frozen case asks for behavior the direct runner cannot execute."""


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def file_sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while chunk := handle.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def load_json(path: pathlib.Path, label: str) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except OSError as exc:
        raise RunnerError(f"could not read {label}") from exc
    except json.JSONDecodeError as exc:
        raise RunnerError(
            f"{label} is invalid JSON at line {exc.lineno}, column {exc.colno}"
        ) from exc
    if not isinstance(value, dict):
        raise RunnerError(f"{label} must contain one JSON object")
    return value


def canonical_path(path: pathlib.Path) -> str:
    rendered = path.resolve().as_posix()
    return rendered.casefold() if os.name == "nt" else rendered


def project_id_for(path: pathlib.Path) -> str:
    return "project-" + hashlib.sha256(canonical_path(path).encode("utf-8")).hexdigest()


def safe_git_environment(extra: dict[str, str] | None = None) -> dict[str, str]:
    env = harness.child_environment("full", False)
    env.update(
        {
            "GIT_TERMINAL_PROMPT": "0",
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_CONFIG_GLOBAL": os.devnull,
            "GIT_LFS_SKIP_SMUDGE": "1",
        }
    )
    if extra:
        env.update(extra)
    return env


def run_process(
    command: Sequence[str],
    *,
    cwd: pathlib.Path | None = None,
    env: dict[str, str] | None = None,
    timeout: float = 120,
    input_bytes: bytes | None = None,
    check: bool = True,
) -> subprocess.CompletedProcess[bytes]:
    try:
        result = subprocess.run(
            list(command),
            cwd=cwd,
            env=env,
            input=input_bytes,
            stdin=None if input_bytes is not None else subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise RunnerError(f"local command failed to execute: {command[0]}") from exc
    if check and result.returncode != 0:
        raise RunnerError(
            f"local command returned {result.returncode}: {pathlib.Path(command[0]).name}"
        )
    return result


def git(
    repo: pathlib.Path,
    *arguments: str,
    env: dict[str, str] | None = None,
    timeout: float = 120,
    check: bool = True,
    input_bytes: bytes | None = None,
) -> subprocess.CompletedProcess[bytes]:
    executable = shutil.which("git")
    if executable is None:
        raise RunnerError("git is required")
    return run_process(
        [executable, "-C", str(repo), *arguments],
        cwd=repo if repo.exists() else None,
        env=env or safe_git_environment(),
        timeout=timeout,
        input_bytes=input_bytes,
        check=check,
    )


def git_text(repo: pathlib.Path, *arguments: str, **kwargs: Any) -> str:
    result = git(repo, *arguments, **kwargs)
    return result.stdout.decode("utf-8", errors="strict").strip()


def require_relative_path(root: pathlib.Path, relative: str) -> pathlib.Path:
    candidate = (root / pathlib.PurePosixPath(relative)).resolve()
    if not harness.is_within(candidate, root.resolve()) or candidate == root.resolve():
        raise RunnerError("a case path escaped its disposable work root")
    return candidate


def fixture_tree_hash(root: pathlib.Path) -> str:
    facts: list[tuple[str, str, int]] = []
    for directory, directories, filenames in os.walk(root, followlinks=False):
        base = pathlib.Path(directory)
        directories[:] = sorted(
            name
            for name in directories
            if name != ".git" and not (base / name).is_symlink()
        )
        for filename in sorted(filenames):
            path = base / filename
            relative = path.relative_to(root)
            if ".git" in relative.parts or path.is_symlink():
                continue
            data = path.read_bytes()
            facts.append((relative.as_posix(), sha256_bytes(data), len(data)))
    digest = hashlib.sha256()
    for relative, sha, size in sorted(facts):
        digest.update(relative.encode("utf-8"))
        digest.update(b"\0")
        digest.update(sha.encode("ascii"))
        digest.update(b"\0")
        digest.update(str(size).encode("ascii"))
        digest.update(b"\n")
    return digest.hexdigest()


def source_inventory(root: pathlib.Path) -> dict[str, Any]:
    entries: list[dict[str, Any]] = []
    for directory, directories, filenames in os.walk(root, followlinks=False):
        base = pathlib.Path(directory)
        retained: list[str] = []
        for name in sorted(directories):
            path = base / name
            if name == ".git":
                continue
            if path.is_symlink():
                relative = path.relative_to(root).as_posix()
                target = os.readlink(path).encode("utf-8", errors="surrogateescape")
                entries.append(
                    {
                        "path": relative,
                        "kind": "directory_symlink",
                        "bytes": len(target),
                        "sha256": sha256_bytes(target),
                        "mode": stat.S_IMODE(path.lstat().st_mode),
                    }
                )
            else:
                retained.append(name)
        directories[:] = retained
        for filename in sorted(filenames):
            path = base / filename
            relative = path.relative_to(root).as_posix()
            if ".git" in pathlib.PurePosixPath(relative).parts:
                continue
            if path.is_symlink():
                target = os.readlink(path).encode("utf-8", errors="surrogateescape")
                entries.append(
                    {
                        "path": relative,
                        "kind": "symlink",
                        "bytes": len(target),
                        "sha256": sha256_bytes(target),
                        "mode": stat.S_IMODE(path.lstat().st_mode),
                    }
                )
                continue
            try:
                data = path.read_bytes()
            except OSError as exc:
                raise RunnerError(
                    f"could not inventory worktree path: {relative}"
                ) from exc
            entries.append(
                {
                    "path": relative,
                    "kind": "file",
                    "bytes": len(data),
                    "sha256": sha256_bytes(data),
                    "mode": stat.S_IMODE(path.stat().st_mode),
                }
            )
    entries.sort(key=lambda entry: entry["path"])
    serialized = harness.canonical_json(entries)
    return {
        "entries": entries,
        "file_count": len(entries),
        "total_bytes": sum(entry["bytes"] for entry in entries),
        "tree_sha256": harness.sha256_text(serialized),
    }


def inventory_changes(
    before: dict[str, Any], after: dict[str, Any]
) -> list[dict[str, Any]]:
    left = {entry["path"]: entry for entry in before["entries"]}
    right = {entry["path"]: entry for entry in after["entries"]}
    changes: list[dict[str, Any]] = []
    for path in sorted(left.keys() | right.keys()):
        old = left.get(path)
        new = right.get(path)
        if old == new:
            continue
        changes.append(
            {
                "path": path,
                "change": "added"
                if old is None
                else "deleted"
                if new is None
                else "modified",
                "before": old,
                "after": new,
            }
        )
    return changes


def git_state_inventory(root: pathlib.Path) -> dict[str, Any]:
    git_dir = root / ".git"
    if not git_dir.exists():
        return {"present": False}
    status_bytes = git(root, "status", "--porcelain=v1", "--untracked-files=all").stdout
    refs_bytes = git(root, "for-each-ref", "--format=%(refname)%00%(objectname)").stdout

    def optional_hash(path: pathlib.Path) -> str | None:
        return file_sha256(path) if path.is_file() else None

    return {
        "present": True,
        "head": git_text(root, "rev-parse", "HEAD"),
        "index_sha256": optional_hash(git_dir / "index"),
        "config_sha256": optional_hash(git_dir / "config"),
        "packed_refs_sha256": optional_hash(git_dir / "packed-refs"),
        "refs_sha256": sha256_bytes(refs_bytes),
        "status_sha256": sha256_bytes(status_bytes),
        "status_porcelain_v1": status_bytes.decode(
            "utf-8", errors="strict"
        ).splitlines(),
    }


def protected_git_state_changed(
    before: dict[str, Any], after: dict[str, Any]
) -> list[str]:
    protected = (
        "present",
        "head",
        "index_sha256",
        "config_sha256",
        "packed_refs_sha256",
        "refs_sha256",
    )
    return [field for field in protected if before.get(field) != after.get(field)]


def matches_any(path: str, patterns: Iterable[str]) -> bool:
    return any(
        fnmatch.fnmatchcase(path, pattern.replace("\\", "/")) for pattern in patterns
    )


def enforce_mutation_policy(
    case: dict[str, Any],
    allowlist: list[str],
    changes: list[dict[str, Any]],
) -> dict[str, Any]:
    source_changes = [
        change
        for change in changes
        if not matches_any(change["path"], RUNTIME_ALLOWLIST)
    ]
    policy = case.get("source_hash_policy")
    violations: list[str] = []
    if policy == "no_source_bytes_may_change":
        violations = [change["path"] for change in source_changes]
    elif policy == "only_allowlisted_paths_may_change":
        violations = [
            change["path"]
            for change in source_changes
            if not matches_any(change["path"], allowlist)
        ]
    else:
        raise RunnerError("unknown source_hash_policy")
    return {
        "policy": policy,
        "resolved_allowlist": allowlist,
        "runtime_allowlist": list(RUNTIME_ALLOWLIST),
        "changed_source_paths": [change["path"] for change in source_changes],
        "violations": violations,
        "safe": not violations,
    }


def flatten_asset_hashes(value: Any, result: dict[str, str]) -> None:
    if isinstance(value, dict):
        path = value.get("path")
        sha = value.get("sha256") or value.get("sha256_hex")
        if (
            isinstance(path, str)
            and isinstance(sha, str)
            and re.fullmatch(r"[0-9a-fA-F]{64}", sha)
        ):
            result[path.replace("\\", "/")] = sha.lower()
        for key, child in value.items():
            if isinstance(child, str) and re.fullmatch(r"[0-9a-fA-F]{64}", child):
                if "/" in key or "\\" in key or "." in pathlib.PurePosixPath(key).name:
                    result[key.replace("\\", "/")] = child.lower()
            elif isinstance(child, dict):
                nested_sha = child.get("sha256") or child.get("sha256_hex")
                if isinstance(nested_sha, str) and re.fullmatch(
                    r"[0-9a-fA-F]{64}", nested_sha
                ):
                    result[key.replace("\\", "/")] = nested_sha.lower()
            flatten_asset_hashes(child, result)
    elif isinstance(value, list):
        for child in value:
            flatten_asset_hashes(child, result)


@dataclass
class InputBundle:
    benchmark_root: pathlib.Path
    fixture_root: pathlib.Path
    cases_path: pathlib.Path
    campaign_path: pathlib.Path
    corpus_lock_path: pathlib.Path
    corpus_manifest_path: pathlib.Path
    oracle_path: pathlib.Path
    cases: dict[str, Any]
    campaign: dict[str, Any]
    corpus_lock: dict[str, Any]
    corpus_manifest: dict[str, Any]
    oracle: dict[str, Any]
    hashes: dict[str, str]
    repositories: dict[str, dict[str, Any]]
    asset_lock_status: dict[str, Any]
    baseline_patches: dict[str, Any] | None
    baseline_patches_path: pathlib.Path | None


def _asset_match(path: pathlib.Path, hashes: dict[str, str]) -> str | None:
    normalized = path.resolve().as_posix()
    candidates = [
        digest
        for name, digest in hashes.items()
        if normalized.endswith(name.replace("\\", "/"))
        or path.name == pathlib.PurePosixPath(name.replace("\\", "/")).name
    ]
    return candidates[0] if len(set(candidates)) == 1 else None


def validate_inputs(
    *,
    benchmark_root: pathlib.Path,
    fixture_root: pathlib.Path,
    cases_path: pathlib.Path,
    campaign_path: pathlib.Path,
    corpus_lock_path: pathlib.Path,
    asset_lock_path: pathlib.Path | None,
    baseline_patches_path: pathlib.Path | None,
    require_asset_lock: bool,
) -> InputBundle:
    benchmark_root = benchmark_root.resolve()
    fixture_root = fixture_root.resolve()
    cases_path = cases_path.resolve()
    campaign_path = campaign_path.resolve()
    corpus_lock_path = corpus_lock_path.resolve()
    corpus_manifest_path = benchmark_root / "corpus-manifest.json"
    oracle_path = fixture_root / "oracle.json"
    cases = load_json(cases_path, "cases manifest")
    campaign = load_json(campaign_path, "campaign config")
    corpus_lock = load_json(corpus_lock_path, "corpus lock")
    corpus_manifest = load_json(corpus_manifest_path, "corpus manifest")
    oracle = load_json(oracle_path, "fixture oracle")
    hashes = {
        "cases": file_sha256(cases_path),
        "campaign": file_sha256(campaign_path),
        "corpus_lock": file_sha256(corpus_lock_path),
        "corpus_manifest": file_sha256(corpus_manifest_path),
        "oracle": file_sha256(oracle_path),
        "direct_case_runner": file_sha256(pathlib.Path(__file__).resolve()),
        "mcp_harness": file_sha256(SCRIPT_DIR / "mcp_harness.py"),
        "fixture_generator": file_sha256(SCRIPT_DIR / "fixture_generator.py"),
        "parity_normalization": file_sha256(SCRIPT_DIR / "parity-normalization.json"),
    }
    baseline_patches: dict[str, Any] | None = None
    resolved_baseline_patches: pathlib.Path | None = None
    if baseline_patches_path is not None and baseline_patches_path.is_file():
        resolved_baseline_patches = baseline_patches_path.resolve()
        baseline_patches = load_json(resolved_baseline_patches, "baseline patches lock")
        hashes["baseline_patches"] = file_sha256(resolved_baseline_patches)

    if cases.get("protocol") != harness.PROTOCOL_ID:
        raise RunnerError("cases protocol does not match the harness")
    if campaign.get("protocol_id") != harness.PROTOCOL_ID:
        raise RunnerError("campaign protocol does not match the harness")
    if campaign.get("mcp", {}).get("primary_transport") != "stdio":
        raise RunnerError("campaign primary direct transport is not stdio")
    expected_version = campaign.get("system_under_test", {}).get("version")
    if not isinstance(expected_version, str) or expected_version not in str(
        cases.get("system_under_test", "")
    ):
        raise RunnerError("cases and campaign disagree on the SUT version")
    safety = campaign.get("safety", {})
    required_safety = {
        "persist_unsanitized_output": False,
        "source_mirrors_are_immutable": True,
        "ordinary_cases_use_independent_clones": True,
        "oracle_visible_to_llm": False,
        "network_during_measurement": False,
    }
    if any(safety.get(key) != expected for key, expected in required_safety.items()):
        raise RunnerError("campaign safety policy is not fail-closed")

    oracle_contract = cases.get("oracle_contract", {})
    if hashes["oracle"] != oracle_contract.get("expected_oracle_sha256"):
        raise RunnerError("fixture oracle SHA-256 does not match cases.json")
    clean_relative = oracle.get("paths", {}).get("clean_repository")
    if not isinstance(clean_relative, str):
        raise RunnerError("fixture oracle omits the clean repository path")
    clean_repo = require_relative_path(fixture_root, clean_relative)
    clean_head = git_text(clean_repo, "rev-parse", "HEAD")
    if clean_head != oracle_contract.get("expected_clean_head"):
        raise RunnerError("fixture clean HEAD does not match cases.json")
    clean_tree = fixture_tree_hash(clean_repo)
    if clean_tree != oracle_contract.get("expected_clean_tree_sha256"):
        raise RunnerError("fixture clean tree hash does not match cases.json")
    if clean_tree != oracle.get("repositories", {}).get("clean", {}).get("tree_sha256"):
        raise RunnerError("fixture clean tree hash does not match oracle.json")
    mutation_relative = oracle.get("paths", {}).get("mutation_repository")
    if not isinstance(mutation_relative, str):
        raise RunnerError("fixture oracle omits the mutation repository path")
    mutation_repo = require_relative_path(fixture_root, mutation_relative)
    mutation_oracle = oracle.get("repositories", {}).get("mutation", {})
    if git_text(mutation_repo, "rev-parse", "HEAD") != mutation_oracle.get("head"):
        raise RunnerError("fixture mutation HEAD does not match oracle.json")
    if fixture_tree_hash(mutation_repo) != mutation_oracle.get("tree_sha256"):
        raise RunnerError("fixture mutation tree hash does not match oracle.json")
    worktree_oracle = oracle.get("worktree", {})
    expected_status = sorted(worktree_oracle.get("status_porcelain_v1", []))
    actual_status = sorted(
        git(mutation_repo, "status", "--porcelain=v1", "--untracked-files=all")
        .stdout.decode("utf-8", errors="strict")
        .splitlines()
    )
    if actual_status != expected_status:
        raise RunnerError("fixture mutation status differs from oracle.json")
    staged_diff = git(
        mutation_repo, "diff", "--cached", "--binary", "--no-ext-diff"
    ).stdout
    unstaged_diff = git(mutation_repo, "diff", "--binary", "--no-ext-diff").stdout
    if sha256_bytes(staged_diff) != worktree_oracle.get("staged", {}).get(
        "diff_sha256"
    ):
        raise RunnerError("fixture staged diff hash differs from oracle.json")
    if sha256_bytes(unstaged_diff) != worktree_oracle.get("unstaged", {}).get(
        "diff_sha256"
    ):
        raise RunnerError("fixture unstaged diff hash differs from oracle.json")
    if corpus_manifest.get("corpus_lock_sha256") != hashes["corpus_lock"]:
        raise RunnerError("corpus lock hash does not match corpus-manifest.json")

    locked = corpus_lock.get("repositories")
    manifested = corpus_manifest.get("repositories")
    if not isinstance(locked, list) or not isinstance(manifested, list):
        raise RunnerError("corpus repository lists are malformed")
    manifest_by_alias = {
        item.get("alias"): item for item in manifested if isinstance(item, dict)
    }
    source_root = benchmark_root / "sources"
    repositories: dict[str, dict[str, Any]] = {}
    for item in locked:
        if not isinstance(item, dict):
            raise RunnerError("corpus lock contains a non-object repository")
        alias = item.get("alias")
        commit = item.get("commit")
        if (
            not isinstance(alias, str)
            or not isinstance(commit, str)
            or not FULL_SHA.fullmatch(commit)
        ):
            raise RunnerError("corpus lock repository identity is malformed")
        manifest_item = manifest_by_alias.get(alias)
        if not isinstance(manifest_item, dict) or manifest_item.get("commit") != commit:
            raise RunnerError(f"corpus manifest mismatch for repository {alias}")
        source = (source_root / alias).resolve()
        if not source.is_dir() or not harness.is_within(source, source_root.resolve()):
            raise RunnerError(f"frozen source mirror is missing for repository {alias}")
        if git_text(source, "rev-parse", "HEAD") != commit:
            raise RunnerError(
                f"frozen source mirror HEAD mismatch for repository {alias}"
            )
        status = git_text(source, "status", "--porcelain=v1", "--untracked-files=all")
        if status:
            raise RunnerError(f"frozen source mirror is dirty for repository {alias}")
        repositories[alias] = {**item, "root": source}

    asset_status: dict[str, Any] = {"required": require_asset_lock, "present": False}
    if asset_lock_path is not None and asset_lock_path.is_file():
        lock = load_json(asset_lock_path.resolve(), "asset lock")
        flattened: dict[str, str] = {}
        flatten_asset_hashes(lock, flattened)
        checked: dict[str, bool] = {}
        asset_inputs: list[tuple[str, pathlib.Path]] = [
            ("cases", cases_path),
            ("campaign", campaign_path),
            ("corpus_lock", corpus_lock_path),
            ("corpus_manifest", corpus_manifest_path),
            ("oracle", oracle_path),
            ("direct_case_runner", pathlib.Path(__file__).resolve()),
            ("mcp_harness", SCRIPT_DIR / "mcp_harness.py"),
            ("fixture_generator", SCRIPT_DIR / "fixture_generator.py"),
            ("parity_normalization", SCRIPT_DIR / "parity-normalization.json"),
        ]
        if resolved_baseline_patches is not None:
            asset_inputs.append(("baseline_patches", resolved_baseline_patches))
        for label, path in asset_inputs:
            expected = _asset_match(path, flattened)
            if expected is None:
                raise RunnerError(f"asset lock has no unique SHA-256 for {label}")
            checked[label] = expected == hashes[label]
            if not checked[label]:
                raise RunnerError(f"asset lock SHA-256 mismatch for {label}")
        asset_status = {
            "required": require_asset_lock,
            "present": True,
            "asset_lock_sha256": file_sha256(asset_lock_path.resolve()),
            "checked": checked,
        }
    elif require_asset_lock:
        raise RunnerError("formal run requires a frozen assets.lock.json")

    validate_case_manifest(cases, repositories)
    if baseline_patches is not None:
        if baseline_patches.get("protocol") != harness.PROTOCOL_ID:
            raise RunnerError("baseline patch lock protocol mismatch")
        sources = baseline_patches.get("source_hashes", {})
        if (
            sources.get("cases") != hashes["cases"]
            or sources.get("oracle") != hashes["oracle"]
        ):
            raise RunnerError("baseline patch lock source hashes are stale")
        patches = baseline_patches.get("patches")
        if not isinstance(patches, dict) or baseline_patches.get("patch_count") != len(
            patches
        ):
            raise RunnerError("baseline patch lock inventory is malformed")
        for case_id, entry in patches.items():
            if not isinstance(entry, dict) or entry.get("case_id") != case_id:
                raise RunnerError("baseline patch lock case identity is malformed")
            patch_text = entry.get("patch")
            if not isinstance(patch_text, str) or harness.sha256_text(
                patch_text
            ) != entry.get("patch_sha256"):
                raise RunnerError("baseline patch text hash mismatch")
    counter = harness.TokenCounter()
    token_metadata = counter.metadata()
    token_config = campaign.get("tokenization", {})
    if token_metadata.get("version") != token_config.get("version"):
        raise RunnerError("installed tokenizer version differs from campaign lock")
    configured = {
        token_config.get("primary", {}).get("encoding"): token_config.get(
            "primary", {}
        ).get("vocabulary_sha256"),
        token_config.get("sensitivity", {}).get("encoding"): token_config.get(
            "sensitivity", {}
        ).get("vocabulary_sha256"),
    }
    actual = {
        entry.get("name"): entry.get("vocabulary_sha256")
        for entry in token_metadata.get("encodings", {}).values()
        if isinstance(entry, dict)
    }
    for encoding, expected_hash in configured.items():
        if not isinstance(encoding, str) or actual.get(encoding) != expected_hash:
            raise RunnerError("tokenizer vocabulary hash differs from campaign lock")

    return InputBundle(
        benchmark_root=benchmark_root,
        fixture_root=fixture_root,
        cases_path=cases_path,
        campaign_path=campaign_path,
        corpus_lock_path=corpus_lock_path,
        corpus_manifest_path=corpus_manifest_path,
        oracle_path=oracle_path,
        cases=cases,
        campaign=campaign,
        corpus_lock=corpus_lock,
        corpus_manifest=corpus_manifest,
        oracle=oracle,
        hashes=hashes,
        repositories=repositories,
        asset_lock_status=asset_status,
        baseline_patches=baseline_patches,
        baseline_patches_path=resolved_baseline_patches,
    )


def validate_case_manifest(
    cases: dict[str, Any], repositories: dict[str, dict[str, Any]]
) -> None:
    rows = cases.get("cases")
    inventory = cases.get("inventory", {})
    if not isinstance(rows, list) or not rows:
        raise RunnerError("cases manifest has no cases")
    ids: set[str] = set()
    expected_surfaces = {
        "full": set(inventory.get("full_surface_tools", [])),
        "compact": set(inventory.get("compact_surface_tools", [])),
        "meta": set(inventory.get("meta_surface_tools", [])),
    }
    if len(expected_surfaces["full"]) != inventory.get("full_surface_count"):
        raise RunnerError("full surface count is internally inconsistent")
    if len(set().union(*expected_surfaces.values())) != inventory.get(
        "unique_tool_names"
    ):
        raise RunnerError("unique tool count is internally inconsistent")
    for case in rows:
        if not isinstance(case, dict):
            raise RunnerError("case row is not an object")
        case_id = case.get("id")
        if not isinstance(case_id, str) or not case_id or case_id in ids:
            raise RunnerError("case IDs must be non-empty and unique")
        ids.add(case_id)
        repo = case.get("repo")
        if repo not in repositories:
            raise RunnerError(f"case {case_id} names an unknown repository")
        if case.get("commit") != f"${{repo.{repo}.commit}}":
            raise RunnerError(
                f"case {case_id} does not pin its declared repository commit"
            )
        surface = case.get("surface")
        primary = case.get("primary_tool")
        if (
            surface not in expected_surfaces
            or primary not in expected_surfaces[surface]
        ):
            raise RunnerError(f"case {case_id} has an invalid surface/tool identity")
        requests = case.get("requests")
        if not isinstance(requests, list) or not requests:
            raise RunnerError(f"case {case_id} has no requests")
        if len(requests) > int(case.get("limits", {}).get("call_limit", 0)):
            raise RunnerError(f"case {case_id} exceeds its frozen call limit")
        for expected_step, request in enumerate(requests, start=1):
            if not isinstance(request, dict) or request.get("step") != expected_step:
                raise RunnerError(f"case {case_id} request steps are not sequential")
            if request.get("tool") not in expected_surfaces[surface]:
                raise RunnerError(
                    f"case {case_id} requests a tool absent from its surface"
                )
            if not isinstance(request.get("args"), dict):
                raise RunnerError(f"case {case_id} request args are not an object")
            if request.get("economics_role") not in ECONOMICS_ROLES:
                raise RunnerError(
                    f"case {case_id} request step {expected_step} lacks a valid "
                    "economics_role"
                )
        if case.get("source_hash_policy") not in {
            "no_source_bytes_may_change",
            "only_allowlisted_paths_may_change",
        }:
            raise RunnerError(f"case {case_id} has an unknown mutation policy")
        if not isinstance(case.get("correctness_oracle"), dict) or not isinstance(
            case.get("stop_condition"), str
        ):
            raise RunnerError(f"case {case_id} lacks a correctness/stop contract")
        if case.get("transport") not in {"stdio", "http"}:
            raise RunnerError(f"case {case_id} has an unknown transport")


def validate_sut(
    bundle: InputBundle,
    server: str,
    *,
    skip_version_probe: bool = False,
) -> tuple[pathlib.Path, dict[str, Any]]:
    server_path = harness.resolve_server(server)
    expected = bundle.campaign.get("system_under_test", {})
    actual_hash = harness.executable_sha256(server_path)
    if actual_hash != expected.get("binary_sha256"):
        raise RunnerError("SymForge executable SHA-256 differs from campaign lock")
    expected_version = expected.get("version")
    version_ok = True
    if not skip_version_probe:
        result = run_process(
            [str(server_path), "--version"],
            env=harness.child_environment("full", False),
            timeout=15,
            check=False,
        )
        rendered = (result.stdout + result.stderr).decode("utf-8", errors="replace")
        version_ok = (
            result.returncode == 0
            and isinstance(expected_version, str)
            and expected_version in rendered
        )
        if not version_ok:
            raise RunnerError("SymForge executable version differs from campaign lock")
    return server_path, {
        "path": str(server_path),
        "sha256": actual_hash,
        "expected_version": expected_version,
        "version_probe_ok": version_ok,
    }


def _tree_lookup(root: Any, dotted: str) -> Any:
    current = root
    for part in dotted.split(".") if dotted else []:
        if not isinstance(current, dict) or part not in current:
            raise KeyError(dotted)
        current = current[part]
    return current


def _set_tree(root: dict[str, Any], dotted: str, value: Any) -> None:
    parts = dotted.split(".")
    current = root
    for part in parts[:-1]:
        child = current.setdefault(part, {})
        if not isinstance(child, dict):
            raise RunnerError("prior binding collides with a scalar value")
        current = child
    current[parts[-1]] = value


@dataclass
class ResolutionContext:
    bundle: InputBundle
    case: dict[str, Any]
    work_root: pathlib.Path
    run_id: str
    project_id: str
    baseline_relative_paths: bool = False
    fixture_runtime_root: pathlib.Path | None = None
    prior: dict[str, Any] = field(default_factory=dict)
    native_oracles: dict[str, Any] = field(default_factory=dict)

    def resolve_name(self, name: str) -> Any:
        if name == "fixture.root":
            if self.fixture_runtime_root is None:
                raise RunnerError(
                    "fixture.root cannot be exposed to a measured command without a disposable auxiliary copy"
                )
            return str(self.fixture_runtime_root)
        if name.startswith("fixture.oracle.repositories."):
            suffix = name.removeprefix("fixture.oracle.repositories.")
            try:
                return _tree_lookup(self.native_oracles, suffix)
            except KeyError:
                pass
        if name.startswith("fixture.oracle.runtime."):
            suffix = name.removeprefix("fixture.oracle.")
            try:
                return _tree_lookup(self.native_oracles, suffix)
            except KeyError:
                pass
        if name.startswith("fixture.oracle.symbol_index."):
            suffix = name.removeprefix("fixture.oracle.symbol_index.")
            parts = suffix.split(".", 2)
            if len(parts) < 2:
                raise RunnerError(f"invalid derived symbol placeholder: {name}")
            language, symbol = parts[:2]
            candidates = [
                entry
                for entry in self.bundle.oracle.get("symbols", {}).get(language, [])
                if isinstance(entry, dict) and entry.get("name") == symbol
            ]
            if len(candidates) != 1:
                raise RunnerError(f"derived symbol placeholder is not unique: {name}")
            if len(parts) == 2:
                return candidates[0]
            try:
                return _tree_lookup(candidates[0], parts[2])
            except KeyError as exc:
                raise RunnerError(
                    f"derived symbol placeholder field does not exist: {name}"
                ) from exc
        fixed_paths = {
            "fixture.oracle.paths.exact_lf_source": "sfbench_fixture/exact/lf_source.py",
            "fixture.oracle.paths.exact_crlf_source": "sfbench_fixture/exact/crlf_source.py",
            "fixture.oracle.paths.exact_no_final_newline": "sfbench_fixture/exact/no_final_newline.py",
            "fixture.oracle.paths.exact_binary": "sfbench_fixture/exact/deterministic.bin",
            "fixture.oracle.history.refs.sfbench-v1": "refs/tags/sfbench-v1",
        }
        if name in fixed_paths:
            return fixed_paths[name]
        if name == "fixture.oracle.mutations.TypeScript.new_file":
            return {
                "path": "sfbench_fixture/typescript/src/new_probe.ts",
                "symbol": "sfbench_new_file_probe",
                "utf8": "export function sfbench_new_file_probe(): string { return 'SF_BENCH_SOURCE_9F31'; }\n",
            }
        if name.startswith("fixture.oracle.mutations.TypeScript.new_file."):
            return _tree_lookup(
                {
                    "path": "sfbench_fixture/typescript/src/new_probe.ts",
                    "symbol": "sfbench_new_file_probe",
                    "utf8": "export function sfbench_new_file_probe(): string { return 'SF_BENCH_SOURCE_9F31'; }\n",
                },
                name.rsplit(".", 1)[1],
            )
        if name in {
            "fixture.oracle.large_file.middle_chunk_100",
            "fixture.oracle.large_file.final_chunk_100",
        }:
            relative = _tree_lookup(self.bundle.oracle, "large_file.path")
            data = require_relative_path(self.work_root, relative).read_bytes()
            lines = data.count(b"\n") + (0 if data.endswith(b"\n") else 1)
            divisor = 200 if name.endswith("middle_chunk_100") else 100
            return max(1, math.ceil(lines / divisor))
        if name.startswith("fixture.oracle."):
            suffix = name.removeprefix("fixture.oracle.")
            try:
                return _tree_lookup(self.bundle.oracle, suffix)
            except KeyError as exc:
                if ".patch" in suffix or suffix.endswith("_patch"):
                    raise UnsupportedCase(
                        f"derived baseline patch is not frozen: {name}"
                    ) from exc
                raise RunnerError(f"unknown oracle placeholder: {name}") from exc
        if name.startswith("repo."):
            _, alias, field_name = name.split(".", 2)
            repository = self.bundle.repositories.get(alias)
            if repository is None or field_name not in {"root", "commit"}:
                raise RunnerError(f"unknown repository placeholder: {name}")
            return (
                str(repository["root"])
                if field_name == "root"
                else repository["commit"]
            )
        if name == "case.work_root":
            return "." if self.baseline_relative_paths else str(self.work_root)
        if name == "case.idempotency_key":
            return f"{harness.PROTOCOL_ID}/{self.run_id}/{self.case['id']}"
        if name == "session.project_id":
            return self.project_id
        if name == "session.model":
            return self.bundle.campaign.get("paired_llm", {}).get("model_alias")
        if name == "session.seed":
            return None
        if name.startswith("prior."):
            try:
                return _tree_lookup(self.prior, name.removeprefix("prior."))
            except KeyError as exc:
                raise RunnerError(f"unbound prior placeholder: {name}") from exc
        raise RunnerError(f"unknown placeholder: {name}")

    def resolve(self, value: Any) -> Any:
        if isinstance(value, dict):
            return {key: self.resolve(child) for key, child in value.items()}
        if isinstance(value, list):
            return [self.resolve(child) for child in value]
        if not isinstance(value, str):
            return value
        exact = PLACEHOLDER.fullmatch(value)
        if exact:
            return self.resolve_name(exact.group(1))
        return PLACEHOLDER.sub(
            lambda match: str(self.resolve_name(match.group(1))), value
        )

    def bind(self, dotted: str, value: Any) -> None:
        normalized = dotted.removeprefix("prior.")
        _set_tree(self.prior, normalized, value)


def selected_overlay_path(path: str, language_root: str | None) -> bool:
    prefixes = [
        "sfbench_fixture/filter_cases/",
        "sfbench_fixture/configs/",
        "sfbench_fixture/exact/",
        "sfbench_fixture/generated/",
        "sfbench_fixture/history/",
        "sfbench_fixture/worktree/",
        "sfbench_fixture/ignored/",
        ".claude/gsd-tools/",
    ]
    if language_root:
        prefixes.insert(0, language_root.rstrip("/") + "/")
    return any(path.startswith(prefix) for prefix in prefixes)


def ls_tree_paths(repo: pathlib.Path, commit: str) -> list[str]:
    raw = git(repo, "ls-tree", "-r", "--name-only", "-z", commit).stdout
    return [part.decode("utf-8", errors="strict") for part in raw.split(b"\0") if part]


def ls_tree_objects(repo: pathlib.Path, commit: str) -> dict[str, str]:
    raw = git(repo, "ls-tree", "-r", "-z", commit).stdout
    result: dict[str, str] = {}
    for record in raw.split(b"\0"):
        if not record:
            continue
        metadata, path_bytes = record.split(b"\t", 1)
        object_id = metadata.split()[2].decode("ascii")
        result[path_bytes.decode("utf-8", errors="strict")] = object_id
    return result


def clone_native(
    source: pathlib.Path,
    destination: pathlib.Path,
    commit: str,
) -> dict[str, str]:
    if destination.exists():
        raise RunnerError("disposable case work root already exists")
    destination.parent.mkdir(parents=True, exist_ok=True)
    executable = shutil.which("git")
    if executable is None:
        raise RunnerError("git is required")
    run_process(
        [
            executable,
            "clone",
            "--local",
            "--no-hardlinks",
            "--no-checkout",
            str(source),
            str(destination),
        ],
        cwd=destination.parent,
        env=safe_git_environment(),
        timeout=300,
    )
    git(destination, "config", "core.autocrlf", "false")
    git(destination, "config", "core.eol", "lf")
    git(destination, "checkout", "--detach", commit, timeout=300)
    if git_text(destination, "rev-parse", "HEAD") != commit:
        raise RunnerError("disposable clone HEAD mismatch")
    if git_text(destination, "status", "--porcelain=v1", "--untracked-files=all"):
        raise RunnerError("disposable native clone is not clean")
    alternates = destination / ".git" / "objects" / "info" / "alternates"
    if alternates.exists():
        raise RunnerError(
            "disposable clone unexpectedly uses an alternate object store"
        )
    return ls_tree_objects(destination, commit)


def source_commit_metadata(repo: pathlib.Path, commit: str) -> dict[str, str]:
    separator = "%x00"
    rendered = (
        git(
            repo,
            "show",
            "-s",
            f"--format=%an{separator}%ae{separator}%aI{separator}%cn{separator}%ce{separator}%cI{separator}%s",
            commit,
        )
        .stdout.decode("utf-8", errors="strict")
        .rstrip("\r\n")
    )
    parts = rendered.split("\0")
    if len(parts) != 7:
        raise RunnerError("fixture commit metadata is malformed")
    return dict(
        zip(
            (
                "author_name",
                "author_email",
                "author_date",
                "committer_name",
                "committer_email",
                "committer_date",
                "subject",
            ),
            parts,
            strict=True,
        )
    )


def replay_fixture_overlay(
    bundle: InputBundle,
    case: dict[str, Any],
    destination: pathlib.Path,
    public_objects: dict[str, str],
) -> dict[str, Any]:
    profile = bundle.cases.get("materialization_profiles", {}).get(
        "public_repo_with_deterministic_fixture_history_overlay", {}
    )
    language_map = profile.get("language_root_map", {})
    language_root = language_map.get(case.get("language"))
    clean_relative = bundle.oracle.get("paths", {}).get("clean_repository")
    fixture_repo = require_relative_path(bundle.fixture_root, clean_relative)
    commits = bundle.oracle.get("history", {}).get("commits", [])
    if not isinstance(commits, list) or len(commits) != 7:
        raise RunnerError("fixture overlay requires exactly seven frozen commits")
    source_commits = [item.get("commit") for item in commits if isinstance(item, dict)]
    if len(source_commits) != 7 or any(
        not isinstance(item, str) for item in source_commits
    ):
        raise RunnerError("fixture history commit list is malformed")

    final_paths = ls_tree_paths(fixture_repo, source_commits[-1])
    selected_final = {
        path for path in final_paths if selected_overlay_path(path, language_root)
    }
    collisions = sorted(path for path in selected_final if path in public_objects)
    if collisions:
        raise RunnerError("fixture overlay would overwrite a public repository path")

    attributes_path = destination / "sfbench_fixture" / ".gitattributes"
    attributes_bytes = (
        b"exact/** -text\n"
        b"generated/large_generated.py -text\n"
        b"configs/bom_crlf_unicode.json -text\n"
    )
    previous: set[str] = set()
    replay_commits: list[str] = []
    for index, source_commit in enumerate(source_commits):
        source_paths = {
            path
            for path in ls_tree_paths(fixture_repo, source_commit)
            if selected_overlay_path(path, language_root)
        }
        for removed in sorted(previous - source_paths):
            target = require_relative_path(destination, removed)
            if target.exists() or target.is_symlink():
                target.unlink()
        for relative in sorted(source_paths):
            target = require_relative_path(destination, relative)
            target.parent.mkdir(parents=True, exist_ok=True)
            data = git(fixture_repo, "show", f"{source_commit}:{relative}").stdout
            target.write_bytes(data)
        if index == 0:
            attributes_path.parent.mkdir(parents=True, exist_ok=True)
            attributes_path.write_bytes(attributes_bytes)
        git(
            destination,
            "add",
            "-A",
            "-f",
            "--",
            ".claude/gsd-tools",
            "sfbench_fixture",
        )
        metadata = source_commit_metadata(fixture_repo, source_commit)
        commit_env = safe_git_environment(
            {
                "GIT_AUTHOR_NAME": metadata["author_name"],
                "GIT_AUTHOR_EMAIL": metadata["author_email"],
                "GIT_AUTHOR_DATE": metadata["author_date"],
                "GIT_COMMITTER_NAME": metadata["committer_name"],
                "GIT_COMMITTER_EMAIL": metadata["committer_email"],
                "GIT_COMMITTER_DATE": metadata["committer_date"],
            }
        )
        git(
            destination,
            "commit",
            "--allow-empty",
            "--no-gpg-sign",
            "-m",
            f"SFBENCH-OVERLAY {metadata['subject']}",
            env=commit_env,
        )
        replay_commits.append(git_text(destination, "rev-parse", "HEAD"))
        previous = source_paths

    source_to_replay = dict(zip(source_commits, replay_commits, strict=True))
    for reference, source_commit in (
        bundle.oracle.get("history", {}).get("refs", {}).items()
    ):
        if not isinstance(reference, str) or not isinstance(source_commit, str):
            continue
        if not (
            reference.startswith("refs/bench/") or reference.startswith("refs/tags/")
        ):
            continue
        replay = source_to_replay.get(source_commit)
        if replay is None:
            raise RunnerError("fixture ref does not resolve to a replay ordinal")
        git(destination, "update-ref", reference, replay)

    files_oracle = bundle.oracle.get("files", {})
    verified = 0
    for relative in sorted(selected_final):
        facts = files_oracle.get(relative)
        if not isinstance(facts, dict):
            raise RunnerError(f"selected fixture file has no oracle hash: {relative}")
        target = require_relative_path(destination, relative)
        if file_sha256(target) != facts.get("sha256"):
            raise RunnerError(f"fixture overlay byte hash mismatch: {relative}")
        verified += 1
    current_objects = ls_tree_objects(destination, "HEAD")
    for relative, object_id in public_objects.items():
        if current_objects.get(relative) != object_id:
            raise RunnerError("fixture overlay changed a public repository object")
    if git_text(destination, "status", "--porcelain=v1", "--untracked-files=all"):
        raise RunnerError("fixture overlay did not produce a clean worktree")
    return {
        "profile": "public_repo_with_deterministic_fixture_history_overlay",
        "language_root": language_root,
        "source_commits": source_commits,
        "replay_commits": replay_commits,
        "verified_fixture_files": verified,
        "public_objects_preserved": len(public_objects),
    }


def copy_file_bytes(source: pathlib.Path, destination: pathlib.Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_bytes(source.read_bytes())


def apply_dirty_fixture_state(
    bundle: InputBundle, destination: pathlib.Path
) -> dict[str, Any]:
    mutation_relative = bundle.oracle.get("paths", {}).get("mutation_repository")
    mutation_repo = require_relative_path(bundle.fixture_root, mutation_relative)
    worktree = bundle.oracle.get("worktree", {})
    staged = worktree.get("staged", {})
    staged_paths: set[str] = set(staged.get("modified", [])) | set(
        staged.get("added", [])
    )
    for rename in staged.get("renamed", []):
        if not isinstance(rename, dict):
            raise RunnerError("dirty-state rename oracle is malformed")
        old = require_relative_path(destination, rename["from"])
        if old.exists():
            old.unlink()
        staged_paths.add(rename["from"])
        staged_paths.add(rename["to"])
    for relative in sorted(staged_paths):
        target = require_relative_path(destination, relative)
        result = git(mutation_repo, "show", f":{relative}", check=False)
        if result.returncode == 0:
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(result.stdout)
        elif target.exists():
            target.unlink()
    git(destination, "add", "-A", "-f", "--", *sorted(staged_paths))
    for relative in worktree.get("unstaged", {}).get("modified", []):
        copy_file_bytes(
            require_relative_path(mutation_repo, relative),
            require_relative_path(destination, relative),
        )
    for relative in worktree.get("unstaged", {}).get("deleted", []):
        target = require_relative_path(destination, relative)
        if target.exists():
            target.unlink()
    for relative in worktree.get("untracked", []):
        copy_file_bytes(
            require_relative_path(mutation_repo, relative),
            require_relative_path(destination, relative),
        )
    actual = (
        git(destination, "status", "--porcelain=v1", "--untracked-files=all")
        .stdout.decode("utf-8", errors="strict")
        .splitlines()
    )
    expected = worktree.get("status_porcelain_v1", [])
    if sorted(actual) != sorted(expected):
        raise RunnerError("dirty-state materialization differs from oracle status")
    return {"status_porcelain_v1": actual}


def materialize_case(
    bundle: InputBundle, case: dict[str, Any], destination: pathlib.Path
) -> dict[str, Any]:
    repository = bundle.repositories[case["repo"]]
    public_objects = clone_native(repository["root"], destination, repository["commit"])
    profiles = bundle.cases.get("materialization_profiles", {})
    native_ids = set(profiles.get("native_only_case_ids", []))
    fixture_repository = case.get("preconditions", {}).get("fixture_repository")
    if case["id"] in native_ids:
        result: dict[str, Any] = {
            "profile": "public_repo_native_only",
            "public_objects_preserved": len(public_objects),
        }
    else:
        result = replay_fixture_overlay(bundle, case, destination, public_objects)
        if fixture_repository == "mutation-repo":
            result["profile"] = "public_repo_with_fixture_history_and_dirty_state"
            result["dirty_state"] = apply_dirty_fixture_state(bundle, destination)
    result.update(
        {
            "repository": case["repo"],
            "base_commit": repository["commit"],
            "head_after_materialization": git_text(destination, "rev-parse", "HEAD"),
        }
    )
    return result


def derive_native_oracles(alias: str, work_root: pathlib.Path) -> dict[str, Any]:
    if alias != "p-map":
        return {}
    anchor = work_root / "index.js"
    if not anchor.is_file():
        raise RunnerError("p-map native anchor index.js is absent")
    data = anchor.read_bytes()
    text = data.decode("utf-8", errors="strict")
    match = re.search(
        r"\b(?:export\s+default\s+)?(?:async\s+)?function\s+([A-Za-z_$][\w$]*)",
        text,
    )
    if match is None:
        raise RunnerError("p-map native symbol anchor could not be derived")
    line_end = 0
    lines = data.splitlines(keepends=True)
    for line in lines[:5]:
        line_end += len(line)
    return {
        "p-map": {
            "native_anchor_file": "index.js",
            "native_anchor_symbol": match.group(1),
            "native_slice": {"start_byte": 0, "end_byte": max(1, line_end)},
            "native_anchor_range_sha256": sha256_bytes(data[:line_end]),
            "inventory": source_inventory(work_root),
        }
    }


@dataclass
class ActiveSession:
    client: harness.StdioMcpClient
    config: harness.CaptureConfig
    role: str
    tools: list[Any]


class SessionManager:
    def __init__(
        self,
        *,
        server_path: pathlib.Path,
        server_args: list[str],
        work_root: pathlib.Path,
        writer: harness.JsonlWriter,
        sanitizer: harness.Sanitizer,
        counter: harness.TokenCounter,
        run_id: str,
        case_id: str,
        timeout: float,
        expected_tools: dict[str, set[str]],
    ) -> None:
        self.server_path = server_path
        self.server_args = server_args
        self.work_root = work_root
        self.writer = writer
        self.sanitizer = sanitizer
        self.counter = counter
        self.run_id = run_id
        self.case_id = case_id
        self.timeout = timeout
        self.expected_tools = expected_tools
        self.active: ActiveSession | None = None
        self.lifecycle_count = 0

    def start(self, surface: str, role: str) -> ActiveSession:
        if self.active is not None:
            raise RunnerError(
                "attempted to start a second MCP process without closing the first"
            )
        config = harness.CaptureConfig(
            mode="manifest",
            repo=self.work_root,
            output=self.writer.path,
            server=str(self.server_path),
            server_args=list(self.server_args),
            surface=surface,
            auto_index=True,
            timeout=self.timeout,
            protocol_version=harness.MCP_PROTOCOL_VERSION,
            run_id=self.run_id,
            case_id=self.case_id,
            append=True,
        )
        client = harness.StdioMcpClient(
            [str(self.server_path), *self.server_args],
            self.work_root,
            harness.child_environment(surface, True),
            self.timeout,
            self.sanitizer,
            self.counter,
        )
        initialize = client.rpc(
            "initialize",
            {
                "protocolVersion": harness.MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {
                    "name": "symforge-sfbench-direct-case-runner",
                    "version": "1.0",
                },
            },
        )
        record = harness.capture_record(config, initialize)
        record.update({"record_type": "session_rpc", "session_role": role})
        self.writer.write(record)
        if initialize.response_raw is None or "error" in initialize.response_raw:
            client.close()
            raise RunnerError("MCP initialize failed")
        initialized = client.notify("notifications/initialized", {})
        record = harness.capture_record(config, initialized)
        record.update({"record_type": "session_rpc", "session_role": role})
        self.writer.write(record)

        manifest: dict[str, list[Any]] = {}
        list_metrics: dict[str, Any] = {}
        for method, result_key in harness.LIST_METHODS:
            items, metrics = harness.list_all(
                client, self.writer, config, method, result_key
            )
            manifest[result_key] = items
            list_metrics[method] = metrics
            if metrics.get("error") is not None:
                client.close()
                raise RunnerError(f"MCP manifest pagination failed for {method}")
        tool_names = {
            item.get("name")
            for item in manifest.get("tools", [])
            if isinstance(item, dict) and isinstance(item.get("name"), str)
        }
        if tool_names != self.expected_tools[surface]:
            client.close()
            raise RunnerError(
                f"live {surface} tools/list differs from frozen inventory"
            )
        manifest_text = harness.canonical_json(manifest)
        counts = self.counter.metrics(manifest_text)
        tools_text = harness.canonical_json(manifest.get("tools", []))
        tool_counts = self.counter.metrics(tools_text)
        self.writer.write(
            {
                **harness.base_record(config),
                "record_type": "session_manifest",
                "session_role": role,
                "manifest": manifest,
                "manifest_sha256": harness.sha256_text(manifest_text),
                "manifest_utf8_bytes": counts["utf8_bytes"],
                "manifest_cl100k": counts["cl100k"],
                "manifest_o200k": counts["o200k"],
                "full_tools_list_tokens": {
                    "cl100k": tool_counts["cl100k"],
                    "o200k": tool_counts["o200k"],
                },
                "schema_metadata": harness.schema_metadata(
                    manifest.get("tools", []), self.counter
                ),
                "list_metrics": list_metrics,
            }
        )
        self.lifecycle_count += 1
        self.active = ActiveSession(
            client=client, config=config, role=role, tools=manifest["tools"]
        )
        return self.active

    def call(
        self,
        tool: str,
        arguments: dict[str, Any],
        *,
        record_type: str,
        extra: dict[str, Any] | None = None,
    ) -> harness.RpcCapture:
        if self.active is None:
            raise RunnerError("no active MCP session")
        capture = self.active.client.rpc(
            "tools/call", {"name": tool, "arguments": arguments}
        )
        record = harness.capture_record(self.active.config, capture)
        record["record_type"] = record_type
        record["session_role"] = self.active.role
        if extra:
            record.update(extra)
        self.writer.write(record)
        return capture

    def wait_ready(self, surface: str, *, timeout: float = 120) -> None:
        deadline = time.monotonic() + timeout
        attempts = 0
        if surface == "full":
            tool, arguments = "health_compact", {}
        elif surface == "compact":
            tool, arguments = "status", {"detail": "compact"}
        else:
            # The meta facade has no side-effect-free readiness probe.
            return
        while True:
            attempts += 1
            capture = self.call(
                tool,
                arguments,
                record_type="preflight_rpc",
                extra={"economics_excluded": True, "preflight_purpose": "wait_ready"},
            )
            text = harness.extract_content_text(capture.response_raw).casefold()
            if harness.rpc_status(capture) == "ok" and "loading" not in text:
                return
            if time.monotonic() >= deadline:
                raise RunnerError("MCP index did not become ready before timeout")
            time.sleep(min(0.25 * attempts, 1.0))

    def close(self) -> None:
        if self.active is None:
            return
        lifecycle = self.active.client.close()
        self.writer.write(
            {
                **harness.base_record(self.active.config),
                "record_type": "process_lifecycle",
                "session_role": self.active.role,
                "process_start_ms": self.active.client.process_start_ms,
                **lifecycle,
            }
        )
        self.active = None


PRE_INDEX_ACTIONS = {
    "write_fixture_file",
    "runtime.quarantine.populate_malformed_configs",
    "session_context.start_empty",
}
SUPPORTED_ORACLE_ACTIONS = {
    "mutations.Rust.replace_symbol.apply_to_work_root",
    "mutations.TypeScript.new_file.create",
    "impact.Rust.leaf_dirty.apply",
    "impact.Python.leaf_dirty.apply",
    "impact.TypeScript.leaf_dirty_and_data.apply",
    "mutations.Python.concurrent_divergence.prepare",
    "mutations.stale_index.prepare",
    "runtime.quarantine.populate_malformed_configs",
    "session_context.start_empty",
    "symbols.Rust.sfbench_leaf.read_exact_body",
}


def action_phase(action: dict[str, Any]) -> str:
    executor = action.get("executor")
    key = action.get("key")
    if executor in {"oracle_lookup", "write_fixture_file"}:
        return "pre_index"
    if executor != "oracle_action" or not isinstance(key, str):
        raise UnsupportedCase(f"unsupported setup executor: {executor}")
    if key not in SUPPORTED_ORACLE_ACTIONS:
        raise UnsupportedCase(f"unsupported setup action: {key}")
    return "pre_index" if key in PRE_INDEX_ACTIONS else "post_index"


def find_symbol_oracle(
    oracle: dict[str, Any], language: str, symbol: str
) -> dict[str, Any]:
    candidates = [
        item
        for item in oracle.get("symbols", {}).get(language, [])
        if isinstance(item, dict) and item.get("name") == symbol
    ]
    if len(candidates) != 1:
        raise RunnerError(f"symbol oracle is not unique: {language}.{symbol}")
    return candidates[0]


def apply_replace_mutation(context: ResolutionContext, language: str) -> dict[str, Any]:
    mutation = context.bundle.oracle.get("mutations", {}).get(language, {})
    relative = mutation.get("path")
    replace = mutation.get("replace_symbol", {})
    if not isinstance(relative, str) or not isinstance(replace, dict):
        raise RunnerError("replace mutation oracle is malformed")
    path = require_relative_path(context.work_root, relative)
    before = path.read_bytes()
    if sha256_bytes(before) != replace.get("before_sha256"):
        raise RunnerError("replace mutation before hash mismatch")
    symbol = find_symbol_oracle(context.bundle.oracle, language, replace["symbol"])
    start = int(symbol["start_byte"])
    end = int(symbol["end_byte_exclusive"])
    source = before[start:end]
    if sha256_bytes(source) != symbol.get("source_sha256"):
        raise RunnerError("replace mutation symbol source hash mismatch")
    after = before[:start] + str(replace["new_body"]).encode("utf-8") + before[end:]
    if sha256_bytes(after) != replace.get("after_sha256"):
        raise RunnerError("replace mutation after hash mismatch")
    path.write_bytes(after)
    return {"path": relative, "after_sha256": sha256_bytes(after)}


def execute_setup_action(
    action: dict[str, Any],
    context: ResolutionContext,
    hooks: dict[str, list[dict[str, Any]]],
) -> dict[str, Any]:
    executor = action.get("executor")
    if executor == "oracle_lookup":
        key = action.get("key")
        where = action.get("where")
        save_as = action.get("save_as")
        if (
            not isinstance(key, str)
            or not isinstance(where, dict)
            or not isinstance(save_as, str)
        ):
            raise RunnerError("oracle_lookup action is malformed")
        try:
            values = _tree_lookup(context.bundle.oracle, key)
        except KeyError as exc:
            raise RunnerError("oracle_lookup key does not exist") from exc
        if not isinstance(values, list):
            raise RunnerError("oracle_lookup key does not select an array")
        matches = [
            item
            for item in values
            if isinstance(item, dict)
            and all(item.get(name) == value for name, value in where.items())
        ]
        if len(matches) != 1:
            raise RunnerError("oracle_lookup did not select exactly one item")
        context.bind(save_as, matches[0])
        return {
            "executor": executor,
            "key": key,
            "selected_sha256": harness.sha256_text(harness.canonical_json(matches[0])),
        }
    if executor == "write_fixture_file":
        relative = action.get("path")
        utf8 = action.get("utf8")
        if not isinstance(relative, str) or not isinstance(utf8, str):
            raise RunnerError("write_fixture_file action is malformed")
        path = require_relative_path(context.work_root, relative)
        path.parent.mkdir(parents=True, exist_ok=True)
        data = utf8.encode("utf-8")
        path.write_bytes(data)
        return {"executor": executor, "path": relative, "sha256": sha256_bytes(data)}
    if executor != "oracle_action":
        raise UnsupportedCase(f"unsupported setup executor: {executor}")
    key = action.get("key")
    if key not in SUPPORTED_ORACLE_ACTIONS:
        raise UnsupportedCase(f"unsupported setup action: {key}")
    result: dict[str, Any]
    if key in {
        "mutations.Rust.replace_symbol.apply_to_work_root",
        "impact.Rust.leaf_dirty.apply",
    }:
        result = apply_replace_mutation(context, "Rust")
    elif key == "impact.Python.leaf_dirty.apply":
        result = apply_replace_mutation(context, "Python")
    elif key == "impact.TypeScript.leaf_dirty_and_data.apply":
        result = apply_replace_mutation(context, "TypeScript")
        data_path = require_relative_path(
            context.work_root, "sfbench_fixture/typescript/src/impact_probe.json"
        )
        data = b'{"marker":"SF_BENCH_DATA_9F31"}\n'
        data_path.write_bytes(data)
        result["data_path"] = data_path.relative_to(context.work_root).as_posix()
        result["data_sha256"] = sha256_bytes(data)
    elif key == "mutations.TypeScript.new_file.create":
        value = context.resolve_name("fixture.oracle.mutations.TypeScript.new_file")
        path = require_relative_path(context.work_root, value["path"])
        path.parent.mkdir(parents=True, exist_ok=True)
        data = value["utf8"].encode("utf-8")
        if data.count(value["symbol"].encode("utf-8")) != 1:
            raise RunnerError("new-file setup symbol is not unique")
        path.write_bytes(data)
        result = {"path": value["path"], "sha256": sha256_bytes(data)}
    elif key == "mutations.Python.concurrent_divergence.prepare":
        relative = context.bundle.oracle["mutations"]["Python"]["path"]
        path = require_relative_path(context.work_root, relative)
        before = path.read_bytes()
        after = before + b"# SFBENCH_CONCURRENT_9F31\n"
        path.write_bytes(after)
        result = {"path": relative, "after_sha256": sha256_bytes(after)}
    elif key == "mutations.stale_index.prepare":
        hooks.setdefault("repeat", []).append({"kind": "stale_index_write"})
        result = {"hook_after_request": "repeat", "kind": "stale_index_write"}
    elif key == "runtime.quarantine.populate_malformed_configs":
        hashes: list[str] = []
        for offset in range(23):
            relative = f"sfbench_fixture/configs/quarantine/page_{offset:03d}.json"
            path = require_relative_path(context.work_root, relative)
            path.parent.mkdir(parents=True, exist_ok=True)
            data = f'{{"offset":{offset},"broken":]\n'.encode("utf-8")
            path.write_bytes(data)
            hashes.append(sha256_bytes(data))
        runtime = context.native_oracles.setdefault("runtime", {}).setdefault(
            "quarantine", {}
        )
        runtime.update(
            {
                "count": 23,
                "middle_offset": 11,
                "final_offset": 22,
                "out_of_range_offset": 24,
            }
        )
        result = {
            "count": 23,
            "aggregate_sha256": harness.sha256_text("\n".join(hashes)),
        }
    elif key == "session_context.start_empty":
        result = {
            "assertion": "runner made no commitment reads before measured request"
        }
    elif key == "symbols.Rust.sfbench_leaf.read_exact_body":
        symbol = find_symbol_oracle(context.bundle.oracle, "Rust", "sfbench_leaf")
        path = require_relative_path(context.work_root, symbol["path"])
        data = path.read_bytes()[
            int(symbol["start_byte"]) : int(symbol["end_byte_exclusive"])
        ]
        if sha256_bytes(data) != symbol["source_sha256"]:
            raise RunnerError("exact-body setup hash mismatch")
        value = {
            "body": data.decode("utf-8", errors="strict"),
            "sha256": sha256_bytes(data),
        }
        save_as = action.get("save_as")
        if isinstance(save_as, str):
            context.bind(save_as, value)
        result = {"path": symbol["path"], "sha256": value["sha256"]}
    else:  # pragma: no cover - guarded by the supported set
        raise UnsupportedCase(f"unsupported setup action: {key}")
    result.update({"executor": executor, "key": key})
    return result


def execute_hook(hook: dict[str, Any], context: ResolutionContext) -> dict[str, Any]:
    kind = hook.get("kind")
    if kind == "stale_index_write":
        mutation = context.bundle.oracle.get("mutations", {}).get("stale_index", {})
        path = require_relative_path(context.work_root, mutation["path"])
        data = str(mutation["after_bytes_utf8"]).encode("utf-8")
        if sha256_bytes(data) != mutation.get("after_sha256"):
            raise RunnerError("stale-index hook oracle hash mismatch")
        path.write_bytes(data)
        return {"kind": kind, "path": mutation["path"], "sha256": sha256_bytes(data)}
    raise UnsupportedCase(f"unsupported between-request hook: {kind}")


def validate_shell_recipe(command: str) -> None:
    forbidden = re.compile(
        r"(?ix)(?:https?://|\bcurl\b|\bwget\b|invoke-webrequest|"
        r"\bgit\s+(?:clone|fetch|pull|push)\b|"
        r"(?:^|[|;&]\s*)symforge(?:\.exe)?(?:\s|$))"
    )
    if forbidden.search(command):
        raise UnsupportedCase("baseline shell recipe violates the local-only policy")


def canonical_unified_diff(path: str, before: bytes, after: bytes) -> str:
    return "".join(
        difflib.unified_diff(
            before.decode("utf-8").splitlines(keepends=True),
            after.decode("utf-8").splitlines(keepends=True),
            fromfile=f"a/{path}",
            tofile=f"b/{path}",
            lineterm="\n",
        )
    )


def splice_symbol(before: bytes, symbol: dict[str, Any], replacement: bytes) -> bytes:
    start = int(symbol["start_byte"])
    end = int(symbol["end_byte_exclusive"])
    return before[:start] + replacement + before[end:]


def mutation_file_after(
    bundle: InputBundle, language: str, operation: str
) -> tuple[str, bytes, bytes]:
    mutation = bundle.oracle["mutations"][language]
    relative = mutation["path"]
    clean_repo = require_relative_path(
        bundle.fixture_root, bundle.oracle["paths"]["clean_repository"]
    )
    before = require_relative_path(clean_repo, relative).read_bytes()
    record = mutation[operation]
    if sha256_bytes(before) != record.get("before_sha256"):
        raise RunnerError(
            f"baseline patch source hash mismatch: {language}.{operation}"
        )
    if operation == "replace_symbol":
        symbol = find_symbol_oracle(bundle.oracle, language, record["symbol"])
        after = splice_symbol(before, symbol, record["new_body"].encode("utf-8"))
    elif operation == "edit_within":
        symbol = find_symbol_oracle(bundle.oracle, language, record["symbol"])
        start = int(symbol["start_byte"])
        end = int(symbol["end_byte_exclusive"])
        body = before[start:end]
        after = (
            before[:start]
            + body.replace(
                record["old_text"].encode("utf-8"),
                record["new_text"].encode("utf-8"),
            )
            + before[end:]
        )
    elif operation == "delete_symbol":
        symbol = find_symbol_oracle(bundle.oracle, language, record["symbol"])
        start = int(symbol["start_byte"])
        end = int(symbol["end_byte_exclusive"])
        prefix, suffix = before[:start], before[end:]
        if not prefix.endswith(b"\n\n") or not suffix.startswith(b"\n\n"):
            raise RunnerError("delete baseline spacing invariant failed")
        after = prefix + suffix[2:]
    elif operation == "rename_symbol":
        old = record["name"].encode("utf-8")
        new = record["new_name"].encode("utf-8")
        references = bundle.oracle["references"][language]
        definition = references["definition"]
        definition_start = int(definition["start_byte"])
        definition_end = int(definition["end_byte_exclusive"])
        definition_offset = definition_start + before[
            definition_start:definition_end
        ].index(old)
        offsets = [definition_offset] + [
            int(item["start_byte"]) for item in references["code_references"]
        ]
        after = before
        for offset in sorted(offsets, reverse=True):
            if after[offset : offset + len(old)] != old:
                raise RunnerError("rename baseline offset invariant failed")
            after = after[:offset] + new + after[offset + len(old) :]
    else:
        raise RunnerError(f"unknown baseline mutation operation: {operation}")
    if sha256_bytes(after) != record.get("after_sha256"):
        raise RunnerError(
            f"baseline patch result hash mismatch: {language}.{operation}"
        )
    diff = canonical_unified_diff(relative, before, after)
    if record.get("diff_sha256") and harness.sha256_text(diff) != record["diff_sha256"]:
        raise RunnerError(f"baseline patch diff hash mismatch: {language}.{operation}")
    return relative, before, after


def insert_symbols(
    before: bytes,
    symbols: list[dict[str, Any]],
    content: bytes,
    position: str,
) -> bytes:
    edits: list[tuple[int, bytes]] = []
    for symbol in symbols:
        if position == "before":
            edits.append((int(symbol["start_byte"]), content + b"\n\n"))
        elif position == "after":
            edits.append((int(symbol["end_byte_exclusive"]), b"\n\n" + content))
        else:
            raise RunnerError("unknown insert position")
    after = before
    for offset, inserted in sorted(edits, reverse=True):
        after = after[:offset] + inserted + after[offset:]
    return after


def generate_case_patch(bundle: InputBundle, case: dict[str, Any]) -> dict[str, Any]:
    case_id = case["id"]
    language = case["language"]
    files: dict[str, tuple[bytes, bytes]] = {}
    simple_operations = {
        "SF-batch_rename-001": ("Rust", "rename_symbol"),
        "SF-delete_symbol-001": ("Rust", "delete_symbol"),
        "SF-delete_symbol-003": ("TypeScript", "delete_symbol"),
        "SF-edit_within_symbol-001": ("Rust", "edit_within"),
        "SF-edit_within_symbol-003": ("TypeScript", "edit_within"),
        "SF-replace_symbol_body-001": ("Rust", "replace_symbol"),
        "SF-replace_symbol_body-003": ("TypeScript", "replace_symbol"),
        "SF-symforge_edit-001": ("Rust", "replace_symbol"),
        "SF-symforge_edit-004": ("Java", "replace_symbol"),
    }
    if case_id in simple_operations:
        selected_language, operation = simple_operations[case_id]
        relative, before, after = mutation_file_after(
            bundle, selected_language, operation
        )
        files[relative] = (before, after)
    elif case_id in {
        "SF-insert_symbol-001",
        "SF-insert_symbol-003",
        "SF-batch_insert-001",
        "SF-batch_insert-003",
    }:
        mutation = bundle.oracle["mutations"][language]
        clean_repo = require_relative_path(
            bundle.fixture_root, bundle.oracle["paths"]["clean_repository"]
        )
        content = mutation["insert_symbol"]["body"].encode("utf-8")
        if case_id.startswith("SF-batch_insert"):
            apply_request = next(
                (
                    request
                    for request in case["requests"]
                    if request.get("args", {}).get("dry_run") is False
                ),
                case["requests"][0],
            )
            args = apply_request["args"]
            position = str(args["position"])
            targets_by_path: dict[str, list[dict[str, Any]]] = {}
            for target in args["targets"]:
                if isinstance(target, str):
                    raw_path, target_name = target.rsplit("::", 1)
                    target = {"path": raw_path, "name": target_name}
                raw_path = target.get("path")
                relative = (
                    str(mutation["path"])
                    if not raw_path or str(raw_path).startswith("${")
                    else str(raw_path)
                )
                targets_by_path.setdefault(relative, []).append(target)
            for relative, targets in sorted(targets_by_path.items()):
                before = require_relative_path(clean_repo, relative).read_bytes()
                symbols = [
                    find_symbol_oracle(bundle.oracle, language, str(target["name"]))
                    for target in targets
                ]
                files[relative] = (
                    before,
                    insert_symbols(before, symbols, content, position),
                )
        else:
            relative = mutation["path"]
            before = require_relative_path(clean_repo, relative).read_bytes()
            symbol = find_symbol_oracle(
                bundle.oracle, language, mutation["insert_symbol"]["anchor"]
            )
            position = "before" if case_id.endswith("001") else "after"
            files[relative] = (
                before,
                insert_symbols(before, [symbol], content, position),
            )
    elif case_id in {"SF-batch_edit-001", "SF-batch_edit-003"}:
        mutation = bundle.oracle["mutations"][language]
        relative = mutation["path"]
        clean_repo = require_relative_path(
            bundle.fixture_root, bundle.oracle["paths"]["clean_repository"]
        )
        before = require_relative_path(clean_repo, relative).read_bytes()
        mutable = find_symbol_oracle(bundle.oracle, language, "sfbench_mutable")
        start, end = int(mutable["start_byte"]), int(mutable["end_byte_exclusive"])
        edit = mutation["edit_within"]
        body = before[start:end].replace(
            edit["old_text"].encode("utf-8"),
            edit["new_text"].encode("utf-8"),
            1,
        )
        after = before[:start] + body + before[end:]
        if case_id.endswith("003"):
            delete = find_symbol_oracle(bundle.oracle, language, "sfbench_delete_me")
            delete_source = before[
                int(delete["start_byte"]) : int(delete["end_byte_exclusive"])
            ]
            location = after.index(delete_source)
            prefix, suffix = after[:location], after[location + len(delete_source) :]
            after = prefix + (suffix[2:] if suffix.startswith(b"\n\n") else suffix)
        files[relative] = (before, after)
        if case_id.endswith("001"):
            protocol_path = (
                f"{bundle.oracle['languages']['Rust']['root']}/src/protocol.rs"
            )
            protocol_before = require_relative_path(
                clean_repo, protocol_path
            ).read_bytes()
            protocol_symbol = find_symbol_oracle(
                bundle.oracle, "Rust", "SfBenchProtocol"
            )
            protocol_after = insert_symbols(
                protocol_before,
                [protocol_symbol],
                mutation["insert_symbol"]["body"].encode("utf-8"),
                "after",
            )
            files[protocol_path] = (protocol_before, protocol_after)
    elif case_id == "SF-symforge_edit-003":
        mutation = bundle.oracle["mutations"]["TypeScript"]
        relative = mutation["path"]
        clean_repo = require_relative_path(
            bundle.fixture_root, bundle.oracle["paths"]["clean_repository"]
        )
        before = require_relative_path(clean_repo, relative).read_bytes()
        edit = mutation["edit_within"]
        mutable = find_symbol_oracle(bundle.oracle, "TypeScript", "sfbench_mutable")
        start, end = int(mutable["start_byte"]), int(mutable["end_byte_exclusive"])
        after = (
            before[:start]
            + before[start:end].replace(
                edit["old_text"].encode(), edit["new_text"].encode()
            )
            + before[end:]
        )
        # This control case previews three alternatives. Freeze their combined
        # request payload only; it is never applied during the baseline arm.
        entry_source = before[
            int(
                find_symbol_oracle(bundle.oracle, "TypeScript", "sfbench_entry")[
                    "start_byte"
                ]
            ) : int(
                find_symbol_oracle(bundle.oracle, "TypeScript", "sfbench_entry")[
                    "end_byte_exclusive"
                ]
            )
        ]
        entry_at = after.index(entry_source)
        content = mutation["insert_symbol"]["body"].encode()
        after = after[:entry_at] + content + b"\n\n" + after[entry_at:]
        files[relative] = (before, after)
    else:
        raise UnsupportedCase(f"no deterministic patch generator for {case_id}")

    patch = "".join(
        canonical_unified_diff(path, before, after)
        for path, (before, after) in sorted(files.items())
    )
    if not patch:
        raise RunnerError(f"generated baseline patch is empty for {case_id}")
    recipe_steps = [
        index
        for index, recipe in enumerate(case.get("baseline_recipe", []), start=1)
        if recipe.get("executor") == "apply_patch"
    ]
    if len(recipe_steps) != 1:
        raise RunnerError(f"expected one apply_patch recipe for {case_id}")
    return {
        "case_id": case_id,
        "recipe_step": recipe_steps[0],
        "placeholder": case["baseline_recipe"][recipe_steps[0] - 1]["patch"],
        "patch": patch,
        "patch_sha256": harness.sha256_text(patch),
        "patch_utf8_bytes": len(patch.encode("utf-8")),
        "before_sha256": {
            path: sha256_bytes(before) for path, (before, _) in sorted(files.items())
        },
        "after_sha256": {
            path: sha256_bytes(after) for path, (_, after) in sorted(files.items())
        },
        "apply_during_baseline": any(
            request.get("args", {}).get("dry_run") is False
            or request.get("args", {}).get("apply") is True
            for request in case.get("requests", [])
        ),
    }


def powershell_executable() -> str:
    candidate = shutil.which("pwsh") or shutil.which("powershell")
    if candidate is None:
        raise UnsupportedCase("PowerShell is unavailable for a frozen baseline recipe")
    return candidate


def baseline_record(
    *,
    config: harness.CaptureConfig,
    counter: harness.TokenCounter,
    sanitizer: harness.Sanitizer,
    step: int,
    executor: str,
    input_text: str,
    output_bytes: bytes,
    stderr_bytes: bytes,
    exit_code: int | None,
    elapsed_ms: float,
    status: str,
    reason: str | None = None,
) -> dict[str, Any]:
    raw_output = output_bytes.decode("utf-8", errors="replace")
    raw_stderr = stderr_bytes.decode("utf-8", errors="replace")
    raw_payload = executor + "\n" + input_text + "\n" + raw_output + raw_stderr
    input_metrics = counter.metrics(input_text)
    output_metrics = counter.metrics(
        raw_output + raw_stderr, byte_length=len(output_bytes) + len(stderr_bytes)
    )
    payload_metrics = counter.metrics(raw_payload)
    safe_input = sanitizer.sanitize_text(input_text, f"baseline[{step}].input")
    safe_output = sanitizer.sanitize_text(raw_output, f"baseline[{step}].stdout")
    safe_stderr = sanitizer.sanitize_text(raw_stderr, f"baseline[{step}].stderr")
    return {
        **harness.base_record(config),
        "execution_mode": "recipe_baseline",
        "record_type": "baseline_step",
        "baseline_step": step,
        "executor": executor,
        "status": status,
        "reason": reason,
        "elapsed_ms": elapsed_ms,
        "exit_code": exit_code,
        "input": safe_input,
        "stdout": safe_output,
        "stderr": safe_stderr,
        "input_utf8_bytes": input_metrics["utf8_bytes"],
        "output_utf8_bytes": output_metrics["utf8_bytes"],
        "direct_payload_utf8_bytes": payload_metrics["utf8_bytes"],
        "input_cl100k": input_metrics["cl100k"],
        "output_cl100k": output_metrics["cl100k"],
        "direct_payload_cl100k": payload_metrics["cl100k"],
        "input_o200k": input_metrics["o200k"],
        "output_o200k": output_metrics["o200k"],
        "direct_payload_o200k": payload_metrics["o200k"],
        "token_count_basis": {
            "primary": "exact unsanitized UTF-8 counted only in memory",
            "artifact": "sanitized persisted fields",
        },
    }


def run_baseline(
    *,
    case: dict[str, Any],
    context: ResolutionContext,
    writer: harness.JsonlWriter,
    counter: harness.TokenCounter,
    sanitizer: harness.Sanitizer,
    timeout: float,
) -> None:
    config = harness.CaptureConfig(
        mode="baseline",
        repo=context.work_root,
        output=writer.path,
        server="",
        server_args=[],
        surface=case["surface"],
        auto_index=False,
        timeout=timeout,
        protocol_version=harness.MCP_PROTOCOL_VERSION,
        run_id=context.run_id,
        case_id=case["id"],
        append=True,
    )
    totals = {"cl100k": 0, "o200k": 0, "utf8_bytes": 0}
    for index, recipe in enumerate(case.get("baseline_recipe", []), start=1):
        if not isinstance(recipe, dict):
            raise RunnerError("baseline recipe row is not an object")
        executor = recipe.get("executor")
        if executor == "capability_only":
            reason = str(recipe.get("reason", "no equivalent baseline"))
            record = baseline_record(
                config=config,
                counter=counter,
                sanitizer=sanitizer,
                step=index,
                executor=executor,
                input_text=reason,
                output_bytes=b"",
                stderr_bytes=b"",
                exit_code=None,
                elapsed_ms=0.0,
                status="not_applicable",
                reason=reason,
            )
        elif executor == "apply_patch":
            lock = context.bundle.baseline_patches
            entry = (
                lock.get("patches", {}).get(case["id"])
                if isinstance(lock, dict)
                else None
            )
            if (
                not isinstance(entry, dict)
                or entry.get("recipe_step") != index
                or entry.get("placeholder") != recipe.get("patch")
            ):
                raise UnsupportedCase(
                    "baseline apply_patch has no matching frozen lock entry"
                )
            patch_value = entry.get("patch")
            if not isinstance(patch_value, str) or harness.sha256_text(
                patch_value
            ) != entry.get("patch_sha256"):
                raise RunnerError(
                    "frozen baseline patch failed its call-time hash gate"
                )
            patch_bytes = patch_value.encode("utf-8")
            arguments = ["apply"]
            if not entry.get("apply_during_baseline"):
                arguments.append("--check")
            arguments.extend(["--whitespace=nowarn", "-"])
            started = time.perf_counter_ns()
            result = git(
                context.work_root,
                *arguments,
                input_bytes=patch_bytes,
                check=False,
                timeout=timeout,
            )
            elapsed_ms = (time.perf_counter_ns() - started) / 1_000_000
            status = (
                "ok"
                if result.returncode == 0 and entry.get("apply_during_baseline")
                else "preview_only"
                if result.returncode == 0
                else "unexpected_exit"
            )
            record = baseline_record(
                config=config,
                counter=counter,
                sanitizer=sanitizer,
                step=index,
                executor=executor,
                input_text=patch_value,
                output_bytes=result.stdout,
                stderr_bytes=result.stderr,
                exit_code=result.returncode,
                elapsed_ms=elapsed_ms,
                status=status,
                reason=f"frozen patch sha256={entry['patch_sha256']}",
            )
            if result.returncode != 0:
                writer.write(record)
                raise RunnerError("frozen baseline patch did not apply cleanly")
        elif executor == "shell":
            if recipe.get("shell") != "powershell" or not isinstance(
                recipe.get("command"), str
            ):
                raise UnsupportedCase("unsupported baseline shell recipe")
            command = context.resolve(recipe["command"])
            if not isinstance(command, str):
                raise RunnerError("resolved baseline command is not text")
            validate_shell_recipe(command)
            started = time.perf_counter_ns()
            result = run_process(
                [
                    powershell_executable(),
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    command,
                ],
                cwd=context.work_root,
                env=safe_git_environment(),
                timeout=timeout,
                check=False,
            )
            elapsed_ms = (time.perf_counter_ns() - started) / 1_000_000
            expected_codes = recipe.get("expected_exit_codes", [0])
            status = "ok" if result.returncode in expected_codes else "unexpected_exit"
            record = baseline_record(
                config=config,
                counter=counter,
                sanitizer=sanitizer,
                step=index,
                executor=executor,
                input_text=command,
                output_bytes=result.stdout,
                stderr_bytes=result.stderr,
                exit_code=result.returncode,
                elapsed_ms=elapsed_ms,
                status=status,
            )
            if status != "ok":
                writer.write(record)
                raise RunnerError("baseline recipe returned an unexpected exit code")
        else:
            raise UnsupportedCase(f"unsupported baseline executor: {executor}")
        writer.write(record)
        if record["status"] != "not_applicable":
            totals["cl100k"] += int(record["direct_payload_cl100k"])
            totals["o200k"] += int(record["direct_payload_o200k"])
            totals["utf8_bytes"] += int(record["direct_payload_utf8_bytes"])
    capability_only = bool(case.get("baseline_recipe")) and all(
        recipe.get("executor") == "capability_only"
        for recipe in case.get("baseline_recipe", [])
    )
    writer.write(
        {
            **harness.base_record(config),
            "execution_mode": "recipe_baseline",
            "record_type": "baseline_total",
            "steps": len(case.get("baseline_recipe", [])),
            "direct_payload_cl100k": None if capability_only else totals["cl100k"],
            "direct_payload_o200k": None if capability_only else totals["o200k"],
            "direct_payload_utf8_bytes": None
            if capability_only
            else totals["utf8_bytes"],
            "economics_status": "N/A_CAPABILITY_GAIN"
            if capability_only
            else "measured_recipe_lower_bound",
            "baseline_equivalence": case.get("baseline_equivalence"),
        }
    )


def freeze_baseline_patches(
    bundle: InputBundle, output: pathlib.Path
) -> dict[str, Any]:
    output = output.resolve()
    if output.exists():
        raise RunnerError("baseline patch lock output already exists")
    if harness.is_within(output, PROJECT_ROOT):
        raise RunnerError(
            "freeze output must be external; review it before adding it to assets"
        )
    patch_cases = [
        case
        for case in bundle.cases["cases"]
        if any(
            recipe.get("executor") == "apply_patch"
            for recipe in case.get("baseline_recipe", [])
        )
    ]
    entries: dict[str, Any] = {}
    clean_repo = require_relative_path(
        bundle.fixture_root, bundle.oracle["paths"]["clean_repository"]
    )
    scratch_parent = bundle.benchmark_root / "work"
    scratch_parent.mkdir(parents=True, exist_ok=True)
    for case in patch_cases:
        entry = generate_case_patch(bundle, case)
        with tempfile.TemporaryDirectory(
            prefix="sfbench-patch-freeze-", dir=scratch_parent
        ) as scratch_text:
            scratch = pathlib.Path(scratch_text) / "repo"
            clone_native(
                clean_repo,
                scratch,
                bundle.oracle["repositories"]["clean"]["head"],
            )
            patch_bytes = entry["patch"].encode("utf-8")
            git(
                scratch,
                "apply",
                "--check",
                "--whitespace=nowarn",
                "-",
                input_bytes=patch_bytes,
            )
            git(
                scratch,
                "apply",
                "--whitespace=nowarn",
                "-",
                input_bytes=patch_bytes,
            )
            for relative, expected in entry["after_sha256"].items():
                if file_sha256(require_relative_path(scratch, relative)) != expected:
                    raise RunnerError(
                        f"frozen patch verification failed for {case['id']}"
                    )
        entries[case["id"]] = entry
    lock = {
        "schema_version": "SFBENCH-BASELINE-PATCHES-1",
        "protocol": harness.PROTOCOL_ID,
        "source_hashes": {
            "cases": bundle.hashes["cases"],
            "oracle": bundle.hashes["oracle"],
            "fixture_clean_tree": bundle.cases["oracle_contract"][
                "expected_clean_tree_sha256"
            ],
        },
        "patch_count": len(entries),
        "patches": entries,
        "visibility": "evaluator-only; never expose to paired LLM arms",
    }
    _write_json(output, lock)
    return {
        "output": str(output),
        "sha256": file_sha256(output),
        "patch_count": len(entries),
        "case_ids": sorted(entries),
    }


@dataclass
class RunSettings:
    run_id: str
    output: pathlib.Path
    work_parent: pathlib.Path
    server_path: pathlib.Path
    server_args: list[str]
    with_baseline: bool
    allow_stdio_shadow: bool
    timeout_override: float | None
    repetitions_override: int | None = None
    self_test: bool = False


def select_cases(
    manifest: dict[str, Any],
    *,
    case_ids: list[str],
    tools: list[str],
    case_kinds: list[str],
    smoke: bool,
    all_cases: bool,
) -> list[dict[str, Any]]:
    filters_present = bool(case_ids or tools or case_kinds)
    if (smoke and (all_cases or filters_present)) or (all_cases and filters_present):
        raise RunnerError("--smoke and --all cannot be combined with cohort filters")
    if not smoke and not all_cases and not filters_present:
        raise RunnerError("select --smoke, --all, or at least one cohort filter")
    rows = manifest["cases"]
    if smoke:
        selected_ids = set(SMOKE_CASE_IDS)
        selected = [case for case in rows if case["id"] in selected_ids]
    elif all_cases:
        selected = list(rows)
    else:
        selected = list(rows)
        if case_ids:
            wanted = set(case_ids)
            existing = {case["id"] for case in rows}
            if wanted - existing:
                raise RunnerError("one or more selected case IDs do not exist")
            selected = [case for case in selected if case["id"] in wanted]
        if tools:
            wanted_tools = set(tools)
            existing_tools = {case["primary_tool"] for case in rows}
            if wanted_tools - existing_tools:
                raise RunnerError("one or more selected tools have no cases")
            selected = [
                case for case in selected if case["primary_tool"] in wanted_tools
            ]
        if case_kinds:
            wanted_kinds = set(case_kinds)
            selected = [
                case for case in selected if case.get("case_kind") in wanted_kinds
            ]
    if not selected:
        raise RunnerError("case selection is empty")
    return selected


def case_profile_name(bundle: InputBundle, case: dict[str, Any]) -> str:
    profiles = bundle.cases.get("materialization_profiles", {})
    if case["id"] in set(profiles.get("native_only_case_ids", [])):
        return "public_repo_native_only"
    if case.get("preconditions", {}).get("fixture_repository") == "mutation-repo":
        return "public_repo_with_fixture_history_and_dirty_state"
    return "public_repo_with_deterministic_fixture_history_overlay"


def setup_support(
    case: dict[str, Any], baseline_patches: dict[str, Any] | None
) -> tuple[bool, list[str]]:
    unsupported: list[str] = []
    for action in case.get("preconditions", {}).get("setup", []):
        try:
            action_phase(action)
        except UnsupportedCase as exc:
            unsupported.append(str(exc))
    needs_patch = any(
        recipe.get("executor") == "apply_patch"
        for recipe in case.get("baseline_recipe", [])
    )
    frozen = (
        baseline_patches.get("patches", {}).get(case["id"])
        if baseline_patches is not None
        else None
    )
    if needs_patch and not isinstance(frozen, dict):
        unsupported.append(
            "baseline apply_patch requires an asset-locked derived patch"
        )
    return not unsupported, unsupported


def dry_run_plan(
    bundle: InputBundle,
    selected: list[dict[str, Any]],
    *,
    allow_stdio_shadow: bool,
) -> dict[str, Any]:
    plans: list[dict[str, Any]] = []
    for case in selected:
        supported, reasons = setup_support(case, bundle.baseline_patches)
        transport = case["transport"]
        effective = transport
        if transport != "stdio":
            if allow_stdio_shadow and case.get("parity_shadow_transport") == "stdio":
                effective = "stdio_shadow"
            else:
                supported = False
                reasons.append(
                    "primary HTTP transport is unsupported by the direct stdio runner"
                )
        plans.append(
            {
                "id": case["id"],
                "primary_tool": case["primary_tool"],
                "repo": case["repo"],
                "language": case["language"],
                "surface": case["surface"],
                "cache_state": case["cache_state"],
                "transport": transport,
                "effective_transport": effective,
                "materialization_profile": case_profile_name(bundle, case),
                "request_count": len(case["requests"]),
                "baseline_steps": len(case.get("baseline_recipe", [])),
                "executable": supported,
                "unsupported_reasons": reasons,
            }
        )
    return {
        "protocol": harness.PROTOCOL_ID,
        "input_hashes": bundle.hashes,
        "asset_lock": bundle.asset_lock_status,
        "selected_cases": plans,
        "executable_count": sum(1 for plan in plans if plan["executable"]),
        "unsupported_count": sum(1 for plan in plans if not plan["executable"]),
    }


def _write_setup_record(
    writer: harness.JsonlWriter,
    config: harness.CaptureConfig,
    case_id: str,
    phase: str,
    result: dict[str, Any],
) -> None:
    writer.write(
        {
            **harness.base_record(config),
            "record_type": "setup_action",
            "case_id": case_id,
            "phase": phase,
            "result": result,
            "result_sha256": harness.sha256_text(harness.canonical_json(result)),
            "hidden_from_llm_arms": True,
        }
    )


def record_response_binding(
    context: ResolutionContext,
    request: dict[str, Any],
    capture: harness.RpcCapture,
) -> None:
    record_as = request.get("record_as")
    if not isinstance(record_as, str) or not record_as:
        return
    result = (capture.response_safe or {}).get("result")
    value: dict[str, Any] = {
        "response": result,
        "response_sha256": harness.sha256_text(harness.canonical_json(result)),
    }
    text = harness.extract_content_text(capture.response_safe)
    handles = CCR_HANDLE.findall(text)
    if handles:
        unique = sorted(set(handles))
        if len(unique) != 1 or len(handles) != 1:
            raise RunnerError(
                "CCR-producing response did not contain exactly one handle"
            )
        value["hash"] = unique[0]
        value["summary_payload_sha256"] = harness.sha256_text(text)
        value["precompression_payload_sha256"] = None
        value["precompression_hash_status"] = "pending measured retrieve response"
    context.bind(f"prior.{context.case['id']}.{record_as}", value)


def request_economics_role(case: dict[str, Any], request: dict[str, Any]) -> str:
    explicit = request.get("economics_role")
    if explicit is not None:
        if explicit not in ECONOMICS_ROLES:
            raise RunnerError("request has an invalid economics_role")
        return str(explicit)
    label = str(request.get("label", "")).casefold()
    if "estimate" in label or request.get("args", {}).get("estimate") is True:
        return "estimate_diagnostic"
    if label in {"replay", "repeat_replay"}:
        return "replay_diagnostic"
    if case.get("primary_tool") == "symforge_retrieve" and label == "repeat":
        return "replay_diagnostic"
    if (
        case.get("primary_tool") == "investigation_suggest"
        and request.get("tool") == "get_symbol"
    ):
        return "required_prerequisite"
    if case.get("primary_tool") == "health_compact" and request.get("tool") == "health":
        return "oracle_reference"
    return "task"


def pending_ccr_bindings(
    value: Any, prefix: str = ""
) -> list[tuple[str, dict[str, Any]]]:
    pending: list[tuple[str, dict[str, Any]]] = []
    if isinstance(value, dict):
        if (
            isinstance(value.get("hash"), str)
            and value.get("precompression_payload_sha256") is None
        ):
            pending.append((prefix, value))
        for key, child in value.items():
            child_prefix = f"{prefix}.{key}" if prefix else key
            pending.extend(pending_ccr_bindings(child, child_prefix))
    elif isinstance(value, list):
        for index, child in enumerate(value):
            pending.extend(pending_ccr_bindings(child, f"{prefix}[{index}]"))
    return pending


def update_ccr_from_retrieve(
    context: ResolutionContext,
    handle: str,
    capture: harness.RpcCapture,
) -> list[str]:
    if harness.rpc_status(capture) != "ok":
        return []
    payload = harness.extract_content_text(capture.response_safe)
    if not payload:
        return []
    updated: list[str] = []
    payload_hash = harness.sha256_text(payload)
    for path, binding in pending_ccr_bindings(context.prior):
        if binding.get("hash") == handle:
            binding["precompression_payload_sha256"] = payload_hash
            binding["precompression_hash_status"] = "measured retrieve response"
            updated.append(path)
    return updated


def finalize_pending_ccr(
    context: ResolutionContext,
    sessions: SessionManager,
    writer: harness.JsonlWriter,
    config: harness.CaptureConfig,
) -> None:
    for path, binding in list(pending_ccr_bindings(context.prior)):
        handle = binding["hash"]
        capture = sessions.call(
            "symforge_retrieve",
            {"hash": handle},
            record_type="evaluator_rpc",
            extra={
                "economics_excluded": True,
                "evaluator_purpose": "retain_precompression_payload_hash",
            },
        )
        updated = update_ccr_from_retrieve(context, handle, capture)
        if not updated:
            raise RunnerError(
                "evaluator could not retain the CCR precompression payload hash"
            )
        writer.write(
            {
                **harness.base_record(config),
                "record_type": "ccr_binding_finalized",
                "binding": path,
                "handle": handle,
                "precompression_payload_sha256": binding[
                    "precompression_payload_sha256"
                ],
                "economics_excluded": True,
            }
        )


def resolved_allowlist(context: ResolutionContext) -> list[str]:
    value = context.resolve(context.case.get("mutation_allowlist", []))
    if isinstance(value, str):
        return [value.replace("\\", "/")]
    if not isinstance(value, list) or any(not isinstance(item, str) for item in value):
        raise RunnerError("resolved mutation_allowlist is not a string list")
    return [item.replace("\\", "/") for item in value]


def step_target_fingerprints(
    context: ResolutionContext, arguments: dict[str, Any]
) -> dict[str, Any]:
    candidates: set[str] = set()
    for value in resolved_allowlist(context):
        if not any(character in value for character in "*?["):
            candidates.add(value)
    path_argument = arguments.get("path")
    if isinstance(path_argument, str) and not pathlib.Path(path_argument).is_absolute():
        candidates.add(path_argument.replace("\\", "/"))
    result: dict[str, Any] = {}
    for relative in sorted(candidates):
        try:
            path = require_relative_path(context.work_root, relative)
        except RunnerError:
            continue
        if path.is_file():
            result[relative] = {
                "present": True,
                "bytes": path.stat().st_size,
                "sha256": file_sha256(path),
            }
        else:
            result[relative] = {"present": False}
    return result


def case_exposes_fixture_root(case: dict[str, Any]) -> bool:
    measured = {
        "requests": case.get("requests", []),
        "baseline_recipe": case.get("baseline_recipe", []),
    }
    return "${fixture.root}" in harness.canonical_json(measured)


def materialize_fixture_auxiliary(
    bundle: InputBundle, case: dict[str, Any], destination: pathlib.Path
) -> pathlib.Path | None:
    if not case_exposes_fixture_root(case):
        return None
    if destination.exists():
        raise RunnerError("fixture auxiliary root already exists")
    non_git = bundle.oracle.get("paths", {}).get("non_git_directory")
    if not isinstance(non_git, str):
        raise RunnerError("fixture oracle omits its non-Git directory")
    source = require_relative_path(bundle.fixture_root, non_git)
    destination.mkdir(parents=True)
    shutil.copytree(source, require_relative_path(destination, non_git))
    return destination


def execute_case_setup_without_session(
    case: dict[str, Any],
    context: ResolutionContext,
    writer: harness.JsonlWriter,
    config: harness.CaptureConfig,
) -> dict[str, list[dict[str, Any]]]:
    hooks: dict[str, list[dict[str, Any]]] = {}
    for action in case.get("preconditions", {}).get("setup", []):
        if action_phase(action) not in {"pre_index", "post_index"}:
            raise UnsupportedCase("unknown setup phase")
        result = execute_setup_action(action, context, hooks)
        _write_setup_record(
            writer, config, case["id"], "baseline_pre_measurement", result
        )
    if hooks:
        raise UnsupportedCase(
            "baseline parity for between-request setup hooks is not implemented"
        )
    return hooks


def run_one_case(
    bundle: InputBundle,
    case: dict[str, Any],
    settings: RunSettings,
    writer: harness.JsonlWriter,
    sanitizer: harness.Sanitizer,
    counter: harness.TokenCounter,
    trial_ordinal: int,
    measured_repetition: int | None,
    discarded_warmup: bool,
) -> None:
    if case["transport"] != "stdio" and not (
        settings.allow_stdio_shadow and case.get("parity_shadow_transport") == "stdio"
    ):
        raise UnsupportedCase(
            "primary HTTP transport is unsupported; explicitly enable its stdio parity shadow"
        )
    supported, reasons = setup_support(case, bundle.baseline_patches)
    if not supported:
        relevant = [
            reason
            for reason in reasons
            if not reason.startswith("baseline apply_patch")
        ]
        if relevant or settings.with_baseline:
            raise UnsupportedCase("; ".join(relevant or reasons))

    base_safe_id = re.sub(r"[^A-Za-z0-9_.-]", "_", case["id"])
    suffix = (
        f"w{trial_ordinal:03d}"
        if discarded_warmup
        else f"r{int(measured_repetition or 0):03d}"
    )
    safe_id = f"{base_safe_id}-{suffix}"
    work_root = (settings.work_parent / safe_id).resolve()
    if not harness.is_within(work_root, settings.work_parent.resolve()):
        raise RunnerError("case work root escaped the run work directory")
    if work_root.exists():
        raise RunnerError("case work root already exists; use a fresh run ID")
    materialization = materialize_case(bundle, case, work_root)
    fixture_auxiliary = materialize_fixture_auxiliary(
        bundle, case, settings.work_parent / f"{safe_id}-fixture-aux"
    )
    harness.validate_paths(work_root, settings.output)
    context = ResolutionContext(
        bundle=bundle,
        case=case,
        work_root=work_root,
        run_id=settings.run_id,
        project_id=project_id_for(work_root),
        fixture_runtime_root=fixture_auxiliary,
    )
    context.native_oracles = derive_native_oracles(case["repo"], work_root)
    timeout = settings.timeout_override or float(
        case.get("limits", {}).get("timeout_seconds", 120)
    )
    base_config = harness.CaptureConfig(
        mode="scenario",
        repo=work_root,
        output=settings.output,
        server=str(settings.server_path),
        server_args=list(settings.server_args),
        surface=case["surface"],
        auto_index=True,
        timeout=timeout,
        protocol_version=harness.MCP_PROTOCOL_VERSION,
        run_id=settings.run_id,
        case_id=case["id"],
        append=True,
    )
    writer.write(
        {
            **harness.base_record(base_config),
            "record_type": "case_start",
            "primary_tool": case["primary_tool"],
            "language": case["language"],
            "repo_stratum": case["repo_stratum"],
            "cache_state": case["cache_state"],
            "declared_transport": case["transport"],
            "effective_transport": "stdio"
            if case["transport"] == "stdio"
            else "stdio_shadow",
            "economics_mode": "formal_with_baseline"
            if settings.with_baseline
            else "diagnostic_direct_only",
            "materialization": materialization,
            "project_id_derivation": "project- + sha256(canonical absolute root)",
            "project_id": context.project_id,
            "trial_ordinal": trial_ordinal,
            "measured_repetition": measured_repetition,
            "discarded_warmup": discarded_warmup,
            "economics_excluded": discarded_warmup,
        }
    )

    setup = case.get("preconditions", {}).get("setup", [])
    hooks: dict[str, list[dict[str, Any]]] = {}
    for action in setup:
        if action_phase(action) == "pre_index":
            result = execute_setup_action(action, context, hooks)
            _write_setup_record(writer, base_config, case["id"], "pre_index", result)

    pre_session_inventory = source_inventory(work_root)
    git_state_before = git_state_inventory(work_root)
    auxiliary_before = (
        source_inventory(fixture_auxiliary) if fixture_auxiliary is not None else None
    )
    writer.write(
        {
            **harness.base_record(base_config),
            "record_type": "source_inventory_pre_session",
            "inventory": pre_session_inventory,
            "git_state": git_state_before,
            "fixture_auxiliary_inventory": auxiliary_before,
        }
    )

    inventory = bundle.cases["inventory"]
    expected_tools = {
        "full": set(inventory["full_surface_tools"]),
        "compact": set(inventory["compact_surface_tools"]),
        "meta": set(inventory["meta_surface_tools"]),
    }
    sessions = SessionManager(
        server_path=settings.server_path,
        server_args=settings.server_args,
        work_root=work_root,
        writer=writer,
        sanitizer=sanitizer,
        counter=counter,
        run_id=settings.run_id,
        case_id=case["id"],
        timeout=timeout,
        expected_tools=expected_tools,
    )
    try:
        cache_state = case["cache_state"]
        if cache_state == "snapshot_warm":
            sessions.start("full", "snapshot_preparation")
            sessions.wait_ready("full", timeout=timeout)
            checkpoint = sessions.call(
                "checkpoint_now",
                {"verify_after_write": True, "export_artifact": False},
                record_type="preflight_rpc",
                extra={
                    "economics_excluded": True,
                    "preflight_purpose": "snapshot_preparation",
                },
            )
            if harness.rpc_status(checkpoint) != "ok":
                raise RunnerError("snapshot preparation checkpoint failed")
            sessions.close()
            sessions.start(case["surface"], "measured")
            sessions.wait_ready(case["surface"], timeout=timeout)
        elif cache_state == "process_warm":
            sessions.start(case["surface"], "measured")
            sessions.wait_ready(case["surface"], timeout=timeout)
        elif cache_state == "cold_build":
            sessions.start(case["surface"], "measured")
        else:
            raise RunnerError("unknown cache_state")

        post_actions = [
            action for action in setup if action_phase(action) == "post_index"
        ]
        if post_actions:
            sessions.wait_ready(case["surface"], timeout=timeout)
        for action in post_actions:
            result = execute_setup_action(action, context, hooks)
            _write_setup_record(writer, base_config, case["id"], "post_index", result)

        before = source_inventory(work_root) if post_actions else pre_session_inventory
        writer.write(
            {
                **harness.base_record(base_config),
                "record_type": "source_inventory_before",
                "inventory": before,
            }
        )
        calls = 0
        effective_cache_state = {
            "cold_build": "cold_build_process",
            "snapshot_warm": "snapshot_warm_ready",
            "process_warm": "process_warm_ready_after_readiness_probe",
        }[case["cache_state"]]
        for request in case["requests"]:
            if request.get("fresh_process_before") or request.get(
                "fresh_session_before"
            ):
                sessions.close()
                snapshot = work_root / ".symforge" / "index.bin"
                snapshot_evidence = {
                    "present": snapshot.is_file(),
                    "sha256": file_sha256(snapshot) if snapshot.is_file() else None,
                    "bytes": snapshot.stat().st_size if snapshot.is_file() else None,
                }
                writer.write(
                    {
                        **harness.base_record(base_config),
                        "record_type": "restart_boundary",
                        "before_request_label": request["label"],
                        "snapshot": snapshot_evidence,
                    }
                )
                role = (
                    "fresh_session"
                    if request.get("fresh_session_before")
                    else "fresh_process"
                )
                sessions.start(case["surface"], role)
                effective_cache_state = (
                    "fresh_process_snapshot_present_unprobed"
                    if snapshot_evidence["present"]
                    else "fresh_process_no_snapshot_unprobed"
                )
            arguments = context.resolve(request["args"])
            if not isinstance(arguments, dict):
                raise RunnerError("resolved request args are not an object")
            calls += 1
            economics_role = request_economics_role(case, request)
            target_before = step_target_fingerprints(context, arguments)
            capture = sessions.call(
                request["tool"],
                arguments,
                record_type="direct_trial",
                extra={
                    "case_step": request["step"],
                    "request_label": request["label"],
                    "cohort": request["label"],
                    "step_role": economics_role,
                    "economics_role": economics_role,
                    "expected_status": "ok"
                    if case.get("case_kind") == "happy"
                    else "oracle_specific",
                    "cache_state": case["cache_state"],
                    "effective_cache_state": effective_cache_state,
                    "fresh_process_before": bool(request.get("fresh_process_before")),
                    "fresh_session_before": bool(request.get("fresh_session_before")),
                    "economics_excluded": discarded_warmup,
                    "trial_ordinal": trial_ordinal,
                    "measured_repetition": measured_repetition,
                    "discarded_warmup": discarded_warmup,
                },
            )
            raw_response_text = (
                harness.canonical_json(capture.response_raw)
                if capture.response_raw is not None
                else ""
            )
            raw_content = harness.extract_content(capture.response_raw)
            raw_content_text = (
                harness.canonical_json(raw_content) if raw_content is not None else ""
            )
            writer.write(
                {
                    **harness.base_record(base_config),
                    "record_type": "trial_raw_fingerprints",
                    "case_step": request["step"],
                    "request_label": request["label"],
                    "trial_ordinal": trial_ordinal,
                    "measured_repetition": measured_repetition,
                    "raw_response_sha256": harness.sha256_text(raw_response_text),
                    "raw_content_sha256": harness.sha256_text(raw_content_text),
                    "raw_values_persisted": False,
                }
            )
            target_after = step_target_fingerprints(context, arguments)
            changed_targets = sorted(
                path
                for path in target_before.keys() | target_after.keys()
                if target_before.get(path) != target_after.get(path)
            )
            writer.write(
                {
                    **harness.base_record(base_config),
                    "record_type": "step_source_fingerprints",
                    "case_step": request["step"],
                    "request_label": request["label"],
                    "trial_ordinal": trial_ordinal,
                    "measured_repetition": measured_repetition,
                    "before": target_before,
                    "after": target_after,
                    "changed_targets": changed_targets,
                    "fingerprint_scope": "resolved mutation allowlist plus request path",
                }
            )
            if case.get("case_kind") == "happy" and harness.rpc_status(capture) != "ok":
                raise RunnerError("happy-path request did not return MCP status ok")
            record_response_binding(context, request, capture)
            if request["tool"] == "symforge_retrieve" and isinstance(
                arguments.get("hash"), str
            ):
                updated = update_ccr_from_retrieve(context, arguments["hash"], capture)
                if updated:
                    payload_hash = harness.sha256_text(
                        harness.extract_content_text(capture.response_safe)
                    )
                    for binding_path in updated:
                        writer.write(
                            {
                                **harness.base_record(base_config),
                                "record_type": "ccr_binding_finalized",
                                "binding": binding_path,
                                "handle": arguments["hash"],
                                "precompression_payload_sha256": payload_hash,
                                "economics_excluded": False,
                                "source": "measured_retrieve",
                            }
                        )
            for hook in hooks.get(request["label"], []):
                result = execute_hook(hook, context)
                _write_setup_record(
                    writer,
                    base_config,
                    case["id"],
                    f"after_request:{request['label']}",
                    result,
                )
        if calls > int(case["limits"]["call_limit"]):
            raise RunnerError("runtime call count exceeded the frozen case limit")
        if pending_ccr_bindings(context.prior):
            finalize_pending_ccr(context, sessions, writer, base_config)
        after = source_inventory(work_root)
        changes = inventory_changes(before, after)
        policy = enforce_mutation_policy(case, resolved_allowlist(context), changes)
        auxiliary_after = (
            source_inventory(fixture_auxiliary)
            if fixture_auxiliary is not None
            else None
        )
        auxiliary_changes = (
            inventory_changes(auxiliary_before, auxiliary_after)
            if auxiliary_before is not None and auxiliary_after is not None
            else []
        )
        auxiliary_violations = [
            change["path"]
            for change in auxiliary_changes
            if not fnmatch.fnmatchcase(change["path"], "**/.symforge/**")
            and not fnmatch.fnmatchcase(change["path"], ".symforge/**")
        ]
        git_state_after = git_state_inventory(work_root)
        git_state_violations = protected_git_state_changed(
            git_state_before, git_state_after
        )
        writer.write(
            {
                **harness.base_record(base_config),
                "record_type": "source_inventory_after",
                "inventory": after,
                "changes": changes,
                "mutation_policy": policy,
                "fixture_auxiliary_after": auxiliary_after,
                "fixture_auxiliary_changes": auxiliary_changes,
                "fixture_auxiliary_violations": auxiliary_violations,
                "git_state_before": git_state_before,
                "git_state_after": git_state_after,
                "git_state_violations": git_state_violations,
            }
        )
        if not policy["safe"]:
            raise RunnerError("source mutation escaped the frozen allowlist")
        if auxiliary_violations:
            raise RunnerError(
                "measured tool mutated a disposable fixture auxiliary source path"
            )
        if git_state_violations:
            raise RunnerError("measured tool mutated protected Git metadata")
    finally:
        sessions.close()

    if settings.with_baseline and not discarded_warmup:
        baseline_root = (settings.work_parent / f"{safe_id}-baseline").resolve()
        if baseline_root.exists():
            raise RunnerError("baseline work root already exists")
        baseline_materialization = materialize_case(bundle, case, baseline_root)
        baseline_auxiliary = materialize_fixture_auxiliary(
            bundle, case, settings.work_parent / f"{safe_id}-baseline-fixture-aux"
        )
        harness.validate_paths(baseline_root, settings.output)
        baseline_context = ResolutionContext(
            bundle=bundle,
            case=case,
            work_root=baseline_root,
            run_id=settings.run_id,
            project_id=project_id_for(baseline_root),
            baseline_relative_paths=True,
            fixture_runtime_root=baseline_auxiliary,
        )
        baseline_context.native_oracles = derive_native_oracles(
            case["repo"], baseline_root
        )
        baseline_config = harness.CaptureConfig(
            mode="baseline",
            repo=baseline_root,
            output=settings.output,
            server="",
            server_args=[],
            surface=case["surface"],
            auto_index=False,
            timeout=timeout,
            protocol_version=harness.MCP_PROTOCOL_VERSION,
            run_id=settings.run_id,
            case_id=case["id"],
            append=True,
        )
        writer.write(
            {
                **harness.base_record(baseline_config),
                "execution_mode": "recipe_baseline",
                "record_type": "baseline_case_start",
                "materialization": baseline_materialization,
                "cwd_policy": "commands execute in baseline clone; ${case.work_root} resolves to '.'",
                "cwd_canonical_sha256": harness.sha256_text(
                    canonical_path(baseline_root)
                ),
            }
        )
        execute_case_setup_without_session(
            case, baseline_context, writer, baseline_config
        )
        baseline_before = source_inventory(baseline_root)
        baseline_auxiliary_before = (
            source_inventory(baseline_auxiliary)
            if baseline_auxiliary is not None
            else None
        )
        run_baseline(
            case=case,
            context=baseline_context,
            writer=writer,
            counter=counter,
            sanitizer=sanitizer,
            timeout=timeout,
        )
        baseline_after = source_inventory(baseline_root)
        baseline_auxiliary_after = (
            source_inventory(baseline_auxiliary)
            if baseline_auxiliary is not None
            else None
        )
        baseline_changes = inventory_changes(baseline_before, baseline_after)
        baseline_policy = enforce_mutation_policy(
            case, resolved_allowlist(baseline_context), baseline_changes
        )
        writer.write(
            {
                **harness.base_record(baseline_config),
                "execution_mode": "recipe_baseline",
                "record_type": "baseline_source_inventory_after",
                "before": baseline_before,
                "after": baseline_after,
                "changes": baseline_changes,
                "mutation_policy": baseline_policy,
                "fixture_auxiliary_before": baseline_auxiliary_before,
                "fixture_auxiliary_after": baseline_auxiliary_after,
                "fixture_auxiliary_changes": inventory_changes(
                    baseline_auxiliary_before, baseline_auxiliary_after
                )
                if baseline_auxiliary_before is not None
                and baseline_auxiliary_after is not None
                else [],
            }
        )
        if not baseline_policy["safe"]:
            raise RunnerError("baseline source mutation escaped the frozen allowlist")

    writer.write(
        {
            **harness.base_record(base_config),
            "record_type": "case_complete",
            "status": "completed",
            "trial_ordinal": trial_ordinal,
            "measured_repetition": measured_repetition,
            "discarded_warmup": discarded_warmup,
            "economics_status": "discarded_warmup"
            if discarded_warmup
            else "baseline_captured"
            if settings.with_baseline
            else "not_formal_baseline_omitted",
            "correctness_status": "pending_machine_or_report_evaluation",
            "oracle_status": "unevaluated",
            "economics_valid": False,
            "correctness_oracle": case.get("correctness_oracle"),
            "stop_condition": case.get("stop_condition"),
        }
    )


def run_campaign(
    bundle: InputBundle,
    selected: list[dict[str, Any]],
    settings: RunSettings,
    sut: dict[str, Any],
) -> int:
    settings.output = settings.output.resolve()
    settings.work_parent = settings.work_parent.resolve()
    approved_runs = (bundle.benchmark_root / "runs").resolve()
    approved_work = (bundle.benchmark_root / "work").resolve()
    if not harness.is_within(settings.output, approved_runs):
        raise RunnerError("campaign output must be under benchmark_root/runs")
    if not harness.is_within(settings.work_parent, approved_work):
        raise RunnerError("campaign work roots must be under benchmark_root/work")
    forbidden_roots = [
        PROJECT_ROOT.resolve(),
        (bundle.benchmark_root / "sources").resolve(),
        bundle.fixture_root.resolve(),
        *[entry["root"].resolve() for entry in bundle.repositories.values()],
    ]
    if any(harness.is_within(settings.output, root) for root in forbidden_roots):
        raise RunnerError("campaign output overlaps an immutable or project root")
    if any(harness.is_within(settings.work_parent, root) for root in forbidden_roots):
        raise RunnerError("campaign work root overlaps an immutable or project root")
    settings.work_parent.mkdir(parents=True, exist_ok=False)
    sanitizer = harness.Sanitizer()
    counter = harness.TokenCounter()
    writer = harness.JsonlWriter(settings.output, sanitizer, append=False)
    failures = 0
    completed_trials = 0
    scheduled_trials = sum(
        (
            settings.repetitions_override
            if settings.repetitions_override is not None
            else int(case.get("limits", {}).get("repetitions", 1))
        )
        + int(case.get("limits", {}).get("discarded_warmups", 0))
        for case in selected
    )
    try:
        writer.write(
            {
                "protocol": harness.PROTOCOL_ID,
                "run_id": settings.run_id,
                "record_type": "campaign_start",
                "execution_mode": "direct_rpc",
                "transport": "stdio",
                "input_hashes": bundle.hashes,
                "asset_lock": bundle.asset_lock_status,
                "sut": sut,
                "tokenizer": counter.metadata(),
                "python": platform.python_version(),
                "platform": platform.platform(),
                "selected_case_ids": [case["id"] for case in selected],
                "economics_mode": "formal_with_baseline"
                if settings.with_baseline
                else "diagnostic_direct_only",
                "formal_economics": False,
                "oracle_status": "unevaluated",
                "economics_valid": False,
                "validity_gate": "external oracle adjudication required",
                "repetitions_policy": "frozen_per_case"
                if settings.repetitions_override is None
                else "diagnostic_override",
                "repetitions_override": settings.repetitions_override,
                "scheduled_trials": scheduled_trials,
                "paid_api_calls": 0,
            }
        )
        for case in selected:
            warmups = int(case.get("limits", {}).get("discarded_warmups", 0))
            repetitions = (
                settings.repetitions_override
                if settings.repetitions_override is not None
                else int(case.get("limits", {}).get("repetitions", 1))
            )
            for ordinal in range(1, warmups + repetitions + 1):
                discarded = ordinal <= warmups
                measured_repetition = None if discarded else ordinal - warmups
                try:
                    run_one_case(
                        bundle,
                        case,
                        settings,
                        writer,
                        sanitizer,
                        counter,
                        trial_ordinal=ordinal,
                        measured_repetition=measured_repetition,
                        discarded_warmup=discarded,
                    )
                    completed_trials += 1
                except Exception as exc:
                    failures += 1
                    safe_message = (
                        str(exc) if isinstance(exc, RunnerError) else type(exc).__name__
                    )
                    writer.write(
                        {
                            "protocol": harness.PROTOCOL_ID,
                            "run_id": settings.run_id,
                            "case_id": case.get("id"),
                            "record_type": "case_error",
                            "trial_ordinal": ordinal,
                            "measured_repetition": measured_repetition,
                            "discarded_warmup": discarded,
                            "error_type": type(exc).__name__,
                            "message": safe_message,
                        }
                    )
        writer.write(
            {
                "protocol": harness.PROTOCOL_ID,
                "run_id": settings.run_id,
                "record_type": "campaign_complete",
                "selected_cases": len(selected),
                "scheduled_trials": scheduled_trials,
                "completed_trials": completed_trials,
                "failed": failures,
                "sanitizer_redactions": sanitizer.event_count(),
                "paid_api_calls": 0,
            }
        )
    finally:
        writer.close()
    return failures


def _write_json(path: pathlib.Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
        newline="\n",
    )


def _init_test_repo(path: pathlib.Path, files: dict[str, bytes]) -> str:
    path.mkdir(parents=True)
    git(path, "init")
    git(path, "config", "user.name", "SFBENCH Test")
    git(path, "config", "user.email", "sfbench@example.invalid")
    for relative, data in files.items():
        target = require_relative_path(path, relative)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    git(path, "add", "-A")
    git(path, "commit", "--no-gpg-sign", "-m", "fixture")
    return git_text(path, "rev-parse", "HEAD")


def self_test() -> int:
    if CCR_HANDLE.findall('retrieve with hash="012345abcdef"') != ["012345abcdef"]:
        raise RunnerError("self-test CCR footer parser invariant failed")
    with tempfile.TemporaryDirectory(prefix="sfbench-direct-runner-") as root_text:
        root = pathlib.Path(root_text)
        benchmark = root / "benchmark"
        source = benchmark / "sources" / "tiny"
        source_head = _init_test_repo(
            source,
            {"index.py": b"def native_anchor():\n    return 31\n"},
        )
        fixture_root = benchmark / "fixtures" / "control-v1"
        clean_repo = fixture_root / "control-repo"
        clean_head = _init_test_repo(clean_repo, {"fixture.txt": b"fixture\n"})
        clean_tree = fixture_tree_hash(clean_repo)
        mutation_repo = fixture_root / "mutation-repo"
        mutation_head = _init_test_repo(mutation_repo, {"fixture.txt": b"fixture\n"})
        mutation_tree = fixture_tree_hash(mutation_repo)
        oracle = {
            "paths": {
                "clean_repository": "control-repo",
                "mutation_repository": "mutation-repo",
            },
            "repositories": {
                "clean": {
                    "head": clean_head,
                    "tree_sha256": clean_tree,
                },
                "mutation": {
                    "head": mutation_head,
                    "tree_sha256": mutation_tree,
                },
            },
            "worktree": {
                "status_porcelain_v1": [],
                "staged": {"diff_sha256": sha256_bytes(b"")},
                "unstaged": {"diff_sha256": sha256_bytes(b"")},
            },
            "history": {"commits": [], "refs": {}},
            "files": {},
            "symbols": {},
            "mutations": {},
        }
        oracle_path = fixture_root / "oracle.json"
        _write_json(oracle_path, oracle)
        corpus_lock = {
            "version": 1,
            "repositories": [
                {
                    "alias": "tiny",
                    "url": "local-only",
                    "commit": source_head,
                    "history_depth": 1,
                    "stratum": "tiny",
                    "primary_language": "Python",
                    "overlay_language": "Python",
                }
            ],
        }
        corpus_lock_path = root / "corpus.lock.json"
        _write_json(corpus_lock_path, corpus_lock)
        corpus_manifest = {
            "version": 1,
            "corpus_lock_sha256": file_sha256(corpus_lock_path),
            "root": str(benchmark / "sources"),
            "repositories": [{"alias": "tiny", "commit": source_head}],
        }
        _write_json(benchmark / "corpus-manifest.json", corpus_manifest)
        token_metadata = harness.TokenCounter().metadata()
        primary = token_metadata["encodings"]["cl100k"]
        sensitivity = token_metadata["encodings"]["o200k"]
        campaign = {
            "schema_version": "self-test",
            "protocol_id": harness.PROTOCOL_ID,
            "system_under_test": {
                "version": "self-test",
                "binary_sha256": harness.executable_sha256(
                    pathlib.Path(sys.executable)
                ),
                "surface_modes": ["full", "compact", "meta"],
            },
            "mcp": {
                "protocol_version": harness.MCP_PROTOCOL_VERSION,
                "primary_transport": "stdio",
            },
            "tokenization": {
                "package": "tiktoken",
                "version": token_metadata["version"],
                "primary": {
                    "encoding": primary["name"],
                    "vocabulary_sha256": primary["vocabulary_sha256"],
                },
                "sensitivity": {
                    "encoding": sensitivity["name"],
                    "vocabulary_sha256": sensitivity["vocabulary_sha256"],
                },
            },
            "paired_llm": {"model_alias": "none"},
            "safety": {
                "persist_unsanitized_output": False,
                "source_mirrors_are_immutable": True,
                "ordinary_cases_use_independent_clones": True,
                "oracle_visible_to_llm": False,
                "network_during_measurement": False,
            },
        }
        campaign_path = root / "campaign.json"
        _write_json(campaign_path, campaign)
        sensitive_input = "request" + "-sensitive" + "-fixture"
        case_id = "SELF-health_compact-001"
        fake_full_tools = sorted(harness.EXPECTED_TOOLS["full"])
        fake_compact_tools = sorted(harness.EXPECTED_TOOLS["compact"])
        fake_meta_tools = sorted(harness.EXPECTED_TOOLS["meta"])
        cases = {
            "schema_version": "self-test",
            "protocol": harness.PROTOCOL_ID,
            "system_under_test": "self-test",
            "oracle_contract": {
                "expected_oracle_sha256": file_sha256(oracle_path),
                "expected_clean_head": clean_head,
                "expected_clean_tree_sha256": clean_tree,
                "forbid_symforge_derived_oracles": True,
            },
            "inventory": {
                "full_surface_tools": fake_full_tools,
                "full_surface_count": len(fake_full_tools),
                "compact_surface_tools": fake_compact_tools,
                "meta_surface_tools": fake_meta_tools,
                "unique_tool_names": len(
                    set(fake_full_tools + fake_compact_tools + fake_meta_tools)
                ),
            },
            "materialization_profiles": {
                "native_only_case_ids": [case_id],
            },
            "cases": [
                {
                    "id": case_id,
                    "primary_tool": "health_compact",
                    "case_kind": "happy",
                    "repo": "tiny",
                    "repo_stratum": "tiny",
                    "commit": "${repo.tiny.commit}",
                    "language": "Python",
                    "preconditions": {
                        "fixture_repository": "control-repo",
                        "setup": [],
                    },
                    "cache_state": "process_warm",
                    "transport": "stdio",
                    "surface": "full",
                    "requests": [
                        {
                            "step": 1,
                            "label": "first",
                            "tool": "health_compact",
                            "economics_role": "task",
                            "args": {"password": sensitive_input},
                            "fresh_process_before": False,
                            "fresh_session_before": False,
                            "record_as": None,
                        },
                        {
                            "step": 2,
                            "label": "restart",
                            "tool": "health_compact",
                            "economics_role": "replay_diagnostic",
                            "args": {"project": "${session.project_id}"},
                            "fresh_process_before": True,
                            "fresh_session_before": False,
                            "record_as": None,
                        },
                    ],
                    "baseline_recipe": [
                        {
                            "executor": "capability_only",
                            "reason": "self-test capability",
                        }
                    ],
                    "baseline_equivalence": "not_applicable",
                    "correctness_oracle": {
                        "key": "self_test.fake_success",
                        "expected_outcome": "two successful sanitized fake calls",
                    },
                    "stop_condition": "both requests captured and source unchanged",
                    "limits": {"call_limit": 2, "timeout_seconds": 10},
                    "mutation_allowlist": [],
                    "source_hash_policy": "no_source_bytes_may_change",
                }
            ],
        }
        cases_path = root / "cases.json"
        _write_json(cases_path, cases)
        bundle = validate_inputs(
            benchmark_root=benchmark,
            fixture_root=fixture_root,
            cases_path=cases_path,
            campaign_path=campaign_path,
            corpus_lock_path=corpus_lock_path,
            asset_lock_path=None,
            baseline_patches_path=None,
            require_asset_lock=False,
        )
        server_path, sut = validate_sut(bundle, sys.executable, skip_version_probe=True)
        output = benchmark / "runs" / "self-test.jsonl"
        settings = RunSettings(
            run_id="self-test",
            output=output,
            work_parent=benchmark / "work" / "self-test",
            server_path=server_path,
            server_args=[str(SCRIPT_DIR / "mcp_harness.py"), "--_fake-server"],
            with_baseline=True,
            allow_stdio_shadow=False,
            timeout_override=10,
            self_test=True,
        )
        failures = run_campaign(bundle, bundle.cases["cases"], settings, sut)
        if failures:
            rows = [
                json.loads(line)
                for line in output.read_text(encoding="utf-8").splitlines()
            ]
            errors = [row for row in rows if row.get("record_type") == "case_error"]
            detail = errors[0].get("message", "unknown") if errors else "unknown"
            raise RunnerError(f"self-test campaign reported a failed case: {detail}")
        persisted = output.read_text(encoding="utf-8")
        forbidden = {
            sensitive_input,
            "unit" + "-sensitive" + "-fixture",
        }
        if any(value in persisted for value in forbidden):
            raise RunnerError("self-test sanitizer invariant failed")
        rows = [json.loads(line) for line in persisted.splitlines()]
        if sum(row.get("record_type") == "direct_trial" for row in rows) != 2:
            raise RunnerError("self-test did not capture both measured requests")
        if sum(row.get("record_type") == "process_lifecycle" for row in rows) < 2:
            raise RunnerError("self-test did not exercise fresh_process_before")
        if not any(row.get("record_type") == "baseline_total" for row in rows):
            raise RunnerError("self-test did not capture baseline accounting")
        if harness.REDACTED not in persisted:
            raise RunnerError("self-test artifact contains no sanitizer evidence")
    print("direct-case-runner self-test: PASS")
    return 0


def add_input_arguments(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--benchmark-root", default=str(DEFAULT_BENCHMARK_ROOT))
    parser.add_argument("--fixture-root")
    parser.add_argument("--cases", default=str(DEFAULT_CASES))
    parser.add_argument("--campaign", default=str(DEFAULT_CAMPAIGN))
    parser.add_argument("--corpus-lock", default=str(DEFAULT_CORPUS_LOCK))
    parser.add_argument("--asset-lock")
    parser.add_argument("--baseline-patches")
    parser.add_argument("--require-asset-lock", action="store_true")
    parser.add_argument("--server", default="symforge")
    parser.add_argument("--server-arg", action="append", default=[])


def add_selection_arguments(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--case-id", action="append", default=[])
    parser.add_argument("--tool", action="append", default=[])
    parser.add_argument(
        "--case-kind",
        action="append",
        choices=("happy", "adverse", "control", "stateful"),
        default=[],
    )
    parser.add_argument("--smoke", action="store_true")
    parser.add_argument("--all", action="store_true")
    parser.add_argument(
        "--allow-stdio-shadow",
        action="store_true",
        help="Run a case's declared stdio parity shadow when its primary transport is HTTP.",
    )


def bundle_from_args(args: argparse.Namespace) -> InputBundle:
    benchmark = pathlib.Path(args.benchmark_root)
    fixture = (
        pathlib.Path(args.fixture_root)
        if args.fixture_root
        else benchmark / "fixtures" / "control-v1"
    )
    if args.asset_lock:
        asset_lock: pathlib.Path | None = pathlib.Path(args.asset_lock)
    elif DEFAULT_ASSET_LOCK.is_file():
        asset_lock = DEFAULT_ASSET_LOCK
    else:
        asset_lock = None
    if args.baseline_patches:
        baseline_patches = pathlib.Path(args.baseline_patches)
    elif (
        args.command != "freeze-baseline-patches" and DEFAULT_BASELINE_PATCHES.is_file()
    ):
        baseline_patches = DEFAULT_BASELINE_PATCHES
    else:
        baseline_patches = None
    return validate_inputs(
        benchmark_root=benchmark,
        fixture_root=fixture,
        cases_path=pathlib.Path(args.cases),
        campaign_path=pathlib.Path(args.campaign),
        corpus_lock_path=pathlib.Path(args.corpus_lock),
        asset_lock_path=asset_lock,
        baseline_patches_path=baseline_patches,
        require_asset_lock=args.require_asset_lock,
    )


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Run frozen SymForge direct-RPC cases in disposable external clones. "
            "No paid model APIs are used."
        )
    )
    subparsers = parser.add_subparsers(dest="command", required=True)
    validate = subparsers.add_parser(
        "validate", help="Validate every frozen input and the SUT identity."
    )
    add_input_arguments(validate)
    dry_run = subparsers.add_parser(
        "dry-run", help="Resolve a case plan without materializing or calling MCP."
    )
    add_input_arguments(dry_run)
    add_selection_arguments(dry_run)
    run = subparsers.add_parser("run", help="Execute selected direct-RPC cases.")
    add_input_arguments(run)
    add_selection_arguments(run)
    run.add_argument("--run-id", required=True)
    run.add_argument("--output")
    run.add_argument("--work-parent")
    baseline_mode = run.add_mutually_exclusive_group()
    baseline_mode.add_argument(
        "--with-baseline",
        dest="with_baseline",
        action="store_true",
        help="Run the baseline arm (default; retained for explicit scripts).",
    )
    baseline_mode.add_argument(
        "--direct-only",
        dest="with_baseline",
        action="store_false",
        help="Diagnostic only: omit baseline and mark the artifact non-formal.",
    )
    run.set_defaults(with_baseline=True)
    run.add_argument("--timeout", type=float)
    run.add_argument(
        "--allow-unlocked-diagnostic",
        action="store_true",
        help="Allow evidence collection before assets.lock.json is frozen; output remains invalid for economics.",
    )
    run.add_argument(
        "--repetitions",
        type=int,
        help="Diagnostic override for measured repetitions; omission uses each frozen case limit.",
    )
    subparsers.add_parser(
        "self-test",
        help="Run fake-MCP transport, restart, sanitizer, and baseline tests.",
    )
    freeze = subparsers.add_parser(
        "freeze-baseline-patches",
        help="Generate and scratch-verify evaluator-only baseline patch text before asset freeze.",
    )
    add_input_arguments(freeze)
    freeze.add_argument("--output", required=True)
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    if args.command == "self-test":
        return self_test()
    bundle = bundle_from_args(args)
    if args.command == "freeze-baseline-patches":
        result = freeze_baseline_patches(bundle, pathlib.Path(args.output))
        print(json.dumps(result, indent=2, sort_keys=True))
        return 0
    server_path, sut = validate_sut(bundle, args.server)
    if args.command == "validate":
        summary = {
            "status": "valid",
            "protocol": harness.PROTOCOL_ID,
            "case_count": len(bundle.cases["cases"]),
            "repository_count": len(bundle.repositories),
            "input_hashes": bundle.hashes,
            "asset_lock": bundle.asset_lock_status,
            "sut": sut,
            "paid_api_calls": 0,
        }
        print(json.dumps(summary, indent=2, sort_keys=True))
        return 0
    selected = select_cases(
        bundle.cases,
        case_ids=args.case_id,
        tools=args.tool,
        case_kinds=args.case_kind,
        smoke=args.smoke,
        all_cases=args.all,
    )
    plan = dry_run_plan(bundle, selected, allow_stdio_shadow=args.allow_stdio_shadow)
    if args.command == "dry-run":
        print(json.dumps(plan, indent=2, sort_keys=True))
        return 0
    if args.timeout is not None and args.timeout <= 0:
        raise RunnerError("--timeout must be greater than zero")
    if args.repetitions is not None and args.repetitions <= 0:
        raise RunnerError("--repetitions must be greater than zero")
    if (
        not bundle.asset_lock_status.get("present")
        and not args.allow_unlocked_diagnostic
    ):
        raise RunnerError(
            "run requires a verified asset lock; use --allow-unlocked-diagnostic only for pre-freeze evidence"
        )
    output = (
        pathlib.Path(args.output)
        if args.output
        else bundle.benchmark_root / "runs" / f"{args.run_id}.jsonl"
    )
    work_parent = (
        pathlib.Path(args.work_parent)
        if args.work_parent
        else bundle.benchmark_root / "work" / f"direct-{args.run_id}"
    )
    settings = RunSettings(
        run_id=args.run_id,
        output=output,
        work_parent=work_parent,
        server_path=server_path,
        server_args=list(args.server_arg),
        with_baseline=args.with_baseline,
        allow_stdio_shadow=args.allow_stdio_shadow,
        timeout_override=args.timeout,
        repetitions_override=args.repetitions,
    )
    failures = run_campaign(bundle, selected, settings, sut)
    print(
        f"direct campaign complete: selected_cases={len(selected)} failed_trials={failures}; "
        f"sanitized evidence={settings.output}"
    )
    return 1 if failures else 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (RunnerError, harness.HarnessError) as exc:
        print(f"direct_case_runner: {exc}", file=sys.stderr)
        raise SystemExit(2) from exc
