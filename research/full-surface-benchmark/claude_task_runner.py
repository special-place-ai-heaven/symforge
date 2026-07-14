# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "tiktoken>=0.7.0,<1",
# ]
# ///
"""Run one resolved SFBENCH task through Claude Code 2.1.207.

The runner persists one sanitized JSONL record.  It deliberately sends the task
prompt over stdin and never serializes the process environment or auth state.
"""

from __future__ import annotations

import argparse
import contextlib
import fnmatch
import hashlib
import io
import json
import os
import pathlib
import queue
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time
from dataclasses import dataclass
from decimal import Decimal, InvalidOperation
from typing import Any

from mcp_harness import (
    REDACTED,
    Sanitizer,
    TokenCounter,
    canonical_json,
    is_within,
    sha256_text,
)


PROTOCOL_ID = "SFBENCH-1.0"
REQUIRED_CLAUDE_VERSION = "2.1.207"
BUILTIN_TOOLS = ("Bash", "Read", "Grep", "Glob", "Edit", "Write")
MCP_PERMISSION = "mcp__symforge__*"
RUNTIME_PATH_PREFIXES = (".symforge/",)
DOLLAR_PLACEHOLDER = re.compile(r"\$\{([^{}]+)\}")
RUNTIME_CASE_FIELDS = (
    "id",
    "task_prompt",
    "model_policy",
    "token_budget",
    "allowed_helpers",
    "forbidden_tools",
    "mutation_allowlist",
    "limits",
    "timeout_seconds",
    "call_limit",
    "transport",
    "surface",
    "cache_state",
    "execution_mode",
    "source_hash_policy",
)
ANSWER_EXAMPLE = {"answer": "...", "evidence": [{"path": "...", "line": 1}]}
SAFE_ENV_NAMES = {
    "APPDATA",
    "COMSPEC",
    "HOME",
    "LANG",
    "LC_ALL",
    "LOCALAPPDATA",
    "PATH",
    "PATHEXT",
    "PROGRAMDATA",
    "SystemRoot",
    "TEMP",
    "TERM",
    "TMP",
    "USERPROFILE",
    "WINDIR",
}
CREDENTIAL_ENV_PREFIXES = (
    "ANTHROPIC_",
    "AWS_",
    "AZURE_",
    "BEDROCK_",
    "CLAUDE_API_",
    "CLOUD_",
    "FOUNDRY_",
    "GCP_",
    "GH_",
    "GITHUB_",
    "GOOGLE_",
    "HF_",
    "OPENAI_",
    "VERTEX_",
)
ANSWER_SCHEMA: dict[str, Any] = {
    "type": "object",
    "additionalProperties": False,
    "required": ["answer", "evidence"],
    "properties": {
        "answer": {"type": "string"},
        "evidence": {
            "type": "array",
            "items": {
                "type": "object",
                "additionalProperties": False,
                "required": ["path", "line"],
                "properties": {
                    "path": {"type": "string"},
                    "line": {"type": "integer", "minimum": 1},
                },
            },
        },
    },
}


class RunnerError(RuntimeError):
    """Expected failure whose message contains no child output or credentials."""


@dataclass(frozen=True)
class TrialConfig:
    case: dict[str, Any]
    arm: str
    repo: pathlib.Path
    benchmark_root: pathlib.Path
    output: pathlib.Path
    claude: pathlib.Path
    claude_prefix_args: tuple[str, ...]
    symforge: pathlib.Path | None
    max_budget_usd: str
    prompt_mode: str = "neutral"


@dataclass(frozen=True)
class GitState:
    available: bool
    status_sha256: str | None
    status_count: int | None
    diff_sha256: str | None
    paths: tuple[str, ...]
    path_hashes: dict[str, str]


@dataclass
class StreamCapture:
    exit_code: int | None
    elapsed_ms: float
    timed_out: bool
    call_limit_exceeded: bool
    stdout_utf8_bytes: int
    stderr_utf8_bytes: int
    stderr_safe: str
    events_safe: list[Any]
    invalid_event_count: int
    tool_uses: list[dict[str, Any]]
    tool_results: list[dict[str, Any]]
    assistant_turns: list[dict[str, Any]]
    final_result_raw: dict[str, Any] | None
    final_result_safe: dict[str, Any] | None


