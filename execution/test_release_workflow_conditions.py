"""Guard the release workflow's job conditions against a fail-open edit.

A job-level `if` containing no status check function is implicitly wrapped in
`success()`, which is false when any job in `needs` was skipped. The release
workflow's pre-activation gate is skipped by design, and that skip propagates
past `gate-release-ref` even though it survives via `always()`, so every job
below names a status function to stop inheriting it.

Naming a status function also discards the implicit `success()`. From then on,
adding an entry to that job's `needs` without also asserting its result lets the
job run when its new dependency FAILED. The `needs:` list and the `if:` line sit
two lines apart and are semantically decoupled, so review does not reliably catch
it. This test does.

Deliberately hand-parsed: PyYAML is not in the stdlib and the CI runners do not
install it, and a test that skips when its import fails is the same silent-pass
defect it exists to prevent.
"""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

WORKFLOW = Path(__file__).resolve().parent.parent / ".github" / "workflows" / "release.yml"

STATUS_FUNCTIONS = ("success(", "failure(", "cancelled(", "always(")

# Jobs that legitimately keep the implicit success() wrap. Each is listed with
# why, so removing one is a deliberate act rather than an oversight.
IMPLICIT_SUCCESS_ALLOWED = {
    "resolve-release-ref": "root job, no needs",
    "verify-release-ref": "implicit success() over resolve-release-ref is what we want",
    "feature-020-v11-gate": "implicit success() plus the phase check; must not run if verification failed",
    "gate-release-ref": "always() by design; its result checks live in the step body",
}

# Strictly narrower, and deliberately NOT the set above: the only job allowed to
# name a status function without asserting its dependencies in the expression,
# because it asserts them in the step body instead. Reusing the wider set here
# would exempt any job that later grows a status function -- the exact fail-open
# this file exists to catch.
STEP_BODY_RESULT_CHECKS = {
    "gate-release-ref": "checks VERIFY_RESULT/FEATURE_GATE_RESULT in the step body",
}

JOB_RE = re.compile(r"^  ([A-Za-z0-9_-]+):\s*$")
NEEDS_ITEM_RE = re.compile(r"^      - ([A-Za-z0-9_-]+)\s*$")
IF_RE = re.compile(r"^    if:\s*(.+?)\s*$")


def parse_jobs(text: str) -> dict[str, dict[str, object]]:
    """Return {job_id: {"needs": [...], "if": str|None}} for release.yml."""
    jobs: dict[str, dict[str, object]] = {}
    current: str | None = None
    in_needs = False
    in_jobs = False
    for line in text.splitlines():
        if line.startswith("jobs:"):
            in_jobs = True
            continue
        if not in_jobs:
            continue
        # A non-indented key ends the jobs block.
        if line and not line.startswith(" ") and not line.startswith("#"):
            break
        job = JOB_RE.match(line)
        if job:
            current = job.group(1)
            jobs[current] = {"needs": [], "if": None}
            in_needs = False
            continue
        if current is None:
            continue
        if line.strip() == "needs:":
            in_needs = True
            continue
        if in_needs:
            item = NEEDS_ITEM_RE.match(line)
            if item:
                jobs[current]["needs"].append(item.group(1))  # type: ignore[union-attr]
                continue
            in_needs = False
        single = re.match(r"^    needs:\s*([A-Za-z0-9_-]+)\s*$", line)
        if single:
            jobs[current]["needs"] = [single.group(1)]
            continue
        condition = IF_RE.match(line)
        if condition:
            jobs[current]["if"] = condition.group(1)
    return jobs


