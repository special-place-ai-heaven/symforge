//! Feature 020 V11 activation-cut oracles.
//!
//! Creating this file arms five `planned_exact` declarations in
//! `contracts/lifecycle-oracle-traceability-v11.md`: TEST-SURFACE (T050,
//! introduced in Slice 3) plus TEST-ACTIVATION, TEST-EMBED, TEST-MUTATION and
//! TEST-STATE (all T058, introduced in Slice 4). The pin requires every
//! declared case to EXIST once the file exists, so the four Slice 4 names are
//! present below as dark stand-ins — `#[ignore]` plus a panic, empty of proof,
//! the shape `process_capacity_pool_v11.rs` uses. They are not T050's work and
//! must not acquire bodies here.
//!
//! T050 IS THE ASSIGNMENT PROOF, NOT THE ACTIVATION. It proves every member of
//! the frozen retirement inventory has an exact Slice 4 owner, and that every
//! INGRESS member additionally carries the closed set of typed authority
//! branches it may take. It does not wire live authority: T058, T064 and T066
//! own that, the stand-ins stay dark, and nothing here reads a V11 runtime.
//!
//! WHY THE MATRIX IS NOT "244 members × eight branches". `INV-SURFACE` reads
//! "Every INGRESS resolves exactly one typed authority branch"; the eight names
//! are `MODEL-SURFACE`, a state model for ingress, not a label every retirement
//! member carries. Seven of the thirteen frozen entries — 153 of the 244 member
//! slots — never spell one of the eight, and the frozen JSON assigns a branch to
//! no member at all. Authoring 244 states would invent Slice 4 content the
//! inventory does not have; a per-category default would be false on its face,
//! since `tools` asserts all eight and `writers` splits permit-bearing from
//! permit-free in the same entry. So the matrix partitions: SURFACE categories
//! carry a per-member ALLOWED SET plus a basis citing frozen evidence, and
//! non-surface categories carry `None` and are proved on owner, seams and
//! disposition alone.
//!
//! ALLOWED SET, NOT A SINGLETON. Per call exactly one branch resolves; per
//! member the matrix records the closed set of branches that call may take.
//! `detect_changes` is the existence proof — it may resolve `GitObserved` or
//! `WorktreeScopeObserved` and must never resolve `GenerationLeased` — so a
//! singleton column could not describe it without lying.
//!
//! The inventory is PARSED at test time, never transcribed: copying 244 member
//! strings into this file would create a second inventory that drifts from the
//! frozen one silently. Only the thirteen entry-level shapes are pinned here,
//! so a change to an owner set or a production seam fails loudly.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// `MODEL-SURFACE`, test-local and closed. Deliberately NOT an enum in `src/`:
/// the typed authority surface is Slice 4's to build, and importing one here
/// would make this proof depend on the thing it exists to plan.
const MODEL_SURFACE: &[&str] = &[
    "DiskObserved",
    "GenerationLeased",
    "GitObserved",
    "MutationPermitted",
    "Refused",
    "RuntimeHealthObserved",
    "StateWriteAuthorized",
    "WorktreeScopeObserved",
];

/// The categories whose members are INGRESS. Membership is a judgement about
/// the frozen entry, recorded once here rather than re-derived per member:
/// each of these entries either spells `MODEL-SURFACE` tokens in its own
/// assertions, or (`writers`) splits permit-bearing from permit-free work in
/// them, which is the same distinction the branches encode.
const SURFACE_CATEGORIES: &[&str] = &[
    "compatibility_aliases",
    "hooks",
    "prompts",
    "resources",
    "sidecar",
    "tools",
    "writers",
];

/// Surface-category members that are NOT ingress, and so resolve no typed
/// authority branch at all. Round-16 shape change: a member can sit in a
/// surface category for owner and seams while doing work no branch describes —
/// mutating a `SharedIndex` is the case that forced it. Carrying `None` here is
/// a positive, pinned claim with a basis, not the absence of a row; the join
/// below requires every surface slot to be in EXACTLY ONE of this list or the
/// overlay, so a member cannot be quietly dropped from both.
const NON_INGRESS_EXCEPTIONS: &[(&str, &str, &str)] = &[
    (
        "writers",
        "src/live_index/single_file.rs::update_file_from_disk",
        "writers assertion 2 (`No writer mutates a V10 SharedIndex`) is what this member IS: it \
         calls `admit_and_index_single_path(.., shared, expected_gen)` (single_file.rs:669). It \
         writes no repository-source bytes and no ProjectStateDir, so neither MutationPermitted \
         nor StateWriteAuthorized reaches it; and a generation ARGUMENT is not GenerationLeased, \
         which INV-SURFACE defines as the branch holding a ProjectQueryLease. T064 routes the \
         equivalent work through the candidate pipeline WITHOUT a permit.",
    ),
    (
        "writers",
        "src/protocol/knowledge_curation.rs::apply_reviewed_mutation",
        "Staging and guards, with the branch at the writer. `prepare_mutation` calls it \
         (knowledge_curation.rs:938) while staging a BTreeMap; it performs no disk write and \
         holds no SourceMutationPermit. FR-037 puts the permit on the source-content side effect \
         — `write_policy`/`apply`, both overlayed — so MutationPermitted here would tell Slice 4 \
         to take a permit on a step that must stay permit-free. Its terminating errors \
         (unknown_action_id, action_preconditions_unmet, stale_target_guard, \
         unreproduced_evidence) are review/precondition failures, not INV-SURFACE Refused, which \
         is ingress selection against Current/Stale/Unavailable/foreign-root; and \
         `validate_target_on_disk` is a hash guard, not a DiskObserved ingress.",
    ),
    (
        "writers",
        "src/live_index/single_file.rs::remove_file",
        "Same ground as `update_file_from_disk`: it calls \
         `shared.remove_file_at_generation(.., shared.current_project_generation())` \
         (single_file.rs:680) — a SharedIndex mutation, not repository-source I/O, not \
         ProjectStateDir, and not a query lease.",
    ),
];

/// Ingress that carries NO source-authority branch: it runs, it succeeds, and
/// it pins no publication and observes no source. A third kind, forced by the
/// tree rather than chosen — overlaying any of the eight would lie about what
/// the call did, and exempting these as non-ingress would lie about what they
/// are.
///
/// Named for the PROPERTY, not for catalogs: `hook:PreTool` belongs here too,
/// and a name like `STATIC_CATALOG` would invite a fourth bucket for "hooks
/// that look like catalogs". The property is the absence of an authority
/// branch, whatever the ingress kind.
///
/// This is a FINDING ABOUT `INV-SURFACE`, recorded here rather than papered
/// over: "every ingress resolves exactly one typed authority branch" has no
/// honest member for these. T050 closes with the residual stated; Slice 4 must
/// either exclude them from that invariant or add a branch. Inventing one here
/// is not this slice's to do.
const AUTHORITY_FREE_INGRESS: &[(&str, &str, &str)] = &[
    (
        "hooks",
        "hook:PreTool",
        "Still ingress — PreToolUse ran and the process returned `Ok(())`; choosing no output is \
         fail-open without Current (hooks assertion 3), not a non-event, so NON_INGRESS_EXCEPTIONS \
         would be wrong. But no branch fits: it is handled before `endpoint_for` and makes no \
         sidecar call on any path (hook.rs:257-276). `read_sidecar_endpoint` is a liveness gate on \
         control state — not RuntimeHealthObserved, which INV-HEALTH scopes to committed-vs-attempt \
         fields — and it writes nothing, so not StateWriteAuthorized. `pre_tool_suggestion` formats \
         the stdin `tool_name`/path strings; it pins no publication and observes no source bytes, \
         so neither GenerationLeased nor DiskObserved. hooks assertion 1 leaves each hook its exact \
         branch, and this one has none.",
    ),
    (
        "resources",
        "symforge://glossary",
        "`render_glossary` is static markdown with no index access. It SUCCEEDS, so Refused would \
         misreport the outcome; it pins no publication, so GenerationLeased would be a false \
         Current; and resources assertion 4 (static catalog resources cannot disclose raw runtime \
         state) forbids the observed branches. None of the eight is honest.",
    ),
    (
        "resources",
        "symforge://tools/catalog",
        "`render_tool_catalog` walks `tool_catalog_groups()` — the advertised surface, not runtime \
         state, which resources assertion 4 forbids disclosing. Same shape as glossary: a \
         succeeding ingress with no publication to lease and no source observed.",
    ),
];

