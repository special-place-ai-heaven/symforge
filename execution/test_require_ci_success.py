"""The auto-merge CI requirement: a release PR merges only onto a commit CI proved green."""

from __future__ import annotations

import json
import re
import subprocess
import unittest
from pathlib import Path

import require_ci_success as rcs

SHA = "a" * 40
OTHER = "b" * 40
CI_WORKFLOW = Path(__file__).resolve().parent.parent / ".github" / "workflows" / "ci.yml"
REQUIRED_COMMANDS = {
    "test": ("cargo nextest run --lib --bins --tests",),
    "bench": ("cargo bench --bench observed_refresh_gate_v1 -- --test",),
    "release-build": ("cargo build --release", "node scripts/verify-tools.cjs --bin target/release/symforge"),
    "embed-build": (
        "cargo build --no-default-features --features embed\n",
        "cargo test --no-default-features --features embed --lib -- --test-threads=1",
        "--test embed_parity --test embed_replay_v1",
    ),
    "server-embed-parity": ("cargo test --features embed,__test-internals --lib server_and_embed_",),
    "npm": ("npm test",),
}


def run(status: str, conclusion: str | None = None, *, run_id: int = 1, attempt: int = 1, sha: str = SHA,
        event: str = "push", branch: str = "main", path: str = ".github/workflows/ci.yml") -> dict:
    return {
        "id": run_id,
        "run_attempt": attempt,
        "head_sha": sha,
        "event": event,
        "head_branch": branch,
        "path": path,
        "status": status,
        "conclusion": conclusion,
    }


def job(name: str, conclusion: str | None = "success", status: str = "completed") -> dict:
    return {"name": name, "status": status, "conclusion": conclusion}


def green_jobs(**overrides: str | None) -> list[dict]:
    jobs = [job("plan"), job("lint"), job("ci-result")]
    for name in rcs.REQUIRED_JOBS:
        if name in overrides and overrides[name] == "missing":
            continue
        jobs.append(job(name, overrides.get(name, "success")))
    return jobs


class FakeClock:
    def __init__(self) -> None:
        self.now = 0.0
        self.sleeps: list[float] = []

    def clock(self) -> float:
        return self.now

    def sleep(self, seconds: float) -> None:
        self.sleeps.append(seconds)
        self.now += seconds


def scripted(*steps):
    queue = list(steps)

    def fetch(*_args):
        step = queue.pop(0) if len(queue) > 1 else queue[0]
        if isinstance(step, Exception):
            raise step
        return step

    return fetch


def runs_of(*runs: dict) -> dict:
    return {"workflow_runs": list(runs)}


def jobs_of(jobs: list[dict]) -> dict:
    return {"jobs": jobs}


def always_green(run_id: int) -> dict:
    return jobs_of(green_jobs())


