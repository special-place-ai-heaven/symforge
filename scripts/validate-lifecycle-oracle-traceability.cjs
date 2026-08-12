#!/usr/bin/env node
"use strict";

const fs = require("node:fs");
const crypto = require("node:crypto");
const path = require("node:path");
const childProcess = require("node:child_process");

const TRACE_PATH = "specs/020-repository-knowledge-index/contracts/lifecycle-oracle-traceability-v11.md";
const ACCEPTANCE_PATH = "specs/020-repository-knowledge-index/contracts/lifecycle-acceptance-oracles-v11.md";
const RETIREMENT_PATH = "specs/020-repository-knowledge-index/contracts/v10-authority-retirement-v11.md";
const TASKS_PATH = "specs/020-repository-knowledge-index/tasks.md";
const PUBLIC_API_PATH = "specs/020-repository-knowledge-index/contracts/public-api-v11.json";
const CHECKER_PATH = "scripts/validate-lifecycle-oracle-traceability.cjs";
const RELEASE_GATE_REVIEW_PATH = "docs/reviews/FEATURE-020-V11-RELEASE-GATE.md";
const RELEASE_WORKFLOW_PATH = ".github/workflows/release.yml";
// `github.job` is the job_id, not the display name, and release.yml is byte-pinned by
// workflow_sha256 above. Pinning it proves the approval result came from the one job
// that declares `environment: feature-020-v11-release-approval`.
const APPROVAL_GATE_JOB_ID = "feature-020-v11-gate";
const PLANNED = "planned_not_executed";
const CAPACITY_DIMENSIONS = [
  "process_slots",
  "project_slots",
  "source_slots",
  "residency_bytes",
  "replacement_headroom_bytes",
  "response_reservation_bytes",
];

const EXPECTED_REQUIREMENTS = [
  ...Array.from({ length: 52 }, (_, index) => `FR-${String(index + 1).padStart(3, "0")}`),
  ...Array.from({ length: 26 }, (_, index) => `SC-${String(index + 1).padStart(3, "0")}`),
];

const EXPECTED_ACCEPTANCE_ORACLES = new Map([
  ["ORACLE-MUTATION-AUTHORITY-FOUNDATION", ["mutation", 1, "TEST-MUTATION-AUTHORITY"]],
  ["ORACLE-MUTATION-WRITE-INTENT", ["mutation", 4, "TEST-MUTATION"]],
  ["ORACLE-INGRESS-CLOSED-SURFACE", ["ingress", 4, "TEST-SURFACE"]],
  ["ORACLE-HEALTH-COMMITTED-VS-ATTEMPT", ["health", 4, "TEST-HEALTH"]],
  ["ORACLE-OBSERVER-STABLE-CUT", ["observer", 4, "TEST-OBSERVER"]],
  ["ORACLE-CAPACITY-PHYSICAL-OWNERSHIP", ["capacity", 2, "TEST-CAPACITY"]],
  ["ORACLE-REGISTRY-IDENTITY-ABA", ["registry", 4, "TEST-REGISTRY"]],
  ["ORACLE-PHYSICAL-ROOT-CONFINEMENT", ["physical_root", 1, "TEST-PHYSICAL-ROOT"]],
  ["ORACLE-OPAQUE-PATH-IDENTITY", ["opaque_path", 4, "TEST-OPAQUE-PATH"]],
  ["ORACLE-OPAQUE-PATH-INHERITED", ["opaque_path_inherited", 0, "TEST-OPAQUE-PATH-INHERITED"]],
  ["ORACLE-QUERY-ATOMIC-LEASE", ["query", 4, "TEST-QUERY"]],
  ["ORACLE-PROVENANCE-OPERATION-MATRIX", ["provenance", 3, "TEST-PROVENANCE"]],
  ["ORACLE-VERIFICATION-COMPLETE-CANDIDATE", ["verification", 4, "TEST-CANDIDATE"]],
  ["ORACLE-ACTIVATION-SINGLE-AUTHORITY", ["activation", 4, "TEST-ACTIVATION"]],
  ["ORACLE-EMBED-ONE-HANDLE", ["embed", 4, "TEST-EMBED"]],
  ["ORACLE-MIGRATION-DELTA-EQUIVALENCE", ["migration", 4, "TEST-DELTA"]],
  ["ORACLE-PUBLICATION-WHOLE-ROOT", ["publication", 4, "TEST-PUBLICATION"]],
  ["ORACLE-KNOWLEDGE-CURRENT-PROJECTION", ["knowledge", 4, "TEST-KNOWLEDGE"]],
  ["ORACLE-PERFORMANCE-OBSERVED-REFRESH", ["performance", 4, "TEST-PERFORMANCE"]],
  ["ORACLE-SNAPSHOT-UNTRUSTED-SEED", ["snapshot", 4, "TEST-SNAPSHOT"]],
  ["ORACLE-STATE-TYPED-OWNERS", ["state", 4, "TEST-STATE"]],
  ["ORACLE-ROLLING-VERIFICATION-COVERAGE", ["rolling_verification", 4, "TEST-ROLLING-VERIFICATION"]],
  ["ORACLE-CAPACITY-RUNTIME-INTEGRATION", ["capacity_runtime", 4, "TEST-CAPACITY-INTEGRATION"]],
  ["ORACLE-EMBED-FOUNDATION", ["embed_foundation", 2, "TEST-EMBED-FOUNDATION"]],
]);

const EXPECTED_AMENDMENT_REGRESSION_BINDINGS = new Map([
  ["F020-V11-R01", "ORACLE-VERIFICATION-COMPLETE-CANDIDATE"],
  ["F020-V11-R02", "ORACLE-SNAPSHOT-UNTRUSTED-SEED"],
  ["F020-V11-R03", "ORACLE-INGRESS-CLOSED-SURFACE"],
  ["F020-V11-R04", "ORACLE-VERIFICATION-COMPLETE-CANDIDATE"],
  ["F020-V11-R05", "ORACLE-VERIFICATION-COMPLETE-CANDIDATE"],
  ["F020-V11-R06", "ORACLE-VERIFICATION-COMPLETE-CANDIDATE"],
  ["F020-V11-R07", "ORACLE-VERIFICATION-COMPLETE-CANDIDATE"],
  ["F020-V11-R08", "ORACLE-HEALTH-COMMITTED-VS-ATTEMPT"],
  ["F020-V11-R09", "ORACLE-VERIFICATION-COMPLETE-CANDIDATE"],
  ["F020-V11-R10", "ORACLE-KNOWLEDGE-CURRENT-PROJECTION"],
  ["F020-V11-R11", "ORACLE-KNOWLEDGE-CURRENT-PROJECTION"],
  ["F020-V11-R12", "ORACLE-REGISTRY-IDENTITY-ABA"],
  ["F020-V11-R13", "ORACLE-QUERY-ATOMIC-LEASE"],
  ["F020-V11-R14", "ORACLE-KNOWLEDGE-CURRENT-PROJECTION"],
  ["F020-V11-R15", "ORACLE-VERIFICATION-COMPLETE-CANDIDATE"],
  ["F020-V11-R16", "ORACLE-HEALTH-COMMITTED-VS-ATTEMPT"],
  ["F020-V11-R17", "ORACLE-ROLLING-VERIFICATION-COVERAGE"],
  ["F020-V11-R18A", "ORACLE-MIGRATION-DELTA-EQUIVALENCE"],
  ["F020-V11-R18B", "ORACLE-PERFORMANCE-OBSERVED-REFRESH"],
  ["F020-V11-R18C", "ORACLE-CAPACITY-RUNTIME-INTEGRATION"],
  ["F020-V11-R19A", "ORACLE-QUERY-ATOMIC-LEASE"],
  ["F020-V11-R19B", "ORACLE-KNOWLEDGE-CURRENT-PROJECTION"],
]);

const RETIREMENT_CATEGORIES = [
  "writers",
  "callbacks",
  "publication_roots",
  "cache",
  "ccr",
  "snapshot",
  "tools",
  "resources",
  "prompts",
  "sidecar",
  "hooks",
  "compatibility_aliases",
  "raw_embed",
];
const RETIREMENT_CLOSURE_CATEGORIES = ["writers", "callbacks", "publication_roots", "cache", "ccr"];

const EXPECTED_COMPATIBILITY_ALIASES = ["detect_changes", "trace_symbol"];

const RETIREMENT_MEMBER_DIGESTS = new Map([
  ["writers", "3ee16991d9c6900f8921e45fd56f604c314f1752765ef389b9f30db73581d256"],
  ["callbacks", "61161d04fa4189c6d2cf2a456655582b468a3c6d071046f956b8a7f3426928e8"],
  ["publication_roots", "379565fc39ae363ce880b8a9601193b56096041bc1d8241ca26c6cb95a4b1749"],
  ["cache", "112eeb2b9a7712d97a311aa483ba0ca74f64e51ab277e83ebce682415622c79b"],
  ["ccr", "2f7cc4b3223e192fd7bb1aa4d145813100b07b438409590b63adeb6b4e6c4ebe"],
  ["snapshot", "8224b2e7df353d6175d45bbd20eafcee35f3c68fc7b3d28b9130228108c32d33"],
  ["tools", "2a012f92472588eaa96705a4655d63e654daf387fa9870bfecfe82f7b92cdad0"],
  ["resources", "79efb3025090455b6afe0cee855933104d8e1a3a3de59b9cd9be935a7f743b52"],
  ["prompts", "ec3f7d5a8b64415fb2041455ffd1e5bb65b9eaee09625ac080e71a1fb136df06"],
  ["sidecar", "72cedae6372744a8d92fc9d5fb7c654ca1ea289e65421acee9e01367e29be7bb"],
  ["hooks", "a38ec819823aea7670db24cc0382701e99258b6037eacf0c15c690b2a5a0721b"],
  ["compatibility_aliases", "1a1a3d3816d33b92e2b868ebf06c098615dee5e15b5eb296bbec5b6c61c190a5"],
]);

const EXPECTED_RETIREMENT_OWNERS = new Map([
  ["writers", ["T064", "T065", "T067"]],
  ["callbacks", ["T064", "T065", "T067"]],
  ["publication_roots", ["T066", "T067"]],
  ["cache", ["T066", "T067"]],
  ["ccr", ["T066", "T067"]],
  ["snapshot", ["T065", "T067"]],
  ["tools", ["T066", "T067"]],
  ["resources", ["T066", "T067"]],
  ["prompts", ["T066", "T067"]],
  ["sidecar", ["T064", "T066", "T067"]],
  ["hooks", ["T064", "T066", "T067"]],
  ["compatibility_aliases", ["T066", "T067"]],
  ["raw_embed", ["T067"]],
]);

const EXPECTED_SEAMS = {
  "SEAM-ACTIVATION": ["src/index_lifecycle/activation.rs::ActivationCut", "src/index_lifecycle/public_api.rs::V11PublicApi"],
  "SEAM-CANDIDATE": ["src/index_lifecycle/authority.rs::CandidateAuthority", "src/index_lifecycle/candidate.rs::CandidateHandle", "src/index_lifecycle/supervisor.rs::SourceSupervisor"],
  "SEAM-CAPACITY": ["src/index_lifecycle/capacity.rs::CapacityPermit", "src/index_lifecycle/capacity.rs::ProcessCapacityPool", "src/index_lifecycle/process_runtime.rs::ProcessIndexRuntime"],
  "SEAM-EMBED": ["src/index_lifecycle/embedded.rs::EmbeddedSourceFactory", "src/index_lifecycle/embedded.rs::EmbeddedSourceHandle", "src/index_lifecycle/process_runtime.rs::ProcessIndexRuntime"],
  "SEAM-HEALTH": ["src/index_lifecycle/query.rs::RuntimeHealthObservation", "src/live_index/health_view.rs::AttemptHealth", "src/live_index/health_view.rs::CommittedGenerationHealth"],
  "SEAM-KNOWLEDGE": ["src/index_lifecycle/query.rs::ProjectQueryLease", "src/protocol/claim_provenance.rs::ClaimProvenance", "src/protocol/read_gate.rs::ReadGate"],
  "SEAM-MUTATION": ["src/index_lifecycle/authority.rs::CurrentMutationGrantAuthority", "src/index_lifecycle/mutation.rs::SourceMutationPermit", "src/index_lifecycle/physical_root.rs::PhysicalRootLease"],
  "SEAM-OBSERVER": ["src/index_lifecycle/observer.rs::ObserverHandoff", "src/index_lifecycle/observer.rs::ObserverHealth", "src/index_lifecycle/supervisor.rs::SourceSupervisor"],
  "SEAM-OPAQUE-PATH": ["src/discovery/mod.rs::catalog_path_projection", "src/domain/index.rs::CatalogPath"],
  "SEAM-PERFORMANCE": ["src/index_lifecycle/candidate.rs::CandidateHandle", "src/index_lifecycle/capacity.rs::ProcessCapacityPool", "src/index_lifecycle/observer.rs::ObserverHandoff", "src/index_lifecycle/runtime.rs::ProjectIndexRuntime"],
  "SEAM-PROVENANCE": ["src/protocol/claim_provenance.rs::ClaimProvenance", "src/protocol/claim_provenance.rs::OperationReceipt", "src/protocol/read_gate.rs::ReadGate"],
  "SEAM-PUBLICATION": ["src/index_lifecycle/candidate.rs::CandidateCommit", "src/index_lifecycle/runtime.rs::ProjectIndexRuntime", "src/index_lifecycle/runtime.rs::ProjectPublicationRoot"],
  "SEAM-QUERY": ["src/index_lifecycle/query.rs::ProjectQueryLease", "src/index_lifecycle/query.rs::QuerySelection", "src/protocol/read_gate.rs::ReadGate"],
  "SEAM-REGISTRY": ["src/index_lifecycle/authority.rs::BindingAuthority", "src/index_lifecycle/physical_root.rs::PhysicalRootLease", "src/index_lifecycle/registry.rs::LiveProjectSlot", "src/index_lifecycle/registry.rs::ProjectRegistry"],
  "SEAM-SNAPSHOT": ["src/live_index/persist.rs::IndexSnapshot", "src/live_index/persist.rs::checkpoint_shared_index", "src/live_index/persist.rs::load_snapshot_for_root"],
  "SEAM-STATE": ["src/index_lifecycle/query.rs::CheckpointAvailability", "src/index_lifecycle/runtime.rs::ProjectStateDir", "src/index_lifecycle/runtime.rs::TeamArtifactState"],
  "SEAM-SURFACE": ["src/index_lifecycle/activation.rs::ActivationCut", "src/index_lifecycle/public_api.rs::V11PublicApi", "src/index_lifecycle/query.rs::ProjectQueryLease"],
};

const EXPECTED_PRODUCTION_SEAMS = new Set([
  ...Object.values(EXPECTED_SEAMS).flat(),
  "src/index_lifecycle/verification.rs::RollingVerification",
  "src/index_lifecycle/verification.rs::VerificationFeasibilityReceipt",
  "src/index_lifecycle/verification.rs::VerificationRecord",
  "src/index_lifecycle/verification.rs::VerificationScopeReceipt",
  "src/index_lifecycle/verification.rs::VerificationWorkBound",
  "src/live_index/persist.rs::load_snapshot",
]);

const REQUIRED_TEST_EDGES = new Map([
  ["FR-020", ["TEST-EMBED", "TEST-EMBED-FOUNDATION"]],
  ["FR-021", ["TEST-HEALTH"]],
  ["FR-025", ["TEST-OPAQUE-PATH", "TEST-OPAQUE-PATH-INHERITED"]],
  ["FR-041", ["TEST-ACTIVATION", "TEST-REGISTRY"]],
  ["FR-042", ["TEST-QUERY", "TEST-REGISTRY"]],
  ["SC-006", ["TEST-KNOWLEDGE", "TEST-QUERY"]],
  ["SC-017", ["TEST-QUERY", "TEST-REGISTRY"]],
  ["SC-024", ["TEST-CAPACITY-INTEGRATION", "TEST-DELTA", "TEST-PERFORMANCE"]],
  ["SC-025", ["TEST-CAPACITY", "TEST-CAPACITY-INTEGRATION", "TEST-EMBED-FOUNDATION"]],
]);

const REQUIRED_TASK_EDGES = new Map([
  ["FR-021", ["T056", "T063"]],
  ["FR-025", ["T013", "T053", "T060"]],
  ["FR-041", ["T058", "T064"]],
  ["FR-042", ["T056", "T064"]],
  ["SC-006", ["T081"]],
  ["SC-017", ["T056", "T064"]],
  ["SC-024", ["T068", "T069", "T070", "T071"]],
  ["SC-025", ["T069"]],
]);

const REQUIRED_SLICE4 = new Set(["FR-025", "FR-041", "FR-042", "SC-017", "SC-024", "SC-025"]);

const EXPECTED_TRACE_TEST_IDS = [
  ...new Set([...EXPECTED_ACCEPTANCE_ORACLES.values()].map((value) => value[2]).concat("TEST-OPAQUE-PATH-INHERITED")),
].sort();
const EXPECTED_BOUND_IDS = ["BOUND-ARTIFACT", "BOUND-CAPACITY", "BOUND-QUERY", "BOUND-REPLAY", "BOUND-SOURCE", "BOUND-VERIFICATION"];
const EXPECTED_FAIRNESS_IDS = ["FAIR-CANCEL", "FAIR-OBSERVER", "FAIR-PROJECT", "FAIR-RETRY"];
const EXPECTED_CI_ARTIFACT_IDS = ["CI-RELEASE", "CI-SLICE0", "CI-SLICE1", "CI-SLICE2", "CI-SLICE3", "CI-SLICE4"];
const EXPECTED_RELEASE_VALIDATION = {
  mode: "require_materialized",
  command: "node scripts/validate-lifecycle-oracle-traceability.cjs --require-materialized --evidence target/ci/lifecycle-v11/release-evidence.json",
  evidence_path: "target/ci/lifecycle-v11/release-evidence.json",
  owner_tasks: ["T020", "T089", "T090"],
  required_task_receipts: ["T078", "T079", "T080", "T081", "T082", "T083", "T084", "T085", "T086", "T087", "T088", "T089"],
  planned_case_policy: "Every planned_exact and inherited_exact Rust target must exist as the exact named test in the release tree and have one same-tree source receipt.",
  planned_benchmark_policy: "Every planned_benchmark target must exist with its frozen registration and command plus one closed code-owned same-tree receipt proving semantic equivalence, both completed-write boundaries, exact first strict byte identity, and for each frozen process_slots/project_slots/source_slots/residency_bytes/replacement_headroom_bytes/response_reservation_bytes dimension retained plus candidate at most pregranted plus declared scratch plus declared headroom; p95 <=2s, max <=5s, ratio <=1.25x, zero forbidden single-path full candidates, legal initial/manual/recovery/Gap/ScopeDirty triggers, and completion/corpus/environment digests.",
  source_anchor_policy: "Every preactivation V10 src/ retirement member resolves on the refreeze tree; release evidence instead covers every frozen V11 production seam with one same-tree source receipt after retirement.",
  same_tree_policy: "Every requirement, release-task, case, benchmark, V11 production-source, contract, and checker receipt binds the current clean Git release tree; V10 preactivation anchors resolve directly from the externally approved refreeze tree whose verified commit is a strict ancestor of the release commit.",
  approval_policy: "Materialized validation consumes the closed T089 result emitted only after the byte-frozen protected release workflow actually runs verify-approval; the result binds the actual approval input hashes, append-only history, exact argv and raw stdout/stderr hashes, workflow identity/blob/commit/run/job, approved commit/tree and release commit/tree, and T089 preserves that exact result hash before T090. Raw trust paths and secret values are removed before T090 and never enter release evidence.",
  materialized_tool_environment: ["SYMFORGE_LIFECYCLE_GIT_EXECUTABLE", "SYMFORGE_LIFECYCLE_CARGO_EXECUTABLE"],
  oracle_result_policy: "Every acceptance oracle owns one closed result artifact binding its exact oracle/test/command/requirement identity, hashes of every frozen semantic field, passed positive/negative/assertion/test controls, and the release commit/tree; arbitrary or hash-only bytes are not execution evidence.",
  release_task_policy: "Every T078-T089 receipt owns a distinct closed task result artifact binding the exact task declaration and code-owned command ID, zero-exit output hashes, verified artifact-result hashes including the same-tree release-gate review, and the release commit/tree; that review contains one exact passed marker for each completed T078-T088 gate, while T089 additionally binds the external-approval result and must not list its containing release-evidence envelope as an artifact, preventing a hash cycle.",
  execution_policy: "Materialized validation derives argv only from each frozen Rust target and kind, runs every planned or inherited exact case and benchmark through the absolute outside-repository Cargo executable with shell=false in the clean pinned release tree, requires unchanged commit/tree/clean state, and requires a freshly created code-owned command receipt binding the exact target, command, and current oracle artifact hashes; evidence JSON alone is never execution proof.",
  evidence_contract: {
    kind: "symforge.lifecycle_release_evidence.v11",
    schema_version: 1,
    top_level_fields: ["kind", "schema_version", "release_commit", "release_tree", "approved_refreeze_commit", "approved_refreeze_tree", "approval_verification", "trace_contract_sha256", "acceptance_contract_sha256", "retirement_contract_sha256", "checker_sha256", "requirement_receipts", "oracle_receipts", "task_receipts", "rust_case_receipts", "benchmark_receipts", "source_receipts", "status"],
    approval_verification_fields: ["status", "result_artifact", "result_sha256"],
    approval_result_fields: ["kind", "schema_version", "approved_commit", "approved_tree", "release_commit", "release_tree", "verifier_sha256", "record_sha256", "signature_sha256", "allowed_signers_sha256", "release_identity_sha256", "approval_sequence", "approval_predecessor_digest", "approval_history_inventory", "approval_history_count", "approval_history_inventory_sha256", "approval_history_root_sha256", "command_argv_sha256", "expected_repository", "external_inputs", "command_id", "exit_code", "stdout_sha256", "stderr_sha256", "runner_kind", "runner_repository", "workflow_path", "workflow_sha256", "workflow_commit", "workflow_run_id", "workflow_run_attempt", "workflow_job", "workflow_event", "status"],
    approval_history_entry_fields: ["sequence", "record_sha256", "signature_sha256"],
    requirement_receipt_fields: ["requirement_id", "oracle_ids", "status", "release_tree"],
    oracle_receipt_fields: ["oracle_id", "artifact", "artifact_sha256", "status", "release_tree"],
    task_receipt_fields: ["task_id", "status", "release_tree", "artifact", "artifact_sha256"],
    rust_case_receipt_fields: ["test_id", "target", "command", "status", "release_tree", "source_sha256"],
    benchmark_receipt_fields: ["test_id", "target", "command", "registration", "status", "release_tree", "source_sha256", "receipt", "receipt_sha256"],
    source_receipt_fields: ["anchor", "seam_ids", "status", "release_tree", "source_sha256"],
    materialized_command_receipt_fields: ["kind", "schema_version", "release_commit", "release_tree", "test_id", "target", "command", "artifact_results", "status"],
  },
};

const EXPECTED_RELEASE_TASK_COMMAND_IDS = new Map([
  ["T078", "format-and-clippy"],
  ["T079", "focused-lifecycle-suites"],
  ["T080", "model-formal-and-loom"],
  ["T081", "serial-all-target-and-token-gate"],
  ["T082", "race-and-observer-campaigns"],
  ["T083", "concurrent-project-memory-gate"],
  ["T084", "provenance-refusal-and-secret-canary"],
  ["T085", "activation-and-restart-campaigns"],
  ["T086", "public-api-and-cfg-gate"],
  ["T087", "secret-safety-scan"],
  ["T088", "freeze-and-adversarial-review"],
  ["T089", "refreeze-approval-and-evidence"],
]);

const EXPECTED_TOOLS = [
  "analyze_file_impact", "ask", "batch_edit", "batch_insert", "batch_rename",
  "checkpoint_now", "context_inventory", "conventions", "curate_knowledge",
  "delete_symbol", "detect_impact", "diff_symbols", "edit_plan",
  "edit_within_symbol", "explore", "find_dependents", "find_references",
  "get_file_content", "get_file_context", "get_repo_map", "get_symbol",
  "get_symbol_context", "health", "health_compact", "index_folder",
  "insert_symbol", "inspect_match", "investigation_suggest",
  "replace_symbol_body", "review_knowledge", "search_files", "search_knowledge",
  "search_symbols", "search_text", "status", "symforge_edit",
  "symforge_retrieve", "validate_file_syntax", "what_changed",
];
const EXPECTED_RETIREMENT_TOOLS = [...EXPECTED_TOOLS, "symforge"].sort();

const EXPECTED_RESOURCES = [
  "symforge://file/content",
  "symforge://file/context",
  "symforge://glossary",
  "symforge://repo/changes/uncommitted",
  "symforge://repo/health",
  "symforge://repo/map",
  "symforge://repo/outline",
  "symforge://symbol/context",
  "symforge://symbol/detail",
  "symforge://tools/catalog",
];

