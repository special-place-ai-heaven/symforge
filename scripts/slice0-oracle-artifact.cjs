#!/usr/bin/env node
"use strict";

// Produce the Slice 0 CI artifact the traceability contract names `CI-SLICE0`:
// `target/ci/lifecycle-v11/slice-0-oracle-contract.json`.
//
// Slice 0's product is positive controls that MUST fail. That makes "the suite
// is green" the wrong success signal and an unbounded log the wrong evidence, so
// this records one deterministic JSON record per case — the contract's
// BOUND-ARTIFACT: "one deterministic JSON record per case ... no unbounded logs
// or repository bytes".
//
// Fail-closed in the direction that matters: a control which STOPS failing is an
// error here, not a success. Either the defect it names was fixed without its
// owning slice removing the `#[ignore]`, or the control has gone vacuous. Both
// need a human to look, so the exit code is non-zero and the artifact says which
// case changed.
//
// The slice that DOES fix a control's defect reclassifies it here as RESOLVED
// rather than deleting it. A resolved control keeps running — in the default
// suite, without `#[ignore]` — and this producer then asserts it PASSES. So the
// artifact records the RED->GREEN transition, which is the fixing slice's actual
// product, instead of going quiet about a case that used to be evidence; and a
// resolved control that regresses (fails again, or is re-`#[ignore]`d back into
// silence) is caught by the same roster check that catches a lost RED one.

const { spawnSync } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");

const repositoryRoot = path.resolve(__dirname, "..");
const ARTIFACT = path.join("target", "ci", "lifecycle-v11", "slice-0-oracle-contract.json");
const CARGO = process.env.SYMFORGE_LIFECYCLE_CARGO_EXECUTABLE || "cargo";
const GIT = process.env.SYMFORGE_LIFECYCLE_GIT_EXECUTABLE || "git";

// Every Slice 0 control, with the exact command that runs it. `expected` is the
// outcome of the SUITE: "red" while its controls still carry `#[ignore]` and must
// fail, "green" once the owning slice fixed the defect and the control moved into
// the default suite (which is why a resolved suite must NOT pass `--ignored` —
// with the attribute gone it would select nothing and exit 0 having run nothing).
const SUITES = [
  {
    target: "tests/project_index_lifecycle_slice0.rs",
    expected: "red",
    args: ["test", "--test", "project_index_lifecycle_slice0", "--", "--ignored", "--test-threads=1"],
  },
  {
    target: "src/watcher/mod.rs::tests",
    expected: "green",
    args: [
      "test",
      "--lib",
      "watcher::tests::generation_before_root_split_cannot_authorize_root_a_reindex_into_root_b",
      "--",
      "--exact",
    ],
  },
  {
    target: "src/daemon.rs::tests",
    expected: "red",
    args: [
      "test",
      "--lib",
      "daemon::tests::concurrent_first_open_performs_exactly_one_cold_load",
      "--",
      "--exact",
      "--ignored",
    ],
  },
];

const NEWLINE = String.fromCharCode(10);
const MAX_REASON_BYTES = 512;

// The exact Slice 0 roster. Without it the producer only checks that the cases it
// HAPPENS to parse are still failing, so deleting or renaming a control leaves the
// rest red and the artifact still reports success -- claimed for a roster it never
// measured. Reclassifying a control is a deliberate act belonging to the slice that
// fixes its defect, so that slice must edit these lists too.
//
// Still RED: the defect each names is unfixed, the test carries `#[ignore]`, and it
// MUST fail.
const RED_CASES = [
  "capacity_refused_open_creates_no_slot_and_no_watcher",
  "configured_capacity_bounds_the_process_not_each_load",
  "daemon::tests::concurrent_first_open_performs_exactly_one_cold_load",
  "empty_placeholder_publication_refuses_watcher_mutation",
  "failed_reload_retains_the_recovery_observer",
  "observer_replacement_gap_is_latched_as_non_current",
  "old_observer_delivery_after_promotion_is_not_current",
  "same_path_root_replacement_is_not_silently_adopted",
  "snapshot_seed_is_not_queryable_before_verification",
  "watcher_mutation_during_candidate_build_is_not_discarded",
  "whole_project_publication_preserves_latest_siblings",
];

