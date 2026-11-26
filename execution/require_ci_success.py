#!/usr/bin/env python
"""Require that CI proved a commit green before a release PR is merged onto it.

A push that cannot publish skips the release verification build. If release-please
then opens a release PR, merging it straight away would put an untagged release
commit on main on top of a commit nothing has verified yet. This waits for every
push run of ci.yml for the exact commit, fails unless each one succeeded, and then
requires that the CI jobs mirroring the release verification build actually ran
green in that run: ci.yml skips them for a push that changes no code, and a green
run with them skipped proves nothing about the code under the release PR. A
failure leaves the release PR open and unmerged.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import time
from typing import Callable

WORKFLOW_FILE = "ci.yml"
WORKFLOW_PATH = ".github/workflows/ci.yml"
EVENT = "push"
BRANCH = "main"
REQUIRED_JOBS = ("test", "bench", "release-build", "embed-build", "server-embed-parity", "npm")
MAX_CONSECUTIVE_LOOKUP_FAILURES = 5
PAGE_SIZE = 100
SAFE_OUTCOME = "The release PR stays open and unmerged."
NEXT_STEP = (
    "The next push that changes code is checked again, or run the Release workflow "
    "manually from main to force a full verification."
)
COMMIT_RE = re.compile(r"[0-9a-f]{40}")


class CiNotGreen(RuntimeError):
    """CI did not prove the commit green."""


class LookupFailed(RuntimeError):
    """A GitHub lookup failed or returned something unreadable; worth retrying."""


def gh_json(argv: list[str], what: str, run: Callable[..., subprocess.CompletedProcess]) -> dict:
    completed = run(argv, stdin=subprocess.DEVNULL, capture_output=True, check=False)
    if completed.returncode != 0:
        raise LookupFailed(f"Could not read {what}.")
    try:
        payload = json.loads(completed.stdout)
    except (json.JSONDecodeError, TypeError, UnicodeDecodeError) as error:
        raise LookupFailed(f"The lookup of {what} returned no JSON.") from error
    if not isinstance(payload, dict):
        raise LookupFailed(f"The lookup of {what} returned an unexpected shape.")
    return payload


def gh_runs(repo: str, sha: str, run: Callable[..., subprocess.CompletedProcess] = subprocess.run) -> dict:
    argv = [
        "gh", "api", "--method", "GET",
        f"repos/{repo}/actions/workflows/{WORKFLOW_FILE}/runs",
        "-f", f"head_sha={sha}", "-f", f"event={EVENT}", "-f", f"branch={BRANCH}", "-f", f"per_page={PAGE_SIZE}",
    ]
    return gh_json(argv, f"the CI runs for {sha}", run)


def gh_jobs(repo: str, run_id: int, run: Callable[..., subprocess.CompletedProcess] = subprocess.run) -> dict:
    argv = [
        "gh", "api", "--method", "GET",
        f"repos/{repo}/actions/runs/{run_id}/jobs",
        "-f", "filter=latest", "-f", f"per_page={PAGE_SIZE}",
    ]
    return gh_json(argv, f"the jobs of CI run {run_id}", run)


def matching_runs(payload: dict, sha: str) -> list[dict]:
    runs = payload.get("workflow_runs")
    if not isinstance(runs, list):
        raise LookupFailed(f"The CI run lookup for {sha} returned no run list.")
    return [
        run
        for run in runs
        if isinstance(run, dict)
        and run.get("head_sha") == sha
        and run.get("event") == EVENT
        and run.get("head_branch") == BRANCH
        and run.get("path") == WORKFLOW_PATH
    ]


def describe(run: dict) -> str:
    return f"CI run {run.get('id')} attempt {run.get('run_attempt')}"


def required_job_failures(payload: dict) -> list[str]:
    jobs = payload.get("jobs")
    if not isinstance(jobs, list) or not all(isinstance(job, dict) for job in jobs):
        raise LookupFailed("The CI job lookup returned no job list.")
    total = payload.get("total_count", len(jobs))
    if not isinstance(total, int) or total > len(jobs):
        raise LookupFailed("The CI job lookup returned an incomplete job list.")
    failures = []
    for name in REQUIRED_JOBS:
        found = [job for job in jobs if job.get("name") == name]
        if not found:
            failures.append(f"{name} is missing")
            continue
        for job in found:
            if job.get("status") != "completed" or job.get("conclusion") != "success":
                state = job.get("conclusion") or job.get("status")
                failures.append(f"{name} is {state}")
    return failures


def check_once(fetch_runs: Callable[[], dict], fetch_jobs: Callable[[int], dict], sha: str) -> str | None:
    """Return a success message, None when no run exists yet, "" while CI is unfinished, or raise CiNotGreen."""
    runs = matching_runs(fetch_runs(), sha)
    if not runs:
        return None
    for run in runs:
        if run.get("status") == "completed" and run.get("conclusion") != "success":
            raise CiNotGreen(f"{describe(run)} for {sha} concluded {run.get('conclusion')}. {SAFE_OUTCOME}")
    if not all(run.get("status") == "completed" for run in runs):
        return ""
    for run in runs:
        run_id = run.get("id")
        if not isinstance(run_id, int) or isinstance(run_id, bool):
            raise LookupFailed(f"A CI run for {sha} has no usable id.")
        failures = required_job_failures(fetch_jobs(run_id))
        if failures:
            raise CiNotGreen(
                f"{describe(run)} for {sha} is green but did not run every required job: "
                f"{'; '.join(failures)}. {SAFE_OUTCOME} {NEXT_STEP}"
            )
    names = ", ".join(describe(run) for run in runs)
    return f"{names} for {sha} succeeded with every required job green."


def require_ci_success(
    fetch_runs: Callable[[], dict],
    fetch_jobs: Callable[[int], dict],
    *,
    sha: str,
    timeout_s: float = 90 * 60,
    interval_s: float = 30,
    grace_s: float = 300,
    clock: Callable[[], float] = time.monotonic,
    sleep: Callable[[float], None] = time.sleep,
) -> str:
    if not COMMIT_RE.fullmatch(sha):
        raise CiNotGreen(f"The commit to check is not a full SHA. {SAFE_OUTCOME}")
    start = clock()
    lookup_failures = 0
    found = False
    while True:
        try:
            outcome = check_once(fetch_runs, fetch_jobs, sha)
            lookup_failures = 0
        except LookupFailed as error:
            lookup_failures += 1
            if lookup_failures >= MAX_CONSECUTIVE_LOOKUP_FAILURES:
                raise CiNotGreen(
                    f"{error} {lookup_failures} consecutive lookups failed. {SAFE_OUTCOME}"
                ) from error
            outcome = False
        if outcome:
            return outcome
        found = found or outcome == ""
        elapsed = clock() - start
        if not found and elapsed >= grace_s:
            raise CiNotGreen(
                f"No push run of {WORKFLOW_PATH} on {BRANCH} for {sha} appeared within "
                f"{int(grace_s)} seconds. {SAFE_OUTCOME}"
            )
        if elapsed >= timeout_s:
            raise CiNotGreen(
                f"CI for {sha} did not finish within {int(timeout_s // 60)} minutes. {SAFE_OUTCOME}"
            )
        sleep(interval_s)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Wait until CI proved a commit green.")
    parser.add_argument("--sha", required=True)
    parser.add_argument("--repo", default=os.environ.get("GH_REPO", ""))
    parser.add_argument("--timeout-minutes", type=float, default=90)
    parser.add_argument("--interval-seconds", type=float, default=30)
    parser.add_argument("--grace-seconds", type=float, default=300)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        if not args.repo:
            raise CiNotGreen(f"No repository was given for the CI run lookup. {SAFE_OUTCOME}")
        message = require_ci_success(
            lambda: gh_runs(args.repo, args.sha),
            lambda run_id: gh_jobs(args.repo, run_id),
            sha=args.sha,
            timeout_s=args.timeout_minutes * 60,
            interval_s=args.interval_seconds,
            grace_s=args.grace_seconds,
        )
    except CiNotGreen as error:
        print(f"::error::{error}", file=sys.stderr)
        return 1
    print(message)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
