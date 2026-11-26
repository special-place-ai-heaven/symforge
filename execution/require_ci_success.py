#!/usr/bin/env python
"""Require that CI proved a commit green before a release PR is merged onto it.

A push that cannot publish skips the release verification build. If release-please
then opens a release PR, merging it straight away would put an untagged release
commit on main on top of a commit nothing has verified yet. This waits for every
push run of ci.yml for the exact commit and fails unless each one succeeded. A
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
SAFE_OUTCOME = "The release PR stays open and unmerged."
COMMIT_RE = re.compile(r"[0-9a-f]{40}")


class CiNotGreen(RuntimeError):
    """CI did not prove the commit green."""


def gh_fetch(repo: str, sha: str, run: Callable[..., subprocess.CompletedProcess] = subprocess.run) -> dict:
    argv = [
        "gh", "api", "--method", "GET",
        f"repos/{repo}/actions/workflows/{WORKFLOW_FILE}/runs",
        "-f", f"head_sha={sha}", "-f", f"event={EVENT}", "-f", f"branch={BRANCH}", "-f", "per_page=100",
    ]
    completed = run(argv, stdin=subprocess.DEVNULL, capture_output=True, check=False)
    if completed.returncode != 0:
        raise CiNotGreen(f"Could not read the CI runs for {sha}. {SAFE_OUTCOME}")
    try:
        payload = json.loads(completed.stdout)
    except (json.JSONDecodeError, TypeError, UnicodeDecodeError) as error:
        raise CiNotGreen(f"The CI run lookup for {sha} returned no JSON. {SAFE_OUTCOME}") from error
    if not isinstance(payload, dict):
        raise CiNotGreen(f"The CI run lookup for {sha} returned an unexpected shape. {SAFE_OUTCOME}")
    return payload


def matching_runs(payload: dict, sha: str) -> list[dict]:
    runs = payload.get("workflow_runs")
    if not isinstance(runs, list):
        raise CiNotGreen(f"The CI run lookup for {sha} returned no run list. {SAFE_OUTCOME}")
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


def require_ci_success(
    fetch: Callable[[], dict],
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
    while True:
        runs = matching_runs(fetch(), sha)
        elapsed = clock() - start
        if runs:
            for run in runs:
                if run.get("status") == "completed" and run.get("conclusion") != "success":
                    raise CiNotGreen(
                        f"{describe(run)} for {sha} concluded {run.get('conclusion')}. {SAFE_OUTCOME}"
                    )
            if all(run.get("status") == "completed" for run in runs):
                names = ", ".join(describe(run) for run in runs)
                return f"{names} for {sha} succeeded."
        elif elapsed >= grace_s:
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
            lambda: gh_fetch(args.repo, args.sha),
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
