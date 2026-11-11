"""Parse benchmark subagent transcripts into per-run metrics.

Usage: python -I parse.py <subagents_dir> <out_json> [--answers]
Reads agent-a[AB]-T*-r*[-model]*.jsonl. Prints one row per run.
"""
import json
import re
import sys
from datetime import datetime
from pathlib import Path

sub_dir = Path(sys.argv[1])
out_json = Path(sys.argv[2])
show_answers = "--answers" in sys.argv
only = None
for a in sys.argv[3:]:
    if a.startswith("--only="):
        only = a.split("=", 1)[1]

NAME_RE = re.compile(r"agent-a((?:[AB])-T\d+-r\d+(?:-[a-z]+)?)-[0-9a-f]+\.jsonl$")


def ts(s):
    return datetime.fromisoformat(s.replace("Z", "+00:00"))


rows = []
for f in sorted(sub_dir.glob("agent-a*.jsonl")):
    m = NAME_RE.search(f.name)
    if not m:
        continue
    name = m.group(1)
    if only and only not in name:
        continue
    usage_by_msg = {}
    order = []
    tools = {}
    tool_result_chars = 0
    hook_ctx_chars = 0
    first = last = None
    final_text = ""
    model = None
    answered_mid = None
    post_answer_turns = 0
    for line in f.read_text(encoding="utf-8", errors="replace").splitlines():
        try:
            rec = json.loads(line)
        except Exception:
            continue
        t = rec.get("timestamp")
        if t:
            try:
                tt = ts(t)
                first = first or tt
                last = tt
            except Exception:
                pass
        msg = rec.get("message") or {}
        if answered_mid is not None and not (
            rec.get("type") == "assistant" and msg.get("id") == answered_mid
        ):
            # Everything after the ANSWER message is supervisor-gate recheck noise.
            if rec.get("type") == "assistant" and msg.get("id") not in usage_by_msg:
                post_answer_turns += 1
                usage_by_msg.setdefault("post:" + str(msg.get("id")), {})
            continue
        if rec.get("type") == "assistant":
            mid = msg.get("id")
            model = msg.get("model") or model
            u = msg.get("usage") or {}
            if mid not in usage_by_msg:
                order.append(mid)
            usage_by_msg[mid] = u
            for part in msg.get("content") or []:
                if part.get("type") == "tool_use":
                    n = part.get("name", "?")
                    tools[n] = tools.get(n, 0) + 1
                elif part.get("type") == "text" and part.get("text", "").strip():
                    final_text = part["text"]
                    if "ANSWER" in part["text"] and "TOOL CALLS" in part["text"]:
                        answered_mid = mid
        elif rec.get("type") == "user":
            content = msg.get("content")
            if isinstance(content, list):
                for part in content:
                    if part.get("type") == "tool_result":
                        c = part.get("content")
                        if isinstance(c, str):
                            tool_result_chars += len(c)
                        elif isinstance(c, list):
                            for x in c:
                                if x.get("type") == "text":
                                    tool_result_chars += len(x.get("text", ""))
        if rec.get("type") in ("attachment", "system") or rec.get("isMeta"):
            hook_ctx_chars += len(json.dumps(rec.get("attachment") or rec.get("content") or ""))
    inp = sum((u.get("input_tokens") or 0) for u in usage_by_msg.values())
    cc = sum((u.get("cache_creation_input_tokens") or 0) for u in usage_by_msg.values())
    cr = sum((u.get("cache_read_input_tokens") or 0) for u in usage_by_msg.values())
    out = sum((u.get("output_tokens") or 0) for u in usage_by_msg.values())
    ctxs = [
        (u.get("input_tokens") or 0)
        + (u.get("cache_creation_input_tokens") or 0)
        + (u.get("cache_read_input_tokens") or 0)
        for u in (usage_by_msg[i] for i in order)
    ]
    arm, task, rep = name.split("-")[:3]
    rows.append(
        {
            "name": name,
            "arm": arm,
            "task": task,
            "rep": rep,
            "model": model,
            "api_turns": len(order),
            "tool_calls": sum(tools.values()),
            "tools": tools,
            "tool_result_chars": tool_result_chars,
            "first_ctx": ctxs[0] if ctxs else 0,
            "peak_ctx": max(ctxs) if ctxs else 0,
            "growth_ctx": (max(ctxs) - ctxs[0]) if ctxs else 0,
            "sum_input_all": inp + cc + cr,
            "fresh_input": inp + cc,
            "output": out,
            "total_all": inp + cc + cr + out,
            "duration_s": round((last - first).total_seconds(), 1) if first and last else None,
            "post_answer_turns": post_answer_turns,
            "answered": answered_mid is not None,
            "final": final_text,
        }
    )

out_json.write_text(json.dumps(rows, indent=1), encoding="utf-8")
hdr = "name          turns calls toolchars first_ctx peak_ctx growth  total_all fresh_in  out  dur_s  tools"
print(hdr)
for r in rows:
    print(
        f"{r['name']:<13} {r['api_turns']:>5} {r['tool_calls']:>5} {r['tool_result_chars']:>9} "
        f"{r['first_ctx']:>9} {r['peak_ctx']:>8} {r['growth_ctx']:>6} {r['total_all']:>10} "
        f"{r['fresh_input']:>8} {r['output']:>5} {r['duration_s']!s:>6}  "
        + ",".join(f"{k.replace('mcp__symforge__', 'sf:')}x{v}" for k, v in sorted(r["tools"].items()))
    )
if show_answers:
    for r in rows:
        print("\n=====", r["name"], r["model"])
        print(r["final"][:1100])