/// The one member string the frozen inventory files under two categories, with
/// different owner sets. Pinned so a future edit that collapses it — or adds a
/// second dual-homed member — fails instead of quietly changing the matrix's
/// row count.
const DUAL_HOMED_MEMBER: &str = "src/live_index/persist.rs::background_verify";
const DUAL_HOMED_CATEGORIES: &[&str] = &["callbacks", "snapshot"];

struct FrozenEntry {
    category: &'static str,
    members: usize,
    owners: &'static [&'static str],
    seams: &'static [&'static str],
}

const FROZEN: &[FrozenEntry] = &[
    FrozenEntry {
        category: "writers",
        members: 25,
        owners: &["T064", "T065", "T067"],
        seams: &[
            "src/index_lifecycle/mutation.rs::SourceMutationPermit",
            "src/index_lifecycle/runtime.rs::ProjectIndexRuntime",
        ],
    },
    FrozenEntry {
        category: "callbacks",
        members: 14,
        owners: &["T064", "T065", "T067"],
        seams: &[
            "src/index_lifecycle/observer.rs::ObserverHandoff",
            "src/index_lifecycle/supervisor.rs::SourceSupervisor",
        ],
    },
    FrozenEntry {
        category: "publication_roots",
        members: 9,
        owners: &["T066", "T067"],
        seams: &[
            "src/index_lifecycle/runtime.rs::ProjectIndexRuntime",
            "src/index_lifecycle/runtime.rs::ProjectPublicationRoot",
        ],
    },
    FrozenEntry {
        category: "cache",
        members: 9,
        owners: &["T066", "T067"],
        seams: &[
            "src/index_lifecycle/query.rs::ProjectQueryLease",
            "src/index_lifecycle/runtime.rs::ProjectPublicationRoot",
        ],
    },
    FrozenEntry {
        category: "ccr",
        members: 4,
        owners: &["T066", "T067"],
        seams: &[
            "src/index_lifecycle/query.rs::ProjectQueryLease",
            "src/protocol/read_gate.rs::ReadGate",
        ],
    },
    FrozenEntry {
        category: "snapshot",
        members: 13,
        owners: &["T065", "T067"],
        seams: &[
            "src/index_lifecycle/verification.rs::VerificationRecord",
            "src/live_index/persist.rs::IndexSnapshot",
        ],
    },
    FrozenEntry {
        category: "tools",
        members: 40,
        owners: &["T066", "T067"],
        seams: &[
            "src/index_lifecycle/mutation.rs::SourceMutationPermit",
            "src/index_lifecycle/public_api.rs::V11PublicApi",
            "src/index_lifecycle/query.rs::ProjectQueryLease",
        ],
    },
    FrozenEntry {
        category: "resources",
        members: 10,
        owners: &["T066", "T067"],
        seams: &[
            "src/index_lifecycle/query.rs::ProjectQueryLease",
            "src/protocol/read_gate.rs::ReadGate",
        ],
    },
    FrozenEntry {
        category: "prompts",
        members: 8,
        owners: &["T066", "T067"],
        seams: &[
            "src/index_lifecycle/query.rs::ProjectQueryLease",
            "src/protocol/read_gate.rs::ReadGate",
        ],
    },
    FrozenEntry {
        category: "sidecar",
        members: 24,
        owners: &["T064", "T066", "T067"],
        seams: &[
            "src/index_lifecycle/public_api.rs::V11PublicApi",
            "src/index_lifecycle/query.rs::ProjectQueryLease",
        ],
    },
    FrozenEntry {
        category: "hooks",
        members: 7,
        owners: &["T064", "T066", "T067"],
        seams: &[
            "src/index_lifecycle/activation.rs::ActivationCut",
            "src/index_lifecycle/mutation.rs::SourceMutationPermit",
            "src/index_lifecycle/query.rs::ProjectQueryLease",
        ],
    },
    FrozenEntry {
        category: "compatibility_aliases",
        members: 2,
        owners: &["T066", "T067"],
        seams: &[
            "src/index_lifecycle/activation.rs::ActivationCut",
            "src/index_lifecycle/public_api.rs::V11PublicApi",
        ],
    },
    FrozenEntry {
        category: "raw_embed",
        members: 79,
        owners: &["T067"],
        seams: &[
            "src/index_lifecycle/embedded.rs::EmbeddedSourceHandle",
            "src/index_lifecycle/process_runtime.rs::ProcessIndexRuntime",
            "src/index_lifecycle/public_api.rs::V11PublicApi",
        ],
    },
];

