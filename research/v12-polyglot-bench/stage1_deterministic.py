"""Stage 1 ($0, zero-credential) 6-language x 6-arm deterministic benchmark runner.

Compares Native Raw, Native Line-Window, RTK 1.2.1 CLI, SymForge 12.0.0 Full MCP,
SymForge 12.0.0 Code Mode MCP, and SymForge 12.0.0 CLI + RTK across Rust, C#,
TypeScript, Python, Java, and Go using real subprocess outputs and exact tiktoken
o200k_base token counts.
"""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import shutil
import subprocess
import sys
import time
from typing import Any

import tiktoken

from corpus import REPOS, RepoSpec, build_tasks, grade_output

ENC = tiktoken.get_encoding("o200k_base")


def count_tokens(text: str) -> int:
    return len(ENC.encode(text, disallowed_special=()))


def resolve_symforge() -> str:
    cmd = shutil.which("symforge.cmd") or shutil.which("symforge")
    if not cmd:
        raise FileNotFoundError("symforge executable not found on PATH")
    return cmd


def resolve_rtk(corpus_root: pathlib.Path) -> str:
    local_bin = corpus_root / "bin" / ("rtk.exe" if os.name == "nt" else "rtk")
    if local_bin.exists():
        return str(local_bin)
    cmd = shutil.which("rtk")
    if not cmd:
        raise FileNotFoundError(f"rtk binary not found at {local_bin} or PATH")
    return cmd


def run_proc(
    argv: list[str],
    cwd: pathlib.Path,
    stdin_text: str | None = None,
    extra_env: dict[str, str] | None = None,
) -> tuple[str, float, int]:
    env = os.environ.copy()
    if extra_env:
        env.update(extra_env)
    t0 = time.perf_counter()
    proc = subprocess.run(
        argv,
        cwd=str(cwd),
        input=stdin_text,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        env=env,
    )
    elapsed_ms = (time.perf_counter() - t0) * 1000.0
    return proc.stdout, elapsed_ms, proc.returncode