def hash_file(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while chunk := handle.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def resolve_executable(value: str) -> pathlib.Path:
    candidate = pathlib.Path(value).expanduser()
    if candidate.is_absolute() or candidate.parent != pathlib.Path("."):
        resolved = candidate.resolve()
        if not resolved.is_file():
            raise RunnerError("a supplied executable does not exist")
        return resolved
    found = shutil.which(value)
    if found is None:
        raise RunnerError("a supplied executable was not found on PATH")
    return pathlib.Path(found).resolve()


def decimal_budget(value: str) -> str:
    try:
        parsed = Decimal(value)
    except InvalidOperation as exc:
        raise RunnerError("--max-budget-usd must be a positive decimal") from exc
    if not parsed.is_finite() or parsed <= 0:
        raise RunnerError("--max-budget-usd must be a positive decimal")
    return format(parsed, "f")


def find_placeholders(value: Any, path: str = "case") -> list[str]:
    found: list[str] = []
    if isinstance(value, dict):
        for key, child in value.items():
            key_text = str(key)
            found.extend(find_placeholders(child, f"{path}.{key_text}"))
    elif isinstance(value, list):
        for index, child in enumerate(value):
            found.extend(find_placeholders(child, f"{path}[{index}]"))
    elif isinstance(value, str):
        stripped = value.strip()
        unresolved = (
            bool(re.fullmatch(r"<[^<>]+>", stripped))
            or "${" in value
            or "{{" in value
            or bool(re.fullmatch(r"__[A-Za-z0-9_-]+__", stripped))
            or stripped.lower()
            in {"placeholder", "replace_me", "tbd", "todo", "unresolved"}
        )
        if unresolved:
            found.append(path)
    return found


def neutral_prompt_names_tool(prompt: str, primary_tool: str | None) -> bool:
    if re.search(r"(?i)\bsymforge\b|mcp__symforge__", prompt):
        return True
    if not primary_tool:
        return False
    escaped = re.escape(primary_tool)
    if "_" in primary_tool and re.search(rf"(?i)(?<!\w){escaped}(?!\w)", prompt):
        return True
    return bool(
        re.search(rf"(?i)`{escaped}`", prompt)
        or re.search(
            rf"(?i)\b(?:use|call|invoke)\s+(?:the\s+)?[`'\"]?{escaped}(?!\w)",
            prompt,
        )
    )


def select_task_prompt(case: dict[str, Any], prompt_mode: str) -> str:
    if prompt_mode == "forced":
        prompt = case.get("task_prompt")
        if not isinstance(prompt, str) or not prompt.strip():
            raise RunnerError("forced prompt mode requires task_prompt")
        return prompt
    if prompt_mode != "neutral":
        raise RunnerError("unsupported prompt mode")

    explicit = case.get("neutral_task_prompt")
    claim = case.get("claim_under_test")
    if isinstance(explicit, str) and explicit.strip():
        prompt = explicit
    elif isinstance(claim, str) and claim.strip():
        prompt = (
            "Test this repository claim using any allowed local tools: "
            f"{claim.strip()} Return exactly {canonical_json(ANSWER_EXAMPLE)}."
        )
    else:
        # A single-case artifact may already contain a resolved neutral prompt.
        prompt = case.get("task_prompt")
        if not isinstance(prompt, str) or not prompt.strip():
            raise RunnerError(
                "neutral prompt mode requires neutral_task_prompt or claim_under_test"
            )
    primary_tool = case.get("primary_tool")
    primary_name = primary_tool if isinstance(primary_tool, str) else None
    if neutral_prompt_names_tool(prompt, primary_name):
        raise RunnerError("neutral task prompt names SymForge or the primary tool")
    return prompt


def load_json_object(path: pathlib.Path, label: str) -> dict[str, Any]:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except OSError as exc:
        raise RunnerError(f"could not read the {label} JSON") from exc
    except json.JSONDecodeError as exc:
        raise RunnerError(
            f"{label} JSON is invalid at line {exc.lineno}, column {exc.colno}"
        ) from exc
    if not isinstance(document, dict):
        raise RunnerError(f"{label} JSON must contain an object")
    return document


def tree_lookup(value: dict[str, Any], dotted: str) -> Any:
    current: Any = value
    for part in dotted.split("."):
        if not isinstance(current, dict) or part not in current:
            raise KeyError(dotted)
        current = current[part]
    return current


def manifest_repository(corpus_manifest: dict[str, Any], alias: str) -> dict[str, Any]:
    repositories = corpus_manifest.get("repositories")
    if not isinstance(repositories, list):
        raise RunnerError("corpus manifest repository list is malformed")
    matches = [
        item
        for item in repositories
        if isinstance(item, dict) and item.get("alias") == alias
    ]
    if len(matches) != 1:
        raise RunnerError(f"corpus manifest has no unique repository alias {alias}")
    return matches[0]


def resolve_case_placeholder(
    name: str,
    field_path: str,
    *,
    campaign: dict[str, Any] | None,
    fixture_oracle: dict[str, Any] | None,
    corpus_manifest: dict[str, Any] | None,
    corpus_manifest_path: pathlib.Path | None,
) -> Any:
    if name in {"session.model", "session.seed"}:
        if not field_path.startswith("case.model_policy"):
            raise RunnerError("session placeholders are allowed only in model_policy")
        if campaign is None:
            raise RunnerError(f"--campaign is required to resolve {name}")
        if name == "session.seed":
            return None
        model = campaign.get("paired_llm", {}).get("model_alias")
        if not isinstance(model, str) or not model.strip():
            raise RunnerError("campaign paired_llm.model_alias is missing")
        return model

    if not field_path.startswith("case.mutation_allowlist"):
        raise RunnerError(
            "external placeholders are allowed only in mutation_allowlist"
        )
    if name.startswith("fixture.oracle."):
        if fixture_oracle is None:
            raise RunnerError(f"--fixture-oracle is required to resolve {name}")
        try:
            return tree_lookup(fixture_oracle, name.removeprefix("fixture.oracle."))
        except KeyError as exc:
            raise RunnerError(f"unknown fixture oracle placeholder: {name}") from exc
    if name.startswith("repo."):
        if corpus_manifest is None:
            raise RunnerError(f"--corpus-manifest is required to resolve {name}")
        parts = name.split(".", 2)
        if len(parts) != 3 or parts[2] not in {"commit", "root"}:
            raise RunnerError(f"unknown repository placeholder: {name}")
        repository = manifest_repository(corpus_manifest, parts[1])
        if parts[2] == "commit":
            if not isinstance(repository.get("commit"), str):
                raise RunnerError(f"repository commit is missing for alias {parts[1]}")
            return repository["commit"]
        if corpus_manifest_path is None:
            raise RunnerError(f"--corpus-manifest is required to resolve {name}")
        return str(corpus_manifest_path.resolve().parent / "sources" / parts[1])
    if name.startswith("corpus."):
        if corpus_manifest is None:
            raise RunnerError(f"--corpus-manifest is required to resolve {name}")
        try:
            return tree_lookup(corpus_manifest, name.removeprefix("corpus."))
        except KeyError as exc:
            raise RunnerError(f"unknown corpus manifest placeholder: {name}") from exc
    raise RunnerError(f"unknown placeholder: {name}")


def resolve_case_value(
    value: Any,
    field_path: str,
    *,
    campaign: dict[str, Any] | None,
    fixture_oracle: dict[str, Any] | None,
    corpus_manifest: dict[str, Any] | None,
    corpus_manifest_path: pathlib.Path | None,
) -> Any:
    if isinstance(value, dict):
        return {
            key: resolve_case_value(
                child,
                f"{field_path}.{key}",
                campaign=campaign,
                fixture_oracle=fixture_oracle,
                corpus_manifest=corpus_manifest,
                corpus_manifest_path=corpus_manifest_path,
            )
            for key, child in value.items()
        }
    if isinstance(value, list):
        return [
            resolve_case_value(
                child,
                f"{field_path}[{index}]",
                campaign=campaign,
                fixture_oracle=fixture_oracle,
                corpus_manifest=corpus_manifest,
                corpus_manifest_path=corpus_manifest_path,
            )
            for index, child in enumerate(value)
        ]
    if not isinstance(value, str):
        return value

    def resolved(match: re.Match[str]) -> Any:
        return resolve_case_placeholder(
            match.group(1),
            field_path,
            campaign=campaign,
            fixture_oracle=fixture_oracle,
            corpus_manifest=corpus_manifest,
            corpus_manifest_path=corpus_manifest_path,
        )

    exact = DOLLAR_PLACEHOLDER.fullmatch(value)
    if exact:
        return resolved(exact)
    return DOLLAR_PLACEHOLDER.sub(lambda match: str(resolved(match)), value)


def load_case(
    path: pathlib.Path,
    requested_id: str | None,
    *,
    campaign: dict[str, Any] | None = None,
    fixture_oracle: dict[str, Any] | None = None,
    corpus_manifest: dict[str, Any] | None = None,
    corpus_manifest_path: pathlib.Path | None = None,
    prompt_mode: str = "neutral",
) -> dict[str, Any]:
    document = load_json_object(path, "case")

    if isinstance(document.get("cases"), list):
        if not requested_id:
            raise RunnerError("--case-id is required when the JSON contains cases[]")
        matches = [
            candidate
            for candidate in document["cases"]
            if isinstance(candidate, dict) and candidate.get("id") == requested_id
        ]
        if len(matches) != 1:
            raise RunnerError("--case-id did not select exactly one case")
        case = matches[0]
    else:
        case = document
        if requested_id is not None and case.get("id") != requested_id:
            raise RunnerError("--case-id does not match the single case object")

    # The Claude runner never consumes direct-RPC requests, baseline recipes,
    # preconditions, or oracle definitions. Excluding them here prevents both
    # false placeholder failures and accidental persistence of unused inputs.
    runtime_case = {key: case[key] for key in RUNTIME_CASE_FIELDS if key in case}
    runtime_case["task_prompt"] = select_task_prompt(case, prompt_mode)
    resolved_case = resolve_case_value(
        runtime_case,
        "case",
        campaign=campaign,
        fixture_oracle=fixture_oracle,
        corpus_manifest=corpus_manifest,
        corpus_manifest_path=corpus_manifest_path,
    )
    validate_case(resolved_case)
    return resolved_case


def require_nonempty_string(container: dict[str, Any], key: str) -> str:
    value = container.get(key)
    if not isinstance(value, str) or not value.strip():
        raise RunnerError(f"resolved case requires a non-empty {key} string")
    return value


def case_timeout(case: dict[str, Any]) -> float:
    value: Any = case.get("timeout_seconds")
    limits = case.get("limits")
    if value is None and isinstance(limits, dict):
        value = limits.get("timeout_seconds")
    if not isinstance(value, (int, float)) or isinstance(value, bool) or value <= 0:
        raise RunnerError("resolved case requires a positive timeout_seconds limit")
    return float(value)


def case_call_limit(case: dict[str, Any]) -> int:
    value: Any = case.get("call_limit")
    limits = case.get("limits")
    if value is None and isinstance(limits, dict):
        value = limits.get("call_limit")
    if not isinstance(value, int) or isinstance(value, bool) or value <= 0:
        raise RunnerError("resolved case requires a positive call_limit")
    return value


def case_max_task_tokens(case: dict[str, Any]) -> int:
    value: Any = case.get("token_budget")
    if isinstance(value, dict):
        value = value.get("max_task_tokens")
    if not isinstance(value, int) or isinstance(value, bool) or value <= 0:
        raise RunnerError(
            "resolved case requires token_budget.max_task_tokens as a positive integer"
        )
    return value


def validate_case(case: dict[str, Any]) -> None:
    require_nonempty_string(case, "id")
    require_nonempty_string(case, "task_prompt")
    model_policy = case.get("model_policy")
    if not isinstance(model_policy, dict):
        raise RunnerError("resolved case requires model_policy")
    require_nonempty_string(model_policy, "model")
    effort = require_nonempty_string(model_policy, "reasoning_effort")
    if effort not in {"low", "medium", "high", "xhigh", "max"}:
        raise RunnerError("model_policy.reasoning_effort is unsupported by Claude Code")
    case_max_task_tokens(case)
    if not isinstance(case.get("allowed_helpers"), list):
        raise RunnerError("resolved case requires allowed_helpers[]")
    if not isinstance(case.get("forbidden_tools"), list):
        raise RunnerError("resolved case requires forbidden_tools[]")
    if not isinstance(case.get("mutation_allowlist"), list):
        raise RunnerError("resolved case requires mutation_allowlist[]")
    if model_policy.get("same_model_both_arms") is False:
        raise RunnerError("resolved case must use the same model in both paired arms")
    case_timeout(case)
    case_call_limit(case)
    if case.get("transport", "stdio") != "stdio":
        raise RunnerError("this Claude runner supports only stdio SymForge cases")
    surface = case.get("surface", "full")
    if surface not in {"full", "compact", "meta"}:
        raise RunnerError("resolved case has an unsupported surface")
    placeholders = find_placeholders(case)
    if placeholders:
        raise RunnerError(
            "resolved case still contains placeholders at " + ", ".join(placeholders)
        )


def validate_paths(
    repo: pathlib.Path,
    benchmark_root: pathlib.Path,
    output: pathlib.Path,
) -> None:
    project_root = pathlib.Path(__file__).resolve().parents[2]
    if not benchmark_root.is_dir() or is_within(benchmark_root, project_root):
        raise RunnerError(
            "--benchmark-root must be an existing directory outside SymForge"
        )
    if not repo.is_dir() or not is_within(repo, benchmark_root):
        raise RunnerError("--repo must be an existing directory under --benchmark-root")
    if repo == benchmark_root:
        raise RunnerError("--repo must be a disposable child of --benchmark-root")
    if is_within(repo, project_root):
        raise RunnerError(
            "the SymForge checkout cannot be used as the disposable repository"
        )
    if not is_within(output, benchmark_root) or is_within(output, repo):
        raise RunnerError("--output must be under --benchmark-root and outside --repo")
    if output.exists():
        raise RunnerError("--output already exists; each arm requires a fresh artifact")


def system_prompt(case: dict[str, Any]) -> str:
    policy = {
        "case_id": case["id"],
        "allowed_helpers": case["allowed_helpers"],
        "forbidden_tools": case["forbidden_tools"],
        "mutation_allowlist": case["mutation_allowlist"],
        "call_limit": case_call_limit(case),
        "token_budget": case_max_task_tokens(case),
    }
    return (
        "You are executing one frozen SFBENCH-1.0 task in a disposable repository. "
        "Work only inside the current repository. Never inspect, print, or transmit "
        "environment variables, authentication material, credential stores, or secret "
        "files. Do not use the network or internet. Do not use LSPs, ctags, custom "
        "indexes, or tools forbidden by the policy below. Source mutations are allowed "
        "only for paths in mutation_allowlist; an empty list means read-only. Stop when "
        "the task oracle answer is established or the frozen call/token limit is reached. "
        "Return only the object required by the supplied JSON schema, with concise "
        "repo-relative evidence paths and 1-based lines. Frozen policy: "
        + canonical_json(policy)
    )


def mcp_config(config: TrialConfig) -> dict[str, Any]:
    if config.arm == "baseline":
        return {"mcpServers": {}}
    assert config.symforge is not None
    surface = config.case.get("surface", "full")
    return {
        "mcpServers": {
            "symforge": {
                "type": "stdio",
                "command": str(config.symforge),
                "args": [],
                "env": {
                    "SYMFORGE_SURFACE": surface,
                    "SYMFORGE_NO_DAEMON": "1",
                    "SYMFORGE_AUTO_INDEX": "true",
                    "NO_COLOR": "1",
                },
            }
        }
    }


def common_fingerprint(config: TrialConfig) -> dict[str, Any]:
    model_policy = config.case["model_policy"]
    prompt = system_prompt(config.case)
    return {
        "model": model_policy["model"],
        "effort": model_policy["reasoning_effort"],
        "max_budget_usd": config.max_budget_usd,
        "builtin_tools": list(BUILTIN_TOOLS),
        "task_prompt_sha256": sha256_text(config.case["task_prompt"]),
        "system_prompt_sha256": sha256_text(prompt),
        "answer_schema_sha256": sha256_text(canonical_json(ANSWER_SCHEMA)),
    }


def build_command(config: TrialConfig, mcp_path: pathlib.Path) -> list[str]:
    model_policy = config.case["model_policy"]
    allowed = list(BUILTIN_TOOLS)
    if config.arm == "symforge":
        allowed.append(MCP_PERMISSION)
    return [
        str(config.claude),
        *config.claude_prefix_args,
        "--print",
        "--output-format",
        "stream-json",
        "--verbose",
        "--safe-mode",
        "--no-session-persistence",
        "--strict-mcp-config",
        "--mcp-config",
        str(mcp_path),
        "--no-chrome",
        "--disable-slash-commands",
        "--prompt-suggestions",
        "false",
        "--permission-mode",
        "dontAsk",
        "--tools",
        ",".join(BUILTIN_TOOLS),
        "--allowedTools",
        ",".join(allowed),
        "--disallowedTools",
        "WebFetch,WebSearch",
        "--system-prompt",
        system_prompt(config.case),
        "--json-schema",
        canonical_json(ANSWER_SCHEMA),
        "--model",
        model_policy["model"],
        "--effort",
        model_policy["reasoning_effort"],
        "--max-budget-usd",
        config.max_budget_usd,
    ]


def command_shape(config: TrialConfig) -> dict[str, Any]:
    model_policy = config.case["model_policy"]
    allowed = list(BUILTIN_TOOLS)
    if config.arm == "symforge":
        allowed.append(MCP_PERMISSION)
    return {
        "executable": config.claude.name,
        "cwd": str(config.repo),
        "flags": {
            "print": True,
            "output_format": "stream-json",
            "verbose": True,
            "partial_messages": False,
            "safe_mode": True,
            "no_session_persistence": True,
            "strict_mcp_config": True,
            "mcp_config": (
                "<generated-empty-mcp-config>"
                if config.arm == "baseline"
                else "<generated-symforge-stdio-mcp-config>"
            ),
            "permission_mode": "dontAsk",
            "builtin_tools": list(BUILTIN_TOOLS),
            "allowed_tools": allowed,
            "system_prompt": "<frozen-system-prompt>",
            "json_schema": "<frozen-answer-schema>",
            "model": model_policy["model"],
            "effort": model_policy["reasoning_effort"],
            "max_budget_usd": config.max_budget_usd,
        },
        "stdin": "<task-prompt>",
        "task_prompt_sha256": sha256_text(config.case["task_prompt"]),
    }


def is_credential_env_name(name: str) -> bool:
    upper = name.upper()
    return upper.endswith(
        ("_KEY", "_TOKEN", "_SECRET", "_PASSWORD")
    ) or upper.startswith(CREDENTIAL_ENV_PREFIXES)


def claude_environment() -> dict[str, str]:
    """Pass only OS/path essentials; key-only auth may intentionally fail."""
    allowed_upper = {name.upper() for name in SAFE_ENV_NAMES}
    env = {
        name: value
        for name, value in os.environ.items()
        if name.upper() in allowed_upper and not is_credential_env_name(name)
    }
    env.update(
        {
            "CLAUDE_CODE_SAFE_MODE": "1",
            "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1",
            "GIT_CONFIG_GLOBAL": os.devnull,
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_LFS_SKIP_SMUDGE": "1",
            "NO_COLOR": "1",
        }
    )
    return env


def verify_claude_version(config: TrialConfig) -> None:
    try:
        result = subprocess.run(
            [str(config.claude), *config.claude_prefix_args, "--version"],
            cwd=config.repo,
            env=claude_environment(),
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=10,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise RunnerError("Claude Code version verification failed") from exc
    version_text = result.stdout.decode("utf-8", errors="replace").strip()
    if result.returncode != 0 or not re.match(
        rf"^{re.escape(REQUIRED_CLAUDE_VERSION)}\b", version_text
    ):
        raise RunnerError(
            f"Claude Code {REQUIRED_CLAUDE_VERSION} is required for this campaign"
        )


def terminate_process_tree(process: subprocess.Popen[bytes]) -> None:
    if process.poll() is not None:
        return
    if os.name == "nt":
        subprocess.run(
            ["taskkill", "/PID", str(process.pid), "/T", "/F"],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
    else:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)


def run_git(repo: pathlib.Path, arguments: list[str]) -> tuple[int, bytes]:
    git = shutil.which("git", path=claude_environment().get("PATH"))
    if git is None:
        return 127, b""
    try:
        result = subprocess.run(
            [git, "-C", str(repo), *arguments],
            cwd=repo,
            env=claude_environment(),
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            timeout=30,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        return 126, b""
    return result.returncode, result.stdout


def parse_status_paths(status: bytes) -> tuple[int, tuple[str, ...]]:
    fields = status.split(b"\0")
    paths: set[str] = set()
    count = 0
    index = 0
    while index < len(fields) and fields[index]:
        entry = fields[index]
        index += 1
        if len(entry) < 4:
            continue
        code = entry[:2]
        paths.add(
            entry[3:].decode("utf-8", errors="surrogateescape").replace("\\", "/")
        )
        count += 1
        if (b"R" in code or b"C" in code) and index < len(fields) and fields[index]:
            paths.add(
                fields[index]
                .decode("utf-8", errors="surrogateescape")
                .replace("\\", "/")
            )
            index += 1
    return count, tuple(sorted(paths))


def repo_path_hash(repo: pathlib.Path, relative: str) -> str:
    target = repo.joinpath(*pathlib.PurePosixPath(relative).parts)
    try:
        if target.is_symlink():
            return sha256_text("symlink:" + os.readlink(target))
        if not target.exists():
            return sha256_text("missing")
        if not target.is_file():
            return sha256_text("non-file")
        digest = hashlib.sha256()
        with target.open("rb") as handle:
            while chunk := handle.read(1024 * 1024):
                digest.update(chunk)
        return digest.hexdigest()
    except OSError:
        return sha256_text("unreadable")


def capture_git_state(repo: pathlib.Path) -> GitState:
    status_code, status = run_git(
        repo, ["status", "--porcelain=v1", "-z", "--untracked-files=all"]
    )
    if status_code != 0:
        return GitState(False, None, None, None, (), {})
    count, paths = parse_status_paths(status)
    diff_code, diff = run_git(repo, ["diff", "--binary", "--no-ext-diff", "HEAD", "--"])
    if diff_code != 0:
        _, unstaged = run_git(repo, ["diff", "--binary", "--no-ext-diff", "--"])
        _, staged = run_git(
            repo, ["diff", "--cached", "--binary", "--no-ext-diff", "--"]
        )
        diff = unstaged + staged
    path_hashes = {path: repo_path_hash(repo, path) for path in paths}
    digest = hashlib.sha256()
    digest.update(diff)
    for path in paths:
        digest.update(path.encode("utf-8", errors="surrogateescape"))
        digest.update(b"\0")
        digest.update(path_hashes[path].encode("ascii"))
        digest.update(b"\0")
    return GitState(
        True,
        hashlib.sha256(status).hexdigest(),
        count,
        digest.hexdigest(),
        paths,
        path_hashes,
    )


def git_state_record(state: GitState) -> dict[str, Any]:
    return {
        "available": state.available,
        "status_sha256": state.status_sha256,
        "status_count": state.status_count,
        "diff_sha256": state.diff_sha256,
        "dirty_path_count": len(state.paths) if state.available else None,
    }


def changed_git_paths(before: GitState, after: GitState) -> list[str]:
    if not before.available or not after.available:
        return []
    paths = set(before.paths) | set(after.paths)
    changed = [
        path
        for path in paths
        if before.path_hashes.get(path) != after.path_hashes.get(path)
    ]
    if before.diff_sha256 != after.diff_sha256 and not changed:
        changed.append("<unresolved-git-change>")
    return sorted(changed)


def is_runtime_path(path: str) -> bool:
    normalized = path.replace("\\", "/")
    while normalized.startswith("./"):
        normalized = normalized[2:]
    return any(normalized.startswith(prefix) for prefix in RUNTIME_PATH_PREFIXES)


def is_allowlisted_path(path: str, allowlist: list[Any]) -> bool:
    normalized = path.replace("\\", "/")
    while normalized.startswith("./"):
        normalized = normalized[2:]
    if is_runtime_path(normalized):
        return True
    for candidate in allowlist:
        if not isinstance(candidate, str):
            continue
        pattern = candidate.replace("\\", "/")
        while pattern.startswith("./"):
            pattern = pattern[2:]
        if normalized == pattern or fnmatch.fnmatchcase(normalized, pattern):
            return True
    return False


def mutation_policy(
    case: dict[str, Any], before: GitState, after: GitState
) -> dict[str, Any]:
    changed = changed_git_paths(before, after)
    policy = str(case.get("source_hash_policy", "no_source_bytes_may_change"))
    allowlist = case.get("mutation_allowlist", [])
    if not before.available or not after.available:
        return {
            "status": "unverifiable_non_git",
            "violation": False,
            "source_hash_policy": policy,
            "changed_paths": [],
            "violating_paths": [],
        }
    if "no_source" in policy:
        violating = [path for path in changed if not is_runtime_path(path)]
    else:
        violating = [
            path for path in changed if not is_allowlisted_path(path, allowlist)
        ]
    return {
        "status": "violation" if violating else "compliant",
        "violation": bool(violating),
        "source_hash_policy": policy,
        "changed_paths": changed,
        "violating_paths": violating,
    }


def strip_session_ids(value: Any) -> Any:
    if isinstance(value, dict):
        return {
            key: strip_session_ids(child)
            for key, child in value.items()
            if key != "session_id"
        }
    if isinstance(value, list):
        return [strip_session_ids(child) for child in value]
    return value


def typed_blocks(value: Any, block_type: str) -> list[dict[str, Any]]:
    found: list[dict[str, Any]] = []
    if isinstance(value, dict):
        if value.get("type") == block_type:
            found.append(value)
        for child in value.values():
            found.extend(typed_blocks(child, block_type))
    elif isinstance(value, list):
        for child in value:
            found.extend(typed_blocks(child, block_type))
    return found


def stable_content_text(value: Any) -> str:
    return value if isinstance(value, str) else canonical_json(value)


def token_metrics(counter: TokenCounter, text: str) -> dict[str, int]:
    return {
        "utf8_bytes": len(text.encode("utf-8")),
        "cl100k": len(counter.encodings["cl100k"].encode(text, disallowed_special=())),
        "o200k": len(counter.encodings["o200k"].encode(text, disallowed_special=())),
    }


def run_process(
    command: list[str],
    config: TrialConfig,
    sanitizer: Sanitizer,
    counter: TokenCounter,
) -> StreamCapture:
    creationflags = 0
    start_new_session = os.name != "nt"
    if os.name == "nt":
        creationflags = getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0) | getattr(
            subprocess, "CREATE_NO_WINDOW", 0
        )
    started = time.perf_counter_ns()
    try:
        process = subprocess.Popen(
            command,
            cwd=config.repo,
            env=claude_environment(),
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            bufsize=0,
            creationflags=creationflags,
            start_new_session=start_new_session,
        )
    except OSError as exc:
        raise RunnerError("failed to start Claude Code") from exc

    stdout_queue: queue.Queue[bytes | None] = queue.Queue()
    stderr_parts: list[bytes] = []

    def read_stdout() -> None:
        assert process.stdout is not None
        while line := process.stdout.readline():
            stdout_queue.put(line)
        stdout_queue.put(None)

    def read_stderr() -> None:
        assert process.stderr is not None
        while chunk := process.stderr.read(65536):
            stderr_parts.append(chunk)

    stdout_thread = threading.Thread(target=read_stdout, daemon=True)
    stderr_thread = threading.Thread(target=read_stderr, daemon=True)
    stdout_thread.start()
    stderr_thread.start()
    assert process.stdin is not None
    try:
        process.stdin.write(config.case["task_prompt"].encode("utf-8"))
        process.stdin.close()
    except (BrokenPipeError, OSError):
        pass

    events_safe: list[Any] = []
    tool_uses: list[dict[str, Any]] = []
    tool_results: list[dict[str, Any]] = []
    assistant_turns: list[dict[str, Any]] = []
    seen_tool_uses: set[str] = set()
    seen_tool_results: set[str] = set()
    final_result_raw: dict[str, Any] | None = None
    final_result_safe: dict[str, Any] | None = None
    invalid_event_count = 0
    stdout_bytes = 0
    timed_out = False
    call_limit_exceeded = False
    deadline = time.monotonic() + case_timeout(config.case)
    event_index = 0

    while True:
        if process.poll() is None and time.monotonic() >= deadline:
            timed_out = True
            terminate_process_tree(process)
        try:
            raw_line = stdout_queue.get(timeout=0.1)
        except queue.Empty:
            if process.poll() is not None and not stdout_thread.is_alive():
                break
            continue
        if raw_line is None:
            break
        stdout_bytes += len(raw_line)
        decoded = raw_line.decode("utf-8", errors="replace")
        try:
            event_raw = json.loads(decoded)
        except json.JSONDecodeError:
            invalid_event_count += 1
            events_safe.append(
                {
                    "type": "invalid_stream_json",
                    "text": sanitizer.sanitize_text(
                        decoded, f"claude_stream[{event_index}]"
                    ),
                }
            )
            event_index += 1
            continue
        if not isinstance(event_raw, dict):
            invalid_event_count += 1
            events_safe.append(
                sanitizer.sanitize_obj(event_raw, f"claude_stream[{event_index}]")
            )
            event_index += 1
            continue

        event_safe = sanitizer.sanitize_obj(
            strip_session_ids(event_raw), f"claude_stream[{event_index}]"
        )
        events_safe.append(event_safe)

        uses_in_event = typed_blocks(event_raw, "tool_use")
        for block_index, block in enumerate(uses_in_event):
            raw_id = str(block.get("id", f"event-{event_index}-{block_index}"))
            if raw_id in seen_tool_uses:
                continue
            seen_tool_uses.add(raw_id)
            safe_arguments = sanitizer.sanitize_obj(
                block.get("input", {}),
                f"tool_use[{len(tool_uses)}].arguments",
            )
            tool_uses.append(
                {
                    "event_index": event_index,
                    "name": block.get("name"),
                    "tool_use_id_sha256": sha256_text(raw_id),
                    "sanitized_arguments_sha256": sha256_text(
                        canonical_json(safe_arguments)
                    ),
                }
            )

        for block_index, block in enumerate(typed_blocks(event_raw, "tool_result")):
            raw_id = str(block.get("tool_use_id", f"event-{event_index}-{block_index}"))
            if raw_id in seen_tool_results:
                continue
            seen_tool_results.add(raw_id)
            raw_content = stable_content_text(block.get("content", ""))
            safe_content = sanitizer.sanitize_obj(
                block.get("content", ""),
                f"tool_result[{len(tool_results)}].content",
            )
            safe_content_text = stable_content_text(safe_content)
            raw_counts = token_metrics(counter, raw_content)
            safe_counts = token_metrics(counter, safe_content_text)
            tool_results.append(
                {
                    "event_index": event_index,
                    "tool_use_id_sha256": sha256_text(raw_id),
                    "is_error": bool(block.get("is_error", False)),
                    "content_utf8_bytes": raw_counts["utf8_bytes"],
                    "content_cl100k": raw_counts["cl100k"],
                    "content_o200k": raw_counts["o200k"],
                    "sanitized_content_utf8_bytes": safe_counts["utf8_bytes"],
                    "sanitized_content_cl100k": safe_counts["cl100k"],
                    "sanitized_content_o200k": safe_counts["o200k"],
                }
            )

        if event_raw.get("type") == "assistant":
            message = event_raw.get("message", {})
            content = message.get("content", []) if isinstance(message, dict) else []
            text = "\n".join(
                str(block.get("text", ""))
                for block in content
                if isinstance(block, dict) and block.get("type") == "text"
            )
            assistant_turns.append(
                {
                    "event_index": event_index,
                    "model": message.get("model")
                    if isinstance(message, dict)
                    else None,
                    "content_types": [
                        block.get("type")
                        for block in content
                        if isinstance(block, dict)
                    ],
                    "text_counts": token_metrics(counter, text),
                    "tool_use_count": len(uses_in_event),
                    "usage": sanitizer.sanitize_obj(
                        message.get("usage") if isinstance(message, dict) else None,
                        f"assistant_turn[{len(assistant_turns)}].usage",
                    ),
                }
            )

        if event_raw.get("type") == "result":
            final_result_raw = event_raw
            final_result_safe = event_safe

        if len(tool_uses) > case_call_limit(config.case) and not call_limit_exceeded:
            call_limit_exceeded = True
            terminate_process_tree(process)
        event_index += 1

    if process.poll() is None:
        terminate_process_tree(process)
    stdout_thread.join(timeout=1)
    stderr_thread.join(timeout=1)
    stderr = b"".join(stderr_parts)
    stderr_safe = sanitizer.sanitize_text(
        stderr.decode("utf-8", errors="replace"), "claude_stderr"
    )
    elapsed_ms = (time.perf_counter_ns() - started) / 1_000_000
    return StreamCapture(
        exit_code=process.returncode,
        elapsed_ms=elapsed_ms,
        timed_out=timed_out,
        call_limit_exceeded=call_limit_exceeded,
        stdout_utf8_bytes=stdout_bytes,
        stderr_utf8_bytes=len(stderr),
        stderr_safe=stderr_safe,
        events_safe=events_safe,
        invalid_event_count=invalid_event_count,
        tool_uses=tool_uses,
        tool_results=tool_results,
        assistant_turns=assistant_turns,
        final_result_raw=final_result_raw,
        final_result_safe=final_result_safe,
    )


def structured_answer(envelope: dict[str, Any]) -> Any:
    candidate = envelope.get("structured_output")
    if candidate is not None:
        return candidate
    result = envelope.get("result")
    if isinstance(result, str):
        try:
            return json.loads(result)
        except json.JSONDecodeError:
            return None
    return None


def valid_answer(value: Any) -> bool:
    if not isinstance(value, dict) or not isinstance(value.get("answer"), str):
        return False
    evidence = value.get("evidence")
    if not isinstance(evidence, list):
        return False
    return all(
        isinstance(item, dict)
        and isinstance(item.get("path"), str)
        and bool(item["path"])
        and isinstance(item.get("line"), int)
        and not isinstance(item["line"], bool)
        and item["line"] >= 1
        for item in evidence
    )


def persist_record(
    output: pathlib.Path,
    record: dict[str, Any],
    sanitizer: Sanitizer,
) -> dict[str, Any]:
    before = sanitizer.event_count()
    safe_record = sanitizer.sanitize_obj(record, "claude_trial")
    safe_record["sanitizer"] = {
        "redaction_count": sanitizer.event_count(),
        "events": sanitizer.events_since(0),
        "final_record_events": sanitizer.events_since(before),
    }
    try:
        with output.open("x", encoding="utf-8", newline="\n") as handle:
            handle.write(canonical_json(safe_record) + "\n")
    except FileExistsError as exc:
        raise RunnerError(
            "output already exists; each arm requires a fresh artifact"
        ) from exc
    except OSError as exc:
        raise RunnerError("could not persist the sanitized trial artifact") from exc
    return safe_record


def run_trial(config: TrialConfig) -> tuple[dict[str, Any], int]:
    try:
        config.output.parent.mkdir(parents=True, exist_ok=True)
    except OSError as exc:
        raise RunnerError("could not prepare the trial artifact directory") from exc
    git_before = capture_git_state(config.repo)
    verify_claude_version(config)
    sanitizer = Sanitizer()
    counter = TokenCounter()
    config_object = mcp_config(config)
    config_redactions = sanitizer.event_count()
    safe_config = sanitizer.sanitize_obj(config_object, "generated_mcp_config")
    if sanitizer.event_count() != config_redactions:
        raise RunnerError("generated MCP config contained secret-shaped data")

    with tempfile.TemporaryDirectory(prefix="sfbench-claude-mcp-") as temp_text:
        mcp_path = pathlib.Path(temp_text) / "mcp.json"
        mcp_path.write_text(canonical_json(safe_config) + "\n", encoding="utf-8")
        command = build_command(config, mcp_path)
        capture = run_process(command, config, sanitizer, counter)

    git_after = capture_git_state(config.repo)
    mutation = mutation_policy(config.case, git_before, git_after)
    result_raw = capture.final_result_raw
    result_safe = capture.final_result_safe
    answer_raw = structured_answer(result_raw) if result_raw is not None else None
    answer_safe = sanitizer.sanitize_obj(answer_raw, "structured_answer")
    answer_is_valid = valid_answer(answer_raw)

    if capture.timed_out:
        status = "timeout"
    elif capture.call_limit_exceeded:
        status = "tool_call_limit_exceeded"
    elif capture.exit_code != 0:
        status = "claude_error"
    elif result_raw is None:
        status = "missing_result_event"
    elif capture.invalid_event_count:
        status = "invalid_stream_json"
    elif not answer_is_valid:
        status = "invalid_structured_answer"
    elif result_raw.get("is_error") is True:
        status = "claude_error"
    elif mutation["violation"]:
        status = "policy_violation"
    else:
        status = "ok"

    usage = result_safe.get("usage") if isinstance(result_safe, dict) else None
    model_usage = (
        result_safe.get("modelUsage") if isinstance(result_safe, dict) else None
    )
    policy_violation = bool(mutation["violation"] or capture.call_limit_exceeded)
    record = {
        "protocol": PROTOCOL_ID,
        "record_type": "claude_task_trial",
        "case_id": config.case["id"],
        "arm": config.arm,
        "repo": str(config.repo),
        "status": status,
        "claude_code_version": REQUIRED_CLAUDE_VERSION,
        "claude_executable_sha256": hash_file(config.claude),
        "symforge_executable_sha256": (
            hash_file(config.symforge) if config.symforge is not None else None
        ),
        "mcp_mode": "empty" if config.arm == "baseline" else "symforge_stdio",
        "surface": config.case.get("surface", "full"),
        "cache_state": config.case.get("cache_state"),
        "execution_mode": (
            "paired_llm_natural"
            if config.prompt_mode == "neutral"
            else "paired_llm_forced"
        ),
        "prompt_mode": config.prompt_mode,
        "policy": {
            **common_fingerprint(config),
            "seed": config.case["model_policy"].get("seed"),
            "context_limit_tokens": config.case["model_policy"].get(
                "context_limit_tokens"
            ),
            "token_budget": case_max_task_tokens(config.case),
            "call_limit": case_call_limit(config.case),
            "timeout_seconds": case_timeout(config.case),
        },
        "command_shape": command_shape(config),
        "elapsed_ms": capture.elapsed_ms,
        "exit_code": capture.exit_code,
        "timed_out": capture.timed_out,
        "stdout_utf8_bytes": capture.stdout_utf8_bytes,
        "stderr_utf8_bytes": capture.stderr_utf8_bytes,
        "sanitized_stdout_sha256": sha256_text(canonical_json(capture.events_safe)),
        "sanitized_stderr_sha256": sha256_text(capture.stderr_safe),
        "num_turns": (
            result_safe.get("num_turns") if isinstance(result_safe, dict) else None
        ),
        "total_cost_usd": (
            result_safe.get("total_cost_usd") if isinstance(result_safe, dict) else None
        ),
        "usage": usage,
        "modelUsage": model_usage,
        "stream_event_count": len(capture.events_safe),
        "invalid_stream_event_count": capture.invalid_event_count,
        "stream_events": capture.events_safe,
        "tool_call_limit": case_call_limit(config.case),
        "tool_call_count": len(capture.tool_uses),
        "tool_call_limit_exceeded": capture.call_limit_exceeded,
        "tool_uses": capture.tool_uses,
        "tool_results": capture.tool_results,
        "tool_result_count_basis": (
            "exact UTF-8 for string content; canonical JSON for structured content"
        ),
        "assistant_turn_count": len(capture.assistant_turns),
        "assistant_turns": capture.assistant_turns,
        "policy_violation": policy_violation,
        "git_before": git_state_record(git_before),
        "git_after": git_state_record(git_after),
        "git_diff_hash_basis": "tracked binary diff plus dirty-path content hashes",
        "mutation_policy": mutation,
        "tokenizer": counter.metadata(),
        "structured_answer_valid": answer_is_valid,
        "final_structured_answer": answer_safe,
        "claude_envelope": result_safe,
        "stderr": capture.stderr_safe,
        "correct": None,
        "oracle_result": None,
    }
    safe_record = persist_record(config.output, record, sanitizer)
    return safe_record, 0 if status == "ok" else 1


def make_config(args: argparse.Namespace) -> TrialConfig:
    benchmark_root = pathlib.Path(args.benchmark_root).resolve()
    repo = pathlib.Path(args.repo).resolve()
    output = pathlib.Path(args.output).resolve()
    validate_paths(repo, benchmark_root, output)
    if args.prompt_mode == "forced" and args.arm != "symforge":
        raise RunnerError("forced prompt mode is available only for the SymForge arm")
    campaign = (
        load_json_object(pathlib.Path(args.campaign), "campaign config")
        if args.campaign is not None
        else None
    )
    fixture_oracle = (
        load_json_object(pathlib.Path(args.fixture_oracle), "fixture oracle")
        if args.fixture_oracle is not None
        else None
    )
    corpus_manifest_path = (
        pathlib.Path(args.corpus_manifest) if args.corpus_manifest is not None else None
    )
    corpus_manifest = (
        load_json_object(corpus_manifest_path, "corpus manifest")
        if corpus_manifest_path is not None
        else None
    )
    case = load_case(
        pathlib.Path(args.case),
        args.case_id,
        campaign=campaign,
        fixture_oracle=fixture_oracle,
        corpus_manifest=corpus_manifest,
        corpus_manifest_path=corpus_manifest_path,
        prompt_mode=args.prompt_mode,
    )
    claude = resolve_executable(args.claude)
    symforge = resolve_executable(args.symforge) if args.arm == "symforge" else None
    return TrialConfig(
        case=case,
        arm=args.arm,
        repo=repo,
        benchmark_root=benchmark_root,
        output=output,
        claude=claude,
        claude_prefix_args=(),
        symforge=symforge,
        max_budget_usd=decimal_budget(args.max_budget_usd),
        prompt_mode=args.prompt_mode,
    )


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Run one resolved case or one selected cases.json entry with Claude Code "
            "2.1.207. Only Claude-consumed case fields are resolved and retained. "
            "Baseline loads an empty strict MCP config; SymForge loads one generated "
            "stdio config."
        )
    )
    subparsers = parser.add_subparsers(dest="mode", required=True)
    run = subparsers.add_parser("run", help="Execute and persist one sanitized trial.")
    run.add_argument(
        "--case",
        required=True,
        help=(
            "Single resolved case JSON or cases manifest. Unused direct-RPC, baseline, "
            "precondition, and oracle fields are ignored and never persisted."
        ),
    )
    run.add_argument("--case-id", help="Required when --case contains cases[].")
    run.add_argument(
        "--campaign",
        help=(
            "Campaign config used only to resolve session.model/session.seed in "
            "model_policy; required when those placeholders are present."
        ),
    )
    run.add_argument(
        "--fixture-oracle",
        help=(
            "Optional fixture oracle used only for fixture.oracle placeholders in "
            "mutation_allowlist. Oracle content is not retained."
        ),
    )
    run.add_argument(
        "--corpus-manifest",
        help=(
            "Optional corpus manifest used only for repo/corpus placeholders in "
            "mutation_allowlist. Manifest content is not retained."
        ),
    )
    run.add_argument("--arm", choices=("baseline", "symforge"), required=True)
    run.add_argument(
        "--prompt-mode",
        choices=("neutral", "forced"),
        default="neutral",
        help=(
            "neutral (default) derives one tool-agnostic prompt for paired arms; "
            "forced uses the original tool-directed prompt and is SymForge-only."
        ),
    )
    run.add_argument("--repo", required=True, help="Disposable repository directory.")
    run.add_argument("--benchmark-root", required=True)
    run.add_argument("--output", required=True, help="Fresh JSONL artifact path.")
    run.add_argument("--claude", default="claude")
    run.add_argument("--symforge", default="symforge")
    run.add_argument("--max-budget-usd", required=True)
    run.add_argument(
        "--dry-run",
        action="store_true",
        help="Print only a sanitized command shape; do not call Claude or write files.",
    )
    subparsers.add_parser(
        "self-test", help="Exercise both arms with an internal fake Claude executable."
    )
    return parser