/// The authored half of the matrix: for each SURFACE `(category, member)`, the
/// closed set of `MODEL-SURFACE` branches that member may resolve, and the
/// basis for that set — a frozen assertion, an `INV-*` id, or a named V10
/// contract. Non-surface members are absent by construction and carry `None`.
///
/// EMPTY ON PURPOSE at T050's RED. Filling it is the next commit, one member at
/// a time with its basis; a member that cannot take an honest set is brought
/// back as a decision rather than parked on a plausible row.
const SURFACE_OVERLAY: &[(&str, &str, &[&str], &str)] = &[
    // ---- compatibility_aliases (2/2) ----
    // The calibration rows: the frozen entry states an allowed SET for one
    // alias and forbids a branch by name, which is the shape every row below
    // follows.
    (
        "compatibility_aliases",
        "detect_changes",
        &["GitObserved", "WorktreeScopeObserved"],
        "compatibility_aliases assertion: `detect_changes` returns GitObserved for committed-ref \
         diffs or WorktreeScopeObserved for worktree diffs, and never acquires a ProjectQueryLease \
         or upgrades observation evidence to GenerationLeased",
    ),
    (
        "compatibility_aliases",
        "trace_symbol",
        &["GenerationLeased", "Refused"],
        "compatibility_aliases assertion: `trace_symbol` cannot reach V10 symbol caches and uses \
         GenerationLeased ONLY for a complete Current publication. The `only` forbids the lease \
         on an incomplete publication but does not name the other outcome; ORACLE-INGRESS-CLOSED-\
         SURFACE and INV-SURFACE do — Refused is the branch that terminates selection. \
         {GenerationLeased} alone would claim this alias can never refuse.",
    ),
    // ---- writers (22/25; 3 brought back, see the T050 decision list) ----
    // Split per writers assertion 3, which draws the line the branches encode:
    // repository-source bytes are source-authorized (MutationPermitted), while
    // ProjectStateDir and post-image team-artifact writes remain permit-free
    // (StateWriteAuthorized). Family membership is cited per member, not
    // inherited from the module.
    (
        "writers",
        "src/cli/init.rs::run_init_with_paths",
        &["MutationPermitted", "StateWriteAuthorized"],
        "writers assertion 3, both halves, by the same wrapping pattern as edit_tools over \
         edit.rs: init calls `gitignore_hygiene::reconcile_project_gitignore` (init.rs ~375/381), \
         the source-authorized write already overlayed on that member, and \
         `paths::ensure_runtime_symforge_dir` (init.rs ~388), a ProjectStateDir write. Its \
         host-config I/O (`~/.claude.json`, desktop and cursor configs) is outside MODEL-SURFACE \
         entirely — that is not a ninth branch and does not eject the member.",
    ),
    (
        "writers",
        "src/gitignore_hygiene.rs::atomic_replace",
        &["MutationPermitted"],
        "writers assertion 3 names gitignore hygiene source-authorized; this is its byte writer",
    ),
    (
        "writers",
        "src/gitignore_hygiene.rs::reconcile_project_gitignore",
        &["MutationPermitted"],
        "writers assertion 3: gitignore hygiene is source-authorized; writes the project .gitignore",
    ),
    (
        "writers",
        "src/gitignore_hygiene.rs::reconcile_root_gitignore",
        &["MutationPermitted"],
        "writers assertion 3: gitignore hygiene is source-authorized; writes the root .gitignore",
    ),
    (
        "writers",
        "src/live_index/persist.rs::ensure_gitattributes_merge_hint",
        &["MutationPermitted"],
        "writers assertion 1: repository-source byte writer — writes `.gitattributes` under \
         project_root (persist.rs:1067), a committed repository file, so it is source-authorized \
         on the same ground as gitignore hygiene",
    ),
    (
        "writers",
        "src/protocol/edit.rs::atomic_write_file",
        &["MutationPermitted"],
        "writers assertion 1: repository-source byte writer",
    ),
    (
        "writers",
        "src/protocol/edit.rs::guarded_atomic_write_file",
        &["MutationPermitted"],
        "writers assertion 1: repository-source byte writer",
    ),
    (
        "writers",
        "src/protocol/edit.rs::execute_batch_edit",
        &["MutationPermitted"],
        "writers assertion 1: repository-source byte writer",
    ),
    (
        "writers",
        "src/protocol/edit.rs::execute_batch_insert",
        &["MutationPermitted"],
        "writers assertion 1: repository-source byte writer",
    ),
    (
        "writers",
        "src/protocol/edit.rs::execute_batch_rename",
        &["MutationPermitted"],
        "writers assertion 1: repository-source byte writer",
    ),
    (
        "writers",
        "src/protocol/edit_tools.rs::SymForgeServer::batch_edit",
        &["MutationPermitted"],
        "writers assertion 1: repository-source byte writer (tool ingress over edit.rs)",
    ),
    (
        "writers",
        "src/protocol/edit_tools.rs::SymForgeServer::batch_insert",
        &["MutationPermitted"],
        "writers assertion 1: repository-source byte writer (tool ingress over edit.rs)",
    ),
    (
        "writers",
        "src/protocol/edit_tools.rs::SymForgeServer::batch_rename",
        &["MutationPermitted"],
        "writers assertion 1: repository-source byte writer (tool ingress over edit.rs)",
    ),
    (
        "writers",
        "src/protocol/edit_tools.rs::SymForgeServer::delete_symbol",
        &["MutationPermitted"],
        "writers assertion 1: repository-source byte writer (tool ingress over edit.rs)",
    ),
    (
        "writers",
        "src/protocol/edit_tools.rs::SymForgeServer::edit_within_symbol",
        &["MutationPermitted"],
        "writers assertion 1: repository-source byte writer (tool ingress over edit.rs)",
    ),
    (
        "writers",
        "src/protocol/edit_tools.rs::SymForgeServer::insert_symbol",
        &["MutationPermitted"],
        "writers assertion 1: repository-source byte writer (tool ingress over edit.rs)",
    ),
    (
        "writers",
        "src/protocol/edit_tools.rs::SymForgeServer::replace_symbol_body",
        &["MutationPermitted"],
        "writers assertion 1: repository-source byte writer (tool ingress over edit.rs)",
    ),
    // ---- tools: edit family (7), the same sets as their writers rows ----
    (
        "tools",
        "batch_edit",
        &["MutationPermitted"],
        "tools assertion 3 (only repository-source MutationPermitted operations acquire a SourceMutationPermit); the tool-name form of the writers row src/protocol/edit_tools.rs::SymForgeServer::batch_edit, which edits repository source bytes",
    ),
    (
        "tools",
        "batch_insert",
        &["MutationPermitted"],
        "tools assertion 3 (only repository-source MutationPermitted operations acquire a SourceMutationPermit); the tool-name form of the writers row src/protocol/edit_tools.rs::SymForgeServer::batch_insert, which edits repository source bytes",
    ),
    (
        "tools",
        "batch_rename",
        &["MutationPermitted"],
        "tools assertion 3 (only repository-source MutationPermitted operations acquire a SourceMutationPermit); the tool-name form of the writers row src/protocol/edit_tools.rs::SymForgeServer::batch_rename, which edits repository source bytes",
    ),
    (
        "tools",
        "delete_symbol",
        &["MutationPermitted"],
        "tools assertion 3 (only repository-source MutationPermitted operations acquire a SourceMutationPermit); the tool-name form of the writers row src/protocol/edit_tools.rs::SymForgeServer::delete_symbol, which edits repository source bytes",
    ),
    (
        "tools",
        "edit_within_symbol",
        &["MutationPermitted"],
        "tools assertion 3 (only repository-source MutationPermitted operations acquire a SourceMutationPermit); the tool-name form of the writers row src/protocol/edit_tools.rs::SymForgeServer::edit_within_symbol, which edits repository source bytes",
    ),
    (
        "tools",
        "insert_symbol",
        &["MutationPermitted"],
        "tools assertion 3 (only repository-source MutationPermitted operations acquire a SourceMutationPermit); the tool-name form of the writers row src/protocol/edit_tools.rs::SymForgeServer::insert_symbol, which edits repository source bytes",
    ),
    (
        "tools",
        "replace_symbol_body",
        &["MutationPermitted"],
        "tools assertion 3 (only repository-source MutationPermitted operations acquire a SourceMutationPermit); the tool-name form of the writers row src/protocol/edit_tools.rs::SymForgeServer::replace_symbol_body, which edits repository source bytes",
    ),
    (
        "tools",
        "curate_knowledge",
        &["MutationPermitted", "StateWriteAuthorized"],
        "Same set as its writers row: the source policy write at repo_root POLICY_FILE (FR-037) plus the ProjectStateDir curation finalization",
    ),
    // ---- tools: the three that needed the whole body read ----
    // Each of these was nearly mis-assigned from a partial read, the same way
    // `run_init_with_paths` was. Publication and reload are NOT GenerationLeased:
    // that branch is the one holding a ProjectQueryLease.
    (
        "tools",
        "index_folder",
        &["MutationPermitted", "StateWriteAuthorized", "Refused"],
        "Three lanes, all in the body. MutationPermitted: it calls \
         `gitignore_hygiene::reconcile_project_gitignore` (tools.rs:7889) — the source write SC-018 \
         and `explicit_normal_index_folder_reconciles_existing_root_gitignore` exist for. \
         StateWriteAuthorized: `persist::reset_snapshot_state` (tools.rs:7843) plus the \
         idempotency records under control state (tools.rs:~7816). Refused: the typed refusal arms \
         — daemon unreachable, Unbound, `add:true`, and `Refused to index sensitive system path` \
         (tools.rs:7794). Publication and `reload_for_state_placement` stay OUTSIDE the set, the \
         way host-config I/O does on init.",
    ),
    (
        "tools",
        "checkpoint_now",
        &["MutationPermitted", "StateWriteAuthorized"],
        "Traced, not assumed: `export_artifact=true` reaches `persist::export_artifact`, which \
         calls `ensure_gitattributes_merge_hint` (persist.rs:1040) — the repository-source write \
         already overlayed on that writers member. The snapshot and `index.bin.zst` are \
         ProjectStateDir (FR-051, where the excluded artifact is persistence). No Refused: FR-052 \
         `applied=false` is the state-write lane reporting unavailability, not ingress \
         termination.",
    ),
    (
        "tools",
        "analyze_file_impact",
        &["DiskObserved", "GitObserved", "Refused"],
        "Refused is `foreign_project_refusal(params.0.project)` at the top of the body. \
         `File not found on disk` is path-local DiskObserved (T042), NOT Refused. Co-changes read \
         `git_temporal::GitTemporalState` when that lane runs, which is GitObserved. \
         `published_generation()` is the T046 footer capture, not a ProjectQueryLease — \
         GenerationLeased here would tell Slice 4 to lease a post-edit reindex that must run while \
         the source is non-Current, and the index update itself is T064 candidate work, the same \
         class as the `single_file` exceptions rather than a branch.",
    ),
    // ---- tools: git/worktree observation, pinned by the detect_changes row ----
    (
        "tools",
        "detect_impact",
        &["GitObserved", "WorktreeScopeObserved"],
        "The target detect_changes delegates to (compatibility_aliases assertion): GitObserved for committed-ref diffs, WorktreeScopeObserved for worktree diffs, and never a ProjectQueryLease",
    ),
    (
        "tools",
        "what_changed",
        &["GitObserved", "WorktreeScopeObserved", "Refused"],
        "Same observation pair as detect_impact; WhatChangedInput additionally documents a project selector that refuses a non-matching value (tools.rs), which is INV-SURFACE Refused terminating selection",
    ),
    // ---- tools: dual-lane ----
    (
        "tools",
        "validate_file_syntax",
        &["GenerationLeased", "DiskObserved"],
        "Its own body carries both lanes: an indexed read off the published generation and an AUTHORITATIVE disk-read lane taken when the same-project publication is refused (tools.rs:8968-8983, permits_authoritative_disk_fallback)",
    ),
    // ---- tools: generation-backed reads ----
    (
        "tools",
        "ask",
        &["GenerationLeased", "Refused"],
        "resources/tools assertion 3: a generation-backed read pinning one V11 publication, so it holds the ProjectQueryLease; SmartQueryInput documents a project selector that refuses a non-matching value, which is INV-SURFACE Refused",
    ),
    (
        "tools",
        "diff_symbols",
        &["GenerationLeased", "Refused"],
        "resources/tools assertion 3: a generation-backed read pinning one V11 publication, so it holds the ProjectQueryLease; DiffSymbolsInput documents a project selector that refuses a non-matching value, which is INV-SURFACE Refused",
    ),
    (
        "tools",
        "edit_plan",
        &["GenerationLeased", "Refused"],
        "resources/tools assertion 3: a generation-backed read pinning one V11 publication, so it holds the ProjectQueryLease; EditPlanInput documents a project selector that refuses a non-matching value, which is INV-SURFACE Refused",
    ),
    (
        "tools",
        "explore",
        &["GenerationLeased", "Refused"],
        "resources/tools assertion 3: a generation-backed read pinning one V11 publication, so it holds the ProjectQueryLease; ExploreInput documents a project selector that refuses a non-matching value, which is INV-SURFACE Refused",
    ),
    (
        "tools",
        "investigation_suggest",
        &["GenerationLeased", "Refused"],
        "resources/tools assertion 3: a generation-backed read pinning one V11 publication, so it holds the ProjectQueryLease; InvestigationInput documents a project selector that refuses a non-matching value, which is INV-SURFACE Refused",
    ),
    (
        "tools",
        "context_inventory",
        &["GenerationLeased"],
        "tools assertion 3: a generation-backed read over one V11 publication, so it is the branch that holds a ProjectQueryLease; its input documents no refusing project selector, so Refused is not sprayed on",
    ),
    (
        "tools",
        "conventions",
        &["GenerationLeased"],
        "tools assertion 3: a generation-backed read over one V11 publication, so it is the branch that holds a ProjectQueryLease; its input documents no refusing project selector, so Refused is not sprayed on",
    ),
    (
        "tools",
        "find_dependents",
        &["GenerationLeased"],
        "tools assertion 3: a generation-backed read over one V11 publication, so it is the branch that holds a ProjectQueryLease; its input documents no refusing project selector, so Refused is not sprayed on",
    ),
    (
        "tools",
        "find_references",
        &["GenerationLeased"],
        "tools assertion 3: a generation-backed read over one V11 publication, so it is the branch that holds a ProjectQueryLease; its input documents no refusing project selector, so Refused is not sprayed on",
    ),
    (
        "tools",
        "get_file_content",
        &["GenerationLeased"],
        "tools assertion 3: a generation-backed read over one V11 publication, so it is the branch that holds a ProjectQueryLease; its input documents no refusing project selector, so Refused is not sprayed on",
    ),
    (
        "tools",
        "get_file_context",
        &["GenerationLeased"],
        "tools assertion 3: a generation-backed read over one V11 publication, so it is the branch that holds a ProjectQueryLease; its input documents no refusing project selector, so Refused is not sprayed on",
    ),
    (
        "tools",
        "get_repo_map",
        &["GenerationLeased"],
        "tools assertion 3: a generation-backed read over one V11 publication, so it is the branch that holds a ProjectQueryLease; its input documents no refusing project selector, so Refused is not sprayed on",
    ),
    (
        "tools",
        "get_symbol",
        &["GenerationLeased"],
        "tools assertion 3: a generation-backed read over one V11 publication, so it is the branch that holds a ProjectQueryLease; its input documents no refusing project selector, so Refused is not sprayed on",
    ),
    (
        "tools",
        "get_symbol_context",
        &["GenerationLeased"],
        "tools assertion 3: a generation-backed read over one V11 publication, so it is the branch that holds a ProjectQueryLease; its input documents no refusing project selector, so Refused is not sprayed on",
    ),
    (
        "tools",
        "inspect_match",
        &["GenerationLeased"],
        "tools assertion 3: a generation-backed read over one V11 publication, so it is the branch that holds a ProjectQueryLease; its input documents no refusing project selector, so Refused is not sprayed on",
    ),
    (
        "tools",
        "review_knowledge",
        &["GenerationLeased"],
        "tools assertion 3: a generation-backed read over one V11 publication, so it is the branch that holds a ProjectQueryLease; its input documents no refusing project selector, so Refused is not sprayed on",
    ),
    (
        "tools",
        "search_files",
        &["GenerationLeased"],
        "tools assertion 3: a generation-backed read over one V11 publication, so it is the branch that holds a ProjectQueryLease; its input documents no refusing project selector, so Refused is not sprayed on",
    ),
    (
        "tools",
        "search_knowledge",
        &["GenerationLeased"],
        "tools assertion 3: a generation-backed read over one V11 publication, so it is the branch that holds a ProjectQueryLease; its input documents no refusing project selector, so Refused is not sprayed on",
    ),
    (
        "tools",
        "search_symbols",
        &["GenerationLeased"],
        "tools assertion 3: a generation-backed read over one V11 publication, so it is the branch that holds a ProjectQueryLease; its input documents no refusing project selector, so Refused is not sprayed on",
    ),
    (
        "tools",
        "search_text",
        &["GenerationLeased"],
        "tools assertion 3: a generation-backed read over one V11 publication, so it is the branch that holds a ProjectQueryLease; its input documents no refusing project selector, so Refused is not sprayed on",
    ),
    (
        "tools",
        "symforge_retrieve",
        &["GenerationLeased"],
        "tools assertion 3: a generation-backed read over one V11 publication, so it is the branch that holds a ProjectQueryLease; its input documents no refusing project selector, so Refused is not sprayed on",
    ),
    // ---- sidecar (24/24): 12 routes, each paired with its session twin ----
    (
        "sidecar",
        "GET /health",
        &["Refused", "RuntimeHealthObserved"],
        "`health_handler`; sidecar assertion 2 (RuntimeHealthObserved endpoints separate committed-generation fields from attempt and work state). standalone route. Refused is sidecar assertion 3 (caller-root mismatch selects Refused and cannot fall through to V10): `caller_root_guard` is layered over every route in this router (router.rs:52-56, handlers.rs:330-350), and the daemon-proxied session twin enforces the SAME check (daemon.rs:11605)",
    ),
    (
        "sidecar",
        "GET /v1/sessions/{session_id}/sidecar/health",
        &["Refused", "RuntimeHealthObserved"],
        "`health_handler`; sidecar assertion 2 (RuntimeHealthObserved endpoints separate committed-generation fields from attempt and work state). session-scoped twin of the standalone route, same handler and same guard. Refused is sidecar assertion 3 (caller-root mismatch selects Refused and cannot fall through to V10): `caller_root_guard` is layered over every route in this router (router.rs:52-56, handlers.rs:330-350), and the daemon-proxied session twin enforces the SAME check (daemon.rs:11605)",
    ),
    (
        "sidecar",
        "GET /impact",
        &["DiskObserved", "GitObserved", "Refused"],
        "`impact_handler`, the same lanes as `analyze_file_impact`: a path-local disk read and the git-temporal co-change lane. Not GenerationLeased - published_generation() there is the T046 footer capture, not a lease. standalone route. Refused is sidecar assertion 3 (caller-root mismatch selects Refused and cannot fall through to V10): `caller_root_guard` is layered over every route in this router (router.rs:52-56, handlers.rs:330-350), and the daemon-proxied session twin enforces the SAME check (daemon.rs:11605)",
    ),
    (
        "sidecar",
        "GET /v1/sessions/{session_id}/sidecar/impact",
        &["DiskObserved", "GitObserved", "Refused"],
        "`impact_handler`, the same lanes as `analyze_file_impact`: a path-local disk read and the git-temporal co-change lane. Not GenerationLeased - published_generation() there is the T046 footer capture, not a lease. session-scoped twin of the standalone route, same handler and same guard. Refused is sidecar assertion 3 (caller-root mismatch selects Refused and cannot fall through to V10): `caller_root_guard` is layered over every route in this router (router.rs:52-56, handlers.rs:330-350), and the daemon-proxied session twin enforces the SAME check (daemon.rs:11605)",
    ),
    (
        "sidecar",
        "GET /outline",
        &["GenerationLeased", "Refused"],
        "`outline_handler`, the generation-backed file outline; sidecar assertion 1 (generation-backed endpoints use GenerationLeased). standalone route. Refused is sidecar assertion 3 (caller-root mismatch selects Refused and cannot fall through to V10): `caller_root_guard` is layered over every route in this router (router.rs:52-56, handlers.rs:330-350), and the daemon-proxied session twin enforces the SAME check (daemon.rs:11605)",
    ),
    (
        "sidecar",
        "GET /v1/sessions/{session_id}/sidecar/outline",
        &["GenerationLeased", "Refused"],
        "`outline_handler`, the generation-backed file outline; sidecar assertion 1 (generation-backed endpoints use GenerationLeased). session-scoped twin of the standalone route, same handler and same guard. Refused is sidecar assertion 3 (caller-root mismatch selects Refused and cannot fall through to V10): `caller_root_guard` is layered over every route in this router (router.rs:52-56, handlers.rs:330-350), and the daemon-proxied session twin enforces the SAME check (daemon.rs:11605)",
    ),
    (
        "sidecar",
        "GET /prompt-context",
        &["GenerationLeased", "Refused"],
        "`prompt_context_handler`, generation-backed: `require_queryable_sidecar_index` then `capture_queryable_sidecar_generation` (handlers.rs:2274+). standalone route. Refused is sidecar assertion 3 (caller-root mismatch selects Refused and cannot fall through to V10): `caller_root_guard` is layered over every route in this router (router.rs:52-56, handlers.rs:330-350), and the daemon-proxied session twin enforces the SAME check (daemon.rs:11605)",
    ),
    (
        "sidecar",
        "GET /v1/sessions/{session_id}/sidecar/prompt-context",
        &["GenerationLeased", "Refused"],
        "`prompt_context_handler`, generation-backed: `require_queryable_sidecar_index` then `capture_queryable_sidecar_generation` (handlers.rs:2274+). session-scoped twin of the standalone route, same handler and same guard. Refused is sidecar assertion 3 (caller-root mismatch selects Refused and cannot fall through to V10): `caller_root_guard` is layered over every route in this router (router.rs:52-56, handlers.rs:330-350), and the daemon-proxied session twin enforces the SAME check (daemon.rs:11605)",
    ),
    (
        "sidecar",
        "GET /repo-map",
        &["GenerationLeased", "Refused"],
        "`repo_map_handler`, the generation-backed repo map; sidecar assertion 1. standalone route. Refused is sidecar assertion 3 (caller-root mismatch selects Refused and cannot fall through to V10): `caller_root_guard` is layered over every route in this router (router.rs:52-56, handlers.rs:330-350), and the daemon-proxied session twin enforces the SAME check (daemon.rs:11605)",
    ),
    (
        "sidecar",
        "GET /v1/sessions/{session_id}/sidecar/repo-map",
        &["GenerationLeased", "Refused"],
        "`repo_map_handler`, the generation-backed repo map; sidecar assertion 1. session-scoped twin of the standalone route, same handler and same guard. Refused is sidecar assertion 3 (caller-root mismatch selects Refused and cannot fall through to V10): `caller_root_guard` is layered over every route in this router (router.rs:52-56, handlers.rs:330-350), and the daemon-proxied session twin enforces the SAME check (daemon.rs:11605)",
    ),
    (
        "sidecar",
        "GET /stats",
        &["Refused", "RuntimeHealthObserved"],
        "`stats_handler` returns `state.token_stats.summary()` (handlers.rs:2452) - sidecar PROCESS work state, no publication pinned and no source observed. Sidecar assertion 1 enumerates runtime-health as the matching typed branch, and assertion 2's committed-vs-work separation is exactly what this endpoint reports. standalone route. Refused is sidecar assertion 3 (caller-root mismatch selects Refused and cannot fall through to V10): `caller_root_guard` is layered over every route in this router (router.rs:52-56, handlers.rs:330-350), and the daemon-proxied session twin enforces the SAME check (daemon.rs:11605)",
    ),
    (
        "sidecar",
        "GET /v1/sessions/{session_id}/sidecar/stats",
        &["Refused", "RuntimeHealthObserved"],
        "`stats_handler` returns `state.token_stats.summary()` (handlers.rs:2452) - sidecar PROCESS work state, no publication pinned and no source observed. Sidecar assertion 1 enumerates runtime-health as the matching typed branch, and assertion 2's committed-vs-work separation is exactly what this endpoint reports. session-scoped twin of the standalone route, same handler and same guard. Refused is sidecar assertion 3 (caller-root mismatch selects Refused and cannot fall through to V10): `caller_root_guard` is layered over every route in this router (router.rs:52-56, handlers.rs:330-350), and the daemon-proxied session twin enforces the SAME check (daemon.rs:11605)",
    ),
    (
        "sidecar",
        "GET /symbol-context",
        &["GenerationLeased", "Refused"],
        "`symbol_context_handler`, generation-backed symbol context; sidecar assertion 1. standalone route. Refused is sidecar assertion 3 (caller-root mismatch selects Refused and cannot fall through to V10): `caller_root_guard` is layered over every route in this router (router.rs:52-56, handlers.rs:330-350), and the daemon-proxied session twin enforces the SAME check (daemon.rs:11605)",
    ),
    (
        "sidecar",
        "GET /v1/sessions/{session_id}/sidecar/symbol-context",
        &["GenerationLeased", "Refused"],
        "`symbol_context_handler`, generation-backed symbol context; sidecar assertion 1. session-scoped twin of the standalone route, same handler and same guard. Refused is sidecar assertion 3 (caller-root mismatch selects Refused and cannot fall through to V10): `caller_root_guard` is layered over every route in this router (router.rs:52-56, handlers.rs:330-350), and the daemon-proxied session twin enforces the SAME check (daemon.rs:11605)",
    ),
    (
        "sidecar",
        "GET /workflows/post-edit-impact",
        &["DiskObserved", "GitObserved", "Refused"],
        "Thin alias (sidecar assertion 4): workflow_post_edit_impact_handler calls impact_handler (handlers.rs:1045), so it takes `/impact`'s set exactly. standalone route. Refused is sidecar assertion 3 (caller-root mismatch selects Refused and cannot fall through to V10): `caller_root_guard` is layered over every route in this router (router.rs:52-56, handlers.rs:330-350), and the daemon-proxied session twin enforces the SAME check (daemon.rs:11605)",
    ),
    (
        "sidecar",
        "GET /v1/sessions/{session_id}/sidecar/workflows/post-edit-impact",
        &["DiskObserved", "GitObserved", "Refused"],
        "Thin alias (sidecar assertion 4): workflow_post_edit_impact_handler calls impact_handler (handlers.rs:1045), so it takes `/impact`'s set exactly. session-scoped twin, same handler and same guard. Refused is sidecar assertion 3 (caller-root mismatch selects Refused and cannot fall through to V10): `caller_root_guard` is layered over every route in this router (router.rs:52-56, handlers.rs:330-350), and the daemon-proxied session twin enforces the SAME check (daemon.rs:11605)",
    ),
    (
        "sidecar",
        "GET /workflows/prompt-context",
        &["GenerationLeased", "Refused"],
        "Thin alias (sidecar assertion 4): workflow_prompt_narrowing_handler calls prompt_context_handler (handlers.rs:2296), so it takes `/prompt-context`'s set exactly. standalone route. Refused is sidecar assertion 3 (caller-root mismatch selects Refused and cannot fall through to V10): `caller_root_guard` is layered over every route in this router (router.rs:52-56, handlers.rs:330-350), and the daemon-proxied session twin enforces the SAME check (daemon.rs:11605)",
    ),
    (
        "sidecar",
        "GET /v1/sessions/{session_id}/sidecar/workflows/prompt-context",
        &["GenerationLeased", "Refused"],
        "Thin alias (sidecar assertion 4): workflow_prompt_narrowing_handler calls prompt_context_handler (handlers.rs:2296), so it takes `/prompt-context`'s set exactly. session-scoped twin, same handler and same guard. Refused is sidecar assertion 3 (caller-root mismatch selects Refused and cannot fall through to V10): `caller_root_guard` is layered over every route in this router (router.rs:52-56, handlers.rs:330-350), and the daemon-proxied session twin enforces the SAME check (daemon.rs:11605)",
    ),
    (
        "sidecar",
        "GET /workflows/repo-start",
        &["GenerationLeased", "Refused"],
        "Thin alias (sidecar assertion 4): workflow_repo_start_handler calls repo_map_handler (handlers.rs:2046), so it takes `/repo-map`'s set exactly. standalone route. Refused is sidecar assertion 3 (caller-root mismatch selects Refused and cannot fall through to V10): `caller_root_guard` is layered over every route in this router (router.rs:52-56, handlers.rs:330-350), and the daemon-proxied session twin enforces the SAME check (daemon.rs:11605)",
    ),
    (
        "sidecar",
        "GET /v1/sessions/{session_id}/sidecar/workflows/repo-start",
        &["GenerationLeased", "Refused"],
        "Thin alias (sidecar assertion 4): workflow_repo_start_handler calls repo_map_handler (handlers.rs:2046), so it takes `/repo-map`'s set exactly. session-scoped twin, same handler and same guard. Refused is sidecar assertion 3 (caller-root mismatch selects Refused and cannot fall through to V10): `caller_root_guard` is layered over every route in this router (router.rs:52-56, handlers.rs:330-350), and the daemon-proxied session twin enforces the SAME check (daemon.rs:11605)",
    ),
    (
        "sidecar",
        "GET /workflows/search-hit-expansion",
        &["GenerationLeased", "Refused"],
        "Thin alias (sidecar assertion 4): workflow_search_hit_expansion_handler calls symbol_context_handler (handlers.rs:1727), so it takes `/symbol-context`'s set exactly. standalone route. Refused is sidecar assertion 3 (caller-root mismatch selects Refused and cannot fall through to V10): `caller_root_guard` is layered over every route in this router (router.rs:52-56, handlers.rs:330-350), and the daemon-proxied session twin enforces the SAME check (daemon.rs:11605)",
    ),
    (
        "sidecar",
        "GET /v1/sessions/{session_id}/sidecar/workflows/search-hit-expansion",
        &["GenerationLeased", "Refused"],
        "Thin alias (sidecar assertion 4): workflow_search_hit_expansion_handler calls symbol_context_handler (handlers.rs:1727), so it takes `/symbol-context`'s set exactly. session-scoped twin, same handler and same guard. Refused is sidecar assertion 3 (caller-root mismatch selects Refused and cannot fall through to V10): `caller_root_guard` is layered over every route in this router (router.rs:52-56, handlers.rs:330-350), and the daemon-proxied session twin enforces the SAME check (daemon.rs:11605)",
    ),
    (
        "sidecar",
        "GET /workflows/source-read",
        &["GenerationLeased", "Refused"],
        "Thin alias (sidecar assertion 4): workflow_source_read_handler calls outline_handler (handlers.rs:634), so it takes `/outline`'s set exactly. standalone route. Refused is sidecar assertion 3 (caller-root mismatch selects Refused and cannot fall through to V10): `caller_root_guard` is layered over every route in this router (router.rs:52-56, handlers.rs:330-350), and the daemon-proxied session twin enforces the SAME check (daemon.rs:11605)",
    ),
    (
        "sidecar",
        "GET /v1/sessions/{session_id}/sidecar/workflows/source-read",
        &["GenerationLeased", "Refused"],
        "Thin alias (sidecar assertion 4): workflow_source_read_handler calls outline_handler (handlers.rs:634), so it takes `/outline`'s set exactly. session-scoped twin, same handler and same guard. Refused is sidecar assertion 3 (caller-root mismatch selects Refused and cannot fall through to V10): `caller_root_guard` is layered over every route in this router (router.rs:52-56, handlers.rs:330-350), and the daemon-proxied session twin enforces the SAME check (daemon.rs:11605)",
    ),
    // ---- hooks (6 of 7; PreTool is unruled) ----
    // Routing confirmed from `endpoint_for` (cli/hook.rs:944), NOT from
    // `workflow_for_subcommand`, which says it does not change routing yet.
    // Fail-open (`fail_open_json`, empty additionalContext) adds no branch: it
    // is not Refused and, per hooks assertion 3, carries no false Current.
    (
        "hooks",
        "hook:Read",
        &["GenerationLeased"],
        "endpoint_for routes Read to `/outline` (hook.rs ~957) — the file-outline read, the same \
         generation-backed set as `get_file_context`, not repo-map. hooks assertion 1: \
         GenerationLeased only for generation-backed context.",
    ),
    (
        "hooks",
        "hook:SessionStart",
        &["GenerationLeased"],
        "endpoint_for routes SessionStart to `/repo-map`, so it takes the set of `get_repo_map`",
    ),
    (
        "hooks",
        "hook:PromptSubmit",
        &["GenerationLeased"],
        "endpoint_for routes PromptSubmit to `/prompt-context`, whose handler is generation-backed \
         — `require_queryable_sidecar_index` then `capture_queryable_sidecar_generation` \
         (sidecar/handlers.rs:2274+)",
    ),
    (
        "hooks",
        "hook:Grep",
        &["GenerationLeased", "RuntimeHealthObserved"],
        "A genuine fork inside one ingress, like detect_changes: endpoint_for sends a plausible \
         symbol name to `/symbol-context` (generation-backed) and everything else to `/health` \
         (hook.rs ~984-990), so the allowed set is the union of the two lanes the call may take",
    ),
    (
        "hooks",
        "hook:Edit",
        &["DiskObserved", "GitObserved"],
        "endpoint_for routes Edit to `/impact`, the `analyze_file_impact` lanes. It CANNOT be \
         MutationPermitted — hooks assertion 2: Edit and Write notifications cannot publish, mint \
         a SourceMutationPermit, or bypass mutation authority. Refused is dropped from the tool's \
         set because THIS process does not terminate selection: the hook fails open with empty \
         additionalContext (hook.rs:8, :314), and the caller_root guard that could refuse belongs \
         to the sidecar, not to this ingress.",
    ),
    (
        "hooks",
        "hook:Write",
        &["DiskObserved", "GitObserved"],
        "Same `/impact` route as hook:Edit with `new_file=true`, same assertion 2 prohibition, and \
         the same fail-open reason for dropping Refused",
    ),
    // ---- resources (7 of 10; glossary and tools/catalog are unruled) ----
    // A resource wrapper starts from the set of the tool it wraps, then drops
    // the lanes that invocation cannot take. It never gains one: resources
    // assertion 5 says template expansion preserves the selected branch and
    // never upgrades an observation into Current.
    (
        "resources",
        "symforge://file/content",
        &["GenerationLeased"],
        "Wraps `get_file_content`, whose set is {GenerationLeased}; resources assertion 1 \
         (generation-backed resources use GenerationLeased and pin one V11 publication)",
    ),
    (
        "resources",
        "symforge://file/context",
        &["GenerationLeased"],
        "Wraps `get_file_context`, same set, same ground as file/content",
    ),
    (
        "resources",
        "symforge://symbol/detail",
        &["GenerationLeased"],
        "Wraps `get_symbol`, whose set is {GenerationLeased} (resources assertion 1)",
    ),
    (
        "resources",
        "symforge://symbol/context",
        &["GenerationLeased"],
        "Wraps `get_symbol_context`, same set, same ground as symbol/detail",
    ),
    (
        "resources",
        "symforge://repo/map",
        &["GenerationLeased"],
        "Wraps `get_repo_map`, whose set is {GenerationLeased} (resources assertion 1)",
    ),
    (
        "resources",
        "symforge://repo/outline",
        &["GenerationLeased"],
        "The same `get_repo_map` invocation at detail=full, so the same set — a detail level is \
         not a different authority branch",
    ),
    (
        "resources",
        "symforge://repo/changes/uncommitted",
        &["WorktreeScopeObserved"],
        "Wraps `what_changed` with `git_ref: None`, which is the worktree-diff lane ONLY, so the \
         wrapper drops GitObserved (that lane needs a committed ref this invocation cannot \
         supply) and drops Refused (no caller-supplied selector to refuse). resources assertion 2: \
         worktree-scope resources use their lease-free observed branch.",
    ),
    // ---- the health trio + its resource (4) ----
    // Runtime health is the one family that reports on the runtime itself
    // rather than on indexed content, so it takes its lease-free observed
    // branch and never a ProjectQueryLease. Family membership is a basis, not a
    // default: each member cites why it is in the family.
    (
        "tools",
        "health",
        &["RuntimeHealthObserved"],
        "tools assertion 4 (RuntimeHealthObserved keeps committed-generation fields separate from \
         bounded attempt and runtime-work fields) is about this tool's report; tools assertion 3 \
         (only GenerationLeased acquires a ProjectQueryLease) keeps it lease-free",
    ),
    (
        "tools",
        "health_compact",
        &["RuntimeHealthObserved"],
        "The compact projection of `health`, reporting the same runtime-health fields under tools \
         assertion 4, and lease-free by tools assertion 3",
    ),
    (
        "tools",
        "status",
        &["RuntimeHealthObserved"],
        "Reports index/daemon runtime state, not indexed content, so it is a runtime-health \
         observation under tools assertion 4 and acquires no ProjectQueryLease (tools assertion 3)",
    ),
    (
        "resources",
        "symforge://repo/health",
        &["RuntimeHealthObserved"],
        "resources assertion 2 (pure disk, worktree-scope, git and runtime-health resources use \
         their lease-free observed branches with typed provenance) plus assertion 3 \
         (RuntimeHealthObserved resources cannot mix attempt-only fields into committed-generation \
         truth) — the resource form of the same family",
    ),
    // The policy file is `repo_root.join(POLICY_FILE)` (knowledge_curation.rs:31,
    // :541, :544, :607, :904) — NORMAL SOURCE, not `.symforge/` state, so FR-037
    // requires a SourceMutationPermit before that write. The "post-image
    // team-artifact state writes" of assertion 3 are FR-037 completion
    // finalization in ProjectStateDir once the source already matches the
    // post-image; they are not the ledger write itself. Rows that do both carry
    // both branches.
    (
        "writers",
        "src/protocol/knowledge_curation.rs::KnowledgeCurationCoordinator::write_policy",
        &["MutationPermitted"],
        "FR-037: writes `repo_root.join(POLICY_FILE)` = `.symforge-knowledge.toml` \
         (knowledge_curation.rs:31, :541), which the data model calls normal source, so the \
         write requires a SourceMutationPermit. It is not a ProjectStateDir write.",
    ),
    (
        "writers",
        "src/protocol/knowledge_curation.rs::KnowledgeCurationCoordinator::apply",
        &["MutationPermitted", "StateWriteAuthorized"],
        "Does both: the source policy write via write_policy (FR-037, repo_root POLICY_FILE) and \
         the ProjectStateDir curation state under state_dir/CURATION_STATE_DIR \
         (knowledge_curation.rs:351), which is the permit-free half of writers assertion 3",
    ),
    (
        "writers",
        "src/protocol/knowledge_curation.rs::durable_replace",
        &["MutationPermitted", "StateWriteAuthorized"],
        "The durable writer for both halves: the policy post-image at repo_root \
         (knowledge_curation.rs:629, source, FR-037) and the curation lineage/record/quarantine \
         state under ProjectStateDir (:1808, :1888, :1901)",
    ),
    (
        "writers",
        "src/protocol/knowledge_curation.rs::durable_replace_io",
        &["MutationPermitted", "StateWriteAuthorized"],
        "The io half of durable_replace, reached from the same call sites, so it spans the same \
         two write kinds",
    ),
    (
        "writers",
        "src/protocol/tools.rs::SymForgeServer::curate_knowledge",
        &["MutationPermitted", "StateWriteAuthorized"],
        "Tool ingress for curation: it dispatches the source policy write and the ProjectStateDir \
         finalization, so its allowed set is the union of what it dispatches, not a weaker \
         singleton",
    ),
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// Parsed frozen inventory: `(category, member)` slots in file order, plus the
/// document-level owner task list.
struct Inventory {
    slots: Vec<(String, String)>,
    entries: BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)>,
    document_tasks: BTreeSet<String>,
}

