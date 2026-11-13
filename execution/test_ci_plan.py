"""The CI plan: who may use self-hosted runners, and when heavy jobs may skip."""

from __future__ import annotations

import unittest

import ci_plan

REPO = "owner/symforge"
TOML_OLD = '[package]\nname = "symforge"\nkeywords = ["a"]\nversion = "1.2.3"\n\n[dependencies]\nfoo = "1"\n'
TOML_NEW = TOML_OLD.replace("1.2.3", "1.2.4")


def never_referenced(_path: str) -> bool:
    return False


def readers(old: dict[str, str], new: dict[str, str]):
    return old.get, new.get


class RunnerChoice(unittest.TestCase):
    def choose(self, event="push", head=REPO, labels=(), setting="", online=None, actor="", head_ref="", ref_name=""):
        return ci_plan.choose_runners(event, head, REPO, list(labels), setting, online or {}, actor, head_ref, ref_name)

    def test_trusted_events_default_to_self_hosted(self):
        chosen = self.choose()
        self.assertEqual(chosen["linux"][0], ci_plan.SELF_HOSTED["linux"])
        # The one Windows runner may be busy; unknown status means hosted.
        self.assertEqual(chosen["windows"][0], "windows-latest")
        free = self.choose(online={"linux": True, "windows": True})
        self.assertEqual(free["windows"][0], ci_plan.SELF_HOSTED["windows"])
        busy = self.choose(online={"linux": True, "windows": False})
        self.assertEqual(busy["windows"][0], "windows-latest")

    def test_fork_pull_requests_never_reach_self_hosted(self):
        for head in ("someone/symforge", ""):
            chosen = self.choose(event="pull_request", head=head, online={"linux": True, "windows": True})
            self.assertEqual(chosen["linux"][0], "ubuntu-latest")
            self.assertEqual(chosen["windows"][0], "windows-latest")

    def test_dependabot_never_reaches_self_hosted(self):
        for chosen in (
            self.choose(event="pull_request", actor="dependabot[bot]", online={"linux": True, "windows": True}),
            self.choose(event="pull_request", head_ref="dependabot/cargo/serde-1.0.300"),
            self.choose(event="workflow_dispatch", ref_name="dependabot/cargo/serde-1.0.300"),
            self.choose(event="push", ref_name="dependabot/github_actions/x-2"),
        ):
            self.assertEqual(chosen["linux"][0], "ubuntu-latest")
            self.assertEqual(chosen["windows"][0], "windows-latest")

    def test_label_variable_and_offline_pool_choose_hosted(self):
        self.assertEqual(self.choose(event="pull_request", labels=["ci-hosted"])["linux"][0], "ubuntu-latest")
        self.assertEqual(self.choose(setting="hosted")["windows"][0], "windows-latest")
        only_windows = self.choose(setting="hosted-windows", online={"linux": True, "windows": True})
        self.assertEqual(only_windows["windows"][0], "windows-latest")
        self.assertEqual(only_windows["linux"][0], ci_plan.SELF_HOSTED["linux"])
        offline = self.choose(online={"linux": False, "windows": True})
        self.assertEqual(offline["linux"][0], "ubuntu-latest")
        self.assertEqual(offline["windows"][0], ci_plan.SELF_HOSTED["windows"])

    def test_non_pull_request_events_render_labels_as_null(self):
        for event in ("push", "schedule", "workflow_dispatch"):
            self.assertEqual(ci_plan.pull_request_labels({"EVENT_NAME": event, "PR_LABELS": "null"}), [])

    def test_scheduled_runs_keep_off_the_workstation(self):
        chosen = self.choose(event="schedule")
        self.assertEqual(chosen["windows"][0], "windows-latest")
        self.assertEqual(chosen["linux"][0], ci_plan.SELF_HOSTED["linux"])


class CodeChanged(unittest.TestCase):
    def classify(self, files, old=None, new=None, referenced=never_referenced):
        read_old, read_new = readers(old or {}, new or {})
        return ci_plan.classify(files, read_old, read_new, referenced)[0]

    def test_release_please_bump_skips_heavy_jobs(self):
        old = {"Cargo.toml": TOML_OLD, "Cargo.lock": 'name = "symforge"\nversion = "1.2.3"\n',
               "npm/platforms/x/package.json": '{"version": "1.2.3"}'}
        new = {path: text.replace("1.2.3", "1.2.4") for path, text in old.items()}
        new["Cargo.toml"] = TOML_NEW
        files = [*old, "CHANGELOG.md"]
        self.assertFalse(self.classify(files, old, new))

    def test_dependency_change_in_version_files_is_code(self):
        old = {"Cargo.toml": TOML_OLD, "Cargo.lock": 'version = "1.2.3"\nversion = "0.9.0"\n'}
        new = {"Cargo.toml": TOML_NEW, "Cargo.lock": 'version = "1.2.4"\nversion = "0.9.1"\n'}
        self.assertTrue(self.classify(list(old), old, new))
        same = {"Cargo.toml": TOML_OLD.replace('foo = "1"', 'foo = "2"')}
        self.assertTrue(self.classify(["Cargo.toml"], {"Cargo.toml": TOML_OLD}, same))

    def test_dependency_pin_sharing_the_version_prefix_is_code(self):
        lock = 'name = "symforge"\nversion = "{0}"\nname = "dep"\nversion = "{1}"\n'
        old = {"Cargo.toml": TOML_OLD.replace("1.2.3", "11.18.1"), "Cargo.lock": lock.format("11.18.1", "11.18.10")}
        new = {"Cargo.toml": TOML_OLD.replace("1.2.3", "11.18.2"), "Cargo.lock": lock.format("11.18.2", "11.18.20")}
        self.assertTrue(self.classify(list(old), old, new))
        bump_only = dict(new, **{"Cargo.lock": new["Cargo.lock"].replace("11.18.20", "11.18.10")})
        self.assertFalse(self.classify(list(old), old, bump_only))

    def test_vendored_files_are_code(self):
        self.assertTrue(self.classify(["vendor/tree-sitter-scss/README.md"]))

    def test_documents_tests_read_are_code(self):
        self.assertFalse(self.classify(["docs/guide.md", "specs/030-x/plan.md"]))
        for path in ("README.md", "AGENTS.md", "specs/020-x/contract.md", "tests/fixtures/a.md", "src/x.rs"):
            self.assertTrue(self.classify([path]), path)
        self.assertTrue(self.classify(["docs/ci/known.md"], referenced=lambda path: True))
        self.assertTrue(self.classify([]))


if __name__ == "__main__":
    unittest.main()
