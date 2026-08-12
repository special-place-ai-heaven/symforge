#!/usr/bin/env node
"use strict";

const childProcess = require("node:child_process");
const crypto = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const repositoryRoot = path.resolve(__dirname, "..");
const checker = path.join(__dirname, "validate-lifecycle-oracle-traceability.cjs");
const { runVerifiedCommand } = require(checker);
const retirementContractPath = "specs/020-repository-knowledge-index/contracts/v10-authority-retirement-v11.md";

function currentRetirementSourcePaths() {
  const text = fs.readFileSync(path.join(repositoryRoot, retirementContractPath), "utf8");
  const start = text.indexOf("<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON START -->");
  const end = text.indexOf("<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON END -->", start);
  const objectStart = text.indexOf("{", start);
  const objectEnd = text.lastIndexOf("}", end);
  const retirement = JSON.parse(text.slice(objectStart, objectEnd + 1));
  return [...new Set(retirement.entries.flatMap((entry) => entry.members)
    .filter((member) => typeof member === "string" && member.startsWith("src/"))
    .map((member) => member.split("::", 1)[0]))].sort();
}

const fixturePaths = [
  "specs/020-repository-knowledge-index/contracts/lifecycle-oracle-traceability-v11.md",
  "specs/020-repository-knowledge-index/contracts/lifecycle-acceptance-oracles-v11.md",
  "specs/020-repository-knowledge-index/contracts/v10-authority-retirement-v11.md",
  "specs/020-repository-knowledge-index/contracts/public-api-v11.json",
  "specs/020-repository-knowledge-index/tasks.md",
  "scripts/validate-lifecycle-oracle-traceability.cjs",
  "execution/refreeze_v11.py",
  ".github/workflows/release.yml",
  "src/discovery/mod.rs",
  "src/domain/index.rs",
  "src/cli/mod.rs",
  "src/embed.rs",
  "src/lib.rs",
  "src/protocol/prompts.rs",
  "src/protocol/resources.rs",
  "src/protocol/surface_probe.rs",
  "src/sidecar/router.rs",
  "tests/admin_api_v1.rs",
  ...currentRetirementSourcePaths(),
];

const tracePath = fixturePaths[0];
const acceptancePath = fixturePaths[1];
const traceStart = "<!-- SYMFORGE LIFECYCLE ORACLE TRACEABILITY V11 JSON START -->";
const traceEnd = "<!-- SYMFORGE LIFECYCLE ORACLE TRACEABILITY V11 JSON END -->";
const acceptanceStart = "<!-- SYMFORGE LIFECYCLE ACCEPTANCE ORACLES V11 JSON START -->";
const acceptanceEnd = "<!-- SYMFORGE LIFECYCLE ACCEPTANCE ORACLES V11 JSON END -->";

function copyFixture(root) {
  for (const relativePath of fixturePaths) {
    const source = path.join(repositoryRoot, relativePath);
    const destination = path.join(root, relativePath);
    fs.mkdirSync(path.dirname(destination), { recursive: true });
    fs.copyFileSync(source, destination);
  }
}

function mutateSentinel(root, relativePath, start, end, mutate) {
  const file = path.join(root, relativePath);
  const text = fs.readFileSync(file, "utf8");
  const startAt = text.indexOf(start);
  const endAt = text.indexOf(end);
  if (startAt === -1 || endAt === -1 || endAt <= startAt) {
    throw new Error(`sentinel missing in ${relativePath}`);
  }
  const bodyStart = startAt + start.length;
  const fenced = text.slice(bodyStart, endAt).trim();
  const match = /^```json\r?\n([\s\S]*?)\r?\n```$/u.exec(fenced);
  if (!match) throw new Error(`json fence missing in ${relativePath}`);
  const value = JSON.parse(match[1]);
  mutate(value);
  const replacement = `${start}\n\`\`\`json\n${JSON.stringify(value, null, 2)}\n\`\`\`\n${end}`;
  fs.writeFileSync(file, `${text.slice(0, startAt)}${replacement}${text.slice(endAt + end.length)}`, "utf8");
}

function replaceSentinelJson(root, relativePath, start, end, json) {
  const file = path.join(root, relativePath);
  const text = fs.readFileSync(file, "utf8");
  const startAt = text.indexOf(start);
  const endAt = text.indexOf(end);
  if (startAt === -1 || endAt === -1 || endAt <= startAt) {
    throw new Error(`sentinel missing in ${relativePath}`);
  }
  const replacement = `${start}\n\`\`\`json\n${json}\n\`\`\`\n${end}`;
  fs.writeFileSync(file, `${text.slice(0, startAt)}${replacement}${text.slice(endAt + end.length)}`, "utf8");
}

function runChecker(root, extraArgs = [], extraEnv = {}) {
  return childProcess.spawnSync(process.execPath, [checker, "--root", root, ...extraArgs], {
    cwd: repositoryRoot,
    env: { ...process.env, ...extraEnv },
    encoding: "utf8",
    windowsHide: true,
  });
}

function safeRemoveFixture(root) {
  const resolvedRoot = path.resolve(root);
  const tempPrefix = `${path.resolve(os.tmpdir())}${path.sep}`;
  if (!resolvedRoot.startsWith(tempPrefix) || !path.basename(resolvedRoot).startsWith("symforge-lifecycle-oracle-")) {
    throw new Error(`refusing to remove unexpected fixture path: ${resolvedRoot}`);
  }
  fs.rmSync(resolvedRoot, { recursive: true, force: true });
}

function safeRemoveFakeCargo(root) {
  const resolvedRoot = path.resolve(root);
  const tempPrefix = `${path.resolve(os.tmpdir())}${path.sep}`;
  if (!resolvedRoot.startsWith(tempPrefix) || !path.basename(resolvedRoot).startsWith("symforge-lifecycle-cargo-")) {
    throw new Error(`refusing to remove unexpected fake-cargo path: ${resolvedRoot}`);
  }
  fs.rmSync(resolvedRoot, { recursive: true, force: true });
}

function readSentinel(root, relativePath, start, end) {
  const text = fs.readFileSync(path.join(root, relativePath), "utf8");
  const startAt = text.indexOf(start);
  const endAt = text.indexOf(end, startAt + start.length);
  const fenced = text.slice(startAt + start.length, endAt).trim();
  const match = /^```json\r?\n([\s\S]*?)\r?\n```$/u.exec(fenced);
  if (!match) throw new Error(`json fence missing in ${relativePath}`);
  return JSON.parse(match[1]);
}

