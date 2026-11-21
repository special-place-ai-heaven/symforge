"""Stage 2 Turnkey Smol-LLM Agent Runner (OpenRouter / OpenAI-compatible + --dry-run).

Evaluates real multi-turn tool-calling agent behavior across all 6 languages and 5 arms:
  - native_window
  - rtk_cli
  - symforge_full_mcp
  - symforge_code_mcp
  - symforge_cli_rtk

Safety & credential boundary:
  - Default mode is `--dry-run` (100% offline deterministic mock agent; reads NO env vars
    and makes NO network requests).
  - Live execution (`--live`) is strictly opt-in for human operators running from their
    own shell with `--api-key-env OPENROUTER_API_KEY` (never hardcoded).
"""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.request
from typing import Any

from corpus import REPOS, RepoSpec, build_tasks, grade_output

ARMS = (
    "native_window",
    "rtk_cli",
    "symforge_full_mcp",
    "symforge_code_mcp",
    "symforge_cli_rtk",
)


def arm_tool_schemas(arm: str) -> list[dict[str, Any]]:
    """Minimal OpenAI function-calling tool schemas exposed to the smol-LLM per arm."""
    if arm == "native_window":
        return [
            {
                "type": "function",
                "function": {
                    "name": "rg_search",
                    "description": "Search code with ripgrep (-n).",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "pattern": {"type": "string"},
                            "path": {"type": "string"},
                        },
                        "required": ["pattern"],
                    },
                },
            },
            {
                "type": "function",
                "function": {
                    "name": "read_file_window",
                    "description": "Read a bounded line slice from a repository file.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "path": {"type": "string"},
                            "start_line": {"type": "integer"},
                            "line_count": {"type": "integer"},
                        },
                        "required": ["path"],
                    },
                },
            },
            {
                "type": "function",
                "function": {
                    "name": "git_log",
                    "description": "Run git log -n 5 --stat.",
                    "parameters": {"type": "object", "properties": {}},
                },
            },
        ]
    if arm == "rtk_cli":
        return [
            {
                "type": "function",
                "function": {
                    "name": "rtk_exec",
                    "description": "Run an rtk subcommand (read, rg, git, ls).",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "args": {"type": "array", "items": {"type": "string"}},
                        },
                        "required": ["args"],
                    },
                },
            }
        ]
    if arm == "symforge_full_mcp":
        return [
            {
                "type": "function",
                "function": {
                    "name": "symforge_query",
                    "description": "Invoke a SymForge MCP read/search tool by name and JSON args.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "tool": {"type": "string"},
                            "args": {"type": "object"},
                        },
                        "required": ["tool"],
                    },
                },
            },
            {
                "type": "function",
                "function": {
                    "name": "git_log",
                    "description": "Run git log -n 5 --stat.",
                    "parameters": {"type": "object", "properties": {}},
                },
            },
        ]
    if arm == "symforge_code_mcp":
        return [
            {
                "type": "function",
                "function": {
                    "name": "code_search",
                    "description": "Search SymForge Code Mode Python SDK typed stubs.",
                    "parameters": {
                        "type": "object",
                        "properties": {"query": {"type": "string"}},
                    },
                },
            },
            {
                "type": "function",
                "function": {
                    "name": "code_execute",
                    "description": "Execute a multi-step Python script in SymForge pure-Rust Monty VM.",
                    "parameters": {
                        "type": "object",
                        "properties": {"code": {"type": "string"}},
                        "required": ["code"],
                    },
                },
            },
            {
                "type": "function",
                "function": {
                    "name": "git_log",
                    "description": "Run git log -n 5 --stat.",
                    "parameters": {"type": "object", "properties": {}},
                },
            },
        ]
    if arm == "symforge_cli_rtk":
        return [
            {
                "type": "function",
                "function": {
                    "name": "symforge_code_run",
                    "description": "Run a Python Code Mode script via `symforge code run -`.",
                    "parameters": {
                        "type": "object",
                        "properties": {"code": {"type": "string"}},
                        "required": ["code"],
                    },
                },
            },
            {
                "type": "function",
                "function": {
                    "name": "rtk_exec",
                    "description": "Run an rtk subcommand for compact git/test/build output.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "args": {"type": "array", "items": {"type": "string"}},
                        },
                        "required": ["args"],
                    },
                },
            },
        ]
    raise ValueError(f"Unknown arm: {arm}")


