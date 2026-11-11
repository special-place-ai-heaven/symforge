"""Aggregate cell records into a report and apply the predeclared decision rule.

Rule (spec 035, section "Decision rule"), evaluated per hook/rule mode for the
headline pair (native arm vs. a SymForge arm):
  1. pair = (repo, task); arm value = median total_tokens over that arm's
     oracle-passing valid repetitions; a pair where either arm has no passing
     repetition is excluded from the token math and counted in `excluded_pairs`;
  2. saving_t = (native - other) / native;
  3. ACCEPT iff total saving (1 - sum(other)/sum(native)) > 0 AND
     median(saving_t) > 0 AND the other arm's oracle pass count >= native's AND
     at least `min_valid_fraction` of expected cells are valid.
A claim needs ACCEPT in every mode when `require_both_modes`. Synthetic records
are evaluated the same way but the claim is never admissible.
"""
from __future__ import annotations

import json
import statistics
from collections import defaultdict
from pathlib import Path
from typing import Any

CATEGORIES = ["discovery", "tracing", "unfamiliar-module", "debugging", "single-file-edit",
              "cross-module-edit", "rename-refactor", "review"]


def load_records(d: Path) -> list[dict]:
    return [json.loads(p.read_text(encoding="utf-8")) for p in sorted(d.glob("*.json"))]


def _med(xs: list[float]) -> float | None:
    return statistics.median(xs) if xs else None


def _passing(r: dict) -> bool:
    return r["status"] == "valid" and r["oracle"]["pass"]


def price(r: dict, p: dict) -> float:
    m = r["metrics"]
    return (m["input_tokens"] * p["input"] + m["cache_write_tokens"] * p["cache_write"]
            + m["cache_read_tokens"] * p["cache_read"] + m["output_tokens"] * p["output"]) / 1e6


def compare(records: list[dict], mode: str, native: str, other: str, decision: dict, expected_cells: int) -> dict[str, Any]:
    rs = [r for r in records if r["mode"] == mode and r["arm"] in (native, other)]
    by_pair: dict[tuple, dict[str, list[dict]]] = defaultdict(lambda: defaultdict(list))
    for r in rs:
        by_pair[(r["repo"], r["task"])][r["arm"]].append(r)
    pairs, excluded = [], []
    for (repo, task), arms in sorted(by_pair.items()):
        a = [x["metrics"]["total_tokens"] for x in arms[native] if _passing(x)]
        b = [x["metrics"]["total_tokens"] for x in arms[other] if _passing(x)]
        cat = (arms[native] or arms[other])[0]["category"]
        if not a or not b:
            excluded.append({"repo": repo, "task": task, "category": cat, "native_passes": len(a), "other_passes": len(b)})
            continue
        ma, mb = _med(a), _med(b)
        pairs.append({"repo": repo, "task": task, "category": cat, "native": ma, "other": mb, "saving": (ma - mb) / ma})
    valid = [r for r in rs if r["status"] == "valid"]
    pass_native = sum(1 for r in valid if r["arm"] == native and r["oracle"]["pass"])
    pass_other = sum(1 for r in valid if r["arm"] == other and r["oracle"]["pass"])
    total_saving = (1 - sum(p["other"] for p in pairs) / sum(p["native"] for p in pairs)) if pairs else None
    median_saving = _med([p["saving"] for p in pairs])
    valid_fraction = len(valid) / expected_cells if expected_cells else 0.0
    gates = {
        "total_saving_positive": total_saving is not None and total_saving > 0,
        "median_saving_positive": median_saving is not None and median_saving > 0,
        "quality_not_worse": (pass_other >= pass_native) or not decision["require_quality_not_worse"],
        "enough_valid_cells": valid_fraction >= decision["min_valid_fraction"],
    }
    cats = {}
    for c in CATEGORIES:
        ps = [p for p in pairs if p["category"] == c]
        cats[c] = {"pairs": len(ps), "won": sum(1 for p in ps if p["saving"] > 0),
                   "median_saving": _med([p["saving"] for p in ps])}
    losing = [c for c, v in cats.items() if v["median_saving"] is not None and v["median_saving"] <= 0]
    return {
        "mode": mode, "native": native, "other": other, "pairs": pairs, "excluded_pairs": excluded,
        "pairs_won": sum(1 for p in pairs if p["saving"] > 0), "pairs_counted": len(pairs),
        "total_saving": total_saving, "median_saving": median_saving,
        "pass_counts": {native: pass_native, other: pass_other}, "valid_fraction": valid_fraction,
        "gates": gates, "verdict": "ACCEPT" if all(gates.values()) else "REJECT",
        "categories": cats, "losing_categories": losing,
    }


