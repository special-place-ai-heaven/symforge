"""Run one benchmark cell: (repo, task, arm, hook/rule mode, repetition).

Each cell gets a fresh git clone of the pinned commit, its own client home
(so user-level hooks and settings never leak in from the operator), and a
generated MCP config. The client is whatever `config.client.command` names;
the dry run points it at fake_client.py.
"""
from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import time
from pathlib import Path
from typing import Any

import oracles
import transcript

HERE = Path(__file__).resolve().parent
GIT_ID = ["-c", "user.name=bench", "-c", "user.email=bench@invalid", "-c", "commit.gpgsign=false"]
ANSWER_SUFFIX = (
    "\n\nThe current directory is the repository. When you are done, end your final "
    "message with a section that starts with the line ANSWER: followed by your answer."
)
# Copied from research/full-surface-benchmark/claude_task_runner.py (SAFE_ENV_NAMES):
# pass only OS and path essentials to the client; credentials only by explicit name.
SAFE_ENV_NAMES = {
    "APPDATA", "COMSPEC", "LANG", "LC_ALL", "LOCALAPPDATA", "PATH", "PATHEXT",
    "PROGRAMDATA", "SystemRoot", "TEMP", "TERM", "TMP", "WINDIR", "GOPATH",
    "GOMODCACHE", "CARGO_HOME", "RUSTUP_HOME", "CARGO_TARGET_DIR", "VIRTUAL_ENV",
}


class SetupError(RuntimeError):
    pass


