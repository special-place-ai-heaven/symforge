# REVIEW-FINDINGS — readiness / supply-chain — 2026-09-03

**Spectrum:** Phase 6 — Supply chain & release integrity
**Baseline:** `main` @ `6188c5af` (worktree HEAD `1b570f1c`, one doc commit ahead)
**Reviewer:** Phase6SupplyChain lane (read-only; no cargo build/test/clippy invoked)
**Spec:** `docs/plans/2026-09-03-production-readiness-diagnosis.md` §Phase 6

Labels: **proven** = reproduced/measured/read directly; **likely** = strong static evidence; **speculative** = needs a named experiment. All `file:line` citations at rev `6188c5af` unless noted.

## 0. Check-results table (every assigned check, observed outcome)

| # | Check | Command / method | Exit | Wall time | Outcome |
|---|-------|------------------|------|-----------|---------|
| 1a | Refreeze internal gate | `python execution/refreeze_v11.py verify-internal --target-ref HEAD` | 0 | 12.4s | "Feature 020 V11 internal refreeze verification passed." |
| 1b | Traceability norm test | `node scripts/validate-lifecycle-oracle-traceability.norm.test.cjs` | 0 | 0.3s | "retirement census normalizer: OK (37 equivalence cases)" |
| 1c | Traceability self-test | `node scripts/validate-lifecycle-oracle-traceability.test.cjs` | 0 | 248.9s | "lifecycle oracle traceability v11 self-test: OK (103 fail-closed cases)". First attempt hit the 180s call timeout mid-run; suite is slow under concurrent cargo load (see finding 10). |
| 1d | Traceability checker | `node scripts/validate-lifecycle-oracle-traceability.cjs` | 0 | 0.8s | "lifecycle oracle traceability v11: OK (78 requirements, 24 acceptance oracles, 13 retirement categories)" |
| 1e | Version sync gate | `python execution/version_sync.py check` | 0 | 0.6s | "Version check passed: 11.1.0" |
| 1f | Execution unittests | `python -m unittest discover -s execution`; CI-matching rerun with `SYMFORGE_REQUIRE_SSHSIG=1` (`.github/workflows/ci.yml:62-64`) | — | >300s, >900s | **Unverified locally — long-running under cargo contention.** Two interrupted runs (300s harness timeout without env, 900s with CI env), both mid-suite with normal progress (dots + expected negative-path "verification failed" strings from `test_refreeze_v11.py` rejection cases). Partial positive signal: the two pure-python modules pass instantly in isolation — `test_conventional_commits` 13/13 OK (4.6s), `test_task_queue` 60/60 OK (0.2s); the slowness concentrates in the git-fixture modules. Full suite proven green in CI at the v11.1.0 release commit (run 33674721933, success, 2026-09-02T19:40Z). |
| 3a | npm wrapper tests | `npm test` in `npm/` | 0 | 1.4s | 31/31 pass (launcher, av-safe packaging, path-shadow). |
| 3b | npm pack contents | `npm pack --dry-run` in `npm/` | 0 | 6.0s | Exactly 5 files: LICENSE, bin/launcher.js, bin/symforge.js, lib/resolve-binary.js, package.json; 6.8kB. No tgz, no fixtures, no `.symforge/`. |
| 3c | Stray tarball | `git check-ignore`, `git ls-files`, repo-wide grep | — | — | `npm/symforge-4.9.8.tgz` untracked, gitignored (`.gitignore:32`), referenced nowhere but the spec; excluded by `files` whitelist (finding 5). |
| 4a | LF-index census | logic trace + live temp-repo experiment | — | — | **Teeth proven**: committed CRLF blob → `i/crlf` → census exits 1 (finding 7). |
| 4b | rmcp single-major assertion | logic trace + lockfile read | — | — | **Teeth confirmed**: fails unless rmcp+rmcp-macros both present with majors == {"3"}; graph today is 3.1.4/3.1.4 (finding 8). |
| 4c | Runner disk script | logic trace | — | — | Frees disk but has no post-cleanup assertion; can silently no-op if runner image paths change (finding 9). |

## Findings

### 1. [proven, P4-info] Spec expectation stale: latest release is v11.1.0, not v11.0.13
The spec (`docs/plans/2026-09-03-production-readiness-diagnosis.md:95`) says to compare the manifest against tag `v11.0.13`. Reality: latest tag is `v11.1.0` → commit `000bb7ac` ("Merge pull request #673 … release-please"), an ancestor of HEAD (verified via `git merge-base --is-ancestor`). `.github/.release-please-manifest.json:2` = `"." : "11.1.0"`, `Cargo.toml:3` = `11.1.0`, `npm/package.json:3` = `11.1.0`. All four version surfaces agree. Not a product defect — the spec was written before 11.1.0 shipped on 2026-09-02.

