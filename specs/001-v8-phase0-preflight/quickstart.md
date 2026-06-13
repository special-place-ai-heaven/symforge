# Quickstart: Validate Phase 0 12A Planning Artifacts

This guide validates the planning package and describes the end-to-end evidence checks expected during implementation. It does not unlock `src/stel/`; only the final independently signed GO decision can do that.

## 1. Confirm active feature

From the repository root:

```powershell
& '.specify\scripts\powershell\check-prerequisites.ps1' -Json -PathsOnly
```

Expected:

- `FEATURE_DIR` points to `specs\001-v8-phase0-preflight`.
- `FEATURE_SPEC` points to `specs\001-v8-phase0-preflight\spec.md`.
- `IMPL_PLAN` points to `specs\001-v8-phase0-preflight\plan.md`.

## 2. Check planning artifacts exist

```powershell
Test-Path 'specs\001-v8-phase0-preflight\plan.md'
Test-Path 'specs\001-v8-phase0-preflight\research.md'
Test-Path 'specs\001-v8-phase0-preflight\data-model.md'
Test-Path 'specs\001-v8-phase0-preflight\contracts\preflight-evidence-contract.md'
Test-Path 'specs\001-v8-phase0-preflight\quickstart.md'
```

Expected: all commands return `True`.

## 3. Check for unresolved planning placeholders

```powershell
rg -n "NEEDS CLARIFICATION|\[FEATURE\]|\[###|ACTION REQUIRED|REMOVE IF UNUSED" specs\001-v8-phase0-preflight -g '!quickstart.md' -g '!**/checklists/**'
```

Expected: no matches.

## 4. Validate the evidence contract before implementation tasks

Review:

- [data-model.md](./data-model.md)
- [contracts/preflight-evidence-contract.md](./contracts/preflight-evidence-contract.md)
- [research.md](./research.md)

Expected:

- Readiness decision requires independent reviewer sign-off.
- Phase 1-blocking OPEN assumptions force NO-GO.
- 7.x results are informational only.
- No `src/stel/` implementation is required or allowed by this planning package.

## 5. Implementation-phase evidence checks

When `/speckit-tasks` produces implementation tasks, those tasks should make the following checks runnable with concrete artifact paths:

```powershell
# Branch binary shakedown and normal repository checks, when source changes exist
cargo check
cargo test --all-targets -- --test-threads=1
cargo build --release

# Gate comparator pre-flight, with concrete paths supplied by the task implementation
node <sf-bench>\compare-results.js --preflight --release 8.0 --baseline <fixture-or-shakedown.json> --candidate <fixture-or-shakedown.json>
```

Expected:

- Gate comparator emits H1 through H8 fields.
- Required row classifications exist for all measured rows.
- Paired measurement runs show accepted-session net variance no greater than 2%.
- Manual baseline spot checks pass 6 of 6 rows.
- Equivalence audit over 20 stratified samples shows combined false positives and false negatives no greater than 10%.
- Golden route corpus contains exactly 36 valid rows with at least 10 reviewed semantic expectations.
- Schema measurement satisfies the 5,000-byte public surface budget and 1,500-byte edit budget, or records an accepted pivot.
- Bypass evidence uses two-hop completion or explicitly scopes H3 to accepted serve rows.

## 6. Final readiness review

Before the first `src/stel/` commit:

1. Confirm every section 12A item has accepted evidence or a binding-doc exemption.
2. Confirm `docs/stel-assumptions.md` has verdicts and artifact links for every Phase 1-blocking assumption.
3. Confirm the final evidence summary maps every completed item to a checklist item or assumption ID.
4. Run a timed reviewer dry-run and confirm the independent reviewer can reach and record GO or NO-GO within 15 minutes.
5. Confirm an independent reviewer signs the evidence bundle.

Expected final outcome:

- GO: all evidence accepted, no blockers, independent reviewer sign-off recorded.
- NO-GO: any missing evidence, unresolved OPEN assumption, contradiction, failed threshold, or missing independent sign-off.