fn load_inventory() -> Inventory {
    let path = repo_root()
        .join("specs")
        .join("020-repository-knowledge-index")
        .join("contracts")
        .join("v10-authority-retirement-v11.md");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("read the frozen retirement inventory at {path:?}: {error}");
    });
    let start = text
        .find("```json")
        .expect("the frozen inventory carries a fenced json block");
    let rest = &text[start + "```json".len()..];
    let end = rest
        .find("```")
        .expect("the fenced json block in the frozen inventory is closed");
    let json: serde_json::Value =
        serde_json::from_str(rest[..end].trim()).expect("the frozen inventory's json block parses");

    let document_tasks = json["slice4_owner"]["tasks"]
        .as_array()
        .expect("slice4_owner.tasks is an array")
        .iter()
        .map(|task| task.as_str().expect("a task id is a string").to_string())
        .collect();

    let mut slots = Vec::new();
    let mut entries = BTreeMap::new();
    for entry in json["entries"]
        .as_array()
        .expect("entries is an array")
        .iter()
    {
        let category = entry["category"]
            .as_str()
            .expect("category is a string")
            .to_string();
        let read_set = |field: &str| -> BTreeSet<String> {
            entry[field]
                .as_array()
                .unwrap_or_else(|| panic!("{category}.{field} is an array"))
                .iter()
                .map(|value| value.as_str().expect("a string").to_string())
                .collect()
        };
        let owners = read_set("slice4_owner_tasks");
        let seams = read_set("production_seams");
        for member in entry["members"].as_array().expect("members is an array") {
            slots.push((
                category.clone(),
                member.as_str().expect("a member is a string").to_string(),
            ));
        }
        assert!(
            entries.insert(category.clone(), (owners, seams)).is_none(),
            "category `{category}` appears twice in the frozen inventory"
        );
    }
    Inventory {
        slots,
        entries,
        document_tasks,
    }
}