def dispatch_tool(
    fn_name: str,
    fn_args: dict[str, Any],
    worktree_dir: pathlib.Path,
    sym_cmd: str,
    rtk_cmd: str,
) -> str:
    rg_cmd = shutil.which("rg") or "rg"
    git_cmd = shutil.which("git") or "git"

    if fn_name == "rg_search":
        cmd = [rg_cmd, "-n", str(fn_args["pattern"])]
        if fn_args.get("path"):
            cmd.append(str(fn_args["path"]))
        p = subprocess.run(cmd, cwd=worktree_dir, capture_output=True, text=True, encoding="utf-8", errors="replace")
        return p.stdout[:16000]

    if fn_name == "read_file_window":
        rel = str(fn_args["path"]).lstrip("/\\")
        target = (worktree_dir / rel).resolve()
        if not str(target).startswith(str(worktree_dir.resolve())):
            return "error: path escapes worktree"
        lines = target.read_text(encoding="utf-8", errors="replace").splitlines()
        lo = max(0, int(fn_args.get("start_line", 1)) - 1)
        cnt = min(200, max(1, int(fn_args.get("line_count", 80))))
        return "\n".join(f"{i + 1}:{lines[i]}" for i in range(lo, min(len(lines), lo + cnt)))

    if fn_name == "git_log":
        p = subprocess.run(
            [git_cmd, "log", "-n", "10"],
            cwd=worktree_dir,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        return p.stdout[:8000]

    if fn_name == "rtk_exec":
        sub_args = [str(x) for x in fn_args.get("args", [])]
        p = subprocess.run(
            [rtk_cmd, *sub_args],
            cwd=worktree_dir,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        return p.stdout[:24000]

    if fn_name == "symforge_query":
        tool = str(fn_args["tool"])
        payload = json.dumps(fn_args.get("args") or {})
        p = subprocess.run(
            [sym_cmd, "q", tool, payload, "--root", str(worktree_dir)],
            cwd=worktree_dir,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        return p.stdout[:24000]

    if fn_name == "code_search":
        q = str(fn_args.get("query") or "")
        cmd = [sym_cmd, "code", "stubs"] + ([q] if q else [])
        p = subprocess.run(cmd, cwd=worktree_dir, capture_output=True, text=True, encoding="utf-8", errors="replace")
        return p.stdout[:12000]

    if fn_name in ("code_execute", "symforge_code_run"):
        script = str(fn_args["code"])
        p = subprocess.run(
            [sym_cmd, "code", "run", "-", "--root", str(worktree_dir)],
            cwd=worktree_dir,
            input=script,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        return p.stdout[:24000]

    return f"error: unknown tool {fn_name}"


def call_chat_api(
    base_url: str,
    api_token: str,
    model: str,
    messages: list[dict[str, Any]],
    tools: list[dict[str, Any]],
) -> dict[str, Any]:
    body = json.dumps(
        {
            "model": model,
            "temperature": 0.0,
            "messages": messages,
            "tools": tools,
        }
    ).encode("utf-8")
    req = urllib.request.Request(
        base_url,
        data=body,
        headers={
            "Content-Type": "application/json",
            "Authorization": " ".join(["Bearer", api_token]),
        },
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=90) as resp:
        return json.loads(resp.read().decode("utf-8"))


def dry_run_mock_step(
    arm: str,
    repo: RepoSpec,
    task: dict[str, Any],
    turn_idx: int,
    last_tool_output: str,
) -> dict[str, Any]:
    """Offline deterministic scripted agent step for --dry-run verification (zero credentials)."""
    if turn_idx == 0:
        if arm in ("symforge_code_mcp", "symforge_cli_rtk"):
            fn = "code_execute" if arm == "symforge_code_mcp" else "symforge_code_run"
            if task["archetype"] == "T5_literal_git_hybrid" and arm == "symforge_cli_rtk":
                return {"tool": "rtk_exec", "args": {"args": ["git", "log", "-n", "5"]}}
            code = (
                f'b = get_symbol(path="{repo.large_file}", name="{repo.target_symbol}", '
                f"symbol_line={repo.target_line})[0]\n"
                '{"name": b["name"], "source": b["source"]}\n'
            )
            return {"tool": fn, "args": {"code": code}}
        if arm == "symforge_full_mcp":
            return {
                "tool": "symforge_query",
                "args": {
                    "tool": "get_symbol",
                    "args": {
                        "path": repo.large_file,
                        "name": repo.target_symbol,
                        "symbol_line": repo.target_line,
                    },
                },
            }
        if arm == "rtk_cli":
            return {"tool": "rtk_exec", "args": {"args": ["read", repo.large_file]}}
        return {
            "tool": "read_file_window",
            "args": {"path": repo.large_file, "start_line": repo.target_line, "line_count": 60},
        }
    return {
        "final": (
            f"Completed {task['task_id']} ({arm}): "
            + " | ".join(task["required_substrings"])
            + f" | tool_bytes={len(last_tool_output)}"
        )
    }


def run_cell_in_worktree(
    repo: RepoSpec,
    base_repo_dir: pathlib.Path,
    task: dict[str, Any],
    arm: str,
    sym_cmd: str,
    rtk_cmd: str,
    live: bool,
    base_url: str,
    api_token: str,
    model: str,
    max_turns: int,
) -> dict[str, Any]:
    git_cmd = shutil.which("git") or "git"
    tmp_wt = pathlib.Path(tempfile.mkdtemp(prefix=f"sf-wt-{repo.repo_id}-"))
    subprocess.run(
        [git_cmd, "worktree", "add", "--detach", str(tmp_wt), "HEAD"],
        cwd=base_repo_dir,
        capture_output=True,
        check=True,
    )
    try:
        tools = arm_tool_schemas(arm)
        messages: list[dict[str, Any]] = [
            {
                "role": "system",
                "content": (
                    "You are a concise software engineering benchmark agent. Use the provided "
                    "tools to answer accurately with exact symbol names and code facts."
                ),
            },
            {"role": "user", "content": task["prompt"]},
        ]
        prompt_tokens = 0
        completion_tokens = 0
        cached_tokens = 0
        turns = 0
        last_tool_out = ""
        final_answer = ""
        t0 = time.perf_counter()

        for turn_idx in range(max_turns):
            turns += 1
            if not live:
                step = dry_run_mock_step(arm, repo, task, turn_idx, last_tool_out)
                if "final" in step:
                    final_answer = step["final"]
                    break
                last_tool_out = dispatch_tool(step["tool"], step["args"], tmp_wt, sym_cmd, rtk_cmd)
                prompt_tokens += 250
                completion_tokens += 40
            else:
                resp = call_chat_api(base_url, api_token, model, messages, tools)
                usage = resp.get("usage") or {}
                prompt_tokens += int(usage.get("prompt_tokens", 0))
                completion_tokens += int(usage.get("completion_tokens", 0))
                details = usage.get("prompt_tokens_details") or {}
                cached_tokens += int(details.get("cached_tokens", 0))

                msg = resp["choices"][0]["message"]
                messages.append(msg)
                tcalls = msg.get("tool_calls") or []
                if not tcalls:
                    final_answer = msg.get("content") or ""
                    break
                for tc in tcalls:
                    fn_name = tc["function"]["name"]
                    raw_args = tc["function"].get("arguments") or "{}"
                    parsed_args = json.loads(raw_args) if isinstance(raw_args, str) else raw_args
                    out = dispatch_tool(fn_name, parsed_args, tmp_wt, sym_cmd, rtk_cmd)
                    last_tool_out = out
                    messages.append(
                        {
                            "role": "tool",
                            "tool_call_id": tc["id"],
                            "content": out,
                        }
                    )

        elapsed_s = round(time.perf_counter() - t0, 2)
        grade = grade_output(final_answer + "\n" + last_tool_out, task["required_substrings"])
        return {
            "task_id": task["task_id"],
            "repo_id": repo.repo_id,
            "language": repo.language,
            "arm": arm,
            "mode": "live" if live else "dry-run",
            "model": model if live else "deterministic-mock",
            "turns": turns,
            "prompt_tokens": prompt_tokens,
            "cached_tokens": cached_tokens,
            "completion_tokens": completion_tokens,
            "wall_s": elapsed_s,
            "oracle_passed": grade["passed"],
            "missing": grade["missing"],
        }
    finally:
        subprocess.run(
            [git_cmd, "worktree", "remove", "--force", str(tmp_wt)],
            cwd=base_repo_dir,
            capture_output=True,
        )
        shutil.rmtree(tmp_wt, ignore_errors=True)


def main() -> None:
    parser = argparse.ArgumentParser(description="Stage 2 smol-LLM polyglot benchmark runner.")
    parser.add_argument("--corpus-root", type=pathlib.Path, default=pathlib.Path("C:/AI_STUFF/symforge-bench-corpus"))
    parser.add_argument("--dry-run", action="store_true", default=False, help="Run offline deterministic mock verification.")
    parser.add_argument("--live", action="store_true", default=False, help="Opt-in live LLM execution (requires env token).")
    parser.add_argument("--model", default="google/gemini-2.5-flash-lite", help="Smol-LLM model identifier.")
    parser.add_argument("--base-url", default="https://openrouter.ai/api/v1/chat/completions")
    parser.add_argument("--api-key-env", default="OPENROUTER_API_KEY", help="Name of env var holding API token.")
    parser.add_argument("--limit-tasks", type=int, default=6, help="Max tasks to execute (0 = all 30).")
    parser.add_argument("--arms", nargs="+", default=list(ARMS))
    parser.add_argument("--max-turns", type=int, default=8)
    parser.add_argument("--out", type=pathlib.Path, default=pathlib.Path(__file__).resolve().parent / "results" / "stage2-results.json")
    args = parser.parse_args()

    if args.live and args.dry_run:
        raise SystemExit("Pass either --dry-run or --live, not both.")
    live_mode = bool(args.live)

    api_token = ""
    if live_mode:
        api_token = os.environ.get(args.api_key_env, "").strip()
        if not api_token:
            raise SystemExit(f"--live requires non-empty environment variable {args.api_key_env}")

    sym_cmd = shutil.which("symforge.cmd") or shutil.which("symforge") or "symforge"
    rtk_bin = args.corpus_root / "bin" / ("rtk.exe" if os.name == "nt" else "rtk")
    rtk_cmd = str(rtk_bin) if rtk_bin.exists() else (shutil.which("rtk") or "rtk")

    repo_map = {r.repo_id: r for r in REPOS}
    tasks = build_tasks()
    if args.limit_tasks > 0:
        # Pick T1 across each language by default when limited
        t1s = [t for t in tasks if t["archetype"] == "T1_symbol_slice"]
        tasks = (t1s + [t for t in tasks if t not in t1s])[: args.limit_tasks]

    rows: list[dict[str, Any]] = []
    for task in tasks:
        repo = repo_map[task["repo_id"]]
        base_dir = args.corpus_root / "repos" / repo.repo_id
        for arm in args.arms:
            row = run_cell_in_worktree(
                repo=repo,
                base_repo_dir=base_dir,
                task=task,
                arm=arm,
                sym_cmd=sym_cmd,
                rtk_cmd=rtk_cmd,
                live=live_mode,
                base_url=args.base_url,
                api_token=api_token,
                model=args.model,
                max_turns=args.max_turns,
            )
            rows.append(row)
            print(
                f"[{row['mode']}] {row['language']:10} | {row['task_id']:32} | {arm:18} "
                f"-> pass={row['oracle_passed']} turns={row['turns']} wall={row['wall_s']}s"
            )

    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps({"mode": "live" if live_mode else "dry-run", "rows": rows}, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
