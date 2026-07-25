# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "tiktoken==0.13.0",
# ]
# ///
"""Validate and summarize sanitized SymForge benchmark evidence.

This analyzer is intentionally correctness-first. A runner ``case_complete``
record proves capture completion only. Correctness can pass only through a
separate evaluator decision with evidence.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.metadata
import json
import math
import pathlib
import statistics
import sys
import tempfile
from dataclasses import dataclass, field
from typing import Any, Iterable, Sequence

import mcp_harness as harness


SCRIPT_DIR = pathlib.Path(__file__).resolve().parent
DEFAULT_CASES = SCRIPT_DIR / "cases.json"
DEFAULT_ASSET_LOCK = SCRIPT_DIR / "assets.lock.json"
EVALUATOR_SCHEMA = "SFBENCH-adjudication-decisions-1"
SUMMARY_SCHEMA = "SFBENCH-adjudication-summary-1"
ENCODINGS = ("cl100k", "o200k")

VALID_ROLES = {
    "task",
    "required_prerequisite",
    "comparison_variant",
    "estimate_diagnostic",
    "oracle_reference",
    "replay_diagnostic",
    "warmup",
}

WORKFLOW_CASES = {
    "SF-batch_edit-001",
    "SF-batch_insert-001",
    "SF-batch_rename-001",
    "SF-delete_symbol-001",
    "SF-edit_within_symbol-001",
    "SF-insert_symbol-001",
    "SF-replace_symbol_body-001",
    "SF-symforge_edit-001",
    "SF-investigation_suggest-001",
}

VARIANT_CASES = {
    "SF-find_references-001",
    "SF-get_file_content-001",
    "SF-get_symbol-001",
    "SF-get_symbol_context-001",
    "SF-health-001",
    "SF-search_files-001",
    "SF-status-001",
    "SF-validate_file_syntax-001",
    "SF-symforge-001",
}


class AdjudicationError(RuntimeError):
    """Expected failure with a message safe to display."""


@dataclass
class Issue:
    severity: str
    code: str
    message: str
    artifact: str | None = None
    case_id: str | None = None

    def as_dict(self) -> dict[str, Any]:
        return {
            "severity": self.severity,
            "code": self.code,
            "message": self.message,
            "artifact": self.artifact,
            "case_id": self.case_id,
        }


@dataclass
class Artifact:
    path: pathlib.Path
    sha256: str
    records: list[dict[str, Any]]


@dataclass
class Trial:
    artifact: Artifact
    run_id: str
    case: dict[str, Any]
    records: list[dict[str, Any]]
    direct_trials: list[dict[str, Any]]
    baseline_total: dict[str, Any] | None
    terminal: str
    automatic_failure_codes: list[str] = field(default_factory=list)


def canonical_json(value: Any) -> str:
    return json.dumps(
        value,
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
    )


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def file_sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while chunk := handle.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def load_json_object(path: pathlib.Path, label: str) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except OSError as exc:
        raise AdjudicationError(f"could not read {label}: {path.name}") from exc
    except json.JSONDecodeError as exc:
        raise AdjudicationError(
            f"invalid {label} JSON at {path.name}:{exc.lineno}:{exc.colno}"
        ) from exc
    if not isinstance(value, dict):
        raise AdjudicationError(f"{label} must be one JSON object")
    return value


def load_records(path: pathlib.Path) -> Artifact:
    try:
        raw = path.read_bytes()
    except OSError as exc:
        raise AdjudicationError(f"could not read artifact: {path.name}") from exc
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise AdjudicationError(f"artifact is not UTF-8: {path.name}") from exc
    stripped = text.strip()
    if not stripped:
        raise AdjudicationError(f"artifact is empty: {path.name}")

    records: list[dict[str, Any]] = []
    try:
        whole = json.loads(stripped)
    except json.JSONDecodeError:
        for line_number, line in enumerate(text.splitlines(), start=1):
            if not line.strip():
                continue
            try:
                value = json.loads(line)
            except json.JSONDecodeError as exc:
                raise AdjudicationError(
                    f"invalid artifact JSON at {path.name}:{line_number}:{exc.colno}"
                ) from exc
            if not isinstance(value, dict):
                raise AdjudicationError(
                    f"artifact row is not an object: {path.name}:{line_number}"
                )
            records.append(value)
    else:
        if isinstance(whole, dict):
            records = [whole]
        elif isinstance(whole, list) and all(isinstance(row, dict) for row in whole):
            records = list(whole)
        else:
            raise AdjudicationError(
                f"artifact must be JSONL, an object, or an object array: {path.name}"
            )
    return Artifact(path=path, sha256=sha256_bytes(raw), records=records)


def flatten_asset_hashes(value: Any, result: dict[str, str]) -> None:
    if isinstance(value, dict):
        path = value.get("path")
        digest = value.get("sha256") or value.get("sha256_hex")
        if isinstance(path, str) and is_sha256(digest):
            result[path.replace("\\", "/")] = str(digest).lower()
        for key, child in value.items():
            if is_sha256(child) and looks_like_path(str(key)):
                result[str(key).replace("\\", "/")] = str(child).lower()
            elif isinstance(child, dict):
                nested = child.get("sha256") or child.get("sha256_hex")
                if is_sha256(nested):
                    result[str(key).replace("\\", "/")] = str(nested).lower()
            flatten_asset_hashes(child, result)
    elif isinstance(value, list):
        for child in value:
            flatten_asset_hashes(child, result)


def is_sha256(value: Any) -> bool:
    return isinstance(value, str) and len(value) == 64 and all(
        character in "0123456789abcdefABCDEF" for character in value
    )


def looks_like_path(value: str) -> bool:
    return "/" in value or "\\" in value or "." in pathlib.PurePosixPath(value).name


def unique_asset_digest(path: pathlib.Path, hashes: dict[str, str]) -> str | None:
    normalized = path.resolve().as_posix()
    matches = {
        digest
        for name, digest in hashes.items()
        if normalized.endswith(name.replace("\\", "/"))
        or path.name == pathlib.PurePosixPath(name.replace("\\", "/")).name
    }
    return next(iter(matches)) if len(matches) == 1 else None


def nonnegative_number(value: Any) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and value >= 0


def integer_field(record: dict[str, Any], key: str) -> int | None:
    value = record.get(key)
    if isinstance(value, int) and not isinstance(value, bool) and value >= 0:
        return value
    return None


def numeric_field(record: dict[str, Any], key: str) -> float | None:
    value = record.get(key)
    return float(value) if nonnegative_number(value) else None


def percentile(values: Sequence[float], fraction: float) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    if len(ordered) == 1:
        return ordered[0]
    position = (len(ordered) - 1) * fraction
    lower = math.floor(position)
    upper = math.ceil(position)
    if lower == upper:
        return ordered[lower]
    weight = position - lower
    return ordered[lower] * (1 - weight) + ordered[upper] * weight


def median(values: Sequence[float]) -> float | None:
    return float(statistics.median(values)) if values else None


def sanitize_output(value: dict[str, Any]) -> dict[str, Any]:
    sanitizer = harness.Sanitizer()
    safe = sanitizer.sanitize_obj(value, "adjudication_output")
    final = harness.Sanitizer()
    rescanned = final.sanitize_obj(safe, "adjudication_output.final")
    if rescanned != safe or final.event_count():
        raise AdjudicationError("final summary secret scan failed")
    if sanitizer.event_count():
        safe["sanitizer_redactions"] = sanitizer.event_count()
    return safe


def write_json(path: pathlib.Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def load_cases(path: pathlib.Path) -> tuple[dict[str, Any], dict[str, dict[str, Any]]]:
    manifest = load_json_object(path, "cases manifest")
    if manifest.get("protocol") != harness.PROTOCOL_ID:
        raise AdjudicationError("cases protocol does not match the harness")
    rows = manifest.get("cases")
    if not isinstance(rows, list) or not rows:
        raise AdjudicationError("cases manifest has no cases")
    by_id: dict[str, dict[str, Any]] = {}
    for row in rows:
        if not isinstance(row, dict) or not isinstance(row.get("id"), str):
            raise AdjudicationError("cases manifest contains a malformed case")
        if row["id"] in by_id:
            raise AdjudicationError("cases manifest contains duplicate case IDs")
        by_id[row["id"]] = row
    return manifest, by_id


def expected_surfaces(cases: dict[str, Any]) -> dict[str, set[str]]:
    inventory = cases.get("inventory", {})
    result = {
        "full": set(inventory.get("full_surface_tools", [])),
        "compact": set(inventory.get("compact_surface_tools", [])),
        "meta": set(inventory.get("meta_surface_tools", [])),
    }
    if any(not tools for tools in result.values()):
        raise AdjudicationError("cases inventory omits a surface")
    return result


def manifest_schema_views(
    artifacts: Sequence[Artifact],
    cases: dict[str, Any],
    counter: harness.TokenCounter,
    issues: list[Issue],
) -> dict[str, dict[str, Any]]:
    expected = expected_surfaces(cases)
    views: dict[str, dict[str, Any]] = {}
    for artifact in artifacts:
        for record in artifact.records:
            if record.get("record_type") not in {"manifest", "session_manifest"}:
                continue
            surface = record.get("surface")
            manifest = record.get("manifest")
            tools = manifest.get("tools") if isinstance(manifest, dict) else None
            if surface not in expected or not isinstance(tools, list):
                issues.append(
                    Issue(
                        "error",
                        "MANIFEST_MALFORMED",
                        "live manifest record lacks a known surface or tools list",
                        artifact.path.name,
                    )
                )
                continue
            names = {
                tool.get("name")
                for tool in tools
                if isinstance(tool, dict) and isinstance(tool.get("name"), str)
            }
            if names != expected[surface]:
                issues.append(
                    Issue(
                        "error",
                        "MANIFEST_TOOL_SET_MISMATCH",
                        f"{surface} tool set differs from cases inventory",
                        artifact.path.name,
                    )
                )
                continue
            serialized = canonical_json(tools)
            counts = counter.metrics(serialized)
            individual: dict[str, dict[str, int]] = {}
            for tool in tools:
                if not isinstance(tool, dict) or not isinstance(tool.get("name"), str):
                    continue
                metrics = counter.metrics(canonical_json(tool))
                individual[tool["name"]] = {
                    "cl100k": metrics["cl100k"],
                    "o200k": metrics["o200k"],
                    "utf8_bytes": metrics["utf8_bytes"],
                }
            view = {
                "manifest_sha256": sha256_bytes(canonical_json(manifest).encode()),
                "tool_count": len(names),
                "tokens": {
                    "cl100k": counts["cl100k"],
                    "o200k": counts["o200k"],
                    "utf8_bytes": counts["utf8_bytes"],
                },
                "individual": individual,
            }
            previous = views.get(surface)
            if previous is not None and previous != view:
                issues.append(
                    Issue(
                        "error",
                        "MANIFEST_NONDETERMINISTIC",
                        f"multiple {surface} manifests disagree",
                        artifact.path.name,
                    )
                )
            else:
                views[surface] = view
    for surface in expected:
        if surface not in views:
            issues.append(
                Issue(
                    "error",
                    "MANIFEST_SURFACE_MISSING",
                    f"no live {surface} manifest was supplied",
                )
            )
    return views


def load_evaluator(path: pathlib.Path | None) -> dict[str, Any]:
    if path is None:
        return {"schema_version": EVALUATOR_SCHEMA, "cases": {}}
    value = load_json_object(path, "evaluator decisions")
    if value.get("schema_version") != EVALUATOR_SCHEMA:
        raise AdjudicationError("evaluator decision schema is unsupported")
    if not isinstance(value.get("cases"), dict):
        raise AdjudicationError("evaluator decisions must contain cases{}")
    return value


def validate_decisions(
    evaluator: dict[str, Any], case_ids: Iterable[str], issues: list[Issue]
) -> None:
    known = set(case_ids)
    for case_id, decision in evaluator.get("cases", {}).items():
        if case_id not in known or not isinstance(decision, dict):
            issues.append(
                Issue(
                    "error",
                    "EVALUATOR_CASE_INVALID",
                    "evaluator names an unknown or malformed case",
                    case_id=case_id,
                )
            )
            continue
        status = str(decision.get("status", "manual_pending")).lower()
        if status not in {"pass", "fail", "manual_pending"}:
            issues.append(
                Issue(
                    "error",
                    "EVALUATOR_STATUS_INVALID",
                    "evaluator status must be pass, fail, or manual_pending",
                    case_id=case_id,
                )
            )
        checks = decision.get("checks")
        if not isinstance(checks, list) or not checks:
            issues.append(
                Issue(
                    "error",
                    "EVALUATOR_CHECKS_MISSING",
                    "evaluator decision must include evidenced checks",
                    case_id=case_id,
                )
            )
            continue
        check_statuses: list[str] = []
        for check in checks:
            if not isinstance(check, dict) or not isinstance(check.get("id"), str):
                issues.append(
                    Issue(
                        "error",
                        "EVALUATOR_CHECK_INVALID",
                        "evaluator check must have an ID",
                        case_id=case_id,
                    )
                )
                continue
            check_status = str(check.get("status", "manual_pending")).lower()
            evidence = check.get("evidence")
            if check_status not in {"pass", "fail", "manual_pending"}:
                issues.append(
                    Issue(
                        "error",
                        "EVALUATOR_CHECK_STATUS_INVALID",
                        "evaluator check has an invalid status",
                        case_id=case_id,
                    )
                )
            if not isinstance(evidence, list) or not evidence:
                issues.append(
                    Issue(
                        "error",
                        "EVALUATOR_EVIDENCE_MISSING",
                        "every evaluator check needs evidence references",
                        case_id=case_id,
                    )
                )
            check_statuses.append(check_status)
        if status == "pass" and any(item != "pass" for item in check_statuses):
            issues.append(
                Issue(
                    "error",
                    "EVALUATOR_PASS_INCONSISTENT",
                    "case pass conflicts with unresolved or failed checks",
                    case_id=case_id,
                )
            )
        if status == "fail" and "fail" not in check_statuses:
            issues.append(
                Issue(
                    "error",
                    "EVALUATOR_FAIL_INCONSISTENT",
                    "case fail has no failed check",
                    case_id=case_id,
                )
            )

        paired_arms = decision.get("paired_arms")
        if paired_arms is None:
            continue
        if not isinstance(paired_arms, dict):
            issues.append(
                Issue(
                    "error",
                    "EVALUATOR_PAIRED_ARMS_INVALID",
                    "paired_arms must be an object when supplied",
                    case_id=case_id,
                )
            )
            continue
        for arm_name in ("baseline", "symforge"):
            arm = paired_arms.get(arm_name)
            if not isinstance(arm, dict):
                issues.append(
                    Issue(
                        "error",
                        "EVALUATOR_PAIRED_ARM_MISSING",
                        f"paired evaluator decision is missing the {arm_name} arm",
                        case_id=case_id,
                    )
                )
                continue
            arm_status = str(arm.get("status", "manual_pending")).lower()
            if arm_status not in {"pass", "fail", "manual_pending"}:
                issues.append(
                    Issue(
                        "error",
                        "EVALUATOR_PAIRED_ARM_STATUS_INVALID",
                        f"paired {arm_name} status is invalid",
                        case_id=case_id,
                    )
                )
            evidence = arm.get("evidence")
            if not isinstance(evidence, list) or not evidence:
                issues.append(
                    Issue(
                        "error",
                        "EVALUATOR_PAIRED_ARM_EVIDENCE_MISSING",
                        f"paired {arm_name} decision needs evidence references",
                        case_id=case_id,
                    )
                )


def evaluator_summary(decision: dict[str, Any] | None) -> dict[str, Any]:
    if not isinstance(decision, dict):
        return {
            "status": "UNEVALUATED",
            "check_counts": {"pass": 0, "fail": 0, "manual_pending": 0},
            "evidence_sha256": None,
        }
    counts = {"pass": 0, "fail": 0, "manual_pending": 0}
    evidence_basis: list[dict[str, Any]] = []
    for check in decision.get("checks", []):
        if not isinstance(check, dict):
            continue
        status = str(check.get("status", "manual_pending")).lower()
        if status in counts:
            counts[status] += 1
        evidence_basis.append(
            {
                "id": check.get("id"),
                "status": status,
                "evidence": check.get("evidence", []),
            }
        )
    status = str(decision.get("status", "manual_pending")).lower()
    normalized = {
        "pass": "PASS",
        "fail": "FAIL",
        "manual_pending": "UNEVALUATED",
    }.get(status, "UNEVALUATED")
    return {
        "status": normalized,
        "check_counts": counts,
        "evidence_sha256": sha256_bytes(canonical_json(evidence_basis).encode()),
    }


def validate_asset_identity(
    cases_path: pathlib.Path,
    asset_lock: dict[str, Any],
    artifacts: Sequence[Artifact],
    issues: list[Issue],
) -> dict[str, Any]:
    flattened: dict[str, str] = {}
    flatten_asset_hashes(asset_lock, flattened)
    locked_digests = set(flattened.values())
    cases_digest = file_sha256(cases_path)
    expected_cases = unique_asset_digest(cases_path, flattened)
    if expected_cases != cases_digest:
        issues.append(
            Issue(
                "error",
                "CASES_ASSET_MISMATCH",
                "cases.json is absent from the asset lock or has the wrong hash",
            )
        )
    checked_input_hashes: set[str] = set()
    for artifact in artifacts:
        for record in artifact.records:
            if record.get("record_type") != "campaign_start":
                continue
            input_hashes = record.get("input_hashes")
            if not isinstance(input_hashes, dict):
                issues.append(
                    Issue(
                        "error",
                        "CAMPAIGN_INPUT_HASHES_MISSING",
                        "campaign_start lacks input_hashes",
                        artifact.path.name,
                    )
                )
                continue
            if input_hashes.get("cases") != cases_digest:
                issues.append(
                    Issue(
                        "error",
                        "CAMPAIGN_CASES_HASH_MISMATCH",
                        "campaign cases hash differs from supplied cases.json",
                        artifact.path.name,
                    )
                )
            for digest in input_hashes.values():
                if not is_sha256(digest):
                    issues.append(
                        Issue(
                            "error",
                            "CAMPAIGN_INPUT_HASH_INVALID",
                            "campaign input hash is malformed",
                            artifact.path.name,
                        )
                    )
                    continue
                normalized = str(digest).lower()
                checked_input_hashes.add(normalized)
                if normalized not in locked_digests:
                    issues.append(
                        Issue(
                            "error",
                            "CAMPAIGN_INPUT_NOT_ASSET_LOCKED",
                            "campaign input hash is absent from the asset lock",
                            artifact.path.name,
                        )
                    )
    return {
        "asset_count": len(flattened),
        "cases_sha256": cases_digest,
        "checked_campaign_input_hashes": len(checked_input_hashes),
    }


def campaign_groups(
    artifact: Artifact,
) -> list[tuple[str, dict[str, Any], dict[str, Any], list[dict[str, Any]]]]:
    starts = [
        row for row in artifact.records if row.get("record_type") == "campaign_start"
    ]
    result: list[tuple[str, dict[str, Any], dict[str, Any], list[dict[str, Any]]]] = []
    for start in starts:
        run_id = start.get("run_id")
        if not isinstance(run_id, str) or not run_id:
            continue
        rows = [row for row in artifact.records if row.get("run_id") == run_id]
        completes = [
            row for row in rows if row.get("record_type") == "campaign_complete"
        ]
        complete = completes[0] if len(completes) == 1 else {}
        result.append((run_id, start, complete, rows))
    return result


def trial_automatic_failures(
    case: dict[str, Any],
    rows: Sequence[dict[str, Any]],
    direct: Sequence[dict[str, Any]],
    terminal: str,
) -> list[str]:
    failures: list[str] = []
    if terminal == "case_error":
        failures.append("case_error")
    if case.get("case_kind") == "happy":
        for record in direct:
            if record.get("status") != "ok":
                failures.append("happy_request_not_ok")
                break
    for record in rows:
        if record.get("record_type") != "source_inventory_after":
            continue
        policy = record.get("mutation_policy")
        if isinstance(policy, dict) and policy.get("safe") is False:
            failures.append("mutation_policy_violation")
        auxiliary = record.get("fixture_auxiliary_violations")
        if isinstance(auxiliary, list) and auxiliary:
            failures.append("fixture_auxiliary_violation")
    return sorted(set(failures))


def baseline_automatic_failures(rows: Sequence[dict[str, Any]]) -> list[str]:
    failures: list[str] = []
    for record in rows:
        if record.get("record_type") != "baseline_source_inventory_after":
            continue
        policy = record.get("mutation_policy")
        if isinstance(policy, dict) and policy.get("safe") is False:
            failures.append("baseline_mutation_policy_violation")
        auxiliary = record.get("fixture_auxiliary_violations")
        if isinstance(auxiliary, list) and auxiliary:
            failures.append("baseline_fixture_auxiliary_violation")
    return sorted(set(failures))


def split_trial_windows(
    rows: Sequence[dict[str, Any]],
    artifact_name: str,
    issues: list[Issue],
) -> list[tuple[str, int, list[dict[str, Any]]]]:
    """Split ordered campaign rows into case_start-to-terminal trial windows."""
    windows: list[tuple[str, int, list[dict[str, Any]]]] = []
    active_case: str | None = None
    active_ordinal = 0
    active_rows: list[dict[str, Any]] = []

    for row in rows:
        record_type = row.get("record_type")
        if record_type == "case_start":
            if active_case is not None:
                issues.append(
                    Issue(
                        "error",
                        "CASE_WINDOW_OVERLAP",
                        "a new case_start appeared before the prior trial terminated",
                        artifact_name,
                        active_case,
                    )
                )
                windows.append((active_case, active_ordinal, active_rows))
            case_id = row.get("case_id")
            ordinal = row.get("trial_ordinal")
            if not isinstance(case_id, str) or not case_id:
                issues.append(
                    Issue(
                        "error",
                        "CASE_WINDOW_ID_INVALID",
                        "case_start has no valid case_id",
                        artifact_name,
                    )
                )
                case_id = "<invalid-case-id>"
            if (
                isinstance(ordinal, bool)
                or not isinstance(ordinal, int)
                or ordinal <= 0
            ):
                issues.append(
                    Issue(
                        "error",
                        "TRIAL_ORDINAL_INVALID",
                        "case_start has no positive integer trial_ordinal",
                        artifact_name,
                        case_id,
                    )
                )
                ordinal = -(len(windows) + 1)
            active_case = case_id
            active_ordinal = ordinal
            active_rows = [row]
            continue

        if active_case is None:
            continue
        active_rows.append(row)
        if record_type not in {"case_complete", "case_error"}:
            continue
        if row.get("case_id") != active_case:
            issues.append(
                Issue(
                    "error",
                    "CASE_WINDOW_TERMINAL_MISMATCH",
                    "trial terminal case_id differs from its case_start",
                    artifact_name,
                    active_case,
                )
            )
        windows.append((active_case, active_ordinal, active_rows))
        active_case = None
        active_ordinal = 0
        active_rows = []

    if active_case is not None:
        issues.append(
            Issue(
                "error",
                "CASE_WINDOW_UNTERMINATED",
                "case_start has no following case_complete or case_error",
                artifact_name,
                active_case,
            )
        )
        windows.append((active_case, active_ordinal, active_rows))
    return windows


def collect_direct_trials(
    artifacts: Sequence[Artifact],
    cases_by_id: dict[str, dict[str, Any]],
    issues: list[Issue],
) -> list[Trial]:
    trials: list[Trial] = []
    seen: set[tuple[str, str, int]] = set()
    for artifact in artifacts:
        for run_id, start, complete, rows in campaign_groups(artifact):
            selected = start.get("selected_case_ids")
            if not isinstance(selected, list) or not selected:
                issues.append(
                    Issue(
                        "error",
                        "CAMPAIGN_SELECTION_MISSING",
                        "campaign_start has no selected case IDs",
                        artifact.path.name,
                    )
                )
                continue
            if not complete:
                issues.append(
                    Issue(
                        "error",
                        "CAMPAIGN_TERMINAL_MISSING",
                        "campaign has no unique campaign_complete record",
                        artifact.path.name,
                    )
                )

            windows = split_trial_windows(rows, artifact.path.name, issues)
            selected_ids = {
                case_id for case_id in selected if isinstance(case_id, str)
            }
            for case_id, _, _ in windows:
                if case_id not in selected_ids:
                    issues.append(
                        Issue(
                            "error",
                            "CAMPAIGN_CASE_UNSELECTED",
                            "campaign contains a trial absent from selected_case_ids",
                            artifact.path.name,
                            case_id,
                        )
                    )

            for case_id in selected:
                if case_id not in cases_by_id:
                    issues.append(
                        Issue(
                            "error",
                            "CAMPAIGN_CASE_UNKNOWN",
                            "campaign selected a case absent from cases.json",
                            artifact.path.name,
                        )
                    )
                    continue
                case_windows = [
                    (ordinal, case_rows)
                    for window_case, ordinal, case_rows in windows
                    if window_case == case_id
                ]
                if not case_windows:
                    issues.append(
                        Issue(
                            "error",
                            "CASE_TRIAL_MISSING",
                            "selected case has no trial window",
                            artifact.path.name,
                            case_id,
                        )
                    )
                    continue

                case = cases_by_id[case_id]
                warmups_required = int(
                    case.get("limits", {}).get("discarded_warmups", 0)
                )
                warmups_observed = sum(
                    case_rows[0].get("discarded_warmup") is True
                    for _, case_rows in case_windows
                    if case_rows
                )
                if warmups_observed != warmups_required:
                    issues.append(
                        Issue(
                            "error",
                            "WARMUPS_INCOMPLETE",
                            (
                                f"case has {warmups_observed} discarded warmups but "
                                f"requires {warmups_required}"
                            ),
                            artifact.path.name,
                            case_id,
                        )
                    )

                for ordinal, case_rows in case_windows:
                    identity = (run_id, case_id, ordinal)
                    if identity in seen:
                        issues.append(
                            Issue(
                                "error",
                                "TRIAL_ID_DUPLICATE",
                                "run_id, case_id, and trial_ordinal are duplicated",
                                artifact.path.name,
                                case_id,
                            )
                        )
                        continue
                    seen.add(identity)

                    starts = [
                        row
                        for row in case_rows
                        if row.get("record_type") == "case_start"
                    ]
                    completed = [
                        row
                        for row in case_rows
                        if row.get("record_type") == "case_complete"
                    ]
                    errors = [
                        row
                        for row in case_rows
                        if row.get("record_type") == "case_error"
                    ]
                    if len(starts) != 1:
                        issues.append(
                            Issue(
                                "error",
                                "CASE_START_COUNT",
                                "trial does not have exactly one case_start",
                                artifact.path.name,
                                case_id,
                            )
                        )
                    if len(completed) + len(errors) != 1:
                        issues.append(
                            Issue(
                                "error",
                                "CASE_TERMINAL_COUNT",
                                "trial does not have exactly one terminal case record",
                                artifact.path.name,
                                case_id,
                            )
                        )
                    terminal = "case_complete" if completed else "case_error"
                    direct = sorted(
                        [
                            row
                            for row in case_rows
                            if row.get("record_type") == "direct_trial"
                        ],
                        key=lambda row: int(row.get("case_step", 0)),
                    )
                    requests = case.get("requests", [])
                    if completed and len(direct) != len(requests):
                        issues.append(
                            Issue(
                                "error",
                                "REQUEST_COUNT_MISMATCH",
                                "completed trial measured request count differs from case",
                                artifact.path.name,
                                case_id,
                            )
                        )
                    if completed:
                        for expected, observed in zip(
                            requests, direct, strict=False
                        ):
                            if (
                                observed.get("case_step") != expected.get("step")
                                or observed.get("request_label")
                                != expected.get("label")
                                or observed.get("tool") != expected.get("tool")
                            ):
                                issues.append(
                                    Issue(
                                        "error",
                                        "REQUEST_IDENTITY_MISMATCH",
                                        "measured request does not match the frozen step",
                                        artifact.path.name,
                                        case_id,
                                    )
                                )

                    discarded_warmup = bool(
                        starts and starts[0].get("discarded_warmup") is True
                    )
                    discard_flags = {
                        row.get("discarded_warmup")
                        for row in case_rows
                        if row.get("record_type")
                        in {"case_start", "direct_trial", "case_complete", "case_error"}
                    }
                    if len(discard_flags) != 1:
                        issues.append(
                            Issue(
                                "error",
                                "WARMUP_FLAG_MISMATCH",
                                "trial rows disagree on discarded_warmup",
                                artifact.path.name,
                                case_id,
                            )
                        )
                    if discarded_warmup:
                        if errors:
                            issues.append(
                                Issue(
                                    "error",
                                    "WARMUP_TRIAL_FAILED",
                                    "discarded warmup terminated with case_error",
                                    artifact.path.name,
                                    case_id,
                                )
                            )
                        continue

                    if completed:
                        before_count = sum(
                            row.get("record_type") == "source_inventory_before"
                            for row in case_rows
                        )
                        after_count = sum(
                            row.get("record_type") == "source_inventory_after"
                            for row in case_rows
                        )
                        if before_count != 1 or after_count != 1:
                            issues.append(
                                Issue(
                                    "error",
                                    "SOURCE_INVENTORY_INCOMPLETE",
                                    "completed trial lacks before/after source inventory",
                                    artifact.path.name,
                                    case_id,
                                )
                            )
                    baseline_rows = [
                        row
                        for row in case_rows
                        if row.get("record_type") == "baseline_total"
                    ]
                    formal = start.get("formal_economics") is True
                    if formal and len(baseline_rows) != 1:
                        issues.append(
                            Issue(
                                "error",
                                "BASELINE_TOTAL_MISSING",
                                "formal trial lacks one recipe baseline_total",
                                artifact.path.name,
                                case_id,
                            )
                        )
                    baseline = (
                        baseline_rows[0] if len(baseline_rows) == 1 else None
                    )
                    trial = Trial(
                        artifact=artifact,
                        run_id=run_id,
                        case=case,
                        records=case_rows,
                        direct_trials=direct,
                        baseline_total=baseline,
                        terminal=terminal,
                    )
                    trial.automatic_failure_codes = trial_automatic_failures(
                        case, case_rows, direct, terminal
                    )
                    trials.append(trial)
    return trials


def validate_repetitions(trials: Sequence[Trial], issues: list[Issue]) -> None:
    grouped: dict[str, list[Trial]] = {}
    for trial in trials:
        grouped.setdefault(trial.case["id"], []).append(trial)
    for case_id, case_trials in grouped.items():
        required = int(case_trials[0].case.get("limits", {}).get("repetitions", 0))
        if required <= 0:
            issues.append(
                Issue(
                    "error",
                    "REPETITION_POLICY_INVALID",
                    "case has no positive repetition count",
                    case_id=case_id,
                )
            )
        elif len(case_trials) != required:
            issues.append(
                Issue(
                    "error",
                    "REPETITIONS_INCOMPLETE",
                    f"case has {len(case_trials)} trials but requires {required}",
                    case_id=case_id,
                )
            )


def tokenizer_status(issues: list[Issue]) -> dict[str, Any]:
    version = importlib.metadata.version("tiktoken")
    if version != "0.13.0":
        issues.append(
            Issue(
                "error",
                "TOKENIZER_VERSION_MISMATCH",
                "adjudicator requires tiktoken 0.13.0",
            )
        )
    return harness.TokenCounter().metadata()


def request_role(case: dict[str, Any], request: dict[str, Any]) -> tuple[str, str]:
    explicit = request.get("economics_role")
    if explicit is not None:
        if explicit not in VALID_ROLES:
            raise AdjudicationError(
                f"case {case['id']} has an invalid economics_role"
            )
        unit = str(request.get("economics_unit") or request.get("label") or "task")
        return str(explicit), unit

    label = str(request.get("label", "task"))
    args = request.get("args", {})
    if isinstance(args, dict) and args.get("estimate") is True:
        return "estimate_diagnostic", label
    if label == "full_reference":
        return "oracle_reference", label
    if label in {"replay", "repeat", "restart_probe"}:
        return "replay_diagnostic", label
    case_id = case["id"]
    if case_id == "SF-investigation_suggest-001":
        role = "required_prerequisite" if label == "step_1" else "task"
        return role, "workflow"
    if case_id in WORKFLOW_CASES:
        return "task", "workflow"
    if case_id == "SF-health_compact-001" and label == "full_reference":
        return "oracle_reference", label
    if case_id in VARIANT_CASES:
        first_label = str(case.get("requests", [{}])[0].get("label", "task"))
        role = "task" if label == first_label else "comparison_variant"
        return role, label
    return "task", "task"


def request_by_step(case: dict[str, Any]) -> dict[int, dict[str, Any]]:
    return {
        int(request["step"]): request
        for request in case.get("requests", [])
        if isinstance(request, dict) and isinstance(request.get("step"), int)
    }


def unit_rows(trial: Trial) -> list[dict[str, Any]]:
    requests = request_by_step(trial.case)
    observed = {
        int(row["case_step"]): row
        for row in trial.direct_trials
        if isinstance(row.get("case_step"), int)
    }
    case_id = trial.case["id"]
    if case_id == "SF-symforge_retrieve-001":
        labels = {
            request.get("label"): observed.get(step)
            for step, request in requests.items()
        }
        result: list[dict[str, Any]] = []
        if labels.get("trigger") is not None:
            result.append(
                {
                    "name": "summary",
                    "kind": "task",
                    "rows": [labels["trigger"]],
                }
            )
        if labels.get("trigger") is not None and labels.get("retrieve") is not None:
            result.append(
                {
                    "name": "exact_detail",
                    "kind": "task",
                    "rows": [labels["trigger"], labels["retrieve"]],
                }
            )
        return result

    grouped: dict[str, dict[str, Any]] = {}
    for step, request in requests.items():
        row = observed.get(step)
        if row is None:
            continue
        role, unit = request_role(trial.case, request)
        if role in {
            "estimate_diagnostic",
            "oracle_reference",
            "replay_diagnostic",
            "warmup",
        }:
            continue
        entry = grouped.setdefault(
            unit,
            {
                "name": unit,
                "kind": "comparison_variant"
                if role == "comparison_variant"
                else "task",
                "rows": [],
            },
        )
        entry["rows"].append(row)
    return list(grouped.values())


def scenario_rows(trial: Trial) -> list[dict[str, Any]]:
    requests = request_by_step(trial.case)
    included: list[dict[str, Any]] = []
    seen_steps: set[int] = set()
    for row in trial.direct_trials:
        step = row.get("case_step")
        if not isinstance(step, int) or step in seen_steps or step not in requests:
            continue
        role, _ = request_role(trial.case, requests[step])
        if role not in {
            "estimate_diagnostic",
            "oracle_reference",
            "replay_diagnostic",
            "warmup",
        }:
            included.append(row)
            seen_steps.add(step)
    return included


def rows_metrics(
    rows: Sequence[dict[str, Any]],
    surface: str,
    schema_views: dict[str, dict[str, Any]],
) -> dict[str, Any] | None:
    if not rows or surface not in schema_views:
        return None
    latency_values = [numeric_field(row, "rpc_ms") for row in rows]
    if any(value is None for value in latency_values):
        return None
    result: dict[str, Any] = {
        "call_count": len(rows),
        "call_tools": sorted(
            {row.get("tool") for row in rows if isinstance(row.get("tool"), str)}
        ),
        "latency_ms": sum(float(value) for value in latency_values if value is not None),
        "encodings": {},
    }
    for encoding in ENCODINGS:
        request_key = f"schema_free_tool_request_{encoding}"
        response_key = f"schema_free_tool_response_{encoding}"
        direct_key = f"schema_free_direct_payload_{encoding}"
        request_values = [integer_field(row, request_key) for row in rows]
        response_values = [integer_field(row, response_key) for row in rows]
        direct_values = [integer_field(row, direct_key) for row in rows]
        if any(value is None for value in request_values + response_values + direct_values):
            return None
        request_total = sum(int(value) for value in request_values)
        response_total = sum(int(value) for value in response_values)
        direct_total = sum(int(value) for value in direct_values)
        schema = int(schema_views[surface]["tokens"][encoding])
        individual = schema_views[surface]["individual"]
        lazy = sum(
            int(individual[name][encoding])
            for name in result["call_tools"]
            if name in individual
        )
        result["encodings"][encoding] = {
            "request": request_total,
            "response": response_total,
            "direct_payload": direct_total,
            "cold_eager_surface": direct_total + schema,
            "lazy_unique": direct_total + lazy,
            "theoretical_amortized_5": direct_total + schema / 5,
            "theoretical_amortized_20": direct_total + schema / 20,
            "surface_schema": schema,
            "lazy_schema_unique": lazy,
        }
    return result


def final_correctness(
    trials: Sequence[Trial], decision: dict[str, Any] | None
) -> tuple[str, dict[str, Any]]:
    summary = evaluator_summary(decision)
    if any(trial.automatic_failure_codes for trial in trials):
        return "FAIL", summary
    return str(summary["status"]), summary


def baseline_policy(decision: dict[str, Any] | None) -> tuple[str, str]:
    if not isinstance(decision, dict):
        return "unreviewed", "manual_pending"
    equivalence = str(decision.get("baseline_equivalence", "unreviewed")).lower()
    correctness = str(decision.get("baseline_correctness", "manual_pending")).lower()
    if equivalence not in {
        "valid",
        "capability_only",
        "lower_bound",
        "invalid",
        "unreviewed",
    }:
        equivalence = "invalid"
    if correctness not in {"pass", "fail", "manual_pending"}:
        correctness = "manual_pending"
    return equivalence, correctness


def numeric_verdict(
    baseline: float, symforge: float
) -> tuple[str, float, float | None, float | None]:
    saved = baseline - symforge
    if saved > 0:
        verdict = "POSITIVE"
    elif saved < 0:
        verdict = "TOKEN_NEGATIVE"
    else:
        verdict = "NEUTRAL"
    percent = 100 * saved / baseline if baseline else None
    multiplier = symforge / baseline if baseline else None
    return verdict, saved, percent, multiplier


def recipe_economics(
    trial: Trial,
    correctness: str,
    decision: dict[str, Any] | None,
    schema_views: dict[str, dict[str, Any]],
) -> dict[str, Any]:
    equivalence, baseline_correctness = baseline_policy(decision)
    result: dict[str, Any] = {
        "arm": "direct_recipe_sensitivity",
        "headline": False,
        "equivalence": equivalence,
        "baseline_correctness": baseline_correctness,
        "views": {},
        "break_even_tasks": None,
    }
    if correctness == "FAIL":
        result["verdict"] = "INVALID_INCORRECT"
        result["reason"] = "symforge_correctness_failed"
        return result
    if correctness != "PASS":
        result["verdict"] = "NOT_SCORED"
        result["reason"] = "correctness_unevaluated"
        return result
    baseline_failures = baseline_automatic_failures(trial.records)
    if baseline_failures:
        result["verdict"] = "N/A_NO_EQUIVALENT_BASELINE"
        result["reason"] = "baseline_automatic_failure"
        result["baseline_automatic_failures"] = baseline_failures
        return result
    if equivalence != "valid" or baseline_correctness != "pass":
        result["verdict"] = "N/A_NO_EQUIVALENT_BASELINE"
        result["reason"] = (
            "capability_gain"
            if equivalence == "capability_only"
            else "baseline_not_equivalent_or_correct"
        )
        return result
    if trial.baseline_total is None:
        result["verdict"] = "NOT_SCORED"
        result["reason"] = "baseline_tokens_missing"
        return result
    metrics = rows_metrics(scenario_rows(trial), trial.case["surface"], schema_views)
    if metrics is None:
        result["verdict"] = "NOT_SCORED"
        result["reason"] = "direct_tokens_missing"
        return result

    verdicts: list[str] = []
    for encoding in ENCODINGS:
        baseline_value = integer_field(
            trial.baseline_total, f"direct_payload_{encoding}"
        )
        if baseline_value is None:
            continue
        symforge = float(metrics["encodings"][encoding]["direct_payload"])
        verdict, saved, percent, multiplier = numeric_verdict(
            float(baseline_value), symforge
        )
        verdicts.append(verdict)
        result["views"][encoding] = {
            "baseline_tokens": baseline_value,
            "symforge_tokens": symforge,
            "saved_tokens": saved,
            "savings_percent": percent,
            "cost_multiplier": multiplier,
            "verdict": verdict,
        }
    if not result["views"]:
        result["verdict"] = "NOT_SCORED"
        result["reason"] = "baseline_tokens_missing"
        return result
    result["verdict"] = verdicts[0] if len(set(verdicts)) == 1 else "MIXED"
    result["reason"] = "valid_direct_recipe_sensitivity"

    setup = decision.get("setup_tokens") if isinstance(decision, dict) else None
    if isinstance(setup, dict):
        primary_setup = setup.get("cl100k")
        view = result["views"].get("cl100k")
        if isinstance(primary_setup, dict) and isinstance(view, dict):
            sym_setup = primary_setup.get("symforge")
            baseline_setup = primary_setup.get("baseline")
            denominator = view["baseline_tokens"] - view["symforge_tokens"]
            if nonnegative_number(sym_setup) and nonnegative_number(baseline_setup):
                if denominator > 0:
                    result["break_even_tasks"] = max(
                        0,
                        math.ceil((float(sym_setup) - float(baseline_setup)) / denominator),
                    )
                else:
                    result["break_even_tasks"] = "no break-even observed"
    return result


def provider_total_tokens(record: dict[str, Any]) -> tuple[int | None, str | None]:
    explicit = record.get("provider_total_tokens")
    if isinstance(explicit, int) and not isinstance(explicit, bool) and explicit >= 0:
        return explicit, "explicit_provider_total_tokens"
    def usage_total(usage: object) -> tuple[int, str] | None:
        if not isinstance(usage, dict):
            return None
        for total_key in ("total_tokens", "totalTokens"):
            value = usage.get(total_key)
            if isinstance(value, int) and not isinstance(value, bool) and value >= 0:
                return value, total_key
        snake = (
            "input_tokens",
            "output_tokens",
            "cache_creation_input_tokens",
            "cache_read_input_tokens",
        )
        camel = (
            "inputTokens",
            "outputTokens",
            "cacheCreationInputTokens",
            "cacheReadInputTokens",
        )
        if any(key in usage for key in snake):
            keys = snake
        elif any(key in usage for key in camel):
            keys = camel
        else:
            return None
        values = [usage.get(key, 0) for key in keys]
        if not all(
            isinstance(value, int) and not isinstance(value, bool) and value >= 0
            for value in values
        ):
            return None
        return sum(int(value) for value in values), "documented_disjoint_usage_fields"

    aggregate = usage_total(record.get("usage"))
    if aggregate is not None:
        return aggregate[0], f"usage.{aggregate[1]}"

    model_usage = record.get("modelUsage")
    if not isinstance(model_usage, dict) or not model_usage:
        return None, None
    model_totals: list[int] = []
    bases: set[str] = set()
    for usage in model_usage.values():
        parsed = usage_total(usage)
        if parsed is None:
            return None, None
        model_totals.append(parsed[0])
        bases.add(parsed[1])
    return sum(model_totals), "modelUsage." + "+".join(sorted(bases))


def paired_provider_summary(
    artifacts: Sequence[Artifact],
    cases_by_id: dict[str, dict[str, Any]],
    evaluator: dict[str, Any],
) -> dict[str, Any]:
    grouped: dict[tuple[str, str], list[dict[str, Any]]] = {}
    for artifact in artifacts:
        for record in artifact.records:
            if record.get("record_type") != "claude_task_trial":
                continue
            case_id = record.get("case_id")
            arm = record.get("arm")
            if case_id in cases_by_id and arm in {"baseline", "symforge"}:
                grouped.setdefault((str(case_id), str(arm)), []).append(record)
    by_tool: dict[str, dict[str, Any]] = {}
    for case_id, case in cases_by_id.items():
        baseline_rows = grouped.get((case_id, "baseline"), [])
        symforge_rows = grouped.get((case_id, "symforge"), [])
        if not baseline_rows and not symforge_rows:
            continue
        decision = evaluator.get("cases", {}).get(case_id, {})
        paired_decision = decision.get("paired_arms", {}) if isinstance(decision, dict) else {}
        baseline_decision = paired_decision.get("baseline", {})
        symforge_decision = paired_decision.get("symforge", {})
        baseline_ok = isinstance(baseline_decision, dict) and str(
            baseline_decision.get("status", "manual_pending")
        ).lower() == "pass"
        symforge_ok = isinstance(symforge_decision, dict) and str(
            symforge_decision.get("status", "manual_pending")
        ).lower() == "pass"
        baseline_tokens: list[int] = []
        symforge_tokens: list[int] = []
        bases: set[str] = set()
        for row in baseline_rows:
            value, basis = provider_total_tokens(row)
            if value is not None:
                baseline_tokens.append(value)
            if basis:
                bases.add(basis)
        for row in symforge_rows:
            value, basis = provider_total_tokens(row)
            if value is not None:
                symforge_tokens.append(value)
            if basis:
                bases.add(basis)
        if not baseline_ok or not symforge_ok:
            verdict = "INVALID_INCORRECT" if (
                (isinstance(baseline_decision, dict) and baseline_decision.get("status") == "fail")
                or (isinstance(symforge_decision, dict) and symforge_decision.get("status") == "fail")
            ) else "NOT_SCORED"
            result = {
                "case_id": case_id,
                "verdict": verdict,
                "reason": "paired_correctness_not_passed",
                "baseline_samples": len(baseline_tokens),
                "symforge_samples": len(symforge_tokens),
            }
        elif not baseline_tokens or not symforge_tokens:
            result = {
                "case_id": case_id,
                "verdict": "NOT_SCORED",
                "reason": "provider_total_missing",
                "baseline_samples": len(baseline_tokens),
                "symforge_samples": len(symforge_tokens),
            }
        else:
            baseline_total = sum(baseline_tokens)
            symforge_total = sum(symforge_tokens)
            verdict, saved, percent, multiplier = numeric_verdict(
                float(baseline_total), float(symforge_total)
            )
            result = {
                "case_id": case_id,
                "verdict": verdict,
                "reason": "paired_provider_total",
                "baseline_samples": len(baseline_tokens),
                "symforge_samples": len(symforge_tokens),
                "baseline_tokens": baseline_total,
                "symforge_tokens": symforge_total,
                "saved_tokens": saved,
                "savings_percent": percent,
                "cost_multiplier": multiplier,
                "usage_basis": sorted(bases),
                "negative_trial_rate": None,
            }
        tool = case["primary_tool"]
        by_tool.setdefault(tool, {"headline": True, "cases": []})["cases"].append(
            result
        )
    return by_tool


def aggregate_units(
    trials: Sequence[Trial],
    schema_views: dict[str, dict[str, Any]],
) -> dict[str, dict[str, Any]]:
    samples: dict[str, list[dict[str, Any]]] = {}
    for trial in trials:
        for unit in unit_rows(trial):
            metrics = rows_metrics(unit["rows"], trial.case["surface"], schema_views)
            if metrics is None:
                continue
            samples.setdefault(trial.case["primary_tool"], []).append(
                {
                    "case_id": trial.case["id"],
                    "run_id_sha256": sha256_bytes(trial.run_id.encode()),
                    "unit": unit["name"],
                    "kind": unit["kind"],
                    "surface": trial.case["surface"],
                    **metrics,
                }
            )
    result: dict[str, dict[str, Any]] = {}
    for tool, tool_samples in samples.items():
        encodings: dict[str, Any] = {}
        for encoding in ENCODINGS:
            keys = (
                "request",
                "response",
                "direct_payload",
                "cold_eager_surface",
                "lazy_unique",
                "theoretical_amortized_5",
                "theoretical_amortized_20",
            )
            encoding_summary: dict[str, Any] = {}
            for key in keys:
                values = [
                    float(sample["encodings"][encoding][key])
                    for sample in tool_samples
                ]
                encoding_summary[key] = {
                    "total": sum(values),
                    "median": median(values),
                    "p95": percentile(values, 0.95),
                }
            encodings[encoding] = encoding_summary
        latency = [float(sample["latency_ms"]) for sample in tool_samples]
        result[tool] = {
            "sample_count": len(tool_samples),
            "units": tool_samples,
            "encodings": encodings,
            "latency_ms": {
                "median": median(latency),
                "p95": percentile(latency, 0.95),
            },
        }
    return result


def summarize_tools(
    trials: Sequence[Trial],
    schema_views: dict[str, dict[str, Any]],
    evaluator: dict[str, Any],
    paired: dict[str, Any],
) -> list[dict[str, Any]]:
    units = aggregate_units(trials, schema_views)
    grouped: dict[str, list[Trial]] = {}
    for trial in trials:
        grouped.setdefault(trial.case["primary_tool"], []).append(trial)
    rows: list[dict[str, Any]] = []
    for tool in sorted(grouped):
        tool_trials = grouped[tool]
        by_case: dict[str, list[Trial]] = {}
        for trial in tool_trials:
            by_case.setdefault(trial.case["id"], []).append(trial)
        case_results: list[dict[str, Any]] = []
        numeric_recipe_verdicts: list[str] = []
        for case_id, case_trials in sorted(by_case.items()):
            decision = evaluator.get("cases", {}).get(case_id)
            correctness, decision_summary = final_correctness(case_trials, decision)
            economics = [
                recipe_economics(
                    trial,
                    correctness,
                    decision if isinstance(decision, dict) else None,
                    schema_views,
                )
                for trial in case_trials
            ]
            numeric_recipe_verdicts.extend(
                item["verdict"]
                for item in economics
                if item.get("verdict") in {"POSITIVE", "TOKEN_NEGATIVE", "NEUTRAL"}
            )
            case_results.append(
                {
                    "case_id": case_id,
                    "correctness": correctness,
                    "automatic_failure_codes": sorted(
                        {
                            code
                            for trial in case_trials
                            for code in trial.automatic_failure_codes
                        }
                    ),
                    "evaluator": decision_summary,
                    "repetitions": len(case_trials),
                    "direct_recipe_sensitivity": economics,
                }
            )
        statuses = {item["correctness"] for item in case_results}
        if "FAIL" in statuses:
            correctness = "FAIL"
        elif statuses == {"PASS"}:
            correctness = "PASS"
        else:
            correctness = "UNEVALUATED"
        negative = sum(item == "TOKEN_NEGATIVE" for item in numeric_recipe_verdicts)
        recipe_rate = (
            negative / len(numeric_recipe_verdicts)
            if numeric_recipe_verdicts
            else None
        )
        rows.append(
            {
                "tool": tool,
                "correctness": correctness,
                "cases": case_results,
                "direct_metrics": units.get(tool),
                "direct_recipe_sensitivity": {
                    "headline": False,
                    "valid_numeric_trials": len(numeric_recipe_verdicts),
                    "negative_trial_rate": recipe_rate,
                },
                "paired_provider": paired.get(
                    tool,
                    {
                        "headline": True,
                        "cases": [],
                        "status": "not_run",
                    },
                ),
            }
        )
    return rows


def validation_result(
    artifact_paths: Sequence[pathlib.Path],
    manifest_paths: Sequence[pathlib.Path],
    cases_path: pathlib.Path,
    asset_lock_path: pathlib.Path,
    evaluator_path: pathlib.Path | None,
) -> tuple[dict[str, Any], dict[str, Any]]:
    issues: list[Issue] = []
    cases, cases_by_id = load_cases(cases_path)
    asset_lock = load_json_object(asset_lock_path, "asset lock")
    artifacts = [load_records(path) for path in artifact_paths]
    manifest_artifacts = [load_records(path) for path in manifest_paths]
    evaluator = load_evaluator(evaluator_path)
    validate_decisions(evaluator, cases_by_id, issues)
    tokenizer = tokenizer_status(issues)
    counter = harness.TokenCounter()
    schema_views = manifest_schema_views(manifest_artifacts, cases, counter, issues)
    asset_identity = validate_asset_identity(
        cases_path, asset_lock, artifacts, issues
    )
    direct_trials = collect_direct_trials(artifacts, cases_by_id, issues)
    validate_repetitions(direct_trials, issues)
    paired_records = sum(
        row.get("record_type") == "claude_task_trial"
        for artifact in artifacts
        for row in artifact.records
    )
    if not direct_trials:
        issues.append(
            Issue(
                "error",
                "DIRECT_TRIALS_MISSING",
                "no direct campaign trials were supplied",
            )
        )
    report = {
        "schema_version": SUMMARY_SCHEMA,
        "status": "valid"
        if not any(issue.severity == "error" for issue in issues)
        else "invalid",
        "protocol": harness.PROTOCOL_ID,
        "artifact_count": len(artifacts),
        "artifact_identities": [
            {"name": artifact.path.name, "sha256": artifact.sha256}
            for artifact in artifacts
        ],
        "manifest_artifact_count": len(manifest_artifacts),
        "direct_trial_count": len(direct_trials),
        "paired_record_count": paired_records,
        "asset_identity": asset_identity,
        "tokenizer": tokenizer,
        "schema_views": {
            surface: {
                "manifest_sha256": view["manifest_sha256"],
                "tool_count": view["tool_count"],
                "tokens": view["tokens"],
            }
            for surface, view in schema_views.items()
        },
        "issues": [issue.as_dict() for issue in issues],
    }
    state = {
        "cases": cases,
        "cases_by_id": cases_by_id,
        "artifacts": artifacts,
        "manifest_artifacts": manifest_artifacts,
        "evaluator": evaluator,
        "schema_views": schema_views,
        "trials": direct_trials,
        "issues": issues,
    }
    return report, state


def build_summary(validation: dict[str, Any], state: dict[str, Any]) -> dict[str, Any]:
    if validation["status"] != "valid":
        raise AdjudicationError(
            "artifacts are incomplete or invalid; run validate and fix every error"
        )
    paired = paired_provider_summary(
        state["artifacts"], state["cases_by_id"], state["evaluator"]
    )
    tools = summarize_tools(
        state["trials"], state["schema_views"], state["evaluator"], paired
    )
    return {
        "schema_version": SUMMARY_SCHEMA,
        "protocol": harness.PROTOCOL_ID,
        "status": "summarized",
        "correctness_rule": (
            "case_complete is capture-only; PASS requires a separate evidenced "
            "evaluator decision"
        ),
        "validation": validation,
        "economics": {
            "headline": "paired_provider",
            "sensitivity": "direct_recipe",
            "schema_views": {
                surface: {
                    "tool_count": view["tool_count"],
                    "tokens": view["tokens"],
                    "manifest_sha256": view["manifest_sha256"],
                }
                for surface, view in state["schema_views"].items()
            },
            "views": [
                "content_only",
                "direct_payload",
                "cold_eager_surface",
                "lazy_unique",
                "theoretical_amortized_5",
                "theoretical_amortized_20",
                "paired_provider_total",
            ],
        },
        "tools": tools,
    }


def markdown_number(value: Any, digits: int = 1) -> str:
    if value is None:
        return "N/A"
    if isinstance(value, float):
        return f"{value:.{digits}f}"
    return str(value)


def markdown_summary(summary: dict[str, Any]) -> str:
    lines = [
        "# SymForge benchmark adjudication summary",
        "",
        "> `case_complete` is capture-only. Correctness requires a separate "
        "evidenced evaluator decision.",
        "",
        "## Schema views",
        "",
        "| Surface | Tools | cl100k | o200k |",
        "|---|---:|---:|---:|",
    ]
    for surface in ("full", "compact", "meta"):
        view = summary["economics"]["schema_views"].get(surface)
        if not isinstance(view, dict):
            continue
        lines.append(
            f"| `{surface}` | {view['tool_count']} | "
            f"{view['tokens']['cl100k']} | {view['tokens']['o200k']} |"
        )
    lines.extend(
        [
            "",
            "## Per-tool results",
            "",
            "Direct recipe results are sensitivity measurements. Paired provider "
            "usage is the headline only after both arms pass the same oracle.",
            "",
            "| Tool | Correctness | Samples | Direct cl100k median | "
            "Latency median / p95 ms | Recipe negative rate | Paired status |",
            "|---|---|---:|---:|---:|---:|---|",
        ]
    )
    for tool in summary["tools"]:
        metrics = tool.get("direct_metrics") or {}
        encoding = metrics.get("encodings", {}).get("cl100k", {})
        payload = encoding.get("direct_payload", {}).get("median")
        latency = metrics.get("latency_ms", {})
        recipe_rate = tool["direct_recipe_sensitivity"].get("negative_trial_rate")
        paired_cases = tool.get("paired_provider", {}).get("cases", [])
        if paired_cases:
            paired_verdicts = sorted({case.get("verdict") for case in paired_cases})
            paired_status = ", ".join(str(value) for value in paired_verdicts)
        else:
            paired_status = "not run"
        lines.append(
            f"| `{tool['tool']}` | {tool['correctness']} | "
            f"{metrics.get('sample_count', 0)} | {markdown_number(payload)} | "
            f"{markdown_number(latency.get('median'))} / "
            f"{markdown_number(latency.get('p95'))} | "
            f"{markdown_number(recipe_rate, 3)} | {paired_status} |"
        )
    lines.extend(
        [
            "",
            "## Interpretation",
            "",
            "- `PASS` requires evidenced evaluator decisions; capture alone remains "
            "`UNEVALUATED`.",
            "- `INVALID_INCORRECT` takes precedence over token economics.",
            "- Capability-only and lower-bound baselines are "
            "`N/A_NO_EQUIVALENT_BASELINE`, never zero-token baselines.",
            "- Cold eager and 5/20-task schema figures are theoretical unless exact "
            "model-visible schemas were captured longitudinally.",
            "- Provider totals already containing schema/cache/reasoning are never "
            "augmented a second time.",
            "",
        ]
    )
    return "\n".join(lines)


def common_arguments(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--artifact", action="append", required=True)
    parser.add_argument("--manifest", action="append", required=True)
    parser.add_argument("--cases", default=str(DEFAULT_CASES))
    parser.add_argument("--asset-lock", default=str(DEFAULT_ASSET_LOCK))
    parser.add_argument("--evaluator")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Validate and summarize sanitized SymForge benchmark JSONL without "
            "inferring correctness from runner completion."
        )
    )
    subparsers = parser.add_subparsers(dest="command", required=True)
    validate = subparsers.add_parser(
        "validate", help="Validate identity, manifests, records, warmups, and repetitions."
    )
    common_arguments(validate)
    summarize = subparsers.add_parser(
        "summarize", help="Validate, then write sanitized JSON and Markdown summaries."
    )
    common_arguments(summarize)
    summarize.add_argument("--output-json", required=True)
    summarize.add_argument("--output-markdown", required=True)
    subparsers.add_parser("self-test", help="Run synthetic sanitized evidence tests.")
    return parser


def paths(values: Sequence[str]) -> list[pathlib.Path]:
    return [pathlib.Path(value).resolve() for value in values]


def run_cli(args: argparse.Namespace) -> int:
    validation, state = validation_result(
        paths(args.artifact),
        paths(args.manifest),
        pathlib.Path(args.cases).resolve(),
        pathlib.Path(args.asset_lock).resolve(),
        pathlib.Path(args.evaluator).resolve() if args.evaluator else None,
    )
    safe_validation = sanitize_output(validation)
    if args.command == "validate":
        print(json.dumps(safe_validation, indent=2, sort_keys=True))
        return 0 if validation["status"] == "valid" else 1
    summary = sanitize_output(build_summary(validation, state))
    output_json = pathlib.Path(args.output_json).resolve()
    output_markdown = pathlib.Path(args.output_markdown).resolve()
    if output_json == output_markdown:
        raise AdjudicationError("JSON and Markdown outputs must be different paths")
    write_json(output_json, summary)
    output_markdown.parent.mkdir(parents=True, exist_ok=True)
    output_markdown.write_text(markdown_summary(summary), encoding="utf-8")
    receipt = sanitize_output(
        {
            "status": "summarized",
            "tools": len(summary["tools"]),
            "output_json": str(output_json),
            "output_markdown": str(output_markdown),
        }
    )
    print(json.dumps(receipt, indent=2, sort_keys=True))
    return 0


def temp_write_json(path: pathlib.Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, sort_keys=True) + "\n", encoding="utf-8")


def temp_write_jsonl(path: pathlib.Path, rows: Sequence[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        "".join(canonical_json(row) + "\n" for row in rows), encoding="utf-8"
    )


def synthetic_manifest(surface: str, names: Sequence[str]) -> dict[str, Any]:
    tools = [
        {
            "name": name,
            "description": "Synthetic read-only benchmark tool.",
            "inputSchema": {"type": "object", "properties": {}},
        }
        for name in names
    ]
    return {
        "protocol": harness.PROTOCOL_ID,
        "record_type": "manifest",
        "surface": surface,
        "manifest": {
            "tools": tools,
            "resources": [],
            "resourceTemplates": [],
            "prompts": [],
        },
    }


def synthetic_campaign_rows(
    run_id: str,
    cases_hash: str,
    campaign_hash: str,
) -> list[dict[str, Any]]:
    case_id = "SELF-get_repo_map-001"
    return [
        {
            "protocol": harness.PROTOCOL_ID,
            "record_type": "campaign_start",
            "run_id": run_id,
            "input_hashes": {"cases": cases_hash, "campaign": campaign_hash},
            "selected_case_ids": [case_id],
            "formal_economics": True,
        },
        {
            "protocol": harness.PROTOCOL_ID,
            "record_type": "case_start",
            "run_id": run_id,
            "case_id": case_id,
            "surface": "full",
        },
        {
            "protocol": harness.PROTOCOL_ID,
            "record_type": "source_inventory_before",
            "run_id": run_id,
            "case_id": case_id,
            "inventory": {"tree_sha256": "0" * 64},
        },
        {
            "protocol": harness.PROTOCOL_ID,
            "record_type": "direct_trial",
            "run_id": run_id,
            "case_id": case_id,
            "case_step": 1,
            "request_label": "actual",
            "tool": "get_repo_map",
            "status": "ok",
            "rpc_ms": 5.0,
            "schema_free_tool_request_cl100k": 10,
            "schema_free_tool_response_cl100k": 20,
            "schema_free_direct_payload_cl100k": 30,
            "schema_free_tool_request_o200k": 9,
            "schema_free_tool_response_o200k": 19,
            "schema_free_direct_payload_o200k": 28,
        },
        {
            "protocol": harness.PROTOCOL_ID,
            "record_type": "source_inventory_after",
            "run_id": run_id,
            "case_id": case_id,
            "inventory": {"tree_sha256": "0" * 64},
            "changes": [],
            "mutation_policy": {"safe": True, "violations": []},
        },
        {
            "protocol": harness.PROTOCOL_ID,
            "record_type": "baseline_total",
            "run_id": run_id,
            "case_id": case_id,
            "direct_payload_cl100k": 50,
            "direct_payload_o200k": 45,
            "baseline_equivalence": "equivalent",
        },
        {
            "protocol": harness.PROTOCOL_ID,
            "record_type": "case_complete",
            "run_id": run_id,
            "case_id": case_id,
            "status": "completed",
        },
        {
            "protocol": harness.PROTOCOL_ID,
            "record_type": "campaign_complete",
            "run_id": run_id,
            "selected": 1,
            "completed": 1,
            "failed": 0,
        },
    ]


def synthetic_repeated_campaign_rows(
    run_id: str,
    cases_hash: str,
    campaign_hash: str,
) -> list[dict[str, Any]]:
    """Mirror production: one selection with two ordered trial windows."""
    base = synthetic_campaign_rows(run_id, cases_hash, campaign_hash)
    rows = [dict(base[0])]
    repeated_types = {"case_start", "direct_trial", "case_complete", "case_error"}
    for ordinal in (1, 2):
        for source in base[1:-1]:
            record = dict(source)
            if record.get("record_type") in repeated_types:
                record["trial_ordinal"] = ordinal
                record["measured_repetition"] = ordinal
                record["discarded_warmup"] = False
            rows.append(record)
    campaign_complete = dict(base[-1])
    campaign_complete["completed"] = 2
    rows.append(campaign_complete)
    return rows


def self_test() -> int:
    with tempfile.TemporaryDirectory(prefix="sfbench-adjudicator-") as root_text:
        root = pathlib.Path(root_text)
        cases_path = root / "cases.json"
        campaign_path = root / "campaign.json"
        asset_lock_path = root / "assets.lock.json"
        evaluator_path = root / "evaluator.json"
        manifest_path = root / "manifest.jsonl"
        case = {
            "id": "SELF-get_repo_map-001",
            "primary_tool": "get_repo_map",
            "case_kind": "happy",
            "repo": "synthetic",
            "language": "Python",
            "surface": "full",
            "requests": [
                {
                    "step": 1,
                    "label": "actual",
                    "tool": "get_repo_map",
                    "args": {},
                }
            ],
            "baseline_recipe": [{"executor": "shell"}],
            "baseline_equivalence": "equivalent",
            "limits": {
                "call_limit": 1,
                "timeout_seconds": 10,
                "repetitions": 2,
                "discarded_warmups": 0,
            },
            "mutation_allowlist": [],
            "source_hash_policy": "no_source_bytes_may_change",
        }
        cases = {
            "protocol": harness.PROTOCOL_ID,
            "inventory": {
                "full_surface_tools": ["get_repo_map"],
                "full_surface_count": 1,
                "compact_surface_tools": ["status", "symforge", "symforge_edit"],
                "meta_surface_tools": ["symforge"],
                "unique_tool_names": 4,
            },
            "cases": [case],
        }
        temp_write_json(cases_path, cases)
        campaign = {"protocol": harness.PROTOCOL_ID, "name": "synthetic"}
        temp_write_json(campaign_path, campaign)
        cases_hash = file_sha256(cases_path)
        campaign_hash = file_sha256(campaign_path)
        asset_lock = {
            "assets": [
                {"path": "cases.json", "sha256": cases_hash},
                {"path": "campaign.json", "sha256": campaign_hash},
            ]
        }
        temp_write_json(asset_lock_path, asset_lock)
        evaluator = {
            "schema_version": EVALUATOR_SCHEMA,
            "cases": {
                case["id"]: {
                    "status": "pass",
                    "checks": [
                        {
                            "id": "inventory",
                            "status": "pass",
                            "evidence": ["synthetic-oracle:inventory"],
                        }
                    ],
                    "baseline_equivalence": "valid",
                    "baseline_correctness": "pass",
                }
            },
        }
        temp_write_json(evaluator_path, evaluator)
        temp_write_jsonl(
            manifest_path,
            [
                synthetic_manifest("full", ["get_repo_map"]),
                synthetic_manifest("compact", ["status", "symforge", "symforge_edit"]),
                synthetic_manifest("meta", ["symforge"]),
            ],
        )
        repeated_path = root / "repeated-trials.jsonl"
        temp_write_jsonl(
            repeated_path,
            synthetic_repeated_campaign_rows(
                "self-test-repeated", cases_hash, campaign_hash
            ),
        )
        artifact_paths = [repeated_path]

        validation, state = validation_result(
            artifact_paths,
            [manifest_path],
            cases_path,
            asset_lock_path,
            evaluator_path,
        )
        if validation["status"] != "valid":
            raise AdjudicationError("self-test validation did not pass")
        if validation["direct_trial_count"] != 2 or len(state["trials"]) != 2:
            raise AdjudicationError("self-test repeated trial windows were collapsed")
        summary = sanitize_output(build_summary(validation, state))
        tool = summary["tools"][0]
        if tool["tool"] != "get_repo_map" or tool["correctness"] != "PASS":
            raise AdjudicationError("self-test correctness summary failed")
        if tool["direct_metrics"]["sample_count"] != 2:
            raise AdjudicationError("self-test repeated samples were not aggregated")
        economics = tool["cases"][0]["direct_recipe_sensitivity"]
        if not economics or any(item["verdict"] != "POSITIVE" for item in economics):
            raise AdjudicationError("self-test recipe economics failed")
        without_evaluator, state_without = validation_result(
            artifact_paths,
            [manifest_path],
            cases_path,
            asset_lock_path,
            None,
        )
        if without_evaluator["status"] != "valid":
            raise AdjudicationError("self-test unevaluated validation failed")
        unevaluated = build_summary(without_evaluator, state_without)
        if unevaluated["tools"][0]["correctness"] != "UNEVALUATED":
            raise AdjudicationError("self-test inferred correctness without decisions")
        if any(
            item["verdict"] != "NOT_SCORED"
            for item in unevaluated["tools"][0]["cases"][0][
                "direct_recipe_sensitivity"
            ]
        ):
            raise AdjudicationError("self-test scored unevaluated economics")
        rendered = markdown_summary(summary)
        if "case_complete` is capture-only" not in rendered:
            raise AdjudicationError("self-test Markdown warning is absent")
    print("adjudicate-results self-test: PASS")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        if args.command == "self-test":
            return self_test()
        return run_cli(args)
    except AdjudicationError as exc:
        print(f"adjudication error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
