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

const { spawnSync } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");

const repositoryRoot = path.resolve(__dirname, "..");
const ARTIFACT = path.join("target", "ci", "lifecycle-v11", "slice-0-oracle-contract.json");
const CARGO = process.env.SYMFORGE_LIFECYCLE_CARGO_EXECUTABLE || "cargo";
const GIT = process.env.SYMFORGE_LIFECYCLE_GIT_EXECUTABLE || "git";

// Every Slice 0 control, with the exact command that runs it. Expected outcome
// is RED for all of them until the named slice removes the attribute.
const SUITES = [
  {
    target: "tests/project_index_lifecycle_slice0.rs",
    args: ["test", "--test", "project_index_lifecycle_slice0", "--", "--ignored", "--test-threads=1"],
  },
  {
    target: "src/watcher/mod.rs::tests",
    args: [
      "test",
      "--lib",
      "watcher::tests::generation_before_root_split_cannot_authorize_root_a_reindex_into_root_b",
      "--",
      "--exact",
      "--ignored",
    ],
  },
  {
    target: "src/daemon.rs::tests",
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

const MAX_REASON_BYTES = 512;

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

function runSuite(suite) {
  const result = spawnSync(CARGO, suite.args, {
    cwd: repositoryRoot,
    encoding: "utf8",
    shell: false,
    maxBuffer: 64 * 1024 * 1024,
  });
  const output = `${result.stdout || ""}${result.stderr || ""}`;
  const cases = [];
  for (const match of output.matchAll(/^test ([A-Za-z0-9_:]+) \.\.\. (ok|FAILED|ignored)$/gmu)) {
    const [, caseName, outcome] = match;
    cases.push({
      case: caseName,
      target: suite.target,
      command: `${CARGO} ${suite.args.join(" ")}`,
      expected: "red",
      observed: outcome === "FAILED" ? "failed" : outcome === "ok" ? "passed" : "ignored",
      reason: outcome === "FAILED" ? reasonFor(output, caseName) : null,
    });
  }
  if (cases.length === 0) {
    throw new Error(`no cases parsed from: ${suite.args.join(" ")}\n${output.slice(-2000)}`);
  }
  return cases;
}

const cases = SUITES.flatMap(runSuite).sort((left, right) => left.case.localeCompare(right.case));
const unexpected = cases.filter((entry) => entry.observed !== "failed");

const artifact = {
  kind: "symforge.lifecycle.v11.slice0_oracle_contract",
  schema_version: 1,
  slice: 0,
  release_commit: git("rev-parse", "--verify", "HEAD"),
  release_tree: git("rev-parse", "--verify", "HEAD^{tree}"),
  case_count: cases.length,
  cases,
  status: unexpected.length === 0 ? "expected_failures_preserved" : "unexpected_outcome",
};

fs.mkdirSync(path.join(repositoryRoot, path.dirname(ARTIFACT)), { recursive: true });
fs.writeFileSync(
  path.join(repositoryRoot, ARTIFACT),
  `${JSON.stringify(artifact, null, 2)}\n`,
  "utf8",
);

if (unexpected.length > 0) {
  for (const entry of unexpected) {
    process.stderr.write(
      `ERROR SLICE0_ORACLE_UNEXPECTED: ${entry.case} observed ${entry.observed}, expected red\n`,
    );
  }
  process.stderr.write(
    "A Slice 0 positive control that stops failing is either a fix landed without its " +
      "owning slice removing #[ignore], or a control gone vacuous. Both need review.\n",
  );
  process.exitCode = 1;
} else {
  process.stdout.write(
    `slice 0 oracle contract: ${cases.length} expected failures preserved -> ${ARTIFACT}\n`,
  );
}