class ReleaseWorkflowConditions(unittest.TestCase):
    def setUp(self) -> None:
        self.assertTrue(WORKFLOW.is_file(), f"missing {WORKFLOW}")
        self.jobs = parse_jobs(WORKFLOW.read_text(encoding="utf-8"))

    def test_parse_found_the_workflow_shape(self) -> None:
        """Fail loudly rather than vacuously if the hand parse stops matching."""
        self.assertGreaterEqual(len(self.jobs), 10, f"parsed only {sorted(self.jobs)}")
        for required in ("resolve-release-ref", "gate-release-ref", "prepare-release", "cargo-publish"):
            self.assertIn(required, self.jobs)
        self.assertEqual(self.jobs["verify-release-ref"]["needs"], ["resolve-release-ref"])
        self.assertGreaterEqual(len(self.jobs["cargo-publish"]["needs"]), 5)

    @staticmethod
    def _requires_success(condition: str, dependency: str) -> bool:
        """True only if the condition demands that dependency SUCCEEDED.

        Both reference forms are accepted. Matching the whole comparison matters:
        `needs.build.result != 'cancelled'` mentions the dependency but still
        permits `skipped` and `failure`.
        """
        return any(
            term in condition
            for term in (
                f"needs.{dependency}.result == 'success'",
                f"needs['{dependency}'].result == 'success'",
            )
        )

    def test_status_function_jobs_assert_every_dependency_succeeded(self) -> None:
        checked = 0
        for name, job in sorted(self.jobs.items()):
            condition = job["if"]
            if not isinstance(condition, str) or not any(f in condition for f in STATUS_FUNCTIONS):
                continue
            checked += 1
            if name in STEP_BODY_RESULT_CHECKS:
                continue
            for dependency in job["needs"]:  # type: ignore[union-attr]
                self.assertTrue(
                    self._requires_success(condition, dependency),
                    f"job {name!r} names a status function, which discards the implicit "
                    f"success(), but never requires needs.{dependency}.result == 'success' "
                    f"-- it will run when {dependency!r} fails",
                )
        self.assertGreaterEqual(
            checked, 6, "expected the six post-gate jobs to name a status function"
        )

    def test_conditions_never_reference_a_job_they_do_not_need(self) -> None:
        """A reference to a job absent from `needs` evaluates to null forever.

        `null == 'success'` is false, so the job never runs and nothing reports an
        error -- a permanent silent no-release, the same family as the skip bug in
        the opposite direction.
        """
        for name, job in sorted(self.jobs.items()):
            condition = job["if"]
            if not isinstance(condition, str):
                continue
            referenced = set(re.findall(r"needs\.([A-Za-z0-9_-]+)\.", condition))
            referenced |= set(re.findall(r"needs\['([A-Za-z0-9_-]+)'\]", condition))
            declared = set(job["needs"])  # type: ignore[arg-type]
            stray = sorted(referenced - declared)
            self.assertEqual(
                stray,
                [],
                f"job {name!r} references {stray} which are not in its needs; those "
                f"expressions evaluate to null and silently block the job forever",
            )

    def test_jobs_without_a_status_function_are_deliberate(self) -> None:
        for name, job in sorted(self.jobs.items()):
            condition = job["if"]
            if isinstance(condition, str) and any(f in condition for f in STATUS_FUNCTIONS):
                continue
            self.assertIn(
                name,
                IMPLICIT_SUCCESS_ALLOWED,
                f"job {name!r} relies on the implicit success() wrap; if anything it needs "
                f"is skipped it silently skips too. Add a status function plus explicit "
                f"result checks, or record why here.",
            )


FULL_VERIFY_CONDITION = "needs.resolve-release-ref.outputs.full_verify == 'true'"
HEAVY_MARKERS = ("cargo ", "npm ", "verify-tools", "ensure_rustup", "ensure_ripgrep", "free_runner_disk", "clone_phase0")
HEAVY_STEPS = (
    "Free disk space for Rust builds",
    "Install Rust toolchain (rust-toolchain.toml)",
    "Run cargo check",
    "Clone phase0 golden-replay corpora",
    "Run Rust tests",
    "Smoke the observed-refresh benchmark (criterion --test mode)",
    "Build release binary on main pushes",
    "Build engine-only embed feature",
    "Run embed-feature lib tests",
    "Run embed parity and replay contracts",
    "Test npm package",
    "Install ripgrep (harness grep oracle)",
    "Verify tool correctness (both fixtures)",
)
EVERY_PUSH_STEPS = (
    "Test release-please recovery contracts",
    "Install GitHub CLI",
    "Validate commit subjects since last successful Release",
    "Validate manual run resolved tag commit subject",
    "Run version sync tests",
)
PUBLISHING_JOBS = ("build", "build-npm-package", "upload-release-assets", "cargo-publish", "npm-publish")
STEP_START_RE = re.compile(r"^      - ")
STEP_NAME_RE = re.compile(r"^(?:      - |        )name:\s*(.+?)\s*$", re.M)
STEP_ID_RE = re.compile(r"^(?:      - |        )id:\s*(.+?)\s*$", re.M)
STEP_IF_RE = re.compile(r"^        if:\s*(.+?)\s*$", re.M)


def job_block(text: str, name: str) -> str:
    lines = text.splitlines()
    start = lines.index(f"  {name}:")
    end = next(
        (i for i in range(start + 1, len(lines)) if JOB_RE.match(lines[i])),
        len(lines),
    )
    return "\n".join(lines[start:end])


