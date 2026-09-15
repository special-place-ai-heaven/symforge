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

- [ ] C0: the ledger oracle rejects vacuous, failing, surviving-mutant, non-additive and partial-release inputs
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs self-test
  EXPECT: LEDGER-ORACLE SELF-TEST GREEN
  EVIDENCE: pending

- [ ] C1: the tree is rustfmt-clean
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs run --label fmt -- cargo fmt --check
  EXPECT: LEDGER-ORACLE RUN GREEN label=fmt;
  EVIDENCE: pending

- [ ] C2: the default feature set type-checks
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs run --label check -- cargo check -j 8
  EXPECT: LEDGER-ORACLE RUN GREEN label=check;
  EVIDENCE: pending

- [ ] C3: every default-feature target is clippy-clean with warnings denied
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs run --label clippy-default -- cargo clippy -j 8 --all-targets -- -D warnings
  EXPECT: LEDGER-ORACLE RUN GREEN label=clippy-default;
  EVIDENCE: pending

- [ ] C4: the default lib, bins and tests suite keeps every v11.2.0 pass and adds no failure
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs suite --baseline docs/research/fathom-embed-ledger-plan-aligned/baselines/suite-default.json -- test -j 8 --no-fail-fast --lib --bins --tests -- --test-threads=1
  EXPECT: /LEDGER-ORACLE SUITE GREEN [^\n]*preexisting_failures=0;/
  EVIDENCE: pending

- [ ] C5: the observed refresh gate bench runs clean in criterion test mode
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs run --label bench-smoke -- cargo bench -j 8 --bench observed_refresh_gate_v1 -- --test
  EXPECT: LEDGER-ORACLE RUN GREEN label=bench-smoke;
  EVIDENCE: pending

- [ ] C6: the release binary builds
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs run --label release-build -- cargo build -j 8 --release
  EXPECT: LEDGER-ORACLE RUN GREEN label=release-build;
  EVIDENCE: pending

- [ ] C7: the engine-only embed lib is clippy-clean with warnings denied
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs run --label clippy-embed -- cargo clippy -j 8 --no-default-features --features embed,__test-internals --lib -- -D warnings
  EXPECT: LEDGER-ORACLE RUN GREEN label=clippy-embed;
  EVIDENCE: pending

- [ ] C8: the embed-only lib test suite keeps every v11.2.0 pass and adds no failure
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs suite --baseline docs/research/fathom-embed-ledger-plan-aligned/baselines/suite-embed-lib.json -- test -j 8 --no-fail-fast --no-default-features --features embed --lib -- --test-threads=1
  EXPECT: /LEDGER-ORACLE SUITE GREEN [^\n]*preexisting_failures=0;/
  EVIDENCE: pending

- [ ] C9: the embed-only integration contract target keeps every v11.2.0 pass and adds no failure
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs suite --baseline docs/research/fathom-embed-ledger-plan-aligned/baselines/suite-embed-integration.json -- test -j 8 --no-fail-fast --no-default-features --features embed --test embed_bound_index -- --test-threads=1
  EXPECT: /LEDGER-ORACLE SUITE GREEN [^\n]*preexisting_failures=0;/
  EVIDENCE: pending
