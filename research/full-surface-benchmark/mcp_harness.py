# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "tiktoken==0.13.0",
# ]
# ///
"""Sanitized stdio MCP capture harness for SFBENCH-1.0.

Run with ``uv run mcp_harness.py --help``.  The harness deliberately supports
stdio only.  Streamable HTTP parity should use the official MCP Python SDK in a
separate runner instead of approximating the protocol here.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import platform
import queue
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from dataclasses import dataclass
from typing import Any

try:
    import tiktoken
except ImportError:  # pragma: no cover - exercised only when not run through uv
    tiktoken = None  # type: ignore[assignment]


PROTOCOL_ID = "SFBENCH-1.0"
MCP_PROTOCOL_VERSION = "2025-06-18"
REDACTED = "<redacted>"
EXPECTED_TIKTOKEN_VERSION = "0.13.0"
EXPECTED_VOCABULARY_SHA256 = {
    "cl100k": "83d6504082a038c7aa769af6e2808471f94bf303ac150db2060cfc2a57020aa4",
    "o200k": "5d2372a38d68a4f7d6b65369719645d2aad38694af99b2a5274f5649ca47f591",
}
EXPECTED_TOOLS = {
    "full": {
        "analyze_file_impact",
        "ask",
        "batch_edit",
        "batch_insert",
        "batch_rename",
        "checkpoint_now",
        "context_inventory",
        "conventions",
        "delete_symbol",
        "detect_impact",
        "diff_symbols",
        "edit_plan",
        "edit_within_symbol",
        "explore",
        "find_dependents",
        "find_references",
        "get_file_content",
        "get_file_context",
        "get_repo_map",
        "get_symbol",
        "get_symbol_context",
        "health",
        "health_compact",
        "index_folder",
        "insert_symbol",
        "inspect_match",
        "investigation_suggest",
        "replace_symbol_body",
        "search_files",
        "search_symbols",
        "search_text",
        "status",
        "symforge_edit",
        "symforge_retrieve",
        "validate_file_syntax",
        "what_changed",
    },
    "compact": {"status", "symforge", "symforge_edit"},
    "meta": {"symforge"},
}
LIST_METHODS = (
    ("tools/list", "tools"),
    ("resources/list", "resources"),
    ("resources/templates/list", "resourceTemplates"),
    ("prompts/list", "prompts"),
)


class HarnessError(RuntimeError):
    """An expected, safe-to-display harness failure."""


def canonical_json(value: Any) -> str:
    return json.dumps(
        value,
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
    )


def wire_json(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":")) + "\n"


def sha256_text(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


class TokenCounter:
    """Count the same text independently with cl100k_base and o200k_base."""

    def __init__(self) -> None:
        if tiktoken is None:
            raise HarnessError(
                "tiktoken is unavailable; run this PEP 723 script with `uv run`."
            )
        if getattr(tiktoken, "__version__", None) != EXPECTED_TIKTOKEN_VERSION:
            raise HarnessError(
                f"campaign requires tiktoken {EXPECTED_TIKTOKEN_VERSION}"
            )
        self.encodings = {
            "cl100k": tiktoken.get_encoding("cl100k_base"),
            "o200k": tiktoken.get_encoding("o200k_base"),
        }
        self.vocabulary_hashes = {
            name: self._encoding_hash(encoding)
            for name, encoding in self.encodings.items()
        }
        if self.vocabulary_hashes != EXPECTED_VOCABULARY_SHA256:
            raise HarnessError(
                "tokenizer vocabulary does not match the frozen campaign"
            )

    def metrics(self, text: str, *, byte_length: int | None = None) -> dict[str, int]:
        return {
            "utf8_bytes": (
                len(text.encode("utf-8")) if byte_length is None else byte_length
            ),
            "cl100k": len(self.encodings["cl100k"].encode(text, disallowed_special=())),
            "o200k": len(self.encodings["o200k"].encode(text, disallowed_special=())),
        }

    def metadata(self) -> dict[str, Any]:
        return {
            "package": "tiktoken",
            "version": EXPECTED_TIKTOKEN_VERSION,
            "encodings": {
                name: {
                    "name": encoding.name,
                    "vocabulary_sha256": self.vocabulary_hashes[name],
                }
                for name, encoding in self.encodings.items()
            },
        }

    @staticmethod
    def _encoding_hash(encoding: Any) -> str | None:
        """Hash tokenizer tables without relying on a cache-file location."""
        ranks = getattr(encoding, "_mergeable_ranks", None)
        special = getattr(encoding, "_special_tokens", None)
        if not isinstance(ranks, dict) or not isinstance(special, dict):
            return None
        digest = hashlib.sha256()
        for token, rank in sorted(ranks.items(), key=lambda item: item[1]):
            digest.update(int(rank).to_bytes(8, "big", signed=False))
            digest.update(len(token).to_bytes(8, "big", signed=False))
            digest.update(token)
        for token, rank in sorted(special.items()):
            encoded = token.encode("utf-8")
            digest.update(len(encoded).to_bytes(8, "big", signed=False))
            digest.update(encoded)
            digest.update(int(rank).to_bytes(8, "big", signed=False))
        return digest.hexdigest()


class Sanitizer:
    """Redact secret-shaped values while retaining rule/location evidence."""

    _sensitive_name_pattern = r"""
        (?:
            [a-z][a-z0-9]*(?:[_-](?:token|key|secret|password|passwd)) |
            (?:api|access|refresh|auth|client|private)[_-]?
                (?:key|token|secret|password) |
            password | passwd | authorization | credential | cookie
        )
    """
    _quoted_assignments = (
        r"""(?ix)
        (?P<prefix>
            \b"""
        + _sensitive_name_pattern
        + r"""\b\s*(?:=|:)\s*
        )
        (?P<quote>")(?P<value>(?:\\.|[^"\\])*)(?P=quote)
        """,
        r"""(?ix)
        (?P<prefix>
            \b"""
        + _sensitive_name_pattern
        + r"""\b\s*(?:=|:)\s*
        )
        (?P<quote>')(?P<value>(?:\\.|[^'\\])*)(?P=quote)
        """,
    )
    _quoted_assignments = (
        re.compile(_quoted_assignments[0]),
        re.compile(_quoted_assignments[1]),
    )
    _bare_assignment = re.compile(
        r"""(?ix)
        (?P<prefix>
            \b"""
        + _sensitive_name_pattern
        + r"""\b\s*(?:=|:)\s*
        )
        (?P<quote>)
        (?P<value>[^\s\\"',;}\]]+)
        (?P=quote)
        """
    )
    _bearer = re.compile(
        r"(?i)(?P<prefix>\b(?:authorization\s*:\s*)?bearer\s+)"
        r"(?P<value>[A-Za-z0-9._~+/=-]{8,})"
    )
    _credentialed_url = re.compile(
        r"(?i)(?P<prefix>[a-z][a-z0-9+.-]*://[^/\s:@]+:)"
        r"(?P<value>[^@\s/]+)(?P<suffix>@)"
    )
    _private_key = re.compile(
        r"(?is)-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----.*?"
        r"-----END [A-Z0-9 ]*PRIVATE KEY-----"
    )
    _unterminated_private_key = re.compile(
        r"(?is)-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----.*\Z"
    )
    _known_tokens = (
        re.compile(r"\bgithub_pat_[A-Za-z0-9_]{20,}\b"),
        re.compile(r"\bgh[pousr]_[A-Za-z0-9]{20,}\b"),
        re.compile(r"\bsk-(?:proj-)?[A-Za-z0-9_-]{20,}\b"),
        re.compile(r"\bAKIA[0-9A-Z]{16}\b"),
        re.compile(
            r"\b[A-Za-z0-9_-]{16,}\.[A-Za-z0-9_-]{16,}\."
            r"[A-Za-z0-9_-]{16,}\b"
        ),
    )

    def __init__(self) -> None:
        self._events: list[dict[str, str]] = []
        self._lock = threading.Lock()

    def event_count(self) -> int:
        with self._lock:
            return len(self._events)

    def events_since(self, offset: int) -> list[dict[str, str]]:
        with self._lock:
            return [dict(event) for event in self._events[offset:]]

    def _record(self, rule: str, location: str) -> None:
        with self._lock:
            self._events.append({"rule": rule, "location": location})

    @staticmethod
    def _sensitive_key(key: str) -> bool:
        normalized = key.lower().replace("-", "_")
        parts = normalized.split("_")
        if normalized in {
            "authorization",
            "cookie",
            "credential",
            "credentials",
            "password",
            "passwd",
            "pwd",
            "secret",
            "token",
        }:
            return True
        if len(parts) > 1 and parts[-1] in {
            "key",
            "password",
            "passwd",
            "secret",
            "token",
        }:
            return True
        collapsed = re.sub(r"[^A-Za-z0-9]", "", key)
        return bool(
            re.search(
                r"(?:api|access|refresh|auth|client|private)"
                r"(?:key|token|secret|password)$",
                collapsed,
                re.IGNORECASE,
            )
        )

    @classmethod
    def _sensitive_cli_flag(cls, value: str) -> bool:
        flag = value.strip().split("=", 1)[0]
        return flag.startswith("-") and cls._sensitive_key(flag.lstrip("-"))

    def sanitize_text(self, text: str, location: str) -> str:
        def located(rule: str, start: int) -> None:
            line = text.count("\n", 0, start) + 1
            self._record(rule, f"{location}:line:{line}")

        def assignment_replacement(match: re.Match[str]) -> str:
            if match.group("value") in {"", REDACTED, "null", "None"}:
                return match.group(0)
            located("secret_name_assignment", match.start())
            quote = match.group("quote")
            return f"{match.group('prefix')}{quote}{REDACTED}{quote}"

        sanitized = text
        for pattern in self._quoted_assignments:
            sanitized = pattern.sub(assignment_replacement, sanitized)
        sanitized = self._bare_assignment.sub(assignment_replacement, sanitized)

        def bearer_replacement(match: re.Match[str]) -> str:
            located("bearer_token", match.start())
            return f"{match.group('prefix')}{REDACTED}"

        sanitized = self._bearer.sub(bearer_replacement, sanitized)

        def url_replacement(match: re.Match[str]) -> str:
            located("credentialed_url", match.start())
            return f"{match.group('prefix')}{REDACTED}{match.group('suffix')}"

        sanitized = self._credentialed_url.sub(url_replacement, sanitized)

        def private_key_replacement(match: re.Match[str]) -> str:
            located("private_key", match.start())
            return REDACTED

        sanitized = self._private_key.sub(private_key_replacement, sanitized)
        sanitized = self._unterminated_private_key.sub(
            private_key_replacement, sanitized
        )

        for pattern in self._known_tokens:

            def known_replacement(
                match: re.Match[str], *, _pattern: Any = pattern
            ) -> str:
                del _pattern
                located("known_token_shape", match.start())
                return REDACTED

            sanitized = pattern.sub(known_replacement, sanitized)
        return sanitized

    def sanitize_obj(self, value: Any, location: str = "record") -> Any:
        if isinstance(value, dict):
            result: dict[str, Any] = {}
            for key, child in value.items():
                key_text = str(key)
                child_location = f"{location}.{key_text}"
                if self._sensitive_key(key_text) and not isinstance(child, dict):
                    already_safe = child is None or child == REDACTED
                    if isinstance(child, list):
                        already_safe = all(
                            item is None or item == REDACTED for item in child
                        )
                    if already_safe:
                        result[key_text] = child
                        continue
                    self._record("secret_named_field", f"{child_location}:line:1")
                    if isinstance(child, list):
                        result[key_text] = [REDACTED for _ in child]
                    elif child is None:
                        result[key_text] = None
                    else:
                        result[key_text] = REDACTED
                else:
                    result[key_text] = self.sanitize_obj(child, child_location)
            return result
        if isinstance(value, list):
            result: list[Any] = []
            redact_next = False
            for index, child in enumerate(value):
                child_location = f"{location}[{index}]"
                if redact_next:
                    if child is None or child == REDACTED:
                        result.append(child)
                    else:
                        self._record("split_cli_secret", f"{child_location}:line:1")
                        result.append(REDACTED)
                    redact_next = False
                    continue
                result.append(self.sanitize_obj(child, child_location))
                if isinstance(child, str) and self._sensitive_cli_flag(child):
                    redact_next = "=" not in child
            return result
        if isinstance(value, str):
            return self.sanitize_text(value, location)
        return value


class JsonlWriter:
    def __init__(
        self,
        path: pathlib.Path,
        sanitizer: Sanitizer,
        *,
        append: bool,
    ) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        try:
            self._handle = path.open(
                "a" if append else "x", encoding="utf-8", newline="\n"
            )
        except FileExistsError as exc:
            raise HarnessError(
                "output already exists; pass --append only for an intentional continuation"
            ) from exc
        self.path = path
        self.sanitizer = sanitizer
        self.rows = 0

    def write(self, record: dict[str, Any]) -> None:
        before = self.sanitizer.event_count()
        safe = self.sanitizer.sanitize_obj(record, f"jsonl[{self.rows}]")
        record_events = self.sanitizer.events_since(before)
        if record_events:
            safe.setdefault("sanitizer", {})["record_redactions"] = record_events
        serialized = canonical_json(safe)
        final_scan = Sanitizer()
        rescanned_obj = final_scan.sanitize_obj(safe, f"jsonl[{self.rows}].final")
        rescanned = final_scan.sanitize_text(
            canonical_json(rescanned_obj), f"jsonl[{self.rows}].final_serialized"
        )
        if rescanned != serialized or final_scan.event_count():
            rules = sorted({event["rule"] for event in final_scan.events_since(0)})
            raise HarnessError(
                "final serialized-row secret scan failed; row was not persisted "
                f"(record_type: {safe.get('record_type', 'unknown')}, "
                f"rules: {','.join(rules)})"
            )
        self._handle.write(serialized + "\n")
        self._handle.flush()
        self.rows += 1

    def close(self) -> None:
        self._handle.close()


@dataclass
class RpcCapture:
    method: str
    request_id: int | None
    rpc_ms: float
    request_safe: dict[str, Any]
    response_safe: dict[str, Any] | None
    response_raw: dict[str, Any] | None
    content_safe: Any
    full_wire_safe: dict[str, list[str]]
    request_metrics: dict[str, int]
    response_metrics: dict[str, int]
    content_metrics: dict[str, int]
    full_wire_metrics: dict[str, int]
    sanitized_request_metrics: dict[str, int]
    sanitized_response_metrics: dict[str, int]
    sanitized_content_metrics: dict[str, int]
    sanitized_full_wire_metrics: dict[str, int]
    schema_free_request_metrics: dict[str, int] | None
    schema_free_response_metrics: dict[str, int] | None
    sanitized_schema_free_request_metrics: dict[str, int] | None
    sanitized_schema_free_response_metrics: dict[str, int] | None
    direct_payload_metrics: dict[str, int] | None
    sanitized_direct_payload_metrics: dict[str, int] | None
    redactions: list[dict[str, str]]


def extract_content(response: dict[str, Any] | None) -> Any:
    if not isinstance(response, dict):
        return None
    result = response.get("result")
    if not isinstance(result, dict):
        return None
    extracted: dict[str, Any] = {}
    if "content" in result:
        extracted["content"] = result["content"]
    if "structuredContent" in result:
        extracted["structuredContent"] = result["structuredContent"]
    return extracted or None


def extract_content_text(response: dict[str, Any] | None) -> str:
    if not isinstance(response, dict):
        return ""
    result = response.get("result")
    if not isinstance(result, dict):
        error = response.get("error")
        return canonical_json(error) if isinstance(error, dict) else ""
    content = result.get("content")
    texts = (
        [
            item["text"]
            for item in content
            if isinstance(item, dict) and isinstance(item.get("text"), str)
        ]
        if isinstance(content, list)
        else []
    )
    if "structuredContent" in result:
        texts.append(canonical_json(result["structuredContent"]))
    if not texts and result.get("isError") is True:
        texts.append(canonical_json(result))
    return "\n".join(texts)


def validate_jsonrpc_response(message: Any, expected_id: int) -> None:
    if not isinstance(message, dict) or message.get("jsonrpc") != "2.0":
        raise HarnessError("invalid JSON-RPC 2.0 response envelope")
    response_id = message.get("id")
    if type(response_id) is not type(expected_id) or response_id != expected_id:
        raise HarnessError("JSON-RPC response id did not match the request")
    has_result = "result" in message
    has_error = "error" in message
    if has_result == has_error:
        raise HarnessError("JSON-RPC response must contain exactly one result or error")
    if has_error:
        error = message["error"]
        if (
            not isinstance(error, dict)
            or not isinstance(error.get("code"), int)
            or isinstance(error.get("code"), bool)
            or not isinstance(error.get("message"), str)
        ):
            raise HarnessError("invalid JSON-RPC error object")


class StdioMcpClient:
    def __init__(
        self,
        command: list[str],
        cwd: pathlib.Path,
        env: dict[str, str],
        timeout: float,
        sanitizer: Sanitizer,
        counter: TokenCounter,
    ) -> None:
        self.timeout = timeout
        self.sanitizer = sanitizer
        self.counter = counter
        self._next_id = 1
        self._stdout: queue.Queue[bytes | None] = queue.Queue()
        self._stderr_lines: list[str] = []
        self._stderr_lock = threading.Lock()
        self._stderr_line_number = 0
        started = time.perf_counter_ns()
        creationflags = 0
        if os.name == "nt":
            creationflags = getattr(subprocess, "CREATE_NO_WINDOW", 0)
        try:
            self.process = subprocess.Popen(
                command,
                cwd=cwd,
                env=env,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                bufsize=0,
                creationflags=creationflags,
            )
        except OSError as exc:
            raise HarnessError(
                "failed to start the supplied server executable"
            ) from exc
        self.process_start_ms = (time.perf_counter_ns() - started) / 1_000_000
        self._stdout_thread = threading.Thread(
            target=self._read_stdout, name="mcp-stdout", daemon=True
        )
        self._stderr_thread = threading.Thread(
            target=self._read_stderr, name="mcp-stderr", daemon=True
        )
        self._stdout_thread.start()
        self._stderr_thread.start()

    def _read_stdout(self) -> None:
        assert self.process.stdout is not None
        while True:
            line = self.process.stdout.readline()
            if not line:
                self._stdout.put(None)
                return
            self._stdout.put(line)

    def _read_stderr(self) -> None:
        assert self.process.stderr is not None
        while True:
            line = self.process.stderr.readline()
            if not line:
                return
            decoded = line.decode("utf-8", errors="replace")
            with self._stderr_lock:
                self._stderr_line_number += 1
                location = f"server_stderr:{self._stderr_line_number}"
            safe = self.sanitizer.sanitize_text(decoded, location)
            with self._stderr_lock:
                self._stderr_lines.append(safe)

    def rpc(self, method: str, params: dict[str, Any]) -> RpcCapture:
        request_id = self._next_id
        self._next_id += 1
        request = {
            "jsonrpc": "2.0",
            "id": request_id,
            "method": method,
            "params": params,
        }
        return self._exchange(request, method, request_id)

    def notify(self, method: str, params: dict[str, Any]) -> RpcCapture:
        request = {"jsonrpc": "2.0", "method": method, "params": params}
        return self._exchange(request, method, None)

    def _exchange(
        self,
        request: dict[str, Any],
        method: str,
        request_id: int | None,
    ) -> RpcCapture:
        redaction_offset = self.sanitizer.event_count()
        request_raw_wire = wire_json(request)
        request_raw_bytes = request_raw_wire.encode("utf-8")
        request_safe = self.sanitizer.sanitize_obj(request, f"{method}.request")
        request_safe_wire = wire_json(request_safe)
        inbound_raw: list[str] = []
        inbound_raw_bytes = 0
        inbound_safe: list[str] = []
        response_raw: dict[str, Any] | None = None
        response_safe: dict[str, Any] | None = None
        response_raw_text = ""
        response_raw_bytes = 0

        assert self.process.stdin is not None
        start = time.perf_counter_ns()
        try:
            self.process.stdin.write(request_raw_bytes)
            self.process.stdin.flush()
        except (BrokenPipeError, OSError) as exc:
            raise HarnessError(
                "server stdio closed while sending an MCP request"
            ) from exc

        if request_id is not None:
            deadline = time.monotonic() + self.timeout
            unsolicited = 0
            while response_raw is None:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise HarnessError(f"MCP RPC timed out for method {method}")
                try:
                    raw_line = self._stdout.get(timeout=remaining)
                except queue.Empty as exc:
                    raise HarnessError(
                        f"MCP RPC timed out for method {method}"
                    ) from exc
                if raw_line is None:
                    raise HarnessError("server stdout closed before the MCP response")
                decoded = raw_line.decode("utf-8", errors="replace")
                inbound_raw.append(decoded)
                inbound_raw_bytes += len(raw_line)
                try:
                    message = json.loads(decoded)
                except json.JSONDecodeError:
                    inbound_safe.append(
                        self.sanitizer.sanitize_text(
                            decoded, f"{method}.stdout_non_json"
                        )
                    )
                    unsolicited += 1
                    if unsolicited > 100:
                        raise HarnessError(
                            "too many non-response frames on server stdout"
                        )
                    continue
                safe_message = self.sanitizer.sanitize_obj(
                    message, f"{method}.response_frame"
                )
                inbound_safe.append(wire_json(safe_message))
                if (
                    isinstance(message, dict)
                    and "id" in message
                    and "method" in message
                ):
                    raise HarnessError(
                        "server-initiated MCP requests are unsupported by this capture harness"
                    )
                if isinstance(message, dict) and "id" in message:
                    validate_jsonrpc_response(message, request_id)
                    response_raw = message
                    response_safe = safe_message
                    response_raw_text = decoded
                    response_raw_bytes = len(raw_line)
                    break
                unsolicited += 1
                if unsolicited > 100:
                    raise HarnessError("too many unsolicited MCP frames")

        rpc_ms = (time.perf_counter_ns() - start) / 1_000_000
        response_safe_wire = (
            wire_json(response_safe) if response_safe is not None else ""
        )
        content_raw = extract_content(response_raw)
        content_safe = extract_content(response_safe)
        content_raw_text = (
            canonical_json(content_raw) if content_raw is not None else ""
        )
        content_safe_text = (
            canonical_json(content_safe) if content_safe is not None else ""
        )
        full_raw_text = request_raw_wire + "".join(inbound_raw)
        full_safe_text = request_safe_wire + "".join(inbound_safe)

        direct_raw_metrics: dict[str, int] | None = None
        direct_safe_metrics: dict[str, int] | None = None
        schema_free_request_metrics: dict[str, int] | None = None
        schema_free_response_metrics: dict[str, int] | None = None
        safe_schema_free_request_metrics: dict[str, int] | None = None
        safe_schema_free_response_metrics: dict[str, int] | None = None
        if method == "tools/call":
            name = str(request.get("params", {}).get("name", ""))
            arguments = request.get("params", {}).get("arguments", {})
            safe_params = request_safe.get("params", {})
            safe_arguments = (
                safe_params.get("arguments", {})
                if isinstance(safe_params, dict)
                else {}
            )
            schema_free_request = name + "\n" + canonical_json(arguments)
            schema_free_response = extract_content_text(response_raw)
            safe_schema_free_request = name + "\n" + canonical_json(safe_arguments)
            safe_schema_free_response = extract_content_text(response_safe)
            direct_raw = schema_free_request + "\n" + schema_free_response
            direct_safe = safe_schema_free_request + "\n" + safe_schema_free_response
            schema_free_request_metrics = self.counter.metrics(schema_free_request)
            schema_free_response_metrics = self.counter.metrics(schema_free_response)
            safe_schema_free_request_metrics = self.counter.metrics(
                safe_schema_free_request
            )
            safe_schema_free_response_metrics = self.counter.metrics(
                safe_schema_free_response
            )
            direct_raw_metrics = self.counter.metrics(direct_raw)
            direct_safe_metrics = self.counter.metrics(direct_safe)

        return RpcCapture(
            method=method,
            request_id=request_id,
            rpc_ms=rpc_ms,
            request_safe=request_safe,
            response_safe=response_safe,
            response_raw=response_raw,
            content_safe=content_safe,
            full_wire_safe={
                "outbound": [request_safe_wire],
                "inbound": inbound_safe,
            },
            request_metrics=self.counter.metrics(
                request_raw_wire, byte_length=len(request_raw_bytes)
            ),
            response_metrics=self.counter.metrics(
                response_raw_text, byte_length=response_raw_bytes
            ),
            content_metrics=self.counter.metrics(content_raw_text),
            full_wire_metrics=self.counter.metrics(
                full_raw_text,
                byte_length=len(request_raw_bytes) + inbound_raw_bytes,
            ),
            sanitized_request_metrics=self.counter.metrics(request_safe_wire),
            sanitized_response_metrics=self.counter.metrics(response_safe_wire),
            sanitized_content_metrics=self.counter.metrics(content_safe_text),
            sanitized_full_wire_metrics=self.counter.metrics(full_safe_text),
            schema_free_request_metrics=schema_free_request_metrics,
            schema_free_response_metrics=schema_free_response_metrics,
            sanitized_schema_free_request_metrics=safe_schema_free_request_metrics,
            sanitized_schema_free_response_metrics=safe_schema_free_response_metrics,
            direct_payload_metrics=direct_raw_metrics,
            sanitized_direct_payload_metrics=direct_safe_metrics,
            redactions=self.sanitizer.events_since(redaction_offset),
        )

    def close(self) -> dict[str, Any]:
        started = time.perf_counter_ns()
        if self.process.stdin is not None and not self.process.stdin.closed:
            try:
                self.process.stdin.close()
            except OSError:
                pass
        try:
            exit_code = self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            try:
                exit_code = self.process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                self.process.kill()
                exit_code = self.process.wait(timeout=2)
        self._stdout_thread.join(timeout=1)
        self._stderr_thread.join(timeout=1)

        trailing_stdout: list[str] = []
        while True:
            try:
                raw_line = self._stdout.get_nowait()
            except queue.Empty:
                break
            if raw_line is None:
                continue
            trailing_stdout.append(
                self.sanitizer.sanitize_text(
                    raw_line.decode("utf-8", errors="replace"),
                    "trailing_stdout",
                )
            )
        with self._stderr_lock:
            stderr = list(self._stderr_lines)
        return {
            "exit_code": exit_code,
            "shutdown_ms": (time.perf_counter_ns() - started) / 1_000_000,
            "server_stderr": stderr,
            "trailing_stdout": trailing_stdout,
        }


@dataclass
class CaptureConfig:
    mode: str
    repo: pathlib.Path
    output: pathlib.Path
    server: str
    server_args: list[str]
    surface: str
    auto_index: bool
    timeout: float
    protocol_version: str
    run_id: str
    case_id: str
    append: bool
    tool: str | None = None
    arguments: dict[str, Any] | None = None
    execution_mode: str = "direct_rpc"


def is_within(path: pathlib.Path, parent: pathlib.Path) -> bool:
    try:
        path.relative_to(parent)
        return True
    except ValueError:
        return False


def validate_paths(repo: pathlib.Path, output: pathlib.Path) -> None:
    if not repo.is_dir():
        raise HarnessError("--repo must name an existing directory")
    script_repo = pathlib.Path(__file__).resolve().parents[2]
    if is_within(output, repo) or is_within(output, script_repo):
        raise HarnessError(
            "capture output must be outside both the tested repository and the SymForge checkout"
        )


def resolve_server(server: str) -> pathlib.Path:
    candidate = pathlib.Path(server).expanduser()
    if candidate.is_absolute() or candidate.parent != pathlib.Path("."):
        resolved = candidate.resolve()
        if not resolved.is_file():
            raise HarnessError("the supplied server executable does not exist")
        return resolved
    found = shutil.which(server)
    if found is None:
        raise HarnessError("the supplied server executable was not found on PATH")
    return pathlib.Path(found).resolve()


def executable_sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while chunk := handle.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def child_environment(surface: str, auto_index: bool) -> dict[str, str]:
    """Build a credential-minimized environment without ever serializing it."""
    allowed = (
        "PATH",
        "PATHEXT",
        "SystemRoot",
        "WINDIR",
        "COMSPEC",
        "TEMP",
        "TMP",
    )
    env = {key: os.environ[key] for key in allowed if key in os.environ}
    env.update(
        {
            "SYMFORGE_SURFACE": surface,
            "SYMFORGE_NO_DAEMON": "1",
            "SYMFORGE_AUTO_INDEX": "true" if auto_index else "false",
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_CONFIG_GLOBAL": os.devnull,
            "GIT_LFS_SKIP_SMUDGE": "1",
            "NO_COLOR": "1",
        }
    )
    if os.name != "nt":
        env["LANG"] = "C.UTF-8"
        env["LC_ALL"] = "C.UTF-8"
    return env


def repository_commit(repo: pathlib.Path, env: dict[str, str]) -> str | None:
    git = shutil.which("git")
    if git is None:
        return None
    try:
        result = subprocess.run(
            [git, "-C", str(repo), "rev-parse", "HEAD"],
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            timeout=5,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        return None
    candidate = result.stdout.decode("ascii", errors="ignore").strip()
    return candidate if re.fullmatch(r"[0-9a-fA-F]{40,64}", candidate) else None


def rpc_status(capture: RpcCapture) -> str:
    response = capture.response_safe
    if response is None:
        return "sent"
    if "error" in response:
        return "jsonrpc_error"
    result = response.get("result")
    if isinstance(result, dict) and result.get("isError") is True:
        return "tool_error"
    return "ok"


def base_record(config: CaptureConfig) -> dict[str, Any]:
    return {
        "protocol": PROTOCOL_ID,
        "run_id": config.run_id,
        "case_id": config.case_id,
        "repo": str(config.repo),
        "transport": "stdio",
        "surface": config.surface,
        "execution_mode": config.execution_mode,
    }


def capture_record(config: CaptureConfig, capture: RpcCapture) -> dict[str, Any]:
    response_wire = "".join(capture.full_wire_safe["inbound"])
    request_wire = "".join(capture.full_wire_safe["outbound"])
    tool = None
    params = capture.request_safe.get("params")
    if capture.method == "tools/call" and isinstance(params, dict):
        tool = params.get("name")
    record = {
        **base_record(config),
        "record_type": "trial" if capture.method == "tools/call" else "rpc",
        "method": capture.method,
        "tool": tool,
        "rpc_ms": capture.rpc_ms,
        "status": rpc_status(capture),
        "request_sha256": sha256_text(request_wire),
        "response_sha256": sha256_text(response_wire) if response_wire else "",
        "request_utf8_bytes": capture.request_metrics["utf8_bytes"],
        "response_utf8_bytes": capture.response_metrics["utf8_bytes"],
        "full_wire_utf8_bytes": capture.full_wire_metrics["utf8_bytes"],
        "request_cl100k": capture.request_metrics["cl100k"],
        "response_cl100k": capture.response_metrics["cl100k"],
        "request_o200k": capture.request_metrics["o200k"],
        "response_o200k": capture.response_metrics["o200k"],
        "content_cl100k": capture.content_metrics["cl100k"],
        "content_o200k": capture.content_metrics["o200k"],
        "full_wire_cl100k": capture.full_wire_metrics["cl100k"],
        "full_wire_o200k": capture.full_wire_metrics["o200k"],
        "wire_request_cl100k": capture.request_metrics["cl100k"],
        "wire_response_cl100k": capture.response_metrics["cl100k"],
        "wire_request_o200k": capture.request_metrics["o200k"],
        "wire_response_o200k": capture.response_metrics["o200k"],
        "sanitized_request_cl100k": capture.sanitized_request_metrics["cl100k"],
        "sanitized_response_cl100k": capture.sanitized_response_metrics["cl100k"],
        "sanitized_content_cl100k": capture.sanitized_content_metrics["cl100k"],
        "sanitized_full_wire_cl100k": capture.sanitized_full_wire_metrics["cl100k"],
        "sanitized_request_o200k": capture.sanitized_request_metrics["o200k"],
        "sanitized_response_o200k": capture.sanitized_response_metrics["o200k"],
        "sanitized_content_o200k": capture.sanitized_content_metrics["o200k"],
        "sanitized_full_wire_o200k": capture.sanitized_full_wire_metrics["o200k"],
        "schema_free_tool_request_cl100k": (
            capture.schema_free_request_metrics["cl100k"]
            if capture.schema_free_request_metrics is not None
            else None
        ),
        "schema_free_tool_request_utf8_bytes": (
            capture.schema_free_request_metrics["utf8_bytes"]
            if capture.schema_free_request_metrics is not None
            else None
        ),
        "schema_free_tool_request_o200k": (
            capture.schema_free_request_metrics["o200k"]
            if capture.schema_free_request_metrics is not None
            else None
        ),
        "schema_free_tool_response_cl100k": (
            capture.schema_free_response_metrics["cl100k"]
            if capture.schema_free_response_metrics is not None
            else None
        ),
        "schema_free_tool_response_utf8_bytes": (
            capture.schema_free_response_metrics["utf8_bytes"]
            if capture.schema_free_response_metrics is not None
            else None
        ),
        "schema_free_tool_response_o200k": (
            capture.schema_free_response_metrics["o200k"]
            if capture.schema_free_response_metrics is not None
            else None
        ),
        "schema_free_direct_payload_cl100k": (
            capture.direct_payload_metrics["cl100k"]
            if capture.direct_payload_metrics is not None
            else None
        ),
        "schema_free_direct_payload_utf8_bytes": (
            capture.direct_payload_metrics["utf8_bytes"]
            if capture.direct_payload_metrics is not None
            else None
        ),
        "schema_free_direct_payload_o200k": (
            capture.direct_payload_metrics["o200k"]
            if capture.direct_payload_metrics is not None
            else None
        ),
        "measurement_views": {
            "schema_free_direct_payload": (
                "tool name + canonical arguments + content[*].text + "
                "canonical structuredContent; canonical JSON-RPC/tool-error payload "
                "when result content is absent"
            ),
            "wire": "exact JSON-RPC request and response UTF-8",
            "interpretation": "diagnostic token counts, not a net-savings claim",
        },
        "token_count_basis": {
            "primary": "exact unsanitized UTF-8 stream counted only in memory",
            "artifact": "sanitized persisted stream",
        },
        "request": capture.request_safe,
        "response": capture.response_safe,
        "content": capture.content_safe,
        "full_wire": capture.full_wire_safe,
        "sanitizer": {
            "redaction_count": len(capture.redactions),
            "events": capture.redactions,
        },
        "provider_total_tokens": None,
        "cached_input_tokens": None,
        "reasoning_tokens": None,
    }
    if capture.sanitized_direct_payload_metrics is not None:
        record["sanitized_schema_free_direct_payload_cl100k"] = (
            capture.sanitized_direct_payload_metrics["cl100k"]
        )
        record["sanitized_schema_free_direct_payload_o200k"] = (
            capture.sanitized_direct_payload_metrics["o200k"]
        )
    if capture.sanitized_schema_free_request_metrics is not None:
        record["sanitized_schema_free_tool_request_cl100k"] = (
            capture.sanitized_schema_free_request_metrics["cl100k"]
        )
        record["sanitized_schema_free_tool_request_o200k"] = (
            capture.sanitized_schema_free_request_metrics["o200k"]
        )
    if capture.sanitized_schema_free_response_metrics is not None:
        record["sanitized_schema_free_tool_response_cl100k"] = (
            capture.sanitized_schema_free_response_metrics["cl100k"]
        )
        record["sanitized_schema_free_tool_response_o200k"] = (
            capture.sanitized_schema_free_response_metrics["o200k"]
        )
    return record


def list_all(
    client: StdioMcpClient,
    writer: JsonlWriter,
    config: CaptureConfig,
    method: str,
    result_key: str,
) -> tuple[list[Any], dict[str, Any]]:
    safe_items: list[Any] = []
    raw_items: list[Any] = []
    cursor: Any = None
    seen_cursors: set[str] = set()
    page_count = 0
    while page_count < 100:
        params = {} if cursor is None else {"cursor": cursor}
        capture = client.rpc(method, params)
        writer.write(capture_record(config, capture))
        page_count += 1
        raw_response = capture.response_raw or {}
        safe_response = capture.response_safe or {}
        if "error" in raw_response:
            raise HarnessError(f"{method} returned a JSON-RPC error")
        raw_result = raw_response.get("result", {})
        safe_result = safe_response.get("result", {})
        if not isinstance(raw_result, dict) or not isinstance(safe_result, dict):
            raise HarnessError(f"{method} result was not an object")
        page_raw_items = raw_result.get(result_key, [])
        page_safe_items = safe_result.get(result_key, [])
        if not isinstance(page_raw_items, list) or not isinstance(
            page_safe_items, list
        ):
            raise HarnessError(f"{method} did not return {result_key} as a list")
        raw_items.extend(page_raw_items)
        safe_items.extend(page_safe_items)
        cursor = raw_result.get("nextCursor")
        if cursor is None:
            break
        if not isinstance(cursor, str) or not cursor:
            raise HarnessError(f"{method} returned an invalid pagination cursor")
        cursor_fingerprint = sha256_text(canonical_json(cursor))
        if cursor_fingerprint in seen_cursors:
            raise HarnessError(f"{method} repeated a pagination cursor")
        seen_cursors.add(cursor_fingerprint)
    else:
        raise HarnessError(f"{method} pagination exceeded 100 pages")

    raw_text = canonical_json(raw_items)
    safe_text = canonical_json(safe_items)
    return safe_items, {
        "pages": page_count,
        "items": len(safe_items),
        "error": None,
        "raw_counts": client.counter.metrics(raw_text),
        "sanitized_counts": client.counter.metrics(safe_text),
        "sanitized_sha256": sha256_text(safe_text),
    }


def schema_metadata(tools: list[Any], counter: TokenCounter) -> list[dict[str, Any]]:
    metadata: list[dict[str, Any]] = []
    for tool in tools:
        if not isinstance(tool, dict):
            continue
        serialized = canonical_json(tool)
        counts = counter.metrics(serialized)
        metadata.append(
            {
                "name": tool.get("name"),
                "schema_sha256": sha256_text(serialized),
                "utf8_bytes": counts["utf8_bytes"],
                "cl100k": counts["cl100k"],
                "o200k": counts["o200k"],
            }
        )
    return metadata


def validate_tool_identity(surface: str, tools: list[Any]) -> None:
    names = [tool.get("name") for tool in tools if isinstance(tool, dict)]
    if len(names) != len(tools) or not all(isinstance(name, str) for name in names):
        raise HarnessError("tools/list contained an invalid tool identity")
    if len(set(names)) != len(names):
        raise HarnessError("tools/list contained duplicate tool identities")
    if set(names) != EXPECTED_TOOLS[surface]:
        raise HarnessError(
            f"tools/list did not match the frozen {surface} surface identity"
        )


def run_capture(config: CaptureConfig) -> int:
    config.repo = config.repo.resolve()
    config.output = config.output.resolve()
    validate_paths(config.repo, config.output)
    if config.auto_index and config.repo.parent.name.casefold() == "sources":
        raise HarnessError(
            "auto-index requires a disposable case clone, not an immutable source mirror"
        )
    if config.timeout <= 0:
        raise HarnessError("--timeout must be greater than zero")

    server_path = resolve_server(config.server)
    counter = TokenCounter()
    sanitizer = Sanitizer()
    writer = JsonlWriter(config.output, sanitizer, append=config.append)
    env = child_environment(config.surface, config.auto_index)
    client: StdioMcpClient | None = None
    pending_error: BaseException | None = None

    try:
        writer.write(
            {
                **base_record(config),
                "record_type": "run_metadata",
                "python": platform.python_version(),
                "platform": platform.platform(),
                "tokenizer": counter.metadata(),
                "server_executable": str(server_path),
                "server_executable_sha256": executable_sha256(server_path),
                "server_argument_count": len(config.server_args),
                "repository_commit": repository_commit(config.repo, env),
                "runtime_policy": {
                    "surface": config.surface,
                    "no_daemon": True,
                    "auto_index": config.auto_index,
                },
                "http_streamable_supported": False,
            }
        )
        client = StdioMcpClient(
            [str(server_path), *config.server_args],
            config.repo,
            env,
            config.timeout,
            sanitizer,
            counter,
        )
        initialize = client.rpc(
            "initialize",
            {
                "protocolVersion": config.protocol_version,
                "capabilities": {},
                "clientInfo": {
                    "name": "symforge-sfbench-harness",
                    "version": "1.0",
                },
            },
        )
        writer.write(capture_record(config, initialize))
        if initialize.response_raw is None or "error" in initialize.response_raw:
            raise HarnessError("MCP initialize failed")
        initialized = client.notify("notifications/initialized", {})
        writer.write(capture_record(config, initialized))

        manifest: dict[str, list[Any]] = {}
        list_metrics: dict[str, Any] = {}
        for method, result_key in LIST_METHODS:
            items, metrics = list_all(client, writer, config, method, result_key)
            manifest[result_key] = items
            list_metrics[method] = metrics

        manifest_text = canonical_json(manifest)
        manifest_counts = counter.metrics(manifest_text)
        tools = manifest.get("tools", [])
        validate_tool_identity(config.surface, tools)
        tools_text = canonical_json(tools)
        tools_counts = counter.metrics(tools_text)
        writer.write(
            {
                **base_record(config),
                "record_type": "manifest",
                "manifest": manifest,
                "manifest_sha256": sha256_text(manifest_text),
                "manifest_utf8_bytes": manifest_counts["utf8_bytes"],
                "manifest_cl100k": manifest_counts["cl100k"],
                "manifest_o200k": manifest_counts["o200k"],
                "full_tools_list_tokens": {
                    "cl100k": tools_counts["cl100k"],
                    "o200k": tools_counts["o200k"],
                },
                "schema_metadata": schema_metadata(tools, counter),
                "list_metrics": list_metrics,
            }
        )

        if config.mode == "call":
            assert config.tool is not None
            arguments = config.arguments or {}
            call = client.rpc(
                "tools/call", {"name": config.tool, "arguments": arguments}
            )
            writer.write(capture_record(config, call))
    except BaseException as exc:
        pending_error = exc
        safe_message = str(exc) if isinstance(exc, HarnessError) else type(exc).__name__
        writer.write(
            {
                **base_record(config),
                "record_type": "harness_error",
                "error_type": type(exc).__name__,
                "message": safe_message,
            }
        )
    finally:
        if client is not None:
            writer.write(
                {
                    **base_record(config),
                    "record_type": "process_lifecycle",
                    "process_start_ms": client.process_start_ms,
                    **client.close(),
                }
            )
        rows = writer.rows
        writer.close()

    if pending_error is not None:
        if isinstance(pending_error, HarnessError):
            raise pending_error
        raise HarnessError(
            f"unexpected harness failure ({type(pending_error).__name__})"
        ) from pending_error
    return rows


def parse_arguments(args: argparse.Namespace) -> dict[str, Any]:
    if args.arguments_file is not None:
        try:
            text = pathlib.Path(args.arguments_file).read_text(encoding="utf-8")
        except OSError as exc:
            raise HarnessError("could not read --arguments-file") from exc
    else:
        text = args.arguments if args.arguments is not None else "{}"
    try:
        value = json.loads(text)
    except json.JSONDecodeError as exc:
        raise HarnessError(
            f"tool arguments are invalid JSON at line {exc.lineno}, column {exc.colno}"
        ) from exc
    if not isinstance(value, dict):
        raise HarnessError("tool arguments must be a JSON object")
    return value


def config_from_args(args: argparse.Namespace) -> CaptureConfig:
    arguments = parse_arguments(args) if args.mode == "call" else None
    return CaptureConfig(
        mode=args.mode,
        repo=pathlib.Path(args.repo),
        output=pathlib.Path(args.output),
        server=args.server,
        server_args=list(args.server_arg),
        surface=args.surface,
        auto_index=args.auto_index == "true",
        timeout=args.timeout,
        protocol_version=args.protocol_version,
        run_id=args.run_id,
        case_id=args.case_id,
        append=args.append,
        tool=getattr(args, "tool", None),
        arguments=arguments,
    )


def add_capture_options(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--repo", required=True, help="Repository used as server cwd.")
    parser.add_argument(
        "--output",
        required=True,
        help="Sanitized JSONL path outside both repositories.",
    )
    parser.add_argument(
        "--server",
        default="symforge",
        help="SymForge executable path or PATH name (default: symforge).",
    )
    parser.add_argument(
        "--server-arg",
        action="append",
        default=[],
        help="Repeat for server arguments; use --server-arg=--flag for flags.",
    )
    parser.add_argument(
        "--transport",
        choices=("stdio", "http"),
        default="stdio",
        help=(
            "Transport. Only stdio is implemented; http fails explicitly because "
            "Streamable HTTP requires the official MCP SDK parity runner."
        ),
    )
    parser.add_argument(
        "--surface", choices=("full", "compact", "meta"), default="full"
    )
    parser.add_argument("--auto-index", choices=("true", "false"), default="true")
    parser.add_argument("--timeout", type=float, default=120.0)
    parser.add_argument("--protocol-version", default=MCP_PROTOCOL_VERSION)
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--case-id", required=True)
    parser.add_argument(
        "--append",
        action="store_true",
        help="Append to an existing JSONL campaign intentionally.",
    )


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Capture sanitized SymForge MCP manifests and tool calls with exact "
            "in-memory cl100k/o200k accounting. Streamable HTTP is deliberately "
            "unsupported in this minimal harness; use an official-SDK parity runner."
        )
    )
    subparsers = parser.add_subparsers(dest="mode", required=True)

    manifest = subparsers.add_parser(
        "manifest",
        help="Initialize and capture tools/resources/templates/prompts lists.",
    )
    add_capture_options(manifest)

    call = subparsers.add_parser(
        "call",
        help="Capture the manifest, then make one tools/call in the same process.",
    )
    add_capture_options(call)
    call.add_argument("--tool", required=True)
    arguments = call.add_mutually_exclusive_group()
    arguments.add_argument(
        "--arguments", help="Tool arguments as one JSON object (default: {})."
    )
    arguments.add_argument(
        "--arguments-file", help="UTF-8 file containing one JSON object."
    )

    subparsers.add_parser(
        "self-test",
        help="Run an end-to-end fake-stdio transport and sanitizer test.",
    )
    return parser


def fake_server(mode: str = "normal") -> int:
    """Harmless internal MCP server used only by ``self-test``."""
    synthetic_value = "unit" + "-sensitive" + "-fixture"
    spaced_value = "unit" + " sensitive" + " fixture"
    for raw_line in sys.stdin.buffer:
        try:
            request = json.loads(raw_line)
        except json.JSONDecodeError:
            continue
        if "id" not in request:
            continue
        method = request.get("method")
        if method == "initialize":
            result: Any = {
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {"tools": {}, "resources": {}, "prompts": {}},
                "serverInfo": {"name": "sfbench-fake", "version": "0"},
            }
        elif method == "tools/list":
            tools: Any = [
                {
                    "name": name,
                    "description": "Harmless fake read tool.",
                    "inputSchema": {"type": "object", "properties": {}},
                }
                for name in sorted(
                    EXPECTED_TOOLS[os.environ.get("SYMFORGE_SURFACE", "full")]
                )
            ]
            if mode == "bad-list":
                tools = "not-a-list"
            result = {"tools": tools}
            if mode == "repeat-cursor":
                result["nextCursor"] = "repeated"
        elif method == "resources/list":
            result = {"resources": []}
        elif method == "resources/templates/list":
            result = {"resourceTemplates": []}
        elif method == "prompts/list":
            result = {"prompts": []}
        elif method == "tools/call":
            result = {
                "content": [
                    {
                        "type": "text",
                        "text": (f'API_TOKEN = "{spaced_value}" special=<|endoftext|>'),
                    }
                ],
                "structuredContent": {
                    "APIKey": synthetic_value,
                    "argv": ["--api-key", synthetic_value],
                    "nested": [["--clientSecret", synthetic_value]],
                    "answer": "ok",
                },
                "isError": False,
            }
        else:
            response = {
                "jsonrpc": "2.0",
                "id": request["id"],
                "error": {"code": -32601, "message": "Method not found"},
            }
            sys.stdout.buffer.write(wire_json(response).encode("utf-8"))
            sys.stdout.buffer.flush()
            continue
        response = {"jsonrpc": "2.0", "id": request["id"], "result": result}
        sys.stdout.buffer.write(wire_json(response).encode("utf-8"))
        sys.stdout.buffer.flush()
    return 0


def self_test() -> int:
    with tempfile.TemporaryDirectory(prefix="sfbench-harness-test-") as root_text:
        root = pathlib.Path(root_text)
        repo = root / "repo"
        repo.mkdir()
        sensitive_input = "request" + "-sensitive" + "-fixture"
        spaced_value = "unit" + " sensitive" + " fixture"
        counter = TokenCounter()
        if counter.metrics("<|endoftext|>")["cl100k"] <= 0:
            raise HarnessError("self-test special-token accounting invariant failed")

        sanitizer = Sanitizer()
        sanitized_probe = sanitizer.sanitize_obj(
            {
                "APIKey": sensitive_input,
                "argv": ["--api-key", sensitive_input],
                "nested": [["--clientSecret", sensitive_input]],
                "text": f'PASSWORD = "{spaced_value}"',
            },
            "self_test",
        )
        forbidden = {
            "unit" + "-sensitive" + "-fixture",
            sensitive_input,
            spaced_value,
        }
        if any(value in canonical_json(sanitized_probe) for value in forbidden):
            raise HarnessError("self-test sanitizer invariant failed")

        valid_response = {"jsonrpc": "2.0", "id": 1, "result": {}}
        validate_jsonrpc_response(valid_response, 1)
        invalid_responses = (
            {"id": 1, "result": {}},
            {"jsonrpc": "2.0", "id": True, "result": {}},
            {"jsonrpc": "2.0", "id": 1, "result": {}, "error": {}},
            {"jsonrpc": "2.0", "id": 1, "error": {"code": "bad"}},
        )
        for response in invalid_responses:
            try:
                validate_jsonrpc_response(response, 1)
            except HarnessError:
                pass
            else:
                raise HarnessError("self-test JSON-RPC validation invariant failed")

        jsonrpc_error = {
            "jsonrpc": "2.0",
            "id": 1,
            "error": {"code": -32602, "message": "synthetic invalid parameters"},
        }
        tool_level_error = {
            "jsonrpc": "2.0",
            "id": 2,
            "result": {"isError": True},
        }
        error_texts = (
            extract_content_text(jsonrpc_error),
            extract_content_text(tool_level_error),
        )
        if error_texts[0] != canonical_json(jsonrpc_error["error"]) or any(
            not text
            or counter.metrics(text)["cl100k"] <= 0
            or counter.metrics(text)["o200k"] <= 0
            for text in error_texts
        ):
            raise HarnessError("self-test schema-free error accounting failed")

        execution_mode_probe = CaptureConfig(
            mode="baseline",
            repo=repo,
            output=root / "unused.jsonl",
            server="",
            server_args=[],
            surface="full",
            auto_index=False,
            timeout=1,
            protocol_version=MCP_PROTOCOL_VERSION,
            run_id="self-test",
            case_id="execution-mode-probe",
            append=False,
            execution_mode="recipe_baseline",
        )
        if base_record(execution_mode_probe)["execution_mode"] != "recipe_baseline":
            raise HarnessError("self-test execution-mode override failed")

        class PassthroughSanitizer(Sanitizer):
            def sanitize_obj(self, value: Any, location: str = "record") -> Any:
                del location
                return value

        fail_path = root / "fail-closed.jsonl"
        fail_writer = JsonlWriter(fail_path, PassthroughSanitizer(), append=False)
        try:
            fail_writer.write({"password": sensitive_input})
        except HarnessError:
            pass
        else:
            raise HarnessError("self-test final secret scan invariant failed")
        finally:
            fail_writer.close()
        if fail_path.stat().st_size != 0:
            raise HarnessError("self-test fail-closed row was persisted")

        script = str(pathlib.Path(__file__).resolve())
        for surface in ("full", "compact", "meta"):
            output = root / f"capture-{surface}.jsonl"
            config = CaptureConfig(
                mode="call" if surface == "full" else "manifest",
                repo=repo,
                output=output,
                server=sys.executable,
                server_args=[script, "--_fake-server"],
                surface=surface,
                auto_index=True,
                timeout=10,
                protocol_version=MCP_PROTOCOL_VERSION,
                run_id="self-test",
                case_id=f"fake-{surface}",
                append=False,
                tool="health_compact" if surface == "full" else None,
                arguments={
                    "APIKey": sensitive_input,
                    "argv": ["--api-key", sensitive_input],
                },
            )
            run_capture(config)
            persisted = output.read_text(encoding="utf-8")
            if (
                any(value in persisted for value in forbidden)
                or "--_fake-server" in persisted
            ):
                raise HarnessError("self-test persisted sanitizer invariant failed")
            rows = [json.loads(line) for line in persisted.splitlines()]
            metadata = next(
                row for row in rows if row.get("record_type") == "run_metadata"
            )
            if (
                metadata.get("server_argument_count") != 2
                or "server_arguments" in metadata
            ):
                raise HarnessError("self-test server-argument privacy invariant failed")
            manifest = next(row for row in rows if row.get("record_type") == "manifest")
            names = {tool["name"] for tool in manifest["manifest"]["tools"]}
            if names != EXPECTED_TOOLS[surface]:
                raise HarnessError("self-test surface identity invariant failed")
            if surface != "full":
                continue
            methods = {row.get("method") for row in rows}
            required = {
                "initialize",
                "notifications/initialized",
                "tools/list",
                "resources/list",
                "resources/templates/list",
                "prompts/list",
                "tools/call",
            }
            if not required.issubset(methods):
                raise HarnessError("self-test MCP coverage invariant failed")
            trials = [row for row in rows if row.get("record_type") == "trial"]
            if len(trials) != 1 or REDACTED not in canonical_json(trials[0]):
                raise HarnessError("self-test persisted capture invariant failed")
            trial = trials[0]
            if not all(
                isinstance(trial.get(field), int) and trial[field] > 0
                for field in (
                    "wire_request_cl100k",
                    "wire_response_cl100k",
                    "schema_free_tool_request_cl100k",
                    "schema_free_tool_response_cl100k",
                    "schema_free_direct_payload_cl100k",
                )
            ):
                raise HarnessError("self-test token accounting invariant failed")
            structured = trial["response"]["result"].get("structuredContent")
            if not isinstance(structured, dict) or structured.get("APIKey") != REDACTED:
                raise HarnessError("self-test structuredContent invariant failed")

        for failure_mode in ("bad-list", "repeat-cursor"):
            failure_config = CaptureConfig(
                mode="manifest",
                repo=repo,
                output=root / f"failure-{failure_mode}.jsonl",
                server=sys.executable,
                server_args=[script, "--_fake-server", failure_mode],
                surface="full",
                auto_index=True,
                timeout=10,
                protocol_version=MCP_PROTOCOL_VERSION,
                run_id="self-test",
                case_id=failure_mode,
                append=False,
            )
            try:
                run_capture(failure_config)
            except HarnessError:
                pass
            else:
                raise HarnessError("self-test list failure was not fatal")

        try:
            validate_tool_identity("meta", [{"name": "unexpected"}])
        except HarnessError:
            pass
        else:
            raise HarnessError("self-test manifest rejection invariant failed")
    print("self-test: PASS")
    return 0


def main(argv: list[str] | None = None) -> int:
    arguments = sys.argv[1:] if argv is None else argv
    if arguments and arguments[0] == "--_fake-server":
        return fake_server(arguments[1] if len(arguments) > 1 else "normal")
    parser = build_parser()
    args = parser.parse_args(arguments)
    if args.mode == "self-test":
        return self_test()
    if args.transport == "http":
        raise HarnessError(
            "HTTP Streamable transport is unsupported in this harness; use the "
            "official MCP Python SDK parity runner described in --help."
        )
    rows = run_capture(config_from_args(args))
    print(f"capture complete: {rows} sanitized JSONL rows")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except HarnessError as exc:
        print(f"mcp_harness: {exc}", file=sys.stderr)
        raise SystemExit(2) from None