/// TEST-SURFACE (T050). The name is pinned by
/// `contracts/lifecycle-oracle-traceability-v11.md` as a `planned_exact`
/// target; do not rename it without amending that contract.
#[test]
fn all_ingress_uses_exact_typed_authority_branch() {
    let inventory = load_inventory();

    // The frozen shape itself, pinned per entry. Parsing gives the members;
    // these constants are what make a change to an owner set or a production
    // seam fail here rather than pass silently into the matrix.
    assert_eq!(
        inventory.entries.len(),
        FROZEN.len(),
        "the frozen inventory holds {} categories, this test pins {}",
        inventory.entries.len(),
        FROZEN.len()
    );
    let mut expected_slots = 0;
    for frozen in FROZEN {
        let (owners, seams) = inventory
            .entries
            .get(frozen.category)
            .unwrap_or_else(|| panic!("frozen inventory lost category `{}`", frozen.category));
        let pinned_owners: BTreeSet<String> =
            frozen.owners.iter().map(|o| (*o).to_string()).collect();
        let pinned_seams: BTreeSet<String> =
            frozen.seams.iter().map(|s| (*s).to_string()).collect();
        assert_eq!(
            *owners, pinned_owners,
            "`{}` owner set moved; owner is the frozen SET, exactly as frozen",
            frozen.category
        );
        assert_eq!(
            *seams, pinned_seams,
            "`{}` production seams moved",
            frozen.category
        );
        assert!(
            owners.is_subset(&inventory.document_tasks),
            "`{}` names an owner task outside the document-level slice4_owner.tasks {:?}",
            frozen.category,
            inventory.document_tasks
        );
        let counted = inventory
            .slots
            .iter()
            .filter(|(category, _)| category == frozen.category)
            .count();
        assert_eq!(
            counted, frozen.members,
            "`{}` member count moved",
            frozen.category
        );
        expected_slots += frozen.members;
    }
    assert_eq!(
        inventory.slots.len(),
        expected_slots,
        "the join must see every frozen slot"
    );
    assert_eq!(
        expected_slots, 244,
        "the frozen inventory holds 244 member slots"
    );

    // The dual-homed member. 244 SLOTS, 243 distinct strings: keying the matrix
    // on `(category, member)` is what keeps the two rows separable, since they
    // carry different owner sets.
    let mut homes: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (category, member) in &inventory.slots {
        homes
            .entry(member.as_str())
            .or_default()
            .insert(category.as_str());
    }
    let dual: Vec<_> = homes
        .iter()
        .filter(|(_, categories)| categories.len() > 1)
        .collect();
    assert_eq!(
        dual.len(),
        1,
        "exactly one member string is dual-homed; found {:?}",
        dual.iter().map(|(m, _)| *m).collect::<Vec<_>>()
    );
    let (member, categories) = dual[0];
    assert_eq!(*member, DUAL_HOMED_MEMBER, "the dual-homed member moved");
    assert_eq!(
        *categories,
        DUAL_HOMED_CATEGORIES
            .iter()
            .copied()
            .collect::<BTreeSet<_>>(),
        "the dual-homed member changed categories"
    );
    assert_eq!(
        homes.len(),
        243,
        "244 slots over 243 distinct member strings"
    );

    // The overlay must join onto the frozen slots BIJECTIVELY: every surface
    // slot supplied exactly once, no overlay row that names a slot the frozen
    // inventory does not have, and no overlay row on a non-surface slot.
    let surface: BTreeSet<&str> = SURFACE_CATEGORIES.iter().copied().collect();
    let model: BTreeSet<&str> = MODEL_SURFACE.iter().copied().collect();
    let mut overlay: BTreeMap<(&str, &str), (BTreeSet<&str>, &str)> = BTreeMap::new();
    for (category, member, allowed, basis) in SURFACE_OVERLAY {
        assert!(
            overlay
                .insert(
                    (*category, *member),
                    (allowed.iter().copied().collect(), *basis)
                )
                .is_none(),
            "overlay names `{category}::{member}` twice"
        );
    }

    // The pinned non-ingress exceptions, checked against the same frozen slots
    // and required to be disjoint from the overlay: a member may be exempt or
    // branch-bearing, never both and never neither.
    let mut exceptions: BTreeMap<(&str, &str), &str> = BTreeMap::new();
    for (category, member, basis) in NON_INGRESS_EXCEPTIONS {
        assert!(
            surface.contains(category),
            "`{category}::{member}` is pinned as a non-ingress exception, but `{category}` is not \
             a surface category — non-surface members already carry no branch"
        );
        assert!(
            !basis.trim().is_empty(),
            "`{category}::{member}` is exempt with no basis; an exemption that cannot say why is \
             the parking this test exists to prevent"
        );
        assert!(
            exceptions.insert((*category, *member), *basis).is_none(),
            "`{category}::{member}` is pinned as an exception twice"
        );
        assert!(
            !overlay.contains_key(&(*category, *member)),
            "`{category}::{member}` is both branch-bearing and exempt; pick one"
        );
    }

    // The static-catalog list, held to the same standard as the exceptions and
    // disjoint from both other lists: every surface slot lands in EXACTLY ONE
    // of overlay, non-ingress exception, or static catalog.
    let mut statics: BTreeMap<(&str, &str), &str> = BTreeMap::new();
    for (category, member, basis) in AUTHORITY_FREE_INGRESS {
        assert!(
            surface.contains(category),
            "`{category}::{member}` is pinned as static catalog, but `{category}` is not a \
             surface category"
        );
        assert!(
            !basis.trim().is_empty(),
            "`{category}::{member}` is pinned as authority-free ingress with no basis"
        );
        assert!(
            statics.insert((*category, *member), *basis).is_none(),
            "`{category}::{member}` is pinned as authority-free ingress twice"
        );
        assert!(
            !overlay.contains_key(&(*category, *member))
                && !exceptions.contains_key(&(*category, *member)),
            "`{category}::{member}` is pinned in more than one of overlay / non-ingress \
             exception / authority-free ingress; the three are exclusive"
        );
    }

    let mut missing = Vec::new();
    let mut wrongly_present = Vec::new();
    for (category, member) in &inventory.slots {
        let key = (category.as_str(), member.as_str());
        if exceptions.contains_key(&key) || statics.contains_key(&key) {
            continue;
        }
        match (surface.contains(category.as_str()), overlay.get(&key)) {
            (true, None) => missing.push(format!("{category}::{member}")),
            (false, Some(_)) => wrongly_present.push(format!("{category}::{member}")),
            (true, Some((allowed, basis))) => {
                assert!(
                    !allowed.is_empty(),
                    "{category}::{member} has an empty allowed set; a member that can take \
                     no branch is a decision, not a row"
                );
                assert!(
                    allowed.is_subset(&model),
                    "{category}::{member} names a branch outside MODEL-SURFACE: {:?}",
                    allowed.difference(&model).collect::<Vec<_>>()
                );
                assert!(
                    !basis.trim().is_empty(),
                    "{category}::{member} has no basis; an assignment that cannot say why \
                     is an assertion, not evidence"
                );
            }
            (false, None) => {}
        }
    }
    let overlay_slots: BTreeSet<(&str, &str)> = overlay.keys().copied().collect();
    let frozen_slots: BTreeSet<(&str, &str)> = inventory
        .slots
        .iter()
        .map(|(c, m)| (c.as_str(), m.as_str()))
        .collect();
    let unknown: Vec<_> = overlay_slots.difference(&frozen_slots).collect();
    assert!(
        unknown.is_empty(),
        "overlay names slots the frozen inventory does not have: {unknown:?}"
    );
    let exception_slots: BTreeSet<(&str, &str)> = exceptions.keys().copied().collect();
    let unknown_exceptions: Vec<_> = exception_slots.difference(&frozen_slots).collect();
    assert!(
        unknown_exceptions.is_empty(),
        "non-ingress exceptions name slots the frozen inventory does not have: \
         {unknown_exceptions:?}"
    );
    let static_slots: BTreeSet<(&str, &str)> = statics.keys().copied().collect();
    let unknown_statics: Vec<_> = static_slots.difference(&frozen_slots).collect();
    assert!(
        unknown_statics.is_empty(),
        "authority-free ingress pins name slots the frozen inventory does not have: \n         {unknown_statics:?}"
    );
    assert!(
        wrongly_present.is_empty(),
        "non-surface slots carry an allowed set; they are proved on owner, seams and \
         disposition alone: {wrongly_present:?}"
    );
    assert!(
        missing.is_empty(),
        "{} of {} surface slots have no allowed set yet (T050 authors these next, each \
         with a basis; a member that cannot take an honest set comes back as a decision). \
         First few: {:?}",
        missing.len(),
        surface
            .iter()
            .map(|c| inventory
                .slots
                .iter()
                .filter(|(category, _)| category == c)
                .count())
            .sum::<usize>(),
        missing.iter().take(5).collect::<Vec<_>>()
    );

    // Every branch in the model must be reachable from some ingress member,
    // or the model carries a name nothing can resolve.
    let union: BTreeSet<&str> = overlay
        .values()
        .flat_map(|(allowed, _)| allowed.iter().copied())
        .collect();
    assert_eq!(
        union,
        model,
        "the union of surface allowed-sets must be all eight MODEL-SURFACE branches; \
         unreached: {:?}",
        model.difference(&union).collect::<Vec<_>>()
    );
}