def job_steps(block: str) -> list[str]:
    steps: list[list[str]] = []
    for line in block.splitlines():
        if STEP_START_RE.match(line):
            steps.append([line])
        elif steps and (line.startswith("        ") or not line.strip()):
            steps[-1].append(line)
        elif steps and line.startswith("      #"):
            continue
        elif steps:
            steps.append([])
    return ["\n".join(step) for step in steps if step]


def step_named(steps: list[str], name: str) -> str:
    found = [step for step in steps if (m := STEP_NAME_RE.search(step)) and m.group(1) == name]
    if len(found) != 1:
        raise AssertionError(f"expected exactly one step named {name!r}, found {len(found)}")
    return found[0]


def step_with_id(steps: list[str], step_id: str) -> str:
    found = [step for step in steps if (m := STEP_ID_RE.search(step)) and m.group(1) == step_id]
    if len(found) != 1:
        raise AssertionError(f"expected exactly one step with id {step_id!r}, found {len(found)}")
    return found[0]


def step_condition(step: str) -> str:
    match = STEP_IF_RE.search(step)
    return match.group(1) if match else ""


def run_script(step: str) -> str:
    lines = step.splitlines()
    start = lines.index("        run: |") + 1
    body = []
    for line in lines[start:]:
        if line and not line.startswith("          "):
            break
        body.append(line[10:])
    return "\n".join(body).rstrip() + "\n"


class ReleaseVerifySkipsOnlyNonPublishingPushes(unittest.TestCase):
    def setUp(self) -> None:
        self.text = WORKFLOW.read_text(encoding="utf-8")
        self.verify_steps = job_steps(job_block(self.text, "verify-release-ref"))

    def test_resolve_exports_the_publish_scope(self) -> None:
        resolve = job_block(self.text, "resolve-release-ref")
        self.assertIn("      can_publish: ${{ steps.publish-scope.outputs.can_publish }}", resolve)
        self.assertIn("      full_verify: ${{ steps.publish-scope.outputs.full_verify }}", resolve)
        step_with_id(job_steps(resolve), "publish-scope")

    def test_every_heavy_verify_step_requires_full_verification(self) -> None:
        for name in HEAVY_STEPS:
            self.assertIn(FULL_VERIFY_CONDITION, step_condition(step_named(self.verify_steps, name)), name)
        heavy = 0
        for step in self.verify_steps:
            body = "\n".join(line for line in step.splitlines() if not line.lstrip().startswith("#"))
            if any(marker in body for marker in HEAVY_MARKERS):
                heavy += 1
                self.assertIn(
                    FULL_VERIFY_CONDITION,
                    step_condition(step),
                    f"verify-release-ref step without the full_verify condition:\n{step}",
                )
        self.assertGreaterEqual(heavy, len(HEAVY_STEPS))

    def test_every_push_steps_still_run(self) -> None:
        for name in EVERY_PUSH_STEPS:
            self.assertNotIn("full_verify", step_condition(step_named(self.verify_steps, name)), name)
        for step in self.verify_steps:
            if "actions/checkout@" in step or "actions/setup-python@" in step:
                self.assertNotIn("full_verify", step)
        self.assertIn("check-push-range", step_named(self.verify_steps, EVERY_PUSH_STEPS[2]))

    def test_context_refuses_a_release_this_run_did_not_fully_verify(self) -> None:
        context = step_with_id(job_steps(job_block(self.text, "prepare-release")), "context")
        self.assertIn(
            "          RESOLVED_CAN_PUBLISH: ${{ needs.resolve-release-ref.outputs.can_publish }}",
            context,
        )
        guard = (
            '          if [[ "$RELEASE_READY" == "true" && "$RESOLVED_CAN_PUBLISH" != "true" ]]; then\n'
            '            echo "::error::This run skipped full verification but now resolves to a publishable release."\n'
            "            exit 1\n"
            "          fi\n"
        )
        self.assertIn(guard, context)
        self.assertLess(context.index(guard), context.index('echo "release_created=$RELEASE_READY"'))
        self.assertEqual(context.count('echo "release_created='), 1)

    def test_publishing_jobs_require_a_created_release(self) -> None:
        jobs = parse_jobs(self.text)
        for name in PUBLISHING_JOBS:
            self.assertIn("needs.prepare-release.outputs.release_created == 'true'", jobs[name]["if"], name)


