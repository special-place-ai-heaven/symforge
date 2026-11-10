"""Executable oracles. A task passes only when every check here passes.

Ground truth is derived from the cell's base commit at grading time (never from
the model and never hand-typed where a script can compute it), so a stale
expectation cannot silently pass. A check reports `error` when it cannot be
evaluated (empty derivation, missing toolchain); that is an infrastructure
fault, not a task failure, and the aggregator counts it separately.
"""
from __future__ import annotations

import fnmatch
import os
import re
import shutil
import subprocess
from pathlib import Path
from typing import Any

RUNTIME_PREFIXES = (".symforge/", ".claude/", ".bench/")
CITE_RE = re.compile(r"([A-Za-z0-9_.\-/\\]+\.[A-Za-z0-9]+)(?::|#L| line )(\d+)")


def git(wt: Path, *args: str, input_bytes: bytes | None = None) -> bytes:
    r = subprocess.run(["git", "-C", str(wt), *args], input=input_bytes, capture_output=True, check=False)
    if r.returncode != 0:
        raise RuntimeError(f"git {' '.join(args[:2])} failed: {r.stderr.decode(errors='replace')[:300]}")
    return r.stdout


def _glob_ok(path: str, glob: str | None) -> bool:
    if not glob:
        return True
    target = path if "/" in glob else path.rsplit("/", 1)[-1]
    return fnmatch.fnmatchcase(target, glob)


def base_files(wt: Path, base: str, glob: str | None = None) -> dict[str, str]:
    """path -> text for every blob in the base tree that matches glob (one cat-file process)."""
    entries = []
    for line in git(wt, "ls-tree", "-r", "-z", base).split(b"\0"):
        if not line:
            continue
        meta, path = line.split(b"\t", 1)
        mode, kind, sha = meta.split()
        p = path.decode()
        if kind == b"blob" and not p.startswith(RUNTIME_PREFIXES) and _glob_ok(p, glob):
            entries.append((p, sha.decode()))
    if not entries:
        return {}
    out = git(wt, "cat-file", "--batch", input_bytes=("\n".join(s for _, s in entries) + "\n").encode())
    files, pos = {}, 0
    for p, _ in entries:
        nl = out.index(b"\n", pos)
        size = int(out[pos:nl].split()[2])
        files[p] = out[nl + 1: nl + 1 + size].decode("utf-8", errors="surrogateescape")
        pos = nl + 1 + size + 1
    return files


def _norm(p: str) -> str:
    p = p.replace("\\", "/")
    while p.startswith("./"):
        p = p[2:]
    return p


def _same_path(cited: str, expected: str) -> bool:
    c, e = _norm(cited), _norm(expected)
    return c == e or c.endswith("/" + e) or e.endswith("/" + c)


def cites(answer: str) -> list[tuple[str, int]]:
    return [(m.group(1), int(m.group(2))) for m in CITE_RE.finditer(answer)]


def _cited_near(answer: str, path: str, line: int, tol: int) -> bool:
    return any(_same_path(p, path) and abs(n - line) <= tol for p, n in cites(answer))


def _line_of(text: str, pos: int) -> int:
    return text.count("\n", 0, pos) + 1


def check_locate(c: dict, wt: Path, base: str, answer: str, ctx: dict) -> dict:
    rx = re.compile(c["pattern"], re.M)
    truth = [(p, _line_of(t, m.start())) for p, t in base_files(wt, base, c.get("glob")).items() for m in rx.finditer(t)]
    if not truth:
        return {"error": "pattern matched nothing in the base tree"}
    tol = c.get("tolerance", 1)
    ok = any(_cited_near(answer, p, n, tol) for p, n in truth)
    return {"pass": ok, "expected": [f"{p}:{n}" for p, n in truth]}


def check_files_mentioned(c: dict, wt: Path, base: str, answer: str, ctx: dict) -> dict:
    rx, ex = re.compile(c["pattern"]), re.compile(c["exclude_line"]) if c.get("exclude_line") else None
    truth = sorted(
        p for p, t in base_files(wt, base, c.get("glob")).items()
        if any(rx.search(ln) and not (ex and ex.search(ln)) for ln in t.splitlines())
    )
    if not truth:
        return {"error": "no file contains a matching line in the base tree"}
    norm_answer = answer.replace("\\", "/")
    missing = [p for p in truth if p not in norm_answer]
    return {"pass": not missing, "expected": truth, "missing": missing}


def check_names(c: dict, wt: Path, base: str, answer: str, ctx: dict) -> dict:
    text = base_files(wt, base, c["file"]).get(c["file"])
    if text is None:
        return {"error": f"{c['file']} not in base tree"}
    start = re.search(c["start"], text, re.M)
    if not start:
        return {"error": "start pattern not found"}
    end = re.compile(c["end"], re.M).search(text, start.end())
    block = text[start.start(): end.start() if end else len(text)]
    names = list(dict.fromkeys(re.findall(c["item"], block, re.M)))
    if not names:
        return {"error": "no names derived"}
    missing = [n for n in names if not re.search(rf"\b{re.escape(n)}\b", answer)]
    return {"pass": not missing, "expected": names, "missing": missing}