/// TEST-ACTIVATION (T058, Slice 4). Dark stand-in: the name exists because
/// creating this file arms its `planned_exact` declaration. It is RED by
/// construction and kept out of the default suite by `#[ignore]`. Removing the
/// attribute without writing the body fails loudly rather than reporting a pass.
#[test]
#[ignore = "Feature 020 planned_not_executed case for TEST-ACTIVATION; remove this attribute in Slice 4 (T058) when the activation cut exists and Preventive V1 can actually be observed as the only live mode"]
fn preventive_v1_is_the_only_live_mode() {
    panic!(
        "TEST-ACTIVATION is planned_not_executed: no activation cut exists, so nothing here \
         has observed a live mode. T058 owns the body."
    );
}

/// TEST-EMBED (T058, Slice 4). Dark stand-in; see the note on
/// `preventive_v1_is_the_only_live_mode`.
#[test]
#[ignore = "Feature 020 planned_not_executed case for TEST-EMBED; remove this attribute in Slice 4 (T058) when the embedded source handle is live and a raw bypass could actually be detected"]
fn embedded_source_has_one_handle_and_no_raw_bypass() {
    panic!(
        "TEST-EMBED is planned_not_executed: the embedded handle is a dark stand-in, so \
         nothing here has observed a bypass or its absence. T058 owns the body."
    );
}

/// TEST-MUTATION (T058, Slice 4). Dark stand-in; see the note on
/// `preventive_v1_is_the_only_live_mode`.
#[test]
#[ignore = "Feature 020 planned_not_executed case for TEST-MUTATION; remove this attribute in Slice 4 (T058) when SourceMutationPermit is live and a write can be observed acquiring one"]
fn every_source_write_requires_current_mutation_permit() {
    panic!(
        "TEST-MUTATION is planned_not_executed: no write path acquires a permit yet, so \
         nothing here has observed the requirement. T058 owns the body."
    );
}

/// TEST-STATE (T058, Slice 4). Dark stand-in; see the note on
/// `preventive_v1_is_the_only_live_mode`.
#[test]
#[ignore = "Feature 020 planned_not_executed case for TEST-STATE; remove this attribute in Slice 4 (T058) when state owners are live and team-artifact exactness can be measured"]
fn state_owners_and_team_artifact_are_exact() {
    panic!(
        "TEST-STATE is planned_not_executed: state ownership is not wired, so nothing here \
         has observed exactness. T058 owns the body."
    );
}