class RequireCiSuccess(unittest.TestCase):
    def require(self, fetch_runs, fetch_jobs=always_green, **kwargs) -> str:
        clock = FakeClock()
        self.clock = clock
        options = {"timeout_s": 5400, "interval_s": 30, "grace_s": 300, **kwargs}
        return rcs.require_ci_success(
            fetch_runs, fetch_jobs, sha=SHA, clock=clock.clock, sleep=clock.sleep, **options
        )

    def test_success_with_every_required_job_green_passes(self) -> None:
        self.assertIn("succeeded", self.require(scripted(runs_of(run("completed", "success")))))
        self.assertEqual(self.clock.sleeps, [])

    def test_unsuccessful_conclusions_fail(self) -> None:
        for conclusion in ("failure", "cancelled", "timed_out", "action_required", "startup_failure", None):
            with self.subTest(conclusion=conclusion):
                with self.assertRaises(rcs.CiNotGreen) as caught:
                    self.require(scripted(runs_of(run("completed", conclusion))))
                self.assertIn("stays open and unmerged", str(caught.exception))

    def test_green_run_with_a_skipped_required_job_fails(self) -> None:
        for conclusion in ("skipped", "cancelled", "neutral", None):
            with self.subTest(conclusion=conclusion):
                jobs = scripted(jobs_of(green_jobs(test=conclusion)))
                with self.assertRaises(rcs.CiNotGreen) as caught:
                    self.require(scripted(runs_of(run("completed", "success"))), jobs)
                message = str(caught.exception)
                self.assertIn("test", message)
                self.assertTrue(message.endswith(rcs.SAFE_OUTCOME + " " + rcs.NEXT_STEP), message)

    def test_green_run_missing_a_required_job_fails(self) -> None:
        for name in rcs.REQUIRED_JOBS:
            with self.subTest(job=name):
                jobs = scripted(jobs_of(green_jobs(**{name: "missing"})))
                with self.assertRaises(rcs.CiNotGreen) as caught:
                    self.require(scripted(runs_of(run("completed", "success"))), jobs)
                self.assertIn(name, str(caught.exception))
                self.assertIn("missing", str(caught.exception))

    def test_duplicate_required_job_must_be_green_everywhere(self) -> None:
        jobs = scripted(jobs_of(green_jobs() + [job("bench", "failure")]))
        with self.assertRaises(rcs.CiNotGreen):
            self.require(scripted(runs_of(run("completed", "success"))), jobs)

    def test_docs_only_fix_cannot_merge_onto_broken_code(self) -> None:
        x0 = run("completed", "failure", run_id=10, sha=OTHER)
        x1 = run("completed", "success", run_id=11)
        docs_only = [job("plan"), job("conventional-commits"), job("release-version"), job("advisories"), job("npm")]
        docs_only += [job(name, "skipped") for name in rcs.REQUIRED_JOBS if name != "npm"]
        docs_only += [job("ci-result")]
        seen: list[int] = []

        def fetch_jobs(run_id: int) -> dict:
            seen.append(run_id)
            return jobs_of(docs_only)

        with self.assertRaises(rcs.CiNotGreen) as caught:
            self.require(scripted(runs_of(x0, x1)), fetch_jobs)
        self.assertEqual(seen, [11])
        self.assertIn("skipped", str(caught.exception))
        self.assertIn("run the Release workflow manually from main", str(caught.exception))

    def test_in_progress_then_success_passes(self) -> None:
        fetch = scripted(runs_of(run("queued")), runs_of(run("in_progress")), runs_of(run("completed", "success")))
        self.assertIn("succeeded", self.require(fetch))
        self.assertEqual(self.clock.sleeps, [30, 30])

    def test_in_progress_then_failure_fails(self) -> None:
        with self.assertRaises(rcs.CiNotGreen):
            self.require(scripted(runs_of(run("in_progress")), runs_of(run("completed", "failure"))))

    def test_missing_run_fails_after_grace(self) -> None:
        with self.assertRaises(rcs.CiNotGreen) as caught:
            self.require(scripted(runs_of()))
        self.assertIn("appeared", str(caught.exception))
        self.assertIn("stays open and unmerged", str(caught.exception))
        self.assertGreaterEqual(self.clock.now, 300)
        self.assertLess(self.clock.now, 400)

    def test_run_appearing_within_grace_is_waited_for(self) -> None:
        fetch = scripted(runs_of(), runs_of(), runs_of(run("in_progress")), runs_of(run("completed", "success")))
        self.assertIn("succeeded", self.require(fetch))

    def test_exhausted_wait_fails(self) -> None:
        with self.assertRaises(rcs.CiNotGreen) as caught:
            self.require(scripted(runs_of(run("in_progress"))))
        self.assertIn("did not finish", str(caught.exception))
        self.assertIn("stays open and unmerged", str(caught.exception))
        self.assertGreaterEqual(self.clock.now, 5400)
        self.assertLess(self.clock.now, 5400 + 60)

    def test_every_matching_run_must_succeed(self) -> None:
        mixed = runs_of(run("completed", "success", run_id=1), run("completed", "failure", run_id=2))
        with self.assertRaises(rcs.CiNotGreen):
            self.require(scripted(mixed))
        pending = runs_of(run("completed", "success", run_id=1), run("in_progress", run_id=2))
        both = runs_of(run("completed", "success", run_id=1), run("completed", "success", run_id=2))
        seen: list[int] = []

        def fetch_jobs(run_id: int) -> dict:
            seen.append(run_id)
            return jobs_of(green_jobs())

        self.assertIn("succeeded", self.require(scripted(pending, both), fetch_jobs))
        self.assertEqual(seen, [1, 2])

    def test_a_rerun_attempt_in_progress_hides_the_old_success(self) -> None:
        fetch = scripted(runs_of(run("in_progress", None, attempt=2)), runs_of(run("completed", "failure", attempt=2)))
        with self.assertRaises(rcs.CiNotGreen):
            self.require(fetch)

    def test_runs_for_other_commits_events_branches_or_workflows_do_not_count(self) -> None:
        strangers = runs_of(
            run("completed", "success", sha=OTHER),
            run("completed", "success", event="pull_request"),
            run("completed", "success", branch="feature"),
            run("completed", "success", path=".github/workflows/release.yml"),
        )
        with self.assertRaises(rcs.CiNotGreen) as caught:
            self.require(scripted(strangers))
        self.assertIn("appeared", str(caught.exception))

    def test_two_transient_lookup_failures_then_success_passes(self) -> None:
        blip = rcs.LookupFailed("HTTP 502")
        fetch = scripted(blip, blip, runs_of(run("completed", "success")))
        self.assertIn("succeeded", self.require(fetch))
        self.assertEqual(self.clock.sleeps, [30, 30])

    def test_transient_job_lookup_failures_are_retried(self) -> None:
        blip = rcs.LookupFailed("HTTP 502")
        jobs = scripted(blip, blip, blip, blip, jobs_of(green_jobs()))
        self.assertIn("succeeded", self.require(scripted(runs_of(run("completed", "success"))), jobs))

    def test_five_consecutive_lookup_failures_fail(self) -> None:
        with self.assertRaises(rcs.CiNotGreen) as caught:
            self.require(scripted(rcs.LookupFailed("HTTP 502")))
        self.assertIn("5 consecutive", str(caught.exception))
        self.assertIn("stays open and unmerged", str(caught.exception))
        self.assertEqual(len(self.clock.sleeps), 4)

    def test_one_success_resets_the_failure_count(self) -> None:
        blip = rcs.LookupFailed("HTTP 502")
        pending = runs_of(run("in_progress"))
        fetch = scripted(blip, blip, blip, blip, pending, blip, blip, blip, blip, runs_of(run("completed", "success")))
        self.assertIn("succeeded", self.require(fetch))

    def test_lookup_failures_do_not_hide_a_missing_run(self) -> None:
        blip = rcs.LookupFailed("HTTP 502")
        fetch = scripted(runs_of(), blip, runs_of(), blip, runs_of())
        with self.assertRaises(rcs.CiNotGreen) as caught:
            self.require(fetch)
        self.assertIn("appeared", str(caught.exception))

    def test_rejects_a_malformed_commit(self) -> None:
        with self.assertRaises(rcs.CiNotGreen):
            rcs.require_ci_success(
                scripted(runs_of()), always_green, sha="HEAD", clock=lambda: 0.0, sleep=lambda _: None
            )