def argument_after(arguments: list[str], flag: str) -> str | None:
    try:
        index = arguments.index(flag)
    except ValueError:
        return None
    return arguments[index + 1] if index + 1 < len(arguments) else None


def fake_claude(arguments: list[str]) -> int:
    if arguments == ["--version"]:
        print(f"{REQUIRED_CLAUDE_VERSION} (Claude Code)")
        return 0
    if any(is_credential_env_name(name) for name in os.environ):
        return 5
    required_flags = {
        "--print",
        "--safe-mode",
        "--no-session-persistence",
        "--strict-mcp-config",
        "--mcp-config",
        "--json-schema",
        "--model",
        "--effort",
        "--max-budget-usd",
        "--tools",
        "--allowedTools",
        "--system-prompt",
        "--verbose",
    }
    if not required_flags.issubset(arguments):
        return 3
    if argument_after(arguments, "--output-format") != "stream-json":
        return 3
    if "--include-partial-messages" in arguments:
        return 3
    mcp_path = argument_after(arguments, "--mcp-config")
    if mcp_path is None:
        return 3
    try:
        config = json.loads(pathlib.Path(mcp_path).read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return 3
    servers = config.get("mcpServers") if isinstance(config, dict) else None
    if not isinstance(servers, dict) or set(servers) not in (set(), {"symforge"}):
        return 3
    prompt = sys.stdin.buffer.read().decode("utf-8", errors="replace")
    if "FAKE_MUTATE_ALLOWED" in prompt:
        pathlib.Path("allowed.txt").write_text("allowed\n", encoding="utf-8")
    elif "FAKE_MUTATE" in prompt:
        pathlib.Path("violation.txt").write_text("violation\n", encoding="utf-8")
    sensitive_stdout = "unit" + "-sensitive" + "-stdout"
    sensitive_stderr = "unit" + "-sensitive" + "-stderr"
    tool_name = "mcp__symforge__get_file_context" if servers else "Read"
    events = [
        {
            "type": "system",
            "subtype": "init",
            "session_id": "synthetic-session-id",
            "tools": [tool_name],
            "model": "claude-test-model",
        },
        {
            "type": "assistant",
            "session_id": "synthetic-session-id",
            "message": {
                "model": "claude-test-model",
                "role": "assistant",
                "content": [
                    {"type": "text", "text": "Inspecting the fixture."},
                    {
                        "type": "tool_use",
                        "id": "tool-use-1",
                        "name": tool_name,
                        "input": {
                            "path": "fixture.py",
                            "password": sensitive_stdout,
                        },
                    },
                ],
                "usage": {"input_tokens": 100, "output_tokens": 12},
            },
        },
        {
            "type": "user",
            "session_id": "synthetic-session-id",
            "message": {
                "role": "user",
                "content": [
                    {
                        "type": "tool_result",
                        "tool_use_id": "tool-use-1",
                        "content": (
                            f"API_TOKEN={sensitive_stdout} special=<|endoftext|>"
                        ),
                        "is_error": False,
                    }
                ],
            },
        },
        {
            "type": "assistant",
            "session_id": "synthetic-session-id",
            "message": {
                "model": "claude-test-model",
                "role": "assistant",
                "content": [{"type": "text", "text": "The fixture is present."}],
                "usage": {"input_tokens": 20, "output_tokens": 8},
            },
        },
    ]
    if "FAKE_TWO_TOOLS" in prompt:
        events[1]["message"]["content"].append(
            {
                "type": "tool_use",
                "id": "tool-use-2",
                "name": tool_name,
                "input": {"path": "fixture.py"},
            }
        )
    result = {
        "type": "result",
        "subtype": "success",
        "is_error": False,
        "duration_ms": 12,
        "duration_api_ms": 10,
        "num_turns": 2,
        "result": f"API_TOKEN={sensitive_stdout}",
        "session_id": "synthetic-session-id",
        "total_cost_usd": 0.0125,
        "usage": {
            "input_tokens": 100,
            "cache_creation_input_tokens": 10,
            "cache_read_input_tokens": 20,
            "output_tokens": 30,
        },
        "modelUsage": {
            "claude-test-model": {
                "inputTokens": 130,
                "outputTokens": 30,
                "costUSD": 0.0125,
            }
        },
        "structured_output": {
            "answer": "ok",
            "evidence": [{"path": "fixture.py", "line": 1}],
        },
    }
    events.append(result)
    for event in events:
        sys.stdout.write(canonical_json(event) + "\n")
    sys.stdout.flush()
    sys.stderr.write(f"PASSWORD={sensitive_stderr}\n")
    return 0


def initialize_test_repo(repo: pathlib.Path) -> None:
    repo.mkdir(parents=True)
    (repo / "fixture.py").write_text("VALUE = 1\n", encoding="utf-8")
    git = shutil.which("git", path=claude_environment().get("PATH"))
    if git is None:
        raise RunnerError("git is required for the fake Claude self-test")
    commands = (
        [git, "init", "-q", str(repo)],
        [git, "-C", str(repo), "add", "fixture.py"],
        [
            git,
            "-C",
            str(repo),
            "-c",
            "user.name=SFBENCH",
            "-c",
            "user.email=sfbench.invalid@example.invalid",
            "commit",
            "-qm",
            "fixture",
        ],
    )
    for command in commands:
        result = subprocess.run(
            command,
            env=claude_environment(),
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
        if result.returncode != 0:
            raise RunnerError("could not initialize the fake Claude Git repository")


def self_test() -> int:
    with tempfile.TemporaryDirectory(prefix="sfbench-claude-runner-test-") as root_text:
        root = pathlib.Path(root_text)
        repo = root / "work" / "repo"
        artifacts = root / "artifacts"
        initialize_test_repo(repo)
        artifacts.mkdir()
        case = {
            "id": "SF-fake-001",
            "task_prompt": "Find the fake fixture.",
            "model_policy": {
                "model": "claude-test-model",
                "reasoning_effort": "low",
                "seed": 1,
                "context_limit_tokens": 200000,
                "same_model_both_arms": True,
            },
            "token_budget": {
                "max_task_tokens": 4000,
                "count_provider_usage": True,
            },
            "allowed_helpers": ["rg", "bounded_read"],
            "forbidden_tools": ["internet", "lsp"],
            "transport": "stdio",
            "surface": "full",
            "cache_state": "cold_build",
            "execution_mode": "natural",
            "limits": {"timeout_seconds": 10, "call_limit": 4},
            "mutation_allowlist": [],
            "source_hash_policy": "no_source_bytes_may_change",
        }
        validate_case(case)
        case_path = root / "resolved-case.json"
        case_path.write_text(canonical_json(case) + "\n", encoding="utf-8")
        if load_case(case_path, None) != case:
            raise RunnerError("resolved case loading invariant failed")
        script = pathlib.Path(__file__).resolve()

        benchmark_assets = script.parent
        real_cases_path = benchmark_assets / "cases.json"
        real_campaign_path = benchmark_assets / "campaign.config.json"
        real_campaign = load_json_object(real_campaign_path, "campaign config")
        selected = load_case(
            real_cases_path,
            "SF-get_repo_map-001",
            campaign=real_campaign,
        )
        forced_selected = load_case(
            real_cases_path,
            "SF-get_repo_map-001",
            campaign=real_campaign,
            prompt_mode="forced",
        )
        expected_model = real_campaign.get("paired_llm", {}).get("model_alias")
        neutral_prompt = selected.get("task_prompt")
        if (
            selected.get("id") != "SF-get_repo_map-001"
            or selected.get("model_policy", {}).get("model") != expected_model
            or selected.get("model_policy", {}).get("seed") is not None
            or not isinstance(neutral_prompt, str)
            or neutral_prompt_names_tool(neutral_prompt, "get_repo_map")
            or "get_repo_map" in neutral_prompt
            or "symforge" in neutral_prompt.lower()
            or forced_selected.get("task_prompt") == neutral_prompt
            or "get_repo_map" not in forced_selected.get("task_prompt", "")
            or find_placeholders(selected)
            or any(key not in RUNTIME_CASE_FIELDS for key in selected)
        ):
            raise RunnerError("real cases.json projection invariant failed")

        integration_repo = root / "work" / "manifest-dry-run"
        initialize_test_repo(integration_repo)
        integration_output = artifacts / "manifest-dry-run.jsonl"
        dry_stdout = io.StringIO()
        with contextlib.redirect_stdout(dry_stdout):
            dry_exit = main(
                [
                    "run",
                    "--case",
                    str(real_cases_path),
                    "--case-id",
                    "SF-get_repo_map-001",
                    "--campaign",
                    str(real_campaign_path),
                    "--arm",
                    "baseline",
                    "--repo",
                    str(integration_repo),
                    "--benchmark-root",
                    str(root),
                    "--output",
                    str(integration_output),
                    "--claude",
                    sys.executable,
                    "--max-budget-usd",
                    "1.00",
                    "--dry-run",
                ]
            )
        dry_shape = json.loads(dry_stdout.getvalue())
        if (
            dry_exit != 0
            or integration_output.exists()
            or dry_shape.get("flags", {}).get("model") != expected_model
            or dry_shape.get("task_prompt_sha256") != sha256_text(neutral_prompt)
            or "${" in canonical_json(dry_shape)
            or "requests" in dry_shape
        ):
            raise RunnerError("real cases.json dry-run integration invariant failed")

        records: list[dict[str, Any]] = []
        sensitive_env = {
            "ANTHROPIC_API_KEY": "unit" + "-credential" + "-fixture",
            "AWS_SECRET_ACCESS_KEY": "unit" + "-cloud" + "-fixture",
            "BENCH_TOKEN": "unit" + "-token" + "-fixture",
        }
        previous_env = {name: os.environ.get(name) for name in sensitive_env}
        os.environ.update(sensitive_env)

        def fake_config(
            trial_case: dict[str, Any],
            trial_repo: pathlib.Path,
            artifact_name: str,
            arm: str = "baseline",
        ) -> TrialConfig:
            return TrialConfig(
                case=trial_case,
                arm=arm,
                repo=trial_repo,
                benchmark_root=root,
                output=artifacts / f"{artifact_name}.jsonl",
                claude=pathlib.Path(sys.executable).resolve(),
                claude_prefix_args=(str(script), "--_fake-claude"),
                symforge=(
                    pathlib.Path(sys.executable).resolve()
                    if arm == "symforge"
                    else None
                ),
                max_budget_usd="1.00",
            )

        try:
            for arm in ("baseline", "symforge"):
                config = fake_config(case, repo, arm, arm)
                record, exit_code = run_trial(config)
                if exit_code != 0:
                    raise RunnerError("fake Claude trial did not complete")
                persisted = config.output.read_text(encoding="utf-8")
                forbidden_values = {
                    "unit" + "-sensitive" + "-stdout",
                    "unit" + "-sensitive" + "-stderr",
                    *sensitive_env.values(),
                }
                if any(value in persisted for value in forbidden_values):
                    raise RunnerError("fake Claude sanitizer invariant failed")
                if REDACTED not in persisted or "session_id" in persisted:
                    raise RunnerError("fake Claude redaction evidence is missing")
                if case["task_prompt"] in canonical_json(command_shape(config)):
                    raise RunnerError("dry-run command shape exposed the task prompt")
                if (
                    record["tool_call_count"] != 1
                    or len(record["tool_results"]) != 1
                    or record["assistant_turn_count"] != 2
                    or record["tool_results"][0]["content_cl100k"] <= 0
                    or record["tool_results"][0]["content_o200k"] <= 0
                    or record["mutation_policy"]["status"] != "compliant"
                    or record["git_before"]["diff_sha256"]
                    != record["git_after"]["diff_sha256"]
                ):
                    raise RunnerError("Claude stream trace invariant failed")
                records.append(record)

            if records[0]["policy"] != records[1]["policy"]:
                raise RunnerError("paired-arm common policy invariant failed")
            if not all(
                record["num_turns"] == 2
                and record["total_cost_usd"] == 0.0125
                and record["structured_answer_valid"] is True
                and isinstance(record["usage"], dict)
                and isinstance(record["modelUsage"], dict)
                for record in records
            ):
                raise RunnerError("Claude JSON capture invariant failed")

            no_source_repo = root / "work" / "no-source-violation"
            initialize_test_repo(no_source_repo)
            no_source_case = {
                **case,
                "id": "SF-fake-no-source",
                "task_prompt": "FAKE_MUTATE",
            }
            no_source_record, no_source_exit = run_trial(
                fake_config(no_source_case, no_source_repo, "no-source-violation")
            )
            if (
                no_source_exit == 0
                or no_source_record["status"] != "policy_violation"
                or "violation.txt"
                not in no_source_record["mutation_policy"]["violating_paths"]
            ):
                raise RunnerError("read-only Git mutation invariant failed")

            escape_repo = root / "work" / "allowlist-escape"
            initialize_test_repo(escape_repo)
            escape_case = {
                **case,
                "id": "SF-fake-allowlist-escape",
                "task_prompt": "FAKE_MUTATE",
                "source_hash_policy": "only_allowlisted_paths_may_change",
                "mutation_allowlist": ["allowed.txt"],
            }
            escape_record, escape_exit = run_trial(
                fake_config(escape_case, escape_repo, "allowlist-escape")
            )
            if (
                escape_exit == 0
                or escape_record["mutation_policy"]["status"] != "violation"
                or "violation.txt"
                not in escape_record["mutation_policy"]["violating_paths"]
            ):
                raise RunnerError("mutation allowlist escape invariant failed")

            limit_repo = root / "work" / "call-limit"
            initialize_test_repo(limit_repo)
            limit_case = {
                **case,
                "id": "SF-fake-call-limit",
                "task_prompt": "FAKE_TWO_TOOLS",
                "limits": {"timeout_seconds": 10, "call_limit": 1},
            }
            limit_record, limit_exit = run_trial(
                fake_config(limit_case, limit_repo, "call-limit")
            )
            if (
                limit_exit == 0
                or limit_record["status"] != "tool_call_limit_exceeded"
                or limit_record["tool_call_count"] != 2
                or limit_record["policy_violation"] is not True
            ):
                raise RunnerError("tool-call limit enforcement invariant failed")
        finally:
            for name, previous in previous_env.items():
                if previous is None:
                    os.environ.pop(name, None)
                else:
                    os.environ[name] = previous
    print("self-test: PASS")
    return 0


def main(argv: list[str] | None = None) -> int:
    arguments = sys.argv[1:] if argv is None else argv
    if arguments and arguments[0] == "--_fake-claude":
        return fake_claude(arguments[1:])
    parser = build_parser()
    args = parser.parse_args(arguments)
    if args.mode == "self-test":
        return self_test()
    config = make_config(args)
    if args.dry_run:
        sanitizer = Sanitizer()
        print(canonical_json(sanitizer.sanitize_obj(command_shape(config), "dry_run")))
        return 0
    record, exit_code = run_trial(config)
    print(f"trial captured: status={record['status']}")
    return exit_code


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except RunnerError as exc:
        print(f"claude_task_runner: {exc}", file=sys.stderr)
        raise SystemExit(2) from None