// RESOLVED: the named slice fixed the defect, the `#[ignore]` is gone, and the
// control MUST now pass. It stays on the roster and stays run — dropping it would
// delete the regression guard and leave this artifact silent about the transition
// it exists to evidence.
const RESOLVED_CASES = new Map([
  [
    "watcher::tests::generation_before_root_split_cannot_authorize_root_a_reindex_into_root_b",
    {
      slice: 1,
      tasks: ["T028"],
      defect: "2.8 root and generation authority can split",
      // Not the commit reordering the oracle's prose predicted: the fence takes
      // both values from one `Arc<PublishedGeneration>`, so the generation and
      // the root that publication served cannot disagree.
      fix: "src/watcher/mod.rs::effective_fence_generation reads one published generation",
    },
  ],
]);

const EXPECTED_CASES = [...RED_CASES, ...RESOLVED_CASES.keys()];

/// The outcome this producer asserts for a case: "green" once its owning slice
/// resolved it, "red" while it is still a positive control.
function expectationFor(caseName) {
  return RESOLVED_CASES.has(caseName) ? "green" : "red";
}

const OBSERVED_FOR_EXPECTED = { red: "failed", green: "passed" };

function git(...args) {
  const result = spawnSync(GIT, args, { cwd: repositoryRoot, encoding: "utf8", shell: false });
  if (result.status !== 0) throw new Error(`git ${args.join(" ")} failed`);
  return result.stdout.trim();
}

/// First assertion line for a case, bounded. Never the whole log.
function reasonFor(output, caseName) {
  const marker = `---- ${caseName} stdout ----`;
  const start = output.indexOf(marker);
  if (start === -1) return null;
  const lines = output.slice(start + marker.length).split("\n");
  for (const line of lines) {
    const text = line.trim();
    if (!text || text.startsWith("thread '") || text.startsWith("note:")) continue;
    if (text.startsWith("----")) break;
    return text.length > MAX_REASON_BYTES ? `${text.slice(0, MAX_REASON_BYTES)}…` : text;
  }
  return null;
}

// A test binary that aborts mid-suite (double panic, OOM, `abort`) still prints
// the results it reached, so parsing alone would yield a silent subset that every
// remaining case "preserves". And the notify-thread leak this slice fixed is
// exactly a binary that never exits, which without a timeout burns the runner's
// whole budget instead of failing with evidence. Both are checked here.
const SUITE_TIMEOUT_MS = 30 * 60 * 1000;

function runSuite(suite) {
  const result = spawnSync(CARGO, suite.args, {
    cwd: repositoryRoot,
    encoding: "utf8",
    shell: false,
    maxBuffer: 64 * 1024 * 1024,
    timeout: SUITE_TIMEOUT_MS,
  });
  const output = `${result.stdout || ""}${result.stderr || ""}`;
  if (result.error && result.error.code === "ETIMEDOUT") {
    throw new Error(`timed out after ${SUITE_TIMEOUT_MS} ms: ${suite.args.join(" ")}`);
  }
  if (result.error) {
    throw new Error(`could not run: ${suite.args.join(" ")}: ${result.error.message}`);
  }
  if (result.signal) {
    throw new Error(`killed by ${result.signal}: ${suite.args.join(" ")}`);
  }
  // A RED suite must exit non-zero. Exit 0 means either nothing ran or its
  // controls stopped failing; the roster check below names which, but the run
  // itself is already not what this artifact assumes.
  if (suite.expected === "red" && result.status === 0) {
    throw new Error(
      `exited 0 with no failing case, so nothing was preserved: ${suite.args.join(" ")}`,
    );
  }
  // A resolved suite exiting non-zero is NOT thrown on: that is a regression in a
  // control this producer is here to report, so it is parsed, recorded with its
  // bounded reason line, and failed by the per-case check. A non-zero exit that
  // parsed no case at all -- a build failure -- still throws below.

  // libtest prints `ignored, {reason}` for `#[ignore = "..."]`, which every Slice 0
  // control uses, so the outcome is NOT always the end of the line. Anchoring there
  // made a silenced control parse as no case at all: re-`#[ignore]`ing a resolved
  // control failed closed (good) with "no cases parsed" (wrong cause — that error
  // means a build failure). The optional suffix keeps the observation honest.
  const cases = [];
  for (const match of output.matchAll(
    /^test ([A-Za-z0-9_:]+) \.\.\. (ok|FAILED|ignored)(?:, [^\n]*)?$/gmu,
  )) {
    const [, caseName, outcome] = match;
    cases.push({
      case: caseName,
      target: suite.target,
      command: `${CARGO} ${suite.args.join(" ")}`,
      expected: expectationFor(caseName),
      observed: outcome === "FAILED" ? "failed" : outcome === "ok" ? "passed" : "ignored",
      reason: outcome === "FAILED" ? reasonFor(output, caseName) : null,
      resolved_by: RESOLVED_CASES.get(caseName) ?? null,
    });
  }
  if (cases.length === 0) {
    throw new Error(`no cases parsed from: ${suite.args.join(" ")}\n${output.slice(-2000)}`);
  }
  return cases;
}