def category_table(records: list[dict], mode: str, arms: list[str]) -> dict:
    out = {}
    for c in CATEGORIES:
        row = {}
        for a in arms:
            rs = [r for r in records if r["mode"] == mode and r["arm"] == a and r["category"] == c and r["status"] == "valid"]
            ok = [r for r in rs if r["oracle"]["pass"]]
            m = lambda k: _med([r["metrics"][k] for r in ok])  # noqa: E731
            row[a] = {"runs": len(rs), "pass_rate": (len(ok) / len(rs)) if rs else None,
                      "median_total_tokens": m("total_tokens"), "median_round_trips": m("round_trips"),
                      "median_tool_result_tokens_est": m("tool_result_tokens_est"),
                      "median_wall_s": _med([r["metrics"]["wall_ms"] / 1000 for r in ok if r["metrics"].get("wall_ms")])}
        out[c] = row
    return out


def cell_config_check(records: list[dict], cfg: dict) -> dict:
    """SF-BEN-02: every cell must state its hook/rule configuration, and it must match its mode."""
    bad = []
    for r in records:
        c, mode, arm = r.get("config", {}), cfg["modes"][r["mode"]], cfg["arms"][r["arm"]]
        expect_hooks = mode["hooks"] and arm["mcp"]
        has_hooks = bool(c.get("installed_hook_events"))
        has_rules = bool(c.get("rule_files"))
        if r["status"] != "infra_error" and (c.get("hooks") != mode["hooks"] or c.get("rules") != mode["rules"]
                                             or has_rules != mode["rules"] or has_hooks != expect_hooks):
            bad.append(r["cell_id"])
    return {"cells": len(records), "mismatched": bad}


def build_report(records: list[dict], cfg: dict, expected_cells_per_mode_pair: int) -> dict[str, Any]:
    run_ids = sorted({r["run_id"] for r in records})
    synthetic = any(r.get("synthetic") for r in records)
    arms = list(cfg["arms"])
    native, head = cfg["headline_pair"]
    comps = []
    for mode in cfg["modes"]:
        for pair in [cfg["headline_pair"], *cfg.get("secondary_pairs", [])]:
            comp = compare(records, mode, pair[0], pair[1], cfg["decision"], expected_cells_per_mode_pair)
            comp["headline"] = list(pair) == list(cfg["headline_pair"])
            comps.append(comp)
    head_comps = [c for c in comps if c["headline"]]
    accepted = all(c["verdict"] == "ACCEPT" for c in head_comps) if cfg["decision"]["require_both_modes"] \
        else any(c["verdict"] == "ACCEPT" for c in head_comps)
    statuses: dict[str, int] = defaultdict(int)
    for r in records:
        statuses[r["status"]] += 1
    per_arm_mode = {}
    for mode in cfg["modes"]:
        for a in arms:
            rs = [r for r in records if r["mode"] == mode and r["arm"] == a and r["status"] != "infra_error"]
            per_arm_mode[f"{a}/{mode}"] = {
                "cells": len(rs),
                "hooks": cfg["modes"][mode]["hooks"] and cfg["arms"][a]["mcp"],
                "rules": cfg["modes"][mode]["rules"],
                "rule_sentinel_hits_median": _med([r["metrics"]["rule_sentinel_hits"] for r in rs]),
                "cells_with_rule_injection": sum(1 for r in rs if r["metrics"]["rule_sentinel_hits"] > 0),
                "median_price_usd": _med([price(r, cfg["prices_usd_per_mtok"]) for r in rs]),
            }
    return {
        "run_ids": run_ids, "synthetic": synthetic, "model": cfg["model"],
        "headline_pair": cfg["headline_pair"], "decision_rule": cfg["decision"],
        "claim": "NOT ADMISSIBLE (synthetic data)" if synthetic else ("ACCEPTED" if accepted else "NOT ACCEPTED"),
        "rule_outcome": "ACCEPT" if accepted else "REJECT",
        "status_counts": dict(statuses), "comparisons": comps,
        "categories": {m: category_table(records, m, arms) for m in cfg["modes"]},
        "arm_mode_config": per_arm_mode, "cell_config_check": cell_config_check(records, cfg),
    }


def _pct(x: float | None) -> str:
    return "n/a" if x is None else f"{x * 100:+.1f}%"


def _num(x: float | None) -> str:
    return "n/a" if x is None else f"{x:,.0f}"