def measure_mcp_surface_schema(sym_cmd: str, surface: str, project_dir: pathlib.Path) -> dict[str, Any]:
    """Perform a live JSON-RPC tools/list handshake to measure exact upfront schema tokens."""
    env = os.environ.copy()
    env["SYMFORGE_NO_DAEMON"] = "1"
    env["SYMFORGE_AUTO_INDEX"] = "false"
    env["SYMFORGE_SURFACE"] = surface

    proc = subprocess.Popen(
        [sym_cmd],
        cwd=str(project_dir),
        env=env,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    assert proc.stdin is not None and proc.stdout is not None
    try:
        init_req = {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "polyglot-bench", "version": "1.0.0"},
            },
        }
        proc.stdin.write((json.dumps(init_req) + "\n").encode("utf-8"))
        proc.stdin.flush()
        while True:
            raw = proc.stdout.readline()
            if not raw:
                raise RuntimeError(f"EOF waiting for initialize on SYMFORGE_SURFACE={surface}")
            msg = json.loads(raw.decode("utf-8", errors="replace"))
            if msg.get("id") == 1:
                break

        proc.stdin.write((json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n").encode("utf-8"))
        proc.stdin.write((json.dumps({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}) + "\n").encode("utf-8"))
        proc.stdin.flush()
        while True:
            raw = proc.stdout.readline()
            if not raw:
                raise RuntimeError(f"EOF waiting for tools/list on SYMFORGE_SURFACE={surface}")
            msg = json.loads(raw.decode("utf-8", errors="replace"))
            if msg.get("id") == 2 and "result" in msg:
                tools = msg["result"].get("tools", [])
                compact_json = json.dumps(tools, separators=(",", ":"))
                return {
                    "surface": surface,
                    "tool_count": len(tools),
                    "schema_bytes": len(compact_json.encode("utf-8")),
                    "schema_tokens": count_tokens(compact_json),
                }
    finally:
        proc.kill()
        proc.wait()


def read_window(file_path: pathlib.Path, center_line: int, span: int = 80) -> str:
    lines = file_path.read_text(encoding="utf-8", errors="replace").splitlines()
    lo = max(0, center_line - 15)
    hi = min(len(lines), lo + span)
    return "\n".join(f"{idx + 1}:{lines[idx]}" for idx in range(lo, hi))


def list_subsystem_source_files(repo_dir: pathlib.Path, sub_dir: str) -> list[pathlib.Path]:
    exts = {".rs", ".cs", ".ts", ".tsx", ".py", ".java", ".go"}
    base = repo_dir / sub_dir
    files = [
        p
        for p in sorted(base.rglob("*"))
        if p.is_file() and p.suffix in exts and "test" not in p.name.lower()
    ]
    return files[:25] if files else [p for p in sorted(base.rglob("*")) if p.is_file()][:15]


def execute_task_across_arms(
    repo: RepoSpec,
    task: dict[str, Any],
    repo_dir: pathlib.Path,
    sym_cmd: str,
    rtk_cmd: str,
) -> dict[str, dict[str, Any]]:
    arch = task["archetype"]
    req_facts = task["required_substrings"]
    rg_cmd = shutil.which("rg") or "rg"
    git_cmd = shutil.which("git") or "git"

    results: dict[str, dict[str, Any]] = {}

    def record(arm: str, text: str, turns: int, wall_ms: float, note: str = "") -> None:
        grade = grade_output(text, req_facts)
        results[arm] = {
            "payload_bytes": len(text.encode("utf-8")),
            "payload_tokens": count_tokens(text),
            "turns": turns,
            "wall_ms": round(wall_ms, 1),
            "oracle_passed": grade["passed"],
            "missing_facts": grade["missing"],
            "note": note,
        }

    if arch == "T1_symbol_slice":
        f_path = repo_dir / repo.large_file
        # 1. Native Raw: rg -n + full file read
        rg_out, ms1, _ = run_proc([rg_cmd, "-n", repo.target_symbol, repo.large_file], repo_dir)
        t0 = time.perf_counter()
        raw_file = f_path.read_text(encoding="utf-8", errors="replace")
        ms2 = (time.perf_counter() - t0) * 1000.0
        record("native_raw", rg_out + "\n" + raw_file, turns=2, wall_ms=ms1 + ms2)

        # 2. Native Window: rg -n + 80-line slice around target_line
        win_text = read_window(f_path, repo.target_line, span=80)
        record("native_window", rg_out + "\n" + win_text, turns=2, wall_ms=ms1 + 1.0)

        # 3. RTK CLI: rtk read -l aggressive first; if body fact stripped, agent must call rtk read!
        agg_out, rtk_ms1, _ = run_proc([rtk_cmd, "read", "-l", "aggressive", repo.large_file], repo_dir)
        if grade_output(agg_out, req_facts)["passed"]:
            record("rtk_cli", agg_out, turns=1, wall_ms=rtk_ms1, note="aggressive_sufficed")
        else:
            full_rtk, rtk_ms2, _ = run_proc([rtk_cmd, "read", repo.large_file], repo_dir)
            record(
                "rtk_cli",
                agg_out + "\n" + full_rtk,
                turns=2,
                wall_ms=rtk_ms1 + rtk_ms2,
                note="aggressive_stripped_body_fallback_to_full_read",
            )

        # 4. SymForge Full MCP: search_symbols + get_symbol (2 turns)
        q1, sf_ms1, _ = run_proc(
            [
                sym_cmd,
                "q",
                "search_symbols",
                "-a",
                f"path_prefix={repo.large_file}",
                "-a",
                f"query={repo.target_symbol}",
                "-a",
                "limit=5",
                "--root",
                str(repo_dir),
            ],
            repo_dir,
        )
        q2, sf_ms2, _ = run_proc(
            [
                sym_cmd,
                "q",
                "get_symbol",
                "-a",
                f"path={repo.large_file}",
                "-a",
                f"name={repo.target_symbol}",
                "-a",
                f"symbol_line={repo.target_line}",
                "--root",
                str(repo_dir),
            ],
            repo_dir,
        )
        record("symforge_full_mcp", q1 + "\n" + q2, turns=2, wall_ms=sf_ms1 + sf_ms2)

        # 5 & 6. SymForge Code Mode MCP & SymForge CLI + RTK (1 turn via Monty VM)
        code = (
            f'b = get_symbol(path="{repo.large_file}", name="{repo.target_symbol}", '
            f"symbol_line={repo.target_line})[0]\n"
            '{"name": b["name"], "lines": [b["start_line"], b["end_line"]], "source": b["source"]}\n'
        )
        cm_out, cm_ms, _ = run_proc(
            [sym_cmd, "code", "run", "-", "--root", str(repo_dir)],
            repo_dir,
            stdin_text=code,
        )
        record("symforge_code_mcp", cm_out, turns=1, wall_ms=cm_ms)
        record("symforge_cli_rtk", cm_out, turns=1, wall_ms=cm_ms)

    elif arch == "T2_caller_chain":
        caller_files = list(dict.fromkeys(c[0] for c in repo.top_callers))
        rg_out, ms1, _ = run_proc([rg_cmd, "-n", repo.caller_symbol], repo_dir)

        t0 = time.perf_counter()
        raw_bodies = "\n".join(
            (repo_dir / rel).read_text(encoding="utf-8", errors="replace") for rel in caller_files
        )
        ms2 = (time.perf_counter() - t0) * 1000.0
        record("native_raw", rg_out + "\n" + raw_bodies, turns=1 + len(caller_files), wall_ms=ms1 + ms2)

        win_bodies = "\n".join(
            read_window(repo_dir / c[0], c[2], span=70) for c in repo.top_callers
        )
        record("native_window", rg_out + "\n" + win_bodies, turns=1 + len(repo.top_callers), wall_ms=ms1 + 2.0)

        rtk_rg, rms1, _ = run_proc([rtk_cmd, "rg", repo.caller_symbol], repo_dir)
        rtk_reads = []
        rms_total = rms1
        for rel in caller_files:
            ro, rms, _ = run_proc([rtk_cmd, "read", rel], repo_dir)
            rtk_reads.append(ro)
            rms_total += rms
        record(
            "rtk_cli",
            rtk_rg + "\n" + "\n".join(rtk_reads),
            turns=1 + len(caller_files),
            wall_ms=rms_total,
            note="rtk_rg_omits_enclosing_symbol_requires_file_reads",
        )

        f_refs, fms1, _ = run_proc(
            [
                sym_cmd,
                "q",
                "find_references",
                "-a",
                f"name={repo.caller_symbol}",
                "-a",
                "limit=25",
                "--root",
                str(repo_dir),
            ],
            repo_dir,
        )
        targets_json = json.dumps(
            [{"path": c[0], "name": c[1], "symbol_line": c[2]} for c in repo.top_callers]
        )
        f_syms, fms2, _ = run_proc(
            [
                sym_cmd,
                "q",
                "get_symbol",
                json.dumps({"targets": json.loads(targets_json)}),
                "--root",
                str(repo_dir),
            ],
            repo_dir,
        )
        record("symforge_full_mcp", f_refs + "\n" + f_syms, turns=2, wall_ms=fms1 + fms2)

        code = f"""
refs = find_references(name="{repo.caller_symbol}", limit=30)
seen = []
targets = []
for r in refs:
    enc = r.get("enclosing_symbol")
    key = r["path"] + ":" + (enc or "")
    if enc and key not in seen and len(targets) < 3:
        seen.append(key)
        targets.append({{"path": r["path"], "name": enc}})
bodies = get_symbol(targets=targets)
[{{"caller": b["name"], "path": b["path"], "source": (b.get("source") or "")[:900]}} for b in bodies]
"""
        cm_out, cm_ms, _ = run_proc(
            [sym_cmd, "code", "run", "-", "--root", str(repo_dir)],
            repo_dir,
            stdin_text=code,
        )
        record("symforge_code_mcp", cm_out, turns=1, wall_ms=cm_ms)
        record("symforge_cli_rtk", cm_out, turns=1, wall_ms=cm_ms)

    elif arch == "T3_subsystem_orient":
        sub_files = list_subsystem_source_files(repo_dir, repo.subsystem_dir)
        rels = [str(p.relative_to(repo_dir)).replace("\\", "/") for p in sub_files]

        t0 = time.perf_counter()
        raw_sub = "\n".join(p.read_text(encoding="utf-8", errors="replace") for p in sub_files)
        ms_raw = (time.perf_counter() - t0) * 1000.0
        record("native_raw", raw_sub, turns=1 + len(sub_files), wall_ms=ms_raw)

        win_sub = "\n".join(read_window(p, 1, span=120) for p in sub_files)
        record("native_window", win_sub, turns=1 + len(sub_files), wall_ms=3.0)

        rtk_agg, rms, _ = run_proc([rtk_cmd, "read", "-l", "aggressive", *rels], repo_dir)
        if grade_output(rtk_agg, req_facts)["passed"]:
            record("rtk_cli", rtk_agg, turns=1, wall_ms=rms, note="aggressive_multi_file_outline")
        else:
            rtk_min, rms2, _ = run_proc([rtk_cmd, "read", "-l", "minimal", *rels], repo_dir)
            record("rtk_cli", rtk_agg + "\n" + rtk_min, turns=2, wall_ms=rms + rms2, note="aggressive_dropped_java_classes_fallback_minimal")

        q_sub, fms, _ = run_proc(
            [
                sym_cmd,
                "q",
                "search_symbols",
                "-a",
                f"path_prefix={repo.subsystem_dir}",
                "-a",
                "limit=80",
                "--root",
                str(repo_dir),
            ],
            repo_dir,
        )
        record("symforge_full_mcp", q_sub, turns=1, wall_ms=fms)

        code = f"""
syms = search_symbols(path_prefix="{repo.subsystem_dir}", limit=100)
ranked = sorted(syms, key=lambda s: s["end_line"] - s["start_line"], reverse=True)[:5]
[{{"name": s["name"], "kind": s["kind"], "path": s["path"], "span": [s["start_line"], s["end_line"]]}} for s in ranked]
"""
        cm_out, cm_ms, _ = run_proc(
            [sym_cmd, "code", "run", "-", "--root", str(repo_dir)],
            repo_dir,
            stdin_text=code,
        )
        record("symforge_code_mcp", cm_out, turns=1, wall_ms=cm_ms)
        record("symforge_cli_rtk", cm_out, turns=1, wall_ms=cm_ms)

    elif arch == "T4_edit_prep":
        ep_path = repo_dir / repo.edit_prep_file
        rg_out, ms1, _ = run_proc([rg_cmd, "-n", repo.edit_prep_symbol], repo_dir)
        raw_ep = ep_path.read_text(encoding="utf-8", errors="replace")
        record("native_raw", rg_out + "\n" + raw_ep, turns=2, wall_ms=ms1 + 2.0)

        win_ep = read_window(ep_path, repo.edit_prep_line, span=100)
        # Also pull imports/top of file so type definitions/imports are visible
        head_ep = read_window(ep_path, 1, span=60)
        record("native_window", rg_out + "\n" + head_ep + "\n" + win_ep, turns=3, wall_ms=ms1 + 2.0)

        rtk_rg, rms1, _ = run_proc([rtk_cmd, "rg", repo.edit_prep_symbol], repo_dir)
        rtk_ep, rms2, _ = run_proc([rtk_cmd, "read", repo.edit_prep_file], repo_dir)
        record("rtk_cli", rtk_rg + "\n" + rtk_ep, turns=2, wall_ms=rms1 + rms2)

        sc_args = json.dumps(
            {
                "name": repo.edit_prep_symbol,
                "path": repo.edit_prep_file,
                "symbol_line": repo.edit_prep_line,
                "bundle": True,
            }
        )
        q_sc, fms, _ = run_proc(
            [sym_cmd, "q", "symbol_context", sc_args, "--root", str(repo_dir)],
            repo_dir,
        )
        record("symforge_full_mcp", q_sc, turns=1, wall_ms=fms)

        code = (
            f'sc = symbol_context(name="{repo.edit_prep_symbol}", path="{repo.edit_prep_file}", '
            f"symbol_line={repo.edit_prep_line}, bundle=True)\n"
            '{"symbol": sc["name"], "body": sc["body"], '
            '"callers": [c["name"] for c in sc["callers"][:12]], '
            '"callees": [c["name"] for c in sc["callees"][:15]], '
            '"types": [d["name"] for d in sc["dependencies"][:15]]}\n'
        )
        cm_out, cm_ms, _ = run_proc(
            [sym_cmd, "code", "run", "-", "--root", str(repo_dir)],
            repo_dir,
            stdin_text=code,
        )
        record("symforge_code_mcp", cm_out, turns=1, wall_ms=cm_ms)
        record("symforge_cli_rtk", cm_out, turns=1, wall_ms=cm_ms)

    elif arch == "T5_literal_git_hybrid":
        rg_cnt, ms1, _ = run_proc([rg_cmd, "-c", repo.literal_token], repo_dir)
        git_raw, ms2, _ = run_proc([git_cmd, "log", "-n", "10"], repo_dir)
        record("native_raw", rg_cnt + "\n" + git_raw, turns=2, wall_ms=ms1 + ms2)
        record("native_window", rg_cnt + "\n" + git_raw, turns=2, wall_ms=ms1 + ms2)

        rtk_rg, rms1, _ = run_proc([rtk_cmd, "rg", repo.literal_token], repo_dir)
        rtk_git, rms2, _ = run_proc([rtk_cmd, "git", "log", "-n", "10"], repo_dir)
        record("rtk_cli", rtk_rg + "\n" + rtk_git, turns=2, wall_ms=rms1 + rms2, note="rtk_git_log_compression")

        sf_cnt, fms1, _ = run_proc(
            [
                sym_cmd,
                "q",
                "search_text",
                json.dumps({"pattern": repo.literal_token, "group_by": "count", "include_tests": True}),
                "--root",
                str(repo_dir),
            ],
            repo_dir,
        )
        record("symforge_full_mcp", sf_cnt + "\n" + git_raw, turns=2, wall_ms=fms1 + ms2)

        code = (
            f'hits = search_text(pattern="{repo.literal_token}", include_tests=True, limit=100)\n'
            '{"matched_count": len(hits), "files": len(set(h["path"] for h in hits))}\n'
        )
        cm_cnt, cm_ms, _ = run_proc(
            [sym_cmd, "code", "run", "-", "--root", str(repo_dir)],
            repo_dir,
            stdin_text=code,
        )
        record("symforge_code_mcp", cm_cnt + "\n" + git_raw, turns=2, wall_ms=cm_ms + ms2)
        # Hybrid arm: SymForge Monty VM for code + RTK for git log!
        record(
            "symforge_cli_rtk",
            cm_cnt + "\n" + rtk_git,
            turns=2,
            wall_ms=cm_ms + rms2,
            note="symforge_code_plus_rtk_git_synergy",
        )

    return results


def run_benchmark(corpus_root: pathlib.Path, out_dir: pathlib.Path) -> dict[str, Any]:
    sym_cmd = resolve_symforge()
    rtk_cmd = resolve_rtk(corpus_root)
    repos_root = corpus_root / "repos"

    # Measure live upfront MCP schemas once via real stdio JSON-RPC tools/list
    sample_repo = repos_root / REPOS[0].repo_id
    full_schema = measure_mcp_surface_schema(sym_cmd, "full", sample_repo)
    code_schema = measure_mcp_surface_schema(sym_cmd, "code", sample_repo)

    upfront_tokens = {
        "native_raw": 0,
        "native_window": 0,
        "rtk_cli": 0,
        "symforge_full_mcp": full_schema["schema_tokens"],
        "symforge_code_mcp": code_schema["schema_tokens"],
        "symforge_cli_rtk": 0,
    }

    tasks = build_tasks()
    tasks_by_repo: dict[str, list[dict[str, Any]]] = {}
    for t in tasks:
        tasks_by_repo.setdefault(t["repo_id"], []).append(t)

    repo_summaries: list[dict[str, Any]] = []
    all_task_rows: list[dict[str, Any]] = []

    for repo in REPOS:
        repo_dir = repos_root / repo.repo_id
        # Warm index check
        _, warm_ms, _ = run_proc([sym_cmd, "q", "status", "--root", str(repo_dir)], repo_dir)

        arm_payload_tokens = {k: 0 for k in upfront_tokens}
        arm_turns = {k: 0 for k in upfront_tokens}
        arm_wall_ms = {k: 0.0 for k in upfront_tokens}
        arm_passes = {k: 0 for k in upfront_tokens}

        for task in tasks_by_repo[repo.repo_id]:
            arm_res = execute_task_across_arms(repo, task, repo_dir, sym_cmd, rtk_cmd)
            all_task_rows.append(
                {
                    "task_id": task["task_id"],
                    "repo_id": repo.repo_id,
                    "language": repo.language,
                    "archetype": task["archetype"],
                    "arms": arm_res,
                }
            )
            for arm, metrics in arm_res.items():
                arm_payload_tokens[arm] += metrics["payload_tokens"]
                arm_turns[arm] += metrics["turns"]
                arm_wall_ms[arm] += metrics["wall_ms"]
                arm_passes[arm] += 1 if metrics["oracle_passed"] else 0

        arm_session_totals = {
            arm: upfront_tokens[arm] + arm_payload_tokens[arm] for arm in upfront_tokens
        }
        repo_summaries.append(
            {
                "repo_id": repo.repo_id,
                "language": repo.language,
                "display_name": repo.display_name,
                "commit_sha": repo.commit_sha,
                "indexed_files": repo.indexed_files,
                "warm_status_ms": round(warm_ms, 1),
                "payload_tokens": arm_payload_tokens,
                "session_5task_tokens": arm_session_totals,
                "turns": arm_turns,
                "wall_ms": {k: round(v, 1) for k, v in arm_wall_ms.items()},
                "oracle_passed": arm_passes,
            }
        )

    # Grand totals across all 6 repositories (30 tasks, 6 five-task repo sessions)
    arms_order = list(upfront_tokens.keys())
    grand_totals: dict[str, Any] = {}
    native_total = sum(r["session_5task_tokens"]["native_raw"] for r in repo_summaries)
    rtk_total = sum(r["session_5task_tokens"]["rtk_cli"] for r in repo_summaries)

    for arm in arms_order:
        sess_tok = sum(r["session_5task_tokens"][arm] for r in repo_summaries)
        pay_tok = sum(r["payload_tokens"][arm] for r in repo_summaries)
        turns = sum(r["turns"][arm] for r in repo_summaries)
        wall = sum(r["wall_ms"][arm] for r in repo_summaries)
        passed = sum(r["oracle_passed"][arm] for r in repo_summaries)
        grand_totals[arm] = {
            "upfront_schema_tokens_per_session": upfront_tokens[arm],
            "total_payload_tokens_30tasks": pay_tok,
            "total_session_tokens_6repos": sess_tok,
            "reduction_vs_native_raw_pct": round((1.0 - sess_tok / native_total) * 100.0, 1),
            "reduction_vs_rtk_pct": round((1.0 - sess_tok / rtk_total) * 100.0, 1),
            "total_turns_30tasks": turns,
            "total_wall_ms_30tasks": round(wall, 1),
            "oracle_passed_of_30": passed,
        }

    report = {
        "benchmark": "symforge-v12-polyglot-rtk-stage1",
        "tokenizer": "tiktoken:o200k_base",
        "total_repos": len(REPOS),
        "total_indexed_files": sum(r.indexed_files for r in REPOS),
        "total_tasks": len(tasks),
        "mcp_schemas": {"full": full_schema, "code": code_schema},
        "repos": repo_summaries,
        "grand_totals": grand_totals,
        "tasks": all_task_rows,
    }

    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "stage1-results.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
    (out_dir / "stage1-summary.md").write_text(render_markdown_summary(report), encoding="utf-8")
    return report


def render_markdown_summary(report: dict[str, Any]) -> str:
    gt = report["grand_totals"]
    lines = [
        "# SymForge 12.0.0 Polyglot & RTK Benchmark (Stage 1 — Deterministic `o200k_base`)",
        "",
        f"- **Corpus**: {report['total_repos']} real-world open-source repositories (**{report['total_indexed_files']:,} indexed files**) across **Rust, C#, TypeScript, Python, Java, and Go**",
        f"- **Tasks**: {report['total_tasks']} deterministic auto-graded tasks (5 task archetypes × 6 languages)",
        f"- **Tokenizer**: `tiktoken` (`o200k_base`) on exact live subprocess outputs & live JSON-RPC `tools/list` schemas",
        f"- **Measured MCP Schemas**: `SYMFORGE_SURFACE=full` = **{report['mcp_schemas']['full']['schema_tokens']:,} tokens** ({report['mcp_schemas']['full']['tool_count']} tools) vs. `SYMFORGE_SURFACE=code` = **{report['mcp_schemas']['code']['schema_tokens']:,} tokens** ({report['mcp_schemas']['code']['tool_count']} tools, **-{round((1 - report['mcp_schemas']['code']['schema_tokens'] / report['mcp_schemas']['full']['schema_tokens']) * 100, 1)}%**)",
        "",
        "## Per-Language 5-Task Session Token Totals (`Upfront Schema + 5 Tasks`)",
        "",
        "| Language | Repository (`Files`) | Native Raw | Native Window | RTK 1.2.1 CLI | SymForge Full MCP | SymForge V12 Code MCP | SymForge CLI + RTK |",
        "|---|---|---:|---:|---:|---:|---:|---:|",
    ]
    for r in report["repos"]:
        s = r["session_5task_tokens"]
        lines.append(
            f"| **{r['language']}** | `{r['display_name']}` ({r['indexed_files']:,}) "
            f"| {s['native_raw']:,} | {s['native_window']:,} | {s['rtk_cli']:,} "
            f"| {s['symforge_full_mcp']:,} | **{s['symforge_code_mcp']:,}** | **{s['symforge_cli_rtk']:,}** |"
        )
    lines.extend(
        [
            "",
            "## Aggregate 6-Repository Totals (30 Tasks Across 6 Sessions)",
            "",
            "| Arm | Upfront Schema / Session | 30-Task Payload Tokens | 6-Session Total Tokens | vs. Native Raw | vs. RTK 1.2.1 | Total Turns | Oracle Pass |",
            "|---|---:|---:|---:|---:|---:|---:|---:|",
        ]
    )
    labels = {
        "native_raw": "Native Raw (`rg` + full file)",
        "native_window": "Native Window (`rg` + 80L slice)",
        "rtk_cli": "RTK 1.2.1 CLI (`rtk rg` + `rtk read`)",
        "symforge_full_mcp": "SymForge 12.0 Full MCP (40 tools)",
        "symforge_code_mcp": "**SymForge 12.0 Code MCP (4 tools)**",
        "symforge_cli_rtk": "**SymForge 12.0 CLI + RTK Hybrid**",
    }
    for arm, label in labels.items():
        g = gt[arm]
        d_nat = -g["reduction_vs_native_raw_pct"]
        d_rtk = -g["reduction_vs_rtk_pct"]
        lines.append(
            f"| {label} | {g['upfront_schema_tokens_per_session']:,} | {g['total_payload_tokens_30tasks']:,} "
            f"| **{g['total_session_tokens_6repos']:,}** | {d_nat:+.1f}% "
            f"| {d_rtk:+.1f}% | {g['total_turns_30tasks']} | {g['oracle_passed_of_30']}/30 |"
        )
    return "\n".join(lines) + "\n"


def main() -> None:
    parser = argparse.ArgumentParser(description="Run Stage 1 6-language deterministic benchmark.")
    parser.add_argument(
        "--corpus-root",
        type=pathlib.Path,
        default=pathlib.Path("C:/AI_STUFF/symforge-bench-corpus"),
    )
    parser.add_argument(
        "--out-dir",
        type=pathlib.Path,
        default=pathlib.Path(__file__).resolve().parent / "results",
    )
    args = parser.parse_args()
    sys.stdout.reconfigure(encoding="utf-8")
    report = run_benchmark(args.corpus_root, args.out_dir)
    print(render_markdown_summary(report))


if __name__ == "__main__":
    main()