function canonicalJson(value) {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(",")}}`;
}

function sha256(value) {
  return crypto.createHash("sha256").update(value).digest("hex");
}

function domainDigest(domain, value) {
  return crypto.createHash("sha256").update(domain, "utf8").update(Buffer.from([0])).update(canonicalJson(value), "utf8").digest("hex");
}

function writeFixtureFile(root, relativePath, bytes) {
  const file = path.join(root, relativePath);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, bytes);
}

function runGit(root, args) {
  const result = childProcess.spawnSync("git", ["-C", root, ...args], { encoding: "utf8", windowsHide: true });
  if (result.status !== 0) throw new Error(`git ${args[0]} failed`);
  return (result.stdout || "").trim();
}

function resolveProgram(name) {
  const locator = process.platform === "win32"
    ? childProcess.spawnSync("where.exe", [name], { encoding: "utf8", windowsHide: true })
    : childProcess.spawnSync("which", [name], { encoding: "utf8" });
  if (locator.status !== 0) return null;
  const candidate = (locator.stdout || "").split(/\r?\n/u).find((line) => line.trim() !== "");
  if (!candidate) return null;
  try {
    return fs.realpathSync(candidate.trim());
  } catch {
    return null;
  }
}

function fileHash(root, relativePath) {
  return sha256(fs.readFileSync(path.join(root, relativePath)));
}

function createV11SeamStubs(root, trace) {
  const byFile = new Map();
  const acceptance = readSentinel(root, acceptancePath, acceptanceStart, acceptanceEnd);
  const retirement = readSentinel(
    root,
    retirementContractPath,
    "<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON START -->",
    "<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON END -->",
  );
  const seamGroups = [
    ...Object.values(trace.catalogs.seams),
    ...acceptance.oracles.map((oracle) => oracle.production_seams),
    ...retirement.entries.map((entry) => entry.production_seams),
  ];
  for (const anchors of seamGroups) {
    for (const anchor of anchors) {
      const separator = anchor.indexOf("::");
      const file = anchor.slice(0, separator);
      if (!byFile.has(file)) byFile.set(file, []);
      byFile.get(file).push(anchor.slice(separator + 2));
    }
  }
  for (const [file, symbols] of byFile) {
    if (fs.existsSync(path.join(root, file))) continue;
    const functions = new Set();
    const types = new Map();
    for (const symbol of symbols) {
      const parts = symbol.split("::");
      if (parts.length === 1) {
        if (/^[A-Z]/u.test(parts[0])) {
          if (!types.has(parts[0])) types.set(parts[0], new Set());
        } else {
          functions.add(parts[0]);
        }
      } else if (/^[A-Z]/u.test(parts[0])) {
        if (!types.has(parts[0])) types.set(parts[0], new Set());
        types.get(parts[0]).add(parts.at(-1));
      } else {
        functions.add(parts[0]);
        functions.add(parts.at(-1));
      }
    }
    const lines = [];
    for (const [type, members] of [...types].sort(([left], [right]) => left.localeCompare(right))) {
      if (members.size === 0) lines.push(`pub struct ${type};`);
      else {
        lines.push(`pub struct ${type} {`);
        for (const member of [...members].sort()) lines.push(`    pub ${member}: (),`);
        lines.push("}");
      }
    }
    for (const fn of [...functions].sort()) lines.push(`pub fn ${fn}() {}`);
    writeFixtureFile(root, file, `${lines.join("\n")}\n`);
  }
}

function createPostactivationOrdinaryFixture(root) {
  const trace = readSentinel(root, tracePath, traceStart, traceEnd);
  const manifest = JSON.parse(fs.readFileSync(
    path.join(root, "specs/020-repository-knowledge-index/contracts/public-api-v11.json"),
    "utf8",
  ));
  createV11SeamStubs(root, trace);
  const keepAtoms = manifest.migration_v10.categories
    .filter((category) => category.decision === "keep")
    .flatMap((category) => category.atoms);
  const directAtoms = [...new Set([...keepAtoms, ...manifest.migration_v10.introduced_v11_atoms]
    .filter((atom) => atom.split("::").length <= 3))].sort();
  const modules = [...new Set(directAtoms.filter((atom) => atom.split("::").length >= 2)
    .map((atom) => atom.split("::")[1]))].sort();
  writeFixtureFile(root, "src/lib.rs", `${modules.map((module) => `pub mod ${module};`).join("\n")}\n`);
  for (const module of modules) {
    const names = directAtoms.filter((atom) => atom.startsWith(`symforge::${module}::`) && atom.split("::").length === 3)
      .map((atom) => atom.split("::")[2]);
    writeFixtureFile(
      root,
      `src/${module}.rs`,
      `${names.map((name) => /^[A-Z]/u.test(name) ? `pub struct ${name};` : `pub fn ${name}() {}`).join("\n")}\n`,
    );
  }
  const retiredWriter = path.resolve(root, "src/gitignore_hygiene.rs");
  const fixturePrefix = `${path.resolve(root)}${path.sep}`;
  if (!retiredWriter.startsWith(fixturePrefix)) throw new Error("postactivation fixture removal escaped fixture root");
  fs.rmSync(retiredWriter, { force: true });
}

function createPlannedExecutables(root, trace) {
  const testsByFile = new Map();
  for (const test of Object.values(trace.catalogs.tests)) {
    const [file, ...symbols] = test.target.split("::");
    if (test.kind === "planned_benchmark") {
      const caseName = symbols.at(-1);
      writeFixtureFile(
        root,
        file,
        [
          `fn ${caseName}() {}`,
          `criterion_group!(${caseName}_group, ${caseName});`,
          `criterion_main!(${caseName}_group);`,
          "",
        ].join("\n"),
      );
    } else if (test.kind === "planned_exact") {
      if (!testsByFile.has(file)) testsByFile.set(file, []);
      testsByFile.get(file).push(symbols.at(-1));
    }
  }
  for (const [file, names] of testsByFile) {
    writeFixtureFile(
      root,
      file,
      [...new Set(names)].sort().flatMap((name) => ["#[test]", `fn ${name}() {`, "    assert!(true);", "}", ""]).join("\n"),
    );
  }
}

function makeGenericOracleArtifact(oracle, releaseCommit, releaseTree) {
  return {
    kind: "symforge.lifecycle_oracle_execution.v11",
    schema_version: 1,
    release_commit: releaseCommit,
    release_tree: releaseTree,
    oracle_id: oracle.oracle_id,
    category: oracle.category,
    trace_test_id: oracle.trace_test_id,
    test: oracle.test,
    command: oracle.command,
    requirement_ids: oracle.requirement_ids,
    implementation_tasks: oracle.implementation_tasks,
    target_slice: oracle.target_slice,
    production_seams_sha256: sha256(canonicalJson(oracle.production_seams)),
    preconditions_sha256: sha256(canonicalJson(oracle.preconditions)),
    actions_sha256: sha256(canonicalJson(oracle.actions)),
    assertions_sha256: sha256(canonicalJson(oracle.assertions)),
    positive_control_sha256: sha256(canonicalJson(oracle.positive_control)),
    negative_controls_sha256: sha256(canonicalJson(oracle.negative_controls)),
    bounds_sha256: sha256(canonicalJson(oracle.bounds)),
    fairness_sha256: sha256(canonicalJson(oracle.fairness)),
    ci_artifact: oracle.ci_artifact,
    positive_control_result: "passed",
    negative_controls_result: "passed",
    assertions_result: "passed",
    test_result: "passed",
    result: "passed",
    status: "passed",
  };
}

function buildPositiveMaterializedFixture(root) {
  const trace = readSentinel(root, tracePath, traceStart, traceEnd);
  const acceptance = readSentinel(root, acceptancePath, acceptanceStart, acceptanceEnd);
  runGit(root, ["init", "--quiet"]);
  runGit(root, ["config", "user.email", "lifecycle-fixture@example.invalid"]);
  runGit(root, ["config", "user.name", "Lifecycle Fixture"]);
  runGit(root, ["config", "core.autocrlf", "false"]);
  runGit(root, ["add", "--all"]);
  runGit(root, ["commit", "--quiet", "-m", "approved refreeze"]);
  const approvedCommit = runGit(root, ["rev-parse", "HEAD^{commit}"]);
  const approvedTree = runGit(root, ["rev-parse", "HEAD^{tree}"]);

  createV11SeamStubs(root, trace);
  createPlannedExecutables(root, trace);
  writeFixtureFile(root, "tests/model/materialized.rs", "#[test]\nfn materialized_model_receipt() { assert!(true); }\n");
  const commandIds = new Map([
    ["T078", "format-and-clippy"], ["T079", "focused-lifecycle-suites"], ["T080", "model-formal-and-loom"],
    ["T081", "serial-all-target-and-token-gate"], ["T082", "race-and-observer-campaigns"],
    ["T083", "concurrent-project-memory-gate"], ["T084", "provenance-refusal-and-secret-canary"],
    ["T085", "activation-and-restart-campaigns"], ["T086", "public-api-and-cfg-gate"],
    ["T087", "secret-safety-scan"], ["T088", "freeze-and-adversarial-review"],
    ["T089", "refreeze-approval-and-evidence"],
  ]);
  const review = [...commandIds.entries()].filter(([id]) => id !== "T089")
    .map(([id, commandId]) => `<!-- SYMFORGE LIFECYCLE RELEASE TASK ${id}: ${commandId} PASSED -->`).join("\n") + "\n";
  writeFixtureFile(root, "docs/reviews/FEATURE-020-V11-RELEASE-GATE.md", review);
  const fakeCommandScript = [
    '"use strict";',
    'const fs = require("node:fs");',
    'const crypto = require("node:crypto");',
    'const path = require("node:path");',
    'const artifacts = JSON.parse(process.env.SYMFORGE_LIFECYCLE_ARTIFACTS);',
    'const hash = (file) => crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex");',
    'const receipt = {',
    '  kind: "symforge.lifecycle_command_execution.v11", schema_version: 1,',
    '  release_commit: process.env.SYMFORGE_LIFECYCLE_RELEASE_COMMIT,',
    '  release_tree: process.env.SYMFORGE_LIFECYCLE_RELEASE_TREE,',
    '  test_id: process.env.SYMFORGE_LIFECYCLE_TEST_ID, target: process.env.SYMFORGE_LIFECYCLE_TARGET,',
    '  command: process.env.SYMFORGE_LIFECYCLE_COMMAND,',
    '  artifact_results: artifacts.map((file) => ({ path: file, sha256: hash(file), status: "passed" })),',
    '  status: "passed"',
    '};',
    'const output = process.env.SYMFORGE_LIFECYCLE_COMMAND_RECEIPT;',
    'fs.mkdirSync(path.dirname(output), { recursive: true });',
    'fs.writeFileSync(output, JSON.stringify(receipt) + "\\n");',
    "",
  ].join("\n");
  writeFixtureFile(root, "test", fakeCommandScript);
  writeFixtureFile(root, "bench", fakeCommandScript);
  runGit(root, ["add", "--all"]);
  runGit(root, ["commit", "--quiet", "-m", "materialized release"]);
  const releaseCommit = runGit(root, ["rev-parse", "HEAD^{commit}"]);
  const releaseTree = runGit(root, ["rev-parse", "HEAD^{tree}"]);

  const fakeBin = fs.mkdtempSync(path.join(os.tmpdir(), "symforge-lifecycle-cargo-"));
  const fakeCargo = path.join(fakeBin, process.platform === "win32" ? "cargo.exe" : "cargo");
  try {
    fs.linkSync(process.execPath, fakeCargo);
  } catch {
    fs.copyFileSync(process.execPath, fakeCargo);
  }
  if (process.platform !== "win32") fs.chmodSync(fakeCargo, 0o755);
  const gitExecutable = resolveProgram("git");
  if (gitExecutable === null) throw new Error("positive materialized fixture requires a Git executable");
  const releaseIdentity = "symforge-v11-materialized-fixture";
  const makeApprovalRecord = (sequence, predecessorDigest) => ({
    kind: "symforge-feature-020-refreeze-approval",
    schema_version: 1,
    repository: "special-place-ai-heaven/symforge",
    purpose: "implementation_start",
    target_commit: approvedCommit,
    target_tree: approvedTree,
    attestation: { path: "specs/020-repository-knowledge-index/contracts/refreeze-v11-attestation.json", sha256: "a".repeat(64) },
    release_identity: releaseIdentity,
    approved_at: "2026-08-11T00:00:00Z",
    sequence,
    store_locator: "materialized-fixture",
    store_version: 1,
    predecessor_digest: predecessorDigest,
    signature_namespace: "symforge-feature-020-refreeze-v11",
  });
  const historyRecordBytes = Buffer.from(`${JSON.stringify(makeApprovalRecord(1, null))}\n`);
  const historySignatureBytes = Buffer.from("historical approval signature fixture\n");
  const historyRecordSha256 = sha256(historyRecordBytes);
  const recordBytes = Buffer.from(`${JSON.stringify(makeApprovalRecord(2, historyRecordSha256))}\n`);
  const signatureBytes = Buffer.from("current approval signature fixture\n");
  const signerBytes = Buffer.from("allowed signer fixture\n");
  const recordSha256 = sha256(recordBytes);
  const signatureSha256 = sha256(signatureBytes);
  const approvalHistory = [{ sequence: 1, record_sha256: historyRecordSha256, signature_sha256: sha256(historySignatureBytes) }];
  const historyInventorySha256 = domainDigest("symforge.refreeze.v11.approval-history-inventory", approvalHistory);
  const historyRootPayload = {
    approval_sequence: 2,
    approval_predecessor_digest: historyRecordSha256,
    approval_history_count: 1,
    approval_history_inventory_sha256: historyInventorySha256,
    current_record_sha256: recordSha256,
    current_signature_sha256: signatureSha256,
  };
  const approvalResult = {
    kind: "symforge.refreeze_approval_verification_result.v11",
    schema_version: 1,
    approved_commit: approvedCommit,
    approved_tree: approvedTree,
    release_commit: releaseCommit,
    release_tree: releaseTree,
    verifier_sha256: fileHash(root, "execution/refreeze_v11.py"),
    record_sha256: recordSha256,
    signature_sha256: signatureSha256,
    allowed_signers_sha256: sha256(signerBytes),
    release_identity_sha256: sha256(Buffer.from(releaseIdentity, "utf8")),
    approval_sequence: 2,
    approval_predecessor_digest: historyRecordSha256,
    approval_history_inventory: approvalHistory,
    approval_history_count: 1,
    approval_history_inventory_sha256: historyInventorySha256,
    approval_history_root_sha256: domainDigest("symforge.refreeze.v11.approval-history-root", historyRootPayload),
    command_argv_sha256: sha256(canonicalJson(["protected-runner-python", "verify-approval", approvedCommit])),
    expected_repository: "special-place-ai-heaven/symforge",
    external_inputs: "outside_repository",
    command_id: "refreeze-v11-verify-approval",
    exit_code: 0,
    stdout_sha256: sha256(Buffer.from(`Feature 020 V11 external approval verification passed.${os.EOL}`)),
    stderr_sha256: sha256(Buffer.alloc(0)),
    runner_kind: "github_actions_protected_environment",
    runner_repository: "special-place-ai-heaven/symforge",
    workflow_path: ".github/workflows/release.yml",
    workflow_sha256: fileHash(root, ".github/workflows/release.yml"),
    workflow_commit: releaseCommit,
    workflow_run_id: "1",
    workflow_run_attempt: 1,
    workflow_job: "feature-020-v11-gate",
    workflow_event: "workflow_dispatch",
    status: "passed",
  };
  const approvalResultPath = "target/ci/lifecycle-v11/refreeze-approval-result.json";
  writeFixtureFile(root, approvalResultPath, `${JSON.stringify(approvalResult)}\n`);

  const benchmarkTest = trace.catalogs.tests["TEST-PERFORMANCE"];
  const benchmarkSource = benchmarkTest.target.split("::", 1)[0];
  const vectors = () => Object.fromEntries([
    "process_slots", "project_slots", "source_slots", "residency_bytes", "replacement_headroom_bytes", "response_reservation_bytes",
  ].map((dimension) => [dimension, 1]));
  for (const oracle of acceptance.oracles) {
    let artifact = makeGenericOracleArtifact(oracle, releaseCommit, releaseTree);
    if (oracle.oracle_id === "ORACLE-PERFORMANCE-OBSERVED-REFRESH") {
      artifact = {
        ...artifact,
        producer_task: "T068",
        registration: benchmarkTest.registration,
        source_sha256: fileHash(root, benchmarkSource),
        baseline_commit: "1521abb0",
        semantic_equivalence: "passed",
        completed_write_boundaries: ["external_write_burst", "symforge_mutation_commit"],
        first_strict_lease_byte_identity: "first_strict_lease_exact_byte_identity",
        pregranted_capacity_vector: vectors(),
        retained_capacity_vector: vectors(),
        candidate_capacity_vector: vectors(),
        declared_scratch_vector: vectors(),
        declared_headroom_vector: Object.fromEntries(Object.keys(vectors()).map((dimension) => [dimension, 0])),
        p95_seconds: 1,
        max_seconds: 1,
        baseline_ratio: 1,
        single_path_full_candidate_violations: 0,
        legal_full_candidate_triggers: ["observer_gap", "scope_dirty", "initial", "manual", "recovery"],
        corpus_sha256: sha256(Buffer.from("corpus")),
        environment_sha256: sha256(Buffer.from("environment")),
        completion_receipts_sha256: sha256(Buffer.from("completion")),
      };
    }
    writeFixtureFile(root, oracle.ci_artifact, `${JSON.stringify(artifact)}\n`);
  }

  const tasksText = fs.readFileSync(path.join(root, "specs/020-repository-knowledge-index/tasks.md"), "utf8");
  const reviewPath = "docs/reviews/FEATURE-020-V11-RELEASE-GATE.md";
  const reviewHash = fileHash(root, reviewPath);
  const taskReceipts = [];
  for (const [taskId, commandId] of commandIds) {
    const declaration = new RegExp(`^- \\[[ xX]\\] ${taskId}\\b[^\\r\\n]*$`, "mu").exec(tasksText);
    if (!declaration) throw new Error(`task declaration missing: ${taskId}`);
    const artifactResults = [{ path: reviewPath, sha256: reviewHash, status: "passed" }];
    if (taskId === "T080") artifactResults.push({ path: "tests/model/materialized.rs", sha256: fileHash(root, "tests/model/materialized.rs"), status: "passed" });
    if (taskId === "T089") artifactResults.push({ path: approvalResultPath, sha256: fileHash(root, approvalResultPath), status: "passed" });
    const taskResult = {
      kind: "symforge.lifecycle_release_task_result.v11",
      schema_version: 1,
      release_commit: releaseCommit,
      release_tree: releaseTree,
      task_id: taskId,
      task_declaration_sha256: sha256(Buffer.from(declaration[0], "utf8")),
      command_results: [{
        command_id: commandId,
        exit_code: 0,
        stdout_sha256: taskId === "T089" ? approvalResult.stdout_sha256 : sha256(Buffer.alloc(0)),
        stderr_sha256: taskId === "T089" ? approvalResult.stderr_sha256 : sha256(Buffer.alloc(0)),
      }],
      artifact_results: artifactResults,
      status: "passed",
    };
    const resultPath = `target/ci/lifecycle-v11/task-${taskId}.json`;
    writeFixtureFile(root, resultPath, `${JSON.stringify(taskResult)}\n`);
    taskReceipts.push({ task_id: taskId, status: "passed", release_tree: releaseTree, artifact: resultPath, artifact_sha256: fileHash(root, resultPath) });
  }

  const oracleByRequirement = new Map(Array.from({ length: 78 }, (_, index) => [
    index < 52 ? `FR-${String(index + 1).padStart(3, "0")}` : `SC-${String(index - 51).padStart(3, "0")}`,
    [],
  ]));
  for (const oracle of acceptance.oracles) for (const requirementId of oracle.requirement_ids) oracleByRequirement.get(requirementId).push(oracle.oracle_id);
  for (const ids of oracleByRequirement.values()) ids.sort();
  const requirementReceipts = [...oracleByRequirement].map(([requirementId, oracleIds]) => ({
    requirement_id: requirementId, oracle_ids: oracleIds, status: "passed", release_tree: releaseTree,
  }));
  const oracleReceipts = [...acceptance.oracles].sort((left, right) => left.oracle_id.localeCompare(right.oracle_id)).map((oracle) => ({
    oracle_id: oracle.oracle_id,
    artifact: oracle.ci_artifact,
    artifact_sha256: fileHash(root, oracle.ci_artifact),
    status: "passed",
    release_tree: releaseTree,
  }));

  const rustCaseReceipts = Object.entries(trace.catalogs.tests).filter(([, test]) => ["planned_exact", "inherited_exact"].includes(test.kind))
    .sort(([left], [right]) => left.localeCompare(right)).map(([testId, test]) => {
      const source = test.target.split("::", 1)[0];
      return { test_id: testId, target: test.target, command: trace.catalogs.commands[test.command_id], status: "passed", release_tree: releaseTree, source_sha256: fileHash(root, source) };
    });
  const performanceOracle = acceptance.oracles.find((oracle) => oracle.oracle_id === "ORACLE-PERFORMANCE-OBSERVED-REFRESH");
  const benchmarkReceipts = [{
    test_id: "TEST-PERFORMANCE",
    target: benchmarkTest.target,
    command: trace.catalogs.commands[benchmarkTest.command_id],
    registration: benchmarkTest.registration,
    status: "passed",
    release_tree: releaseTree,
    source_sha256: fileHash(root, benchmarkSource),
    receipt: performanceOracle.ci_artifact,
    receipt_sha256: fileHash(root, performanceOracle.ci_artifact),
  }];
  const seamReverse = new Map();
  for (const [seamId, anchors] of Object.entries(trace.catalogs.seams)) for (const anchor of anchors) {
    if (!seamReverse.has(anchor)) seamReverse.set(anchor, []);
    seamReverse.get(anchor).push(seamId);
  }
  const sourceReceipts = [...seamReverse].sort(([left], [right]) => left < right ? -1 : left > right ? 1 : 0).map(([anchor, seamIds]) => {
    const source = anchor.split("::", 1)[0];
    return { anchor, seam_ids: seamIds.sort(), status: "passed", release_tree: releaseTree, source_sha256: fileHash(root, source) };
  });
  const evidence = {
    kind: "symforge.lifecycle_release_evidence.v11",
    schema_version: 1,
    release_commit: releaseCommit,
    release_tree: releaseTree,
    approved_refreeze_commit: approvedCommit,
    approved_refreeze_tree: approvedTree,
    approval_verification: { status: "passed", result_artifact: approvalResultPath, result_sha256: fileHash(root, approvalResultPath) },
    trace_contract_sha256: fileHash(root, fixturePaths[0]),
    acceptance_contract_sha256: fileHash(root, fixturePaths[1]),
    retirement_contract_sha256: fileHash(root, fixturePaths[2]),
    checker_sha256: fileHash(root, "scripts/validate-lifecycle-oracle-traceability.cjs"),
    requirement_receipts: requirementReceipts,
    oracle_receipts: oracleReceipts,
    task_receipts: taskReceipts,
    rust_case_receipts: rustCaseReceipts,
    benchmark_receipts: benchmarkReceipts,
    source_receipts: sourceReceipts,
    status: "passed",
  };
  const evidencePath = "target/ci/lifecycle-v11/release-evidence.json";
  writeFixtureFile(root, evidencePath, `${JSON.stringify(evidence)}\n`);

  return {
    evidencePath,
    fakeBin,
    materializedEnvironment: {
      SYMFORGE_LIFECYCLE_GIT_EXECUTABLE: gitExecutable,
      SYMFORGE_LIFECYCLE_CARGO_EXECUTABLE: fs.realpathSync(fakeCargo),
    },
  };
}

const cases = [
  {
    name: "falsy sentinel root",
    expected: "ERROR SENTINEL_ROOT_INVALID:",
    mutate(root) {
      replaceSentinelJson(root, tracePath, traceStart, traceEnd, "null");
    },
  },
  {
    name: "verification obligation interval may be infinite",
    expected: "ERROR OVERDUE_VERIFICATION_ORACLE_INVALID:",
    mutate(root) {
      mutateSentinel(root, acceptancePath, acceptanceStart, acceptanceEnd, (acceptance) => {
        const oracle = acceptance.oracles.find((item) => item.oracle_id === "ORACLE-ROLLING-VERIFICATION-COVERAGE");
        oracle.bounds = ["The verification interval may be infinite or disabled while Current remains queryable"];
      });
    },
  },
  {
    name: "whole-scope verification omits its exact completion and feasibility proof",
    expected: "ERROR VERIFICATION_PASS_ORACLE_INVALID:",
    mutate(root) {
      mutateSentinel(root, acceptancePath, acceptanceStart, acceptanceEnd, (acceptance) => {
        const oracle = acceptance.oracles.find((item) => item.oracle_id === "ORACLE-ROLLING-VERIFICATION-COVERAGE");
        oracle.assertions = oracle.assertions.filter((assertion) => !assertion.startsWith("A complete whole-declared-scope pass"));
      });
    },
  },
  {
    name: "new writer in a retirement-owned source file",
    expected: "ERROR RETIREMENT_CLOSURE_MISMATCH:",
    mutate(root) {
      fs.appendFileSync(
        path.join(root, "src/protocol/edit.rs"),
        "\npub fn unretired_writer(path: &std::path::Path) { std::fs::write(path, b\"unretired\").unwrap(); }\n",
        "utf8",
      );
    },
  },
  {
    name: "ignored case is still rejected where the catalog requires it to have run",
    expected: "ERROR INHERITED_TEST_CASE_MISSING:",
    mutate(root) {
      const file = path.join(root, "src/discovery/mod.rs");
      const text = fs.readFileSync(file, "utf8");
      const marker = "        fn non_utf8_path_is_opaque_catalog_only_without_lossy_collision() {";
      if (!text.includes(marker)) throw new Error("inherited opaque-path case not found in fixture");
      fs.writeFileSync(file, text.replace(marker, `        #[ignore = "self-test"]\n${marker}`), "utf8");
    },
  },
  {
    name: "string literal content changed in a retirement-owned source file",
    expected: "ERROR RETIREMENT_CLOSURE_MISMATCH:",
    mutate(root) {
      const file = path.join(root, "src/protocol/edit.rs");
      const text = fs.readFileSync(file, "utf8");
      const match = /"[A-Za-z0-9 _.:-]{6,}"/u.exec(text);
      if (!match) throw new Error("no string literal available in the censused writer source");
      fs.writeFileSync(file, text.replace(match[0], match[0].slice(0, -1) + 'X"'), "utf8");
    },
  },
  {
    name: "cfg(not(test)) item added to a retirement-owned source file",
    expected: "ERROR RETIREMENT_CLOSURE_MISMATCH:",
    mutate(root) {
      // not(test) is the opposite of test-only: it ships. The census must see it.
      fs.appendFileSync(
        path.join(root, "src/protocol/edit.rs"),
        ["", "#[cfg(not(test))]", "pub fn ships_in_release() {}", ""].join("\n"),
        "utf8",
      );
    },
  },
  {
    name: "cfg(any(test, feature)) item added to a retirement-owned source file",
    expected: "ERROR RETIREMENT_CLOSURE_MISMATCH:",
    mutate(root) {
      // any() needs only one disjunct, so a feature build compiles this.
      fs.appendFileSync(
        path.join(root, "src/protocol/ccr.rs"),
        ["", '#[cfg(any(test, feature = "server"))]', "pub fn maybe_ships() {}", ""].join("\n"),
        "utf8",
      );
    },
  },
  {
    name: "new CCR path in its retirement-owned source file",
    expected: "ERROR RETIREMENT_CLOSURE_MISMATCH:",
    mutate(root) {
      fs.appendFileSync(
        path.join(root, "src/protocol/ccr.rs"),
        "\npub fn unretired_ccr_path(result: String) -> String { rewrite_footer_for_symforge_facade(result) }\n",
        "utf8",
      );
    },
  },
  {
    name: "amendment regression binding drifts from its executable oracle",
    expected: "ERROR AMENDMENT_REGRESSION_BINDING_INVALID:",
    mutate(root) {
      const file = path.join(root, acceptancePath);
      const text = fs.readFileSync(file, "utf8");
      fs.writeFileSync(file, text.replace("Regression: `F020-V11-R19B`", "Regression: `F020-V11-R19C`"), "utf8");
    },
  },
  {
    name: "missing requirement",
    expected: "ERROR TRACE_REQUIREMENT_MISSING:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => trace.requirements.shift());
    },
  },
  {
    name: "duplicate requirement",
    expected: "ERROR TRACE_REQUIREMENT_DUPLICATE:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => trace.requirements.push({ ...trace.requirements[0] }));
    },
  },
  {
    name: "unresolved task",
    expected: "ERROR TASK_UNRESOLVED:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => { trace.requirements[0].implementation_tasks = ["T999"]; });
    },
  },
  {
    name: "absent implementation mapping",
    expected: "ERROR TRACE_REQUIREMENT_IMPLEMENTATION_MISSING:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => { trace.requirements[0].implementation_tasks = []; });
    },
  },
  {
    name: "absent test mapping",
    expected: "ERROR TRACE_REQUIREMENT_TESTS_EMPTY:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => { trace.requirements[0].test_ids = []; });
    },
  },
  {
    name: "absent command",
    expected: "ERROR COMMAND_REF_UNRESOLVED:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        const testId = trace.requirements[0].test_ids[0];
        delete trace.catalogs.commands[trace.catalogs.tests[testId].command_id];
      });
    },
  },
  {
    name: "absent target slice",
    expected: "ERROR SLICE_INVALID:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => { delete trace.requirements[0].target_slice; });
    },
  },
  {
    name: "falsely executed planned oracle",
    expected: "ERROR EXECUTION_CLAIM_FORBIDDEN:",
    mutate(root) {
      mutateSentinel(root, acceptancePath, acceptanceStart, acceptanceEnd, (acceptance) => { acceptance.oracles[0].executed = true; });
    },
  },
  {
    name: "obsolete lifecycle seam namespace",
    expected: "ERROR SEAM_NAMESPACE_INVALID:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        trace.catalogs.seams["SEAM-CANDIDATE"] = ["src/lifecycle/candidate.rs::CandidateHandle"];
      });
    },
  },
  {
    name: "unresolved production seam",
    expected: "ERROR SEAM_UNRESOLVED:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        trace.catalogs.seams["SEAM-CANDIDATE"] = ["src/index_lifecycle/fictional.rs::PhantomCandidate"];
      });
    },
  },
  {
    name: "universal ingress authority claim erases pure lanes",
    expected: "ERROR INGRESS_LANE_CONTRACT_INVALID:",
    mutate(root) {
      mutateSentinel(root, acceptancePath, acceptanceStart, acceptanceEnd, (acceptance) => {
        const oracle = acceptance.oracles.find((item) => item.category === "ingress");
        oracle.assertions = [
          "Every read-capable ingress acquires a ProjectQueryLease",
          "Every mutation-capable ingress acquires a MutationPermit",
          "No alias, prompt, resource, sidecar, hook, or daemon callback reaches V10 authority",
          "Typed refusals are stable across full and compact surfaces",
        ];
      });
    },
  },
  {
    name: "ingress source-write authority is logically inverted",
    expected: "ERROR INGRESS_LANE_CONTRACT_INVALID:",
    mutate(root) {
      mutateSentinel(root, acceptancePath, acceptanceStart, acceptanceEnd, (acceptance) => {
        const oracle = acceptance.oracles.find((item) => item.category === "ingress");
        oracle.assertions = oracle.assertions.map((assertion) => assertion.replace(
          "MutationPermitted is used only for repository-source byte writes and holds a current SourceMutationPermit",
          "MutationPermitted is never used for repository-source byte writes and never holds a current SourceMutationPermit",
        ));
      });
    },
  },
  {
    name: "strict query model admits stale lease",
    expected: "ERROR STRICT_QUERY_MODEL_INVALID:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        trace.catalogs.state_models["MODEL-QUERY"] = ["Selecting", "LeasedCurrent", "LeasedStale", "Refused", "Released"];
      });
    },
  },
  {
    name: "capacity oracle substitutes fifo round robin",
    expected: "ERROR CAPACITY_ORACLE_INVALID:",
    mutate(root) {
      mutateSentinel(root, acceptancePath, acceptanceStart, acceptanceEnd, (acceptance) => {
        const oracle = acceptance.oracles.find((item) => item.category === "capacity");
        oracle.actions = [
          "Generate FIFO grants, cancellations, panics, replacement, project stop, and embedded shutdown",
          "Hold the physical worker after logical cancellation",
          "Attempt new grants before and after drop acknowledgement",
        ];
        oracle.fairness = ["FIFO within priority class", "Round-robin across projects", "Cancellation wins over a later grant"];
      });
    },
  },
  {
    name: "capacity physical ownership is logically inverted",
    expected: "ERROR CAPACITY_ORACLE_INVALID:",
    mutate(root) {
      mutateSentinel(root, acceptancePath, acceptanceStart, acceptanceEnd, (acceptance) => {
        const oracle = acceptance.oracles.find((item) => item.category === "capacity");
        oracle.assertions = oracle.assertions.map((assertion) => assertion.replace(
          "Logical cancellation never refunds live physical ownership",
          "Logical cancellation always refunds live physical ownership",
        ));
      });
    },
  },
  {
    name: "degenerate requirement implementation and test mapping",
    expected: "ERROR TRACE_MAPPING_DEGENERATE:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        for (const requirement of trace.requirements) {
          requirement.implementation_tasks = ["T001"];
          requirement.test_ids = ["TEST-REGISTRY"];
        }
      });
    },
  },
  {
    name: "acceptance oracles collapse to one requirement",
    expected: "ERROR ORACLE_REQUIREMENT_COVERAGE_INVALID:",
    mutate(root) {
      mutateSentinel(root, acceptancePath, acceptanceStart, acceptanceEnd, (acceptance) => {
        for (const oracle of acceptance.oracles) oracle.requirement_ids = ["FR-001"];
      });
    },
  },
  {
    name: "fictional retirement member",
    expected: "ERROR RETIREMENT_MEMBER_UNRESOLVED:",
    mutate(root) {
      const retirementPath = fixturePaths[2];
      const retirementStart = "<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON START -->";
      const retirementEnd = "<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON END -->";
      mutateSentinel(root, retirementPath, retirementStart, retirementEnd, (retirement) => {
        const writers = retirement.entries.find((item) => item.category === "writers");
        writers.members.push("src/index_lifecycle/fictional.rs::PhantomWriter");
        writers.members.sort();
      });
    },
  },
  {
    name: "stale retirement holder names replace corrected anchors",
    expected: "ERROR RETIREMENT_MEMBER_UNRESOLVED:",
    mutate(root) {
      mutateSentinel(
        root,
        fixturePaths[2],
        "<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON START -->",
        "<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON END -->",
        (retirement) => {
          const roots = retirement.entries.find((entry) => entry.category === "publication_roots");
          roots.members = roots.members.map((member) => member.replace("ProjectInstance::index", "ProjectSlot::index"));
          const cache = retirement.entries.find((entry) => entry.category === "cache");
          cache.members = cache.members.map((member) => member
            .replace("ProjectInstance::symbol_cache", "ProjectSlot::symbol_cache")
            .replace("KnowledgeCurationCoordinator::probe_cache", "KnowledgeCurationStore::probe_cache"));
        },
      );
    },
  },
  {
    name: "required retirement publication and cache anchors are omitted",
    expected: "ERROR RETIREMENT_MEMBER_UNRESOLVED:",
    mutate(root) {
      mutateSentinel(
        root,
        fixturePaths[2],
        "<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON START -->",
        "<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON END -->",
        (retirement) => {
          const omitted = new Set([
            "src/daemon.rs::SessionRuntime::index",
            "src/daemon.rs::SessionRuntime::project_indexes",
            "src/server/mod.rs::ServerRuntime::index",
            "src/daemon.rs::SessionRuntime::symbol_cache",
            "src/protocol/session.rs::SessionInner::detailed_fetches",
          ]);
          for (const category of ["publication_roots", "cache"]) {
            const entry = retirement.entries.find((item) => item.category === category);
            entry.members = entry.members.filter((member) => !omitted.has(member));
          }
        },
      );
    },
  },
  {
    name: "required writer callback hook and snapshot anchors are omitted",
    expected: "ERROR RETIREMENT_MEMBER_UNRESOLVED:",
    mutate(root) {
      mutateSentinel(
        root,
        fixturePaths[2],
        "<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON START -->",
        "<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON END -->",
        (retirement) => {
          const omitted = new Set([
            "src/gitignore_hygiene.rs::atomic_replace",
            "src/live_index/git_temporal.rs::spawn_git_temporal_computation",
            "hook:PromptSubmit",
            "src/live_index/persist.rs::serialize_shared_index",
          ]);
          for (const category of ["writers", "callbacks", "hooks", "snapshot"]) {
            const entry = retirement.entries.find((item) => item.category === category);
            entry.members = entry.members.filter((member) => !omitted.has(member));
          }
        },
      );
    },
  },
  {
    name: "snapshot writer owner T065 is omitted",
    expected: "ERROR SLICE4_OWNER_TASK_INVALID:",
    mutate(root) {
      mutateSentinel(
        root,
        fixturePaths[2],
        "<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON START -->",
        "<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON END -->",
        (retirement) => {
          const writers = retirement.entries.find((entry) => entry.category === "writers");
          writers.slice4_owner_tasks = writers.slice4_owner_tasks.filter((task) => task !== "T065");
        },
      );
    },
  },
  {
    name: "nonexistent exact-shaped planned test",
    expected: "ERROR TEST_TARGET_UNRESOLVED:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        const test = trace.catalogs.tests["TEST-CANDIDATE"];
        test.target = "tests/fictional_lifecycle_v11.rs::fictional_case";
        trace.catalogs.commands[test.command_id] = "cargo test --test fictional_lifecycle_v11 fictional_case -- --exact";
      });
    },
  },
  {
    name: "executed test target is absent",
    expected: "ERROR EXECUTED_TEST_TARGET_MISSING:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        const test = trace.catalogs.tests["TEST-CANDIDATE"];
        test.kind = "executed_exact";
        test.target = "tests/fictional_executed_lifecycle_v11.rs::fictional_executed_case";
        trace.catalogs.commands[test.command_id] = "cargo test --test fictional_executed_lifecycle_v11 fictional_executed_case -- --exact";
      });
    },
  },
  {
    name: "executed test case is absent from an existing Rust target",
    expected: "ERROR EXECUTED_TEST_CASE_MISSING:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        const test = trace.catalogs.tests["TEST-CANDIDATE"];
        test.kind = "executed_exact";
        test.target = "tests/admin_api_v1.rs::fictional_executed_case";
        trace.catalogs.commands[test.command_id] = "cargo test --test admin_api_v1 fictional_executed_case -- --exact";
      });
    },
  },
  {
    name: "SC-024 points at an integration test instead of the benchmark",
    expected: "ERROR SC024_TARGET_INVALID:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        const test = trace.catalogs.tests["TEST-PERFORMANCE"];
        test.target = "tests/observed_refresh_gate_v1.rs::observed_refresh_gate_v1";
        trace.catalogs.commands[test.command_id] = "cargo test --test observed_refresh_gate_v1 observed_refresh_gate_v1 -- --exact";
      });
    },
  },
  {
    name: "dedicated acceptance oracle families are absent",
    expected: "ERROR ORACLE_CATEGORY_COUNT:",
    mutate(root) {
      mutateSentinel(root, acceptancePath, acceptanceStart, acceptanceEnd, (acceptance) => {
        const required = new Set(["publication", "knowledge", "performance", "snapshot", "state", "rolling_verification"]);
        acceptance.oracles = acceptance.oracles.filter((oracle) => !required.has(oracle.category));
      });
    },
  },
  {
    name: "acceptance union omits FR-036 and FR-046",
    expected: "ERROR ORACLE_REQUIREMENT_COVERAGE_INVALID:",
    mutate(root) {
      mutateSentinel(root, acceptancePath, acceptanceStart, acceptanceEnd, (acceptance) => {
        for (const oracle of acceptance.oracles) {
          oracle.requirement_ids = oracle.requirement_ids.filter((id) => id !== "FR-036" && id !== "FR-046");
        }
      });
    },
  },
  {
    name: "trace acceptance cross edge is inconsistent",
    expected: "ERROR TRACE_ACCEPTANCE_EDGE_MISMATCH:",
    mutate(root) {
      mutateSentinel(root, acceptancePath, acceptanceStart, acceptanceEnd, (acceptance) => {
        const ingress = acceptance.oracles.find((oracle) => oracle.category === "ingress");
        ingress.requirement_ids.push("FR-001");
      });
    },
  },
  {
    name: "test owner task does not own requirement implementation",
    expected: "ERROR TEST_TASK_OWNERSHIP_INVALID:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        trace.requirements[0].implementation_tasks = ["T033"];
      });
    },
  },
  {
    name: "raw embed bypass test is assigned to Slice 2",
    expected: "ERROR TEST_SLICE_OWNERSHIP_INVALID:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        trace.catalogs.tests["TEST-EMBED"].introduced_slice = 2;
      });
    },
  },
  {
    name: "Current and whole-runtime requirements are assigned before Slice 4",
    expected: "ERROR REQUIREMENT_SLICE_INVALID:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        for (const id of ["FR-041", "FR-042", "SC-017", "SC-025"]) {
          trace.requirements.find((row) => row.requirement_id === id).target_slice = 2;
        }
      });
    },
  },
  {
    name: "SC-006 omits release workflow owner T081",
    expected: "ERROR REQUIREMENT_TASK_EDGE_INVALID:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        const row = trace.requirements.find((item) => item.requirement_id === "SC-006");
        row.implementation_tasks = row.implementation_tasks.filter((id) => id !== "T081");
      });
    },
  },
  {
    name: "FR-020 and FR-025 use inconsistent test edges",
    expected: "ERROR REQUIREMENT_TEST_EDGE_INVALID:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        trace.requirements.find((row) => row.requirement_id === "FR-020").test_ids = ["TEST-KNOWLEDGE", "TEST-OBSERVER"];
        trace.requirements.find((row) => row.requirement_id === "FR-025").test_ids = ["TEST-CANDIDATE"];
      });
    },
  },
  {
    name: "compatibility alias retirement category is absent",
    expected: "ERROR RETIREMENT_CATEGORY_COUNT:",
    mutate(root) {
      mutateSentinel(
        root,
        fixturePaths[2],
        "<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON START -->",
        "<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON END -->",
        (retirement) => { retirement.entries = retirement.entries.filter((entry) => entry.category !== "compatibility_aliases"); },
      );
    },
  },
  {
    name: "compatibility alias retirement omits detect_changes",
    expected: "ERROR RETIREMENT_ALIASES_MISMATCH:",
    mutate(root) {
      mutateSentinel(
        root,
        fixturePaths[2],
        "<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON START -->",
        "<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON END -->",
        (retirement) => {
          let aliases = retirement.entries.find((entry) => entry.category === "compatibility_aliases");
          if (!aliases) {
            aliases = {
              category: "compatibility_aliases",
              members: ["trace_symbol"],
              production_seams: ["src/index_lifecycle/activation.rs::ActivationCut"],
              slice4_owner_tasks: ["T066", "T067"],
              disposition: "retire compatibility aliases through the activation cut",
              retirement_test: "tests/activation_cut_v11.rs::v10_authority_retirement_inventory_is_unreachable",
              command: "cargo test --test activation_cut_v11 v10_authority_retirement_inventory_is_unreachable -- --exact",
              assertions: ["No compatibility alias bypasses V11 authority"],
              status: "planned_not_executed",
              executed: false,
            };
            retirement.entries.push(aliases);
          } else {
            aliases.members = ["trace_symbol"];
          }
        },
      );
    },
  },
  {
    name: "surface authority model collapses typed observation branches",
    expected: "ERROR SURFACE_AUTHORITY_MODEL_INVALID:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        trace.catalogs.state_models["MODEL-SURFACE"] = ["Ingress", "AuthorityChecked", "QueryLeased", "Answered", "TypedRefusal"];
      });
    },
  },
  {
    name: "protected persistence refuses before user-local fallback",
    expected: "ERROR PROTECTED_STATE_FALLBACK_INVALID:",
    mutate(root) {
      mutateSentinel(root, acceptancePath, acceptanceStart, acceptanceEnd, (acceptance) => {
        const oracle = acceptance.oracles.find((item) => item.category === "state");
        oracle.negative_controls = ["Protected and memory-only placements refuse persistence explicitly", "A foreign or nested state path cannot redirect source or query authority"];
      });
    },
  },
  {
    name: "FR-025 is remapped to registry ABA",
    expected: "ERROR FR025_OPAQUE_PATH_EDGE_INVALID:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        const row = trace.requirements.find((item) => item.requirement_id === "FR-025");
        row.implementation_tasks = ["T023", "T024", "T025", "T030", "T033"];
        row.test_ids = ["TEST-PHYSICAL-ROOT", "TEST-REGISTRY"];
        row.seam_ids = ["SEAM-REGISTRY"];
        row.invariant_id = "INV-REGISTRY";
        row.state_model_id = "MODEL-REGISTRY";
        row.target_slice = 2;
        row.ci_artifact_id = "CI-SLICE2";
      });
    },
  },
  {
    name: "physical-root acceptance oracle is absent",
    expected: "ERROR ORACLE_ID_MISSING:",
    mutate(root) {
      mutateSentinel(root, acceptancePath, acceptanceStart, acceptanceEnd, (acceptance) => {
        acceptance.oracles = acceptance.oracles.filter((oracle) => oracle.oracle_id !== "ORACLE-PHYSICAL-ROOT-CONFINEMENT");
      });
    },
  },
  {
    name: "publication oracle loses the T017 pause rebase schedule",
    expected: "ERROR PUBLICATION_ORACLE_INVALID:",
    mutate(root) {
      mutateSentinel(root, acceptancePath, acceptanceStart, acceptanceEnd, (acceptance) => {
        const oracle = acceptance.oracles.find((item) => item.category === "publication");
        oracle.actions = ["Attempt every partial candidate completion order", "Race a complete commit with activation, cancellation, stop, and a query lease"];
        oracle.assertions = ["A public root changes atomically or not at all", "No query can observe a source-by-source mixture", "The activation cut cannot expose a dark or incomplete root"];
      });
    },
  },
  {
    name: "capacity oracle omits safety precharge and bounded bypass",
    expected: "ERROR CAPACITY_ORACLE_INVALID:",
    mutate(root) {
      mutateSentinel(root, acceptancePath, acceptanceStart, acceptanceEnd, (acceptance) => {
        const oracle = acceptance.oracles.find((item) => item.oracle_id === "ORACLE-CAPACITY-PHYSICAL-OWNERSHIP");
        oracle.preconditions = ["Finite process, project, source, and embedded permit ceilings are configured", "Worker drop and logical cancellation are separately pausable"];
        oracle.assertions = ["Owned plus available units equal every configured dimension", "Logical cancellation never refunds live physical ownership", "Oldest-satisfiable dispatch never bypasses an applicable drain barrier", "Resize cleanup completes before requeue and every permit returns exactly once"];
      });
    },
  },
  {
    name: "SC-024 performance oracle drops T070 and exact thresholds",
    expected: "ERROR PERFORMANCE_ORACLE_INVALID:",
    mutate(root) {
      mutateSentinel(root, acceptancePath, acceptanceStart, acceptanceEnd, (acceptance) => {
        const oracle = acceptance.oracles.find((item) => item.category === "performance");
        oracle.implementation_tasks = ["T068", "T069", "T071"];
        oracle.assertions = ["The benchmark target and command are task-owned", "No shortcut weakens candidate verification, capacity conservation, or delta equivalence", "The frozen performance threshold is evaluated from reproducible artifacts"];
      });
    },
  },
  {
    name: "migration oracle reclaims benchmark authority",
    expected: "ERROR MIGRATION_ORACLE_INVALID:",
    mutate(root) {
      mutateSentinel(root, acceptancePath, acceptanceStart, acceptanceEnd, (acceptance) => {
        const oracle = acceptance.oracles.find((item) => item.category === "migration");
        if (!oracle.actions.includes("Run the frozen observed-refresh performance fixture")) oracle.actions.push("Run the frozen observed-refresh performance fixture");
        if (!oracle.assertions.includes("Observed refresh meets the frozen bound without weakening correctness")) oracle.assertions.push("Observed refresh meets the frozen bound without weakening correctness");
      });
    },
  },
  {
    name: "one requirement silently loses production owners",
    expected: "ERROR FROZEN_REQUIREMENT_ROWS_DIGEST_MISMATCH:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        trace.requirements.find((row) => row.requirement_id === "FR-001").implementation_tasks = ["T053"];
      });
    },
  },
  {
    name: "catalog payload changes while references remain valid",
    expected: "ERROR FROZEN_CATALOGS_DIGEST_MISMATCH:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        trace.catalogs.ci_artifacts["CI-SLICE4"] = "A structurally valid but unfrozen Slice 4 artifact description.";
      });
    },
  },
  {
    name: "invariant meaning changes under the same identifier",
    expected: "ERROR FROZEN_INVARIANTS_DIGEST_MISMATCH:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        trace.catalogs.invariants["INV-PUBLICATION"] = "Partial source generations may publish when convenient.";
      });
    },
  },
  {
    name: "state-model meaning changes under the same identifier",
    expected: "ERROR FROZEN_STATE_MODELS_DIGEST_MISMATCH:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        trace.catalogs.state_models["MODEL-PUBLICATION"] = ["NoPublication", "PartiallyCurrent", "Current"];
      });
    },
  },
  {
    name: "oracle record changes without breaking its shape",
    expected: "ERROR FROZEN_ACCEPTANCE_ORACLES_DIGEST_MISMATCH:",
    mutate(root) {
      mutateSentinel(root, acceptancePath, acceptanceStart, acceptanceEnd, (acceptance) => {
        acceptance.oracles.find((oracle) => oracle.category === "observer").positive_control = "Any delivered watcher record proves completeness.";
      });
    },
  },
  {
    name: "retirement edge narrows to a valid but incomplete seam",
    expected: "ERROR FROZEN_RETIREMENT_EDGES_DIGEST_MISMATCH:",
    mutate(root) {
      mutateSentinel(root, fixturePaths[2], "<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON START -->", "<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON END -->", (retirement) => {
        retirement.entries.find((entry) => entry.category === "writers").production_seams = ["src/index_lifecycle/mutation.rs::SourceMutationPermit"];
      });
    },
  },
  {
    name: "retirement record changes outside its edge set",
    expected: "ERROR FROZEN_RETIREMENT_RECORDS_DIGEST_MISMATCH:",
    mutate(root) {
      mutateSentinel(root, fixturePaths[2], "<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON START -->", "<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON END -->", (retirement) => {
        retirement.entries.find((entry) => entry.category === "callbacks").disposition += " eventually";
      });
    },
  },
  {
    name: "planned benchmark omits its registration declaration",
    expected: "ERROR BENCHMARK_REGISTRATION_INVALID:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        delete trace.catalogs.tests["TEST-PERFORMANCE"].registration;
      });
    },
  },
  {
    name: "criterion group containing the benchmark is unreachable from criterion_main",
    expected: "ERROR BENCHMARK_REGISTRATION_INVALID:",
    mutate(root) {
      const file = path.join(root, "benches/observed_refresh_gate_v1.rs");
      fs.mkdirSync(path.dirname(file), { recursive: true });
      fs.writeFileSync(
        file,
        [
          "fn observed_refresh_gate_v1() {}",
          "fn unrelated_benchmark() {}",
          "criterion_group!(observed_refresh_gate_v1_group, observed_refresh_gate_v1);",
          "criterion_group!(unrelated_group, unrelated_benchmark);",
          "criterion_main!(unrelated_group);",
          "",
        ].join("\n"),
        "utf8",
      );
    },
  },
  {
    name: "criterion registration rejects a duplicate benchmark group",
    expected: "ERROR BENCHMARK_REGISTRATION_INVALID:",
    mutate(root) {
      const file = path.join(root, "benches/observed_refresh_gate_v1.rs");
      fs.mkdirSync(path.dirname(file), { recursive: true });
      fs.writeFileSync(
        file,
        [
          "fn observed_refresh_gate_v1() {}",
          "criterion_group!(observed_refresh_gate_v1_group, observed_refresh_gate_v1);",
          "criterion_group!(observed_refresh_gate_v1_group, observed_refresh_gate_v1);",
          "criterion_main!(observed_refresh_gate_v1_group);",
          "",
        ].join("\n"),
        "utf8",
      );
    },
  },
  {
    name: "executed benchmark omits its evidence contract",
    expected: "ERROR BENCHMARK_EVIDENCE_CONTRACT_MISSING:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        const test = trace.catalogs.tests["TEST-PERFORMANCE"];
        test.kind = "executed_benchmark";
        delete test.evidence_contract;
      });
    },
  },
  {
    name: "executed non-Rust target omits an explicit resolver",
    expected: "ERROR NON_RUST_RESOLVER_MISSING:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        const test = trace.catalogs.tests["TEST-CANDIDATE"];
        test.kind = "executed_exact";
        test.target = "scripts/validate-lifecycle-oracle-traceability.cjs::main";
        trace.catalogs.commands[test.command_id] = "node scripts/validate-lifecycle-oracle-traceability.cjs";
      });
    },
  },
  {
    name: "Rust test target uses a non-Rust extension",
    expected: "ERROR RUST_TARGET_EXTENSION_INVALID:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        const test = trace.catalogs.tests["TEST-CANDIDATE"];
        test.target = "tests/index_candidate_lifecycle_v11.txt::closed_candidate_promotion_matrix";
        trace.catalogs.commands[test.command_id] = "cargo test --test index_candidate_lifecycle_v11 closed_candidate_promotion_matrix -- --exact";
      });
    },
  },
  {
    name: "inherited opaque-path test drops its exact module path",
    expected: "ERROR INHERITED_TEST_CASE_MISSING:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        const test = trace.catalogs.tests["TEST-OPAQUE-PATH-INHERITED"];
        test.target = "src/discovery/mod.rs::non_utf8_path_is_opaque_catalog_only_without_lossy_collision";
        trace.catalogs.commands[test.command_id] = "cargo test --lib discovery::non_utf8_path_is_opaque_catalog_only_without_lossy_collision -- --exact";
      });
    },
  },
  {
    name: "retirement resource inventory restores a universal query lease",
    expected: "ERROR FROZEN_RETIREMENT_RECORDS_DIGEST_MISMATCH:",
    mutate(root) {
      mutateSentinel(root, fixturePaths[2], "<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON START -->", "<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON END -->", (retirement) => {
        retirement.entries.find((entry) => entry.category === "resources").assertions = [
          "Every resource and template acquires one ProjectQueryLease",
          "Static catalog resources cannot disclose raw runtime state",
        ];
      });
    },
  },
  {
    name: "retirement snapshot inventory refuses protected persistence before fallback",
    expected: "ERROR FROZEN_RETIREMENT_RECORDS_DIGEST_MISMATCH:",
    mutate(root) {
      mutateSentinel(root, fixturePaths[2], "<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON START -->", "<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON END -->", (retirement) => {
        retirement.entries.find((entry) => entry.category === "snapshot").assertions = [
          "Snapshot load never publishes directly",
          "Compatibility and source identity are re-proved before promotion",
          "Memory-only and protected placement produce typed checkpoint unavailability",
        ];
      });
    },
  },
  {
    name: "executed non-Rust target supplies a self-attested resolver",
    expected: "ERROR NON_RUST_RESOLVER_INVALID:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        const test = trace.catalogs.tests["TEST-CANDIDATE"];
        test.kind = "executed_exact";
        test.target = "scripts/validate-lifecycle-oracle-traceability.cjs::main";
        test.resolver = { kind: "node_export", symbol: "main" };
        trace.catalogs.commands[test.command_id] = "node scripts/validate-lifecycle-oracle-traceability.cjs";
      });
    },
  },
  {
    name: "surface authority union drops RuntimeHealthObserved",
    expected: "ERROR RUNTIME_HEALTH_BRANCH_INVALID:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        trace.catalogs.invariants["INV-SURFACE"] = trace.catalogs.invariants["INV-SURFACE"].replace("RuntimeHealthObserved, ", "");
        trace.catalogs.state_models["MODEL-SURFACE"] = trace.catalogs.state_models["MODEL-SURFACE"].filter((state) => state !== "RuntimeHealthObserved");
      });
    },
  },
  {
    name: "health oracle allows attempt evidence to populate committed fields",
    expected: "ERROR HEALTH_ORACLE_INVALID:",
    mutate(root) {
      mutateSentinel(root, acceptancePath, acceptanceStart, acceptanceEnd, (acceptance) => {
        const oracle = acceptance.oracles.find((item) => item.oracle_id === "ORACLE-HEALTH-COMMITTED-VS-ATTEMPT");
        oracle.assertions = ["Attempt evidence may populate committed digest, equality, coverage, and source-truth fields"];
      });
    },
  },
  {
    name: "SC-024 broadens the single-path full-candidate ban to legal rebuilds",
    expected: "ERROR SC024_BOUNDARY_INVALID:",
    mutate(root) {
      mutateSentinel(root, acceptancePath, acceptanceStart, acceptanceEnd, (acceptance) => {
        const oracle = acceptance.oracles.find((item) => item.oracle_id === "ORACLE-PERFORMANCE-OBSERVED-REFRESH");
        oracle.assertions = oracle.assertions.map((assertion) => assertion.replace(
          "A single-path hint cannot trigger a full candidate outside observer Gap or ScopeDirty",
          "No full candidate is legal outside observer Gap or ScopeDirty",
        )).filter((assertion) => !assertion.startsWith("Initial indexing"));
      });
    },
  },
  {
    name: "detect_changes compatibility alias is upgraded to a query lease",
    expected: "ERROR DETECT_CHANGES_ALIAS_INVALID:",
    mutate(root) {
      mutateSentinel(root, fixturePaths[2], "<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON START -->", "<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON END -->", (retirement) => {
        const aliases = retirement.entries.find((entry) => entry.category === "compatibility_aliases");
        aliases.assertions = ["detect_changes acquires GenerationLeased and a ProjectQueryLease"];
      });
    },
  },
  {
    name: "reverse trace-to-acceptance query edge is removed",
    expected: "ERROR REVERSE_ACCEPTANCE_EDGE_INVALID:",
    mutate(root) {
      mutateSentinel(root, acceptancePath, acceptanceStart, acceptanceEnd, (acceptance) => {
        const oracle = acceptance.oracles.find((item) => item.oracle_id === "ORACLE-QUERY-ATOMIC-LEASE");
        oracle.requirement_ids = oracle.requirement_ids.filter((id) => id !== "FR-017");
      });
    },
  },
  {
    name: "preactivation retirement source anchor disappears",
    expected: "ERROR PREACTIVATION_SOURCE_ANCHOR_UNRESOLVED:",
    mutate(root) {
      const file = path.join(root, "src/gitignore_hygiene.rs");
      const text = fs.readFileSync(file, "utf8");
      fs.writeFileSync(file, text.replaceAll("atomic_replace", "removed_atomic_replace_anchor"), "utf8");
    },
  },
  {
    name: "callback anchor requires an exact nested Rust call relation",
    expected: "ERROR PREACTIVATION_SOURCE_ANCHOR_UNRESOLVED:",
    mutate(root) {
      const file = path.join(root, "src/daemon.rs");
      const text = fs.readFileSync(file, "utf8");
      const call = "live_index::persist::background_verify(bg_index, bg_root, snapshot_mtimes).await;";
      if (!text.includes(call)) throw new Error("background_verify call fixture missing");
      fs.writeFileSync(
        file,
        text.replace(call, "let background_verify = true; let _ = background_verify;"),
        "utf8",
      );
    },
  },
  {
    name: "materialization mode has no release evidence",
    expected: "ERROR RELEASE_EVIDENCE_READ:",
    args: ["--require-materialized", "--evidence", "target/ci/lifecycle-v11/missing-release-evidence.json"],
    mutate() {},
  },
  {
    name: "materialization evidence has an open or incomplete schema",
    expected: "ERROR RELEASE_EVIDENCE_SCHEMA_INVALID:",
    args: ["--require-materialized", "--evidence", "target/ci/lifecycle-v11/release-evidence.json"],
    mutate(root) {
      const file = path.join(root, "target/ci/lifecycle-v11/release-evidence.json");
      fs.mkdirSync(path.dirname(file), { recursive: true });
      fs.writeFileSync(file, "{}\n", "utf8");
    },
  },
  {
    name: "release validation command is weakened",
    expected: "ERROR RELEASE_VALIDATION_CONTRACT_INVALID:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        trace.release_validation.command = "node scripts/validate-lifecycle-oracle-traceability.cjs";
      });
    },
  },
  {
    name: "release approval reverts to a self-asserted passed flag",
    expected: "ERROR RELEASE_VALIDATION_CONTRACT_INVALID:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        trace.release_validation.approval_policy = "Evidence may self-assert that approval passed.";
      });
    },
  },
  {
    name: "release oracle execution accepts hash-only arbitrary bytes",
    expected: "ERROR RELEASE_VALIDATION_CONTRACT_INVALID:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        trace.release_validation.oracle_result_policy = "Any artifact hash is sufficient execution evidence.";
      });
    },
  },
  {
    name: "release tasks collapse to one unparsed review blob",
    expected: "ERROR RELEASE_VALIDATION_CONTRACT_INVALID:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        trace.release_validation.release_task_policy = "T078-T089 may all cite the same unparsed review.";
      });
    },
  },
  {
    name: "TEST-SURFACE reverts to the wrong Slice 4 owner",
    expected: "ERROR SURFACE_TEST_OWNER_INVALID:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        const test = trace.catalogs.tests["TEST-SURFACE"];
        test.owner_tasks = ["T058"];
        test.introduced_slice = 4;
      });
    },
  },
  {
    name: "FR-025 restores snapshot ownership instead of candidate identity preservation",
    expected: "ERROR FR025_OPAQUE_PATH_EDGE_INVALID:",
    mutate(root) {
      mutateSentinel(root, tracePath, traceStart, traceEnd, (trace) => {
        trace.requirements.find((row) => row.requirement_id === "FR-025").implementation_tasks = ["T013", "T053", "T065"];
      });
    },
  },
  {
    name: "inherited opaque-path acceptance binding is absent",
    expected: "ERROR ORACLE_ID_MISSING:",
    mutate(root) {
      mutateSentinel(root, acceptancePath, acceptanceStart, acceptanceEnd, (acceptance) => {
        acceptance.oracles = acceptance.oracles.filter((oracle) => oracle.oracle_id !== "ORACLE-OPAQUE-PATH-INHERITED");
      });
    },
  },
  {
    name: "planned Rust target is an empty test body",
    expected: "ERROR PLANNED_TEST_CASE_EMPTY:",
    mutate(root) {
      const file = path.join(root, "tests/index_candidate_lifecycle_v11.rs");
      fs.mkdirSync(path.dirname(file), { recursive: true });
      fs.writeFileSync(file, "#[test]\nfn closed_candidate_promotion_matrix() {}\n", "utf8");
    },
  },
  {
    name: "source-derived prompt inventory rejects an unlisted prompt",
    expected: "ERROR RETIREMENT_SOURCE_INVENTORY_MISMATCH: prompts",
    mutate(root) {
      const file = path.join(root, "src/protocol/prompts.rs");
      fs.appendFileSync(
        file,
        [
          "",
          "impl SymForgeServer {",
          "    #[prompt(name = \"symforge-retirement-tamper\", description = \"tamper\")]",
          "    pub(crate) async fn retirement_tamper_prompt(&self) -> GetPromptResult {",
          "        unreachable!()",
          "    }",
          "}",
          "",
        ].join("\n"),
        "utf8",
      );
    },
  },
  {
    name: "SC-024 capacity inequality is inverted into aggregate headroom",
    expected: "ERROR SC024_BOUNDARY_INVALID:",
    mutate(root) {
      mutateSentinel(root, acceptancePath, acceptanceStart, acceptanceEnd, (acceptance) => {
        const oracle = acceptance.oracles.find((item) => item.oracle_id === "ORACLE-PERFORMANCE-OBSERVED-REFRESH");
        oracle.assertions = oracle.assertions.map((assertion) => assertion.replace(
          "For every frozen capacity dimension d, retained[d] plus candidate[d] is at most pregranted[d] plus declared_scratch[d] plus declared_headroom[d]",
          "The pre-granted multidimensional vector plus scratch and retained-plus-candidate ownership remains within reserved replacement headroom",
        ));
      });
    },
  },
];

