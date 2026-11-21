"""Unit tests for SymForge 12.0.0 Polyglot & RTK Benchmark Suite."""

from __future__ import annotations

import pathlib
import sys
import unittest

BENCH_DIR = pathlib.Path(__file__).resolve().parent.parent
if str(BENCH_DIR) not in sys.path:
    sys.path.insert(0, str(BENCH_DIR))

from corpus import REPOS, build_tasks, grade_output  # noqa: E402
from render_charts import render_svg  # noqa: E402
from stage2_smol_agent import ARMS, arm_tool_schemas, dry_run_mock_step  # noqa: E402


class PolyglotBenchTests(unittest.TestCase):
    def test_six_languages_and_thirty_tasks(self) -> None:
        self.assertEqual(len(REPOS), 6)
        langs = [r.language for r in REPOS]
        self.assertEqual(langs, ["Rust", "C#", "TypeScript", "Python", "Java", "Go"])
        tasks = build_tasks()
        self.assertEqual(len(tasks), 30)
        for t in tasks:
            self.assertTrue(t["required_substrings"])
            self.assertEqual(len(repo_sha := [r.commit_sha for r in REPOS if r.repo_id == t["repo_id"]][0]), 40)
            self.assertTrue(all(c in "0123456789abcdef" for c in repo_sha))

    def test_oracle_grading_is_deterministic(self) -> None:
        ok = grade_output("Builder::new(Kind::MultiThread, 61) in new_multi_thread", ["new_multi_thread", "61"])
        self.assertTrue(ok["passed"])
        miss = grade_output("pub fn new_multi_thread() -> Builder {", ["new_multi_thread", "61"])
        self.assertFalse(miss["passed"])
        self.assertEqual(miss["missing"], ["61"])

    def test_stage2_arm_schemas_and_dry_run_steps(self) -> None:
        tasks = build_tasks()
        t0 = tasks[0]
        repo = REPOS[0]
        for arm in ARMS:
            schemas = arm_tool_schemas(arm)
            self.assertGreaterEqual(len(schemas), 1)
            step0 = dry_run_mock_step(arm, repo, t0, 0, "")
            self.assertIn("tool", step0)
            step1 = dry_run_mock_step(arm, repo, t0, 1, "mock-output")
            self.assertIn("final", step1)
            graded = grade_output(step1["final"], t0["required_substrings"])
            self.assertTrue(graded["passed"])

    def test_svg_renderer_produces_valid_dark_and_light_xml(self) -> None:
        results_file = BENCH_DIR / "results" / "stage1-results.json"
        if results_file.exists():
            import json

            report = json.loads(results_file.read_text(encoding="utf-8"))
            for theme in ("dark", "light"):
                svg = render_svg(report, theme)
                self.assertIn("<svg", svg)
                self.assertIn("SymForge 12.0.0 vs. RTK 1.2.1", svg)


if __name__ == "__main__":
    unittest.main()
