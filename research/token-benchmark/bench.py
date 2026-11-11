"""Three-arm token/quality benchmark for SymForge (spec 035).

Commands:
  verify-pins                      check every pinned commit with git ls-remote
  calibrate  --work DIR            prove each task's oracle evaluates and rejects the base tree
  matrix     --work DIR --out DIR  run cells (filters: --repo --task --arm --mode --reps)
  report     --out DIR             aggregate records in DIR/records into DIR/report.{md,json}
  dry-run    --out DIR             whole pipeline on the bundled fixture with the fake client
Live runs refuse tasks that are not calibrated unless --allow-uncalibrated is given.
"""
from __future__ import annotations

import argparse
import json
import random
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import aggregate  # noqa: E402
import runner  # noqa: E402


def merge(a: dict, b: dict) -> dict:
    out = dict(a)
    for k, v in b.items():
        out[k] = merge(out[k], v) if isinstance(v, dict) and isinstance(out.get(k), dict) else v
    return out


def load(args) -> tuple[dict, list[dict], list[dict]]:
    cfg = json.loads((HERE / "config.json").read_text(encoding="utf-8"))
    if getattr(args, "config_override", None):
        cfg = merge(cfg, json.loads(Path(args.config_override).read_text(encoding="utf-8")))
    if getattr(args, "python", None):
        cfg["python"] = args.python
    repos_file = Path(getattr(args, "repos", None) or HERE / "repos.json")
    repos = json.loads(repos_file.read_text(encoding="utf-8"))["repositories"]
    for r in repos:
        if r.get("fixture"):
            r["fixture"] = str((repos_file.parent / r["fixture"]).resolve())
        if r.get("hidden_root"):
            r["hidden_root"] = str((repos_file.parent / r["hidden_root"]).resolve())
    tasks_dir = Path(getattr(args, "tasks", None) or HERE / "tasks")
    tasks = [t for f in sorted(tasks_dir.glob("*.json")) for t in json.loads(f.read_text(encoding="utf-8"))]
    return cfg, repos, tasks


def select(tasks: list[dict], args) -> list[dict]:
    return [t for t in tasks if (not args.repo or t["repo"] in args.repo) and (not args.task or t["id"] in args.task)]


def cmd_verify_pins(args) -> int:
    _, repos, _ = load(args)
    missing = 0
    for r in repos:
        if r.get("fixture"):
            continue
        out = subprocess.run(["git", "ls-remote", r["url"]], capture_output=True, text=True, timeout=120).stdout
        shas = {line.split("\t")[0] for line in out.splitlines()}
        head = next((line.split("\t")[0] for line in out.splitlines() if line.endswith("\tHEAD")), "?")
        status = "HEAD" if head == r["commit"] else ("on a ref" if r["commit"] in shas else "not advertised (still fetchable if reachable)")
        missing += head == "?"
        print(f"{r['id']:<8} {r['commit']}  {status}; remote HEAD {head}")
    return 1 if missing else 0


def cmd_calibrate(args) -> int:
    cfg, repos, tasks = load(args)
    by_id = {r["id"]: r for r in repos}
    work = Path(args.work)
    results = [runner.calibrate_task(cfg, by_id[t["repo"]], t, work, args.no_fetch) for t in select(tasks, args)]
    out = work / "calibration.json"
    out.write_text(json.dumps(results, indent=1, default=str), encoding="utf-8")
    for r in results:
        print(f"{r['task']:<36} {'ok' if r['calibrated'] else 'FAILED'}")
    print(f"calibrated {sum(r['calibrated'] for r in results)}/{len(results)}; written {out}")
    return 0 if all(r["calibrated"] for r in results) else 1