def sha256_bytes(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def run(argv: list[str], cwd: Path | None = None, env: dict | None = None, timeout: int = 1800) -> None:
    r = subprocess.run(argv, cwd=cwd, env=env, capture_output=True, timeout=timeout, check=False)
    if r.returncode != 0:
        raise SetupError(f"{argv[0]} {argv[1] if len(argv) > 1 else ''} failed: {r.stderr.decode(errors='replace')[-400:]}")


def subst(value: str, values: dict[str, str]) -> str:
    for k, v in values.items():
        value = value.replace("{" + k + "}", v)
    return value


def prepare_pristine(repo: dict, work: Path, cfg: dict, no_fetch: bool = False) -> tuple[Path, str]:
    dst = work / "pristine" / repo["id"]
    if repo.get("fixture"):
        if not dst.exists():
            shutil.copytree(HERE / repo["fixture"], dst)
            env = {**os.environ, "GIT_AUTHOR_DATE": "2026-01-01T00:00:00Z", "GIT_COMMITTER_DATE": "2026-01-01T00:00:00Z"}
            run(["git", "-c", "init.defaultBranch=main", "init", "-q", str(dst)])
            run(["git", "-C", str(dst), "config", "core.autocrlf", "false"])
            run(["git", "-C", str(dst), "add", "-A"])
            run(["git", "-C", str(dst), *GIT_ID, "commit", "-q", "-m", "fixture"], env=env)
        sha = subprocess.run(["git", "-C", str(dst), "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
        return dst, sha
    if not dst.exists():
        if no_fetch:
            raise SetupError(f"{repo['id']} is not cloned and --no-fetch was given")
        run(["git", "-c", "core.autocrlf=false", "clone", "-q", "--no-checkout", repo["url"], str(dst)])
        run(["git", "-C", str(dst), "-c", "core.autocrlf=false", "checkout", "-q", repo["commit"]])
        for argv in repo.get("prepare", []):
            run([subst(a, {"python": cfg["python"]}) for a in argv], cwd=dst)
    return dst, repo["commit"]


def apply_seed(wt: Path, seed: dict) -> dict:
    f = wt / seed["path"]
    text = f.read_bytes().decode("utf-8", errors="surrogateescape")
    if text.count(seed["find"]) != 1:
        raise SetupError(f"seed {seed['id']}: expected exactly one match in {seed['path']}, found {text.count(seed['find'])}")
    pos = text.index(seed["find"])
    f.write_bytes(text.replace(seed["find"], seed["replace"]).encode("utf-8", errors="surrogateescape"))
    return {"path": seed["path"], "line": text.count("\n", 0, pos) + 1, "benign": bool(seed.get("benign"))}


def setup_worktree(pristine: Path, commit: str, wt: Path, task: dict, mode: dict) -> dict[str, Any]:
    """Clone, strip repo-provided agent instructions, add the rules fixture, seed, commit as base."""
    run(["git", "-c", "core.autocrlf=false", "clone", "-q", "--shared", "--no-checkout", str(pristine), str(wt)])
    run(["git", "-C", str(wt), "config", "core.autocrlf", "false"])
    run(["git", "-C", str(wt), "checkout", "-q", commit])
    removed = [p for p in subprocess.run(["git", "-C", str(wt), "ls-files"], capture_output=True, text=True).stdout.splitlines()
               if p.rsplit("/", 1)[-1] == "CLAUDE.md" or p.startswith(".claude/")]
    if removed:
        run(["git", "-C", str(wt), "rm", "-q", "--", *removed])
    rule_files = []
    if mode["rules"]:
        dst = wt / ".claude" / "rules"
        dst.mkdir(parents=True, exist_ok=True)
        for src in sorted((HERE / "overlays" / "rules").glob("*.md")):
            data = src.read_bytes()
            (dst / src.name).write_bytes(data)
            rule_files.append({"path": f".claude/rules/{src.name}", "bytes": len(data), "sha256": sha256_bytes(data)})
        run(["git", "-C", str(wt), "add", "-f", ".claude"])
    seeds = {}
    for s in task.get("seeds", []):
        if s["stage"] == "base":
            seeds[s["id"]] = apply_seed(wt, s)
    run(["git", "-C", str(wt), "add", "-A"])
    run(["git", "-C", str(wt), *GIT_ID, "commit", "-q", "--allow-empty", "-m", "bench base"])
    base = subprocess.run(["git", "-C", str(wt), "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
    for s in task.get("seeds", []):
        if s["stage"] == "review":
            seeds[s["id"]] = apply_seed(wt, s)
    return {"base": base, "seeds": seeds, "removed_instruction_files": removed, "rule_files": rule_files}


def client_env(cfg: dict, home: Path) -> dict[str, str]:
    allowed = {n.upper() for n in SAFE_ENV_NAMES}
    env = {k: v for k, v in os.environ.items() if k.upper() in allowed}
    for name in cfg["client"].get("pass_env", []):  # names only; values never recorded
        if name in os.environ:
            env[name] = os.environ[name]
    env.update({"HOME": str(home), "USERPROFILE": str(home), "CLAUDE_CONFIG_DIR": str(home / ".claude"), "NO_COLOR": "1"})
    return env


def mcp_config(cfg: dict, arm: dict) -> dict:
    if not arm["mcp"]:
        return {"mcpServers": {}}
    sf = cfg["symforge"]
    return {"mcpServers": {"symforge": {"type": "stdio", "command": sf["command"], "args": [], "env": dict(sf["server_env"])}}}


def build_command(cfg: dict, arm: dict, values: dict[str, str]) -> list[str]:
    out = []
    for item in cfg["client"]["command"]:
        if item == "{*steering_args}":
            if arm["steering"]:
                out.extend(subst(a, values) for a in cfg["client"]["steering_args"])
        else:
            out.append(subst(item, values))
    return out


def installed_hooks(home: Path) -> dict:
    f = home / ".claude" / "settings.json"
    if not f.is_file():
        return {"events": [], "sha256": None}
    data = f.read_bytes()
    try:
        events = sorted((json.loads(data).get("hooks") or {}).keys())
    except json.JSONDecodeError:
        events = ["<unparseable settings.json>"]
    return {"events": events, "sha256": sha256_bytes(data)}


def kill_tree(p: subprocess.Popen) -> None:
    if os.name == "nt":
        subprocess.run(["taskkill", "/PID", str(p.pid), "/T", "/F"], capture_output=True, check=False)
    else:
        p.kill()


def run_cell(cfg: dict, repo: dict, task: dict, arm_id: str, mode_id: str, rep: int, work: Path, run_id: str,
             no_fetch: bool = False) -> dict[str, Any]:
    arm, mode = cfg["arms"][arm_id], cfg["modes"][mode_id]
    cell_id = f"{repo['id']}/{task['id']}/{arm_id}/{mode_id}/r{rep}"
    cell_dir = work / "cells" / run_id / cell_id.replace("/", "__")
    if cell_dir.exists():
        shutil.rmtree(cell_dir)  # cell scratch only: a rerun of the same cell replaces its own scratch
    cell_dir.mkdir(parents=True)
    wt, home = cell_dir / "wt", cell_dir / "home"
    (home / ".claude").mkdir(parents=True)
    record: dict[str, Any] = {
        "run_id": run_id, "cell_id": cell_id, "repo": repo["id"], "task": task["id"], "category": task["category"],
        "arm": arm_id, "mode": mode_id, "rep": rep, "synthetic": bool(cfg.get("synthetic")), "model": cfg["model"],
        "config": {"hooks": mode["hooks"], "rules": mode["rules"], "mcp": arm["mcp"], "steering": arm["steering"]},
    }
    try:
        pristine, commit = prepare_pristine(repo, work, cfg, no_fetch)
        setup = setup_worktree(pristine, commit, wt, task, mode)
        env = client_env(cfg, home)
        values = {
            "python": cfg["python"], "here": str(HERE), "symforge": cfg["symforge"]["command"], "model": cfg["model"],
            "effort": cfg.get("effort", "medium"), "task_id": task["id"], "arm": arm_id, "mode": mode_id, "rep": str(rep),
            "native_tools": ",".join(cfg["tools"]["native"]),
            "allowed_tools": ",".join(transcript.allowed_patterns(arm, cfg["tools"])),
            "disallowed_tools": ",".join(cfg["tools"]["disallowed"]),
            "max_budget_usd": cfg["client"]["max_budget_usd"],
            "steering_text": (HERE / "steering.md").read_text(encoding="utf-8"),
        }
        if mode["hooks"] and arm["mcp"]:
            run([subst(a, values) for a in cfg["symforge"]["hook_install"]], cwd=wt, env=env)
        hooks = installed_hooks(home)
        (cell_dir / "mcp.json").write_text(json.dumps(mcp_config(cfg, arm), indent=1), encoding="utf-8")
        values["mcp_config"] = str(cell_dir / "mcp.json")
        record["config"].update({
            "rule_files": setup["rule_files"], "installed_hook_events": hooks["events"], "hook_settings_sha256": hooks["sha256"],
            "removed_instruction_files": setup["removed_instruction_files"], "base_commit": setup["base"],
            "steering_sha256": sha256_bytes(values["steering_text"].encode()) if arm["steering"] else None,
        })
        argv = build_command(cfg, arm, values)
        prompt = task["prompt"] + ANSWER_SUFFIX
        t0 = time.monotonic()
        with open(cell_dir / "transcript.jsonl", "wb") as out, open(cell_dir / "stderr.txt", "wb") as err:
            p = subprocess.Popen(argv, cwd=wt, env=env, stdin=subprocess.PIPE, stdout=out, stderr=err)
            try:
                p.communicate(prompt.encode("utf-8"), timeout=cfg["client"]["timeout_s"])
                timed_out = False
            except subprocess.TimeoutExpired:
                kill_tree(p)
                p.wait()
                timed_out = True
        record["client"] = {"exit": p.returncode, "timed_out": timed_out, "elapsed_s": round(time.monotonic() - t0, 2)}
        metrics = transcript.parse_file(cell_dir / "transcript.jsonl", cfg["rules_sentinel"])
        answer = metrics.pop("answer")
        record["metrics"] = metrics
        record["answer_tail"] = answer[-1500:]
        patterns = transcript.allowed_patterns(arm, cfg["tools"])
        record["allowlist"] = {"patterns": patterns, "violations": transcript.allowlist_violations(metrics["tools"], patterns)}
        ctx = {"seeds": setup["seeds"], "hidden_root": str(repo.get("hidden_root") or HERE / "hidden" / repo["id"]),
               "python": cfg["python"], "oracle_env": repo.get("oracle_env", {})}
        record["oracle"] = oracles.grade(task, wt, setup["base"], answer, ctx)
    except (SetupError, OSError, subprocess.SubprocessError) as exc:
        record["infra_error"] = f"{type(exc).__name__}: {exc}"
    record["status"] = status_of(record)
    if record["status"] == "valid" and not cfg.get("keep_worktrees"):
        shutil.rmtree(wt, ignore_errors=True)
    return record


def status_of(r: dict) -> str:
    if r.get("infra_error") or r.get("client", {}).get("timed_out"):
        return "infra_error"
    if not r.get("metrics", {}).get("has_result"):
        return "infra_error"
    if r["allowlist"]["violations"]:
        return "invalid_allowlist"
    if r["oracle"]["error"]:
        return "oracle_error"
    return "valid"


def calibrate_task(cfg: dict, repo: dict, task: dict, work: Path, no_fetch: bool = False) -> dict[str, Any]:
    """Prove the oracle can be evaluated and rejects the untouched base tree with an empty answer."""
    wt = work / "calibrate" / task["id"]
    if wt.exists():
        shutil.rmtree(wt)
    try:
        pristine, commit = prepare_pristine(repo, work, cfg, no_fetch)
        setup = setup_worktree(pristine, commit, wt, task, cfg["modes"]["off"])
        ctx = {"seeds": setup["seeds"], "hidden_root": str(repo.get("hidden_root") or HERE / "hidden" / repo["id"]),
               "python": cfg["python"], "oracle_env": repo.get("oracle_env", {})}
        g = oracles.grade(task, wt, setup["base"], "", ctx)
        return {"task": task["id"], "calibrated": not g["error"] and not g["pass"], "grade": g}
    except (SetupError, OSError, subprocess.SubprocessError) as exc:
        return {"task": task["id"], "calibrated": False, "error": str(exc)}
