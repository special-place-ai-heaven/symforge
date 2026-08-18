<!--
Sync Impact Report
- Version change: (unfilled template) → 1.0.0 (initial ratification)
- Modified principles: none prior — all six principles newly ratified:
  I. Reporting Invariant; II. RED-First Evidence; III. Frozen Contracts Win;
  IV. Verification Gates; V. Unrepresentable Over Checked;
  VI. Independent Review Before Merge
- Added sections: Build & Verification Constraints; Review & Delivery Workflow
- Removed sections: none (template placeholders replaced)
- Deferred items: none — every placeholder filled; no TODOs
- Source: codifies rules already enforced by repo CLAUDE.md, the darkness
  seal (tests/preventive_runtime_dark_v11.rs), the CI gate set, and the
  Feature 020 campaign discipline. This document records enforced practice;
  it invents nothing.
-->

# SymForge Constitution

## Core Principles

### I. Reporting Invariant

A component MUST NOT report success for an operation whose completion it did
not observe — not "attempted", not "usually works", not "the code path was
called": observed. Error variants that nothing can emit are forbidden; an
unconstructible refusal is the reporting defect in miniature, because it reads
as a check that does not exist. When adding any status line, banner,
envelope, or success return, the change MUST answer: what did this observe,
and what does it emit when the observation fails?

Rationale: SymForge's product is being trustworthy about what it knows. A
wrong answer is recoverable; a confidently wrong answer is not. Six defects
fixed on 2026-08-06 were this one defect wearing different clothes, and every
one shipped green.

### II. RED-First Evidence

Every acceptance behavior MUST be pinned by an oracle observed failing before
its machinery exists, with the failure receipted (test output or job id).
Every negative test MUST carry its accepting positive control in the same
test — a system that refuses everything satisfies a lone negative perfectly.
Review-found defects are fixed RED-first too: the new oracle is observed
failing against the defective code before the fix lands.

Rationale: a test never seen red proves only that it compiles. The positive
control is what separates an enforced rule from a vacuous one.

### III. Frozen Contracts Win

Immutable spec trees (currently `specs/020-repository-knowledge-index/`,
including checkbox bytes) MUST NOT be edited. Execution specs derive from
frozen trees and are amended when they diverge — the frozen tree is always
the authority. Contract identifiers — test names, file paths, frozen
constants — MUST be quoted exactly; renames require amending the contract
that pins them, never the other way around.

Rationale: two sources of truth drift; a frozen tree plus derived execution
documents cannot.

### IV. Verification Gates

Before any success claim, ALL of the following MUST be observed green:
`cargo fmt --check`; `cargo clippy --all-targets -- -D warnings`; the full
serial test suite (`--test-threads=1`); the embed feature gate
(`cargo test --no-default-features --features embed --lib`); the release
build plus the tool-correctness harness (`verify-tools.cjs`); and the npm
suite. Long cargo runs (anything that can exceed ten minutes) MUST go
through Terminal Commander, one cargo invocation at a time.

Rationale: the embed gate catches cfg mistakes every default-feature gate
misses; the Bash tool's ten-minute ceiling kills builds mid-write and
corrupts `target/`; each rule here was purchased with a real incident.

### V. Unrepresentable Over Checked

Prefer making invalid states unspellable in the type system to validating
them at runtime: private constructors, non-Clone move-consumed tokens,
payloads living inside the enum variant that legitimizes them, exhaustive
destructuring in oracles so new fields cannot slip past review. Out-of-band
recomputation of sealed values (source-set pins, digests) is forbidden — the
Rust oracle that owns the seal is the only recompute authority, and
`rustfmt` runs BEFORE any pin refresh.

Rationale: a checked invariant can be skipped by the next caller; an
unrepresentable one cannot. The seal-recompute rule exists because a
hand-rolled recompute silently diverged from the oracle once already.

### VI. Independent Review Before Merge

Every PR MUST receive at least one independent adversarial review before
merge, and that review MUST include a cfg-lens sweep: never-executed
cfg-gated bodies are unverified claims until their first real executor
(Linux CI) runs them. Every finding is either fixed RED-first or explicitly
adjudicated with recorded rationale — silently dropping a finding is
forbidden. Irreversible or activation-grade changes additionally require
multi-round adversarial review and explicit operator approval to merge.

Rationale: five review rounds missed two cfg(unix) defects in Slice 3
because no round owned the cfg lens; the sweep is now structural, not
optional diligence.

## Build & Verification Constraints

- Serial cargo discipline: one cargo process at a time, `-j 4`, tests with
  `--test-threads=1`; interleaving feature sets in one target directory is
  forbidden (it corrupts incremental state without any kill).
- Heavy local sessions end with `cargo clean` so debug artifacts do not
  accumulate on the working drive.
- Volatile facts (SHAs, branch lists, open PR state, current version) are
  never hand-written into documents; they are generated
  (`scripts/campaign-state.ps1`) or cited by the command that produces them.
- Squash merges MUST pass an explicit conventional `--subject` and a safe
  one-paragraph `--body` (no parentheses, no colon-bearing prose lines):
  gh's default squash body has made merged commits invisible to
  release-please.

## Review & Delivery Workflow

- Dark/behavior-neutral work lands incrementally as one PR per RED+machinery
  pair; each landing extends the darkness seal and census and keeps CI fully
  green. Behavior-changing cuts ship as one PR and pause for explicit
  operator approval.
- Documentation read by tests is code: before restructuring any doc, grep
  `tests/` for phrases it pins.
- Evidence lives under `docs/reviews/`; durable decisions and lessons go to
  agentmemory with the `[symforge]` content prefix.

## Governance

This constitution records enforced practice — every rule above is backed by
an executing test, CI gate, hook, or documented incident, and the document
invents nothing. Amendments MUST update the enforcing test, gate, or hook in
the same change that amends the text; a principle whose enforcement was
removed is removed or rewritten, not left aspirational. Compliance review is
the plan-phase Constitution Check of every speckit feature plus the
per-merge independent review (Principle VI). Versioning follows semantic
rules: MAJOR for principle removals or redefinitions, MINOR for new or
materially expanded principles, PATCH for clarifications.

**Version**: 1.0.0 | **Ratified**: 2026-08-18 | **Last Amended**: 2026-08-18
