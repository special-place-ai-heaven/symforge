"""End to end: runner -> fake client -> transcript -> allow-list -> oracle -> report, on the fixture."""
import json
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
import bench  # noqa: E402


class DryRunTests(unittest.TestCase):
    def test_one_repetition_dry_run_produces_a_complete_synthetic_report(self):
        out = Path(tempfile.mkdtemp())
        rc = bench.main(["dry-run", "--out", str(out), "--reps", "1", "--python", sys.executable])
        self.assertEqual(rc, 0)
        recs = [json.loads(p.read_text()) for p in (out / "records").glob("*.json")]
        self.assertEqual(len(recs), 8 * 3 * 2)
        self.assertTrue(all(r["synthetic"] for r in recs))
        self.assertEqual({r["status"] for r in recs}, {"valid"})
        # off-mode cells carry no rules or hooks; on-mode SymForge cells carry both
        for r in recs:
            on = r["mode"] == "on"
            self.assertEqual(bool(r["config"]["rule_files"]), on, r["cell_id"])
            self.assertEqual(bool(r["config"]["installed_hook_events"]), on and r["arm"] != "a", r["cell_id"])
        rep = json.loads((out / "report.json").read_text())
        self.assertEqual(rep["claim"], "NOT ADMISSIBLE (synthetic data)")
        self.assertEqual(rep["cell_config_check"]["mismatched"], [])
        self.assertIn("SYNTHETIC DATA", (out / "report.md").read_text())

    def test_scripted_allowlist_violation_is_marked_invalid(self):
        out = Path(tempfile.mkdtemp())
        rc = bench.main(["dry-run", "--out", str(out), "--reps", "3", "--python", sys.executable,
                         "--run-id", "dryrun-violation"])
        self.assertEqual(rc, 0)
        bad = json.loads((out / "records" / "mini__mini-discovery__a__off__r3.json").read_text())
        self.assertEqual(bad["status"], "invalid_allowlist")
        self.assertEqual(bad["allowlist"]["violations"], ["mcp__symforge__search_symbols"])


if __name__ == "__main__":
    unittest.main()
