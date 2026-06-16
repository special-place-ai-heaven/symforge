# 005 Harness Onboarding — Validation (2026-06-16)

Verified on `review/v8-004-operator-serve` (worktree `E:\project\symforge-review`), commits `33731dd` (impl) + `38e19ff` (verification).

## Gates
- `cargo fmt --check` — GREEN
- `cargo check` — GREEN (4m29s cold)
- `cargo clippy --all-targets -- -D warnings` — GREEN (0 warnings)
- Targeted feature tests (`--test-threads=1`):
  - `harness_scan` — **11/11** (NotInstalled / Absent / PresentCurrent / PresentStale / Malformed-reported)
  - `harness_apply_backup` — GREEN (dry-run no-op; timestamped backup + byte-exact restore; idempotent re-apply; malformed/perm-denied reported, never corrupts/aborts)
  - `onboarding_state` — **5/5** (fresh shows + records; same-version suppressed; version-change re-surfaces; state under SymForge data dir; corrupt state = never-shown)
- Full-suite regression: deferred this session (CPU contention from the parallel 007 cold-rebuild). Additive feature; re-run `cargo test --all-targets -- --test-threads=1` clean at the next checkpoint.

## Success-criteria mapping
- **SC-001** hands-free attach → `harness_scan` + `harness_apply_backup` (add/refresh entry into fixture configs, no hand-editing).
- **SC-002** restorable backup → `harness_apply_backup` byte-exact restore.
- **SC-003** idempotent / no-dup → `harness_apply_backup` second-apply no-op.
- **SC-004** dry-run no writes → `harness_apply_backup` plan-only.
- **SC-005** malformed/inaccessible never corrupts/aborts → `harness_scan` malformed-reported + apply error handling.
- **SC-006** banner once per version → `onboarding_state` suppression + version re-surface.

## Implemented surface
- `src/cli/harness.rs` (HarnessRegistry, HarnessTarget, scan, JSON/TOML/Gemini attach-entry add/refresh/de-dup), `src/cli/harness_apply.rs` (plan/apply/backup/restore), `src/cli/onboarding.rs` (OnboardingState + banner), `src/cli/init.rs` + `mod.rs` + `main.rs` (`init --scan [--apply --serve-url --serve-key]`).
- Fixtures: `tests/fixtures/harness/{claude_populated,claude_empty,claude_stale_symforge,codex,gemini_settings,malformed,bom_utf8}`.

## Residual
- T015 (mark G-040/G-041 closed in `docs/v8-gap-closure-plan.md`) and T016 (operator doc) — pending; tracked, not blocking the verified feature.
- Live end-to-end (`init --scan` against a running `serve` writing a real harness config) validated at fixture + unit level, not against a live external client install.