const cases = SUITES.flatMap(runSuite).sort((left, right) => left.case.localeCompare(right.case));
const unexpected = cases.filter(
  (entry) => entry.observed !== OBSERVED_FOR_EXPECTED[entry.expected],
);
const observedNames = cases.map((entry) => entry.case).sort();
const expectedNames = [...EXPECTED_CASES].sort();
const missing = expectedNames.filter((name) => !observedNames.includes(name));
const extra = observedNames.filter((name) => !expectedNames.includes(name));

const artifact = {
  kind: "symforge.lifecycle.v11.slice0_oracle_contract",
  // 2: `expected` is per case and may be "green" for a control its owning slice
  // resolved, which every v1 reader assumed impossible; cases carry `resolved_by`.
  schema_version: 2,
  slice: 0,
  release_commit: git("rev-parse", "--verify", "HEAD"),
  release_tree: git("rev-parse", "--verify", "HEAD^{tree}"),
  case_count: cases.length,
  expected_case_count: EXPECTED_CASES.length,
  red_case_count: RED_CASES.length,
  resolved_case_count: RESOLVED_CASES.size,
  missing_cases: missing,
  unexpected_cases: extra,
  cases,
  status:
    unexpected.length === 0 && missing.length === 0 && extra.length === 0
      ? "expected_outcomes_preserved"
      : "unexpected_outcome",
};

fs.mkdirSync(path.join(repositoryRoot, path.dirname(ARTIFACT)), { recursive: true });
fs.writeFileSync(
  path.join(repositoryRoot, ARTIFACT),
  `${JSON.stringify(artifact, null, 2)}\n`,
  "utf8",
);

for (const name of missing) {
  process.stderr.write(`ERROR SLICE0_ORACLE_MISSING: ${name} was not run` + NEWLINE);
}
for (const name of extra) {
  process.stderr.write(`ERROR SLICE0_ORACLE_EXTRA: ${name} is not in the expected roster` + NEWLINE);
}
if (unexpected.length > 0 || missing.length > 0 || extra.length > 0) {
  for (const entry of unexpected) {
    process.stderr.write(
      `ERROR SLICE0_ORACLE_UNEXPECTED: ${entry.case} observed ${entry.observed}, expected ${entry.expected}\n`,
    );
  }
  process.stderr.write(
    "A Slice 0 positive control that stops failing is either a fix landed without its " +
      "owning slice reclassifying it here, or a control gone vacuous. A resolved control "
      + "that stops passing is a regression of the fix that resolved it, and one observed "
      + "`ignored` has been silenced by a re-added attribute. A control that disappears "
      + "from the roster is a lost positive control. All need review.\n",
  );
  process.exitCode = 1;
} else {
  process.stdout.write(
    `slice 0 oracle contract: ${cases.length} controls preserved `
      + `(${RED_CASES.length} red, ${RESOLVED_CASES.size} resolved-green) -> ${ARTIFACT}\n`,
  );
}