def check_seed_located(c: dict, wt: Path, base: str, answer: str, ctx: dict) -> dict:
    seed = ctx.get("seeds", {}).get(c["seed"])
    if not seed:
        return {"error": f"seed {c['seed']} was not applied"}
    ok = _cited_near(answer, seed["path"], seed["line"], c.get("tolerance", 2))
    return {"pass": ok, "expected": f"{seed['path']}:{seed['line']}"}


def check_answer_regex(c: dict, wt: Path, base: str, answer: str, ctx: dict) -> dict:
    missing = [alts[0] for alts in c["all"] if not any(re.search(a, answer) for a in alts)]
    return {"pass": not missing, "missing": missing}


def changed_paths(wt: Path, base: str) -> list[str]:
    tracked = git(wt, "diff", "--name-only", "-z", base).decode().split("\0")
    untracked = git(wt, "ls-files", "--others", "--exclude-standard", "-z").decode().split("\0")
    keep = {p for p in tracked if p} | {p for p in untracked if p and not _build_artifact(p)}
    return sorted(p for p in keep if not p.startswith(RUNTIME_PREFIXES))


BUILD_ARTIFACT_PARTS = {"__pycache__", ".pytest_cache", "target", "node_modules", ".mypy_cache"}


def _build_artifact(p: str) -> bool:
    """Untracked tool output (test caches, build dirs) is not part of the agent's change."""
    return p.endswith(".pyc") or any(part in BUILD_ARTIFACT_PARTS for part in p.split("/")[:-1])


def check_diff_scope(c: dict, wt: Path, base: str, answer: str, ctx: dict) -> dict:
    changed = changed_paths(wt, base)
    outside = [p for p in changed if not any(fnmatch.fnmatchcase(p, g) for g in c["allow"])]
    return {"pass": bool(changed) and not outside, "changed": changed, "outside_scope": outside}


def check_exact_rename(c: dict, wt: Path, base: str, answer: str, ctx: dict) -> dict:
    """The tree after the run must equal the base tree with \\bold\\b replaced by new, nothing else."""
    rx = re.compile(rf"\b{re.escape(c['old'])}\b")
    files = base_files(wt, base)
    occurrences = sum(len(rx.findall(t)) for t in files.values())
    if occurrences == 0:
        return {"error": f"{c['old']} does not occur in the base tree"}
    wrong = []
    for p in changed_paths(wt, base):
        want = rx.sub(c["new"], files[p]) if p in files else None
        f = wt / p
        got = f.read_bytes().decode("utf-8", errors="surrogateescape") if f.is_file() else None
        if want is None or got != want:
            wrong.append(p)
    missed = [p for p, t in files.items() if rx.search(t) and p not in changed_paths(wt, base)]
    return {"pass": not wrong and not missed, "occurrences": occurrences, "wrong": wrong, "not_renamed": missed}


def check_command(c: dict, wt: Path, base: str, answer: str, ctx: dict) -> dict:
    hidden_root = ctx.get("hidden_root")
    for rel in c.get("hidden", []):
        src = Path(hidden_root) / rel
        dst = wt / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(src, dst)
    argv = [a.replace("{python}", ctx.get("python", "python")) for a in c["argv"]]
    if shutil.which(argv[0]) is None:
        return {"error": f"toolchain {argv[0]!r} not found"}
    env = {**os.environ, **ctx.get("oracle_env", {}), **c.get("env", {})}
    try:
        r = subprocess.run(argv, cwd=wt, env=env, capture_output=True, timeout=c.get("timeout_s", 900), check=False)
    except subprocess.TimeoutExpired:
        return {"pass": False, "detail": "timeout"}
    tail = (r.stdout + r.stderr).decode(errors="replace")[-1500:]
    return {"pass": r.returncode == 0, "exit": r.returncode, "tail": tail}


CHECKS = {
    "locate": check_locate,
    "files_mentioned": check_files_mentioned,
    "names": check_names,
    "seed_located": check_seed_located,
    "answer_regex": check_answer_regex,
    "diff_scope": check_diff_scope,
    "exact_rename": check_exact_rename,
    "command": check_command,
}


def grade(task: dict, wt: Path, base: str, answer: str, ctx: dict) -> dict[str, Any]:
    # Tree checks run before command checks, which add hidden files to the worktree.
    ordered = sorted(task["oracle"], key=lambda c: c["type"] == "command")
    results = []
    for c in ordered:
        try:
            r = CHECKS[c["type"]](c, wt, base, answer, ctx)
        except Exception as exc:  # an oracle that cannot run is an infrastructure fault
            r = {"error": f"{type(exc).__name__}: {exc}"}
        results.append({"type": c["type"], **r})
    errored = any("error" in r for r in results)
    return {"pass": not errored and all(r.get("pass") for r in results), "error": errored, "checks": results}
