# Gates: SymForge binding verification cells (plan-aligned ledger)

Scope: the verification commands SymForge's CLAUDE.md makes binding, the embed lib cell, and the embed integration cell the plan's session added to CI, all pass at the current HEAD without losing any test that passed at v11.2.0

This is a shared helper ledger, not a tree node. Every code leaf reverifies it
through one nested gate. Run it only through Terminal Commander `run_and_watch`
from the repository root:

    node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-check.mjs --root . --cwd . --timeout 10800 --reverify docs/research/fathom-embed-ledger-plan-aligned/gates/cells.md

Deviations from the literal CLAUDE.md commands, none of which changes what is
tested: `-j 8` caps build parallelism (CLAUDE.md, Windows commit-limit note);
`--no-fail-fast` lets every test binary run so a suite is compared with its
baseline by test name. The embed clippy line is CI's exact `embed-build` form.
C9 is the `embed_bound_index` step the plan's session added to
`.github/workflows/ci.yml`. The three baselines are recorded once at v11.2.0
(b549aa7) in a detached temporary worktree, by the commands in PROMPT.md step 3.

- [x] C0: the ledger oracle rejects vacuous, failing, surviving-mutant, non-additive and partial-release inputs
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs self-test
  EXPECT: LEDGER-ORACLE SELF-TEST GREEN
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=bb5b8fdc332f13cfaa8aaf81be9b382d4a2fbaae1cd238f11de448f2e06089f1; output-bytes=41

- [x] C1: the tree is rustfmt-clean
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs run --label fmt -- cargo fmt --check
  EXPECT: LEDGER-ORACLE RUN GREEN label=fmt;
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=cdd5e3bee7692ff2744e883d94775d504e833095a9f8196f2f7ee4b2f059dbb8; output-bytes=35

- [x] C2: the default feature set type-checks
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs run --label check -- cargo check -j 8
  EXPECT: LEDGER-ORACLE RUN GREEN label=check;
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=6582f2b3e2185ed03de7818091db315e99f8873d37ff94d5a8bb22b103a58b6f; output-bytes=37

- [x] C3: every default-feature target is clippy-clean with warnings denied
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs run --label clippy-default -- cargo clippy -j 8 --all-targets -- -D warnings
  EXPECT: LEDGER-ORACLE RUN GREEN label=clippy-default;
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=a0e1b84c5b640a9eff9fcf3a86877f97490a865a55426993d4c237331adc3ea8; output-bytes=46

- [x] C4: the default lib, bins and tests suite keeps every v11.2.0 pass and adds no failure
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs suite --baseline docs/research/fathom-embed-ledger-plan-aligned/baselines/suite-default.json -- test -j 8 --no-fail-fast --lib --bins --tests -- --test-threads=1
  EXPECT: /LEDGER-ORACLE SUITE GREEN [^\n]*preexisting_failures=0;/
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=d0327a803d3aa5fd92a3f422e478ae35a52e34a83b6f3636a4b328687024c18d; output-bytes=83

- [x] C5: the observed refresh gate bench runs clean in criterion test mode
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs run --label bench-smoke -- cargo bench -j 8 --bench observed_refresh_gate_v1 -- --test
  EXPECT: LEDGER-ORACLE RUN GREEN label=bench-smoke;
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=ca9bbf9ce946271f567a20d20e2eee5189edfb5c658b17d61ef23363de002b6a; output-bytes=43

- [x] C6: the release binary builds
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs run --label release-build -- cargo build -j 8 --release
  EXPECT: LEDGER-ORACLE RUN GREEN label=release-build;
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=aec15e7c87fd2a42713ebf0caa6ad564dce25010416c97b41e547135033424a1; output-bytes=45

- [x] C7: the engine-only embed lib is clippy-clean with warnings denied
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs run --label clippy-embed -- cargo clippy -j 8 --no-default-features --features embed,__test-internals --lib -- -D warnings
  EXPECT: LEDGER-ORACLE RUN GREEN label=clippy-embed;
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=e17b1848c6b47315dfa953e30db80bc5988dfeb1447e27207b19a9bc48323335; output-bytes=44

- [x] C8: the embed-only lib test suite keeps every v11.2.0 pass and adds no failure
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs suite --baseline docs/research/fathom-embed-ledger-plan-aligned/baselines/suite-embed-lib.json -- test -j 8 --no-fail-fast --no-default-features --features embed --lib -- --test-threads=1
  EXPECT: /LEDGER-ORACLE SUITE GREEN [^\n]*preexisting_failures=0;/
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=f9fd4078a521656d239cf57a044d7f7b582bd4d4685e2c3c1d3141cb3b5b62ec; output-bytes=83

- [x] C9: the embed-only integration contract target keeps every v11.2.0 pass and adds no failure
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs suite --baseline docs/research/fathom-embed-ledger-plan-aligned/baselines/suite-embed-integration.json -- test -j 8 --no-fail-fast --no-default-features --features embed --test embed_bound_index -- --test-threads=1
  EXPECT: /LEDGER-ORACLE SUITE GREEN [^\n]*preexisting_failures=0;/
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=11e4be14d3b9a7360c763a6507ea42c31ccaa4a9fb5790f9cbd967fa2c5cbe52; output-bytes=78