def cmd_matrix(args) -> int:
    cfg, repos, tasks = load(args)
    by_id = {r["id"]: r for r in repos}
    work, out = Path(args.work), Path(args.out)
    rec_dir = out / "records"
    rec_dir.mkdir(parents=True, exist_ok=True)
    chosen = select(tasks, args)
    if not cfg.get("synthetic") and not args.allow_uncalibrated:
        cal_file = work / "calibration.json"
        cal = {c["task"]: c["calibrated"] for c in json.loads(cal_file.read_text())} if cal_file.exists() else {}
        missing = [t["id"] for t in chosen if not cal.get(t["id"])]
        if missing:
            print(f"refusing live run: {len(missing)} task(s) not calibrated: {missing[:5]}... run `calibrate` first")
            return 2
    reps = args.reps or cfg["repetitions"]
    arms = args.arm or list(cfg["arms"])
    modes = args.mode or list(cfg["modes"])
    cells = [(t, a, m, k) for t in chosen for a in arms for m in modes for k in range(1, reps + 1)]
    seed = args.seed if args.seed is not None else int(time.time())
    random.Random(seed).shuffle(cells)  # interleave arms so no arm always pays the first cache write
    run_id = args.run_id or time.strftime("bench-%Y%m%dT%H%M%SZ", time.gmtime())
    (out / f"plan-{run_id}.json").write_text(json.dumps({"run_id": run_id, "seed": seed, "cells": len(cells)}), encoding="utf-8")
    for i, (t, a, m, k) in enumerate(cells, 1):
        name = f"{t['repo']}__{t['id']}__{a}__{m}__r{k}.json"
        if (rec_dir / name).exists():
            continue  # resume: a recorded cell is never rerun silently
        rec = runner.run_cell(cfg, by_id[t["repo"]], t, a, m, k, work, run_id, args.no_fetch)
        (rec_dir / name).write_text(json.dumps(rec, indent=1, default=str), encoding="utf-8")
        print(f"[{i}/{len(cells)}] {rec['cell_id']} {rec['status']} pass={rec.get('oracle', {}).get('pass')}", flush=True)
    return 0


def cmd_report(args) -> int:
    cfg, _, tasks = load(args)
    out = Path(args.out)
    records = aggregate.load_records(out / "records")
    # Expected cells come from the suite, not from the records, so a shard that never
    # uploaded lowers the valid fraction instead of silently shrinking the denominator.
    n_tasks = len(select(tasks, args))
    reps = args.reps or cfg["repetitions"]
    rep = aggregate.write_report(records, cfg, n_tasks * 2 * reps, out)
    print(f"{len(records)} records -> {out / 'report.md'}; rule outcome {rep['rule_outcome']}; claim {rep['claim']}")
    return 0


def cmd_dry_run(args) -> int:
    fx = HERE / "fixtures"
    args.config_override = str(fx / "config.dryrun.json")
    args.repos, args.tasks = str(fx / "repos.json"), str(fx / "tasks")
    args.work = args.work or str(Path(args.out) / "work")
    args.repo = args.task = args.arm = args.mode = None
    args.allow_uncalibrated, args.no_fetch, args.run_id = True, True, args.run_id or "dryrun-synthetic"
    args.seed = 7 if args.seed is None else args.seed
    rc = cmd_matrix(args)
    return rc or cmd_report(args)


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    for name in ("verify-pins", "calibrate", "matrix", "report", "dry-run"):
        s = sub.add_parser(name)
        s.add_argument("--config-override")
        s.add_argument("--repos")
        s.add_argument("--tasks")
        s.add_argument("--python")
        s.add_argument("--work")
        s.add_argument("--out")
        s.add_argument("--repo", action="append")
        s.add_argument("--task", action="append")
        s.add_argument("--arm", action="append")
        s.add_argument("--mode", action="append")
        s.add_argument("--reps", type=int)
        s.add_argument("--seed", type=int)
        s.add_argument("--run-id")
        s.add_argument("--no-fetch", action="store_true")
        s.add_argument("--allow-uncalibrated", action="store_true")
    args = p.parse_args(argv)
    return {"verify-pins": cmd_verify_pins, "calibrate": cmd_calibrate, "matrix": cmd_matrix,
            "report": cmd_report, "dry-run": cmd_dry_run}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
