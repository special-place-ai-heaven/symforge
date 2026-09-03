# REVIEW-FINDINGS — readiness docs live-truth — 2026-09-03

**Spectrum:** Phase 7 (Documentation live-truth)
**Baseline:** `main` @ `6188c5af`
**Instruments:** Stash inspection (`stash@{0}`), spec ledger parser, documentation claim verification.

---

## 7.1 Outstanding-work & backlog documents status

Inspected `docs/OUTSTANDING-WORK.md`, `docs/backlog.md`, and git stash `stash@{0}` (`wip-feat-030-before-clean-pull-2026-09-02`):

1. **`docs/OUTSTANDING-WORK.md` (July 2026):**
   - **Label:** **PROVEN STALE / FULLY SUPERSEDED**
   - **Evidence:** Stash `stash@{0}` contained an unstaged deletion of this entire file (117 lines). The document describes pre-Feature 020 items ("Phase 0 MCP battery", "rmcp 1.x upgrade") which have since shipped (Spec 025 upgraded `rmcp` to `3.1.4`, Feature 020 landed in v11).
   - **Recommendation:** Archive to `docs/archive/2026-07-outstanding-work.md` and remove from active `docs/`.
2. **`docs/backlog.md` (July 2026):**
   - **Label:** **PROVEN STALE / RESOLVED**
   - **Evidence:** Stash `stash@{0}` staged deletion of this file (142 lines). The primary warning — *"release-please race: release PR not opening after merge+branch-delete"* — has been resolved upstream, with PR #673 cleanly cutting v11.1.0.
   - **Recommendation:** Archive to `docs/archive/2026-07-backlog.md`.

---

## 7.2 `tasks/todo.md` append-only log

- **Current state:** 178 KB append-only file (2,031 lines).
- **Finding (PROVEN):**
  - The file is not a current task checklist; it is an unindexed historical session log spanning from early v8 architecture reviews through the 2026-08-24 MCP stress tests.
  - Stash `stash@{0}` attempted to delete all 2,031 lines.
- **Remediation recommendation (Owner Decision Required):**
  - Do NOT edit or delete in this diagnosis campaign.
  - Recommend moving to `docs/archive/2026-08-tasks-todo-history.md` and replacing `tasks/todo.md` with a concise pointer to active Spec Kit specifications (`specs/NNN-*/tasks.md`).

---

## 7.3 Spec ledger drift

A systematic audit across all 32 specification directories in `specs/` reveals massive checkbox drift:

### 1. Specs completely lacking `tasks.md` (8 specs)
The following directories contain design/plan documents but no tracking tasks file:
- `specs/012-harness-agnostic-mcp`
- `specs/019-sfbench-surface-correctness`
- `specs/022-hook-stale-descriptor-scan`
- `specs/023-raw-read-admission-gate`
- `specs/024-optimization-backlog`
- `specs/026-serve-snapshot-restore`
- `specs/027-answer-identity-disclosure`
- `specs/029-mechanical-removal`

### 2. Specs with unchecked task checkboxes (15 specs, 510 total unchecked boxes)
Even though features shipped in releases v11.0.x and v11.1.0, their spec ledgers were never reconciled:
- `specs/020-repository-knowledge-index`: **140 unchecked**, 264 checked
- `specs/015-cbm-capability-ports`: **106 unchecked**, 43 checked
- `specs/021-admission-coverage-honesty`: **75 unchecked**, 0 checked (all tasks implemented, 0 checked)
- `specs/016-perl-parser-hardening`: **60 unchecked**, 0 checked
- `specs/013-stel-predictor-calibration`: **53 unchecked**, 0 checked
- `specs/011-ccr-output-compression`: **41 unchecked**, 0 checked
- `specs/003-81-index-recall`: **25 unchecked**, 10 checked
- `specs/008-v8-aap-panel`: **21 unchecked**
- `specs/005-v8-harness-onboarding`: **17 unchecked**
- `specs/009-operator-setup-wizard`: **10 unchecked**, 20 checked
- `specs/004-v8-operator-serve`: **8 unchecked**, 27 checked
- `specs/010-v8-trust-remediation`: **6 unchecked**, 42 checked
- `specs/002-v8-phase2-stel-controller`: **1 unchecked**, 29 checked
- `specs/018-dogfood-surface-hardening`: **1 unchecked**, 28 checked
- `specs/025-rmcp-3-migration`: **1 unchecked**, 29 checked

**Risk:** Unchecked task ledgers create the false impression that delivered features are unfinished, while hiding genuinely deferred work.

---

## 7.4 Documentation claims vs Empirical reality

1. **Token Economy Claims:**
   - **Stated Claim:** *"90% token savings with SymForge"* (in marketing / overview materials).
   - **Empirical Measurement (Phase 4.4):**
     - **Schema Overhead:** In `SYMFORGE_SURFACE=compact` mode, tool schema size drops from 85.5 kB (39 tools) to 4.7 kB (3 tools), achieving a **94.4% schema token reduction** (saving ~21,261 tokens per prompt turn).
     - **Session Context Savings:** In actual code-editing sessions, token savings average **24.8%** across paired runs, because file content and reasoning tokens still dominate prompt turns.
   - **Recommendation:** Align documentation claims to state: *"94% prompt schema reduction in compact mode; 25% net session token reduction on average agent workflows"*.
2. **Daemon Status Command:**
   - Documentation often instructs operators to run `symforge status`.
   - As verified, `status` is exclusively an MCP tool (`status`), not a CLI subcommand. The CLI binary only supports `analytics`, `init`, `daemon`, `serve`, `setup`, `admin`, `hook`, `trust`, and `update`.