const EXPECTED_PROMPTS = [
  "symforge-admin",
  "symforge-architecture",
  "symforge-debug",
  "symforge-knowledge-hygiene",
  "symforge-onboard",
  "symforge-refactor",
  "symforge-review",
  "symforge-triage",
];

const FROZEN_DIGESTS = {
  catalogs: {
    domain: "symforge.lifecycle.v11.trace.catalogs",
    hash: "e5b5080ede1761ec9f2d4d265dc352b6e41192780e7b00d9246393cca8bdc5b4",
  },
  requirement_rows: {
    domain: "symforge.lifecycle.v11.trace.requirement_rows",
    hash: "a76397f3d24e6e7d7524347853fe35562c635585e40df2b26248ecd3a6a11f4d",
  },
  invariants: {
    domain: "symforge.lifecycle.v11.trace.invariants",
    hash: "10108e816eb880c2752f473ff8c1ba12fb4bf8d981bc6babd1d8a733daf5cff0",
  },
  state_models: {
    domain: "symforge.lifecycle.v11.trace.state_models",
    hash: "7293af9fdf456a51dcf678be233f7930fbe1cbc999d3f16e573b6266f0cca659",
  },
  release_validation: {
    domain: "symforge.lifecycle.v11.trace.release_validation",
    hash: "9e2645ae73d6e75476a5c1a6e1c6fcd062a13df9f412dd5e55dcfbd0fd00cb96",
  },
  acceptance_oracles: {
    domain: "symforge.lifecycle.v11.acceptance.oracles",
    hash: "d1d47c59a3a23952e4e598e8c44b5f33778915a744c51d4c3b2ec58e07b84fec",
  },
  retirement_records: {
    domain: "symforge.lifecycle.v11.retirement.records",
    hash: "6da0f3adc4a4673c9be00af811b5f3fd2cb39b15d7e9fd6067db7d97634f1e27",
  },
  retirement_edges: {
    domain: "symforge.lifecycle.v11.retirement.edges",
    hash: "076bcc43ef2ed32e8d4d80dfee0c890291791c31ea1735122c6d5fba3472ec86",
  },
};

const FROZEN_DIGEST_KEYS = [
  "catalogs",
  "requirement_rows",
  "invariants",
  "state_models",
  "release_validation",
  "acceptance_oracles",
  "retirement_records",
  "retirement_edges",
];

// Executed non-Rust cases are never resolved by extension or by a contract-
// supplied symbol claim. Each future entry must be code-reviewed here as an
// exact target -> resolver id -> command triple.
const EXECUTED_NON_RUST_RESOLVERS = new Map([]);

const errors = [];

function fail(code, detail) {
  errors.push(`ERROR ${code}: ${detail}`);
}

function parseCli(argv) {
  let root = process.cwd();
  let rootSeen = false;
  let requireMaterialized = false;
  let evidence = null;
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--root" && !rootSeen && index + 1 < argv.length && argv[index + 1].trim() !== "") {
      root = path.resolve(argv[index + 1]);
      rootSeen = true;
      index += 1;
    } else if (argument === "--require-materialized" && !requireMaterialized) {
      requireMaterialized = true;
    } else if (argument === "--evidence" && evidence === null && index + 1 < argv.length && argv[index + 1].trim() !== "") {
      evidence = argv[index + 1];
      index += 1;
    } else {
      fail("CLI_USAGE", `unknown, duplicate, or incomplete argument: ${String(argument)}`);
    }
  }
  if (requireMaterialized !== (evidence !== null)) {
    fail("CLI_USAGE", "--require-materialized and --evidence <release-evidence.json> must be supplied together");
  }
  if (evidence !== null && (path.isAbsolute(evidence) || evidence.includes("..") || !/^target\/ci\/lifecycle-v11\/[A-Za-z0-9_.-]+\.json$/u.test(evidence))) {
    fail("RELEASE_EVIDENCE_PATH_INVALID", String(evidence));
  }
  return { root, requireMaterialized, evidence };
}

const cli = require.main === module
  ? parseCli(process.argv.slice(2))
  : { root: process.cwd(), requireMaterialized: false, evidence: null };
const repositoryRoot = cli.root;

function readText(relativePath) {
  try {
    return fs.readFileSync(path.join(repositoryRoot, relativePath), "utf8");
  } catch (error) {
    fail("FILE_READ", `${relativePath}: ${error.code || "read_failed"}`);
    return null;
  }
}

function isRegularFile(relativePath) {
  try {
    return fs.statSync(path.join(repositoryRoot, relativePath)).isFile();
  } catch {
    return false;
  }
}

function readBytes(relativePath) {
  try {
    return fs.readFileSync(path.join(repositoryRoot, relativePath));
  } catch {
    return null;
  }
}

function sha256Bytes(value) {
  return crypto.createHash("sha256").update(value).digest("hex");
}

function validSha256(value) {
  return typeof value === "string" && /^[0-9a-f]{64}$/u.test(value);
}

function safeArtifactPath(value) {
  return typeof value === "string" &&
    /^(?:target\/ci\/lifecycle-v11|docs\/reviews)\/[A-Za-z0-9_./-]+$/u.test(value) &&
    !value.includes("..") &&
    !value.includes("\\") &&
    path.posix.normalize(value) === value;
}

function neutralGitEnvironment() {
  const environment = { ...process.env, GIT_NO_REPLACE_OBJECTS: "1", GIT_OPTIONAL_LOCKS: "0" };
  const exactOverrides = new Set([
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_COMMON_DIR",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CEILING_DIRECTORIES",
    "GIT_DISCOVERY_ACROSS_FILESYSTEM",
    "GIT_CONFIG",
    "GIT_CONFIG_GLOBAL",
    "GIT_CONFIG_SYSTEM",
    "GIT_CONFIG_NOSYSTEM",
    "GIT_CONFIG_COUNT",
  ]);
  for (const key of Object.keys(environment)) {
    if (exactOverrides.has(key) || /^GIT_CONFIG_(?:KEY|VALUE)_\d+$/u.test(key)) delete environment[key];
  }
  return environment;
}

function canonicalExternalPath(value, kind) {
  if (typeof value !== "string" || value.trim() === "" || !path.isAbsolute(value) || value.includes("\0")) return null;
  let resolved;
  let root;
  let stat;
  try {
    resolved = fs.realpathSync(value);
    root = fs.realpathSync(repositoryRoot);
    stat = fs.statSync(resolved);
  } catch {
    return null;
  }
  const rootPrefix = `${root.toLowerCase()}${path.sep}`;
  const resolvedLower = resolved.toLowerCase();
  if (resolvedLower === root.toLowerCase() || resolvedLower.startsWith(rootPrefix)) return null;
  if (kind === "file" && !stat.isFile()) return null;
  if (kind === "directory" && !stat.isDirectory()) return null;
  return resolved;
}

function materializedEnvironmentPath(name, kind) {
  return cli.requireMaterialized ? canonicalExternalPath(process.env[name], kind) : null;
}

function runGit(argumentsList) {
  const program = cli.requireMaterialized
    ? materializedEnvironmentPath("SYMFORGE_LIFECYCLE_GIT_EXECUTABLE", "file")
    : "git";
  if (program === null) return { ok: false, stdout: Buffer.alloc(0) };
  const result = childProcess.spawnSync(program, argumentsList, {
    cwd: repositoryRoot,
    encoding: null,
    windowsHide: true,
    shell: false,
    maxBuffer: 64 * 1024 * 1024,
    env: neutralGitEnvironment(),
  });
  return {
    ok: !result.error && result.status === 0 && Buffer.isBuffer(result.stdout),
    stdout: Buffer.isBuffer(result.stdout) ? result.stdout : Buffer.alloc(0),
  };
}

function gitText(argumentsList) {
  const result = runGit(argumentsList);
  return result.ok ? result.stdout.toString("utf8").trim() : null;
}

function gitBlob(commit, relativePath) {
  if (typeof commit !== "string" || !/^[0-9a-f]{40,64}$/u.test(commit)) return null;
  if (typeof relativePath !== "string" || !/^(?:\.github|src|tests|benches|scripts|execution|specs|docs)\/[A-Za-z0-9_./-]+$/u.test(relativePath) ||
      relativePath.includes("..") || relativePath.includes("\\") || path.posix.normalize(relativePath) !== relativePath) {
    return null;
  }
  const result = runGit(["cat-file", "blob", commit + ":" + relativePath]);
  return result.ok ? result.stdout : null;
}

function countOccurrences(text, needle) {
  let count = 0;
  let offset = 0;
  while ((offset = text.indexOf(needle, offset)) !== -1) {
    count += 1;
    offset += needle.length;
  }
  return count;
}

function parseSentinel(relativePath, heading, start, end) {
  const text = readText(relativePath);
  if (text === null) {
    return null;
  }
  const headingCount = text.split(/\r?\n/u).filter((line) => line === heading).length;
  if (headingCount !== 1) {
    fail("HEADING_COUNT", `${relativePath}: ${heading} appears ${headingCount} times`);
  }
  const startCount = countOccurrences(text, start);
  const endCount = countOccurrences(text, end);
  if (startCount !== 1 || endCount !== 1) {
    fail("SENTINEL_COUNT", `${relativePath}: start=${startCount}, end=${endCount}`);
    return null;
  }
  const startAt = text.indexOf(start) + start.length;
  const endAt = text.indexOf(end);
  if (endAt <= startAt) {
    fail("SENTINEL_ORDER", `${relativePath}: end sentinel does not follow start sentinel`);
    return null;
  }
  const fenced = text.slice(startAt, endAt).trim();
  const match = /^```json\r?\n([\s\S]*?)\r?\n```$/u.exec(fenced);
  if (!match) {
    fail("SENTINEL_FENCE", `${relativePath}: sentinel body must be one json fence`);
    return null;
  }
  try {
    const parsed = JSON.parse(match[1]);
    if (!isObject(parsed)) {
      fail("SENTINEL_ROOT_INVALID", `${relativePath}: sentinel root must be an object`);
      return null;
    }
    return parsed;
  } catch (error) {
    fail("JSON_PARSE", `${relativePath}: ${error.message}`);
    return null;
  }
}

function isObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function exactKeys(value, expected, context) {
  if (!isObject(value)) {
    fail("SCHEMA_OBJECT", `${context}: expected object`);
    return false;
  }
  let valid = true;
  const actual = Object.keys(value);
  for (const key of expected) {
    if (!Object.prototype.hasOwnProperty.call(value, key)) {
      fail("SCHEMA_MISSING_KEY", `${context}: ${key}`);
      valid = false;
    }
  }
  for (const key of actual) {
    if (!expected.includes(key)) {
      fail("SCHEMA_UNKNOWN_KEY", `${context}: ${key}`);
      valid = false;
    }
  }
  return valid;
}

function nonEmptyString(value, context) {
  if (typeof value !== "string" || value.trim() === "") {
    fail("STRING_INVALID", context);
    return false;
  }
  return true;
}

function stringArray(value, context, allowEmpty = false) {
  if (!Array.isArray(value)) {
    fail("ARRAY_INVALID", `${context}: expected array`);
    return [];
  }
  if (!allowEmpty && value.length === 0) {
    fail("ARRAY_EMPTY", context);
  }
  const seen = new Set();
  for (const item of value) {
    if (typeof item !== "string" || item.trim() === "") {
      fail("ARRAY_ITEM_INVALID", context);
      continue;
    }
    if (seen.has(item)) {
      fail("ARRAY_DUPLICATE", `${context}: ${item}`);
    }
    seen.add(item);
  }
  return value.filter((item) => typeof item === "string" && item.trim() !== "");
}

function validatePlanned(value, context, requireExecuted) {
  if (!isObject(value)) {
    return;
  }
  if (value.status !== PLANNED) {
    fail("STATUS_NOT_PLANNED", `${context}: ${String(value.status)}`);
  }
  if (requireExecuted && value.executed !== false) {
    fail("EXECUTION_CLAIM_FORBIDDEN", `${context}: executed must be false`);
  }
}

function validateSlice(value, context) {
  if (!Number.isInteger(value) || value < 0 || value > 4) {
    fail("SLICE_INVALID", `${context}: ${String(value)}`);
    return false;
  }
  return true;
}

function validateTaskIds(value, context, taskIds, emptyCode = "TASK_LIST_EMPTY") {
  const ids = stringArray(value, context);
  if (Array.isArray(value) && value.length === 0) {
    fail(emptyCode, context);
  }
  for (const id of ids) {
    if (!/^T\d{3}$/u.test(id)) {
      fail("TASK_ID_INVALID", `${context}: ${id}`);
    } else if (!taskIds.has(id)) {
      fail("TASK_UNRESOLVED", `${context}: ${id}`);
    }
  }
  return ids;
}

function targetFile(target) {
  return typeof target === "string" ? target.split("::", 1)[0] : "";
}

function parseExecutableTarget(target, context) {
  if (!nonEmptyString(target, `${context}.test`)) {
    fail("TEST_INVALID", context);
    return null;
  }
  const match = /^(tests|src|benches|scripts|execution)\/([A-Za-z0-9_./-]+)::((?:[A-Za-z0-9_]+::)*[A-Za-z0-9_]+)$/u.exec(target);
  if (!match || target.includes("..")) {
    fail("TEST_INVALID", `${context}: ${target}`);
    return null;
  }
  const area = match[1];
  const file = `${area}/${match[2]}`;
  const symbolPath = match[3];
  const caseName = symbolPath.split("::").at(-1);
  const rustArea = area === "tests" || area === "src" || area === "benches";
  if (rustArea && path.posix.extname(file) !== ".rs") {
    fail("RUST_TARGET_EXTENSION_INVALID", `${context}: ${file}`);
  }
  if (!rustArea && !/\.(?:c?js|mjs|py)$/u.test(file)) {
    fail("NON_RUST_TARGET_EXTENSION_INVALID", `${context}: ${file}`);
  }
  return { area, file, symbolPath, caseName, rustArea };
}

function sourceModulePath(file) {
  const parts = file.slice("src/".length, -".rs".length).split("/");
  if (parts.at(-1) === "mod") parts.pop();
  if (parts.length === 1 && (parts[0] === "lib" || parts[0] === "main")) return "";
  return parts.join("::");
}

function validateTestAndCommand(target, command, context, kind = "planned_exact") {
  const parsed = parseExecutableTarget(target, context);
  if (!parsed) return null;
  if (!nonEmptyString(command, `${context}.command`)) {
    fail("COMMAND_INVALID", context);
    return parsed;
  }
  if (/[\u0000-\u001f;&|><`]/u.test(command) || command.includes("..") || /<[^>]+>/u.test(command)) {
    fail("COMMAND_INVALID", `${context}: unsafe or placeholder command`);
    return parsed;
  }
  const fileStem = path.posix.basename(parsed.file, path.posix.extname(parsed.file));
  const expectedIntegration = `cargo test --test ${fileStem} ${parsed.symbolPath} -- --exact`;
  const sourceModule = parsed.area === "src" ? sourceModulePath(parsed.file) : "";
  const sourceCase = sourceModule === "" ? parsed.symbolPath : `${sourceModule}::${parsed.symbolPath}`;
  const expectedLibrary = `cargo test --lib ${sourceCase} -- --exact`;
  const expectedBenchmark = `cargo bench --bench ${fileStem} -- ${parsed.caseName}`;
  const expectedNonRust = parsed.file.endsWith(".py") ? `uv run python ${parsed.file}` : `node ${parsed.file}`;
  const benchmarkKind = kind === "planned_benchmark" || kind === "executed_benchmark";
  const valid = benchmarkKind
    ? parsed.area === "benches" && parsed.file.endsWith(".rs") && command === expectedBenchmark
    : (parsed.area === "tests" && parsed.file.endsWith(".rs") && command === expectedIntegration) ||
      (parsed.area === "src" && parsed.file.endsWith(".rs") && command === expectedLibrary) ||
      ((parsed.area === "scripts" || parsed.area === "execution") && command === expectedNonRust);
  if (!valid) {
    fail("COMMAND_INVALID", `${context}: command does not exactly execute ${target}`);
  }
  return parsed;
}

function taskSlice(taskId) {
  const number = Number(taskId.slice(1));
  if (number >= 13 && number <= 21) return 0;
  if (number >= 22 && number <= 29) return 1;
  if (number >= 30 && number <= 40) return 2;
  if (number >= 41 && number <= 52) return 3;
  if (number >= 53 && number <= 73) return 4;
  if (number >= 74 && number <= 77) return 5;
  if (number >= 78 && number <= 90) return 6;
  return -1;
}

function normalizeTaskPath(raw) {
  const withoutSymbol = raw.replace(/::[A-Za-z0-9_:{}-]+$/u, "");
  if (withoutSymbol.startsWith("contracts/") || withoutSymbol.startsWith("checklists/")) {
    return `specs/020-repository-knowledge-index/${withoutSymbol}`;
  }
  return withoutSymbol;
}

function loadV11TaskCatalog() {
  const text = readText(TASKS_PATH);
  if (text === null) {
    return { ids: new Set(), pathsByTask: new Map(), sliceByTask: new Map(), declaredPaths: new Set() };
  }
  const marker = "# Executable V11 tasks: Preventive project-index lifecycle";
  const markerAt = text.indexOf(marker);
  if (markerAt === -1) {
    fail("TASK_SECTION_MISSING", TASKS_PATH);
    return { ids: new Set(), pathsByTask: new Map(), sliceByTask: new Map(), declaredPaths: new Set() };
  }
  const ids = new Set();
  const pathsByTask = new Map();
  const sliceByTask = new Map();
  const declaredPaths = new Set();
  const section = text.slice(markerAt);
  for (const match of section.matchAll(/^- \[[ xX]\] (T\d{3})\b([^\r\n]*)$/gmu)) {
    const id = match[1];
    if (ids.has(id)) {
      fail("TASK_DUPLICATE", id);
    }
    ids.add(id);
    const ownedPaths = new Set();
    for (const pathMatch of match[2].matchAll(/`([^`]+)`/gu)) {
      const candidate = normalizeTaskPath(pathMatch[1]);
      if (/^(?:src|tests|benches|scripts|execution|formal|specs|docs)\//u.test(candidate) && !/[{}*]/u.test(candidate)) {
        ownedPaths.add(candidate);
        declaredPaths.add(candidate);
      }
    }
    pathsByTask.set(id, ownedPaths);
    sliceByTask.set(id, taskSlice(id));
  }
  if (ids.size === 0) {
    fail("TASK_SECTION_EMPTY", TASKS_PATH);
  }
  return { ids, pathsByTask, sliceByTask, declaredPaths };
}

function taskDeclaresPath(taskCatalog, taskId, file) {
  const owned = taskCatalog.pathsByTask.get(taskId) || new Set();
  for (const declared of owned) {
    if (declared === file || (declared.endsWith("/") && file.startsWith(declared))) return true;
  }
  return false;
}

function pathIsExistingOrDeclared(taskCatalog, file) {
  return fs.existsSync(path.join(repositoryRoot, file)) || [...taskCatalog.declaredPaths].some((declared) => declared === file || (declared.endsWith("/") && file.startsWith(declared)));
}

function seamPath(seam) {
  return typeof seam === "string" ? seam.split("::", 1)[0] : "";
}

/// Classify every character as `code`, `comment`, or `literal`.
///
/// One walker, two consumers: the mask below and the canonical release form.
/// Duplicating this state machine is how a subtle lexer bug gets fixed in one
/// place and not the other.
function rustCharacterKinds(source) {
  const kinds = new Array(source.length).fill("code");
  let index = 0;
  let state = "code";
  let blockDepth = 0;
  let rawTerminator = "";
  const set = (at, kind) => {
    if (at < kinds.length) kinds[at] = kind;
  };
  while (index < source.length) {
    if (state === "code") {
      if (source.startsWith("//", index)) {
        set(index, "comment");
        set(index + 1, "comment");
        index += 2;
        state = "line_comment";
        continue;
      }
      if (source.startsWith("/*", index)) {
        set(index, "comment");
        set(index + 1, "comment");
        index += 2;
        blockDepth = 1;
        state = "block_comment";
        continue;
      }
      const raw = /^(?:br|r)(#*)"/u.exec(source.slice(index));
      if (raw) {
        for (let offset = 0; offset < raw[0].length; offset += 1) set(index + offset, "literal");
        index += raw[0].length;
        rawTerminator = `"${raw[1]}`;
        state = "raw_string";
        continue;
      }
      if (source[index] === '"') {
        set(index, "literal");
        index += 1;
        state = "string";
        continue;
      }
      const character = /^'(?:\\.|[^'\\\r\n])'/u.exec(source.slice(index));
      if (character) {
        for (let offset = 0; offset < character[0].length; offset += 1) set(index + offset, "literal");
        index += character[0].length;
        continue;
      }
      index += 1;
      continue;
    }
    if (state === "line_comment") {
      if (source[index] === "\n" || source[index] === "\r") state = "code";
      else set(index, "comment");
      index += 1;
      continue;
    }
    if (state === "block_comment") {
      if (source.startsWith("/*", index)) {
        set(index, "comment");
        set(index + 1, "comment");
        blockDepth += 1;
        index += 2;
      } else if (source.startsWith("*/", index)) {
        set(index, "comment");
        set(index + 1, "comment");
        blockDepth -= 1;
        index += 2;
        if (blockDepth === 0) state = "code";
      } else {
        set(index, "comment");
        index += 1;
      }
      continue;
    }
    if (state === "raw_string") {
      if (source.startsWith(rawTerminator, index)) {
        for (let offset = 0; offset < rawTerminator.length; offset += 1) set(index + offset, "literal");
        index += rawTerminator.length;
        state = "code";
      } else {
        set(index, "literal");
        index += 1;
      }
      continue;
    }
    if (source[index] === "\\") {
      set(index, "literal");
      if (index + 1 < source.length) set(index + 1, "literal");
      index += 2;
    } else if (source[index] === '"') {
      set(index, "literal");
      index += 1;
      state = "code";
    } else {
      set(index, "literal");
      index += 1;
    }
  }
  return kinds;
}

function maskRustCommentsAndLiterals(source) {
  const kinds = rustCharacterKinds(source);
  const chars = source.split("");
  for (let index = 0; index < chars.length; index += 1) {
    if (kinds[index] === "code") continue;
    if (chars[index] !== "\n" && chars[index] !== "\r") chars[index] = " ";
  }
  return chars.join("");
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
}

