"""Parse a client stream-json transcript into per-run metrics.

Adapted from docs/reviews/2026-11-09-product-audit/evidence/parse.py (copy in
audit_evidence/parse.py): usage is summed once per distinct assistant message
id, tool-result size is the character count of tool_result content, and the
answer is the final text. Differences: input is `claude -p --output-format
stream-json --verbose` stdout instead of a session subagent file, the answer is
taken from the `result` event, and the rule-file sentinel is counted in every
non-assistant event.
"""
from __future__ import annotations

import fnmatch
import json
from pathlib import Path
from typing import Any


def _text_len(content: Any) -> int:
    if isinstance(content, str):
        return len(content)
    if isinstance(content, list):
        return sum(len(x.get("text", "")) for x in content if isinstance(x, dict) and x.get("type") == "text")
    return 0


def parse_stream(lines: list[str], sentinel: str = "") -> dict[str, Any]:
    usage_by_msg: dict[str, dict] = {}
    tool_uses: dict[str, str] = {}
    tool_result_chars = 0
    sentinel_hits = 0
    hook_events = 0
    model = None
    client_version = None
    init_tools: list[str] = []
    result: dict[str, Any] = {}
    last_text = ""
    malformed = 0
    for i, line in enumerate(lines):
        line = line.strip()
        if not line:
            continue
        try:
            ev = json.loads(line)
        except json.JSONDecodeError:
            malformed += 1
            continue
        etype = ev.get("type")
        if etype != "assistant" and sentinel and sentinel in line:
            sentinel_hits += line.count(sentinel)
        if etype == "system":
            if ev.get("subtype") == "init":
                model = ev.get("model") or model
                client_version = ev.get("claude_code_version") or client_version
                init_tools = list(ev.get("tools") or [])
            elif "hook" in str(ev.get("subtype", "")):
                hook_events += 1
        elif etype == "assistant":
            msg = ev.get("message") or {}
            mid = msg.get("id") or f"anon-{i}"
            model = msg.get("model") or model
            usage_by_msg[mid] = msg.get("usage") or usage_by_msg.get(mid, {})
            for part in msg.get("content") or []:
                if part.get("type") == "tool_use":
                    tool_uses[part.get("id") or f"tu-{i}-{len(tool_uses)}"] = part.get("name", "?")
                elif part.get("type") == "text" and part.get("text", "").strip():
                    last_text = part["text"]
        elif etype == "user":
            content = (ev.get("message") or {}).get("content")
            if isinstance(content, list):
                for part in content:
                    if isinstance(part, dict) and part.get("type") == "tool_result":
                        tool_result_chars += _text_len(part.get("content"))
        elif etype == "result":
            result = ev
    u = list(usage_by_msg.values())
    s = lambda k: sum((x.get(k) or 0) for x in u)  # noqa: E731
    inp, cw, cr, out = s("input_tokens"), s("cache_creation_input_tokens"), s("cache_read_input_tokens"), s("output_tokens")
    tools: dict[str, int] = {}
    for name in tool_uses.values():
        tools[name] = tools.get(name, 0) + 1
    answer = result.get("result") if isinstance(result.get("result"), str) else last_text
    return {
        "model": model,
        "client_version": client_version,
        "init_tools": init_tools,
        "round_trips": len(u),
        "tool_calls": len(tool_uses),
        "tools": tools,
        "tool_result_chars": tool_result_chars,
        "tool_result_tokens_est": tool_result_chars // 4,
        "input_tokens": inp,
        "cache_write_tokens": cw,
        "cache_read_tokens": cr,
        "output_tokens": out,
        "total_tokens": inp + cw + cr + out,
        "wall_ms": result.get("duration_ms"),
        "cost_usd_reported": result.get("total_cost_usd"),
        "result_subtype": result.get("subtype"),
        "has_result": bool(result),
        "rule_sentinel_hits": sentinel_hits,
        "hook_events": hook_events,
        "malformed_lines": malformed,
        "answer": answer or "",
    }


def parse_file(path: Path, sentinel: str = "") -> dict[str, Any]:
    return parse_stream(path.read_text(encoding="utf-8", errors="replace").splitlines(), sentinel)


def allowed_patterns(arm: dict, tools_cfg: dict) -> list[str]:
    pats = list(tools_cfg["native"]) + list(tools_cfg.get("client_meta", []))
    if arm.get("mcp"):
        pats.append(tools_cfg["symforge_pattern"])
    return pats


def allowlist_violations(tools_used: dict[str, int], patterns: list[str]) -> list[str]:
    """Every tool name the transcript shows that no arm pattern admits."""
    return sorted(n for n in tools_used if not any(fnmatch.fnmatchcase(n, p) for p in patterns))