class GhLookups(unittest.TestCase):
    def fake(self, payload: object, code: int = 0, raw: bytes | None = None):
        seen: list[list[str]] = []

        def fake_run(argv, **kwargs):
            seen.append(argv)
            body = raw if raw is not None else json.dumps(payload).encode()
            return subprocess.CompletedProcess(argv, code, stdout=body, stderr=b"HTTP 502")

        return seen, fake_run

    def test_queries_ci_push_runs_for_the_exact_commit(self) -> None:
        seen, fake_run = self.fake({"workflow_runs": []})
        self.assertEqual(rcs.gh_runs("owner/repo", SHA, run=fake_run), {"workflow_runs": []})
        self.assertEqual(
            seen,
            [[
                "gh", "api", "--method", "GET",
                "repos/owner/repo/actions/workflows/ci.yml/runs",
                "-f", f"head_sha={SHA}", "-f", "event=push", "-f", "branch=main", "-f", "per_page=100",
            ]],
        )

    def test_queries_the_latest_attempt_jobs_of_a_run(self) -> None:
        seen, fake_run = self.fake({"jobs": []})
        self.assertEqual(rcs.gh_jobs("owner/repo", 42, run=fake_run), {"jobs": []})
        self.assertEqual(
            seen,
            [[
                "gh", "api", "--method", "GET",
                "repos/owner/repo/actions/runs/42/jobs",
                "-f", "filter=latest", "-f", "per_page=100",
            ]],
        )

    def test_failed_or_unparsable_lookups_are_transient(self) -> None:
        for code, raw in ((1, b""), (0, b"<html>"), (0, b"[]")):
            with self.subTest(code=code, raw=raw):
                _, fake_run = self.fake(None, code, raw)
                with self.assertRaises(rcs.LookupFailed):
                    rcs.gh_runs("owner/repo", SHA, run=fake_run)
                with self.assertRaises(rcs.LookupFailed):
                    rcs.gh_jobs("owner/repo", 42, run=fake_run)

    def test_job_lists_must_be_complete(self) -> None:
        many = {"total_count": 101, "jobs": [job("plan")] * 100}
        with self.assertRaises(rcs.LookupFailed):
            rcs.required_job_failures(many)


