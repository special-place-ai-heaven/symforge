# V10 Authority Retirement Inventory V11

This is the closed V10 authority and ingress retirement inventory. Slice 4 owns
the final cut: every member below must either route through the V11 runtime or
be unreachable before Preventive V1 is exposed. Absence from this inventory is
not permission to survive. Before activation, the byte census below closes the five
authority-bearing source categories against unlisted additions. Each source blob is
decoded as UTF-8 and only CRLF pairs are normalized to LF before hashing, so an
otherwise byte-identical LF or CRLF checkout has the same census; no other byte or
Unicode normalization is permitted. After activation, the executed Slice 4
reachability cases replace the preactivation census. All retirement evidence is
planned and unexecuted.

<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON START -->
```json
{
  "entries": [
    {
      "assertions": [
        "Every repository-source byte writer obtains a current SourceMutationPermit before I/O",
        "No writer mutates a V10 SharedIndex or publishes directly",
        "Gitignore hygiene is source-authorized while ProjectStateDir and post-image team-artifact state writes remain permit-free"
      ],
      "category": "writers",
      "command": "cargo test --test activation_cut_v11 all_ingress_uses_exact_typed_authority_branch -- --exact",
      "disposition": "route_all_repository-source writes through mutation intent and V11 reconciliation; remove direct index publication",
      "executed": false,
      "members": [
        "src/cli/init.rs::run_init_with_paths",
        "src/gitignore_hygiene.rs::atomic_replace",
        "src/gitignore_hygiene.rs::reconcile_project_gitignore",
        "src/gitignore_hygiene.rs::reconcile_root_gitignore",
        "src/live_index/persist.rs::ensure_gitattributes_merge_hint",
        "src/live_index/single_file.rs::remove_file",
        "src/live_index/single_file.rs::update_file_from_disk",
        "src/protocol/edit.rs::atomic_write_file",
        "src/protocol/edit.rs::execute_batch_edit",
        "src/protocol/edit.rs::execute_batch_insert",
        "src/protocol/edit.rs::execute_batch_rename",
        "src/protocol/edit.rs::guarded_atomic_write_file",
        "src/protocol/edit_tools.rs::SymForgeServer::batch_edit",
        "src/protocol/edit_tools.rs::SymForgeServer::batch_insert",
        "src/protocol/edit_tools.rs::SymForgeServer::batch_rename",
        "src/protocol/edit_tools.rs::SymForgeServer::delete_symbol",
        "src/protocol/edit_tools.rs::SymForgeServer::edit_within_symbol",
        "src/protocol/edit_tools.rs::SymForgeServer::insert_symbol",
        "src/protocol/edit_tools.rs::SymForgeServer::replace_symbol_body",
        "src/protocol/knowledge_curation.rs::KnowledgeCurationCoordinator::apply",
        "src/protocol/knowledge_curation.rs::KnowledgeCurationCoordinator::write_policy",
        "src/protocol/knowledge_curation.rs::apply_reviewed_mutation",
        "src/protocol/knowledge_curation.rs::durable_replace",
        "src/protocol/knowledge_curation.rs::durable_replace_io",
        "src/protocol/tools.rs::SymForgeServer::curate_knowledge"
      ],
      "production_seams": [
        "src/index_lifecycle/mutation.rs::SourceMutationPermit",
        "src/index_lifecycle/runtime.rs::ProjectIndexRuntime"
      ],
      "retirement_test": "tests/activation_cut_v11.rs::all_ingress_uses_exact_typed_authority_branch",
      "slice4_owner_tasks": [
        "T064",
        "T065",
        "T067"
      ],
      "status": "planned_not_executed"
    },
    {
      "assertions": [
        "No callback holds publication authority",
        "Every callback carries current project and source incarnations",
        "Late V10 callbacks are unreachable after the activation cut"
      ],
      "category": "callbacks",
      "command": "cargo test --test activation_cut_v11 all_ingress_uses_exact_typed_authority_branch -- --exact",
      "disposition": "register callbacks with a source-slot incarnation and revoke them before tombstone retirement",
      "executed": false,
      "members": [
        "src/daemon.rs::bootstrap_project_index::background_verify spawn",
        "src/daemon.rs::spawn_local_ref_reconcile",
        "src/daemon.rs::start_project_watcher",
        "src/live_index/git_temporal.rs::spawn_git_temporal_computation",
        "src/live_index/persist.rs::background_verify",
        "src/main.rs::run_local_mcp_server_async::background_verify spawn",
        "src/main.rs::spawn_periodic_checkpoint",
        "src/protocol/edit_hooks.rs::after_commit",
        "src/protocol/edit_hooks.rs::resolve",
        "src/protocol/knowledge_curation.rs::KnowledgeCurationCoordinator::recover_on_project_load",
        "src/server/serve.rs::background_verify spawn",
        "src/watcher/mod.rs::process_events",
        "src/watcher/mod.rs::restart_watcher",
        "src/watcher/mod.rs::start_watcher"
      ],
      "production_seams": [
        "src/index_lifecycle/observer.rs::ObserverHandoff",
        "src/index_lifecycle/supervisor.rs::SourceSupervisor"
      ],
      "retirement_test": "tests/activation_cut_v11.rs::all_ingress_uses_exact_typed_authority_branch",
      "slice4_owner_tasks": [
        "T064",
        "T065",
        "T067"
      ],
      "status": "planned_not_executed"
    },
    {
      "assertions": [
        "Only ProjectPublicationRoot is query-visible",
        "No bare SharedIndex is stored in daemon, protocol, server, sidecar, or embed state",
        "Partial source generations cannot be published"
      ],
      "category": "publication_roots",
      "command": "cargo test --test activation_cut_v11 all_ingress_uses_exact_typed_authority_branch -- --exact",
      "disposition": "replace every V10 root with the sole immutable whole-project publication root",
      "executed": false,
      "members": [
        "src/daemon.rs::ProjectInstance::index",
        "src/daemon.rs::SessionRuntime::index",
        "src/daemon.rs::SessionRuntime::project_indexes",
        "src/live_index/store.rs::SharedIndexHandle",
        "src/live_index/store.rs::SharedIndexHandle::reload",
        "src/live_index/store.rs::SharedIndexHandle::reload_for_state_placement",
        "src/protocol/mod.rs::SymForgeServer::index",
        "src/server/mod.rs::ServerRuntime::index",
        "src/sidecar/mod.rs::SidecarState::index"
      ],
      "production_seams": [
        "src/index_lifecycle/runtime.rs::ProjectIndexRuntime",
        "src/index_lifecycle/runtime.rs::ProjectPublicationRoot"
      ],
      "retirement_test": "tests/activation_cut_v11.rs::all_ingress_uses_exact_typed_authority_branch",
      "slice4_owner_tasks": [
        "T066",
        "T067"
      ],
      "status": "planned_not_executed"
    },
    {
      "assertions": [
        "A cache miss never falls back to a V10 authority",
        "A cache hit is fenced to the leased publication",
        "Retarget and publication swap invalidate stale cache entries deterministically"
      ],
      "category": "cache",
      "command": "cargo test --test activation_cut_v11 all_ingress_uses_exact_typed_authority_branch -- --exact",
      "disposition": "make caches generation-keyed, non-authoritative projections of a pinned V11 publication or remove them",
      "executed": false,
      "members": [
        "src/daemon.rs::DaemonState::bases",
        "src/daemon.rs::ProjectInstance::symbol_cache",
        "src/daemon.rs::SessionRecord::working_set",
        "src/daemon.rs::SessionRuntime::symbol_cache",
        "src/daemon.rs::SessionRuntime::working_set",
        "src/protocol/knowledge_curation.rs::KnowledgeCurationCoordinator::probe_cache",
        "src/protocol/session.rs::SessionInner::detailed_fetches",
        "src/sidecar/mod.rs::SidecarState::symbol_cache",
        "src/worktree.rs::WorktreeCache"
      ],
      "production_seams": [
        "src/index_lifecycle/query.rs::ProjectQueryLease",
        "src/index_lifecycle/runtime.rs::ProjectPublicationRoot"
      ],
      "retirement_test": "tests/activation_cut_v11.rs::all_ingress_uses_exact_typed_authority_branch",
      "slice4_owner_tasks": [
        "T066",
        "T067"
      ],
      "status": "planned_not_executed"
    },
    {
      "assertions": [
        "CCR cannot originate truth or extend a lease",
        "CCR handles encode the source publication identity",
        "Evicted or foreign generations return typed unavailability"
      ],
      "category": "ccr",
      "command": "cargo test --test activation_cut_v11 all_ingress_uses_exact_typed_authority_branch -- --exact",
      "disposition": "retain CCR only as a generation-bound rendering cache downstream of a V11 query lease",
      "executed": false,
      "members": [
        "src/protocol/ccr.rs::CcrStore",
        "src/protocol/ccr.rs::apply_ccr_overflow",
        "src/protocol/ccr.rs::enforce_token_budget_with_ccr",
        "src/protocol/ccr.rs::rewrite_footer_for_symforge_facade"
      ],
      "production_seams": [
        "src/index_lifecycle/query.rs::ProjectQueryLease",
        "src/protocol/read_gate.rs::ReadGate"
      ],
      "retirement_test": "tests/activation_cut_v11.rs::all_ingress_uses_exact_typed_authority_branch",
      "slice4_owner_tasks": [
        "T066",
        "T067"
      ],
      "status": "planned_not_executed"
    },
    {
      "assertions": [
        "Snapshot load never publishes directly",
        "Compatibility and source identity are re-proved before promotion",
        "Protected placement first selects deterministic user-local state fallback",
        "Only memory-only placement or failed user-local fallback produces typed checkpoint unavailability"
      ],
      "category": "snapshot",
      "command": "cargo test --test activation_cut_v11 all_ingress_uses_exact_typed_authority_branch -- --exact",
      "disposition": "demote restore to private candidate seed and route checkpointing through the typed V11 state owner with protected-root user-local fallback",
      "executed": false,
      "members": [
        "src/live_index/persist.rs::IndexSnapshot",
        "src/live_index/persist.rs::background_verify",
        "src/live_index/persist.rs::checkpoint_shared_index",
        "src/live_index/persist.rs::export_artifact",
        "src/live_index/persist.rs::import_portable_snapshot",
        "src/live_index/persist.rs::load_snapshot",
        "src/live_index/persist.rs::load_snapshot_for_root",
        "src/live_index/persist.rs::project_local_state_placement",
        "src/live_index/persist.rs::reset_snapshot_state",
        "src/live_index/persist.rs::serialize_shared_index",
        "src/live_index/persist.rs::snapshot_compatible",
        "src/live_index/persist.rs::snapshot_to_live_index",
        "src/live_index/persist.rs::snapshot_to_live_index_with_code_signals"
      ],
      "production_seams": [
        "src/index_lifecycle/verification.rs::VerificationRecord",
        "src/live_index/persist.rs::IndexSnapshot"
      ],
      "retirement_test": "tests/activation_cut_v11.rs::all_ingress_uses_exact_typed_authority_branch",
      "slice4_owner_tasks": [
        "T065",
        "T067"
      ],
      "status": "planned_not_executed"
    },
    {
      "assertions": [
        "Every source-derived union member selects exactly one of GenerationLeased, DiskObserved, WorktreeScopeObserved, GitObserved, RuntimeHealthObserved, MutationPermitted, StateWriteAuthorized, or Refused",
        "Full contains exactly 39 tools, Compact contains exactly status, symforge, and symforge_edit, and their unique union contains exactly 40",
        "Only GenerationLeased acquires a ProjectQueryLease and only repository-source MutationPermitted operations acquire a SourceMutationPermit",
        "RuntimeHealthObserved keeps committed-generation fields separate from bounded attempt and runtime-work fields",
        "No tool retains direct V10 index access",
        "The compact facade does not weaken authority checks"
      ],
      "category": "tools",
      "command": "cargo test --test activation_cut_v11 all_ingress_uses_exact_typed_authority_branch -- --exact",
      "disposition": "route the source-derived full-39 and compact-3 profiles, whose unique union is 40 tools, through exactly one typed V11 authority branch",
      "executed": false,
      "members": [
        "analyze_file_impact",
        "ask",
        "batch_edit",
        "batch_insert",
        "batch_rename",
        "checkpoint_now",
        "context_inventory",
        "conventions",
        "curate_knowledge",
        "delete_symbol",
        "detect_impact",
        "diff_symbols",
        "edit_plan",
        "edit_within_symbol",
        "explore",
        "find_dependents",
        "find_references",
        "get_file_content",
        "get_file_context",
        "get_repo_map",
        "get_symbol",
        "get_symbol_context",
        "health",
        "health_compact",
        "index_folder",
        "insert_symbol",
        "inspect_match",
        "investigation_suggest",
        "replace_symbol_body",
        "review_knowledge",
        "search_files",
        "search_knowledge",
        "search_symbols",
        "search_text",
        "status",
        "symforge",
        "symforge_edit",
        "symforge_retrieve",
        "validate_file_syntax",
        "what_changed"
      ],
      "production_seams": [
        "src/index_lifecycle/mutation.rs::SourceMutationPermit",
        "src/index_lifecycle/public_api.rs::V11PublicApi",
        "src/index_lifecycle/query.rs::ProjectQueryLease"
      ],
      "retirement_test": "tests/activation_cut_v11.rs::all_ingress_uses_exact_typed_authority_branch",
      "slice4_owner_tasks": [
        "T066",
        "T067"
      ],
      "status": "planned_not_executed"
    },
    {
      "assertions": [
        "Generation-backed resources use GenerationLeased and pin one V11 publication",
        "Pure disk, worktree-scope, git, and runtime-health resources use their lease-free observed branches with typed provenance",
        "RuntimeHealthObserved resources cannot mix attempt-only fields into committed-generation truth",
        "Static catalog resources cannot disclose raw runtime state",
        "Template expansion preserves the selected branch and never upgrades an observation into Current"
      ],
      "category": "resources",
      "command": "cargo test --test activation_cut_v11 all_ingress_uses_exact_typed_authority_branch -- --exact",
      "disposition": "classify every static resource and resource template into its exact V11 generation, disk, worktree, git, state, or refusal branch",
      "executed": false,
      "members": [
        "symforge://file/content",
        "symforge://file/context",
        "symforge://glossary",
        "symforge://repo/changes/uncommitted",
        "symforge://repo/health",
        "symforge://repo/map",
        "symforge://repo/outline",
        "symforge://symbol/context",
        "symforge://symbol/detail",
        "symforge://tools/catalog"
      ],
      "production_seams": [
        "src/index_lifecycle/query.rs::ProjectQueryLease",
        "src/protocol/read_gate.rs::ReadGate"
      ],
      "retirement_test": "tests/activation_cut_v11.rs::all_ingress_uses_exact_typed_authority_branch",
      "slice4_owner_tasks": [
        "T066",
        "T067"
      ],
      "status": "planned_not_executed"
    },
    {
      "assertions": [
        "Generation-backed prompt context uses GenerationLeased while pure observation context remains lease-free",
        "Prompt aliases cannot reach V10 caches or upgrade observations into Current",
        "Unavailable context selects Refused rather than silently empty success"
      ],
      "category": "prompts",
      "command": "cargo test --test activation_cut_v11 all_ingress_uses_exact_typed_authority_branch -- --exact",
      "disposition": "route prompt context through the exact typed V11 branch; static prompt text carries no publication authority",
      "executed": false,
      "members": [
        "symforge-admin",
        "symforge-architecture",
        "symforge-debug",
        "symforge-knowledge-hygiene",
        "symforge-onboard",
        "symforge-refactor",
        "symforge-review",
        "symforge-triage"
      ],
      "production_seams": [
        "src/index_lifecycle/query.rs::ProjectQueryLease",
        "src/protocol/read_gate.rs::ReadGate"
      ],
      "retirement_test": "tests/activation_cut_v11.rs::all_ingress_uses_exact_typed_authority_branch",
      "slice4_owner_tasks": [
        "T066",
        "T067"
      ],
      "status": "planned_not_executed"
    },
    {
      "assertions": [
        "Generation-backed endpoints use GenerationLeased; disk, worktree-scope, git, runtime-health, mutation, and state endpoints use only their matching typed branch",
        "RuntimeHealthObserved endpoints separate committed-generation fields from attempt and work state",
        "Caller-root mismatch selects Refused and cannot fall through to V10",
        "Workflow aliases are thin aliases over the same branch selector"
      ],
      "category": "sidecar",
      "command": "cargo test --test activation_cut_v11 all_ingress_uses_exact_typed_authority_branch -- --exact",
      "disposition": "route standalone and daemon-proxied sidecar endpoints through the same typed V11 branch selector and root guard",
      "executed": false,
      "members": [
        "GET /health",
        "GET /impact",
        "GET /outline",
        "GET /prompt-context",
        "GET /repo-map",
        "GET /stats",
        "GET /symbol-context",
        "GET /v1/sessions/{session_id}/sidecar/health",
        "GET /v1/sessions/{session_id}/sidecar/impact",
        "GET /v1/sessions/{session_id}/sidecar/outline",
        "GET /v1/sessions/{session_id}/sidecar/prompt-context",
        "GET /v1/sessions/{session_id}/sidecar/repo-map",
        "GET /v1/sessions/{session_id}/sidecar/stats",
        "GET /v1/sessions/{session_id}/sidecar/symbol-context",
        "GET /v1/sessions/{session_id}/sidecar/workflows/post-edit-impact",
        "GET /v1/sessions/{session_id}/sidecar/workflows/prompt-context",
        "GET /v1/sessions/{session_id}/sidecar/workflows/repo-start",
        "GET /v1/sessions/{session_id}/sidecar/workflows/search-hit-expansion",
        "GET /v1/sessions/{session_id}/sidecar/workflows/source-read",
        "GET /workflows/post-edit-impact",
        "GET /workflows/prompt-context",
        "GET /workflows/repo-start",
        "GET /workflows/search-hit-expansion",
        "GET /workflows/source-read"
      ],
      "production_seams": [
        "src/index_lifecycle/public_api.rs::V11PublicApi",
        "src/index_lifecycle/query.rs::ProjectQueryLease"
      ],
      "retirement_test": "tests/activation_cut_v11.rs::all_ingress_uses_exact_typed_authority_branch",
      "slice4_owner_tasks": [
        "T064",
        "T066",
        "T067"
      ],
      "status": "planned_not_executed"
    },
    {
      "assertions": [
        "Read, Grep, SessionStart, PromptSubmit, and PreTool select GenerationLeased only for generation-backed context and otherwise retain their exact disk, worktree, git, runtime-health, state, or refusal branch",
        "Edit and Write notifications cannot publish, mint a SourceMutationPermit, or bypass mutation authority",
        "Fallback output carries no false Current claim and cannot upgrade a pure observation"
      ],
      "category": "hooks",
      "command": "cargo test --test activation_cut_v11 all_ingress_uses_exact_typed_authority_branch -- --exact",
      "disposition": "route all seven hook classes through root-bound typed V11 sidecar or daemon ingress; fail open only without claiming Current evidence",
      "executed": false,
      "members": [
        "hook:Edit",
        "hook:Grep",
        "hook:PreTool",
        "hook:PromptSubmit",
        "hook:Read",
        "hook:SessionStart",
        "hook:Write"
      ],
      "production_seams": [
        "src/index_lifecycle/activation.rs::ActivationCut",
        "src/index_lifecycle/mutation.rs::SourceMutationPermit",
        "src/index_lifecycle/query.rs::ProjectQueryLease"
      ],
      "retirement_test": "tests/activation_cut_v11.rs::all_ingress_uses_exact_typed_authority_branch",
      "slice4_owner_tasks": [
        "T064",
        "T066",
        "T067"
      ],
      "status": "planned_not_executed"
    },
    {
      "assertions": [
        "Neither alias is advertised as an additional tool",
        "detect_changes delegates to detect_impact and returns GitObserved for committed-ref diffs or WorktreeScopeObserved for worktree diffs",
        "detect_changes never acquires a ProjectQueryLease or upgrades observation evidence to GenerationLeased",
        "trace_symbol cannot reach V10 symbol caches and uses GenerationLeased only for a complete Current publication"
      ],
      "category": "compatibility_aliases",
      "command": "cargo test --test activation_cut_v11 all_ingress_uses_exact_typed_authority_branch -- --exact",
      "disposition": "route trace_symbol through V11 generation authority and route detect_changes to detect_impact as typed Git/worktree observation, or retire either alias",
      "executed": false,
      "members": [
        "detect_changes",
        "trace_symbol"
      ],
      "production_seams": [
        "src/index_lifecycle/activation.rs::ActivationCut",
        "src/index_lifecycle/public_api.rs::V11PublicApi"
      ],
      "retirement_test": "tests/activation_cut_v11.rs::all_ingress_uses_exact_typed_authority_branch",
      "slice4_owner_tasks": [
        "T066",
        "T067"
      ],
      "status": "planned_not_executed"
    },
    {
      "assertions": [
        "The member set exactly equals all remove/replace V10 migration atoms",
        "No forbidden raw module, state, parser, snapshot, search, mutation, STEL, Git, or deep re-export remains public",
        "The observed public graph equals the frozen V11 graph in every supported configuration cell"
      ],
      "category": "raw_embed",
      "command": "cargo test --test activation_cut_v11 all_ingress_uses_exact_typed_authority_branch -- --exact",
      "disposition": "remove or replace exactly as frozen by every migration_v10 category whose decision is remove or replace",
      "executed": false,
      "members": [
        "symforge::analytics",
        "symforge::capability",
        "symforge::cli",
        "symforge::daemon",
        "symforge::discovery",
        "symforge::domain",
        "symforge::edit_safety",
        "symforge::embed::AdmissionDecision",
        "symforge::embed::CalibrationVerdict",
        "symforge::embed::FileClassification",
        "symforge::embed::FileProcessingResult",
        "symforge::embed::GitRepo",
        "symforge::embed::IndexLoadSource",
        "symforge::embed::IndexSnapshot",
        "symforge::embed::IndexedFile",
        "symforge::embed::IntentBucket",
        "symforge::embed::LanguageId",
        "symforge::embed::LedgerStoreStatus",
        "symforge::embed::LedgerSubsystemState",
        "symforge::embed::LedgerSummary",
        "symforge::embed::LiveIndex",
        "symforge::embed::ParseStatus",
        "symforge::embed::PortableSnapshotProvenance",
        "symforge::embed::PublishedIndexState",
        "symforge::embed::PublishedIndexStatus",
        "symforge::embed::ReferenceKind",
        "symforge::embed::ReindexResult",
        "symforge::embed::RouteConfidence",
        "symforge::embed::SearchFilesTier",
        "symforge::embed::SearchFilesView",
        "symforge::embed::SharedIndex",
        "symforge::embed::SnapshotVerifyState",
        "symforge::embed::StatePlacement",
        "symforge::embed::StelCalibrationSummary",
        "symforge::embed::StelLedgerEvent",
        "symforge::embed::StelLedgerStore",
        "symforge::embed::StoredLedgerRecord",
        "symforge::embed::SymbolKind",
        "symforge::embed::SymbolRecord",
        "symforge::embed::SymbolSearchResult",
        "symforge::embed::TextSearchError",
        "symforge::embed::TextSearchResult",
        "symforge::embed::domain",
        "symforge::embed::format_calibration_section",
        "symforge::embed::git",
        "symforge::embed::import_portable_snapshot",
        "symforge::embed::live_index",
        "symforge::embed::load_snapshot",
        "symforge::embed::load_snapshot_for_root",
        "symforge::embed::parsing",
        "symforge::embed::process_file",
        "symforge::embed::project_local_state_placement",
        "symforge::embed::remove_file",
        "symforge::embed::search_symbols",
        "symforge::embed::search_text",
        "symforge::embed::snapshot_compatible",
        "symforge::embed::snapshot_to_live_index",
        "symforge::embed::summarize_calibration",
        "symforge::embed::update_file_from_disk",
        "symforge::git",
        "symforge::gitignore_hygiene",
        "symforge::hash",
        "symforge::idempotency",
        "symforge::knowledge",
        "symforge::live_index",
        "symforge::observability",
        "symforge::parsing",
        "symforge::path_shadow",
        "symforge::paths",
        "symforge::process_util",
        "symforge::protocol",
        "symforge::server",
        "symforge::sidecar",
        "symforge::stel",
        "symforge::stel_core",
        "symforge::version_registry",
        "symforge::watcher",
        "symforge::watcher_state",
        "symforge::worktree"
      ],
      "production_seams": [
        "src/index_lifecycle/embedded.rs::EmbeddedSourceHandle",
        "src/index_lifecycle/process_runtime.rs::ProcessIndexRuntime",
        "src/index_lifecycle/public_api.rs::V11PublicApi"
      ],
      "retirement_test": "tests/activation_cut_v11.rs::all_ingress_uses_exact_typed_authority_branch",
      "slice4_owner_tasks": [
        "T067"
      ],
      "status": "planned_not_executed"
    }
  ],
  "kind": "symforge.v10_authority_retirement.v11",
  "preactivation_closure": {
    "cache": {
      "digest": "0cc3a6fd161a93b764e2db0e8528f61cedb50ac31caeff4289231bcc0f1eb63b",
      "paths": [
        "src/daemon.rs",
        "src/protocol/knowledge_curation.rs",
        "src/protocol/session.rs",
        "src/sidecar/mod.rs",
        "src/worktree.rs"
      ]
    },
    "callbacks": {
      "digest": "ee553c22236eb8a1d641d3b60a1573b93cba72b955f80c854136f41166f0e0c1",
      "paths": [
        "src/daemon.rs",
        "src/live_index/git_temporal.rs",
        "src/live_index/persist.rs",
        "src/main.rs",
        "src/protocol/edit_hooks.rs",
        "src/protocol/knowledge_curation.rs",
        "src/server/serve.rs",
        "src/watcher/mod.rs"
      ]
    },
    "ccr": {
      "digest": "8ad77748b8fd9e6eb31853cc9615730fc632a890898321deb915546e384ad246",
      "paths": [
        "src/protocol/ccr.rs"
      ]
    },
    "publication_roots": {
      "digest": "e37555add0073b6bafb1e023591d2b1ca623698785ad198f2d9b793ab761e82d",
      "paths": [
        "src/daemon.rs",
        "src/live_index/store.rs",
        "src/protocol/mod.rs",
        "src/server/mod.rs",
        "src/sidecar/mod.rs"
      ]
    },
    "writers": {
      "digest": "1085e716eff8e852d638466d34d63848a6444493a12af5d80480a7ae92b8e4a4",
      "paths": [
        "src/cli/init.rs",
        "src/gitignore_hygiene.rs",
        "src/live_index/persist.rs",
        "src/live_index/single_file.rs",
        "src/protocol/edit.rs",
        "src/protocol/edit_tools.rs",
        "src/protocol/knowledge_curation.rs",
        "src/protocol/tools.rs"
      ]
    }
  },
  "schema_version": 1,
  "slice4_owner": {
    "slice": 4,
    "tasks": [
      "T064",
      "T065",
      "T066",
      "T067"
    ]
  },
  "status": "planned_not_executed"
}
```
<!-- SYMFORGE V10 AUTHORITY RETIREMENT V11 JSON END -->