function rustModuleIntervals(code) {
  const closeByOpen = new Map();
  const braceStack = [];
  for (let index = 0; index < code.length; index += 1) {
    if (code[index] === "{") braceStack.push(index);
    else if (code[index] === "}" && braceStack.length > 0) closeByOpen.set(braceStack.pop(), index);
  }
  const intervals = [];
  for (const match of code.matchAll(/\bmod\s+([A-Za-z_][A-Za-z0-9_]*)\s*\{/gu)) {
    const open = match.index + match[0].lastIndexOf("{");
    const close = closeByOpen.get(open);
    if (Number.isInteger(close)) intervals.push({ name: match[1], open, close });
  }
  return intervals;
}

function matchingRustBrace(code, open) {
  let depth = 0;
  for (let index = open; index < code.length; index += 1) {
    if (code[index] === "{") depth += 1;
    else if (code[index] === "}") {
      depth -= 1;
      if (depth === 0) return index;
    }
  }
  return -1;
}

function rustDeclarationBody(code, declarationAt) {
  const semicolon = code.indexOf(";", declarationAt);
  const open = code.indexOf("{", declarationAt);
  if (open === -1 || (semicolon !== -1 && semicolon < open)) return null;
  const close = matchingRustBrace(code, open);
  return close === -1 ? null : code.slice(open + 1, close);
}

function rustFunctionBodies(code, name) {
  const bodies = [];
  const pattern = new RegExp(`\\bfn\\s+${escapeRegExp(name)}\\s*(?:<[^>{}]*>)?\\s*\\(`, "gu");
  for (const match of code.matchAll(pattern)) {
    const body = rustDeclarationBody(code, match.index);
    if (body !== null) bodies.push(body);
  }
  return bodies;
}

function rustCallIntervals(code, name) {
  const intervals = [];
  const pattern = new RegExp("\\b" + escapeRegExp(name) + "\\s*(?:::<[^(){};]*>)?\\s*\\(", "gu");
  for (const match of code.matchAll(pattern)) {
    if (/\bfn\s*$/u.test(code.slice(Math.max(0, match.index - 32), match.index))) continue;
    const open = code.indexOf("(", match.index);
    let depth = 0;
    let close = -1;
    for (let index = open; index < code.length; index += 1) {
      if (code[index] === "(") depth += 1;
      else if (code[index] === ")") {
        depth -= 1;
        if (depth === 0) {
          close = index;
          break;
        }
      }
    }
    if (close !== -1) intervals.push({ start: match.index, open, close });
  }
  return intervals;
}

function rustCallExists(code, name) {
  return rustCallIntervals(code, name).length > 0;
}

function rustNestedCallExists(code, outer, inner) {
  const innerCalls = rustCallIntervals(code, inner);
  return rustCallIntervals(code, outer).some(
    (outerCall) => innerCalls.some((innerCall) => outerCall.open < innerCall.start && innerCall.close < outerCall.close),
  );
}

function rustTypeMemberExists(code, owner, member) {
  const ownerEscaped = escapeRegExp(owner);
  const memberEscaped = escapeRegExp(member);
  const typePattern = new RegExp(`\\b(?:struct|enum|union)\\s+${ownerEscaped}\\b`, "gu");
  for (const match of code.matchAll(typePattern)) {
    const body = rustDeclarationBody(code, match.index);
    if (body !== null && new RegExp(`\\b${memberEscaped}\\s*:`, "u").test(body)) return true;
  }
  const implPattern = new RegExp(`\\bimpl\\b[^{};]*\\b${ownerEscaped}\\b[^{};]*\\{`, "gu");
  for (const match of code.matchAll(implPattern)) {
    const open = match.index + match[0].lastIndexOf("{");
    const close = matchingRustBrace(code, open);
    if (close !== -1 && new RegExp(`\\bfn\\s+${memberEscaped}\\s*(?:<[^>{}]*>)?\\s*\\(`, "u").test(code.slice(open + 1, close))) return true;
  }
  return false;
}

function sourceAnchorResolvesText(anchor, source) {
  if (typeof anchor !== "string" || !anchor.startsWith("src/") || !anchor.includes("::") || typeof source !== "string") return false;
  const separator = anchor.indexOf("::");
  const rawSymbol = anchor.slice(separator + 2);
  if (!anchor.slice(0, separator).endsWith(".rs")) return false;
  const code = maskRustCommentsAndLiterals(source);
  const spawnCallsite = rawSymbol.endsWith(" spawn");
  const symbol = spawnCallsite ? rawSymbol.slice(0, -" spawn".length) : rawSymbol;
  const segments = symbol.split("::");
  if (segments.some((segment) => !/^[A-Za-z_][A-Za-z0-9_]*$/u.test(segment))) return false;
  if (segments.length === 1) {
    const name = segments[0];
    if (spawnCallsite) return rustNestedCallExists(code, "spawn", name);
    return new RegExp(`\\b(?:fn|struct|enum|union|type|trait|static|const)\\s+${escapeRegExp(name)}\\b`, "u").test(code);
  }
  const owner = segments[0];
  const member = segments.at(-1);
  if (/^[A-Z]/u.test(owner)) return rustTypeMemberExists(code, owner, member);
  return rustFunctionBodies(code, owner).some(
    (body) => spawnCallsite ? rustNestedCallExists(body, "spawn", member) : rustCallExists(body, member),
  );
}

function sourceAnchorResolves(anchor) {
  if (typeof anchor !== "string" || !anchor.startsWith("src/") || !anchor.includes("::")) return false;
  const file = anchor.slice(0, anchor.indexOf("::"));
  if (!isRegularFile(file) || !file.endsWith(".rs")) return false;
  let source;
  try {
    source = fs.readFileSync(path.join(repositoryRoot, file), "utf8");
  } catch {
    return false;
  }
  return sourceAnchorResolvesText(anchor, source);
}

function rustNamedCaseBodyInSource(symbolPath, source, allowIgnored = false) {
  if (typeof symbolPath !== "string" || typeof source !== "string") return null;
  const code = maskRustCommentsAndLiterals(source);
  const caseName = symbolPath.split("::").at(-1);
  const escaped = escapeRegExp(caseName);
  const moduleIntervals = rustModuleIntervals(code);
  const functionPattern = new RegExp(
    `\\b(?:(?:pub(?:\\s*\\([^)]*\\))?|async|unsafe|const)\\s+)*fn\\s+${escaped}\\s*\\(`,
    "gu",
  );
  for (const match of code.matchAll(functionPattern)) {
    const prefix = code.slice(0, match.index);
    const attributes = /(?:#\s*\[[^\]]*\]\s*)+$/u.exec(prefix);
    if (attributes &&
        /#\s*\[\s*(?:test\b|(?:tokio|async_std|actix_rt)::test\b)/u.test(attributes[0]) &&
        // `cfg_attr` can add `ignore` conditionally, so it is never resolvable.
        // A literal `ignore` is resolvable only where the case is PLANNED: a
        // Slice 0 positive control is RED by construction and must be kept out
        // of the default suite, but an executed receipt claiming `passed` can
        // never come from an ignored case, so that path stays strict.
        !/#\s*\[\s*cfg_attr\b/u.test(attributes[0]) &&
        (allowIgnored || !/#\s*\[\s*ignore\b/u.test(attributes[0])) &&
        ![...attributes[0].matchAll(/#\s*\[\s*cfg\s*\(([^)]*)\)\s*\]/gu)].some((cfg) => cfg[1].trim() !== "test")) {
      const modules = moduleIntervals
        .filter((interval) => interval.open < match.index && match.index < interval.close)
        .sort((left, right) => left.open - right.open)
        .map((interval) => interval.name);
      if ([...modules, caseName].join("::") === symbolPath) return rustDeclarationBody(code, match.index);
    }
  }
  return null;
}

function rustNamedCaseExistsInSource(symbolPath, source, allowIgnored = false) {
  return rustNamedCaseBodyInSource(symbolPath, source, allowIgnored) !== null;
}

function rustNamedCaseIsNonEmptyInSource(symbolPath, source, allowIgnored = false) {
  const body = rustNamedCaseBodyInSource(symbolPath, source, allowIgnored);
  return typeof body === "string" && /[A-Za-z0-9_]/u.test(body);
}

function rustNamedCaseExists(file, symbolPath) {
  try {
    return rustNamedCaseExistsInSource(symbolPath, fs.readFileSync(path.join(repositoryRoot, file), "utf8"));
  } catch {
    return false;
  }
}

function rustMacroBodies(code, name) {
  const bodies = [];
  const pattern = new RegExp("\\b" + escapeRegExp(name) + "!\\s*([({\\[])", "gu");
  const closing = { "(": ")", "{": "}", "[": "]" };
  for (const match of code.matchAll(pattern)) {
    const open = match.index + match[0].lastIndexOf(match[1]);
    const stack = [];
    let close = -1;
    for (let index = open; index < code.length; index += 1) {
      if (Object.prototype.hasOwnProperty.call(closing, code[index])) stack.push(closing[code[index]]);
      else if (stack.length > 0 && code[index] === stack.at(-1)) {
        stack.pop();
        if (stack.length === 0) {
          close = index;
          break;
        }
      }
    }
    if (close !== -1) bodies.push(code.slice(open + 1, close));
  }
  return bodies;
}

function benchmarkRegistrationExistsInSource(registration, source) {
  if (typeof registration !== "string" || typeof source !== "string") return false;
  const parsed = /^criterion_group:([A-Za-z_][A-Za-z0-9_]*)->([A-Za-z_][A-Za-z0-9_]*)$/u.exec(registration);
  if (!parsed || parsed[1] === parsed[2]) return false;
  const group = parsed[1];
  const target = parsed[2];
  const code = maskRustCommentsAndLiterals(source);
  const groupBodies = rustMacroBodies(code, "criterion_group").filter((body) => {
    const namedGroup = /\bname\s*=\s*([A-Za-z_][A-Za-z0-9_]*)\s*;/u.exec(body);
    if (namedGroup) return namedGroup[1] === group;
    const parts = topLevelParts(body);
    return parts[0] === group;
  });
  if (groupBodies.length !== 1) return false;
  const namedGroup = /\bname\s*=\s*([A-Za-z_][A-Za-z0-9_]*)\s*;/u.exec(groupBodies[0]);
  const targetItems = namedGroup
    ? (() => {
      const targets = /\btargets\s*=\s*([^;]+);/u.exec(groupBodies[0]);
      return targets ? topLevelParts(targets[1]) : [];
    })()
    : topLevelParts(groupBodies[0]).slice(1);
  const mainBodies = rustMacroBodies(code, "criterion_main");
  return targetItems.filter((item) => item === target).length === 1 &&
    mainBodies.length === 1 && topLevelParts(mainBodies[0]).filter((item) => item === group).length === 1;
}

function benchmarkRegistrationExists(file, registration) {
  try {
    return benchmarkRegistrationExistsInSource(registration, fs.readFileSync(path.join(repositoryRoot, file), "utf8"));
  } catch {
    return false;
  }
}

function validateBenchmarkEvidenceContract(value, context) {
  if (!isObject(value)) {
    fail("BENCHMARK_EVIDENCE_CONTRACT_MISSING", context);
    return;
  }
  exactKeys(value, ["artifact", "schema", "baseline", "semantic_gate", "status"], `${context}.evidence_contract`);
  if (typeof value.artifact !== "string" || !/^target\/ci\/lifecycle-v11\/[A-Za-z0-9_.-]+\.json$/u.test(value.artifact)) {
    fail("BENCHMARK_EVIDENCE_CONTRACT_INVALID", `${context}: artifact`);
  } else if (!isRegularFile(value.artifact)) {
    fail("BENCHMARK_EVIDENCE_ARTIFACT_MISSING", `${context}: ${value.artifact}`);
  }
  if (value.schema !== "symforge.executed_benchmark_evidence.v1") fail("BENCHMARK_EVIDENCE_CONTRACT_INVALID", `${context}: schema`);
  if (typeof value.baseline !== "string" || !/^[0-9a-f]{7,64}$/u.test(value.baseline)) fail("BENCHMARK_EVIDENCE_CONTRACT_INVALID", `${context}: baseline`);
  if (value.semantic_gate !== "passed" || value.status !== "executed") fail("BENCHMARK_EVIDENCE_CONTRACT_INVALID", `${context}: execution evidence`);
}

function validateNonRustResolver(file, symbolPath, command, resolver, context) {
  if (!isObject(resolver)) {
    fail("NON_RUST_RESOLVER_MISSING", context);
    return;
  }
  exactKeys(resolver, ["id"], `${context}.resolver`);
  const registered = EXECUTED_NON_RUST_RESOLVERS.get(`${file}::${symbolPath}`);
  if (!isObject(registered) || resolver.id !== registered.id || command !== registered.command) {
    fail("NON_RUST_RESOLVER_INVALID", `${context}: no code-owned exact resolver for ${file}::${symbolPath}`);
  }
}

function validateStringMap(value, idPattern, context) {
  if (!isObject(value)) {
    fail("SCHEMA_OBJECT", `${context}: expected object`);
    return;
  }
  for (const [id, description] of Object.entries(value)) {
    if (!idPattern.test(id)) {
      fail("CATALOG_ID_INVALID", `${context}: ${id}`);
    }
    nonEmptyString(description, `${context}.${id}`);
  }
  if (Object.keys(value).length === 0) {
    fail("CATALOG_EMPTY", context);
  }
}

function validateReleaseValidation(value, taskCatalog) {
  const keys = ["mode", "command", "evidence_path", "owner_tasks", "required_task_receipts", "planned_case_policy", "planned_benchmark_policy", "source_anchor_policy", "same_tree_policy", "approval_policy", "materialized_tool_environment", "oracle_result_policy", "release_task_policy", "execution_policy", "evidence_contract"];
  exactKeys(value, keys, "trace.release_validation");
  if (!isObject(value)) {
    fail("RELEASE_VALIDATION_CONTRACT_INVALID", "trace.release_validation");
    return;
  }
  validateTaskIds(value.owner_tasks, "trace.release_validation.owner_tasks", taskCatalog.ids);
  validateTaskIds(value.required_task_receipts, "trace.release_validation.required_task_receipts", taskCatalog.ids);
  if (JSON.stringify(value) !== JSON.stringify(EXPECTED_RELEASE_VALIDATION)) {
    fail("RELEASE_VALIDATION_CONTRACT_INVALID", "release materialization mode, evidence path, owners, receipts, and same-tree policies must be exact");
  }
}

function validateTrace(trace, taskCatalog) {
  if (!trace) return;
  const taskIds = taskCatalog.ids;
  exactKeys(trace, ["kind", "schema_version", "status", "release_validation", "catalogs", "requirements"], "trace");
  if (trace.kind !== "symforge.lifecycle_oracle_traceability.v11") fail("KIND_INVALID", "trace.kind");
  if (trace.schema_version !== 1) fail("SCHEMA_VERSION_INVALID", "trace.schema_version");
  validatePlanned(trace, "trace", false);
  validateReleaseValidation(trace.release_validation, taskCatalog);

  const catalogs = isObject(trace.catalogs) ? trace.catalogs : {};
  exactKeys(catalogs, ["commands", "tests", "seams", "invariants", "state_models", "bounds", "fairness", "ci_artifacts"], "trace.catalogs");
  const commands = isObject(catalogs.commands) ? catalogs.commands : {};
  const tests = isObject(catalogs.tests) ? catalogs.tests : {};
  const seams = isObject(catalogs.seams) ? catalogs.seams : {};
  const invariants = isObject(catalogs.invariants) ? catalogs.invariants : {};
  const models = isObject(catalogs.state_models) ? catalogs.state_models : {};
  const bounds = isObject(catalogs.bounds) ? catalogs.bounds : {};
  const fairness = isObject(catalogs.fairness) ? catalogs.fairness : {};
  const artifacts = isObject(catalogs.ci_artifacts) ? catalogs.ci_artifacts : {};

  validateStringMap(commands, /^CMD-[A-Z0-9-]+$/u, "trace.catalogs.commands");
  validateStringMap(invariants, /^INV-[A-Z0-9-]+$/u, "trace.catalogs.invariants");
  validateStringMap(bounds, /^BOUND-[A-Z0-9-]+$/u, "trace.catalogs.bounds");
  validateStringMap(fairness, /^FAIR-[A-Z0-9-]+$/u, "trace.catalogs.fairness");
  validateStringMap(artifacts, /^CI-[A-Z0-9-]+$/u, "trace.catalogs.ci_artifacts");
  exactArray(Object.keys(bounds).sort(), EXPECTED_BOUND_IDS, "BOUND_CATALOG_MISMATCH", "trace.catalogs.bounds");
  exactArray(Object.keys(fairness).sort(), EXPECTED_FAIRNESS_IDS, "FAIRNESS_CATALOG_MISMATCH", "trace.catalogs.fairness");
  exactArray(Object.keys(artifacts).sort(), EXPECTED_CI_ARTIFACT_IDS, "CI_ARTIFACT_CATALOG_MISMATCH", "trace.catalogs.ci_artifacts");

  for (const [id, value] of Object.entries(seams)) {
    if (!/^SEAM-[A-Z0-9-]+$/u.test(id)) fail("CATALOG_ID_INVALID", `trace.catalogs.seams: ${id}`);
    const atoms = stringArray(value, `trace.catalogs.seams.${id}`);
    for (const atom of atoms) {
      if (/^src\/lifecycle\//u.test(atom) || /^src\/(?:watcher|sidecar|snapshot)\.rs(?:::|$)/u.test(atom)) {
        fail("SEAM_NAMESPACE_INVALID", `${id}: ${atom}`);
      }
      const file = seamPath(atom);
      if (!/^(?:src|tests|benches|scripts|execution|specs)\/[A-Za-z0-9_./-]+\.rs$|^specs\/[A-Za-z0-9_./-]+\.json$/u.test(file) || file.includes("..") || !pathIsExistingOrDeclared(taskCatalog, file)) {
        fail("SEAM_UNRESOLVED", `${id}: ${atom}`);
      }
    }
    if (!Object.prototype.hasOwnProperty.call(EXPECTED_SEAMS, id) || JSON.stringify(atoms) !== JSON.stringify(EXPECTED_SEAMS[id])) {
      fail("SEAM_UNRESOLVED", `${id}: catalog does not match the frozen V11 seam`);
    }
  }
  exactArray(Object.keys(seams), Object.keys(EXPECTED_SEAMS), "SEAM_CATALOG_MISMATCH", "trace.catalogs.seams");
  exactArray(Object.keys(invariants), Object.keys(EXPECTED_SEAMS).map((id) => id.replace("SEAM-", "INV-")), "INVARIANT_CATALOG_MISMATCH", "trace.catalogs.invariants");
  exactArray(Object.keys(models), Object.keys(EXPECTED_SEAMS).map((id) => id.replace("SEAM-", "MODEL-")), "STATE_MODEL_CATALOG_MISMATCH", "trace.catalogs.state_models");
  for (const [id, value] of Object.entries(models)) {
    if (!/^MODEL-[A-Z0-9-]+$/u.test(id)) fail("CATALOG_ID_INVALID", `trace.catalogs.state_models: ${id}`);
    stringArray(value, `trace.catalogs.state_models.${id}`);
  }
  if (Object.prototype.hasOwnProperty.call(models, "MODEL-QUERY")) {
    exactArray(models["MODEL-QUERY"], ["Selecting", "LeasedCurrent", "Refused", "Released"], "STRICT_QUERY_MODEL_INVALID", "MODEL-QUERY");
  }
  if (Object.prototype.hasOwnProperty.call(models, "MODEL-SURFACE")) {
    const expectedSurfaceModel = ["GenerationLeased", "DiskObserved", "WorktreeScopeObserved", "GitObserved", "RuntimeHealthObserved", "MutationPermitted", "StateWriteAuthorized", "Refused"];
    if (JSON.stringify(models["MODEL-SURFACE"]) !== JSON.stringify(expectedSurfaceModel)) {
      const withoutHealth = expectedSurfaceModel.filter((state) => state !== "RuntimeHealthObserved");
      fail(
        JSON.stringify(models["MODEL-SURFACE"]) === JSON.stringify(withoutHealth)
          ? "RUNTIME_HEALTH_BRANCH_INVALID"
          : "SURFACE_AUTHORITY_MODEL_INVALID",
        "MODEL-SURFACE",
      );
    }
  }

  for (const [id, test] of Object.entries(tests)) {
    const context = `trace.catalogs.tests.${id}`;
    if (!/^TEST-[A-Z0-9-]+$/u.test(id)) fail("CATALOG_ID_INVALID", context);
    const baseTestKeys = ["kind", "target", "command_id", "owner_tasks", "introduced_slice"];
    const target = isObject(test) ? parseExecutableTarget(test.target, context) : null;
    const expectedTestKeys = [...baseTestKeys];
    if (isObject(test) && (test.kind === "planned_benchmark" || test.kind === "executed_benchmark")) expectedTestKeys.push("registration");
    if (isObject(test) && test.kind === "executed_benchmark") expectedTestKeys.push("evidence_contract");
    if (isObject(test) && test.kind === "executed_exact" && target && !target.rustArea) expectedTestKeys.push("resolver");
    exactKeys(test, expectedTestKeys, context);
    if (!isObject(test)) continue;
    if (!["planned_exact", "planned_benchmark", "inherited_exact", "executed_exact", "executed_benchmark"].includes(test.kind)) {
      fail("TEST_KIND_INVALID", `${context}: ${String(test.kind)}`);
    }
    validateSlice(test.introduced_slice, `${context}.introduced_slice`);
    const owners = validateTaskIds(test.owner_tasks, `${context}.owner_tasks`, taskIds, "TEST_OWNER_TASKS_EMPTY");
    for (const owner of owners) {
      if (taskCatalog.sliceByTask.get(owner) !== test.introduced_slice) {
        fail("TEST_SLICE_OWNERSHIP_INVALID", `${context}: ${owner} belongs to Slice ${taskCatalog.sliceByTask.get(owner)}`);
      }
    }
    if (!Object.prototype.hasOwnProperty.call(commands, test.command_id)) {
      fail("COMMAND_REF_UNRESOLVED", `${context}: ${String(test.command_id)}`);
    } else {
      validateTestAndCommand(test.target, commands[test.command_id], context, test.kind);
    }
    const file = targetFile(test.target);
    if (test.kind.startsWith("planned_") && !owners.some((owner) => taskDeclaresPath(taskCatalog, owner, file))) {
      fail("TEST_TARGET_UNRESOLVED", `${context}: ${file} is not declared by an owner task`);
    }
    if (test.kind === "planned_exact" && isRegularFile(file) && target && target.rustArea) {
      let plannedSource = null;
      try {
        plannedSource = fs.readFileSync(path.join(repositoryRoot, file), "utf8");
      } catch {
        plannedSource = null;
      }
      if (typeof plannedSource === "string" && !rustNamedCaseExistsInSource(target.symbolPath, plannedSource, true)) {
        fail("PLANNED_TEST_CASE_MISSING", `${context}: ${test.target}`);
      } else if (typeof plannedSource === "string" && !rustNamedCaseIsNonEmptyInSource(target.symbolPath, plannedSource, true)) {
        fail("PLANNED_TEST_CASE_EMPTY", `${context}: ${test.target}`);
      }
    }
    if (test.kind === "planned_benchmark" || test.kind === "executed_benchmark") {
      const expectedRegistration = target ? `criterion_group:${target.caseName}_group->${target.caseName}` : null;
      if (test.registration !== expectedRegistration) {
        fail("BENCHMARK_REGISTRATION_INVALID", `${context}: expected ${String(expectedRegistration)}`);
      } else if (fs.existsSync(path.join(repositoryRoot, file)) && !isRegularFile(file)) {
        fail("TEST_TARGET_KIND_MISMATCH", `${context}: ${file}`);
      } else if (isRegularFile(file) && !benchmarkRegistrationExists(file, test.registration)) {
        fail("BENCHMARK_REGISTRATION_INVALID", `${context}: ${test.registration} is not registered in ${file}`);
      }
    }
    if (test.kind === "executed_benchmark") validateBenchmarkEvidenceContract(test.evidence_contract, context);
    if (test.kind === "executed_exact" && target && !target.rustArea) {
      validateNonRustResolver(file, target.symbolPath, commands[test.command_id], test.resolver, context);
    }
    if ((test.kind === "inherited_exact" || test.kind === "executed_exact" || test.kind === "executed_benchmark") && typeof test.target === "string") {
      if (!isRegularFile(file)) {
        fail(test.kind.startsWith("executed_") ? "EXECUTED_TEST_TARGET_MISSING" : "INHERITED_TEST_MISSING", `${context}: ${file}`);
      } else if (file.endsWith(".rs") && test.kind !== "executed_benchmark") {
        const symbolPath = target ? target.symbolPath : test.target.split("::").slice(1).join("::");
        if (!rustNamedCaseExists(file, symbolPath)) {
          fail(test.kind.startsWith("executed_") ? "EXECUTED_TEST_CASE_MISSING" : "INHERITED_TEST_CASE_MISSING", `${context}: ${test.target}`);
        }
      }
    }
  }
  exactArray(Object.keys(tests).sort(), EXPECTED_TRACE_TEST_IDS, "TEST_CATALOG_MISMATCH", "trace.catalogs.tests");
  const referencedCommandIds = Object.values(tests).filter(isObject).map((test) => test.command_id);
  if (new Set(referencedCommandIds).size !== referencedCommandIds.length) fail("COMMAND_CATALOG_MISMATCH", "each test must own one command");
  exactArray(Object.keys(commands).sort(), [...new Set(referencedCommandIds)].sort(), "COMMAND_CATALOG_MISMATCH", "trace.catalogs.commands");
  const embedTest = tests["TEST-EMBED"];
  if (!isObject(embedTest) || embedTest.introduced_slice !== 4 || JSON.stringify(embedTest.owner_tasks) !== JSON.stringify(["T058"])) {
    fail("TEST_SLICE_OWNERSHIP_INVALID", "TEST-EMBED must be owned by T058 in Slice 4");
  }
  const surfaceTest = tests["TEST-SURFACE"];
  if (!isObject(surfaceTest) || surfaceTest.target !== "tests/activation_cut_v11.rs::all_ingress_uses_exact_typed_authority_branch" || surfaceTest.introduced_slice !== 3 || JSON.stringify(surfaceTest.owner_tasks) !== JSON.stringify(["T050"])) {
    fail("SURFACE_TEST_OWNER_INVALID", "TEST-SURFACE must be the exact T050 Slice 3 reachability case");
  }
  const performanceTest = tests["TEST-PERFORMANCE"];
  if (!isObject(performanceTest) || performanceTest.target !== "benches/observed_refresh_gate_v1.rs::observed_refresh_gate_v1" || performanceTest.kind !== "planned_benchmark" || performanceTest.registration !== "criterion_group:observed_refresh_gate_v1_group->observed_refresh_gate_v1" || JSON.stringify(performanceTest.owner_tasks) !== JSON.stringify(["T068"]) || commands[performanceTest.command_id] !== "cargo bench --bench observed_refresh_gate_v1 -- observed_refresh_gate_v1") {
    fail("SC024_TARGET_INVALID", "TEST-PERFORMANCE must be the T068 benchmark and exact command");
  }

  const rows = Array.isArray(trace.requirements) ? trace.requirements : [];
  if (!Array.isArray(trace.requirements)) fail("ARRAY_INVALID", "trace.requirements");
  const counts = new Map();
  for (const row of rows) {
    if (isObject(row) && typeof row.requirement_id === "string") {
      counts.set(row.requirement_id, (counts.get(row.requirement_id) || 0) + 1);
    }
  }
  for (const id of EXPECTED_REQUIREMENTS) {
    const count = counts.get(id) || 0;
    if (count === 0) fail("TRACE_REQUIREMENT_MISSING", id);
    if (count > 1) fail("TRACE_REQUIREMENT_DUPLICATE", `${id}: ${count}`);
  }
  for (const id of counts.keys()) {
    if (!EXPECTED_REQUIREMENTS.includes(id)) fail("TRACE_REQUIREMENT_EXTRA", id);
  }
  const actualOrder = rows.map((row) => isObject(row) ? row.requirement_id : null);
  if (JSON.stringify(actualOrder) !== JSON.stringify(EXPECTED_REQUIREMENTS)) {
    fail("TRACE_REQUIREMENT_ORDER", "requirements must be FR-001..FR-052 then SC-001..SC-026");
  }

  const rowKeys = ["requirement_id", "implementation_tasks", "test_ids", "seam_ids", "invariant_id", "state_model_id", "target_slice", "bound_ids", "fairness_ids", "ci_artifact_id", "status", "executed"];
  for (const [index, row] of rows.entries()) {
    const context = `trace.requirements[${index}]`;
    exactKeys(row, rowKeys, context);
    if (!isObject(row)) continue;
    if (!EXPECTED_REQUIREMENTS.includes(row.requirement_id)) fail("REQUIREMENT_ID_INVALID", `${context}: ${String(row.requirement_id)}`);
    const mappedTasks = validateTaskIds(row.implementation_tasks, `${context}.implementation_tasks`, taskIds, "TRACE_REQUIREMENT_TASKS_EMPTY");
    if (mappedTasks.length === 0) fail("TRACE_REQUIREMENT_IMPLEMENTATION_MISSING", context);
    const testIds = stringArray(row.test_ids, `${context}.test_ids`);
    if (testIds.length === 0) fail("TRACE_REQUIREMENT_TESTS_EMPTY", context);
    for (const id of testIds) {
      if (!Object.prototype.hasOwnProperty.call(tests, id)) fail("TEST_REF_UNRESOLVED", `${context}: ${id}`);
      else if (Number.isInteger(row.target_slice) && tests[id].introduced_slice > row.target_slice) fail("TEST_AFTER_TARGET_SLICE", `${context}: ${id}`);
    }
    for (const testId of testIds) {
      const owners = isObject(tests[testId]) && Array.isArray(tests[testId].owner_tasks) ? tests[testId].owner_tasks : [];
      if (!owners.some((owner) => mappedTasks.includes(owner))) {
        fail("TEST_TASK_OWNERSHIP_INVALID", `${context}: no implementation task owns ${testId}`);
      }
    }
    const seamIds = stringArray(row.seam_ids, `${context}.seam_ids`);
    for (const id of seamIds) if (!Object.prototype.hasOwnProperty.call(seams, id)) fail("SEAM_REF_UNRESOLVED", `${context}: ${id}`);
    for (const id of stringArray(row.bound_ids, `${context}.bound_ids`)) if (!Object.prototype.hasOwnProperty.call(bounds, id)) fail("BOUND_REF_UNRESOLVED", `${context}: ${id}`);
    for (const id of stringArray(row.fairness_ids, `${context}.fairness_ids`)) if (!Object.prototype.hasOwnProperty.call(fairness, id)) fail("FAIRNESS_REF_UNRESOLVED", `${context}: ${id}`);
    if (!Object.prototype.hasOwnProperty.call(invariants, row.invariant_id)) fail("INVARIANT_REF_UNRESOLVED", `${context}: ${String(row.invariant_id)}`);
    if (!Object.prototype.hasOwnProperty.call(models, row.state_model_id)) fail("STATE_MODEL_REF_UNRESOLVED", `${context}: ${String(row.state_model_id)}`);
    if (typeof row.invariant_id === "string" && !seamIds.includes(row.invariant_id.replace("INV-", "SEAM-"))) fail("INVARIANT_SEAM_EDGE_INVALID", `${context}: ${row.invariant_id}`);
    if (typeof row.state_model_id === "string" && !seamIds.includes(row.state_model_id.replace("MODEL-", "SEAM-"))) fail("STATE_MODEL_SEAM_EDGE_INVALID", `${context}: ${row.state_model_id}`);
    if (!Object.prototype.hasOwnProperty.call(artifacts, row.ci_artifact_id)) fail("CI_ARTIFACT_REF_UNRESOLVED", `${context}: ${String(row.ci_artifact_id)}`);
    validateSlice(row.target_slice, `${context}.target_slice`);
    validatePlanned(row, context, true);

    for (const requiredTest of REQUIRED_TEST_EDGES.get(row.requirement_id) || []) {
      if (!testIds.includes(requiredTest)) fail("REQUIREMENT_TEST_EDGE_INVALID", `${row.requirement_id}: missing ${requiredTest}`);
    }
    for (const requiredTask of REQUIRED_TASK_EDGES.get(row.requirement_id) || []) {
      if (!mappedTasks.includes(requiredTask)) fail("REQUIREMENT_TASK_EDGE_INVALID", `${row.requirement_id}: missing ${requiredTask}`);
    }
    if (REQUIRED_SLICE4.has(row.requirement_id) && row.target_slice !== 4) {
      fail("REQUIREMENT_SLICE_INVALID", `${row.requirement_id}: expected Slice 4`);
    }
    if (row.requirement_id === "FR-020" && !row.seam_ids.includes("SEAM-EMBED")) fail("REQUIREMENT_TEST_EDGE_INVALID", "FR-020: missing SEAM-EMBED");
    if (row.requirement_id === "FR-025") {
      const expectedOpaquePathEdge = {
        implementation_tasks: ["T013", "T053", "T060"],
        test_ids: ["TEST-OPAQUE-PATH", "TEST-OPAQUE-PATH-INHERITED"],
        seam_ids: ["SEAM-OPAQUE-PATH"],
        invariant_id: "INV-OPAQUE-PATH",
        state_model_id: "MODEL-OPAQUE-PATH",
        target_slice: 4,
        ci_artifact_id: "CI-SLICE4",
      };
      if (Object.entries(expectedOpaquePathEdge).some(([key, expected]) => JSON.stringify(row[key]) !== JSON.stringify(expected))) {
        fail("FR025_OPAQUE_PATH_EDGE_INVALID", "FR-025 must map only to lossless opaque native-path identity and its V11 integration");
      }
    }
  }
  const uniqueTasks = new Set(rows.flatMap((row) => isObject(row) && Array.isArray(row.implementation_tasks) ? row.implementation_tasks : []));
  const uniqueTests = new Set(rows.flatMap((row) => isObject(row) && Array.isArray(row.test_ids) ? row.test_ids : []));
  if (uniqueTasks.size < 20 || uniqueTests.size < 15) fail("TRACE_MAPPING_DEGENERATE", `tasks=${uniqueTasks.size}, tests=${uniqueTests.size}`);
}

function validateAmendmentRegressionBindings(oracles) {
  const text = readText(ACCEPTANCE_PATH);
  if (text === null) return;
  const bindingLines = text.split(/\r?\n/u).filter((line) => line.startsWith("- Regression:"));
  const pattern = /^- Regression: `([A-Z0-9-]+)` — `(ORACLE-[A-Z0-9-]+)`; test `([^`]+)`; command `([^`]+)`\.$/u;
  const byOracle = new Map(oracles.filter(isObject).map((oracle) => [oracle.oracle_id, oracle]));
  const seen = new Set();
  for (const line of bindingLines) {
    const match = pattern.exec(line);
    if (!match) {
      fail("AMENDMENT_REGRESSION_BINDING_INVALID", line);
      continue;
    }
    const [, regressionId, oracleId, test, command] = match;
    const oracle = byOracle.get(oracleId);
    if (seen.has(regressionId) || EXPECTED_AMENDMENT_REGRESSION_BINDINGS.get(regressionId) !== oracleId ||
        !isObject(oracle) || oracle.test !== test || oracle.command !== command) {
      fail("AMENDMENT_REGRESSION_BINDING_INVALID", regressionId);
    }
    seen.add(regressionId);
  }
  const expectedIds = [...EXPECTED_AMENDMENT_REGRESSION_BINDINGS.keys()];
  const actualIds = [...seen].sort();
  if (JSON.stringify(actualIds) !== JSON.stringify([...expectedIds].sort()) || bindingLines.length !== expectedIds.length) {
    fail("AMENDMENT_REGRESSION_BINDING_INVALID", `expected=${expectedIds.length}, actual=${bindingLines.length}`);
  }
}

function validateAcceptance(acceptance, trace, taskCatalog) {
  if (!acceptance) return;
  const taskIds = taskCatalog.ids;
  exactKeys(acceptance, ["kind", "schema_version", "status", "oracles"], "acceptance");
  if (acceptance.kind !== "symforge.lifecycle_acceptance_oracles.v11") fail("KIND_INVALID", "acceptance.kind");
  if (acceptance.schema_version !== 1) fail("SCHEMA_VERSION_INVALID", "acceptance.schema_version");
  validatePlanned(acceptance, "acceptance", false);
  const oracles = Array.isArray(acceptance.oracles) ? acceptance.oracles : [];
  if (!Array.isArray(acceptance.oracles)) fail("ARRAY_INVALID", "acceptance.oracles");
  const traceRows = new Map(trace && Array.isArray(trace.requirements) ? trace.requirements.map((row) => [row.requirement_id, row]) : []);
  const traceTests = trace && isObject(trace.catalogs) && isObject(trace.catalogs.tests) ? trace.catalogs.tests : {};
  const traceCommands = trace && isObject(trace.catalogs) && isObject(trace.catalogs.commands) ? trace.catalogs.commands : {};
  const categoryCounts = new Map();
  const expectedCategoryCounts = new Map();
  for (const [category] of EXPECTED_ACCEPTANCE_ORACLES.values()) expectedCategoryCounts.set(category, (expectedCategoryCounts.get(category) || 0) + 1);
  const oracleIds = new Set();
  const coveredRequirements = new Set();
  const acceptanceEdges = new Set();
  const keys = ["oracle_id", "category", "trace_test_id", "requirement_ids", "implementation_tasks", "target_slice", "test", "command", "production_seams", "preconditions", "actions", "assertions", "positive_control", "negative_controls", "bounds", "fairness", "ci_artifact", "status", "executed"];
  for (const [index, oracle] of oracles.entries()) {
    const context = `acceptance.oracles[${index}]`;
    exactKeys(oracle, keys, context);
    if (!isObject(oracle)) continue;
    if (!/^ORACLE-[A-Z0-9-]+$/u.test(oracle.oracle_id || "")) fail("ORACLE_ID_INVALID", `${context}: ${String(oracle.oracle_id)}`);
    if (oracleIds.has(oracle.oracle_id)) fail("ORACLE_ID_DUPLICATE", String(oracle.oracle_id));
    oracleIds.add(oracle.oracle_id);
    categoryCounts.set(oracle.category, (categoryCounts.get(oracle.category) || 0) + 1);
    const expected = EXPECTED_ACCEPTANCE_ORACLES.get(oracle.oracle_id);
    if (!expected) {
      fail("ORACLE_ID_INVALID", `${context}: ${String(oracle.oracle_id)}`);
    } else {
      if (oracle.category !== expected[0]) fail("ORACLE_CATEGORY_INVALID", `${oracle.oracle_id}: ${String(oracle.category)}`);
      if (oracle.target_slice !== expected[1]) fail("ORACLE_SLICE_MISMATCH", `${oracle.oracle_id}: ${String(oracle.target_slice)}`);
      if (oracle.trace_test_id !== expected[2]) fail("TRACE_ACCEPTANCE_EDGE_MISMATCH", `${oracle.oracle_id}: expected ${expected[2]}`);
    }
    validateSlice(oracle.target_slice, `${context}.target_slice`);
    const requirementIds = stringArray(oracle.requirement_ids, `${context}.requirement_ids`);
    for (const id of requirementIds) {
      if (!EXPECTED_REQUIREMENTS.includes(id)) fail("REQUIREMENT_REF_UNRESOLVED", `${context}: ${id}`);
      else coveredRequirements.add(id);
      acceptanceEdges.add(`${id}|${String(oracle.trace_test_id)}`);
      const row = traceRows.get(id);
      if (!row || !Array.isArray(row.test_ids) || !row.test_ids.includes(oracle.trace_test_id)) {
        fail("TRACE_ACCEPTANCE_EDGE_MISMATCH", `${oracle.oracle_id}: ${id} lacks ${String(oracle.trace_test_id)}`);
      }
    }
    const tasks = validateTaskIds(oracle.implementation_tasks, `${context}.implementation_tasks`, taskIds, "ORACLE_TASKS_EMPTY");
    const traceTest = traceTests[oracle.trace_test_id];
    if (!isObject(traceTest)) {
      fail("TRACE_TEST_UNRESOLVED", `${context}: ${String(oracle.trace_test_id)}`);
    } else {
      const expectedCommand = traceCommands[traceTest.command_id];
      if (oracle.test !== traceTest.target || oracle.command !== expectedCommand) {
        fail("TRACE_ACCEPTANCE_EDGE_MISMATCH", `${oracle.oracle_id}: test or command differs from ${oracle.trace_test_id}`);
      }
      const owners = Array.isArray(traceTest.owner_tasks) ? traceTest.owner_tasks : [];
      if (!owners.some((owner) => tasks.includes(owner))) fail("TEST_TASK_OWNERSHIP_INVALID", `${oracle.oracle_id}: no trace-test owner task`);
      validateTestAndCommand(oracle.test, oracle.command, context, traceTest.kind);
    }
    for (const seam of stringArray(oracle.production_seams, `${context}.production_seams`)) {
      if (/^src\/lifecycle\//u.test(seam) || /^src\/(?:watcher|sidecar|snapshot)\.rs(?:::|$)/u.test(seam)) fail("SEAM_NAMESPACE_INVALID", `${context}: ${seam}`);
      if (!EXPECTED_PRODUCTION_SEAMS.has(seam) || !pathIsExistingOrDeclared(taskCatalog, seamPath(seam))) fail("SEAM_UNRESOLVED", `${context}: ${seam}`);
    }
    stringArray(oracle.preconditions, `${context}.preconditions`);
    stringArray(oracle.actions, `${context}.actions`);
    stringArray(oracle.assertions, `${context}.assertions`);
    nonEmptyString(oracle.positive_control, `${context}.positive_control`);
    stringArray(oracle.negative_controls, `${context}.negative_controls`);
    stringArray(oracle.bounds, `${context}.bounds`);
    stringArray(oracle.fairness, `${context}.fairness`);
    if (!nonEmptyString(oracle.ci_artifact, `${context}.ci_artifact`) || !/^target\/ci\/lifecycle-v11\/[A-Za-z0-9_.-]+\.json$/u.test(oracle.ci_artifact || "")) fail("CI_ARTIFACT_INVALID", context);
    validatePlanned(oracle, context, true);
  }
  for (const [oracleId] of EXPECTED_ACCEPTANCE_ORACLES) if (!oracleIds.has(oracleId)) fail("ORACLE_ID_MISSING", oracleId);
  validateAmendmentRegressionBindings(oracles);
  for (const [category, expectedCount] of expectedCategoryCounts) {
    const count = categoryCounts.get(category) || 0;
    if (count !== expectedCount) fail("ORACLE_CATEGORY_COUNT", `${category}: ${count}`);
  }
  for (const category of categoryCounts.keys()) if (!expectedCategoryCounts.has(category)) fail("ORACLE_CATEGORY_EXTRA", String(category));
  if (EXPECTED_REQUIREMENTS.some((id) => !coveredRequirements.has(id)) || [...coveredRequirements].some((id) => !EXPECTED_REQUIREMENTS.includes(id))) {
    fail("ORACLE_REQUIREMENT_COVERAGE_INVALID", `covered=${coveredRequirements.size}, expected=${EXPECTED_REQUIREMENTS.length}`);
  }
  for (const row of traceRows.values()) {
    for (const testId of Array.isArray(row.test_ids) ? row.test_ids : []) {
      if (!acceptanceEdges.has(`${row.requirement_id}|${testId}`)) {
        fail("REVERSE_ACCEPTANCE_EDGE_INVALID", `${row.requirement_id}: ${testId} has no acceptance oracle edge`);
      }
    }
  }

  const ingress = oracles.find((oracle) => isObject(oracle) && oracle.oracle_id === "ORACLE-INGRESS-CLOSED-SURFACE");
  const expectedIngressSemantics = {
    actions: [
      "Invoke every ingress in both surface modes against Current, Stale, Unavailable, Stopping, and foreign-root states",
      "Require exactly one result branch: GenerationLeased, DiskObserved, WorktreeScopeObserved, GitObserved, RuntimeHealthObserved, MutationPermitted, StateWriteAuthorized, or Refused",
    ],
    assertions: [
      "GenerationLeased is the only branch that acquires a ProjectQueryLease",
      "DiskObserved, WorktreeScopeObserved, GitObserved, and RuntimeHealthObserved remain lease-free and provenance-bounded",
      "RuntimeHealthObserved separates committed-generation fields from bounded attempt and runtime-work fields",
      "MutationPermitted is used only for repository-source byte writes and holds a current SourceMutationPermit",
      "StateWriteAuthorized covers ProjectStateDir writes and exact post-image team-artifact receipt finalization without minting or requiring a SourceMutationPermit",
      "Refused is typed and cannot fall through to another branch",
      "detect_changes routes to detect_impact as GitObserved or WorktreeScopeObserved and never upgrades to GenerationLeased",
      "No alias, prompt, resource, sidecar, hook, or daemon callback reaches V10 authority",
    ],
    negative_controls: [
      "An unlisted ingress fixture fails the closed inventory",
      "A pure observation cannot claim Current or acquire a ProjectQueryLease",
      "A ProjectStateDir or post-image receipt write cannot mint or require a source mutation permit",
      "A compatibility alias that bypasses V11 selection fails",
      "One ingress cannot resolve to two authority branches",
    ],
    fairness: ["No ingress class is omitted because another class passes"],
    positive_control: "Each of the seven authorized branches succeeds independently under its own authority contract and Refused terminates selection.",
  };
  if (!isObject(ingress) || Object.entries(expectedIngressSemantics).some(([key, expected]) => JSON.stringify(ingress[key]) !== JSON.stringify(expected))) {
    fail("INGRESS_LANE_CONTRACT_INVALID", "ingress assertions must preserve pure observation and permit-free state lanes");
  }
  const health = oracles.find((oracle) => isObject(oracle) && oracle.oracle_id === "ORACLE-HEALTH-COMMITTED-VS-ATTEMPT");
  const expectedHealthAssertions = [
    "RuntimeHealthObserved never acquires a ProjectQueryLease or claims a strict Current answer",
    "Attempt evidence never populates or changes committed digest, equality, coverage, or source-truth fields",
    "Committed-generation fields remain bound to the retained publication until a complete candidate promotes",
    "Attempt bytes-by-stage, safe causes, retry/reconciliation, snapshot-verification, and runtime-work fields are bounded and separately labeled",
    "health, health_compact, status, and health resources agree on the committed-versus-attempt partition",
  ];
  if (!isObject(health) || JSON.stringify(health.requirement_ids) !== JSON.stringify(["FR-021"]) || JSON.stringify(health.implementation_tasks) !== JSON.stringify(["T056", "T063"]) || JSON.stringify(health.assertions) !== JSON.stringify(expectedHealthAssertions)) {
    fail("HEALTH_ORACLE_INVALID", "FR-021 health must keep committed-generation truth separate from bounded attempt and runtime-work evidence");
  }
  const expectedObserverModel = ["Unregistered", "Registering", "Replaying", "Live", "GapLatched", "OverflowLatched", "VerificationOverdueLatched", "Stopped"];
  const observerModel = trace && trace.catalogs && trace.catalogs.state_models && trace.catalogs.state_models["MODEL-OBSERVER"];
  const rollingVerification = oracles.find((oracle) => isObject(oracle) && oracle.oracle_id === "ORACLE-ROLLING-VERIFICATION-COVERAGE");
  const expectedRequirementIds = ["FR-011", "FR-031", "FR-039", "FR-049", "FR-052", "SC-003", "SC-004", "SC-009"];
  const expectedImplementationTasks = ["T055", "T062", "T063", "T065"];
  const requiredOverdueAssertions = [
    "Partial progress, cancellation, retry, cursor resume, persistence, and restart never extend or reconstruct verification_deadline",
    "At or after verification_deadline without a newer exact-bound complete record, VerificationOverdueLatched becomes non-Current before any strict lease linearizes and returns SourceRefusal",
  ];
  const requiredOverdueBounds = [
    "MAX_CURRENT_UNVERIFIED_AGE is exactly 15 minutes and cannot be infinite, disabled, or configured upward",
  ];
  if (JSON.stringify(observerModel) !== JSON.stringify(expectedObserverModel) || !isObject(rollingVerification) ||
      JSON.stringify(rollingVerification.requirement_ids) !== JSON.stringify(expectedRequirementIds) ||
      JSON.stringify(rollingVerification.implementation_tasks) !== JSON.stringify(expectedImplementationTasks) ||
      requiredOverdueAssertions.some((value) => !Array.isArray(rollingVerification.assertions) || !rollingVerification.assertions.includes(value)) ||
      requiredOverdueBounds.some((value) => !Array.isArray(rollingVerification.bounds) || !rollingVerification.bounds.includes(value))) {
    fail("OVERDUE_VERIFICATION_ORACLE_INVALID", "the finite deadline, overdue latch, exact-bound refresh, and strict-refusal oracle must remain closed");
  }
  const expectedVerificationPassSemantics = {
    preconditions: [
      "A Current root carries a sealed VerificationScopeReceipt, a complete VerificationRecord, a feasible VerificationWorkBound, and a monotonic verification_deadline exactly 15 minutes after that record completed",
      "The monotonic clock, rolling cursor, observer health, strict-lease linearization, capacity reservation, and independently corruptible scopes are controllable",
    ],
    actions: [
      "Build the sealed scope from every canonical catalog entry, admitted byte range, required derived artifact and certificate, policy version, and stable observer cut",
      "Advance rolling verification through partial, cancelled, resumed, restarted, and complete whole-declared-scope passes at the maximum default work bound",
    ],
    assertions: [
      "A complete whole-declared-scope pass performs authoritative stable-cut rescans at both boundaries, proves an exact path/disposition bijection to the sealed scope, verifies every catalog entry, rehashes every admitted byte range, recomputes every required derived artifact and certificate, and finishes with zero missing, extra, skipped, or unresolved obligations",
      "Only a complete whole-declared-scope VerificationRecord bound to the exact project slot, source slot, generation digest, observer cut, policy version, and sealed VerificationScopeReceipt advances verification_deadline",
      "VerificationWorkBound records verification_bytes as admitted-source plus required-artifact bytes and verification_entries as catalog, disposition, discovery, and artifact obligations; ceil(verification_bytes / 33554432) plus ceil(verification_entries / 1000) seconds is at most 720",
      "Current promotion requires a VerificationFeasibilityReceipt reserving at least 33554432 verification bytes per second and 1000 verification entries per second, and the successor pass starts no later than 180 seconds after the prior completion",
    ],
    negative_controls: [
      "A scope with any omitted catalog entry, disposition, admitted byte range, discovery obligation, derived artifact, or certificate cannot emit a complete VerificationRecord",
      "A default scope above 17179869184 bytes, above 200000 entries, without both reserved service floors, or whose computed bound exceeds 720 seconds remains non-Current with SourceRefusal",
    ],
    bounds: [
      "DEFAULT_MAX_VERIFICATION_BYTES is 17179869184, DEFAULT_MAX_VERIFICATION_ENTRIES is 200000, reserved floors are 33554432 bytes per second and 1000 entries per second, and DEFAULT_MAX_COMPLETE_VERIFICATION_PASS is 720 seconds",
      "The successor pass starts within 180 seconds, so the maximum reachable 712-second pass completes before the fixed 900-second overdue deadline",
    ],
    fairness: [
      "A feasible successor pass starts no later than 180 seconds after the prior complete record and retains its reserved service floors until completion or typed refusal",
      "A source that cannot obtain or retain that reservation becomes non-Current rather than extending the deadline",
    ],
  };
  if (!isObject(rollingVerification) || Object.entries(expectedVerificationPassSemantics).some(([key, expected]) =>
    expected.some((value) => !Array.isArray(rollingVerification[key]) || !rollingVerification[key].includes(value)))) {
    fail("VERIFICATION_PASS_ORACLE_INVALID", "whole-scope completion and its finite default feasibility bound must remain closed");
  }
  const capacity = oracles.find((oracle) => isObject(oracle) && oracle.oracle_id === "ORACLE-CAPACITY-PHYSICAL-OWNERSHIP");
  const expectedCapacitySemantics = {
    preconditions: [
      "Finite process, project, source, residency, replacement-headroom, and response-reservation ceilings are configured",
      "Safety precharge, reservation, allocation construction, physical drop, logical cancellation, drain barriers, and resize cleanup are independently pausable",
    ],
    actions: [
      "Safety-precharge before allocation construction and pause at precharge, reservation, construction, charge transfer, ownership, and physical drop",
      "Queue multidimensional requests so the oldest request is unsatisfiable, a younger conflicting request is satisfiable, and a younger disjoint request can progress",
      "Exercise bounded bypass, the applicable drain barrier, cancellation, panic, pin-aware parking, and resize cleanup-before-requeue",
    ],
    assertions: [
      "Safety precharge is held before construction and is returned exactly once if reservation or construction fails",
      "Reservation to construction to charged physical ownership conserves every unit in every configured dimension",
      "Multidimensional dispatch selects the oldest satisfiable request, permits only bounded bypass, blocks conflicting work at the applicable drain barrier, and allows disjoint-dimension progress",
      "Logical cancellation never refunds live physical ownership",
      "Resize cleanup completes before requeue and retained ownership remains charged until physical drop",
    ],
    negative_controls: [
      "Construction without safety precharge fails",
      "Double return, early cancellation refund, or partial-dimension transfer fails conservation",
      "Unbounded bypass or a younger conflicting request crossing a drain barrier fails",
      "A cancelled or reused-incarnation waiter cannot be granted",
      "Resize requeue before cleanup fails",
    ],
    fairness: [
      "Oldest satisfiable request first within bounded bypass",
      "An applicable drain barrier eventually blocks conflicting grants",
      "Disjoint-dimension progress remains possible",
      "Pin-aware parking retains charges",
      "Cleanup finishes before resize requeue",
    ],
  };
  if (!isObject(capacity) || Object.entries(expectedCapacitySemantics).some(([key, expected]) => JSON.stringify(capacity[key]) !== JSON.stringify(expected))) {
    fail("CAPACITY_ORACLE_INVALID", "capacity oracle must freeze oldest-satisfiable multidimensional conservation semantics");
  }

  const publication = oracles.find((oracle) => isObject(oracle) && oracle.oracle_id === "ORACLE-PUBLICATION-WHOLE-ROOT");
  const expectedPublicationSemantics = {
    actions: [
      "Pause candidate A at T017's final commit point",
      "Publish candidate B as the new whole-project root",
      "Resume A, force it to rebase against the latest root, and inject both same-source retryable and same-source terminal-conflict outcomes",
      "Observe the publication store count and every strict query across the schedule",
    ],
    assertions: [
      "A resumed candidate rebases against the latest root and preserves B's latest sibling updates",
      "A same-source retryable conflict retries from the latest root while a terminal conflict aborts without publication",
      "Candidate tokens are compared only for opaque equality and never by numeric order",
      "Resuming A performs exactly one whole-project store on success and zero stores on retry or abort paths",
      "No strict query observes a partial, mixed, or superseded root",
    ],
  };
  if (!isObject(publication) || Object.entries(expectedPublicationSemantics).some(([key, expected]) => JSON.stringify(publication[key]) !== JSON.stringify(expected))) {
    fail("PUBLICATION_ORACLE_INVALID", "publication oracle must freeze the T017 pause-A/publish-B/resume-A latest-root rebase schedule");
  }

  const performance = oracles.find((oracle) => isObject(oracle) && oracle.oracle_id === "ORACLE-PERFORMANCE-OBSERVED-REFRESH");
  const expectedPerformanceOwnership = {
    implementation_tasks: ["T068", "T069", "T070", "T071"],
    production_seams: [
      "src/index_lifecycle/candidate.rs::CandidateHandle",
      "src/index_lifecycle/capacity.rs::ProcessCapacityPool",
      "src/index_lifecycle/observer.rs::ObserverHandoff",
      "src/index_lifecycle/runtime.rs::ProjectIndexRuntime",
    ],
  };
  if (!isObject(performance) || Object.entries(expectedPerformanceOwnership).some(([key, expected]) => JSON.stringify(performance[key]) !== JSON.stringify(expected))) {
    fail("PERFORMANCE_ORACLE_INVALID", "SC-024 must be owned by T068-T071 and the frozen candidate/capacity/observer/runtime seams");
  }
  const expectedPerformanceBoundary = {
    preconditions: [
      "The frozen observed-refresh corpus, benchmark registration, environment record, semantic-equivalence gate, retained-plus-candidate headroom record, and baseline 1521abb0 are present",
      "Before the measured burst, pregranted, retained, candidate, declared_scratch, and declared_headroom vectors exist with exactly process_slots, project_slots, source_slots, residency_bytes, replacement_headroom_bytes, and response_reservation_bytes",
    ],
    actions: [
      "Measure from a completed external write burst or SymForge mutation commit to the first strict lease carrying that exact byte identity",
      "Run single-path hint, Gap, ScopeDirty, initial index, manual rebuild, and recovery rebuild trigger classes",
      "Record p95, maximum, frozen-baseline ratio, corpus hash, environment, completion receipts, candidate-build reason, and all five frozen capacity vectors",
    ],
    assertions: [
      "Observed refresh p95 is at most 2 seconds",
      "Observed refresh maximum is at most 5 seconds",
      "Observed refresh p95 is at most 1.25x baseline 1521abb0",
      "A single-path hint cannot trigger a full candidate outside observer Gap or ScopeDirty",
      "Initial indexing, explicit manual rebuild, and recovery rebuild remain legal full-candidate triggers",
      "The first measured strict lease carries the completed burst or mutation-commit byte identity",
      "For every frozen capacity dimension d, retained[d] plus candidate[d] is at most pregranted[d] plus declared_scratch[d] plus declared_headroom[d]",
      "The benchmark target, registration, command, and completion receipt are task-owned by T068 through T071",
    ],
  };
  if (!isObject(performance) || Object.entries(expectedPerformanceBoundary).some(([key, expected]) => JSON.stringify(performance[key]) !== JSON.stringify(expected))) {
    fail("SC024_BOUNDARY_INVALID", "SC-024 must measure completed burst to first exact-byte strict lease with pre-granted vector/scratch/headroom and a narrow single-path trigger rule");
  }

  const migration = oracles.find((oracle) => isObject(oracle) && oracle.oracle_id === "ORACLE-MIGRATION-DELTA-EQUIVALENCE");
  const forbiddenMigrationClaim = /(?:benchmark|performance|median|p95|maximum|baseline|measurement window|warmup)/iu;
  if (!isObject(migration) || ["preconditions", "actions", "assertions", "positive_control", "negative_controls", "bounds", "fairness"].some((key) => {
    const value = migration[key];
    return forbiddenMigrationClaim.test(Array.isArray(value) ? value.join("\n") : String(value));
  })) {
    fail("MIGRATION_ORACLE_INVALID", "migration oracle is semantic equivalence only and cannot claim benchmark authority");
  }

  const state = oracles.find((oracle) => isObject(oracle) && oracle.oracle_id === "ORACLE-STATE-TYPED-OWNERS");
  const expectedStateNegativeControls = [
    "A protected placement that cannot establish user-local fallback returns a typed refusal without repository-local writes",
    "Memory-only placement returns typed persistence refusal",
    "A foreign or nested state path cannot redirect source or query authority",
  ];
  if (!isObject(state) || JSON.stringify(state.negative_controls) !== JSON.stringify(expectedStateNegativeControls)) {
    fail("PROTECTED_STATE_FALLBACK_INVALID", "protected state must attempt user-local fallback before memory-only typed refusal");
  }
}

function listRustSourceFiles(root) {
  const files = [];
  const visit = (absolute, relative) => {
    let entries;
    try {
      entries = fs.readdirSync(absolute, { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries.sort((left, right) => left.name.localeCompare(right.name))) {
      const childRelative = `${relative}/${entry.name}`.replaceAll("\\", "/");
      const childAbsolute = path.join(absolute, entry.name);
      if (entry.isDirectory()) visit(childAbsolute, childRelative);
      else if (entry.isFile() && entry.name.endsWith(".rs")) files.push(childRelative);
    }
  };
  visit(path.join(root, "src"), "src");
  return files.sort();
}

function currentRustSourceMap() {
  const result = new Map();
  for (const file of listRustSourceFiles(repositoryRoot)) {
    try {
      result.set(file, fs.readFileSync(path.join(repositoryRoot, file), "utf8"));
    } catch {
      fail("RETIREMENT_SOURCE_READ", file);
    }
  }
  return result;
}

function gitRustSourceMap(commit) {
  const listing = runGit(["ls-tree", "-rz", "--name-only", commit, "--", "src"]);
  if (!listing.ok) {
    fail("RETIREMENT_SOURCE_READ", `${commit}: src tree`);
    return new Map();
  }
  const files = listing.stdout.toString("utf8").split("\0").filter((file) => file.endsWith(".rs")).sort();
  const result = new Map();
  for (const file of files) {
    const blob = gitBlob(commit, file);
    if (!Buffer.isBuffer(blob)) fail("RETIREMENT_SOURCE_READ", `${commit}:${file}`);
    else result.set(file, blob.toString("utf8"));
  }
  return result;
}

function topLevelParts(value) {
  const parts = [];
  const stack = [];
  let start = 0;
  const pairs = { "(": ")", "[": "]", "{": "}", "<": ">" };
  for (let index = 0; index < value.length; index += 1) {
    const token = value[index];
    if (Object.prototype.hasOwnProperty.call(pairs, token)) stack.push(pairs[token]);
    else if (stack.at(-1) === token) stack.pop();
    else if (token === "," && stack.length === 0) {
      parts.push(value.slice(start, index).trim());
      start = index + 1;
    }
  }
  parts.push(value.slice(start).trim());
  return parts.filter((part) => part !== "");
}

function rustImplIntervals(code) {
  const intervals = [];
  for (const match of code.matchAll(/\bimpl\b([^{};]*)\{/gu)) {
    const header = match[1].replace(/\bwhere\b[\s\S]*$/u, "");
    const identifiers = [...header.matchAll(/[A-Za-z_][A-Za-z0-9_]*/gu)].map((item) => item[0]);
    const owner = identifiers.at(-1);
    const open = match.index + match[0].lastIndexOf("{");
    const close = matchingRustBrace(code, open);
    if (owner && close !== -1) intervals.push({ owner, open, close });
  }
  return intervals;
}

function rustItemsInFile(file, source) {
  const code = maskRustCommentsAndLiterals(source);
  const impls = rustImplIntervals(code);
  const testModules = rustModuleIntervals(code).filter((interval) => interval.name === "tests" || interval.name === "test");
  const items = [];
  const pattern = /\b((?:pub(?:\s*\([^)]*\))?\s+)?(?:(?:async|unsafe|const)\s+)*)fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?:<[^>{}]*>)?\s*\(/gu;
  for (const match of code.matchAll(pattern)) {
    if (testModules.some((interval) => interval.open < match.index && match.index < interval.close)) continue;
    const body = rustDeclarationBody(code, match.index);
    if (body === null) continue;
    const rawOpen = code.indexOf("{", match.index);
    const rawClose = matchingRustBrace(code, rawOpen);
    const containing = impls.filter((interval) => interval.open < match.index && match.index < interval.close)
      .sort((left, right) => right.open - left.open)[0];
    const owner = containing ? containing.owner : null;
    const name = match[2];
    items.push({
      file,
      owner,
      name,
      anchor: `${file}::${owner ? `${owner}::` : ""}${name}`,
      visibility: /\bpub\b/u.test(match[1]) ? "pub" : "private",
      body,
      rawBody: rawClose === -1 ? "" : source.slice(rawOpen + 1, rawClose),
      at: match.index,
    });
  }
  return items;
}

function rustStructFieldsInFile(file, source) {
  const code = maskRustCommentsAndLiterals(source);
  const testModules = rustModuleIntervals(code).filter((interval) => interval.name === "tests" || interval.name === "test");
  const fields = [];
  for (const match of code.matchAll(/\bstruct\s+([A-Za-z_][A-Za-z0-9_]*)[^;{]*\{/gu)) {
    if (testModules.some((interval) => interval.open < match.index && match.index < interval.close)) continue;
    const owner = match[1];
    const open = match.index + match[0].lastIndexOf("{");
    const close = matchingRustBrace(code, open);
    if (close === -1) continue;
    const body = code.slice(open + 1, close);
    for (const part of topLevelParts(body)) {
      const field = /^\s*(?:#\s*\[[\s\S]*?\]\s*)*(?:pub(?:\s*\([^)]*\))?\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*:\s*([\s\S]+)$/u.exec(part);
      if (field) fields.push({ file, owner, name: field[1], type: field[2].trim(), anchor: `${file}::${owner}::${field[1]}` });
    }
  }
  return fields;
}

function attributedRustFunctions(source, attributeName) {
  const code = maskRustCommentsAndLiterals(source);
  const result = [];
  const pattern = new RegExp("#\\s*\\[\\s*" + escapeRegExp(attributeName) + "\\s*\\(", "gu");
  for (const match of code.matchAll(pattern)) {
    const open = code.indexOf("(", match.index);
    let depth = 0;
    let close = -1;
    for (let index = open; index < code.length; index += 1) {
      if (code[index] === "(") depth += 1;
      else if (code[index] === ")") {
        depth -= 1;
        if (depth === 0) { close = index; break; }
      }
    }
    if (close === -1) continue;
    const functionMatch = /\bfn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(/gu;
    functionMatch.lastIndex = close;
    const fn = functionMatch.exec(code);
    if (!fn || fn.index - close > 256) continue;
    const attribute = source.slice(open + 1, close);
    const explicitName = /\bname\s*=\s*"([A-Za-z0-9_-]+)"/u.exec(attribute);
    result.push({
      functionName: fn[1],
      publishedName: explicitName ? explicitName[1] : fn[1],
      attribute,
      at: fn.index,
    });
  }
  return result;
}

function rawRustFunctionBody(source, name) {
  const code = maskRustCommentsAndLiterals(source);
  const match = new RegExp(`\\bfn\\s+${escapeRegExp(name)}\\s*(?:<[^>{}]*>)?\\s*\\(`, "u").exec(code);
  if (!match) return null;
  const open = code.indexOf("{", match.index);
  const close = matchingRustBrace(code, open);
  return open === -1 || close === -1 ? null : source.slice(open + 1, close);
}

function deriveToolProfiles(sourceMap) {
  const routerTools = new Set();
  for (const source of sourceMap.values()) {
    for (const item of attributedRustFunctions(source, "tool")) routerTools.add(item.publishedName);
  }
  const surface = sourceMap.get("src/protocol/surface_probe.rs") || "";
  const compactBody = rawRustFunctionBody(surface, "compact_probe_tools") || "";
  const compact = [...compactBody.matchAll(/\bprobe_tool\s*\(\s*"([A-Za-z0-9_-]+)"/gu)].map((match) => match[1]).sort();
  const full = [...routerTools].filter((name) => name !== "symforge").sort();
  const union = [...new Set([...full, ...compact])].sort();
  return { full, compact, union };
}

function deriveResources(sourceMap) {
  const source = sourceMap.get("src/protocol/resources.rs") || "";
  return [...new Set([...source.matchAll(/\b(?:const|static)\s+[A-Za-z_][A-Za-z0-9_]*\s*:\s*&str\s*=\s*"(symforge:\/\/[^"\r\n]+)"/gu)]
    .map((match) => match[1].split("?", 1)[0]))].sort();
}

function derivePrompts(sourceMap) {
  const result = new Set();
  for (const source of sourceMap.values()) {
    for (const item of attributedRustFunctions(source, "prompt")) result.add(item.publishedName);
  }
  return [...result].sort();
}

function deriveSidecarRoutes(sourceMap) {
  const result = new Set();
  for (const file of ["src/sidecar/router.rs", "src/daemon.rs"]) {
    const source = sourceMap.get(file) || "";
    for (const match of source.matchAll(/\.route\s*\(\s*"([^"\r\n]+)"\s*,\s*get\s*\(/gu)) {
      if (file !== "src/daemon.rs" || match[1].includes("/sidecar/")) result.add(`GET ${match[1]}`);
    }
  }
  return [...result].sort();
}

function deriveHooks(sourceMap) {
  const source = sourceMap.get("src/cli/mod.rs") || "";
  const code = maskRustCommentsAndLiterals(source);
  const match = /\benum\s+HookSubcommand\s*\{/u.exec(code);
  if (!match) return [];
  const open = code.indexOf("{", match.index);
  const close = matchingRustBrace(code, open);
  if (close === -1) return [];
  return topLevelParts(code.slice(open + 1, close)).map((part) => {
    const withoutAttributes = part.replace(/^(?:\s*#\s*\[[\s\S]*?\]\s*)+/u, "");
    return /^([A-Za-z_][A-Za-z0-9_]*)/u.exec(withoutAttributes);
  }).filter(Boolean).map((item) => `hook:${item[1]}`).sort();
}

function deriveCompatibilityAliases(sourceMap, advertisedTools) {
  const source = sourceMap.get("src/daemon.rs") || "";
  const body = rawRustFunctionBody(source, "execute_tool_call") || "";
  const advertised = new Set(advertisedTools);
  return [...new Set([...body.matchAll(/^\s*"([a-z][a-z0-9_]*)"\s*=>/gmu)]
    .map((match) => match[1]).filter((name) => !advertised.has(name)))].sort();
}

function parsePublicApiManifest() {
  const text = readText(PUBLIC_API_PATH);
  if (text === null) return null;
  try {
    return JSON.parse(text);
  } catch (error) {
    fail("JSON_PARSE", `${PUBLIC_API_PATH}: ${error.message}`);
    return null;
  }
}

function derivePublicApiAtoms(sourceMap) {
  const result = new Set(["symforge"]);
  const lib = sourceMap.get("src/lib.rs") || "";
  for (const match of lib.matchAll(/^\s*pub\s+mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;/gmu)) result.add(`symforge::${match[1]}`);
  const embed = sourceMap.get("src/embed.rs") || "";
  for (const match of embed.matchAll(/^\s*pub\s+(?:async\s+)?(?:fn|struct|enum|type|trait|const|static)\s+([A-Za-z_][A-Za-z0-9_]*)/gmu)) {
    result.add(`symforge::embed::${match[1]}`);
  }
  for (const match of embed.matchAll(/\bpub\s+use\s+crate::([\s\S]*?);/gu)) {
    const expression = match[1].trim();
    const open = expression.indexOf("{");
    const close = expression.lastIndexOf("}");
    const leaves = open === -1 || close < open ? [expression.split("::").at(-1)] : topLevelParts(expression.slice(open + 1, close));
    for (const leaf of leaves) {
      const alias = /\bas\s+([A-Za-z_][A-Za-z0-9_]*)\s*$/u.exec(leaf);
      const identifiers = [...leaf.matchAll(/[A-Za-z_][A-Za-z0-9_]*/gu)].map((item) => item[0]);
      const name = alias ? alias[1] : identifiers.at(-1);
      if (name && name !== "self") result.add(`symforge::embed::${name}`);
    }
  }
  return [...result].sort();
}

function migrationAtoms(manifest, decisions = null) {
  const categories = manifest && manifest.migration_v10 && manifest.migration_v10.categories;
  if (!Array.isArray(categories)) return [];
  const atoms = [];
  for (const category of categories) {
    if (!isObject(category) || !Array.isArray(category.atoms) || (decisions && !decisions.has(category.decision))) continue;
    atoms.push(...category.atoms);
  }
  return atoms.sort();
}

function directPublicAtoms(atoms) {
  return [...new Set((Array.isArray(atoms) ? atoms : [])
    .filter((atom) => typeof atom === "string" && atom.split("::").length <= 3))].sort();
}

function deriveLifecyclePublicAtoms(sourceMap, manifest) {
  const result = new Set(derivePublicApiAtoms(sourceMap));
  const introduced = manifest && manifest.migration_v10 && manifest.migration_v10.introduced_v11_atoms;
  const modules = new Set((Array.isArray(introduced) ? introduced : [])
    .filter((atom) => typeof atom === "string" && atom.split("::").length >= 2)
    .map((atom) => atom.split("::")[1])
    .filter((module) => module !== "embed"));
  for (const module of modules) {
    if (!result.has(`symforge::${module}`)) continue;
    const source = sourceMap.get(`src/${module}.rs`) || sourceMap.get(`src/${module}/mod.rs`) || "";
    for (const match of source.matchAll(/^\s*pub\s+(?:async\s+)?(?:fn|struct|enum|type|trait|const|static)\s+([A-Za-z_][A-Za-z0-9_]*)/gmu)) {
      result.add(`symforge::${module}::${match[1]}`);
    }
  }
  return [...result].sort();
}

function ordinaryRetirementLifecycle(sourceMap, manifest) {
  const actual = deriveLifecyclePublicAtoms(sourceMap, manifest);
  const preactivation = directPublicAtoms(migrationAtoms(manifest));
  const categories = manifest && manifest.migration_v10 && manifest.migration_v10.categories;
  const kept = Array.isArray(categories)
    ? categories.filter((category) => isObject(category) && category.decision === "keep")
      .flatMap((category) => Array.isArray(category.atoms) ? category.atoms : [])
    : [];
  const introduced = manifest && manifest.migration_v10 && manifest.migration_v10.introduced_v11_atoms;
  const postactivation = directPublicAtoms([...kept, ...(Array.isArray(introduced) ? introduced : [])]);
  if (JSON.stringify(actual) === JSON.stringify(preactivation)) return "preactivation";
  if (JSON.stringify(actual) === JSON.stringify(postactivation)) return "postactivation";
  fail(
    "RETIREMENT_LIFECYCLE_PHASE_INVALID",
    `public API is neither frozen preactivation (${preactivation.length}) nor postactivation (${postactivation.length}); actual=${actual.length}`,
  );
  return "invalid";
}

function validatePostactivationRetirement(sourceMap, manifest) {
  const categories = manifest && manifest.migration_v10 && manifest.migration_v10.categories;
  const introduced = new Set(manifest && manifest.migration_v10 && Array.isArray(manifest.migration_v10.introduced_v11_atoms)
    ? manifest.migration_v10.introduced_v11_atoms
    : []);
  const retired = new Set((Array.isArray(categories)
    ? categories.filter((category) => isObject(category) && ["remove", "replace"].includes(category.decision))
      .flatMap((category) => Array.isArray(category.atoms) ? category.atoms : [])
    : []).filter((atom) => !introduced.has(atom)));
  const reachableRetired = deriveLifecyclePublicAtoms(sourceMap, manifest).filter((atom) => retired.has(atom));
  if (reachableRetired.length > 0) {
    fail("POSTACTIVATION_RETIRED_API_REACHABLE", reachableRetired.join("|"));
  }
  for (const anchor of [...EXPECTED_PRODUCTION_SEAMS].sort()) {
    const file = seamPath(anchor);
    const source = sourceMap.get(file);
    if (typeof source !== "string" || !sourceAnchorResolvesText(anchor, source)) {
      fail("POSTACTIVATION_V11_SEAM_UNRESOLVED", anchor);
    }
  }
}

function deriveSemanticRetirementInventory(sourceMap) {
  const items = [...sourceMap.entries()].flatMap(([file, source]) => rustItemsInFile(file, source));
  const fields = [...sourceMap.entries()].flatMap(([file, source]) => rustStructFieldsInFile(file, source));
  const publicationRoots = fields.filter((field) => /\bSharedIndex(?:Handle)?\b/u.test(field.type) && (field.name === "index" || field.name === "project_indexes"))
    .map((field) => field.anchor);
  if ((sourceMap.get("src/live_index/store.rs") || "").includes("pub struct SharedIndexHandle")) {
    publicationRoots.push("src/live_index/store.rs::SharedIndexHandle");
  }
  publicationRoots.push(...items.filter((item) => item.file === "src/live_index/store.rs" && item.owner === "SharedIndexHandle" && item.visibility === "pub" && /^reload(?:_for_state_placement)?$/u.test(item.name)).map((item) => item.anchor));

  const cache = fields.filter((field) => ["bases", "symbol_cache", "working_set", "probe_cache", "detailed_fetches"].includes(field.name))
    .map((field) => field.anchor);
  if ((sourceMap.get("src/worktree.rs") || "").includes("pub struct WorktreeCache")) cache.push("src/worktree.rs::WorktreeCache");

  const writers = items.filter((item) =>
    (item.file === "src/cli/init.rs" && item.name === "run_init_with_paths") ||
    (item.file === "src/gitignore_hygiene.rs" && /^(?:atomic_replace|reconcile_(?:project|root)_gitignore)$/u.test(item.name)) ||
    (item.file === "src/live_index/persist.rs" && item.name === "ensure_gitattributes_merge_hint") ||
    (item.file === "src/live_index/single_file.rs" && /^(?:remove_file|update_file_from_disk)$/u.test(item.name)) ||
    (item.file === "src/protocol/edit.rs" && /^(?:atomic_write_file|guarded_atomic_write_file|execute_batch_(?:edit|rename|insert))$/u.test(item.name)) ||
    (item.file === "src/protocol/edit_tools.rs" && item.owner === "SymForgeServer" && /^(?:replace_symbol_body|insert_symbol|delete_symbol|edit_within_symbol|batch_edit|batch_rename|batch_insert)$/u.test(item.name)) ||
    (item.file === "src/protocol/knowledge_curation.rs" && /^(?:apply|write_policy|apply_reviewed_mutation|durable_replace|durable_replace_io)$/u.test(item.name)) ||
    (item.file === "src/protocol/tools.rs" && item.owner === "SymForgeServer" && item.name === "curate_knowledge")
  ).map((item) => item.anchor);

  const callbacks = items.filter((item) =>
    (item.file === "src/daemon.rs" && /^(?:spawn_local_ref_reconcile|start_project_watcher)$/u.test(item.name)) ||
    (item.file === "src/live_index/git_temporal.rs" && item.name === "spawn_git_temporal_computation") ||
    (item.file === "src/live_index/persist.rs" && item.name === "background_verify") ||
    (item.file === "src/main.rs" && item.name === "spawn_periodic_checkpoint") ||
    (item.file === "src/protocol/edit_hooks.rs" && /^(?:after_commit|resolve)$/u.test(item.name)) ||
    (item.file === "src/protocol/knowledge_curation.rs" && item.owner === "KnowledgeCurationCoordinator" && item.name === "recover_on_project_load") ||
    (item.file === "src/watcher/mod.rs" && /^(?:process_events|restart_watcher|start_watcher)$/u.test(item.name))
  ).map((item) => item.anchor);
  for (const [file, outer] of [["src/daemon.rs", "bootstrap_project_index"], ["src/main.rs", "run_local_mcp_server_async"]]) {
    const item = items.find((candidate) => candidate.file === file && candidate.name === outer);
    if (item && rustNestedCallExists(maskRustCommentsAndLiterals(item.rawBody), "spawn", "background_verify")) {
      callbacks.push(`${file}::${outer}::background_verify spawn`);
    }
  }
  const serverSource = sourceMap.get("src/server/serve.rs") || "";
  if (rustNestedCallExists(maskRustCommentsAndLiterals(serverSource), "spawn", "background_verify")) {
    callbacks.push("src/server/serve.rs::background_verify spawn");
  }

  const ccr = items.filter((item) => item.file === "src/protocol/ccr.rs" && /^(?:apply_ccr_overflow|enforce_token_budget_with_ccr|rewrite_footer_for_symforge_facade)$/u.test(item.name)).map((item) => item.anchor);
  if ((sourceMap.get("src/protocol/ccr.rs") || "").includes("pub struct CcrStore")) ccr.push("src/protocol/ccr.rs::CcrStore");

  const snapshotNames = new Set(["background_verify", "checkpoint_shared_index", "export_artifact", "import_portable_snapshot", "load_snapshot", "load_snapshot_for_root", "project_local_state_placement", "reset_snapshot_state", "serialize_shared_index", "snapshot_compatible", "snapshot_to_live_index", "snapshot_to_live_index_with_code_signals"]);
  const snapshot = items.filter((item) => item.file === "src/live_index/persist.rs" && snapshotNames.has(item.name)).map((item) => item.anchor);
  if ((sourceMap.get("src/live_index/persist.rs") || "").includes("pub struct IndexSnapshot")) snapshot.push("src/live_index/persist.rs::IndexSnapshot");

  return new Map([
    ["writers", [...new Set(writers)].sort()],
    ["callbacks", [...new Set(callbacks)].sort()],
    ["publication_roots", [...new Set(publicationRoots)].sort()],
    ["cache", [...new Set(cache)].sort()],
    ["ccr", [...new Set(ccr)].sort()],
    ["snapshot", [...new Set(snapshot)].sort()],
  ]);
}

function deriveRetirementSourceInventory(sourceMap) {
  const profiles = deriveToolProfiles(sourceMap);
  const manifest = parsePublicApiManifest();
  const publicAtoms = derivePublicApiAtoms(sourceMap);
  const classifiedAtoms = migrationAtoms(manifest);
  if (JSON.stringify(publicAtoms) !== JSON.stringify(classifiedAtoms)) {
    fail("RETIREMENT_PUBLIC_API_CLASSIFICATION_MISMATCH", "source public atoms must be classified exactly once by migration_v10");
  }
  if (profiles.full.length !== 39 || JSON.stringify(profiles.compact) !== JSON.stringify(["status", "symforge", "symforge_edit"])) {
    fail("RETIREMENT_TOOL_PROFILE_MISMATCH", `full=${profiles.full.length}, compact=${profiles.compact.join(",")}`);
  }
  return new Map([
    ...deriveSemanticRetirementInventory(sourceMap),
    ["tools", profiles.union],
    ["resources", deriveResources(sourceMap)],
    ["prompts", derivePrompts(sourceMap)],
    ["sidecar", deriveSidecarRoutes(sourceMap)],
    ["hooks", deriveHooks(sourceMap)],
    ["compatibility_aliases", deriveCompatibilityAliases(sourceMap, profiles.union)],
    ["raw_embed", migrationAtoms(manifest, new Set(["remove", "replace"]))],
  ]);
}

function validateRetirementSourceInventory(retirement, sourceMap) {
  const contract = new Map((Array.isArray(retirement && retirement.entries) ? retirement.entries : [])
    .filter(isObject).map((entry) => [entry.category, Array.isArray(entry.members) ? entry.members : []]));
  const derived = deriveRetirementSourceInventory(sourceMap);
  for (const category of RETIREMENT_CATEGORIES) {
    const expected = derived.get(category) || [];
    const actual = contract.get(category) || [];
    if (JSON.stringify(actual) !== JSON.stringify(expected)) {
      const actualSet = new Set(actual);
      const expectedSet = new Set(expected);
      const missing = expected.filter((member) => !actualSet.has(member));
      const extra = actual.filter((member) => !expectedSet.has(member));
      fail("RETIREMENT_SOURCE_INVENTORY_MISMATCH", `${category} missing_from_contract=${missing.join("|") || "-"} extra_in_contract=${extra.join("|") || "-"}`);
    }
  }
}

/// Does this `cfg` predicate compile ONLY under `cfg(test)`?
///
/// `test` is test-only. `all(..)` is test-only when any conjunct is, because
/// every conjunct must hold. `any(..)` is test-only only when every disjunct is,
/// because any one suffices. `not(..)` is never treated as test-only: `not(test)`
/// is the exact opposite, and anything subtler is not worth guessing about a
/// digest that gates a release.
///
/// Unknown shapes answer false, so an unrecognised predicate keeps its item in
/// the census rather than silently removing production code from it.
function cfgPredicateIsTestOnly(predicate) {
  const text = predicate.trim();
  if (text === "test") return true;
  const call = /^(all|any|not)\s*\(([\s\S]*)\)$/u.exec(text);
  if (!call) return false;
  const [, name, inner] = call;
  if (name === "not") return false;
  const parts = [];
  let depth = 0;
  let current = "";
  for (const character of inner) {
    if (character === "(") depth += 1;
    else if (character === ")") depth -= 1;
    if (character === "," && depth === 0) {
      parts.push(current);
      current = "";
      continue;
    }
    current += character;
  }
  if (current.trim() !== "") parts.push(current);
  const resolved = parts.map((part) => cfgPredicateIsTestOnly(part));
  if (resolved.length === 0) return false;
  return name === "all" ? resolved.some(Boolean) : resolved.every(Boolean);
}

/// Every `#[...]` attribute starting at `index`, as `{start, end, predicate}`.
/// `predicate` is the text inside `cfg(...)`, or null for any other attribute.
function rustAttributeAt(masked, index) {
  if (!masked.startsWith("#", index)) return null;
  let scan = index + 1;
  while (scan < masked.length && /\s/u.test(masked[scan])) scan += 1;
  if (masked[scan] !== "[") return null;
  let depth = 0;
  for (; scan < masked.length; scan += 1) {
    if (masked[scan] === "[") depth += 1;
    else if (masked[scan] === "]") {
      depth -= 1;
      if (depth === 0) {
        scan += 1;
        break;
      }
    }
  }
  const body = masked.slice(index, scan);
  const cfg = /^#\s*\[\s*cfg\s*\(([\s\S]*)\)\s*\]$/u.exec(body);
  return { start: index, end: scan, predicate: cfg ? cfg[1] : null };
}

/// Remove every item or statement that compiles only under `cfg(test)`.
///
/// The census exists to freeze V10 *authority* while V11 is built beside it, so
/// what it must pin is the source the release build compiles. Test-only code is
/// compiled out and changes no shipped behaviour; freezing it too made the
/// retirement contract forbid the very edits `tasks.md` T014 requires.
/// Production additions still move the digest, which is the property the closure
/// self-tests assert.
///
/// A run of consecutive attributes is taken as a unit: if any of them is a
/// test-only `cfg`, the whole run and its item go, so `#[derive(Debug)]
/// #[cfg(test)] struct X;` does not leave a stray attribute behind.
///
/// Offsets come from the masked source so a `;` or brace inside a comment or
/// string literal cannot terminate an item early.
function stripCfgTestItems(source) {
  const masked = maskRustCommentsAndLiterals(source);
  const cuts = [];
  let index = 0;
  while (index < masked.length) {
    const first = rustAttributeAt(masked, index);
    if (!first) {
      index += 1;
      continue;
    }
    // Collect the whole run of attributes that share this item.
    const run = [first];
    let cursor = first.end;
    for (;;) {
      while (cursor < masked.length && /\s/u.test(masked[cursor])) cursor += 1;
      const next = rustAttributeAt(masked, cursor);
      if (!next) break;
      run.push(next);
      cursor = next.end;
    }
    const testOnly = run.some(
      (attribute) => attribute.predicate !== null && cfgPredicateIsTestOnly(attribute.predicate),
    );
    if (!testOnly) {
      index = first.end;
      continue;
    }
    // Consume exactly the attributed construct, and nothing past it.
    //
    // An item ends at `;` or at a balanced `{…}` block. A struct field, enum
    // variant, or match arm ends at `,` instead, and the LAST member of a body
    // ends at the enclosing `}` with no separator at all. Scanning only for `;`
    // and `{` therefore ran straight through comma-separated members and out of
    // the enclosing block, deleting production siblings and following items from
    // the census: `pub struct S { #[cfg(test)] t: u8, prod: u8, } fn keep() {}`
    // reduced to `pub struct S {`, so renaming `prod` did not move the digest.
    // That is the exact drift the census exists to detect, so the scan is
    // bounded by all three terminators and by the enclosing brace.
    let scan = cursor;
    let nesting = 0;
    let end = -1;
    while (scan < masked.length) {
      const character = masked[scan];
      if (character === "(" || character === "[") nesting += 1;
      else if (character === ")" || character === "]") nesting -= 1;
      else if (nesting === 0 && (character === ";" || character === ",")) {
        end = scan + 1;
        break;
      } else if (nesting === 0 && character === "}") {
        // The enclosing body closed first: this was its final member, which owns
        // no separator. Stop before the brace so the body itself survives.
        end = scan;
        break;
      } else if (nesting === 0 && character === "{") {
        let depth = 0;
        let block = scan;
        for (; block < masked.length; block += 1) {
          if (masked[block] === "{") depth += 1;
          else if (masked[block] === "}") {
            depth -= 1;
            if (depth === 0) {
              block += 1;
              break;
            }
          }
        }
        end = block;
        // A block-bodied member (`#[cfg(test)] 0 => { … },`) still owns the
        // comma that follows it.
        let after = end;
        while (after < masked.length && /\s/u.test(masked[after])) after += 1;
        if (masked[after] === ",") end = after + 1;
        break;
      }
      scan += 1;
    }
    // No line-framing heuristics: whatever whitespace the cut leaves behind is
    // erased by `canonicalReleaseSource`, so where a removed item's blank lines
    // went is not observable in the digest.
    const cutEnd = end === -1 ? masked.length : end;
    cuts.push([first.start, cutEnd]);
    index = cutEnd;
  }
  if (cuts.length === 0) return source;
  let result = "";
  let position = 0;
  for (const [start, end] of cuts) {
    if (start < position) continue;
    result += source.slice(position, start);
    position = end;
  }
  return result + source.slice(position);
}

/// Reduce source to the code a release build compiles, in a canonical form.
///
/// Comments are dropped and runs of code whitespace collapse to a single space,
/// while string and character literals are emitted verbatim: their contents are
/// behaviour, so a change inside one must still move the digest. A comment
/// between two tokens becomes a separator rather than vanishing, so `a/*x*/b`
/// cannot canonicalize to `ab`.
///
/// Digesting this form rather than edited text is what makes the census
/// position-independent. Reformatting, re-wrapping a doc comment, or moving a
/// stripped test item's surrounding blank lines are all invisible; adding,
/// removing, or altering a single production token is not.
function canonicalReleaseSource(source) {
  const kinds = rustCharacterKinds(source);
  let result = "";
  let pendingSeparator = false;
  for (let index = 0; index < source.length; index += 1) {
    const kind = kinds[index];
    if (kind === "comment" || (kind === "code" && /\s/u.test(source[index]))) {
      pendingSeparator = true;
      continue;
    }
    if (pendingSeparator && result.length > 0) result += " ";
    pendingSeparator = false;
    result += source[index];
  }
  return result;
}

function normalizeRetirementClosureSource(source) {
  return canonicalReleaseSource(stripCfgTestItems(source.replace(/\r\n/gu, "\n")));
}

function validateRetirementClosure(retirement, sourceMap) {
  const closure = retirement && retirement.preactivation_closure;
  if (!exactKeys(closure, RETIREMENT_CLOSURE_CATEGORIES, "retirement.preactivation_closure")) return;
  const entries = new Map((Array.isArray(retirement.entries) ? retirement.entries : [])
    .filter(isObject).map((entry) => [entry.category, entry]));
  for (const category of RETIREMENT_CLOSURE_CATEGORIES) {
    const record = closure[category];
    const context = `retirement.preactivation_closure.${category}`;
    if (!exactKeys(record, ["paths", "digest"], context)) continue;
    const members = entries.has(category) && Array.isArray(entries.get(category).members) ? entries.get(category).members : [];
    const expectedPaths = [...new Set(members.filter((member) => typeof member === "string" && member.startsWith("src/"))
      .map(seamPath))].sort();
    const paths = stringArray(record.paths, `${context}.paths`);
    if (JSON.stringify(paths) !== JSON.stringify(expectedPaths)) {
      fail("RETIREMENT_CLOSURE_MISMATCH", `${category}: closure paths must equal every member-owned source path`);
      continue;
    }
    const blobHashes = {};
    for (const relativePath of paths) {
      const source = sourceMap.get(relativePath);
      if (typeof source !== "string") {
        fail("RETIREMENT_CLOSURE_MISMATCH", `${category}: missing ${relativePath}`);
        continue;
      }
      blobHashes[relativePath] = sha256Bytes(Buffer.from(normalizeRetirementClosureSource(source), "utf8"));
    }
    const actualDigest = canonicalDigest(`symforge.lifecycle.v11.retirement.closure.${category}`, blobHashes);
    if (!validSha256(record.digest) || record.digest !== actualDigest) {
      fail("RETIREMENT_CLOSURE_MISMATCH", `${category}: preactivation source census changed`);
    }
  }
}

function expectedRawEmbedAtoms() {
  const text = readText(PUBLIC_API_PATH);
  if (text === null) return [];
  let manifest;
  try {
    manifest = JSON.parse(text);
  } catch (error) {
    fail("JSON_PARSE", `${PUBLIC_API_PATH}: ${error.message}`);
    return [];
  }
  const categories = manifest && manifest.migration_v10 && manifest.migration_v10.categories;
  if (!Array.isArray(categories)) {
    fail("PUBLIC_API_MIGRATION_MISSING", PUBLIC_API_PATH);
    return [];
  }
  const atoms = [];
  for (const category of categories) {
    if (category && (category.decision === "remove" || category.decision === "replace")) {
      if (!Array.isArray(category.atoms)) {
        fail("PUBLIC_API_ATOMS_INVALID", String(category.id));
        continue;
      }
      atoms.push(...category.atoms);
    }
  }
  const unique = [...new Set(atoms)];
  if (unique.length !== atoms.length) fail("PUBLIC_API_ATOM_DUPLICATE", PUBLIC_API_PATH);
  return unique.sort();
}

function exactArray(actual, expected, code, context) {
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    fail(code, context);
  }
}

function canonicalJson(value) {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(",")}}`;
}

function canonicalDigest(domain, value) {
  return crypto.createHash("sha256")
    .update(domain, "utf8")
    .update(Buffer.from([0]))
    .update(canonicalJson(value), "utf8")
    .digest("hex");
}

function keyedRecords(records, idKey) {
  return Object.fromEntries((Array.isArray(records) ? records : []).filter(isObject).map((record) => [record[idKey], record]));
}

function validateFrozenDigest(key, value, code) {
  const spec = FROZEN_DIGESTS[key];
  if (!isObject(spec) || typeof spec.domain !== "string" || !/^[a-z0-9_.-]+$/u.test(spec.domain) || typeof spec.hash !== "string" || !/^[0-9a-f]{64}$/u.test(spec.hash)) {
    fail("CANONICAL_DIGEST_SPEC_INVALID", key);
    return;
  }
  if (canonicalDigest(spec.domain, value) !== spec.hash) fail(code, key);
}

function validateFrozenContracts(trace, acceptance, retirement) {
  exactArray(Object.keys(FROZEN_DIGESTS), FROZEN_DIGEST_KEYS, "CANONICAL_DIGEST_SPEC_INVALID", "frozen digest domain key set");
  if (isObject(trace)) {
    const catalogs = isObject(trace.catalogs) ? trace.catalogs : {};
    validateFrozenDigest("catalogs", catalogs, "FROZEN_CATALOGS_DIGEST_MISMATCH");
    validateFrozenDigest("requirement_rows", keyedRecords(trace.requirements, "requirement_id"), "FROZEN_REQUIREMENT_ROWS_DIGEST_MISMATCH");
    validateFrozenDigest("invariants", isObject(catalogs.invariants) ? catalogs.invariants : {}, "FROZEN_INVARIANTS_DIGEST_MISMATCH");
    validateFrozenDigest("state_models", isObject(catalogs.state_models) ? catalogs.state_models : {}, "FROZEN_STATE_MODELS_DIGEST_MISMATCH");
    validateFrozenDigest("release_validation", isObject(trace.release_validation) ? trace.release_validation : {}, "FROZEN_RELEASE_VALIDATION_DIGEST_MISMATCH");
  }
  if (isObject(acceptance)) {
    validateFrozenDigest("acceptance_oracles", keyedRecords(acceptance.oracles, "oracle_id"), "FROZEN_ACCEPTANCE_ORACLES_DIGEST_MISMATCH");
  }
  if (isObject(retirement)) {
    const entries = Array.isArray(retirement.entries) ? retirement.entries.filter(isObject) : [];
    const records = {
      preactivation_closure: retirement.preactivation_closure,
      entries: Object.fromEntries(entries.map((entry) => [entry.category, {
        members: entry.members,
        disposition: entry.disposition,
        assertions: entry.assertions,
        status: entry.status,
        executed: entry.executed,
      }])),
    };
    const edges = {
      slice4_owner: retirement.slice4_owner,
      entries: Object.fromEntries(entries.map((entry) => [entry.category, {
        production_seams: entry.production_seams,
        slice4_owner_tasks: entry.slice4_owner_tasks,
        retirement_test: entry.retirement_test,
        command: entry.command,
      }])),
    };
    validateFrozenDigest("retirement_records", records, "FROZEN_RETIREMENT_RECORDS_DIGEST_MISMATCH");
    validateFrozenDigest("retirement_edges", edges, "FROZEN_RETIREMENT_EDGES_DIGEST_MISMATCH");
  }
}

function memberDigest(members) {
  return crypto.createHash("sha256").update(JSON.stringify(members)).digest("hex");
}

function validateRetirement(retirement, taskCatalog) {
  if (!retirement) return;
  const taskIds = taskCatalog.ids;
  const ordinarySourceMap = cli.requireMaterialized ? null : currentRustSourceMap();
  const ordinaryManifest = cli.requireMaterialized ? null : parsePublicApiManifest();
  const ordinaryLifecycle = cli.requireMaterialized
    ? null
    : ordinaryRetirementLifecycle(ordinarySourceMap, ordinaryManifest);
  exactKeys(retirement, ["kind", "schema_version", "status", "slice4_owner", "preactivation_closure", "entries"], "retirement");
  if (retirement.kind !== "symforge.v10_authority_retirement.v11") fail("KIND_INVALID", "retirement.kind");
  if (retirement.schema_version !== 1) fail("SCHEMA_VERSION_INVALID", "retirement.schema_version");
  validatePlanned(retirement, "retirement", false);
  exactKeys(retirement.slice4_owner, ["slice", "tasks"], "retirement.slice4_owner");
  const ownerTasks = isObject(retirement.slice4_owner) ? validateTaskIds(retirement.slice4_owner.tasks, "retirement.slice4_owner.tasks", taskIds) : [];
  if (!isObject(retirement.slice4_owner) || retirement.slice4_owner.slice !== 4) fail("SLICE4_OWNER_INVALID", "retirement.slice4_owner.slice");
  exactArray(ownerTasks, ["T064", "T065", "T066", "T067"], "SLICE4_OWNER_INVALID", "retirement.slice4_owner.tasks");

  const entries = Array.isArray(retirement.entries) ? retirement.entries : [];
  if (!Array.isArray(retirement.entries)) fail("ARRAY_INVALID", "retirement.entries");
  const counts = new Map();
  const byCategory = new Map();
  const keys = ["category", "members", "production_seams", "slice4_owner_tasks", "disposition", "retirement_test", "command", "assertions", "status", "executed"];
  for (const [index, entry] of entries.entries()) {
    const context = `retirement.entries[${index}]`;
    exactKeys(entry, keys, context);
    if (!isObject(entry)) continue;
    counts.set(entry.category, (counts.get(entry.category) || 0) + 1);
    byCategory.set(entry.category, entry);
    if (!RETIREMENT_CATEGORIES.includes(entry.category)) fail("RETIREMENT_CATEGORY_INVALID", `${context}: ${String(entry.category)}`);
    const members = stringArray(entry.members, `${context}.members`);
    exactArray(members, [...members].sort(), "RETIREMENT_MEMBERS_NOT_SORTED", String(entry.category));
    if (ordinaryLifecycle === "preactivation") {
      for (const member of members.filter((item) => item.startsWith("src/"))) {
        if (!sourceAnchorResolves(member)) fail("PREACTIVATION_SOURCE_ANCHOR_UNRESOLVED", `${entry.category}: ${member}`);
      }
    }
    for (const seam of stringArray(entry.production_seams, `${context}.production_seams`)) {
      if (/^src\/lifecycle\//u.test(seam) || /^src\/(?:watcher|sidecar|snapshot)\.rs(?:::|$)/u.test(seam)) fail("SEAM_NAMESPACE_INVALID", `${context}: ${seam}`);
      if (!EXPECTED_PRODUCTION_SEAMS.has(seam) || !pathIsExistingOrDeclared(taskCatalog, seamPath(seam))) fail("SEAM_UNRESOLVED", `${context}: ${seam}`);
    }
    const tasks = validateTaskIds(entry.slice4_owner_tasks, `${context}.slice4_owner_tasks`, taskIds);
    for (const task of tasks) if (!ownerTasks.includes(task)) fail("SLICE4_OWNER_TASK_INVALID", `${context}: ${task}`);
    if (EXPECTED_RETIREMENT_OWNERS.has(entry.category)) exactArray(tasks, EXPECTED_RETIREMENT_OWNERS.get(entry.category), "SLICE4_OWNER_TASK_INVALID", String(entry.category));
    nonEmptyString(entry.disposition, `${context}.disposition`);
    validateTestAndCommand(entry.retirement_test, entry.command, context);
    if (![...taskCatalog.ids].some((task) => taskDeclaresPath(taskCatalog, task, targetFile(entry.retirement_test)))) {
      fail("TEST_TARGET_UNRESOLVED", `${context}: ${targetFile(entry.retirement_test)} is not task-declared`);
    }
    stringArray(entry.assertions, `${context}.assertions`);
    validatePlanned(entry, context, true);
    if (RETIREMENT_MEMBER_DIGESTS.has(entry.category) && memberDigest(members) !== RETIREMENT_MEMBER_DIGESTS.get(entry.category)) {
      fail("RETIREMENT_MEMBER_UNRESOLVED", String(entry.category));
    }
  }
  for (const category of RETIREMENT_CATEGORIES) {
    const count = counts.get(category) || 0;
    if (count !== 1) fail("RETIREMENT_CATEGORY_COUNT", `${category}: ${count}`);
  }
  for (const category of counts.keys()) {
    if (!RETIREMENT_CATEGORIES.includes(category)) fail("RETIREMENT_CATEGORY_EXTRA", String(category));
  }
  if (byCategory.has("tools")) exactArray(byCategory.get("tools").members, EXPECTED_RETIREMENT_TOOLS, "RETIREMENT_TOOLS_MISMATCH", "tools");
  if (byCategory.has("resources")) exactArray(byCategory.get("resources").members, EXPECTED_RESOURCES, "RETIREMENT_RESOURCES_MISMATCH", "resources");
  if (byCategory.has("prompts")) exactArray(byCategory.get("prompts").members, EXPECTED_PROMPTS, "RETIREMENT_PROMPTS_MISMATCH", "prompts");
  if (byCategory.has("compatibility_aliases")) exactArray(byCategory.get("compatibility_aliases").members, EXPECTED_COMPATIBILITY_ALIASES, "RETIREMENT_ALIASES_MISMATCH", "compatibility_aliases");
  if (byCategory.has("raw_embed")) exactArray(byCategory.get("raw_embed").members, expectedRawEmbedAtoms(), "RETIREMENT_RAW_EMBED_MISMATCH", "raw_embed");
  if (ordinaryLifecycle === "preactivation") {
    validateRetirementSourceInventory(retirement, ordinarySourceMap);
    validateRetirementClosure(retirement, ordinarySourceMap);
  }
  if (ordinaryLifecycle === "postactivation") validatePostactivationRetirement(ordinarySourceMap, ordinaryManifest);
  const aliases = byCategory.get("compatibility_aliases");
  const expectedAliasAssertions = [
    "Neither alias is advertised as an additional tool",
    "detect_changes delegates to detect_impact and returns GitObserved for committed-ref diffs or WorktreeScopeObserved for worktree diffs",
    "detect_changes never acquires a ProjectQueryLease or upgrades observation evidence to GenerationLeased",
    "trace_symbol cannot reach V10 symbol caches and uses GenerationLeased only for a complete Current publication",
  ];
  if (!isObject(aliases) || aliases.disposition !== "route trace_symbol through V11 generation authority and route detect_changes to detect_impact as typed Git/worktree observation, or retire either alias" || JSON.stringify(aliases.assertions) !== JSON.stringify(expectedAliasAssertions)) {
    fail("DETECT_CHANGES_ALIAS_INVALID", "detect_changes must remain a detect_impact Git/worktree observation alias without query-lease authority");
  }
}

function validateReceiptSequence(value, idKey, expectedIds, expectedFields, code, context) {
  if (!Array.isArray(value)) {
    fail(code, context + ": expected array");
    return [];
  }
  const ids = [];
  for (const [index, receipt] of value.entries()) {
    const itemContext = context + "[" + index + "]";
    if (!exactKeys(receipt, expectedFields, itemContext)) {
      fail(code, itemContext + ": invalid object");
      continue;
    }
    ids.push(receipt[idKey]);
  }
  if (JSON.stringify(ids) !== JSON.stringify(expectedIds)) {
    fail(code, context + ": exact ordered id set mismatch");
  }
  return value.filter(isObject);
}

function artifactMatchesHash(relativePath, expectedHash, code, context) {
  if (!safeArtifactPath(relativePath) || !validSha256(expectedHash)) {
    fail(code, context + ": invalid artifact path or digest");
    return null;
  }
  const bytes = readBytes(relativePath);
  if (bytes === null || sha256Bytes(bytes) !== expectedHash) {
    fail(code, context + ": artifact missing or digest mismatch");
    return null;
  }
  return bytes;
}

function worktreeMatchesBlob(relativePath, blob) {
  const current = readBytes(relativePath);
  return Buffer.isBuffer(blob) && Buffer.isBuffer(current) && current.equals(blob);
}

function releaseWorktreeIsClean() {
  const status = runGit(["status", "--porcelain=v1", "--untracked-files=all"]);
  if (!status.ok) return false;
  return status.stdout.toString("utf8").split(/\r?\n/u).filter((line) => line !== "").every(
    (line) => line.startsWith("?? target/ci/lifecycle-v11/"),
  );
}

function validatePinnedReleaseFile(evidence, field, relativePath, releaseCommit) {
  const blob = gitBlob(releaseCommit, relativePath);
  if (blob === null || !worktreeMatchesBlob(relativePath, blob) || evidence[field] !== sha256Bytes(blob)) {
    fail("RELEASE_PINNED_FILE_INVALID", field + ": " + relativePath);
  }
}

function requirementOracleMap(acceptance) {
  const result = new Map(EXPECTED_REQUIREMENTS.map((id) => [id, []]));
  for (const oracle of Array.isArray(acceptance && acceptance.oracles) ? acceptance.oracles : []) {
    if (!isObject(oracle) || !safeArtifactPath(oracle.ci_artifact)) continue;
    for (const id of Array.isArray(oracle.requirement_ids) ? oracle.requirement_ids : []) {
      if (result.has(id)) result.get(id).push({ oracle_id: oracle.oracle_id, artifact: oracle.ci_artifact });
    }
  }
  for (const oracles of result.values()) oracles.sort((left, right) => left.oracle_id.localeCompare(right.oracle_id));
  return result;
}

function releaseSeamReceiptMap(trace) {
  const reverse = new Map();
  const seams = isObject(trace && trace.catalogs && trace.catalogs.seams) ? trace.catalogs.seams : {};
  for (const [seamId, anchors] of Object.entries(seams)) {
    for (const anchor of Array.isArray(anchors) ? anchors : []) {
      if (!reverse.has(anchor)) reverse.set(anchor, []);
      reverse.get(anchor).push(seamId);
    }
  }
  for (const seamIds of reverse.values()) seamIds.sort();
  return new Map([...reverse.entries()].sort(([left], [right]) => left < right ? -1 : left > right ? 1 : 0));
}

function validateApprovedRefreeze(evidence, retirement, releaseCommit) {
  const commit = evidence.approved_refreeze_commit;
  const tree = evidence.approved_refreeze_tree;
  const verification = evidence.approval_verification;
  if (typeof commit !== "string" || !/^[0-9a-f]{40,64}$/u.test(commit) ||
      typeof tree !== "string" || !/^[0-9a-f]{40,64}$/u.test(tree) ||
      commit === releaseCommit ||
      !exactKeys(verification, EXPECTED_RELEASE_VALIDATION.evidence_contract.approval_verification_fields, "release_evidence.approval_verification") ||
      verification.status !== "passed" ||
      verification.result_artifact !== "target/ci/lifecycle-v11/refreeze-approval-result.json" ||
      !validSha256(verification.result_sha256)) {
    fail("APPROVED_REFREEZE_INVALID", "identity or verification status");
    return;
  }
  const resolvedCommit = gitText(["rev-parse", "--verify", commit + "^{commit}"]);
  const resolvedTree = gitText(["rev-parse", "--verify", commit + "^{tree}"]);
  const ancestry = runGit(["merge-base", "--is-ancestor", commit, releaseCommit]);
  if (resolvedCommit !== commit || resolvedTree !== tree || !ancestry.ok) {
    fail("APPROVED_REFREEZE_INVALID", "commit/tree/ancestry proof");
    return;
  }
  const verifierBlob = gitBlob(releaseCommit, "execution/refreeze_v11.py");
  const resultBytes = artifactMatchesHash(
    verification.result_artifact,
    verification.result_sha256,
    "APPROVED_REFREEZE_INVALID",
    "approval verification result",
  );
  let result;
  try {
    result = Buffer.isBuffer(resultBytes) ? JSON.parse(resultBytes.toString("utf8")) : null;
  } catch {
    result = null;
  }
  const resultFields = EXPECTED_RELEASE_VALIDATION.evidence_contract.approval_result_fields;
  const history = isObject(result) && Array.isArray(result.approval_history_inventory)
    ? result.approval_history_inventory
    : [];
  const historyEntriesValid = history.every((entry, index) =>
    exactKeys(entry, EXPECTED_RELEASE_VALIDATION.evidence_contract.approval_history_entry_fields, `refreeze_approval_result.approval_history_inventory[${index}]`) &&
      entry.sequence === index + 1 && validSha256(entry.record_sha256) && validSha256(entry.signature_sha256));
  const historyInventorySha256 = canonicalDigest("symforge.refreeze.v11.approval-history-inventory", history);
  const historyRootSha256 = isObject(result) ? canonicalDigest("symforge.refreeze.v11.approval-history-root", {
    approval_sequence: result.approval_sequence,
    approval_predecessor_digest: result.approval_predecessor_digest,
    approval_history_count: result.approval_history_count,
    approval_history_inventory_sha256: result.approval_history_inventory_sha256,
    current_record_sha256: result.record_sha256,
    current_signature_sha256: result.signature_sha256,
  }) : null;
  const workflowBlob = gitBlob(releaseCommit, RELEASE_WORKFLOW_PATH);
  const runnerWorkflowBlob = isObject(result) && typeof result.workflow_commit === "string"
    ? gitBlob(result.workflow_commit, RELEASE_WORKFLOW_PATH)
    : null;
  if (!Buffer.isBuffer(verifierBlob) ||
      !worktreeMatchesBlob("execution/refreeze_v11.py", verifierBlob) ||
      !Buffer.isBuffer(workflowBlob) || !worktreeMatchesBlob(RELEASE_WORKFLOW_PATH, workflowBlob) ||
      !Buffer.isBuffer(runnerWorkflowBlob) || !runnerWorkflowBlob.equals(workflowBlob) ||
      !exactKeys(result, resultFields, "refreeze_approval_result") ||
      result.kind !== "symforge.refreeze_approval_verification_result.v11" ||
      result.schema_version !== 1 ||
      result.approved_commit !== commit ||
      result.approved_tree !== tree ||
      result.release_commit !== releaseCommit ||
      result.release_tree !== evidence.release_tree ||
      result.verifier_sha256 !== sha256Bytes(verifierBlob) ||
      !validSha256(result.record_sha256) ||
      !validSha256(result.signature_sha256) ||
      !validSha256(result.allowed_signers_sha256) ||
      !validSha256(result.release_identity_sha256) ||
      !Number.isInteger(result.approval_sequence) || result.approval_sequence < 1 || result.approval_sequence > 256 ||
      result.approval_history_count !== result.approval_sequence - 1 || history.length !== result.approval_history_count ||
      !historyEntriesValid || result.approval_history_inventory_sha256 !== historyInventorySha256 ||
      result.approval_history_root_sha256 !== historyRootSha256 ||
      (result.approval_sequence === 1
        ? result.approval_predecessor_digest !== null
        : result.approval_predecessor_digest !== history.at(-1).record_sha256) ||
      !validSha256(result.command_argv_sha256) ||
      result.expected_repository !== "special-place-ai-heaven/symforge" ||
      result.external_inputs !== "outside_repository" ||
      result.command_id !== "refreeze-v11-verify-approval" ||
      result.exit_code !== 0 ||
      !validSha256(result.stdout_sha256) ||
      !validSha256(result.stderr_sha256) ||
      result.runner_kind !== "github_actions_protected_environment" ||
      result.runner_repository !== "special-place-ai-heaven/symforge" ||
      result.workflow_path !== RELEASE_WORKFLOW_PATH ||
      result.workflow_sha256 !== sha256Bytes(workflowBlob) ||
      result.workflow_job !== APPROVAL_GATE_JOB_ID ||
      typeof result.workflow_commit !== "string" || !/^[0-9a-f]{40,64}$/u.test(result.workflow_commit) ||
      typeof result.workflow_run_id !== "string" || !/^[1-9][0-9]{0,19}$/u.test(result.workflow_run_id) ||
      !Number.isInteger(result.workflow_run_attempt) || result.workflow_run_attempt < 1 || result.workflow_run_attempt > 1000 ||
      !["push", "workflow_dispatch"].includes(result.workflow_event) ||
      result.status !== "passed") {
    fail("APPROVED_REFREEZE_INVALID", "closed external-verification result mismatch");
    return;
  }
  const anchors = [...new Set((Array.isArray(retirement && retirement.entries) ? retirement.entries : [])
    .filter(isObject)
    .flatMap((entry) => Array.isArray(entry.members) ? entry.members : [])
    .filter((member) => typeof member === "string" && member.startsWith("src/")))].sort();
  const blobByFile = new Map();
  for (const anchor of anchors) {
    const file = seamPath(anchor);
    if (!blobByFile.has(file)) blobByFile.set(file, gitBlob(commit, file));
    const blob = blobByFile.get(file);
    if (!Buffer.isBuffer(blob) || !sourceAnchorResolvesText(anchor, blob.toString("utf8"))) {
      fail("PREACTIVATION_SOURCE_ANCHOR_UNRESOLVED", anchor);
    }
  }
  const approvedSourceMap = gitRustSourceMap(commit);
  validateRetirementSourceInventory(retirement, approvedSourceMap);
  validateRetirementClosure(retirement, approvedSourceMap);
}

const ORACLE_RESULT_FIELDS = [
  "kind",
  "schema_version",
  "release_commit",
  "release_tree",
  "oracle_id",
  "category",
  "trace_test_id",
  "test",
  "command",
  "requirement_ids",
  "implementation_tasks",
  "target_slice",
  "production_seams_sha256",
  "preconditions_sha256",
  "actions_sha256",
  "assertions_sha256",
  "positive_control_sha256",
  "negative_controls_sha256",
  "bounds_sha256",
  "fairness_sha256",
  "ci_artifact",
  "positive_control_result",
  "negative_controls_result",
  "assertions_result",
  "test_result",
  "result",
  "status",
];

function semanticPayloadHash(value) {
  return sha256Bytes(Buffer.from(canonicalJson(value), "utf8"));
}

function oracleArtifactBaseIsExact(artifact, evidence, oracle) {
  return isObject(artifact) &&
    artifact.kind === "symforge.lifecycle_oracle_execution.v11" &&
    artifact.schema_version === 1 &&
    artifact.release_commit === evidence.release_commit &&
    artifact.release_tree === evidence.release_tree &&
    artifact.oracle_id === oracle.oracle_id &&
    artifact.category === oracle.category &&
    artifact.trace_test_id === oracle.trace_test_id &&
    artifact.test === oracle.test &&
    artifact.command === oracle.command &&
    JSON.stringify(artifact.requirement_ids) === JSON.stringify(oracle.requirement_ids) &&
    JSON.stringify(artifact.implementation_tasks) === JSON.stringify(oracle.implementation_tasks) &&
    artifact.target_slice === oracle.target_slice &&
    artifact.production_seams_sha256 === semanticPayloadHash(oracle.production_seams) &&
    artifact.preconditions_sha256 === semanticPayloadHash(oracle.preconditions) &&
    artifact.actions_sha256 === semanticPayloadHash(oracle.actions) &&
    artifact.assertions_sha256 === semanticPayloadHash(oracle.assertions) &&
    artifact.positive_control_sha256 === semanticPayloadHash(oracle.positive_control) &&
    artifact.negative_controls_sha256 === semanticPayloadHash(oracle.negative_controls) &&
    artifact.bounds_sha256 === semanticPayloadHash(oracle.bounds) &&
    artifact.fairness_sha256 === semanticPayloadHash(oracle.fairness) &&
    artifact.ci_artifact === oracle.ci_artifact &&
    artifact.positive_control_result === "passed" &&
    artifact.negative_controls_result === "passed" &&
    artifact.assertions_result === "passed" &&
    artifact.test_result === "passed" &&
    artifact.result === "passed" &&
    artifact.status === "passed";
}

function genericOracleArtifactIsExact(bytes, evidence, oracle) {
  let artifact;
  try {
    artifact = JSON.parse(bytes.toString("utf8"));
  } catch {
    return false;
  }
  return exactKeys(artifact, ORACLE_RESULT_FIELDS, "oracle_result." + oracle.oracle_id) &&
    oracleArtifactBaseIsExact(artifact, evidence, oracle);
}

function validateRequirementReceipts(evidence, acceptance) {
  const contract = EXPECTED_RELEASE_VALIDATION.evidence_contract;
  const receipts = validateReceiptSequence(
    evidence.requirement_receipts,
    "requirement_id",
    EXPECTED_REQUIREMENTS,
    contract.requirement_receipt_fields,
    "RELEASE_REQUIREMENT_RECEIPT_INVALID",
    "release_evidence.requirement_receipts",
  );
  const expectedByRequirement = requirementOracleMap(acceptance);
  for (const receipt of receipts) {
    const context = "release_evidence.requirement_receipts." + String(receipt.requirement_id);
    const expected = (expectedByRequirement.get(receipt.requirement_id) || []).map((oracle) => oracle.oracle_id);
    if (receipt.status !== "passed" || receipt.release_tree !== evidence.release_tree || expected.length === 0 ||
        JSON.stringify(receipt.oracle_ids) !== JSON.stringify(expected)) {
      fail("RELEASE_REQUIREMENT_RECEIPT_INVALID", context + ": status, tree, or exact oracle-id set");
    }
  }
}

function validateOracleReceipts(evidence, acceptance) {
  const contract = EXPECTED_RELEASE_VALIDATION.evidence_contract;
  const oracles = new Map((Array.isArray(acceptance && acceptance.oracles) ? acceptance.oracles : [])
    .filter(isObject).map((oracle) => [oracle.oracle_id, oracle]));
  const expectedIds = [...oracles.keys()].sort();
  const receipts = validateReceiptSequence(
    evidence.oracle_receipts,
    "oracle_id",
    expectedIds,
    contract.oracle_receipt_fields,
    "RELEASE_ORACLE_RECEIPT_INVALID",
    "release_evidence.oracle_receipts",
  );
  const artifactPaths = new Set();
  for (const receipt of receipts) {
    const oracle = oracles.get(receipt.oracle_id);
    if (!isObject(oracle) || receipt.artifact !== oracle.ci_artifact || receipt.status !== "passed" ||
        receipt.release_tree !== evidence.release_tree || !validSha256(receipt.artifact_sha256) ||
        artifactPaths.has(receipt.artifact)) {
      fail("RELEASE_ORACLE_RECEIPT_INVALID", String(receipt.oracle_id));
      continue;
    }
    artifactPaths.add(receipt.artifact);
    const bytes = artifactMatchesHash(
      receipt.artifact,
      receipt.artifact_sha256,
      "RELEASE_ORACLE_RECEIPT_INVALID",
      String(receipt.oracle_id),
    );
    if (Buffer.isBuffer(bytes) && oracle.oracle_id !== "ORACLE-PERFORMANCE-OBSERVED-REFRESH" &&
        !genericOracleArtifactIsExact(bytes, evidence, oracle)) {
      fail("RELEASE_ORACLE_RECEIPT_INVALID", String(receipt.oracle_id) + ": closed oracle result mismatch");
    }
  }
}

function safeTaskArtifactPath(value) {
  return typeof value === "string" &&
    /^(?:target\/ci\/lifecycle-v11|docs\/reviews|src|tests|formal|specs|execution|scripts)\/[A-Za-z0-9_./-]+$/u.test(value) &&
    !value.includes("..") &&
    !value.includes("\\") &&
    path.posix.normalize(value) === value;
}

function taskDeclarationDigest(taskId, releaseCommit) {
  const tasksBlob = gitBlob(releaseCommit, TASKS_PATH);
  if (!Buffer.isBuffer(tasksBlob)) return null;
  const pattern = new RegExp("^- \\[[ xX]\\] " + escapeRegExp(taskId) + "\\b[^\\r\\n]*$", "gmu");
  const matches = [...tasksBlob.toString("utf8").matchAll(pattern)];
  const digest = matches.length === 1 ? sha256Bytes(Buffer.from(matches[0][0], "utf8")) : null;
  return validSha256(digest) ? digest : null;
}

function taskArtifactHash(relativePath, releaseCommit) {
  if (!safeTaskArtifactPath(relativePath)) return null;
  if (relativePath.startsWith("target/ci/lifecycle-v11/")) {
    const bytes = readBytes(relativePath);
    return Buffer.isBuffer(bytes) ? sha256Bytes(bytes) : null;
  }
  const blob = gitBlob(releaseCommit, relativePath);
  return Buffer.isBuffer(blob) && worktreeMatchesBlob(relativePath, blob) ? sha256Bytes(blob) : null;
}

function taskResultArtifactIsExact(bytes, evidence, taskId, taskCatalog, releaseCommit, receiptPath, reviewHash) {
  let result;
  try {
    result = JSON.parse(bytes.toString("utf8"));
  } catch {
    return false;
  }
  const fields = ["kind", "schema_version", "release_commit", "release_tree", "task_id", "task_declaration_sha256", "command_results", "artifact_results", "status"];
  const declarationDigest = taskDeclarationDigest(taskId, releaseCommit);
  if (!exactKeys(result, fields, "release_task_result." + taskId) ||
      result.kind !== "symforge.lifecycle_release_task_result.v11" ||
      result.schema_version !== 1 ||
      result.release_commit !== evidence.release_commit ||
      result.release_tree !== evidence.release_tree ||
      result.task_id !== taskId ||
      !validSha256(result.task_declaration_sha256) || !validSha256(declarationDigest) ||
      result.task_declaration_sha256 !== declarationDigest ||
      result.status !== "passed" ||
      !Array.isArray(result.command_results) || result.command_results.length !== 1 ||
      !Array.isArray(result.artifact_results) || result.artifact_results.length === 0) {
    return false;
  }
  const commandIds = new Set();
  for (const [index, command] of result.command_results.entries()) {
    if (!exactKeys(command, ["command_id", "exit_code", "stdout_sha256", "stderr_sha256"], "release_task_result." + taskId + ".command_results[" + index + "]") ||
        typeof command.command_id !== "string" || !/^[a-z0-9][a-z0-9_-]*$/u.test(command.command_id) ||
        commandIds.has(command.command_id) || command.exit_code !== 0 ||
        !validSha256(command.stdout_sha256) || !validSha256(command.stderr_sha256)) {
      return false;
    }
    commandIds.add(command.command_id);
  }
  if (result.command_results[0].command_id !== EXPECTED_RELEASE_TASK_COMMAND_IDS.get(taskId)) return false;
  let reviewSeen = false;
  let declaredArtifactSeen = false;
  let approvalResultSeen = false;
  const artifactPaths = new Set();
  for (const [index, artifact] of result.artifact_results.entries()) {
    if (!exactKeys(artifact, ["path", "sha256", "status"], "release_task_result." + taskId + ".artifact_results[" + index + "]") ||
        artifact.status !== "passed" || !safeTaskArtifactPath(artifact.path) ||
        artifact.path === receiptPath || artifact.path === EXPECTED_RELEASE_VALIDATION.evidence_path || artifact.path === cli.evidence || artifactPaths.has(artifact.path) ||
        artifact.sha256 !== taskArtifactHash(artifact.path, releaseCommit)) {
      return false;
    }
    artifactPaths.add(artifact.path);
    if (artifact.path === RELEASE_GATE_REVIEW_PATH && artifact.sha256 === reviewHash) reviewSeen = true;
    if (artifact.path === "target/ci/lifecycle-v11/refreeze-approval-result.json") approvalResultSeen = true;
    if (taskDeclaresPath(taskCatalog, taskId, artifact.path) ||
        (taskId === "T089" && artifact.path === "target/ci/lifecycle-v11/refreeze-approval-result.json")) declaredArtifactSeen = true;
  }
  if (!reviewSeen || !declaredArtifactSeen || (taskId === "T089" && !approvalResultSeen)) return false;
  if (taskId === "T089") {
    if (JSON.stringify([...artifactPaths].sort()) !== JSON.stringify([
      RELEASE_GATE_REVIEW_PATH,
      "target/ci/lifecycle-v11/refreeze-approval-result.json",
    ].sort())) return false;
    let approvalResult;
    try {
      const approvalBytes = readBytes(evidence.approval_verification.result_artifact);
      approvalResult = Buffer.isBuffer(approvalBytes) ? JSON.parse(approvalBytes.toString("utf8")) : null;
    } catch {
      approvalResult = null;
    }
    if (!isObject(approvalResult) || result.command_results[0].stdout_sha256 !== approvalResult.stdout_sha256 ||
        result.command_results[0].stderr_sha256 !== approvalResult.stderr_sha256) return false;
  }
  return true;
}

function validateTaskReceipts(evidence, releaseCommit, taskCatalog) {
  const contract = EXPECTED_RELEASE_VALIDATION.evidence_contract;
  const expectedIds = EXPECTED_RELEASE_VALIDATION.required_task_receipts;
  const receipts = validateReceiptSequence(
    evidence.task_receipts,
    "task_id",
    expectedIds,
    contract.task_receipt_fields,
    "RELEASE_TASK_RECEIPT_INVALID",
    "release_evidence.task_receipts",
  );
  const reviewBlob = gitBlob(releaseCommit, RELEASE_GATE_REVIEW_PATH);
  const reviewHash = Buffer.isBuffer(reviewBlob) ? sha256Bytes(reviewBlob) : null;
  if (!Buffer.isBuffer(reviewBlob) || !worktreeMatchesBlob(RELEASE_GATE_REVIEW_PATH, reviewBlob)) {
    fail("RELEASE_TASK_RECEIPT_INVALID", "release-gate review is not pinned to the release tree");
  } else {
    const reviewText = reviewBlob.toString("utf8");
    for (const taskId of expectedIds.filter((id) => id !== "T089")) {
      const marker = "<!-- SYMFORGE LIFECYCLE RELEASE TASK " + taskId + ": " + EXPECTED_RELEASE_TASK_COMMAND_IDS.get(taskId) + " PASSED -->";
      if (countOccurrences(reviewText, marker) !== 1) {
        fail("RELEASE_TASK_RECEIPT_INVALID", taskId + ": exact release-review marker missing");
      }
    }
  }
  for (const receipt of receipts) {
    const expectedArtifact = "target/ci/lifecycle-v11/task-" + receipt.task_id + ".json";
    const artifactBytes = receipt.artifact === expectedArtifact
      ? artifactMatchesHash(receipt.artifact, receipt.artifact_sha256, "RELEASE_TASK_RECEIPT_INVALID", String(receipt.task_id))
      : null;
    if (receipt.status !== "passed" || receipt.release_tree !== evidence.release_tree ||
        receipt.artifact !== expectedArtifact || !Buffer.isBuffer(artifactBytes) ||
        !taskResultArtifactIsExact(artifactBytes, evidence, receipt.task_id, taskCatalog, releaseCommit, receipt.artifact, reviewHash)) {
      fail("RELEASE_TASK_RECEIPT_INVALID", String(receipt.task_id));
    }
  }
}

function validateRustCaseReceipts(evidence, trace, releaseCommit) {
  const contract = EXPECTED_RELEASE_VALIDATION.evidence_contract;
  const commands = trace.catalogs.commands;
  const tests = trace.catalogs.tests;
  const expected = Object.entries(tests)
    .filter(([, test]) => isObject(test) && (test.kind === "planned_exact" || test.kind === "inherited_exact"))
    .map(([id]) => id)
    .sort();
  const receipts = validateReceiptSequence(
    evidence.rust_case_receipts,
    "test_id",
    expected,
    contract.rust_case_receipt_fields,
    "MATERIALIZED_CASE_INVALID",
    "release_evidence.rust_case_receipts",
  );
  for (const receipt of receipts) {
    const test = tests[receipt.test_id];
    const parsed = isObject(test) ? parseExecutableTarget(test.target, "release_evidence.rust_case_receipts." + receipt.test_id) : null;
    const blob = parsed ? gitBlob(releaseCommit, parsed.file) : null;
    if (!isObject(test) || !parsed || !parsed.rustArea || test.kind === "planned_benchmark" ||
        receipt.target !== test.target || receipt.command !== commands[test.command_id] ||
        receipt.status !== "passed" || receipt.release_tree !== evidence.release_tree ||
        !Buffer.isBuffer(blob) || receipt.source_sha256 !== sha256Bytes(blob) ||
        !worktreeMatchesBlob(parsed.file, blob) ||
        !rustNamedCaseIsNonEmptyInSource(parsed.symbolPath, blob.toString("utf8"))) {
      fail("MATERIALIZED_CASE_INVALID", String(receipt.test_id));
    }
  }
}

function benchmarkArtifactIsExact(bytes, evidence, receipt, oracle) {
  let artifact;
  try {
    artifact = JSON.parse(bytes.toString("utf8"));
  } catch {
    return false;
  }
  const fields = [
    ...ORACLE_RESULT_FIELDS,
    "producer_task",
    "registration",
    "source_sha256",
    "baseline_commit",
    "semantic_equivalence",
    "completed_write_boundaries",
    "first_strict_lease_byte_identity",
    "pregranted_capacity_vector",
    "retained_capacity_vector",
    "candidate_capacity_vector",
    "declared_scratch_vector",
    "declared_headroom_vector",
    "p95_seconds",
    "max_seconds",
    "baseline_ratio",
    "single_path_full_candidate_violations",
    "legal_full_candidate_triggers",
    "corpus_sha256",
    "environment_sha256",
    "completion_receipts_sha256",
  ];
  const vectorNames = [
    "pregranted_capacity_vector",
    "retained_capacity_vector",
    "candidate_capacity_vector",
    "declared_scratch_vector",
    "declared_headroom_vector",
  ];
  const vectors = Object.fromEntries(vectorNames.map((name) => [name, isObject(artifact && artifact[name]) ? artifact[name] : {}]));
  const vectorsAreExact = vectorNames.every((name) =>
    JSON.stringify(Object.keys(vectors[name])) === JSON.stringify(CAPACITY_DIMENSIONS) &&
    CAPACITY_DIMENSIONS.every((dimension) => Number.isInteger(vectors[name][dimension]) && vectors[name][dimension] >= 0),
  );
  const capacityIsConserved = vectorsAreExact && CAPACITY_DIMENSIONS.every((dimension) =>
    vectors.retained_capacity_vector[dimension] + vectors.candidate_capacity_vector[dimension] <=
      vectors.pregranted_capacity_vector[dimension] + vectors.declared_scratch_vector[dimension] + vectors.declared_headroom_vector[dimension],
  );
  const nonNegativeNumber = (value) => typeof value === "number" && Number.isFinite(value) && value >= 0;
  return exactKeys(artifact, fields, "benchmark_result." + String(receipt.test_id)) &&
    oracleArtifactBaseIsExact(artifact, evidence, oracle) &&
    artifact.producer_task === "T068" &&
    artifact.registration === receipt.registration &&
    artifact.source_sha256 === receipt.source_sha256 &&
    artifact.baseline_commit === "1521abb0" &&
    artifact.semantic_equivalence === "passed" &&
    JSON.stringify(artifact.completed_write_boundaries) === JSON.stringify(["external_write_burst", "symforge_mutation_commit"]) &&
    artifact.first_strict_lease_byte_identity === "first_strict_lease_exact_byte_identity" &&
    vectorsAreExact &&
    CAPACITY_DIMENSIONS.some((dimension) => vectors.pregranted_capacity_vector[dimension] > 0) &&
    CAPACITY_DIMENSIONS.some((dimension) => vectors.retained_capacity_vector[dimension] > 0) &&
    CAPACITY_DIMENSIONS.some((dimension) => vectors.candidate_capacity_vector[dimension] > 0) &&
    capacityIsConserved &&
    nonNegativeNumber(artifact.p95_seconds) && artifact.p95_seconds > 0 && artifact.p95_seconds <= 2 &&
    nonNegativeNumber(artifact.max_seconds) && artifact.max_seconds >= artifact.p95_seconds && artifact.max_seconds <= 5 &&
    nonNegativeNumber(artifact.baseline_ratio) && artifact.baseline_ratio > 0 && artifact.baseline_ratio <= 1.25 &&
    artifact.single_path_full_candidate_violations === 0 &&
    JSON.stringify(artifact.legal_full_candidate_triggers) === JSON.stringify(["observer_gap", "scope_dirty", "initial", "manual", "recovery"]) &&
    validSha256(artifact.corpus_sha256) &&
    validSha256(artifact.environment_sha256) &&
    validSha256(artifact.completion_receipts_sha256);
}

function validateBenchmarkReceipts(evidence, trace, acceptance, releaseCommit) {
  const contract = EXPECTED_RELEASE_VALIDATION.evidence_contract;
  const commands = trace.catalogs.commands;
  const tests = trace.catalogs.tests;
  const expected = Object.entries(tests)
    .filter(([, test]) => isObject(test) && test.kind === "planned_benchmark")
    .map(([id]) => id)
    .sort();
  const receipts = validateReceiptSequence(
    evidence.benchmark_receipts,
    "test_id",
    expected,
    contract.benchmark_receipt_fields,
    "MATERIALIZED_BENCHMARK_INVALID",
    "release_evidence.benchmark_receipts",
  );
  for (const receipt of receipts) {
    const test = tests[receipt.test_id];
    const oracle = Array.isArray(acceptance && acceptance.oracles)
      ? acceptance.oracles.find((item) => isObject(item) && item.trace_test_id === receipt.test_id)
      : null;
    const parsed = isObject(test) ? parseExecutableTarget(test.target, "release_evidence.benchmark_receipts." + receipt.test_id) : null;
    const blob = parsed ? gitBlob(releaseCommit, parsed.file) : null;
    const artifactBytes = receipt.receipt === (oracle && oracle.ci_artifact)
      ? artifactMatchesHash(receipt.receipt, receipt.receipt_sha256, "MATERIALIZED_BENCHMARK_INVALID", String(receipt.test_id))
      : null;
    const oracleReceipt = Array.isArray(evidence.oracle_receipts)
      ? evidence.oracle_receipts.find((item) => isObject(item) && item.oracle_id === (oracle && oracle.oracle_id))
      : null;
    if (!isObject(test) || !parsed || test.kind !== "planned_benchmark" ||
        receipt.target !== test.target || receipt.command !== commands[test.command_id] ||
        receipt.registration !== test.registration || receipt.status !== "passed" ||
        receipt.release_tree !== evidence.release_tree || !Buffer.isBuffer(blob) ||
        receipt.source_sha256 !== sha256Bytes(blob) || !worktreeMatchesBlob(parsed.file, blob) ||
        !benchmarkRegistrationExistsInSource(test.registration, blob.toString("utf8")) ||
        !isObject(oracleReceipt) || oracleReceipt.artifact !== receipt.receipt || oracleReceipt.artifact_sha256 !== receipt.receipt_sha256 ||
        !Buffer.isBuffer(artifactBytes) || !isObject(oracle) || !benchmarkArtifactIsExact(artifactBytes, evidence, receipt, oracle)) {
      fail("MATERIALIZED_BENCHMARK_INVALID", String(receipt.test_id));
    }
  }
}

function validateSourceReceipts(evidence, trace, releaseCommit) {
  const contract = EXPECTED_RELEASE_VALIDATION.evidence_contract;
  const expectedMap = releaseSeamReceiptMap(trace);
  const expectedAnchors = [...expectedMap.keys()];
  const receipts = validateReceiptSequence(
    evidence.source_receipts,
    "anchor",
    expectedAnchors,
    contract.source_receipt_fields,
    "MATERIALIZED_SOURCE_INVALID",
    "release_evidence.source_receipts",
  );
  for (const receipt of receipts) {
    const file = seamPath(receipt.anchor);
    const blob = gitBlob(releaseCommit, file);
    if (JSON.stringify(receipt.seam_ids) !== JSON.stringify(expectedMap.get(receipt.anchor)) ||
        receipt.status !== "passed" || receipt.release_tree !== evidence.release_tree ||
        !Buffer.isBuffer(blob) || receipt.source_sha256 !== sha256Bytes(blob) ||
        !worktreeMatchesBlob(file, blob) ||
        !sourceAnchorResolvesText(receipt.anchor, blob.toString("utf8"))) {
      fail("MATERIALIZED_SOURCE_INVALID", String(receipt.anchor));
    }
  }
}

function materializedCommandIdentity() {
  return {
    commit: gitText(["rev-parse", "--verify", "HEAD^{commit}"]),
    tree: gitText(["rev-parse", "--verify", "HEAD^{tree}"]),
    clean: releaseWorktreeIsClean(),
  };
}

function runVerifiedCommand(spec, runtime = {}) {
  const spawn = runtime.spawnSync || childProcess.spawnSync;
  const identity = runtime.identity || materializedCommandIdentity;
  if (!isObject(spec) || typeof spec.program !== "string" ||
      !(path.isAbsolute(spec.program) && !/[\u0000-\u001f;&|><`]/u.test(spec.program)) ||
      !Array.isArray(spec.args) || spec.args.some((argument) => typeof argument !== "string" || argument.includes("\0")) ||
      !Number.isInteger(spec.timeout_ms) || spec.timeout_ms < 1 || spec.timeout_ms > 3_600_000 ||
      !isObject(spec.env)) {
    return { ok: false, reason: "invalid_spec" };
  }
  const before = identity();
  if (!isObject(before) || !before.clean || typeof before.commit !== "string" || typeof before.tree !== "string") {
    return { ok: false, reason: "unclean_before" };
  }
  let result;
  try {
    result = spawn(spec.program, spec.args, {
      cwd: repositoryRoot,
      env: { ...process.env, ...spec.env },
      shell: false,
      windowsHide: true,
      encoding: null,
      stdio: ["ignore", "pipe", "pipe"],
      timeout: spec.timeout_ms,
      maxBuffer: 64 * 1024 * 1024,
    });
  } catch {
    return { ok: false, reason: "spawn_error" };
  }
  const after = identity();
  const stdout = Buffer.isBuffer(result && result.stdout) ? result.stdout : Buffer.from(result && result.stdout || "");
  const stderr = Buffer.isBuffer(result && result.stderr) ? result.stderr : Buffer.from(result && result.stderr || "");
  if (result && result.error) return { ok: false, reason: result.error.code === "ETIMEDOUT" ? "timeout" : "spawn_error" };
  if (!result || result.signal !== null && result.signal !== undefined) return { ok: false, reason: "signal" };
  if (result.status !== 0) return { ok: false, reason: "nonzero" };
  if (!isObject(after) || !after.clean || after.commit !== before.commit || after.tree !== before.tree) {
    return { ok: false, reason: "tree_changed" };
  }
  return {
    ok: true,
    reason: "passed",
    stdout_sha256: sha256Bytes(stdout),
    stderr_sha256: sha256Bytes(stderr),
  };
}

function materializedCommandSpec(testId, test, command, evidence, receiptPath, artifactPaths) {
  const parsed = parseExecutableTarget(test.target, `materialized_command.${testId}`);
  const cargoExecutable = materializedEnvironmentPath("SYMFORGE_LIFECYCLE_CARGO_EXECUTABLE", "file");
  if (!parsed || !parsed.rustArea || cargoExecutable === null) return null;
  let args;
  if (test.kind === "planned_benchmark" || test.kind === "executed_benchmark") {
    args = ["bench", "--bench", path.posix.basename(parsed.file, ".rs"), "--", parsed.caseName];
  } else if (parsed.area === "tests") {
    args = ["test", "--test", path.posix.basename(parsed.file, ".rs"), parsed.symbolPath, "--", "--exact"];
  } else if (parsed.area === "src") {
    const module = sourceModulePath(parsed.file);
    const exactCase = module === "" ? parsed.symbolPath : `${module}::${parsed.symbolPath}`;
    args = ["test", "--lib", exactCase, "--", "--exact"];
  } else {
    return null;
  }
  return {
    program: cargoExecutable,
    args,
    timeout_ms: test.kind.includes("benchmark") ? 3_600_000 : 1_800_000,
    env: {
      SYMFORGE_LIFECYCLE_COMMAND_RECEIPT: receiptPath,
      SYMFORGE_LIFECYCLE_RELEASE_COMMIT: evidence.release_commit,
      SYMFORGE_LIFECYCLE_RELEASE_TREE: evidence.release_tree,
      SYMFORGE_LIFECYCLE_TEST_ID: testId,
      SYMFORGE_LIFECYCLE_TARGET: test.target,
      SYMFORGE_LIFECYCLE_COMMAND: command,
      SYMFORGE_LIFECYCLE_ARTIFACTS: JSON.stringify(artifactPaths),
    },
  };
}

function validateFreshCommandReceipt(receiptPath, testId, test, command, evidence, artifactPaths) {
  const bytes = readBytes(receiptPath);
  let receipt;
  try {
    receipt = Buffer.isBuffer(bytes) ? JSON.parse(bytes.toString("utf8")) : null;
  } catch {
    receipt = null;
  }
  const fields = EXPECTED_RELEASE_VALIDATION.evidence_contract.materialized_command_receipt_fields;
  const expectedArtifacts = artifactPaths.map((artifactPath) => {
    const artifact = readBytes(artifactPath);
    return { path: artifactPath, sha256: Buffer.isBuffer(artifact) ? sha256Bytes(artifact) : null, status: "passed" };
  });
  return exactKeys(receipt, fields, `materialized_command_receipt.${testId}`) &&
    receipt.kind === "symforge.lifecycle_command_execution.v11" &&
    receipt.schema_version === 1 &&
    receipt.release_commit === evidence.release_commit &&
    receipt.release_tree === evidence.release_tree &&
    receipt.test_id === testId &&
    receipt.target === test.target &&
    receipt.command === command &&
    JSON.stringify(receipt.artifact_results) === JSON.stringify(expectedArtifacts) &&
    expectedArtifacts.every((artifact) => validSha256(artifact.sha256)) &&
    receipt.status === "passed";
}

function executeMaterializedOracleCommands(trace, acceptance, evidence) {
  const tests = trace.catalogs.tests;
  const commands = trace.catalogs.commands;
  const artifactsByTest = new Map();
  for (const oracle of acceptance.oracles) {
    if (!artifactsByTest.has(oracle.trace_test_id)) artifactsByTest.set(oracle.trace_test_id, []);
    artifactsByTest.get(oracle.trace_test_id).push(oracle.ci_artifact);
  }
  for (const artifacts of artifactsByTest.values()) artifacts.sort();
  for (const [testId, test] of Object.entries(tests).sort(([left], [right]) => left.localeCompare(right))) {
    if (!isObject(test) || !["planned_exact", "inherited_exact", "planned_benchmark"].includes(test.kind)) continue;
    const command = commands[test.command_id];
    const artifactPaths = artifactsByTest.get(testId) || [];
    if (artifactPaths.length === 0 || artifactPaths.some((artifact) => !safeArtifactPath(artifact))) {
      fail("MATERIALIZED_COMMAND_RECEIPT_INVALID", `${testId}: no closed oracle artifacts`);
      continue;
    }
    const receiptPath = `target/ci/lifecycle-v11/command-receipts/${testId.toLowerCase()}.json`;
    const receiptAbsolute = path.resolve(repositoryRoot, receiptPath);
    const receiptRoot = path.resolve(repositoryRoot, "target/ci/lifecycle-v11/command-receipts") + path.sep;
    if (!receiptAbsolute.startsWith(receiptRoot)) {
      fail("MATERIALIZED_COMMAND_RECEIPT_INVALID", `${testId}: receipt path`);
      continue;
    }
    try {
      fs.mkdirSync(path.dirname(receiptAbsolute), { recursive: true });
      fs.rmSync(receiptAbsolute, { force: true });
    } catch {
      fail("MATERIALIZED_COMMAND_RECEIPT_INVALID", `${testId}: cannot reset receipt`);
      continue;
    }
    const spec = materializedCommandSpec(testId, test, command, evidence, receiptPath, artifactPaths);
    const result = spec ? runVerifiedCommand(spec) : { ok: false, reason: "invalid_spec" };
    if (!result.ok) {
      fail("MATERIALIZED_COMMAND_FAILED", `${testId}: ${result.reason}`);
      continue;
    }
    if (!validateFreshCommandReceipt(receiptPath, testId, test, command, evidence, artifactPaths)) {
      fail("MATERIALIZED_COMMAND_RECEIPT_INVALID", testId);
    }
  }
}

function validateMaterialization(trace, acceptance, retirement, taskCatalog) {
  if (!cli.requireMaterialized) return;
  const evidenceBytes = readBytes(cli.evidence);
  if (evidenceBytes === null) {
    fail("RELEASE_EVIDENCE_READ", String(cli.evidence));
    return;
  }
  let evidence;
  try {
    evidence = JSON.parse(evidenceBytes.toString("utf8"));
  } catch {
    fail("RELEASE_EVIDENCE_SCHEMA_INVALID", String(cli.evidence));
    return;
  }
  if (!isObject(trace) || !isObject(acceptance) || !isObject(retirement) || !isObject(evidence)) {
    fail("RELEASE_EVIDENCE_SCHEMA_INVALID", "contracts or evidence unavailable");
    return;
  }
  const contract = EXPECTED_RELEASE_VALIDATION.evidence_contract;
  if (!exactKeys(evidence, contract.top_level_fields, "release_evidence")) {
    fail("RELEASE_EVIDENCE_SCHEMA_INVALID", "top-level object");
    return;
  }
  if (evidence.kind !== contract.kind || evidence.schema_version !== contract.schema_version || evidence.status !== "passed") {
    fail("RELEASE_EVIDENCE_SCHEMA_INVALID", "kind, version, or status");
  }
  const topLevel = gitText(["rev-parse", "--show-toplevel"]);
  const headCommit = gitText(["rev-parse", "--verify", "HEAD^{commit}"]);
  const headTree = gitText(["rev-parse", "--verify", "HEAD^{tree}"]);
  let canonicalRoot;
  let canonicalTopLevel;
  try {
    canonicalRoot = fs.realpathSync(repositoryRoot);
    canonicalTopLevel = topLevel === null ? null : fs.realpathSync(topLevel);
  } catch {
    canonicalRoot = null;
    canonicalTopLevel = null;
  }
  if (canonicalRoot === null || canonicalTopLevel === null ||
      canonicalRoot.toLowerCase() !== canonicalTopLevel.toLowerCase() ||
      evidence.release_commit !== headCommit || evidence.release_tree !== headTree) {
    fail("RELEASE_TREE_MISMATCH", "evidence must bind the current repository HEAD commit and tree");
    return;
  }
  if (!releaseWorktreeIsClean()) {
    fail("RELEASE_WORKTREE_DIRTY", "materialized validation requires a clean tracked and untracked release tree outside target/ci/lifecycle-v11");
    return;
  }
  validatePinnedReleaseFile(evidence, "trace_contract_sha256", TRACE_PATH, headCommit);
  validatePinnedReleaseFile(evidence, "acceptance_contract_sha256", ACCEPTANCE_PATH, headCommit);
  validatePinnedReleaseFile(evidence, "retirement_contract_sha256", RETIREMENT_PATH, headCommit);
  validatePinnedReleaseFile(evidence, "checker_sha256", CHECKER_PATH, headCommit);
  validateApprovedRefreeze(evidence, retirement, headCommit);
  validateTaskReceipts(evidence, headCommit, taskCatalog);
  validateRustCaseReceipts(evidence, trace, headCommit);
  validateSourceReceipts(evidence, trace, headCommit);
  validateRequirementReceipts(evidence, acceptance);
  validateOracleReceipts(evidence, acceptance);
  validateBenchmarkReceipts(evidence, trace, acceptance, headCommit);
  if (errors.length === 0) executeMaterializedOracleCommands(trace, acceptance, evidence);
}

function main() {
  const taskCatalog = loadV11TaskCatalog();
  const trace = parseSentinel(
    TRACE_PATH,
    "# Lifecycle Oracle Traceability Contract V11",
    "<!-- SYMFORGE LIFECYCLE ORACLE TRACEABILITY V11 JSON START -->",
    "<!-- SYMFORGE LIFECYCLE ORACLE TRACEABILITY V11 JSON END -->",
  );
  const acceptance = parseSentinel(
    ACCEPTANCE_PATH,
    "# Lifecycle Acceptance Oracles V11",
    "<!-- SYMFORGE LIFECYCLE ACCEPTANCE ORACLES V11 JSON START -->",
    "<!-- SYMFORGE LIFECYCLE ACCEPTANCE ORACLES V11 JSON END -->",
  );
  const retirement = parseSentinel(
    RETIREMENT_PATH,
    "# V10 Authority Retirement Inventory V11",
    "<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON START -->",
    "<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON END -->",
  );

  validateTrace(trace, taskCatalog);
  validateAcceptance(acceptance, trace, taskCatalog);
  validateRetirement(retirement, taskCatalog);
  validateFrozenContracts(trace, acceptance, retirement);
  validateMaterialization(trace, acceptance, retirement, taskCatalog);

  const stableErrors = [...new Set(errors)].sort();
  if (stableErrors.length > 0) {
    for (const error of stableErrors) process.stderr.write(`${error}\n`);
    process.exitCode = 1;
  } else {
    process.stdout.write(`lifecycle oracle traceability v11: OK (${EXPECTED_REQUIREMENTS.length} requirements, ${EXPECTED_ACCEPTANCE_ORACLES.size} acceptance oracles, ${RETIREMENT_CATEGORIES.length} retirement categories)\n`);
  }
}

if (require.main === module) main();
else module.exports = { runVerifiedCommand };
