import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import oracles  # noqa: E402

GIT_ID = ["-c", "user.name=t", "-c", "user.email=t@invalid", "-c", "commit.gpgsign=false"]


def make_repo(files: dict[str, str]) -> tuple[Path, str]:
    d = Path(tempfile.mkdtemp())
    subprocess.run(["git", "init", "-q", str(d)], check=True)
    subprocess.run(["git", "-C", str(d), "config", "core.autocrlf", "false"], check=True)
    for p, t in files.items():
        (d / p).parent.mkdir(parents=True, exist_ok=True)
        (d / p).write_text(t, encoding="utf-8", newline="\n")
    subprocess.run(["git", "-C", str(d), "add", "-A"], check=True)
    subprocess.run(["git", "-C", str(d), *GIT_ID, "commit", "-q", "-m", "base"], check=True)
    base = subprocess.run(["git", "-C", str(d), "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
    return d, base


FILES = {
    "pkg/a.py": "def helper(x):\n    return x\n\n\ndef caller():\n    return helper(1)\n",
    "pkg/b.py": "from .a import helper\n\nVALUE = helper(2)\n",
    "pkg/c.py": "class Box:\n    def put(self):\n        pass\n\n    def take(self):\n        pass\n\n    def _hidden(self):\n        pass\n",
    "README.md": "helper is documented here\n",
}


def grade(task_oracle, wt, base, answer, ctx=None):
    return oracles.grade({"oracle": task_oracle}, wt, base, answer, ctx or {})


class OracleTests(unittest.TestCase):
    def setUp(self):
        self.wt, self.base = make_repo(FILES)

    def test_locate_passes_on_exact_and_suffix_paths_and_fails_otherwise(self):
        o = [{"type": "locate", "pattern": r"^def helper\(", "glob": "*.py", "tolerance": 1}]
        self.assertTrue(grade(o, self.wt, self.base, "ANSWER: pkg/a.py:1")["pass"])
        self.assertTrue(grade(o, self.wt, self.base, r"ANSWER: C:\repo\pkg\a.py:2")["pass"])
        self.assertFalse(grade(o, self.wt, self.base, "ANSWER: pkg/a.py:5")["pass"])
        self.assertFalse(grade(o, self.wt, self.base, "ANSWER: pkg/b.py:1")["pass"])

    def test_locate_with_no_match_is_an_error_not_a_failure(self):
        r = grade([{"type": "locate", "pattern": "^def nothing\\("}], self.wt, self.base, "x")
        self.assertTrue(r["error"])
        self.assertFalse(r["pass"])

    def test_files_mentioned_requires_every_caller_file(self):
        o = [{"type": "files_mentioned", "pattern": r"\bhelper\(", "exclude_line": r"^def helper\(", "glob": "*.py"}]
        self.assertTrue(grade(o, self.wt, self.base, "pkg/a.py:6 and pkg/b.py:3")["pass"])
        r = grade(o, self.wt, self.base, "pkg/b.py:3")
        self.assertFalse(r["pass"])
        self.assertEqual(r["checks"][0]["missing"], ["pkg/a.py"])

    def test_names_derives_public_methods_of_a_block(self):
        o = [{"type": "names", "file": "pkg/c.py", "start": "^class Box", "end": "^class |\\Z", "item": r"^    def ([a-z]\w*)\("}]
        self.assertTrue(grade(o, self.wt, self.base, "put and take")["pass"])
        self.assertFalse(grade(o, self.wt, self.base, "put only")["pass"])

    def test_answer_regex(self):
        o = [{"type": "answer_regex", "all": [["foo", "bar"], ["baz"]]}]
        self.assertTrue(grade(o, self.wt, self.base, "bar baz")["pass"])
        self.assertFalse(grade(o, self.wt, self.base, "bar")["pass"])

    def test_seed_located_uses_recorded_seed_line(self):
        o = [{"type": "seed_located", "seed": "s", "tolerance": 2}]
        ctx = {"seeds": {"s": {"path": "pkg/a.py", "line": 6}}}
        self.assertTrue(grade(o, self.wt, self.base, "pkg/a.py:5 is wrong", ctx)["pass"])
        self.assertFalse(grade(o, self.wt, self.base, "pkg/a.py:1 is wrong", ctx)["pass"])
        self.assertTrue(grade(o, self.wt, self.base, "pkg/a.py:6", {})["error"])

    def test_exact_rename_accepts_only_the_pure_rename(self):
        o = [{"type": "exact_rename", "old": "helper", "new": "assist"}]
        self.assertFalse(grade(o, self.wt, self.base, "")["pass"])  # nothing renamed
        for p in ("pkg/a.py", "pkg/b.py", "README.md"):
            f = self.wt / p
            f.write_text(f.read_text().replace("helper", "assist"), encoding="utf-8", newline="\n")
        self.assertTrue(grade(o, self.wt, self.base, "")["pass"])
        (self.wt / "pkg/c.py").write_text("changed\n", encoding="utf-8", newline="\n")
        self.assertFalse(grade(o, self.wt, self.base, "")["pass"])

    def test_exact_rename_fails_when_one_site_is_missed(self):
        o = [{"type": "exact_rename", "old": "helper", "new": "assist"}]
        f = self.wt / "pkg/a.py"
        f.write_text(f.read_text().replace("helper", "assist"), encoding="utf-8", newline="\n")
        r = grade(o, self.wt, self.base, "")
        self.assertFalse(r["pass"])
        self.assertIn("pkg/b.py", r["checks"][0]["not_renamed"])

    def test_diff_scope_requires_a_change_and_rejects_out_of_scope_files(self):
        o = [{"type": "diff_scope", "allow": ["pkg/a.py"]}]
        self.assertFalse(grade(o, self.wt, self.base, "")["pass"])
        (self.wt / "pkg/a.py").write_text("x = 1\n", encoding="utf-8")
        (self.wt / "pkg/__pycache__").mkdir()
        (self.wt / "pkg/__pycache__/a.pyc").write_bytes(b"\0")
        self.assertTrue(grade(o, self.wt, self.base, "")["pass"])
        (self.wt / "new.py").write_text("y = 2\n", encoding="utf-8")
        self.assertFalse(grade(o, self.wt, self.base, "")["pass"])

    def test_command_copies_hidden_files_and_uses_exit_code(self):
        hidden = Path(tempfile.mkdtemp())
        (hidden / "check.py").write_text("import pkg.a\nassert pkg.a.helper(3) == 3\n", encoding="utf-8")
        (self.wt / "pkg/__init__.py").write_text("", encoding="utf-8")
        o = [{"type": "command", "hidden": ["check.py"], "argv": ["{python}", "check.py"]}]
        ctx = {"hidden_root": str(hidden), "python": sys.executable}
        self.assertTrue(grade(o, self.wt, self.base, "", ctx)["pass"])
        (hidden / "check.py").write_text("raise SystemExit(3)\n", encoding="utf-8")
        self.assertFalse(grade(o, self.wt, self.base, "", ctx)["pass"])

    def test_missing_toolchain_is_an_error(self):
        o = [{"type": "command", "argv": ["definitely-not-a-real-tool-xyz"]}]
        self.assertTrue(grade(o, self.wt, self.base, "", {})["error"])

    def test_tree_checks_run_before_command_checks(self):
        hidden = Path(tempfile.mkdtemp())
        (hidden / "extra.py").write_text("pass\n", encoding="utf-8")
        (self.wt / "pkg/a.py").write_text("x = 1\n", encoding="utf-8")
        o = [{"type": "command", "hidden": ["extra.py"], "argv": [sys.executable, "-c", "pass"]},
             {"type": "diff_scope", "allow": ["pkg/a.py"]}]
        r = grade(o, self.wt, self.base, "", {"hidden_root": str(hidden)})
        self.assertTrue(r["pass"], r)


if __name__ == "__main__":
    unittest.main()
