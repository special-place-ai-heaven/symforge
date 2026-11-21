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
        self.assertEqual(len(recs), 8 * 5 * 2)
        self.assertTrue(all(r["synthetic"] for r in recs))
        self.assertEqual({r["status"] for r in recs}, {"valid"})
        # off-mode cells carry no rules or hooks; on-mode MCP SymForge cells carry both
        for r in recs:
            on = r["mode"] == "on"
            mcp_arm = r["arm"] in ("b", "c", "d")
            self.assertEqual(bool(r["config"]["rule_files"]), on, r["cell_id"])
            self.assertEqual(bool(r["config"]["installed_hook_events"]), on and mcp_arm, r["cell_id"])
            if r["arm"] == "e":
                self.assertGreaterEqual(r["metrics"]["symforge_cli_calls"], 1, r["cell_id"])
        rep = json.loads((out / "report.json").read_text())
        self.assertEqual(rep["claim"], "NOT ADMISSIBLE (synthetic data)")
        self.assertEqual(rep["cell_config_check"]["mismatched"], [])
        for mode in ("off", "on"):
            self.assertEqual(rep["arm_mode_config"][f"d/{mode}"]["median_symforge_cli_calls"], 0)
            self.assertEqual(rep["arm_mode_config"][f"e/{mode}"]["median_symforge_cli_calls"], 1)
            self.assertGreater(rep["arm_mode_config"][f"d/{mode}"]["median_price_usd"], 0)
            self.assertGreater(rep["arm_mode_config"][f"e/{mode}"]["median_price_usd"], 0)
            for cat, row in rep["categories"][mode].items():
                if row["d"]["pass_rate"]:
                    self.assertEqual(row["d"]["median_symforge_cli_calls"], 0, f"{mode}/{cat}")
                    self.assertGreater(row["d"]["median_price_usd"], 0, f"{mode}/{cat}")
                if row["e"]["pass_rate"]:
                    self.assertEqual(row["e"]["median_symforge_cli_calls"], 1, f"{mode}/{cat}")
                    self.assertGreater(row["e"]["median_price_usd"], 0, f"{mode}/{cat}")
        md = (out / "report.md").read_text()
        self.assertIn("SYNTHETIC DATA", md)
        self.assertIn("median price USD", md)
        self.assertIn("median cli_calls", md)

    def test_scripted_allowlist_violation_is_marked_invalid(self):
        out = Path(tempfile.mkdtemp())
        rc = bench.main(["dry-run", "--out", str(out), "--reps", "3", "--python", sys.executable,
                         "--run-id", "dryrun-violation"])
        self.assertEqual(rc, 0)
        bad = json.loads((out / "records" / "mini__mini-discovery__a__off__r3.json").read_text())
        self.assertEqual(bad["status"], "invalid_allowlist")
        self.assertEqual(bad["allowlist"]["violations"], ["mcp__symforge__search_symbols"])
        bad_e = json.loads((out / "records" / "mini__mini-discovery__e__off__r3.json").read_text())
        self.assertEqual(bad_e["status"], "invalid_allowlist")
        self.assertEqual(bad_e["allowlist"]["violations"], ["mcp__symforge__search_symbols"])


if __name__ == "__main__":
    unittest.main()
