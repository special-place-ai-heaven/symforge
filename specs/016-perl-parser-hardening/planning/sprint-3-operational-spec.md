# Sprint S3 — Operational Hardening

**Feature**: 016 · **US**: US3, US4

## Objective

Ship grammar bump protocol, finalize investigation doc, clean stale HANDOFF reference.

## Grammar bump dry-run

Execute [quickstart.md § Grammar bump](../quickstart.md#grammar-bump-checklist-maintainers) without changing version — confirm steps are complete and ordered.

Record dry-run in decision-log if CI hook deferred (D-016-008 follow-up).

## Investigation doc final structure

Mirror `docs/dart-parser-investigation.md`:

1. Executive summary
2. Empirical evidence (fixture corpus metrics)
3. SymForge context (dispatch map, three surfaces)
4. Failure classes
5. Final answers (what works, accepted losses, bump protocol)

## Planning Gate checklist

- [ ] P-S3-001 dry-run complete
- [ ] P-S3-002 CI decision recorded (R-016-003)
- [ ] P-S3-003 HANDOFF edit scoped

## Release Gate checklist

- [ ] Full Release Gate green
- [ ] SC-005 bump checklist validated
- [ ] SC-006 converge zero tasks
- [ ] PR ready for main

## Program closure

When V-S3-003 merges: update spec.md Status → **Complete**; archive sprint gate sign-offs in acceptance-matrix.