def render_markdown(rep: dict) -> str:
    L = []
    if rep["synthetic"]:
        L += ["> **SYNTHETIC DATA.** Every number below comes from fake_client.py replaying scripted transcripts",
              "> against a bundled fixture repository. Nothing here is a measurement of SymForge or of any model.", ""]
    L += [f"# Token benchmark report: {', '.join(rep['run_ids'])}", "",
          f"- Model: `{rep['model']}`", f"- Headline pair: {rep['headline_pair'][0]} vs {rep['headline_pair'][1]}",
          f"- Rule outcome: **{rep['rule_outcome']}**; whole-task token claim: **{rep['claim']}**",
          f"- Cell status counts: {json.dumps(rep['status_counts'], sort_keys=True)}",
          f"- Cells whose recorded hook/rule configuration does not match their mode: {len(rep['cell_config_check']['mismatched'])} of {rep['cell_config_check']['cells']}",
          ""]
    L += ["## Decision rule results", "",
          "| mode | pair | total saving | median saving | pairs won | excluded pairs | pass counts | valid fraction | verdict | losing categories |",
          "|---|---|---|---|---|---|---|---|---|---|"]
    for c in rep["comparisons"]:
        L.append(f"| {c['mode']} | {c['native']} vs {c['other']}{' (headline)' if c['headline'] else ''} | {_pct(c['total_saving'])} | "
                 f"{_pct(c['median_saving'])} | {c['pairs_won']}/{c['pairs_counted']} | {len(c['excluded_pairs'])} | "
                 f"{c['pass_counts']} | {c['valid_fraction']:.2f} | {c['verdict']} | {', '.join(c['losing_categories']) or 'none'} |")
    L += ["", "Saving is native minus SymForge arm over native, on median total tokens processed (input + cache write + cache read + output) per pair.", ""]
    for c in rep["comparisons"]:
        L += [f"### Per category: {c['native']} vs {c['other']}, mode {c['mode']}", "",
              "| category | pairs | won | median saving |", "|---|---|---|---|"]
        for cat, v in c["categories"].items():
            L.append(f"| {cat} | {v['pairs']} | {v['won']} | {_pct(v['median_saving'])} |")
        L.append("")
    for mode, table in rep["categories"].items():
        L += [f"## Per-category medians, mode {mode} (oracle-passing valid runs)", "",
              "| category | arm | runs | pass rate | median total tokens | median round trips | median tool-result tokens (chars/4) | median wall s |",
              "|---|---|---|---|---|---|---|---|"]
        for cat, row in table.items():
            for arm, v in row.items():
                pr = "n/a" if v["pass_rate"] is None else f"{v['pass_rate']:.2f}"
                ws = "n/a" if v["median_wall_s"] is None else f"{v['median_wall_s']:.1f}"
                L.append(f"| {cat} | {arm} | {v['runs']} | {pr} | {_num(v['median_total_tokens'])} | "
                         f"{_num(v['median_round_trips'])} | {_num(v['median_tool_result_tokens_est'])} | {ws} |")
        L.append("")
    L += ["## Hook and rule configuration per arm and mode", "",
          "Every cell record carries its own configuration (`config` in cells.jsonl); this table summarises it.", "",
          "| arm/mode | cells | client hooks installed | path-scoped rules present | cells where rule text was injected | median rule-sentinel hits | median price USD (assumed rates) |",
          "|---|---|---|---|---|---|---|"]
    for k, v in rep["arm_mode_config"].items():
        mp = "n/a" if v["median_price_usd"] is None else f"{v['median_price_usd']:.4f}"
        L.append(f"| {k} | {v['cells']} | {v['hooks']} | {v['rules']} | {v['cells_with_rule_injection']} | "
                 f"{_num(v['rule_sentinel_hits_median'])} | {mp} |")
    return "\n".join(L) + "\n"


def write_report(records: list[dict], cfg: dict, expected: int, out: Path) -> dict:
    out.mkdir(parents=True, exist_ok=True)
    rep = build_report(records, cfg, expected)
    (out / "report.json").write_text(json.dumps(rep, indent=1, default=str), encoding="utf-8")
    (out / "report.md").write_text(render_markdown(rep), encoding="utf-8")
    with open(out / "cells.jsonl", "w", encoding="utf-8") as f:
        for r in records:
            f.write(json.dumps({k: r.get(k) for k in ("cell_id", "run_id", "status", "synthetic", "config", "metrics", "allowlist")}
                               | {"oracle_pass": (r.get("oracle") or {}).get("pass")}, default=str) + "\n")
    return rep