const failures = [];
const stableIdentity = () => ({ commit: "a".repeat(40), tree: "b".repeat(40), clean: true });
const runnerChecks = [
  {
    name: "verified command rejects a forged-success receipt when the process exits nonzero",
    run() {
      const result = runVerifiedCommand(
        { program: process.execPath, args: ["-e", "process.exit(0)"], timeout_ms: 1000, env: {} },
        { identity: stableIdentity, spawnSync: () => ({ status: 7, signal: null, stdout: Buffer.alloc(0), stderr: Buffer.alloc(0) }) },
      );
      return result.ok === false && result.reason === "nonzero";
    },
  },
  {
    name: "verified command always disables the shell",
    run() {
      let observedShell = null;
      const result = runVerifiedCommand(
        { program: process.execPath, args: ["-e", "process.exit(0)"], timeout_ms: 1000, env: {} },
        {
          identity: stableIdentity,
          spawnSync(program, args, options) {
            observedShell = options.shell;
            return { status: 0, signal: null, stdout: Buffer.from(`${program}:${args.length}`), stderr: Buffer.alloc(0) };
          },
        },
      );
      return result.ok === true && observedShell === false;
    },
  },
  {
    name: "verified command rejects a release tree change",
    run() {
      let calls = 0;
      const result = runVerifiedCommand(
        { program: process.execPath, args: ["-e", "process.exit(0)"], timeout_ms: 1000, env: {} },
        {
          identity() {
            calls += 1;
            return { commit: "a".repeat(40), tree: (calls === 1 ? "b" : "c").repeat(40), clean: true };
          },
          spawnSync: () => ({ status: 0, signal: null, stdout: Buffer.alloc(0), stderr: Buffer.alloc(0) }),
        },
      );
      return result.ok === false && result.reason === "tree_changed";
    },
  },
  {
    name: "verified command rejects a shell command string",
    run() {
      const result = runVerifiedCommand(
        { program: "cargo;test", args: [], timeout_ms: 1000, env: {} },
        { identity: stableIdentity },
      );
      return result.ok === false && result.reason === "invalid_spec";
    },
  },
  {
    name: "verified command executes an argv-only process and observes its exit",
    run() {
      const passed = runVerifiedCommand(
        { program: process.execPath, args: ["-e", "process.exit(0)"], timeout_ms: 5000, env: {} },
        { identity: stableIdentity },
      );
      const failed = runVerifiedCommand(
        { program: process.execPath, args: ["-e", "process.exit(9)"], timeout_ms: 5000, env: {} },
        { identity: stableIdentity },
      );
      return passed.ok === true && failed.ok === false && failed.reason === "nonzero";
    },
  },
];
for (const check of runnerChecks) {
  try {
    if (!check.run()) failures.push(`${check.name}: assertion failed`);
  } catch (error) {
    failures.push(`${check.name}: threw ${error && error.message ? error.message : "unknown error"}`);
  }
}
const baseline = runChecker(repositoryRoot);
if (baseline.error) {
  failures.push(`baseline: spawn failed (${baseline.error.code || baseline.error.message})`);
} else if (baseline.status !== 0) {
  failures.push(`baseline: checker failed (${`${baseline.stdout || ""}${baseline.stderr || ""}`.trim()})`);
}
const fixtureBaselineRoot = fs.mkdtempSync(path.join(os.tmpdir(), "symforge-lifecycle-oracle-"));
try {
  copyFixture(fixtureBaselineRoot);
  const fixtureBaseline = runChecker(fixtureBaselineRoot);
  if (fixtureBaseline.error) failures.push(`fixture baseline: spawn failed (${fixtureBaseline.error.code || fixtureBaseline.error.message})`);
  else if (fixtureBaseline.status !== 0) failures.push(`fixture baseline: checker failed (${`${fixtureBaseline.stdout || ""}${fixtureBaseline.stderr || ""}`.trim()})`);
} finally {
  safeRemoveFixture(fixtureBaselineRoot);
}
const closureLineEndingRoot = fs.mkdtempSync(path.join(os.tmpdir(), "symforge-lifecycle-oracle-"));
try {
  copyFixture(closureLineEndingRoot);
  const writerPath = path.join(closureLineEndingRoot, "src/protocol/edit.rs");
  const writerText = fs.readFileSync(writerPath, "utf8").replace(/\r\n/gu, "\n");
  fs.writeFileSync(writerPath, writerText, "utf8");
  const closureLineEnding = runChecker(closureLineEndingRoot);
  if (closureLineEnding.error) failures.push(`closure line-ending equivalence: spawn failed (${closureLineEnding.error.code || closureLineEnding.error.message})`);
  else if (closureLineEnding.status !== 0) failures.push(`closure line-ending equivalence: checker failed (${`${closureLineEnding.stdout || ""}${closureLineEnding.stderr || ""}`.trim()})`);
} finally {
  safeRemoveFixture(closureLineEndingRoot);
}
const closureCfgTestRoot = fs.mkdtempSync(path.join(os.tmpdir(), "symforge-lifecycle-oracle-"));
try {
  copyFixture(closureCfgTestRoot);
  // The census freezes V10 authority, which is what the release build contains.
  // Test-only code is compiled out, so adding it must NOT move the digest --
  // otherwise the retirement contract forbids the very edits tasks.md requires.
  // The two RETIREMENT_CLOSURE_MISMATCH cases above still cover the converse:
  // production code added to the same file does move it.
  fs.appendFileSync(
    path.join(closureCfgTestRoot, "src/protocol/edit.rs"),
    [
      "",
      "/// Test-only helper documented above its attribute, which is the",
      "/// idiomatic placement and must not move the census either.",
      '#[cfg(all(test, feature = "server"))]',
      "mod census_equivalence_probe {",
      "    #[test]",
      "    fn probe() { assert!(true); }",
      "}",
      "",
      "",
    ].join("\n"),
    "utf8",
  );
  // Prose and layout are not authority either: rewording a comment and
  // re-indenting must be invisible to the census for the same reason.
  const censusProse = path.join(closureCfgTestRoot, "src/protocol/ccr.rs");
  const proseText = fs.readFileSync(censusProse, "utf8");
  const proseLine = /^([ \t]*)\/\/[^\n]*$/mu.exec(proseText);
  if (!proseLine) throw new Error("no line comment available in the censused CCR source");
  fs.writeFileSync(
    censusProse,
    proseText.replace(proseLine[0], `${proseLine[1]}//   reworded for the census equivalence probe`),
    "utf8",
  );
  const closureCfgTest = runChecker(closureCfgTestRoot);
  if (closureCfgTest.error) failures.push(`closure cfg(test) equivalence: spawn failed (${closureCfgTest.error.code || closureCfgTest.error.message})`);
  else if (closureCfgTest.status !== 0) failures.push(`closure cfg(test) equivalence: checker failed (${`${closureCfgTest.stdout || ""}${closureCfgTest.stderr || ""}`.trim()})`);
} finally {
  safeRemoveFixture(closureCfgTestRoot);
}
const postactivationOrdinaryRoot = fs.mkdtempSync(path.join(os.tmpdir(), "symforge-lifecycle-oracle-"));
try {
  copyFixture(postactivationOrdinaryRoot);
  createPostactivationOrdinaryFixture(postactivationOrdinaryRoot);
  const postactivationOrdinary = runChecker(postactivationOrdinaryRoot);
  if (postactivationOrdinary.error) {
    failures.push(`postactivation ordinary lifecycle: spawn failed (${postactivationOrdinary.error.code || postactivationOrdinary.error.message})`);
  } else if (postactivationOrdinary.status !== 0) {
    failures.push(`postactivation ordinary lifecycle: checker failed (${`${postactivationOrdinary.stdout || ""}${postactivationOrdinary.stderr || ""}`.trim()})`);
  }
} finally {
  safeRemoveFixture(postactivationOrdinaryRoot);
}
const positiveMaterializedRoot = fs.mkdtempSync(path.join(os.tmpdir(), "symforge-lifecycle-oracle-"));
let positiveMaterializedFakeBin = null;
let materializedTamperCount = 0;
try {
  copyFixture(positiveMaterializedRoot);
  const built = buildPositiveMaterializedFixture(positiveMaterializedRoot);
  positiveMaterializedFakeBin = built.fakeBin;
  const positiveMaterialized = runChecker(
    positiveMaterializedRoot,
    ["--require-materialized", "--evidence", built.evidencePath],
    built.materializedEnvironment,
  );
  if (positiveMaterialized.error) {
    failures.push(`positive materialized fixture: spawn failed (${positiveMaterialized.error.code || positiveMaterialized.error.message})`);
  } else if (positiveMaterialized.status !== 0) {
    failures.push(`positive materialized fixture: checker failed (${`${positiveMaterialized.stdout || ""}${positiveMaterialized.stderr || ""}`.trim()})`);
  } else {
    const approvalPath = "target/ci/lifecycle-v11/refreeze-approval-result.json";
    const taskPath = "target/ci/lifecycle-v11/task-T089.json";
    const originalEvidence = fs.readFileSync(path.join(positiveMaterializedRoot, built.evidencePath));
    const originalApproval = fs.readFileSync(path.join(positiveMaterializedRoot, approvalPath));
    const originalTask = fs.readFileSync(path.join(positiveMaterializedRoot, taskPath));
    const restoreMaterialized = () => {
      fs.writeFileSync(path.join(positiveMaterializedRoot, built.evidencePath), originalEvidence);
      fs.writeFileSync(path.join(positiveMaterializedRoot, approvalPath), originalApproval);
      fs.writeFileSync(path.join(positiveMaterializedRoot, taskPath), originalTask);
    };
    const rewriteApprovalClosure = (mutate) => {
      const approval = JSON.parse(fs.readFileSync(path.join(positiveMaterializedRoot, approvalPath), "utf8"));
      mutate(approval);
      fs.writeFileSync(path.join(positiveMaterializedRoot, approvalPath), `${JSON.stringify(approval)}\n`);
      const task = JSON.parse(fs.readFileSync(path.join(positiveMaterializedRoot, taskPath), "utf8"));
      task.artifact_results.find((artifact) => artifact.path === approvalPath).sha256 = fileHash(positiveMaterializedRoot, approvalPath);
      fs.writeFileSync(path.join(positiveMaterializedRoot, taskPath), `${JSON.stringify(task)}\n`);
      const evidence = JSON.parse(fs.readFileSync(path.join(positiveMaterializedRoot, built.evidencePath), "utf8"));
      evidence.approval_verification.result_sha256 = fileHash(positiveMaterializedRoot, approvalPath);
      evidence.task_receipts.find((receipt) => receipt.task_id === "T089").artifact_sha256 = fileHash(positiveMaterializedRoot, taskPath);
      fs.writeFileSync(path.join(positiveMaterializedRoot, built.evidencePath), `${JSON.stringify(evidence)}\n`);
    };
    const runMaterializedTamper = (name, expected, mutate, evidencePath = built.evidencePath) => {
      restoreMaterialized();
      mutate();
      materializedTamperCount += 1;
      const result = runChecker(
        positiveMaterializedRoot,
        ["--require-materialized", "--evidence", evidencePath],
        built.materializedEnvironment,
      );
      const output = `${result.stdout || ""}${result.stderr || ""}`;
      if (result.error) failures.push(`${name}: spawn failed (${result.error.code || result.error.message})`);
      else if (result.status === 0) failures.push(`${name}: checker unexpectedly succeeded`);
      else if (!output.includes(expected)) failures.push(`${name}: missing ${expected}`);
    };
    runMaterializedTamper("materialized oracle receipt omission", "ERROR RELEASE_ORACLE_RECEIPT_INVALID:", () => {
      const evidence = JSON.parse(fs.readFileSync(path.join(positiveMaterializedRoot, built.evidencePath), "utf8"));
      evidence.oracle_receipts.pop();
      fs.writeFileSync(path.join(positiveMaterializedRoot, built.evidencePath), `${JSON.stringify(evidence)}\n`);
    });
    runMaterializedTamper("materialized oracle receipt duplication", "ERROR RELEASE_ORACLE_RECEIPT_INVALID:", () => {
      const evidence = JSON.parse(fs.readFileSync(path.join(positiveMaterializedRoot, built.evidencePath), "utf8"));
      evidence.oracle_receipts.push({ ...evidence.oracle_receipts.at(-1) });
      fs.writeFileSync(path.join(positiveMaterializedRoot, built.evidencePath), `${JSON.stringify(evidence)}\n`);
    });
    runMaterializedTamper("approval history root corruption", "ERROR APPROVED_REFREEZE_INVALID:", () => {
      rewriteApprovalClosure((approval) => { approval.approval_history_root_sha256 = "f".repeat(64); });
    });
    runMaterializedTamper("approval predecessor is not the last history record", "ERROR APPROVED_REFREEZE_INVALID:", () => {
      rewriteApprovalClosure((approval) => {
        approval.approval_predecessor_digest = "e".repeat(64);
        approval.approval_history_root_sha256 = domainDigest("symforge.refreeze.v11.approval-history-root", {
          approval_sequence: approval.approval_sequence,
          approval_predecessor_digest: approval.approval_predecessor_digest,
          approval_history_count: approval.approval_history_count,
          approval_history_inventory_sha256: approval.approval_history_inventory_sha256,
          current_record_sha256: approval.record_sha256,
          current_signature_sha256: approval.signature_sha256,
        });
      });
    });
    runMaterializedTamper("approval runner workflow blob is not the release workflow", "ERROR APPROVED_REFREEZE_INVALID:", () => {
      rewriteApprovalClosure((approval) => { approval.workflow_sha256 = "d".repeat(64); });
    });
    runMaterializedTamper("approval result did not come from the environment-protected gate job", "ERROR APPROVED_REFREEZE_INVALID:", () => {
      rewriteApprovalClosure((approval) => { approval.workflow_job = "gate-release-ref"; });
    });
    runMaterializedTamper("T089 output hashes diverge from protected approval execution", "ERROR RELEASE_TASK_RECEIPT_INVALID:", () => {
      const task = JSON.parse(fs.readFileSync(path.join(positiveMaterializedRoot, taskPath), "utf8"));
      task.command_results[0].stdout_sha256 = "c".repeat(64);
      fs.writeFileSync(path.join(positiveMaterializedRoot, taskPath), `${JSON.stringify(task)}\n`);
      const evidence = JSON.parse(fs.readFileSync(path.join(positiveMaterializedRoot, built.evidencePath), "utf8"));
      evidence.task_receipts.find((receipt) => receipt.task_id === "T089").artifact_sha256 = fileHash(positiveMaterializedRoot, taskPath);
      fs.writeFileSync(path.join(positiveMaterializedRoot, built.evidencePath), `${JSON.stringify(evidence)}\n`);
    });
    runMaterializedTamper("T089 cannot include an alternate containing evidence envelope", "ERROR RELEASE_TASK_RECEIPT_INVALID:", () => {
      const alternateEvidencePath = "target/ci/lifecycle-v11/release-evidence-alt.json";
      const evidence = JSON.parse(fs.readFileSync(path.join(positiveMaterializedRoot, built.evidencePath), "utf8"));
      fs.writeFileSync(path.join(positiveMaterializedRoot, alternateEvidencePath), `${JSON.stringify(evidence)}\n`);
      const task = JSON.parse(fs.readFileSync(path.join(positiveMaterializedRoot, taskPath), "utf8"));
      task.artifact_results.push({ path: alternateEvidencePath, sha256: fileHash(positiveMaterializedRoot, alternateEvidencePath), status: "passed" });
      fs.writeFileSync(path.join(positiveMaterializedRoot, taskPath), `${JSON.stringify(task)}\n`);
      evidence.task_receipts.find((receipt) => receipt.task_id === "T089").artifact_sha256 = fileHash(positiveMaterializedRoot, taskPath);
      fs.writeFileSync(path.join(positiveMaterializedRoot, alternateEvidencePath), `${JSON.stringify(evidence)}\n`);
    }, "target/ci/lifecycle-v11/release-evidence-alt.json");
    restoreMaterialized();
  }
} catch (error) {
  failures.push(`positive materialized fixture: threw ${error && error.message ? error.message : "unknown error"}`);
} finally {
  safeRemoveFixture(positiveMaterializedRoot);
  if (positiveMaterializedFakeBin !== null) safeRemoveFakeCargo(positiveMaterializedFakeBin);
}
for (const testCase of cases) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "symforge-lifecycle-oracle-"));
  try {
    copyFixture(root);
    testCase.mutate(root);
    const result = runChecker(root, testCase.args || []);
    const output = `${result.stdout || ""}${result.stderr || ""}`;
    if (result.error) failures.push(`${testCase.name}: spawn failed (${result.error.code || result.error.message})`);
    else if (result.status === 0) failures.push(`${testCase.name}: checker unexpectedly succeeded`);
    else if (!output.includes(testCase.expected)) failures.push(`${testCase.name}: missing ${testCase.expected}`);
  } finally {
    safeRemoveFixture(root);
  }
}

if (failures.length > 0) {
  for (const failure of failures.sort()) process.stderr.write(`ERROR SELF_TEST: ${failure}\n`);
  process.exitCode = 1;
} else {
  process.stdout.write(`lifecycle oracle traceability v11 self-test: OK (${cases.length + runnerChecks.length + materializedTamperCount} fail-closed cases)\n`);
}
