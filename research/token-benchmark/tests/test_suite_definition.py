"""The task suite and repository pins satisfy the owner decisions structurally."""
import json
import re
import sys
import unittest
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
import aggregate  # noqa: E402
import oracles  # noqa: E402

TASKS = [t for f in sorted((ROOT / "tasks").glob("*.json")) for t in json.loads(f.read_text(encoding="utf-8"))]
REPOS = json.loads((ROOT / "repos.json").read_text(encoding="utf-8"))["repositories"]
CFG = json.loads((ROOT / "config.json").read_text(encoding="utf-8"))


class SuiteTests(unittest.TestCase):
    def test_twenty_four_tasks_three_per_category_six_per_repo(self):
        self.assertEqual(len(TASKS), 24)
        self.assertEqual(len({t["id"] for t in TASKS}), 24)
        self.assertEqual(Counter(t["category"] for t in TASKS), Counter({c: 3 for c in aggregate.CATEGORIES}))
        self.assertEqual(Counter(t["repo"] for t in TASKS), Counter({r["id"]: 6 for r in REPOS}))

    def test_four_pinned_permissive_repos_in_four_languages(self):
        self.assertEqual(len(REPOS), 4)
        self.assertEqual(len({r["language"] for r in REPOS}), 4)
        for r in REPOS:
            self.assertRegex(r["commit"], r"^[0-9a-f]{40}$")
            self.assertTrue(r["url"].startswith("https://github.com/"))
            self.assertRegex(r["license"], r"MIT|BSD|Apache")

    def test_every_oracle_check_is_known_and_every_hidden_file_exists(self):
        for t in TASKS:
            self.assertTrue(t["oracle"], t["id"])
            for c in t["oracle"]:
                self.assertIn(c["type"], oracles.CHECKS, t["id"])
                for h in c.get("hidden", []):
                    self.assertTrue((ROOT / "hidden" / t["repo"] / h).is_file(), f"{t['id']}: {h}")
                if c["type"] == "seed_located":
                    self.assertIn(c["seed"], {s["id"] for s in t.get("seeds", [])}, t["id"])

    def test_no_task_is_graded_by_self_report_alone(self):
        for t in TASKS:
            types = {c["type"] for c in t["oracle"]}
            self.assertFalse(types <= {"answer_regex"}, f"{t['id']} has only frozen answer facts")

    def test_edit_categories_execute_or_compare_trees(self):
        for t in TASKS:
            if t["category"] in ("single-file-edit", "cross-module-edit", "rename-refactor"):
                types = {c["type"] for c in t["oracle"]}
                self.assertTrue(types & {"command", "exact_rename"}, t["id"])

    def test_config_names_model_and_client_only_as_configuration(self):
        self.assertTrue(CFG["model"])
        self.assertIn("{model}", CFG["client"]["command"])
        self.assertEqual(sorted(CFG["arms"]), ["a", "b", "c"])
        self.assertEqual(CFG["repetitions"], 5)
        self.assertIn(list(CFG["headline_pair"]), [["a", "b"], ["a", "c"]])
        for py in ("runner.py", "bench.py", "aggregate.py", "oracles.py", "transcript.py"):
            text = (ROOT / py).read_text(encoding="utf-8")
            self.assertIsNone(re.search(r"claude-(opus|sonnet|haiku|fable)-\d", text), py)

    def test_no_secret_values_in_config(self):
        blob = json.dumps(CFG)
        self.assertIsNone(re.search(r"sk-ant-|ghp_|AKIA", blob))
        self.assertEqual(CFG["client"]["pass_env"], ["ANTHROPIC_API_KEY"])


if __name__ == "__main__":
    unittest.main()