### 2. [proven, P3-watch] The documented release-please race is not currently manifesting
The race — merge + branch-delete racing release-please's commit collection so it sees only the merge commit ("Splitting 1 commits … Considering: 0") and opens no release PR — is documented at `docs/backlog.md:28-34` (observed July, run 28822434928, 8.13.0 cycle; self-heals next run). Current GitHub state (verified via `gh`): the most recent feature merge, PR #672 (`feat(032)`, merged 2026-09-02T19:03:47Z), triggered Release run 33671033132 (success), which opened release PR #673 (`chore(main): release 11.1.0`, merged 19:40:25Z) → tag `v11.1.0` → final Release run 33674721933 success. Zero open PRs. The pipeline behaved exactly as designed on the latest cycle; the race remains a documented transient with a re-run mitigation, not a standing failure.

### 3. [proven, P3-hygiene] A release-gate run failed on 2026-08-25 and recovered only via later merges
Run 32901709894 (`fix(release): accept a release PR merged ahead of a queued push run`, PR #667, sha `e541e9a3`) failed in job `verify-release-ref` step "Run Rust tests" (with `gate-release-ref` cascading). That commit's own gate run never went green; recovery happened transitively — `e541e9a3` is an ancestor of HEAD (verified), and every Release run since 2026-09-02 is green, so the fix is validated by later runs. Ironically the failed run was itself a fix for a release-race edge. No action needed; recorded because "the release gate went red on main's history and was carried forward by subsequent green runs" is exactly the pattern that can mask a real break if the intervening merges don't re-exercise the failing surface. (The visible "Feature 020 V11 verification failed: …" lines in that run's log are negative-path unittest output, not the failure cause.)

### 4. [proven, healthy] npm wrapper version matrix is fully coherent
`npm/package.json:3` = 11.1.0; `optionalDependencies` (`npm/package.json:16-21`) pin all four platform packages at exactly 11.1.0. All four `npm/platforms/*/package.json` files read directly: version 11.1.0, correct `os`/`cpu` pairs (linux-x64, darwin-arm64, darwin-x64, win32-x64), `files: ["bin/", "LICENSE"]`. `npm test` = 31/31 pass. `npm pack --dry-run` = 5 files / 6.8kB, no stray content (`.symforge/hook-adoption.log` inside `npm/` is correctly excluded by the `files` whitelist).

### 5. [proven, P4-clutter] Stray `npm/symforge-4.9.8.tgz` is inert — recommend delete (owner decision)
Evidence: untracked (`git ls-files npm/` shows no tgz); ignored by `.gitignore:32` (`npm/*.tgz`); excluded from any future pack by the `files` whitelist (`npm/package.json:22-26`); the release pipeline packs fresh from stamped sources (`.github/workflows/release.yml:2422-2424`); the only repo-wide reference to the filename is the diagnosis spec itself. It cannot be published or shipped. It is 7.2kB of dead weight and a standing "what is this?" — deletion is safe but is the owner's call per campaign rules.

### 6. [likely, P3] `version_sync.py check` does not cover `npm/platforms/*/package.json`
`check_versions` (`execution/version_sync.py:92-120`) verifies manifest ↔ `Cargo.toml` ↔ `npm/package.json` ↔ root `optionalDependencies` pins, plus an optional tag — nothing ever reads the four platform manifests. A mid-cycle manual edit drifting a platform `version` would pass the CI gate (`.github/workflows/ci.yml:91-92`) unnoticed. Mitigations keep this low-severity: release-please bumps all four platform files as `extra-files` (`.github/release-please-config.json`), and `release.yml:2373-2377` restamps platform versions in-workflow immediately before packing, so a wrong in-repo platform version cannot reach the registry through the normal path. Remediation size: ~15 lines in `version_sync.py` + one test.

### 7. [proven, healthy] LF-index census has teeth — CRLF blob verified caught
Gate: `.github/workflows/ci.yml:66-80` — `git ls-files --eol | grep -vE '^i/(lf|-text|none)'`, `exit 1` on any offender. Proven by a live temp-repo experiment (not just reading the logic): committing a CRLF file with `core.autocrlf=false` and no attributes yields `i/crlf` in `git ls-files --eol`, and the exact pipeline catches it ("CAUGHT: i/crlf … crlf.txt"). A binary blob reports `i/-text` and passes, as intended. Defense in depth confirmed: with the repo's `.gitattributes` (`* text=auto eol=lf`, `.gitattributes:9`; plus targeted pins at lines 14-17), `git add --renormalize` converts the CRLF blob to `i/lf`, so the census only needs to fire for attribute-bypassing paths (`-text`, `-crlf` configs, or a force-added blob). The `.gitattributes:1-2` header claim ("1054 text files, 0 crlf/mixed") is the property this gate preserves.

### 8. [proven, healthy + P4 comment drift] rmcp single-major assertion has teeth; its debt comment is stale
Assertion: `.github/workflows/ci.yml:112-130` parses `cargo metadata` and fails unless both `rmcp` and `rmcp-macros` exist and each version-major set equals exactly `{"3"}` (`len(majors) < 2` also fails, catching a silently dropped macros crate). Current graph: rmcp 3.1.4 + rmcp-macros 3.1.4 (`Cargo.lock`) against requirement `rmcp = "3.1"` (`Cargo.toml:88`) — passes. Note it fails closed on a *clean* rmcp 4.x upgrade too (set `{"4"}` ≠ `{"3"}`): deliberate tripwire, but whoever upgrades rmcp must update the assertion in the same PR or CI goes red. Separately, the REVIEW P3-C debt comment at `Cargo.toml:84-87` is stale: it says the requirement is "1.1.0" with the lockfile resolving ">=1.7" (allowed_hosts DNS-rebinding APIs) — describing the 1.x era. The dep is now `"3.1"`; the comment's specifics mislead any reviewer acting on the debt (Phase 5 covers the substance). The residual concern (requirement floor "3.1" vs lockfile 3.1.4; no minimum-version enforcement against downgrade) survives the text drift.

### 9. [likely, P3-latent] `free_runner_disk.sh` frees space but cannot itself fail when it stops working
`execution/free_runner_disk.sh:9-13` hard-deletes four known GitHub-runner paths (dotnet, android, ghc, CodeQL) and prunes docker images. `set -euo pipefail` (line 4) does not trip on already-absent directories (`rm -rf` exits 0), and the docker prune is `|| true` (line 13). There is no post-cleanup free-space assertion — the step succeeds even if it freed zero bytes. If GitHub renames/removes those image paths (they have done so historically), the script becomes a silent no-op and the failure surfaces much later as an opaque out-of-disk error in the Rust build. Callers: `.github/workflows/ci.yml:101`, `.github/workflows/release.yml:248`. Remediation: append a one-line threshold check (e.g. fail if `df --output=avail /` < N GB) — turns a latent no-op into an explicit, diagnosable failure.

### 10. [proven, P4-friction] Local release-gate suites are slow under concurrent cargo load; full execution-unittest result unverified locally
Measured on this machine while the main lane ran cargo builds: the traceability self-test needed 248.9s (a 180s harness timeout killed the first attempt mid-run; green on retry). `python -m unittest discover -s execution` could not complete locally in two attempts — interrupted at 300s (without env) and again at 900s with the CI-matching `SYMFORGE_REQUIRE_SSHSIG=1` (`.github/workflows/ci.yml:62-64`) — both times mid-suite with normal progress markers (dots + expected negative-path "verification failed" strings from `test_refreeze_v11.py` rejection cases). Partial local signal: the two modules that don't build git fixtures pass instantly in isolation (`test_conventional_commits` 13/13 OK in 4.6s; `test_task_queue` 60/60 OK in 0.2s); the wall time concentrates in the git-fixture-heavy modules (`test_refreeze_v11.py` builds a fixture repo per case, e.g. `execution/test_refreeze_v11.py:169-170`). **Label for the local full-suite run: unverified — needs rerun on an idle machine.** CI proves the suite green at the v11.1.0 commit (run 33674721933). No product defect; recorded so the rollup knows the local signal is missing, not red, and so Phase 4 can note the wall times.

## Summary by severity

| Severity | Count | Findings |
|----------|-------|----------|
| P0 | 0 | — |
| P1 | 0 | — |
| P2 | 0 | — |
| P3 | 4 | #2 race watch item, #3 historical red gate run, #6 version_sync platform gap, #9 disk-script no-assertion |
| P4 | 4 | #1 spec drift, #5 stray tgz, #8 rmcp comment drift, #10 local suite friction/unverified |
| Healthy gates confirmed | 4 | #4 npm coherence, #7 LF census teeth (experiment-proven), #8 rmcp assertion teeth, plus gates 1a-1e all exit 0 |

**Top risks:** (1) finding 9 — the disk-cleanup step can silently no-op and fail a release build opaquely; cheapest fix with the best diagnosability payoff. (2) finding 6 — the version-sync gate has a real (though doubly-mitigated) blind spot over platform manifests. (3) finding 2/3 — the release-please race family is documented, transient, and currently quiet, but the 2026-08-25 red-run recovery pattern shows the pipeline's failures heal by absorption; keep the re-run runbook entry in `docs/backlog.md:28-34` alive.
