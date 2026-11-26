"""The auto-merge CI requirement: a release PR merges only onto a commit CI proved green."""

from __future__ import annotations

import json
import subprocess
import unittest

import require_ci_success as rcs

SHA = "a" * 40
OTHER = "b" * 40


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


class FakeClock:
    def __init__(self) -> None:
        self.now = 0.0
        self.sleeps: list[float] = []

    def clock(self) -> float:
        return self.now

    def sleep(self, seconds: float) -> None:
        self.sleeps.append(seconds)
        self.now += seconds


def scripted(*payloads: list[dict]):
    queue = list(payloads)

    def fetch() -> dict:
        runs = queue.pop(0) if len(queue) > 1 else queue[0]
        return {"workflow_runs": runs}

    return fetch


class RequireCiSuccess(unittest.TestCase):
    def require(self, fetch, **kwargs) -> str:
        clock = FakeClock()
        self.clock = clock
        options = {"timeout_s": 5400, "interval_s": 30, "grace_s": 300, **kwargs}
        return rcs.require_ci_success(fetch, sha=SHA, clock=clock.clock, sleep=clock.sleep, **options)

    def test_success_passes(self) -> None:
        self.assertIn("succeeded", self.require(scripted([run("completed", "success")])))
        self.assertEqual(self.clock.sleeps, [])

    def test_unsuccessful_conclusions_fail(self) -> None:
        for conclusion in ("failure", "cancelled", "timed_out", "action_required", "startup_failure", None):
            with self.subTest(conclusion=conclusion):
                with self.assertRaises(rcs.CiNotGreen) as caught:
                    self.require(scripted([run("completed", conclusion)]))
                self.assertIn("stays open and unmerged", str(caught.exception))

    def test_in_progress_then_success_passes(self) -> None:
        fetch = scripted([run("queued")], [run("in_progress")], [run("completed", "success")])
        self.assertIn("succeeded", self.require(fetch))
        self.assertEqual(self.clock.sleeps, [30, 30])

    def test_in_progress_then_failure_fails(self) -> None:
        with self.assertRaises(rcs.CiNotGreen):
            self.require(scripted([run("in_progress")], [run("completed", "failure")]))

    def test_missing_run_fails_after_grace(self) -> None:
        with self.assertRaises(rcs.CiNotGreen) as caught:
            self.require(scripted([]))
        self.assertIn("appeared", str(caught.exception))
        self.assertIn("stays open and unmerged", str(caught.exception))
        self.assertGreaterEqual(self.clock.now, 300)
        self.assertLess(self.clock.now, 400)

    def test_run_appearing_within_grace_is_waited_for(self) -> None:
        fetch = scripted([], [], [run("in_progress")], [run("completed", "success")])
        self.assertIn("succeeded", self.require(fetch))

    def test_exhausted_wait_fails(self) -> None:
        with self.assertRaises(rcs.CiNotGreen) as caught:
            self.require(scripted([run("in_progress")]))
        self.assertIn("did not finish", str(caught.exception))
        self.assertIn("stays open and unmerged", str(caught.exception))
        self.assertGreaterEqual(self.clock.now, 5400)
        self.assertLess(self.clock.now, 5400 + 60)

    def test_every_matching_run_must_succeed(self) -> None:
        mixed = [run("completed", "success", run_id=1), run("completed", "failure", run_id=2)]
        with self.assertRaises(rcs.CiNotGreen):
            self.require(scripted(mixed))
        pending = [run("completed", "success", run_id=1), run("in_progress", run_id=2)]
        both = [run("completed", "success", run_id=1), run("completed", "success", run_id=2)]
        self.assertIn("succeeded", self.require(scripted(pending, both)))

    def test_a_rerun_attempt_in_progress_hides_the_old_success(self) -> None:
        fetch = scripted([run("in_progress", None, attempt=2)], [run("completed", "failure", attempt=2)])
        with self.assertRaises(rcs.CiNotGreen):
            self.require(fetch)

    def test_runs_for_other_commits_events_branches_or_workflows_do_not_count(self) -> None:
        strangers = [
            run("completed", "success", sha=OTHER),
            run("completed", "success", event="pull_request"),
            run("completed", "success", branch="feature"),
            run("completed", "success", path=".github/workflows/release.yml"),
        ]
        with self.assertRaises(rcs.CiNotGreen) as caught:
            self.require(scripted(strangers))
        self.assertIn("appeared", str(caught.exception))

    def test_rejects_a_malformed_commit(self) -> None:
        with self.assertRaises(rcs.CiNotGreen):
            rcs.require_ci_success(scripted([]), sha="HEAD", clock=lambda: 0.0, sleep=lambda _: None)


class GhFetch(unittest.TestCase):
    def test_queries_ci_push_runs_for_the_exact_commit(self) -> None:
        seen: list[list[str]] = []

        def fake_run(argv, **kwargs):
            seen.append(argv)
            return subprocess.CompletedProcess(argv, 0, stdout=json.dumps({"workflow_runs": []}).encode(), stderr=b"")

        self.assertEqual(rcs.gh_fetch("owner/repo", SHA, run=fake_run), {"workflow_runs": []})
        self.assertEqual(
            seen,
            [[
                "gh", "api", "--method", "GET",
                "repos/owner/repo/actions/workflows/ci.yml/runs",
                "-f", f"head_sha={SHA}", "-f", "event=push", "-f", "branch=main", "-f", "per_page=100",
            ]],
        )

    def test_a_failed_lookup_fails_closed(self) -> None:
        def fake_run(argv, **kwargs):
            return subprocess.CompletedProcess(argv, 1, stdout=b"", stderr=b"HTTP 403")

        with self.assertRaises(rcs.CiNotGreen) as caught:
            rcs.gh_fetch("owner/repo", SHA, run=fake_run)
        self.assertIn("stays open and unmerged", str(caught.exception))

    def test_non_json_lookup_fails_closed(self) -> None:
        def fake_run(argv, **kwargs):
            return subprocess.CompletedProcess(argv, 0, stdout=b"<html>", stderr=b"")

        with self.assertRaises(rcs.CiNotGreen):
            rcs.gh_fetch("owner/repo", SHA, run=fake_run)


class Entry(unittest.TestCase):
    def test_defaults_wait_ninety_minutes_polling_every_thirty_seconds(self) -> None:
        args = rcs.build_parser().parse_args(["--sha", SHA, "--repo", "owner/repo"])
        self.assertEqual((args.timeout_minutes, args.interval_seconds, args.grace_seconds), (90, 30, 300))

    def test_missing_repository_fails_closed(self) -> None:
        self.assertEqual(rcs.main(["--sha", SHA, "--repo", ""]), 1)


if __name__ == "__main__":
    unittest.main()
