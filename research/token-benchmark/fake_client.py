"""Synthetic stand-in for the model client, used only by `bench.py dry-run`.

It speaks the same stream-json event shapes the parser reads, performs the
scripted edits for fixture tasks in the current directory, and makes its token
counts up from fixtures/fake_script.json. Every number it produces is
synthetic. It never contacts a network or a model.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import random
import re
import subprocess
import sys
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "fixtures" / "fake_script.json"
SENTINEL = "BENCH-RULE-SENTINEL-7F3A"


def emit(ev: dict) -> None:
    sys.stdout.write(json.dumps(ev) + "\n")


def apply_edits(task: dict) -> None:
    for e in task.get("edits", []):
        f = Path(e["path"])
        text = f.read_text(encoding="utf-8")
        f.write_text(text.replace(e["find"], e["replace"]) if e.get("find") else text + e["append"], encoding="utf-8",
                     newline="\n")
    if "rename" in task:
        rx = re.compile(rf"\b{re.escape(task['rename']['old'])}\b")
        for p in subprocess.run(["git", "ls-files"], capture_output=True, text=True).stdout.splitlines():
            f = Path(p)
            text = f.read_text(encoding="utf-8")
            if rx.search(text):
                f.write_text(rx.sub(task["rename"]["new"], text), encoding="utf-8", newline="\n")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--version", action="store_true")
    ap.add_argument("--install-hooks", action="store_true")
    ap.add_argument("--task")
    ap.add_argument("--arm")
    ap.add_argument("--mode")
    ap.add_argument("--rep", type=int)
    ap.add_argument("--mcp-config")
    ap.add_argument("--steered", action="store_true")
    a = ap.parse_args()
    if a.version:
        print("fake-client 0.0 (synthetic)")
        return 0
    if a.install_hooks:
        d = Path(os.environ["CLAUDE_CONFIG_DIR"])
        d.mkdir(parents=True, exist_ok=True)
        hooks = {"hooks": {"PostToolUse": [{"matcher": "*", "hooks": [{"type": "command", "command": "fake-hook"}]}],
                           "SessionStart": [{"hooks": [{"type": "command", "command": "fake-hook"}]}]}}
        (d / "settings.json").write_text(json.dumps(hooks), encoding="utf-8")
        return 0
    sys.stdin.read()
    script = json.loads(SCRIPT.read_text(encoding="utf-8"))
    task = script["tasks"][a.task]
    seed = int(hashlib.sha256(f"{a.task}/{a.arm}/{a.mode}/{a.rep}".encode()).hexdigest()[:12], 16)
    rng = random.Random(seed)
    mcp = bool(json.loads(Path(a.mcp_config).read_text())["mcpServers"])
    fails = rng.random() < script["fail_rate"][a.arm]
    tools = dict(task["tools"]["sf" if mcp else "native"])
    if a.arm == "c" and rng.random() < 0.5:  # unsteered: sometimes ignores the index and greps
        tools = dict(task["tools"]["native"])
    if [a.task, a.arm, a.mode, a.rep] in script["allowlist_violation_cells"]:
        tools["mcp__symforge__search_symbols"] = 1
    factor = task["factor"].get(a.arm if mcp else "a", 1.0)
    total = int(task["base_tokens"] * factor * rng.uniform(0.85, 1.15) * (1.08 if a.mode == "on" and "Read" in tools else 1.0))
    names = [n for n, k in tools.items() for _ in range(k)]
    turns = len(names) + 1
    emit({"type": "system", "subtype": "init", "model": "synthetic-fake-client", "claude_code_version": "fake-0.0",
          "tools": sorted(set(names))})
    if a.mode == "on" and mcp:
        emit({"type": "system", "subtype": "hook_response", "hook_event": "SessionStart"})
    for i, name in enumerate(names):
        emit({"type": "assistant", "message": {"id": f"m{i}", "model": "synthetic-fake-client",
              "content": [{"type": "tool_use", "id": f"t{i}", "name": name, "input": {}}],
              "usage": {"input_tokens": 40, "cache_creation_input_tokens": int(total * 0.12 / turns),
                        "cache_read_input_tokens": int(total * 0.85 / turns), "output_tokens": int(total * 0.03 / turns)}}})
        payload = "x" * rng.randint(200, 3000)
        if a.mode == "on" and name in ("Read", "Edit") and i == names.index(name):
            payload += f"\n<system-reminder>rule file injected: {SENTINEL}</system-reminder>"
        emit({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": f"t{i}", "content": payload}]}})
    if not fails:
        apply_edits(task)
    answer = "ANSWER:\nI could not determine this." if fails else task["answer"]
    emit({"type": "assistant", "message": {"id": f"m{turns}", "model": "synthetic-fake-client",
          "content": [{"type": "text", "text": answer}],
          "usage": {"input_tokens": 40, "cache_creation_input_tokens": int(total * 0.12 / turns),
                    "cache_read_input_tokens": int(total * 0.85 / turns), "output_tokens": int(total * 0.03 / turns)}}})
    emit({"type": "result", "subtype": "success", "duration_ms": rng.randint(20_000, 240_000), "num_turns": turns,
          "total_cost_usd": None, "result": answer})
    return 0


if __name__ == "__main__":
    sys.exit(main())
