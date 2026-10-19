# Security Requirements Checklist: Containment and Durability Hardening

**Purpose**: Requirements-quality review of security stories (unit tests for English, not implementation verification)
**Created**: 2026-09-18
**Feature**: [spec.md](../spec.md)

**Review Ownership**: Reviewer-owned. `[x]` means the *requirement* is complete and unambiguous.

## Containment (F-001)

- [x] CHK001 The spec states that outside-workspace bytes must not appear in retrieval, untracked search, sidecar source-read, or the live index
- [x] CHK002 Freshen join-fallback is forbidden, not merely discouraged
- [x] CHK003 Follow vs no-follow is specified for worktree `is_file()`, disk read, watcher stat, and stable-read
- [x] CHK004 A positive control (ordinary in-repo file) is required so total-refusal cannot satisfy the story
- [x] CHK005 Refusal messages must not include outside file contents

## Sidecar bind (F-002)

- [x] CHK006 Non-loopback without explicit opt-in must fail to spawn
- [x] CHK007 Non-loopback with opt-in still requires a token
- [x] CHK008 Loopback must not gain Bearer (hook compatibility)
- [x] CHK009 Sidecar opt-in env is distinct from the daemon's

## Trust hasher (F-003)

- [x] CHK010 Junctions and symlinks under project-config are excluded from the hash input
- [x] CHK011 Evaluation is fail-closed Untrusted when such an entry exists, with a warning

## Durability (F-004, F-007, F-008)

- [x] CHK012 Success is reported only after sync-before-rename
- [x] CHK013 Crash-before-rename leaves the previous parseable file
- [x] CHK014 Source-mutation atomic_write_file is out of scope for control-state JSON

## Panic-free ranges (F-005)

- [x] CHK015 Inverted, OOB, and non-char-boundary ranges are structured errors, not panics
- [x] CHK016 Valid ranges remain successful (positive control)

## Token ACL (F-006)

- [x] CHK017 Windows DACL must exclude Everyone/Users read
- [x] CHK018 Unix 0o600 must not regress

## Docs honesty (F-009)

- [x] CHK019 Parent module docs must not claim production darkness while call edges exist
- [x] CHK020 T051 call-edge rules stay the executing authority; pin refresh is oracle-owned

## Notes

- Implementation verification is tasks.md Phase 12, not this checklist.
- Ready for implementation; no requirement-quality blockers.
