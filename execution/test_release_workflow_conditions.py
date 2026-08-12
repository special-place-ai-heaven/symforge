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

import re
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


if __name__ == "__main__":
    unittest.main()
