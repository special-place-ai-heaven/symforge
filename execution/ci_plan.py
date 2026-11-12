#!/usr/bin/env python
"""Decide where a CI run executes and whether its heavy jobs have anything to test.

The `plan` job runs this first, on a GitHub-hosted runner, so the decision never
depends on the self-hosted pool it is deciding about. It writes three outputs:

- `linux` / `windows`: JSON `runs-on` values (a label list or a hosted image).
- `code_changed`: `false` only when the change is a release version bump or
  documentation no Rust test reads, so the heavy jobs report "skipped".

Every input arrives through the environment, never through the workflow text,
because pull-request fields are attacker-controlled on forks.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import tomllib
import urllib.error
import urllib.request
from typing import Callable, Optional

SELF_HOSTED = {
    "linux": ["self-hosted", "Linux", "X64", "fresh"],
    "windows": ["self-hosted", "Windows", "X64", "windows-host-interim"],
}
HOSTED = {"linux": "ubuntu-latest", "windows": "windows-latest"}
HOSTED_LABEL = "ci-hosted"

# Files release-please rewrites on a version bump. They count as "not code"
# only when the bump is the whole change (see `version_bump_only`).
VERSION_FILES = re.compile(
    r"^(Cargo\.toml|Cargo\.lock|CHANGELOG\.md|\.github/\.release-please-manifest\.json"
    r"|npm/package\.json|npm/platforms/[^/]+/package\.json)$"
)
# README.md and AGENTS.md are read by tests/conformance.rs; specs/020-* and
# everything under tests/ are test inputs. Anything here is code.
CODE_DOCS = {"README.md", "AGENTS.md"}
CODE_PREFIXES = (
    "src/", "tests/", "benches/", "scripts/", "execution/", "npm/",
    ".github/", ".specify/", ".config/", ".cargo/", "specs/020-",
    # Vendored crates embed their own files (vendor/rmcp/src/lib.rs does
    # include_str!("../README.md")).
    "vendor/",
)
# Where a Rust test, the bench, or a harness script could name a document.
REFERENCE_ROOTS = ["src", "tests", "benches", "scripts", "build.rs"]
ZERO_SHA = re.compile(r"^0+$")
DEPENDABOT = "dependabot[bot]"


def replace_version(text: str, old: str, new: str) -> str:
    """Replace `old` only where it is a whole version, so 11.18.10 is not 11.18.1."""
    whole = re.compile(r"(?<![0-9.])" + re.escape(old) + r"(?![0-9]|\.[0-9])")
    return whole.sub(lambda _match: new, text)


def choose_runners(
    event: str,
    head_repo: str,
    repository: str,
    labels: list[str],
    setting: str,
    online: dict[str, Optional[bool]],
    actor: str = "",
    head_ref: str = "",
    ref_name: str = "",
) -> dict[str, tuple[object, str]]:
    """Return `{os: (runs_on, reason)}` for linux and windows."""
    setting = setting.strip().lower()
    chosen: dict[str, tuple[object, str]] = {}
    for os_name in ("linux", "windows"):
        if event.startswith("pull_request") and head_repo != repository:
            reason = "pull request from another repository: untrusted code never runs on self-hosted runners"
        # head_ref is empty outside pull requests, so a dispatch or push on a
        # Dependabot branch is recognized by the ref name.
        elif actor == DEPENDABOT or head_ref.startswith("dependabot/") or ref_name.startswith("dependabot/"):
            reason = "Dependabot change: an updated dependency's build script never runs on self-hosted runners"
        elif HOSTED_LABEL in labels:
            reason = f"pull request carries the {HOSTED_LABEL} label"
        elif setting in ("hosted", f"hosted-{os_name}"):
            reason = f"repository variable CI_RUNNERS={setting}"
        elif os_name == "windows" and event == "schedule":
            reason = "unattended scheduled run: the self-hosted Windows runner is the owner's workstation"
        elif online.get(os_name) is False:
            reason = "the runners API lists no free self-hosted runner with these labels"
        # One Windows runner serves the whole organization. A job sent to it
        # while it is busy waits with no time limit, so without proof that it
        # is free the hosted image is the slot that is always available.
        elif os_name == "windows" and not online.get(os_name):
            reason = "the single self-hosted Windows runner is not known to be free (runner status not checked)"
        else:
            reason = (
                "self-hosted runner free"
                if online.get(os_name)
                else "default for trusted events (runner status not checked)"
            )
            chosen[os_name] = (SELF_HOSTED[os_name], reason)
            continue
        chosen[os_name] = (HOSTED[os_name], reason)
    return chosen


def package_version(text: Optional[str]) -> Optional[str]:
    try:
        version = tomllib.loads(text or "").get("package", {}).get("version")
    except tomllib.TOMLDecodeError:
        return None
    return version if isinstance(version, str) else None


def is_unread_doc(path: str, referenced: Callable[[str], bool]) -> bool:
    return (
        path.endswith(".md")
        and path not in CODE_DOCS
        and not path.startswith(CODE_PREFIXES)
        and not referenced(path)
    )


def classify(
    files: list[str],
    read_old: Callable[[str], Optional[str]],
    read_new: Callable[[str], Optional[str]],
    referenced: Callable[[str], bool],
) -> tuple[bool, str]:
    """Return `(code_changed, reason)`. When unsure, the answer is code."""
    if not files:
        return True, "no changed files were found, so everything runs"
    version_files = [f for f in files if VERSION_FILES.match(f)]
    code = [f for f in files if f not in version_files and not is_unread_doc(f, referenced)]
    if code:
        return True, f"{len(code)} code path(s) changed, first: {code[0]}"
    bumped = [f for f in version_files if f != "CHANGELOG.md"]
    if not bumped:
        return False, "only documentation that no test reads changed"
    old = package_version(read_old("Cargo.toml"))
    new = package_version(read_new("Cargo.toml"))
    if not old or not new or old == new:
        return True, "version files changed without a package version bump"
    for path in bumped:
        before, after = read_old(path), read_new(path)
        if before is None or after is None or replace_version(before, old, new) != after:
            return True, f"{path} changed beyond the {old} -> {new} version bump"
    return False, f"only the release version moved ({old} -> {new}), plus documentation"


def git(*args: str) -> str:
    return subprocess.run(["git", *args], check=True, capture_output=True, text=True).stdout


def have_commit(sha: str) -> bool:
    for attempt in ("local", "fetch"):
        if attempt == "fetch":
            subprocess.run(["git", "fetch", "--no-tags", "--quiet", "origin", sha], capture_output=True)
        probe = subprocess.run(["git", "cat-file", "-e", f"{sha}^{{commit}}"], capture_output=True)
        if probe.returncode == 0:
            return True
    return False


def diff_range(env: dict[str, str]) -> Optional[tuple[str, str]]:
    event = env.get("EVENT_NAME", "")
    if event == "pull_request":
        base, head = env.get("BASE_SHA", ""), env.get("HEAD_SHA", "")
    elif event == "push":
        base, head = env.get("BEFORE_SHA", ""), env.get("GITHUB_SHA", "")
    else:
        return None
    if not base or not head or ZERO_SHA.match(base) or not (have_commit(base) and have_commit(head)):
        return None
    if event == "pull_request":
        base = git("merge-base", base, head).strip()
    return base, head


def changed_code(env: dict[str, str]) -> tuple[bool, str]:
    span = diff_range(env)
    if span is None:
        return True, f"{env.get('EVENT_NAME', 'this')} event has no comparable range, so everything runs"
    base, head = span
    files = [f for f in git("diff", "--name-only", "--no-renames", base, head).splitlines() if f]

    def reader(rev: str) -> Callable[[str], Optional[str]]:
        def read(path: str) -> Optional[str]:
            shown = subprocess.run(["git", "show", f"{rev}:{path}"], capture_output=True, text=True)
            return shown.stdout if shown.returncode == 0 else None

        return read

    def referenced(path: str) -> bool:
        found = subprocess.run(
            ["git", "grep", "-q", "-F", "-e", path, head, "--", *REFERENCE_ROOTS], capture_output=True
        )
        return found.returncode == 0

    return classify(files, reader(base), reader(head), referenced)


def api_get(url: str, token: str) -> object:
    request = urllib.request.Request(
        url,
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "User-Agent": "symforge-ci-plan",
        },
    )
    with urllib.request.urlopen(request, timeout=20) as response:
        return json.load(response)


def runner_status(env: dict[str, str]) -> tuple[dict[str, Optional[bool]], str]:
    token = env.get("RUNNER_STATUS_TOKEN", "")
    if not token:
        return {}, "CI_RUNNER_STATUS_TOKEN is not set, so runner status was not checked"
    runners: list[dict] = []
    failures = []
    for scope in (f"orgs/{env.get('REPOSITORY_OWNER', '')}", f"repos/{env.get('REPOSITORY', '')}"):
        try:
            page = 1
            while True:
                body = api_get(f"https://api.github.com/{scope}/actions/runners?per_page=100&page={page}", token)
                batch = body.get("runners", []) if isinstance(body, dict) else []
                runners.extend(batch)
                if len(batch) < 100:
                    break
                page += 1
        except (urllib.error.URLError, OSError, ValueError) as error:
            failures.append(f"{scope}: {getattr(error, 'code', '')} {getattr(error, 'reason', error)}")
    if len(failures) == 2:
        return {}, "runner status lookup failed (" + "; ".join(failures) + "), so it was not used"
    online: dict[str, Optional[bool]] = {}
    for os_name, wanted in SELF_HOSTED.items():
        need = {label.lower() for label in wanted}
        online[os_name] = any(
            runner.get("status") == "online"
            and not runner.get("busy")
            and need <= {label.get("name", "").lower() for label in runner.get("labels", [])}
            for runner in runners
        )
    return online, f"runners API checked ({len(runners)} self-hosted runners visible)"


def pull_request_labels(env: dict[str, str]) -> list[str]:
    """Current labels, so a re-run after adding ci-hosted sees it; the payload is frozen."""
    # Outside pull requests the workflow renders the label list as `null`.
    fallback = json.loads(env.get("PR_LABELS") or "[]") or []
    number, token = env.get("PR_NUMBER", ""), env.get("GITHUB_TOKEN", "")
    if env.get("EVENT_NAME") != "pull_request" or not number or not token:
        return fallback
    try:
        body = api_get(f"https://api.github.com/repos/{env.get('REPOSITORY', '')}/issues/{number}/labels", token)
        return [label.get("name", "") for label in body] if isinstance(body, list) else fallback
    except (urllib.error.URLError, OSError, ValueError):
        return fallback


def main() -> int:
    env = dict(os.environ)
    online, status_note = runner_status(env)
    runners = choose_runners(
        env.get("EVENT_NAME", ""),
        env.get("HEAD_REPO", ""),
        env.get("REPOSITORY", ""),
        pull_request_labels(env),
        env.get("CI_RUNNERS", ""),
        online,
        env.get("ACTOR", ""),
        env.get("HEAD_REF", ""),
        env.get("REF_NAME", ""),
    )
    code_changed, code_reason = changed_code(env)
    outputs = {
        "linux": json.dumps(runners["linux"][0]),
        "windows": json.dumps(runners["windows"][0]),
        "code_changed": "true" if code_changed else "false",
    }
    with open(env["GITHUB_OUTPUT"], "a", encoding="utf-8") as handle:
        for key, value in outputs.items():
            handle.write(f"{key}={value}\n")
    summary = [
        "### CI plan",
        "",
        "| Decision | Value | Reason |",
        "| --- | --- | --- |",
        f"| Linux runners | `{outputs['linux']}` | {runners['linux'][1]} |",
        f"| Windows runner | `{outputs['windows']}` | {runners['windows'][1]} |",
        f"| Heavy jobs | {'run' if code_changed else 'skipped'} | {code_reason} |",
        "",
        f"Runner status: {status_note}.",
    ]
    text = "\n".join(summary) + "\n"
    print(text)
    if env.get("GITHUB_STEP_SUMMARY"):
        with open(env["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as handle:
            handle.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
