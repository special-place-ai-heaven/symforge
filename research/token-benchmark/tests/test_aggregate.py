import copy
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import aggregate  # noqa: E402

CFG = {
    "model": "m",
    "arms": {"a": {"mcp": False, "steering": False}, "b": {"mcp": True, "steering": True}, "c": {"mcp": True, "steering": False}},
    "modes": {"off": {"hooks": False, "rules": False}, "on": {"hooks": True, "rules": True}},
    "headline_pair": ["a", "c"],
    "secondary_pairs": [],
    "decision": {"min_valid_fraction": 0.95, "require_quality_not_worse": True, "require_both_modes": True},
    "prices_usd_per_mtok": {"input": 2, "cache_write": 2.5, "cache_read": 0.1, "output": 10},
}


def rec(task, cat, arm, mode, rep, tokens, passed=True, status="valid", synthetic=False):
    m = CFG["modes"][mode]
    return {
        "run_id": "r1", "cell_id": f"x/{task}/{arm}/{mode}/r{rep}", "repo": "x", "task": task, "category": cat,
        "arm": arm, "mode": mode, "rep": rep, "status": status, "synthetic": synthetic,
        "config": {"hooks": m["hooks"], "rules": m["rules"], "rule_files": [{"path": "r"}] if m["rules"] else [],
                   "installed_hook_events": ["PostToolUse"] if m["hooks"] and arm != "a" else []},
        "metrics": {"total_tokens": tokens, "round_trips": 3, "tool_result_tokens_est": 10, "wall_ms": 1000,
                    "input_tokens": 0, "cache_write_tokens": 0, "cache_read_tokens": tokens, "output_tokens": 0,
                    "rule_sentinel_hits": 0},
        "oracle": {"pass": passed, "error": False},
    }


def suite(native_tokens, other_tokens, cats=None, modes=("off", "on"), reps=1, other_pass=True, synthetic=False):
    out = []
    for i, (na, ot) in enumerate(zip(native_tokens, other_tokens)):
        cat = (cats or ["discovery"] * len(native_tokens))[i]
        for mode in modes:
            for k in range(1, reps + 1):
                out.append(rec(f"t{i}", cat, "a", mode, k, na, synthetic=synthetic))
                out.append(rec(f"t{i}", cat, "c", mode, k, ot, passed=other_pass, synthetic=synthetic))
    return out


def report(records, cfg=CFG, n_tasks=3, reps=1):
    return aggregate.build_report(records, cfg, n_tasks * 2 * reps)


class DecisionRuleTests(unittest.TestCase):
    def test_accept_when_total_and_median_positive_in_both_modes(self):
        r = report(suite([100, 100, 100], [80, 90, 95]))
        self.assertEqual(r["rule_outcome"], "ACCEPT")
        self.assertEqual(r["claim"], "ACCEPTED")
        head = [c for c in r["comparisons"] if c["headline"] and c["mode"] == "off"][0]
        self.assertAlmostEqual(head["total_saving"], 1 - 265 / 300)
        self.assertEqual(head["pairs_won"], 3)

    def test_reject_when_total_positive_but_median_negative(self):
        r = report(suite([1000, 100, 100], [100, 110, 110]))
        head = [c for c in r["comparisons"] if c["headline"]][0]
        self.assertGreater(head["total_saving"], 0)
        self.assertLess(head["median_saving"], 0)
        self.assertEqual(head["verdict"], "REJECT")
        self.assertEqual(r["claim"], "NOT ACCEPTED")

    def test_reject_when_median_positive_but_total_negative(self):
        r = report(suite([100, 100, 100], [90, 90, 500]))
        head = [c for c in r["comparisons"] if c["headline"]][0]
        self.assertGreater(head["median_saving"], 0)
        self.assertEqual(head["verdict"], "REJECT")

    def test_quality_gate_blocks_cheaper_but_worse_arm(self):
        recs = suite([100, 100, 100], [50, 50, 50])
        recs.append(rec("t0", "discovery", "c", "off", 2, 10, passed=False))
        recs.append(rec("t0", "discovery", "a", "off", 2, 100, passed=True))
        recs.append(rec("t1", "discovery", "c", "off", 2, 10, passed=False))
        recs.append(rec("t1", "discovery", "a", "off", 2, 100, passed=True))
        r = report(recs, reps=2, n_tasks=3)
        off = [c for c in r["comparisons"] if c["mode"] == "off"][0]
        self.assertFalse(off["gates"]["quality_not_worse"])
        self.assertEqual(off["verdict"], "REJECT")

    def test_pair_without_a_passing_run_is_excluded_not_counted_as_saving(self):
        recs = suite([100, 100, 100], [90, 90, 90])
        for r_ in recs:
            if r_["task"] == "t2" and r_["arm"] == "c":
                r_["oracle"]["pass"] = False
        r = report(recs)
        head = [c for c in r["comparisons"] if c["mode"] == "off"][0]
        self.assertEqual(head["pairs_counted"], 2)
        self.assertEqual([e["task"] for e in head["excluded_pairs"]], ["t2"])

    def test_losing_categories_are_listed_even_when_accepted(self):
        r = report(suite([100, 100, 100, 100], [50, 60, 70, 130],
                         cats=["discovery", "tracing", "review", "rename-refactor"]), n_tasks=4)
        head = [c for c in r["comparisons"] if c["mode"] == "off"][0]
        self.assertEqual(head["verdict"], "ACCEPT")
        self.assertEqual(head["losing_categories"], ["rename-refactor"])

    def test_synthetic_records_never_make_an_admissible_claim(self):
        r = report(suite([100, 100, 100], [80, 80, 80], synthetic=True))
        self.assertEqual(r["rule_outcome"], "ACCEPT")
        self.assertEqual(r["claim"], "NOT ADMISSIBLE (synthetic data)")

    def test_both_modes_required(self):
        recs = suite([100, 100, 100], [80, 80, 80], modes=("off",)) + suite([100, 100, 100], [120, 120, 120], modes=("on",))
        self.assertEqual(report(recs)["claim"], "NOT ACCEPTED")
        cfg = copy.deepcopy(CFG)
        cfg["decision"]["require_both_modes"] = False
        self.assertEqual(report(recs, cfg)["claim"], "ACCEPTED")

    def test_allowlist_violations_and_missing_cells_fail_the_valid_fraction_gate(self):
        recs = suite([100, 100, 100], [80, 80, 80])
        recs[1]["status"] = "invalid_allowlist"  # t0 / c / off
        r = report(recs)
        off = [c for c in r["comparisons"] if c["mode"] == "off"][0]
        self.assertFalse(off["gates"]["enough_valid_cells"])
        self.assertEqual(off["pairs_counted"], 2)

    def test_cell_config_check_flags_a_cell_whose_rules_do_not_match_its_mode(self):
        recs = suite([100, 100, 100], [80, 80, 80])
        self.assertEqual(report(recs)["cell_config_check"]["mismatched"], [])
        recs[0]["config"]["rule_files"] = [{"path": "stray"}]  # an off-mode cell that carried rules
        self.assertEqual(len(report(recs)["cell_config_check"]["mismatched"]), 1)

    def test_markdown_renders_synthetic_banner_and_every_section(self):
        md = aggregate.render_markdown(report(suite([100, 100, 100], [80, 80, 80], synthetic=True)))
        for needle in ("SYNTHETIC DATA", "Decision rule results", "Per-category medians", "Hook and rule configuration"):
            self.assertIn(needle, md)


if __name__ == "__main__":
    unittest.main()
