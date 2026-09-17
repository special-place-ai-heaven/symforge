# Gates: leaf 1.1, plan Task 1, align the release baseline

OWNS: docs/research/fathom-embed-ledger-plan-aligned/baselines/*.json

Scope: the working branch `feature/fathom-embed-contract` is built on v11.2.0 (b549aa7), that revision is the cited worker-backed source at version 11.2.0, the verification suites were measured green at exactly that revision, and nothing outside the plan's tasks rides along

Plan: docs/plans/2026-09-15-fathom-embed-alignment.md, Task 1. At drafting time
(2026-09-15) the branch's HEAD was b549aa7 with uncommitted work for Tasks 2
and 3 already present, so the "aligned source" moment of Task 1 Step 3 is
measured on a detached temporary worktree at b549aa7 (PROMPT.md step 3) rather
than on the branch.

- [x] G0: this ledger states outcomes that can fail
  CHECK: node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-lint.mjs docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.1.md
  EXPECT: LINT OK
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=f2a2bf958a5727fc73f6d4c5860d14eb16b34a66a01f98f6102021ecc3780e77; output-bytes=384

- [x] G1: the checked-out branch descends from v11.2.0 commit b549aa7
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs run --label descends-from-v11.2.0 -- git merge-base --is-ancestor b549aa7db59dcc10aff3f3493f12ed6509f2e2e1 HEAD
  EXPECT: LEDGER-ORACLE RUN GREEN label=descends-from-v11.2.0;
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=0ca74e1562a94ca95e6b1f6c61c66459effe6fc847c94cd191eaaf39510deb28; output-bytes=53

- [x] G2: the local tag v11.2.0 names commit b549aa7
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs run --label tag-is-b549aa7 --require b549aa7db59dcc10aff3f3493f12ed6509f2e2e1 -- git rev-list -n 1 v11.2.0
  EXPECT: LEDGER-ORACLE RUN GREEN label=tag-is-b549aa7;
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=ce95e84e5dbb9c2da6edae97143aefb0f139af72842d8b3b8f6789e4c9ff90c1; output-bytes=46

- [x] G3: the Cargo.toml at b549aa7 declares version 11.2.0
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs run --label base-version --require "version = " --require 11.2.0 -- git show b549aa7:Cargo.toml
  EXPECT: LEDGER-ORACLE RUN GREEN label=base-version;
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=b610ea581345b7507d7669c7df621c682adf628e3daa4ba2345bffaaff1757aa; output-bytes=44

- [x] G4: the embedded.rs at b549aa7 is the worker-backed shape the brief cites
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs run --label worker-backed-embedded --require symforge-embed- --require reload_and_publish --require wait_refresh_visibility_or_stop -- git show b549aa7:src/index_lifecycle/embedded.rs
  EXPECT: LEDGER-ORACLE RUN GREEN label=worker-backed-embedded;
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=157e4521ae64507a2f3aa32babec63280a5de4e65c47f5ff29d795ae2da34745; output-bytes=54

- [x] G5: the default suite baseline was recorded at b549aa7 with no failing test
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs baseline-head --baseline docs/research/fathom-embed-ledger-plan-aligned/baselines/suite-default.json --expect b549aa7
  EXPECT: /LEDGER-ORACLE BASELINE GREEN at=b549aa7 passed=\d+ failed=0;/
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=d9779f9bb22317a384e604a13d6c6bdb01a7ad2c135158b6190d230020a0f537; output-bytes=62

- [x] G6: the embed lib suite baseline, which is plan Task 1's embed gate on the aligned source, was recorded at b549aa7 with no failing test
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs baseline-head --baseline docs/research/fathom-embed-ledger-plan-aligned/baselines/suite-embed-lib.json --expect b549aa7
  EXPECT: /LEDGER-ORACLE BASELINE GREEN at=b549aa7 passed=\d+ failed=0;/
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=af203be1969c3db0959dc7a0150990cdf54ec3e3e60f6d5a8d4fdc9fd761ca19; output-bytes=62

- [x] G7: the embed integration target baseline was recorded at b549aa7 with no failing test
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs baseline-head --baseline docs/research/fathom-embed-ledger-plan-aligned/baselines/suite-embed-integration.json --expect b549aa7
  EXPECT: /LEDGER-ORACLE BASELINE GREEN at=b549aa7 passed=\d+ failed=0;/
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=d7410165a46aa7ebd2ed3c38241723a6b124df4020f6d1ef857cac3151fa8cd6; output-bytes=59

- [x] G8: every path changed since the release baseline belongs to a named C3 row, with the reason recorded
  RECORD: the output of `git diff --stat v11.2.0` mapped path by path. Owner revision 2: `.github/workflows/ci.yml` and `tests/preventive_runtime_dark_v11.rs` belong to Task 3 Step 5 (embed integration CI step plus its workflow fingerprint and cargo-line pins). `src/protocol/tools.rs`, `src/protocol/edit_tools.rs`, `tests/conformance.rs` and `src/stel/surface_list.rs` are the named 200-character harness companion: keep them, isolate as their own commit, do not treat as unlabeled extras. Owner revision 3 (2026-09-16): #695 SnapshotStore restore and #701 lock-only version bumps of already-present crates are named C3 companions. Any other unlisted path is a defect.
  EVIDENCE: 2026-09-16 from `feature/fathom-embed-wave2` after cells ALL MET. `git diff --name-only v11.2.0` remapped; every path is a named C3 row or a Task 1–7 owned path. Owner named #695 and #701 as C3. Inspect api GREEN; atoms GREEN added=9. Deps CHECK lists the five #701 --allow-lock rows.
    C3 / Task 3 Step 5: .github/workflows/ci.yml; tests/preventive_runtime_dark_v11.rs
    C3 / 200-char companion: src/protocol/tools.rs; src/protocol/edit_tools.rs; tests/conformance.rs; src/stel/surface_list.rs
    C3 / #695 SnapshotStore restore: src/index_lifecycle/snapshot.rs; src/live_index/persist.rs
    C3 / #701 lock-only bumps: Cargo.lock (reqwest@0.13.5, rmcp@3.3.0, rmcp-macros@3.3.0, toml_edit@0.25.15+spec-1.1.0, toml_parser@1.1.3+spec-1.1.0)
    C3 / cell-fix companions: tests/claim_provenance_v11.rs (cartesian 9*4*4 after IndexCensus+SearchKnowledge); tests/rmcp3_protocol.rs (rmcp 3.3 initialize of 2026-07-28 falls back to 2025-11-25)
    Task 1 (plan, brief, ledger, baselines): docs/plans/2026-09-15-fathom-embed-alignment.md; docs/research/fathom-embed-change-brief.md; docs/research/fathom-embed-ledger-plan-aligned/{GATES.md,PLAN.md,PROMPT.md,PROMPT-TO-FATHOM.md,PROMPT-TO-SYMFORGE.md,oracles/ledger-oracle.mjs,gates/cells.md,gates/leaf-1.1.md,gates/leaf-1.2.md,gates/leaf-1.3.md,gates/leaf-1.4.md,gates/leaf-1.5.md,gates/leaf-1.6.md,gates/leaf-1.7.md,baselines/suite-default.json,baselines/suite-embed-lib.json,baselines/suite-embed-integration.json}
    Tasks 2–6 (CR-0/1/2/3, F-1/F-2, atoms, mutation patches): CHANGELOG.md; src/daemon.rs; src/discovery/mod.rs; src/embed.rs; src/index_lifecycle/{activation.rs,embedded.rs,process_runtime.rs,public_api.rs,registry.rs}; src/lifecycle_identity.rs; src/live_index/{knowledge_retrieve.rs,mod.rs,search.rs,store.rs}; src/protocol/{ccr.rs,claim_provenance.rs,format.rs,format/tests.rs,knowledge_search.rs}; specs/020-repository-knowledge-index/{REFREEZE-MANIFEST-v11.md,contracts/public-api-v11.json}; docs/reviews/{FEATURE-020-EXPORT-DELTA-v11.json,FEATURE-020-REFREEZE-ATTESTATION-v11.md}; execution/refreeze_v11.py; tests/{activation_cut_v11.rs,embed_bound_index.rs,public_api_delta_v11.rs,fixtures/public-api-v11-consumer/dependent-positive/src/lib.rs,fixtures/public-api-v11-consumer/fixture-manifest.json}; docs/research/fathom-embed-ledger-plan-aligned/mutations/{task2-m1..m6,task3-m0..m7,task4-m1..m6,task5-m1..m4,task6-m1..m12}.patch
    Task 7 handoff (not yet the tag): tasks/todo.md; tasks/lessons.md