def find_bash() -> str | None:
    if os.name != "nt":
        return shutil.which("bash")
    git = shutil.which("git")
    candidates = [Path(os.environ.get("ProgramFiles", r"C:\Program Files")) / "Git" / "bin" / "bash.exe"]
    if git:
        candidates.insert(0, Path(git).resolve().parent.parent / "bin" / "bash.exe")
    return next((str(c) for c in candidates if c.is_file()), None)


class PublishScopeScriptBehaviour(unittest.TestCase):
    def setUp(self) -> None:
        bash = find_bash()
        if bash is None:
            if os.name == "nt":
                self.skipTest("Git Bash is not available on this Windows host")
            self.fail("bash is required to exercise the publish-scope script")
        self.bash = bash
        text = WORKFLOW.read_text(encoding="utf-8")
        step = step_with_id(job_steps(job_block(text, "resolve-release-ref")), "publish-scope")
        self.script = run_script(step)
        self.tmp = tempfile.TemporaryDirectory()
        self.repo = Path(self.tmp.name)
        self.env = {
            **{k: v for k, v in os.environ.items() if not k.startswith("GIT_")},
            "GIT_AUTHOR_NAME": "t",
            "GIT_AUTHOR_EMAIL": "t@example.invalid",
            "GIT_COMMITTER_NAME": "t",
            "GIT_COMMITTER_EMAIL": "t@example.invalid",
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_CONFIG_GLOBAL": os.devnull,
            "PATH": os.path.dirname(sys.executable) + os.pathsep + os.environ.get("PATH", ""),
        }
        (self.repo / "scope.sh").write_bytes(self.script.encode("utf-8"))
        self.git("init", "-q", "-b", "main")

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def git(self, *args: str) -> str:
        return subprocess.run(
            ["git", *args], cwd=self.repo, env=self.env, check=True, capture_output=True
        ).stdout.decode("utf-8").strip()

    def commit(self, version: str) -> str:
        manifest = self.repo / ".github" / ".release-please-manifest.json"
        manifest.parent.mkdir(exist_ok=True)
        manifest.write_bytes(json.dumps({".": version}).encode("utf-8"))
        self.git("add", ".github")
        self.git("commit", "-q", "--allow-empty", "-m", f"chore: {version}")
        return self.git("rev-parse", "HEAD")

    def scope(self, *, manual_tag: str = "", lifecycle: str = "false") -> dict[str, str]:
        output = self.repo / "out.txt"
        output.write_bytes(b"")
        env = {
            **self.env,
            "GITHUB_OUTPUT": str(output),
            "MANUAL_TAG": manual_tag,
            "LIFECYCLE_GATE_ACTIVE": lifecycle,
            "RELEASE_REF": self.git("rev-parse", "HEAD"),
        }
        subprocess.run([self.bash, "scope.sh"], cwd=self.repo, env=env, check=True, capture_output=True)
        pairs = output.read_bytes().decode("utf-8").split("\n")
        return dict(line.split("=", 1) for line in pairs if line)

    def test_missing_manifest_tag_can_publish(self) -> None:
        self.commit("1.0.0")
        self.assertEqual(self.scope(), {"can_publish": "true", "full_verify": "true"})

    def test_manifest_tag_on_head_can_publish(self) -> None:
        self.commit("1.0.0")
        self.git("tag", "v1.0.0")
        self.assertEqual(self.scope(), {"can_publish": "true", "full_verify": "true"})

    def test_manifest_tag_on_ancestor_cannot_publish(self) -> None:
        self.commit("1.0.0")
        self.git("tag", "v1.0.0")
        self.commit("1.0.0")
        self.assertEqual(self.scope(), {"can_publish": "false", "full_verify": "false"})

    def test_active_lifecycle_keeps_full_verification(self) -> None:
        self.commit("1.0.0")
        self.git("tag", "v1.0.0")
        self.commit("1.0.0")
        for lifecycle in ("true", ""):
            self.assertEqual(
                self.scope(lifecycle=lifecycle), {"can_publish": "false", "full_verify": "true"}
            )

    def test_manual_tag_can_publish(self) -> None:
        self.commit("1.0.0")
        self.git("tag", "v1.0.0")
        self.commit("1.0.0")
        self.assertEqual(self.scope(manual_tag="v1.0.0"), {"can_publish": "true", "full_verify": "true"})

    def test_unreadable_manifest_fails_the_step(self) -> None:
        self.commit("")
        with self.assertRaises(subprocess.CalledProcessError):
            self.scope()


if __name__ == "__main__":
    unittest.main()