class RequiredJobsMirrorCi(unittest.TestCase):
    def setUp(self) -> None:
        self.text = CI_WORKFLOW.read_text(encoding="utf-8")
        self.jobs: dict[str, str] = {}
        current = None
        for line in self.text.splitlines():
            head = re.match(r"^  ([A-Za-z0-9_-]+):\s*$", line)
            if head:
                current = head.group(1)
                self.jobs[current] = ""
            elif current is not None:
                self.jobs[current] += line + "\n"

    def display_names(self) -> dict[str, str]:
        names = {}
        for job_id, block in self.jobs.items():
            named = re.search(r"^    name:\s*(.+?)\s*$", block, re.M)
            names[named.group(1) if named else job_id] = block
        return names

    def test_every_required_job_is_a_ci_job(self) -> None:
        names = self.display_names()
        for name in rcs.REQUIRED_JOBS:
            self.assertIn(name, names, f"required CI job {name!r} is not a job in ci.yml")

    def test_required_jobs_run_what_release_verification_runs(self) -> None:
        self.assertEqual(set(rcs.REQUIRED_JOBS), set(REQUIRED_COMMANDS))
        names = self.display_names()
        for name, commands in REQUIRED_COMMANDS.items():
            for command in commands:
                self.assertIn(command, names[name], f"{name} no longer runs {command!r}")


class Entry(unittest.TestCase):
    def test_defaults_wait_ninety_minutes_polling_every_thirty_seconds(self) -> None:
        args = rcs.build_parser().parse_args(["--sha", SHA, "--repo", "owner/repo"])
        self.assertEqual((args.timeout_minutes, args.interval_seconds, args.grace_seconds), (90, 30, 300))
        self.assertEqual(rcs.MAX_CONSECUTIVE_LOOKUP_FAILURES, 5)

    def test_missing_repository_fails_closed(self) -> None:
        self.assertEqual(rcs.main(["--sha", SHA, "--repo", ""]), 1)


if __name__ == "__main__":
    unittest.main()
