"""Price-weighted and confound-adjusted views of the paired benchmark.

Usage: python -I cost_model.py <subagents_transcript_dir>

Reads the same transcripts as parse.py and cuts each run at its ANSWER message.
Prints, per model cohort and arm:
  - raw tokens processed, split into fresh input, cached input and output
  - a price-weighted cost under ASSUMED ratios (cache read 0.1, new input 1.25, output 5.0,
    relative to 1.0 for plain input) with the shared prefix treated as already cached for
    every run, which removes the launch-order bias in who paid the first cache write
  - the same cost with client-injected rule files removed from arm A (sensitivity)
  - non-payload context per run: context growth - tool_result_chars/4 - output tokens
The ratios are assumptions for comparison only, not a bill.
"""
import json
import re
import statistics
import sys
from pathlib import Path

READ_W, NEW_W, OUT_W = 0.1, 1.25, 5.0
NAME_RE = re.compile(r"agent-a((?:[AB])-T\d+-r\d+(?:-[a-z]+)?)-[0-9a-f]+\.jsonl$")


def strings(o):
    if isinstance(o, str):
        yield o
    elif isinstance(o, dict):
        for v in o.values():
            yield from strings(v)
    elif isinstance(o, list):
        for v in o:
            yield from strings(v)


runs = []
for f in sorted(Path(sys.argv[1]).glob("agent-a*.jsonl")):
    m = NAME_RE.search(f.name)
    if not m:
        continue
    name = m.group(1)
    order, usage, model = [], {}, None
    first_read_turn = None
    answered = None
    rule_chars = 0
    tool_chars = 0
    retrieval_calls = 0
    for line in f.read_text(encoding="utf-8", errors="replace").splitlines():
        try:
            rec = json.loads(line)
        except Exception:
            continue
        msg = rec.get("message") or {}
        if answered is not None and not (rec.get("type") == "assistant" and msg.get("id") == answered):
            continue
        if rec.get("type") == "attachment" and (rec.get("attachment") or {}).get("type") == "nested_memory":
            ss = list(strings(rec["attachment"]))
            rule_chars += max((len(s) for s in ss), default=0)
        if rec.get("type") == "assistant":
            mid = msg.get("id")
            model = msg.get("model") or model
            if mid not in usage:
                order.append(mid)
            usage[mid] = msg.get("usage") or {}
            for part in msg.get("content") or []:
                if part.get("type") == "tool_use":
                    if part.get("name") != "SendMessage":
                        retrieval_calls += 1
                    if part.get("name") == "Read" and first_read_turn is None:
                        first_read_turn = len(order) - 1
                elif part.get("type") == "text" and "ANSWER" in part.get("text", "") and "TOOL CALLS" in part.get("text", ""):
                    answered = mid
        elif rec.get("type") == "user" and isinstance(msg.get("content"), list):
            for part in msg["content"]:
                if part.get("type") == "tool_result":
                    c = part.get("content")
                    if isinstance(c, str):
                        tool_chars += len(c)
                    elif isinstance(c, list):
                        tool_chars += sum(len(x.get("text", "")) for x in c if x.get("type") == "text")
    us = [usage[i] for i in order]
    ctx = [(u.get("input_tokens") or 0) + (u.get("cache_creation_input_tokens") or 0) + (u.get("cache_read_input_tokens") or 0) for u in us]
    out = [(u.get("output_tokens") or 0) for u in us]
    fresh = sum((u.get("input_tokens") or 0) + (u.get("cache_creation_input_tokens") or 0) for u in us)
    cached = sum((u.get("cache_read_input_tokens") or 0) for u in us)

    def weighted(series):
        cost = READ_W * series[0]
        for i in range(1, len(series)):
            cost += READ_W * series[i - 1] + NEW_W * max(series[i] - series[i - 1], 0)
        return cost + OUT_W * sum(out)

    rule_tokens = rule_chars / 4
    adj = list(ctx)
    if first_read_turn is not None and rule_tokens:
        adj = [c - rule_tokens if i > first_read_turn else c for i, c in enumerate(ctx)]
    arm, task = name.split("-")[:2]
    runs.append({
        "name": name, "arm": arm, "task": task, "model": "haiku" if "haiku" in (model or "") else "sonnet",
        "turns": len(ctx), "retrieval_calls": retrieval_calls, "fresh": fresh, "cached": cached, "out": sum(out),
        "weighted": weighted(ctx), "weighted_no_rules": weighted(adj),
        "rule_tokens": rule_tokens, "used_read": first_read_turn is not None,
        "non_payload": (max(ctx) - ctx[0]) - tool_chars / 4 - sum(out), "tool_chars": tool_chars,
    })


def pct(a, b):
    return f"{(b - a) / a * 100:+.0f}%"


for model in ("sonnet", "haiku"):
    A = [r for r in runs if r["model"] == model and r["arm"] == "A"]
    B = [r for r in runs if r["model"] == model and r["arm"] == "B"]
    s = lambda xs, k: sum(x[k] for x in xs)
    print(f"== {model}: {len(A)} runs per arm (A native, B SymForge) ==")
    for key, label in (("turns", "round trips"), ("retrieval_calls", "retrieval tool calls"),
                       ("tool_chars", "retrieved chars"), ("fresh", "fresh input tokens (raw, launch-order biased)"),
                       ("cached", "cached input tokens"), ("out", "output tokens"),
                       ("weighted", "price-weighted cost, warm prefix"),
                       ("weighted_no_rules", "price-weighted cost, warm prefix, rule files removed from A")):
        print(f"  {label:<62} A {s(A, key):>12.0f}  B {s(B, key):>12.0f}  {pct(s(A, key), s(B, key))}")
    wins = sum(
        1 for t in sorted({r["task"] for r in A})
        if s([r for r in B if r["task"] == t], "weighted_no_rules") < s([r for r in A if r["task"] == t], "weighted_no_rules")
    )
    print(f"  tasks where B is cheaper on the rules-removed weighted cost: {wins}/6")
    for t in sorted({r["task"] for r in A}):
        a = s([r for r in A if r["task"] == t], "weighted_no_rules")
        b = s([r for r in B if r["task"] == t], "weighted_no_rules")
        print(f"    {t}: A {a:>9.0f}  B {b:>9.0f}  {pct(a, b)}")

print("== non-payload context per run (growth - tool chars/4 - output), all 36 runs ==")
for label, pred in (("A, used Read", lambda r: r["arm"] == "A" and r["used_read"]),
                    ("A, Grep only", lambda r: r["arm"] == "A" and not r["used_read"]),
                    ("B", lambda r: r["arm"] == "B")):
    xs = [r["non_payload"] for r in runs if pred(r)]
    print(f"  {label:<14} n={len(xs):>2} median {statistics.median(xs):>7.0f} min {min(xs):>7.0f} max {max(xs):>7.0f}")
rt = [r["rule_tokens"] for r in runs if r["rule_tokens"]]
print(f"rule-file attachments: {len(rt)} runs, median {statistics.median(rt):.0f} tokens (chars/4), arms: "
      f"A {sum(1 for r in runs if r['rule_tokens'] and r['arm']=='A')}, B {sum(1 for r in runs if r['rule_tokens'] and r['arm']=='B')}")
allA = sum(r["tool_chars"] for r in runs if r["arm"] == "A")
allB = sum(r["tool_chars"] for r in runs if r["arm"] == "B")
print(f"retrieved chars, all 36 runs: A {allA} B {allB} {pct(allA, allB)}")
