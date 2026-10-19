# Gates: 033-containment-durability (campaign)

OWNS: specs/033-containment-durability/**, docs/reviews/**, src/**, tests/**

Scope: Close F-001 … F-009 from docs/reviews/agent-review-2026-09-18.md with a full speckit loop, worktree-isolated implementers, RED-first oracles, adversarial review, and cleanup.

- [x] G-SPEC: Specification and requirements checklist exist and name all nine kept findings as independently testable stories
  CHECK: powershell -NoProfile -Command "if ((Test-Path specs/033-containment-durability/spec.md) -and (Test-Path specs/033-containment-durability/checklists/requirements.md)) { $s = Get-Content -Raw specs/033-containment-durability/spec.md; $ok = $true; foreach ($id in @('F-001','F-002','F-003','F-004','F-005','F-006','F-007','F-008','F-009')) { if ($s -notmatch [regex]::Escape($id)) { $ok = $false } }; if ($ok) { 'spec names F-001..F-009' } else { exit 1 } } else { exit 1 }"
  EXPECT: spec names F-001..F-009
  EVIDENCE: 2026-09-18 CHECK printed spec names F-001..F-009

- [x] G-PLAN: plan.md, research.md, data-model.md, contracts/, and quickstart.md exist under the feature directory
  CHECK: powershell -NoProfile -Command "$d='specs/033-containment-durability'; $need=@('plan.md','research.md','data-model.md','quickstart.md','contracts/no-follow-read.md','contracts/durable-persist.md','contracts/sidecar-bind.md'); $ok=$true; foreach($n in $need){ if (-not (Test-Path (Join-Path $d $n))) { $ok=$false; Write-Output \"missing $n\" } }; if($ok){'plan artifacts present'} else {exit 1}"
  EXPECT: plan artifacts present
  EVIDENCE: 2026-09-18 CHECK printed plan artifacts present

- [x] G-TASKS: tasks.md uses checklist format with T### ids, story labels, and disjoint file ownership notes
  CHECK: powershell -NoProfile -Command "$t=Get-Content -Raw specs/033-containment-durability/tasks.md; if ($t -match '- \[[ xX]\] T001' -and $t -match '\[US1\]' -and $t -match '\[US9\]' -and $t -match 'T067' -and $t -match 'T068') { 'tasks checklist format' } else { exit 1 }"
  EXPECT: tasks checklist format
  EVIDENCE: 2026-09-18 CHECK printed tasks checklist format

- [x] G-F001: Outbound symlink/junction disclosure tests exist and pass
  CHECK: cargo test --lib --bins --tests -- --test-threads=1 outbound_link --nocapture
  EXPECT: test result: ok
  EVIDENCE: 2026-09-18 included in T063 serial suite job_01a0b4ad8e7a7902b11848029777a9fd (outcome_trust=observed, exit 0): test result ok. 3387 passed; 4 ignored. Filter name outbound_link is substring-matched by that run.

- [x] G-F002: Non-loopback sidecar spawn without opt-in fails; loopback still starts
  CHECK: cargo test --lib --bins --tests -- --test-threads=1 sidecar_bind --nocapture
  EXPECT: test result: ok
  EVIDENCE: 2026-09-18 included in T063 job_01a0b4ad8e7a7902b11848029777a9fd (observed exit 0, 3387 passed)

- [x] G-F003: Trust hasher refuses reparse/symlink recursion
  CHECK: cargo test --lib --bins --tests -- --test-threads=1 trust_config_reparse --nocapture
  EXPECT: test result: ok
  EVIDENCE: 2026-09-18 included in T063 job_01a0b4ad8e7a7902b11848029777a9fd (observed exit 0, 3387 passed)

- [x] G-F004: Snapshot/artifact persist syncs before rename
  CHECK: cargo test --lib --bins --tests -- --test-threads=1 snapshot_sync_all --nocapture
  EXPECT: test result: ok
  EVIDENCE: 2026-09-18 included in T063 job_01a0b4ad8e7a7902b11848029777a9fd (observed exit 0, 3387 passed). Ordering spy snapshot_sync_all_is_observed_before_rename plus T069 fail oracle snapshot_sync_all_failure_does_not_rename_or_report_success: job_01a0b4d342137f4387c88d770d34d973 outcome_trust=observed exit 0, 2 passed.

- [x] G-F005: apply_splice and extract_symbols_for_diff refuse illegal ranges without panic
  CHECK: cargo test --lib --bins --tests -- --test-threads=1 splice_bounds --nocapture
  EXPECT: test result: ok
  EVIDENCE: 2026-09-18 included in T063 job_01a0b4ad8e7a7902b11848029777a9fd (observed exit 0, 3387 passed)

- [x] G-F006: Windows token DACL is owner-only (cfg-gated)
  CHECK: cargo test --lib --bins --tests -- --test-threads=1 daemon_token_acl --nocapture
  EXPECT: test result: ok
  EVIDENCE: 2026-09-18 included in T063 job_01a0b4ad8e7a7902b11848029777a9fd (observed exit 0, 3387 passed)

- [x] G-F007: Trust store persist is atomic
  CHECK: cargo test --lib --bins --tests -- --test-threads=1 trust_store_atomic --nocapture
  EXPECT: test result: ok
  EVIDENCE: 2026-09-18 included in T063 job_01a0b4ad8e7a7902b11848029777a9fd (observed exit 0, 3387 passed)

- [x] G-F008: Init client-config writers are atomic
  CHECK: cargo test --lib --bins --tests -- --test-threads=1 init_atomic_write --nocapture
  EXPECT: test result: ok
  EVIDENCE: 2026-09-18 included in T063 job_01a0b4ad8e7a7902b11848029777a9fd (observed exit 0, 3387 passed)

- [x] G-F009: Parent index_lifecycle docs no longer claim production darkness; Feature 020 T051 still passes
  CHECK: powershell -NoProfile -Command "$p='src/index_lifecycle/mod.rs'; $t=Get-Content -Raw $p; if ($t -match 'Nothing in production calls this module') { exit 1 } else { 'parent darkness claim gone' }"
  EXPECT: parent darkness claim gone
  EVIDENCE: 2026-09-18 rg on src/index_lifecycle/mod.rs: no match. Phrase remains only on candidate.rs, supervisor.rs, verification.rs. T051 ran inside T063 job_01a0b4ad8e7a7902b11848029777a9fd (observed exit 0).

- [x] G-FMT: cargo fmt --check
  CHECK: cargo fmt --check
  EXPECT:
  EVIDENCE: 2026-09-18 job_01a0b4c8e1da7381aa15f25df76ec139 outcome_trust=observed exit 0 (3489 ms)

- [x] G-CLIPPY: clippy all-targets deny warnings
  CHECK: cargo clippy --all-targets -- -D warnings
  EXPECT:
  EVIDENCE: 2026-09-18 job_01a0b4a1212d7aa28c50e608fce5ae0a outcome_trust=observed exit 0 (~56s) with constitution `-j 4`

- [x] G-TEST: full serial suite
  CHECK: cargo test --lib --bins --tests -- --test-threads=1
  EXPECT: test result: ok
  EVIDENCE: 2026-09-18 job_01a0b4ad8e7a7902b11848029777a9fd outcome_trust=observed exit 0 (~956s): lib 3387 passed; 4 ignored. Integration including activation_residuals, preventive_runtime_dark_v11, worktree_awareness ok. PATH included Git and WindowsPowerShell.

- [x] G-EMBED: embed feature gate
  CHECK: cargo test --no-default-features --features embed --lib -- --test-threads=1
  EXPECT: test result: ok
  EVIDENCE: 2026-09-18 job_01a0b4bdb84d717385fb6198ca2a1c2d outcome_trust=observed exit 0: test result ok. 1375 passed; 0 failed; 4 ignored; finished in 78.00s. Ran after T063, not interleaved.

- [x] G-RELEASE: release build plus tool-correctness harness
  CHECK: cargo build --release -j 4 && node scripts/verify-tools.cjs --bin target/release/symforge
  EXPECT: verify-tools ok
  EVIDENCE: 2026-09-18 cargo build --release -j 4 job_01a0b4c12e637c80ae881b16b7c60055 outcome_trust=observed exit 0, Finished release profile in 6m 04s. Windows binary is target/release/symforge.exe. verify-tools job_01a0b4c8317972a29458eaec23254d79 with rg on PATH: 7 PASS, 1 REVIEW (find_references grep over-match, harness says not a failure), 0 FAIL, exit 0.

- [x] G-NPM: npm suite
  CHECK: npm test --prefix npm
  EXPECT: npm test ok
  EVIDENCE: 2026-09-18 job_01a0b4c872e575529cff7f68e640454d outcome_trust=observed exit 0: pass 31, fail 0, duration_ms 216.

- [x] G-CLEAN: worktrees removed and cargo clean observed after the campaign
  EVIDENCE: 2026-09-18 story worktrees `.worktrees/033-us*` gone (`git worktree list` is checkout plus unrelated `E:/project/symforge-embed-refreeze`). `cargo clean` job_01a0b4d460f27ad1aea500a418792f0d outcome_trust=observed exit 0: Removed 58845 files, 56.5GiB total.
