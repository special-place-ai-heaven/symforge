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
//! ALLOWED SET, NOT A SINGLETON. Per source-derived call exactly one branch
//! resolves; per member the matrix records the closed set of branches that
//! source-derived call may take.
//! `detect_changes` is the existence proof — it may resolve `GitObserved` or
//! `WorktreeScopeObserved` and must never resolve `GenerationLeased` — so a
//! singleton column could not describe it without lying.
//!
//! The inventory is PARSED at test time, never transcribed: copying 244 member
//! strings into this file would create a second inventory that drifts from the
//! frozen one silently. Only the thirteen entry-level shapes are pinned here,
//! so a change to an owner set or a production seam fails loudly.
//!
//! WHAT THIS TEST'S GREEN DOES NOT PROVE. It proves the join is bijective over
//! the 244 frozen slots, that every surface member is pinned in exactly one of
//! the three lists with a non-empty basis, that no allowed set names a branch
//! outside `MODEL-SURFACE`, and that the union of the surface sets is all
//! eight. It does NOT prove any individual member's set is exactly right:
//! dropping `Refused` from `symforge_edit` leaves the suite green, because the
//! union still closes on other rows. Per-member correctness rests on the basis
//! strings and on review, which is why every row cites a frozen assertion, an
//! `INV-*`/`FR-*` id, or a call site rather than asserting a branch bare.
//!
//! STATED TARGET RESIDUAL — `INV-SURFACE` vs `AUTHORITY_FREE_INGRESS`. Eleven
//! members are assigned no typed authority branch in the target model. Current
//! V10 violations named in their row bases—most notably unfenced project
//! evidence on static resources—are deferred implementation gaps, not reasons
//! to bless a false branch. The target assignment falsifies "every ingress
//! resolves exactly one typed authority branch" as written, and it is recorded
//! rather than papered over: Slice 4 (T066) must either exclude these from the
//! invariant or add a branch for them. Frozen prompts
//! assertions 1 and 3 are part of the same residual — they govern how
//! generation-backed prompt context is selected WHEN a prompt fetches it, and
//! no V10 prompt fetches.
//!
//! STATED RESIDUAL — identical idempotent edit replay. Eight edit-tool
//! ingresses can return a stored successful response after source preparation
//! but before edit dispatch or permit acquisition. `ReplayRecord` v1 does not
//! bind that fresh observation—or any typed authority receipt—to the stored
//! response, so none of the eight branches honestly describes the terminal
//! zero-write mode. The exact eight tool rows and seven
//! duplicate writer rows are pinned below; T058 owns the causal RED, T064 the
//! source-bound replay receipt, T066 branch registration, and T072 activation.
//!
//! STATED RESIDUAL — source-free successful semantic-result modes. Sixteen
//! modes on otherwise branch-bearing members return their body from arguments,
//! static guidance, or an ungrounded plan before source-authority selection.
//! Tool wire responses may still carry ancillary untyped `ProjectEvidence`
//! `_meta`, the already-recorded D16 activation gap, but that is not a typed
//! Current branch. The exact mode triples, estimate disposition, and owners are
//! pinned below. No ninth branch is invented here; T066/T067 must resolve the
//! advertised tool modes, with T064 additionally owning the hook pass-through
//! and T072 the activation boundary.

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
/// below requires every surface slot to be in EXACTLY ONE of this list, the
/// overlay, or `AUTHORITY_FREE_INGRESS`, so a member cannot be quietly dropped
/// from all three.
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

/// Target assignment for ingress whose semantic result carries NO
/// source-authority branch: it runs, it succeeds, and it is specified to pin no
/// publication and observe no source. Current V10 boundary violations named in
/// row bases are deferred implementation gaps, not authority branches to
/// bless. A third kind, forced by the tree rather than chosen — overlaying any
/// of the eight would lie about the target result, and exempting these as
/// non-ingress would lie about what they are.
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
        "prompts",
        "symforge-admin",
        "V10 does not fetch: the handler emits instruction text and attaches `PromptMessage::new_resource_link` URIs for the CLIENT to read. A resource link is not a `resources/read` inside `get_prompt`, and the linked resources carry their own overlay rows - leasing the prompt for naming a URI would tell Slice 4 to take a ProjectQueryLease in a handler that never fetches, and would double-count the resource. No handler reaches `self.index` or a published generation. Frozen prompts assertions 1 and 3 govern how generation-backed CONTEXT is selected when a prompt fetches it; the half that matches this tree is that static prompt text carries no publication authority, so 1 and 3 are a stated residual for T066 rather than a lease that does not happen. This one also probes `resolve_running_dashboard_url` off the runtime (prompts.rs:284) - operator-server LIVENESS, the `hook:PreTool` shape, not INV-HEALTH's committed-vs-attempt fields, so not RuntimeHealthObserved.",
    ),
    (
        "prompts",
        "symforge-architecture",
        "V10 does not fetch: the handler emits instruction text and attaches `PromptMessage::new_resource_link` URIs for the CLIENT to read. A resource link is not a `resources/read` inside `get_prompt`, and the linked resources carry their own overlay rows - leasing the prompt for naming a URI would tell Slice 4 to take a ProjectQueryLease in a handler that never fetches, and would double-count the resource. No handler reaches `self.index` or a published generation. Frozen prompts assertions 1 and 3 govern how generation-backed CONTEXT is selected when a prompt fetches it; the half that matches this tree is that static prompt text carries no publication authority, so 1 and 3 are a stated residual for T066 rather than a lease that does not happen.",
    ),
    (
        "prompts",
        "symforge-debug",
        "V10 does not fetch: the handler emits instruction text and attaches `PromptMessage::new_resource_link` URIs for the CLIENT to read. A resource link is not a `resources/read` inside `get_prompt`, and the linked resources carry their own overlay rows - leasing the prompt for naming a URI would tell Slice 4 to take a ProjectQueryLease in a handler that never fetches, and would double-count the resource. No handler reaches `self.index` or a published generation. Frozen prompts assertions 1 and 3 govern how generation-backed CONTEXT is selected when a prompt fetches it; the half that matches this tree is that static prompt text carries no publication authority, so 1 and 3 are a stated residual for T066 rather than a lease that does not happen.",
    ),
    (
        "prompts",
        "symforge-knowledge-hygiene",
        "V10 does not fetch: the handler emits instruction text and attaches `PromptMessage::new_resource_link` URIs for the CLIENT to read. A resource link is not a `resources/read` inside `get_prompt`, and the linked resources carry their own overlay rows - leasing the prompt for naming a URI would tell Slice 4 to take a ProjectQueryLease in a handler that never fetches, and would double-count the resource. No handler reaches `self.index` or a published generation. Frozen prompts assertions 1 and 3 govern how generation-backed CONTEXT is selected when a prompt fetches it; the half that matches this tree is that static prompt text carries no publication authority, so 1 and 3 are a stated residual for T066 rather than a lease that does not happen. Its `guard_query` on `path_prefix` is INPUT SAFETY, not assertion 3's unavailable-context selection, so it takes no Refused.",
    ),
    (
        "prompts",
        "symforge-onboard",
        "V10 does not fetch: the handler emits instruction text and attaches `PromptMessage::new_resource_link` URIs for the CLIENT to read. A resource link is not a `resources/read` inside `get_prompt`, and the linked resources carry their own overlay rows - leasing the prompt for naming a URI would tell Slice 4 to take a ProjectQueryLease in a handler that never fetches, and would double-count the resource. No handler reaches `self.index` or a published generation. Frozen prompts assertions 1 and 3 govern how generation-backed CONTEXT is selected when a prompt fetches it; the half that matches this tree is that static prompt text carries no publication authority, so 1 and 3 are a stated residual for T066 rather than a lease that does not happen.",
    ),
    (
        "prompts",
        "symforge-refactor",
        "V10 does not fetch: the handler emits instruction text and attaches `PromptMessage::new_resource_link` URIs for the CLIENT to read. A resource link is not a `resources/read` inside `get_prompt`, and the linked resources carry their own overlay rows - leasing the prompt for naming a URI would tell Slice 4 to take a ProjectQueryLease in a handler that never fetches, and would double-count the resource. No handler reaches `self.index` or a published generation. Frozen prompts assertions 1 and 3 govern how generation-backed CONTEXT is selected when a prompt fetches it; the half that matches this tree is that static prompt text carries no publication authority, so 1 and 3 are a stated residual for T066 rather than a lease that does not happen.",
    ),
    (
        "prompts",
        "symforge-review",
        "V10 does not fetch: the handler emits instruction text and attaches `PromptMessage::new_resource_link` URIs for the CLIENT to read. A resource link is not a `resources/read` inside `get_prompt`, and the linked resources carry their own overlay rows - leasing the prompt for naming a URI would tell Slice 4 to take a ProjectQueryLease in a handler that never fetches, and would double-count the resource. No handler reaches `self.index` or a published generation. Frozen prompts assertions 1 and 3 govern how generation-backed CONTEXT is selected when a prompt fetches it; the half that matches this tree is that static prompt text carries no publication authority, so 1 and 3 are a stated residual for T066 rather than a lease that does not happen.",
    ),
    (
        "prompts",
        "symforge-triage",
        "V10 does not fetch: the handler emits instruction text and attaches `PromptMessage::new_resource_link` URIs for the CLIENT to read. A resource link is not a `resources/read` inside `get_prompt`, and the linked resources carry their own overlay rows - leasing the prompt for naming a URI would tell Slice 4 to take a ProjectQueryLease in a handler that never fetches, and would double-count the resource. No handler reaches `self.index` or a published generation. Frozen prompts assertions 1 and 3 govern how generation-backed CONTEXT is selected when a prompt fetches it; the half that matches this tree is that static prompt text carries no publication authority, so 1 and 3 are a stated residual for T066 rather than a lease that does not happen.",
    ),
    (
        "resources",
        "symforge://glossary",
        "`render_glossary` is static markdown with no index access. It SUCCEEDS, so Refused would \
         misreport the outcome; it pins no publication, so GenerationLeased would be a false \
         Current; and resources assertion 4 forbids static catalogs from disclosing raw runtime \
         state. Current V10 nevertheless attaches `local_project_evidence` at resources/read and \
         leaks publication/runtime fields without a lease (protocol/mod.rs:1748-1759; \
         tools.rs:7546-7571; result_status.rs:53-68). T066/T067 must remove or replace that boundary \
         metadata; adding GenerationLeased here would bless the unfenced leak.",
    ),
    (
        "resources",
        "symforge://tools/catalog",
        "`render_tool_catalog` walks `tool_catalog_groups()` — the advertised surface, not runtime \
         state, which resources assertion 4 forbids disclosing. Same target shape as glossary: a \
         succeeding ingress with no publication to lease and no source observed. Current V10's \
         resources/read wrapper still injects unfenced `local_project_evidence`; T066/T067 must \
         remove or replace that boundary metadata rather than relabel this static catalog Current.",
    ),
];

/// Successful identical-key edit replays prepare source first, then return the
/// stored response before edit dispatch or permit acquisition. The fresh
/// observation is not bound to that stored response. The optional second field
/// pins the duplicate writer member for the seven granular handlers;
/// `symforge_edit` has no separate writer slot in the frozen inventory.
const EDIT_REPLAY_AUTHORITY_RESIDUAL: &[(&str, Option<&str>)] = &[
    (
        "batch_edit",
        Some("src/protocol/edit_tools.rs::SymForgeServer::batch_edit"),
    ),
    (
        "batch_insert",
        Some("src/protocol/edit_tools.rs::SymForgeServer::batch_insert"),
    ),
    (
        "batch_rename",
        Some("src/protocol/edit_tools.rs::SymForgeServer::batch_rename"),
    ),
    (
        "delete_symbol",
        Some("src/protocol/edit_tools.rs::SymForgeServer::delete_symbol"),
    ),
    (
        "edit_within_symbol",
        Some("src/protocol/edit_tools.rs::SymForgeServer::edit_within_symbol"),
    ),
    (
        "insert_symbol",
        Some("src/protocol/edit_tools.rs::SymForgeServer::insert_symbol"),
    ),
    (
        "replace_symbol_body",
        Some("src/protocol/edit_tools.rs::SymForgeServer::replace_symbol_body"),
    ),
    ("symforge_edit", None),
];
const EDIT_REPLAY_AUTHORITY_RESIDUAL_OWNERS: &[&str] = &["T058", "T064", "T066", "T072"];

/// Successful source-free semantic-result modes on members that also have
/// source-derived overlay rows. Ancillary untyped wire `_meta` remains D16;
/// these are mode residuals, not whole-member exemptions.
const AUTHORITY_FREE_MODE_RESIDUAL: &[(&str, &str, &str, &str)] = &[
    (
        "hooks",
        "hook:Read",
        "pass_through.non_source_read",
        "Non-source Read subcommands select PassThrough and return fail-open success before endpoint or source access (hook.rs:291-315, :811-820, :854-869).",
    ),
    (
        "tools",
        "analyze_file_impact",
        "estimate.args_only",
        "SymForgeServer::analyze_file_impact returns an argument-derived estimate before source selection (src/protocol/tools.rs).",
    ),
    (
        "tools",
        "search_text",
        "estimate.args_only",
        "SymForgeServer::search_text returns an argument-derived estimate before source selection (src/protocol/tools.rs).",
    ),
    (
        "tools",
        "inspect_match",
        "estimate.args_only",
        "SymForgeServer::inspect_match returns an argument-derived estimate before source selection (src/protocol/tools.rs).",
    ),
    (
        "tools",
        "search_files",
        "estimate.args_only",
        "SymForgeServer::search_files returns an argument-derived estimate before source selection (src/protocol/tools.rs).",
    ),
    (
        "tools",
        "what_changed",
        "estimate.args_only",
        "SymForgeServer::what_changed returns an argument-derived estimate before source selection (src/protocol/tools.rs).",
    ),
    (
        "tools",
        "explore",
        "estimate.args_only",
        "SymForgeServer::explore returns an argument-derived estimate before source selection (src/protocol/tools.rs).",
    ),
    (
        "tools",
        "diff_symbols",
        "estimate.args_only",
        "SymForgeServer::diff_symbols returns an argument-derived estimate before source selection (src/protocol/tools.rs).",
    ),
    (
        "tools",
        "ask",
        "tool_help.static_catalog",
        "SymForgeServer::ask routes smart_query::QueryIntent::ToolHelp to static catalog guidance without repository source (src/protocol/tools.rs; src/protocol/smart_query.rs).",
    ),
    (
        "tools",
        "symforge",
        "probe_relay.search_text.estimate.args_only",
        "SymForgeServer::facade_probe_is_measurement_safe exposes SymForgeServer::search_text's pre-authority estimate mode through the source-mutation-safe A-019 relay (src/protocol/tools.rs).",
    ),
    (
        "tools",
        "symforge",
        "probe_relay.search_files.estimate.args_only",
        "SymForgeServer::facade_probe_is_measurement_safe exposes SymForgeServer::search_files's pre-authority estimate mode through the source-mutation-safe A-019 relay (src/protocol/tools.rs).",
    ),
    (
        "tools",
        "symforge",
        "preview.pff_bypass.plan_floor",
        "Preview can return a PFF bypass estimate from an ungrounded plan whose steps carry no IndexRefs (tools.rs:10844-10982; controller.rs:108-185, :581-597).",
    ),
    (
        "tools",
        "symforge",
        "preview.plan_floor",
        "Preview can return the ungrounded plan-floor estimate before repository source is selected (tools.rs:10959-10982; controller.rs:319-475).",
    ),
    (
        "tools",
        "symforge",
        "pff_bypass.plan_floor",
        "A non-preview PFF bypass can return from an ungrounded plan before repository source is selected (tools.rs:10985-11018; controller.rs:108-185, :581-597; executor.rs:34-106).",
    ),
    (
        "tools",
        "symforge",
        "economics_bypass.plan_floor",
        "A non-preview economics bypass can return from an ungrounded plan before repository source is selected (tools.rs:10985-11018; controller.rs:254-267, :349-475; executor.rs:34-106).",
    ),
    (
        "tools",
        "symforge",
        "serve.ask_tool_help.static_catalog",
        "The facade can serve ask ToolHelp's static catalog without repository source (tools.rs:11039-11083, :12190; smart_query.rs:598-622, :780-932).",
    ),
];

const AUTHORITY_FREE_MODE_RESIDUAL_OWNERS: &[(&str, &[&str])] = &[
    ("hooks", &["T064", "T066", "T067", "T072"]),
    ("tools", &["T066", "T067", "T072"]),
];

/// Stable source tokens behind the tool-mode residual. Line numbers drift as
/// repairs land; these symbol anchors fail if the reviewed implementation seam
/// is renamed or removed, while the basis strings above remain human-readable.
const AUTHORITY_FREE_MODE_SOURCE_ANCHORS: &[(&str, &str)] = &[
    (
        "analyze_file_impact",
        "pub(crate) async fn analyze_file_impact",
    ),
    ("search_text", "pub(crate) async fn search_text"),
    ("inspect_match", "pub(crate) async fn inspect_match"),
    ("search_files", "pub(crate) async fn search_files"),
    ("what_changed", "pub(crate) async fn what_changed"),
    ("explore", "pub(crate) async fn explore"),
    ("diff_symbols", "pub(crate) async fn diff_symbols"),
    ("ask", "smart_query::QueryIntent::ToolHelp"),
    (
        "symforge",
        "fn facade_probe_is_measurement_safe(tool: &str, args: &serde_json::Value)",
    ),
];

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum EstimateTrueDisposition {
    PreAuthority,
    SourceDerived,
    IgnoredSourceDerived,
    AliasDropsEstimateSourceDerived,
}

/// Every advertised `estimate=true` ingress, pinned to what current release
/// dispatch actually does rather than inferred from the common parameter name.
const ESTIMATE_TRUE_DISPOSITION: &[(&str, &str, EstimateTrueDisposition, &str)] = &[
    (
        "tools",
        "analyze_file_impact",
        EstimateTrueDisposition::PreAuthority,
        "args-only estimate",
    ),
    (
        "tools",
        "search_text",
        EstimateTrueDisposition::PreAuthority,
        "args-only estimate",
    ),
    (
        "tools",
        "inspect_match",
        EstimateTrueDisposition::PreAuthority,
        "args-only estimate",
    ),
    (
        "tools",
        "search_files",
        EstimateTrueDisposition::PreAuthority,
        "args-only estimate",
    ),
    (
        "tools",
        "what_changed",
        EstimateTrueDisposition::PreAuthority,
        "args-only estimate",
    ),
    (
        "tools",
        "explore",
        EstimateTrueDisposition::PreAuthority,
        "args-only estimate",
    ),
    (
        "tools",
        "diff_symbols",
        EstimateTrueDisposition::PreAuthority,
        "args-only estimate",
    ),
    (
        "tools",
        "get_symbol",
        EstimateTrueDisposition::SourceDerived,
        "estimate reads selected source/publication state",
    ),
    (
        "tools",
        "get_file_content",
        EstimateTrueDisposition::SourceDerived,
        "estimate reads selected source/publication state",
    ),
    (
        "tools",
        "get_repo_map",
        EstimateTrueDisposition::SourceDerived,
        "estimate reads selected source/publication state",
    ),
    (
        "tools",
        "get_file_context",
        EstimateTrueDisposition::SourceDerived,
        "estimate reads selected source/publication state",
    ),
    (
        "tools",
        "get_symbol_context",
        EstimateTrueDisposition::SourceDerived,
        "estimate reads selected source/publication state",
    ),
    (
        "tools",
        "search_symbols",
        EstimateTrueDisposition::IgnoredSourceDerived,
        "advertised flag is ignored; normal source-derived search runs",
    ),
    (
        "tools",
        "find_references",
        EstimateTrueDisposition::IgnoredSourceDerived,
        "advertised flag is ignored; normal source-derived search runs",
    ),
    (
        "tools",
        "find_dependents",
        EstimateTrueDisposition::IgnoredSourceDerived,
        "advertised flag is ignored; normal source-derived search runs",
    ),
    (
        "compatibility_aliases",
        "trace_symbol",
        EstimateTrueDisposition::AliasDropsEstimateSourceDerived,
        "release alias drops estimate and dispatches get_symbol_context (daemon.rs:5523-5542)",
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
/// Authored one member at a time, each with its basis; members that could not
/// take an honest set were brought back as decisions rather than parked on a
/// plausible row, and landed on one of the other two lists.
const SURFACE_OVERLAY: &[(&str, &str, &[&str], &str)] = &[
    // ---- compatibility_aliases (2/2) ----
    // The calibration rows: the frozen entry states an allowed SET for one
    // alias and forbids a branch by name, which is the shape every row below
    // follows.
    (
        "compatibility_aliases",
        "detect_changes",
        &["GitObserved", "Refused", "WorktreeScopeObserved"],
        "compatibility_aliases assertion: `detect_changes` returns GitObserved for committed-ref \
         diffs or WorktreeScopeObserved for worktree diffs, and never acquires a ProjectQueryLease \
         or upgrades observation evidence to GenerationLeased. Current V10's delegated \
         `detect_impact` still consumes generation symbols/graph; T064 must refactor that \
         implementation into the frozen pure-observation target before activation. Its loading \
         guard can terminate unavailable publication state as Refused.",
    ),
    (
        "compatibility_aliases",
        "trace_symbol",
        &["DiskObserved", "GenerationLeased", "Refused"],
        "compatibility_aliases assertion: `trace_symbol` cannot reach V10 symbol caches and uses \
         GenerationLeased ONLY for a complete Current publication. Its delegated path-local \
         symbol-context degradation can return DiskObserved. The `only` forbids the lease \
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
        &["GenerationLeased", "MutationPermitted"],
        "Dry-run renders a plan from the captured current indexed inputs without writing, so that mode is GenerationLeased; apply performs the repository-source writes under MutationPermitted",
    ),
    (
        "writers",
        "src/protocol/edit.rs::execute_batch_insert",
        &["GenerationLeased", "MutationPermitted"],
        "Dry-run renders a plan from the captured current indexed inputs without writing, so that mode is GenerationLeased; apply performs the repository-source writes under MutationPermitted",
    ),
    (
        "writers",
        "src/protocol/edit.rs::execute_batch_rename",
        &["GenerationLeased", "MutationPermitted"],
        "Dry-run renders a plan from the captured current indexed inputs without writing, so that mode is GenerationLeased; apply performs the repository-source writes under MutationPermitted",
    ),
    // These seven writer members duplicate the corresponding tool ingresses.
    // Their identical stored-success replay returns before edit dispatch and is
    // the explicit T058/T064/T066/T072 mode-level residual: ReplayRecord v1
    // carries no typed source/authority receipt, so PR 4 does not mislabel that
    // zero-write response as a fresh MutationPermitted branch.
    (
        "writers",
        "src/protocol/edit_tools.rs::SymForgeServer::batch_edit",
        &[
            "DiskObserved",
            "GenerationLeased",
            "MutationPermitted",
            "Refused",
        ],
        "Dry-run source preparation reports DiskRefreshed or CurrentIndex; apply is a repository-source mutation. Refused covers foreign-project and unavailable-publication termination. This writer member is the same tool ingress as the matching tools row.",
    ),
    (
        "writers",
        "src/protocol/edit_tools.rs::SymForgeServer::batch_insert",
        &[
            "DiskObserved",
            "GenerationLeased",
            "MutationPermitted",
            "Refused",
        ],
        "Dry-run source preparation reports DiskRefreshed or CurrentIndex; apply is a repository-source mutation. Refused covers foreign-project and unavailable-publication termination. This writer member is the same tool ingress as the matching tools row.",
    ),
    (
        "writers",
        "src/protocol/edit_tools.rs::SymForgeServer::batch_rename",
        &[
            "DiskObserved",
            "GenerationLeased",
            "MutationPermitted",
            "Refused",
        ],
        "Dry-run source preparation reports DiskRefreshed or CurrentIndex; apply is a repository-source mutation. Refused covers foreign-project and unavailable-publication termination. This writer member is the same tool ingress as the matching tools row.",
    ),
    (
        "writers",
        "src/protocol/edit_tools.rs::SymForgeServer::delete_symbol",
        &[
            "DiskObserved",
            "GenerationLeased",
            "MutationPermitted",
            "Refused",
            "WorktreeScopeObserved",
        ],
        "Dry-run source preparation reports DiskRefreshed, CurrentIndex, or WorktreeTarget after reroute rebasing; apply is a repository-source mutation. Refused covers foreign-project and unavailable-publication termination. This writer member is the same tool ingress as the matching tools row.",
    ),
    (
        "writers",
        "src/protocol/edit_tools.rs::SymForgeServer::edit_within_symbol",
        &[
            "DiskObserved",
            "GenerationLeased",
            "MutationPermitted",
            "Refused",
            "WorktreeScopeObserved",
        ],
        "Dry-run source preparation reports DiskRefreshed, CurrentIndex, or WorktreeTarget after reroute rebasing; apply is a repository-source mutation. Refused covers foreign-project and unavailable-publication termination. This writer member is the same tool ingress as the matching tools row.",
    ),
    (
        "writers",
        "src/protocol/edit_tools.rs::SymForgeServer::insert_symbol",
        &[
            "DiskObserved",
            "GenerationLeased",
            "MutationPermitted",
            "Refused",
            "WorktreeScopeObserved",
        ],
        "Dry-run source preparation reports DiskRefreshed, CurrentIndex, or WorktreeTarget after reroute rebasing; apply is a repository-source mutation. Refused covers foreign-project and unavailable-publication termination. This writer member is the same tool ingress as the matching tools row.",
    ),
    (
        "writers",
        "src/protocol/edit_tools.rs::SymForgeServer::replace_symbol_body",
        &[
            "DiskObserved",
            "GenerationLeased",
            "MutationPermitted",
            "Refused",
            "WorktreeScopeObserved",
        ],
        "Dry-run source preparation reports DiskRefreshed, CurrentIndex, or WorktreeTarget after reroute rebasing; apply is a repository-source mutation. Refused covers foreign-project and unavailable-publication termination. This writer member is the same tool ingress as the matching tools row.",
    ),
    // ---- tools: edit family (7), the same sets as their writers rows ----
    // The shared stored-success replay residual above applies to each tool row.
    (
        "tools",
        "batch_edit",
        &[
            "DiskObserved",
            "GenerationLeased",
            "MutationPermitted",
            "Refused",
        ],
        "Dry-run source preparation reports DiskRefreshed or CurrentIndex without writing; apply acquires the repository-source MutationPermitted branch. Foreign-project and unavailable-publication paths select Refused.",
    ),
    (
        "tools",
        "batch_insert",
        &[
            "DiskObserved",
            "GenerationLeased",
            "MutationPermitted",
            "Refused",
        ],
        "Dry-run source preparation reports DiskRefreshed or CurrentIndex without writing; apply acquires the repository-source MutationPermitted branch. Foreign-project and unavailable-publication paths select Refused.",
    ),
    (
        "tools",
        "batch_rename",
        &[
            "DiskObserved",
            "GenerationLeased",
            "MutationPermitted",
            "Refused",
        ],
        "Dry-run source preparation reports DiskRefreshed or CurrentIndex without writing; apply acquires the repository-source MutationPermitted branch. Foreign-project and unavailable-publication paths select Refused.",
    ),
    (
        "tools",
        "delete_symbol",
        &[
            "DiskObserved",
            "GenerationLeased",
            "MutationPermitted",
            "Refused",
            "WorktreeScopeObserved",
        ],
        "Dry-run source preparation reports DiskRefreshed, CurrentIndex, or WorktreeTarget after reroute rebasing; apply acquires MutationPermitted. Foreign-project and unavailable-publication paths select Refused.",
    ),
    (
        "tools",
        "edit_within_symbol",
        &[
            "DiskObserved",
            "GenerationLeased",
            "MutationPermitted",
            "Refused",
            "WorktreeScopeObserved",
        ],
        "Dry-run source preparation reports DiskRefreshed, CurrentIndex, or WorktreeTarget after reroute rebasing; apply acquires MutationPermitted. Foreign-project and unavailable-publication paths select Refused.",
    ),
    (
        "tools",
        "insert_symbol",
        &[
            "DiskObserved",
            "GenerationLeased",
            "MutationPermitted",
            "Refused",
            "WorktreeScopeObserved",
        ],
        "Dry-run source preparation reports DiskRefreshed, CurrentIndex, or WorktreeTarget after reroute rebasing; apply acquires MutationPermitted. Foreign-project and unavailable-publication paths select Refused.",
    ),
    (
        "tools",
        "replace_symbol_body",
        &[
            "DiskObserved",
            "GenerationLeased",
            "MutationPermitted",
            "Refused",
            "WorktreeScopeObserved",
        ],
        "Dry-run source preparation reports DiskRefreshed, CurrentIndex, or WorktreeTarget after reroute rebasing; apply acquires MutationPermitted. Foreign-project and unavailable-publication paths select Refused.",
    ),
    (
        "tools",
        "curate_knowledge",
        &[
            "GenerationLeased",
            "MutationPermitted",
            "Refused",
            "StateWriteAuthorized",
        ],
        "Preview validates the reviewed action set against one captured current knowledge publication, selecting GenerationLeased without writing. A fresh source-changing apply selects MutationPermitted; its journal/state writes are subordinate. Recovery or post-image-only finalization selects StateWriteAuthorized without rewriting source. A non-matching local project selects Refused (knowledge_curation.rs:403, :654, :740).",
    ),
    // ---- tools: the two facades, closed last as the union of what they
    // dispatch. Production compact serve is `build_plan` ->
    // `dispatch_tool_for_tests(&step.tool)`. The production-reachable A-019
    // measurement relay is separately restricted to a source-mutation-safe
    // allowlist; `batch_rename` is admitted only with `dry_run=true`. Normal
    // read-path cache/frecency effects remain subordinate.
    (
        "tools",
        "symforge",
        &[
            "DiskObserved",
            "GenerationLeased",
            "GitObserved",
            "Refused",
            "RuntimeHealthObserved",
            "WorktreeScopeObserved",
        ],
        "Union of the planned steps plus the enforced A-019 source-mutation-safe relay. DiskObserved: the \
         planner can emit targeted read/reference tools whose returned degradation or raw fallback \
         is disk-authoritative. GenerationLeased: the planner emits generation-backed read, search, \
         exploration, knowledge, and retrieval steps. GitObserved + WorktreeScopeObserved: \
         `route_impact` plans `detect_impact` (planner.rs:1101-1108), while FindChanges routes via \
         `route_tool_name` (smart_query.rs:613) to `what_changed`; empty default args \
         (src/stel/planner.rs:1203) select its worktree lane. The allowlisted \
         Fusion, co-change, and untracked-path enrichment on another selected result remain \
         subordinate. RuntimeHealthObserved: `index \
         health` plans `health_compact` (planner.rs:879-886). Refused: \
         `foreign_project_refusal` and a set-valued `projects` on THIS handler; empty-query and \
         compact-surface gates are InvalidRequest instead. `facade_probe_is_measurement_safe` constrains \
         the relay to Disk/Generation/Refused modes already in this set and prevents \
         MutationPermitted or an explicit StateWriteAuthorized selection. It emits no semantic \
         result-status metadata because rendered legacy/source text cannot prove an OutcomeClass. \
         ToolHelp routes to `ask`, whose catalog arm is \
         authority-free; that adds no ninth state. This is the target set: current repeat-cache and \
         CCR retrieval paths still lack publication/source identity and must gain the frozen fences \
         before activation rather than being relabeled authority-free.",
    ),
    (
        "tools",
        "symforge_edit",
        &[
            "DiskObserved",
            "GenerationLeased",
            "MutationPermitted",
            "Refused",
            "WorktreeScopeObserved",
        ],
        "`build_edit_plan` emits only `replace_symbol_body`, `insert_symbol`, and `edit_within_symbol` (edit_planner.rs:150-179). Their preview/source-preparation modes report DiskRefreshed, CurrentIndex, or WorktreeTarget; a dispatched source-changing apply acquires MutationPermitted. `foreign_project_refusal` on this facade selects Refused. Identical stored-success replay probes after source preparation but returns before dispatch/permit; ReplayRecord v1 does not bind the fresh observation to its stored result, so that terminal mode is the explicit T058/T064/T066/T072 residual rather than a fabricated branch. `curate_knowledge` remains its own member.",
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
        &["DiskObserved", "GenerationLeased", "Refused"],
        "Refused is `foreign_project_refusal(params.0.project)` at the top of the body. \
         `File not found on disk` is path-local DiskObserved (T042), NOT Refused. The successful \
         edit/new-file modes also consume generation-owned pre/post symbols, publication snapshots, \
         admission tier, and caller graph; under the frozen mixed-derivation rule that result selects \
         GenerationLeased. Optional co-change data is subordinate enrichment, not an independently \
         selected GitObserved result branch (tools.rs:5315-5329; handlers.rs:1219-1298, \
         :1385-1433, :1500-1528, :1631-1668).",
    ),
    // ---- tools: change observation across generation/git/worktree lanes ----
    (
        "tools",
        "detect_impact",
        &["GitObserved", "Refused", "WorktreeScopeObserved"],
        "Frozen target of the detect_changes alias: GitObserved for committed-ref diffs, WorktreeScopeObserved for worktree diffs, and never a ProjectQueryLease. Current V10 still consumes generation symbols and caller graph (tools.rs:8463-8561); T064 must refactor that implementation to pure observation before activation. `loading_guard!` can terminate unavailable publication state as Refused.",
    ),
    (
        "tools",
        "what_changed",
        &[
            "GenerationLeased",
            "GitObserved",
            "Refused",
            "WorktreeScopeObserved",
        ],
        "`since=` selects `WhatChangedMode::Timestamp`, captures the live-index timestamp view, and derives authority with `SourceAuthority::from_freshness`, so a Current result is GenerationLeased. Git-ref mode is GitObserved and uncommitted mode is WorktreeScopeObserved. `foreign_project_refusal` and the loading guard can terminate selection as Refused (tools.rs:3283-3297, :8019-8020, :8058-8064).",
    ),
    // ---- tools: dual-lane ----
    (
        "tools",
        "validate_file_syntax",
        &["DiskObserved", "GenerationLeased", "Refused"],
        "Both lanes are in the body: an indexed read off the published generation and an AUTHORITATIVE disk-read lane taken when the same-project publication is refused (tools.rs:8968-8983, permits_authoritative_disk_fallback); Refused because this handler calls `foreign_project_refusal` on a non-matching `project` selector, which is INV-SURFACE selection termination.",
    ),
    // ---- tools: remaining read-only surfaces ----
    (
        "tools",
        "ask",
        &[
            "DiskObserved",
            "GenerationLeased",
            "Refused",
            "WorktreeScopeObserved",
        ],
        "The routed tool set includes generation-backed reads, the disk-authoritative degradation/reference modes, and worktree-backed search/change modes. SmartQueryInput's non-matching project selector and delegated unavailable-publication paths select Refused. No ask route selects a pure committed-Git result mode.",
    ),
    (
        "tools",
        "diff_symbols",
        &["GitObserved", "Refused"],
        "`diff_symbols` routes every successful observation through `GitRepo::changed_paths_between_refs(base, target)` and parses the Git-ref bytes directly; LiveIndex supplies admission policy, not the parsed content. Missing refs default to `main`/`HEAD`; an explicit empty target fails ref resolution rather than selecting a worktree lane. `foreign_project_refusal` and the loading guard can terminate selection as Refused (tools.rs:12239-12330; format.rs:6541-6605; git.rs:121-157).",
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
        &["RuntimeHealthObserved"],
        "`context_inventory` snapshots `session_context` and CCR compression economics and formats live session/runtime-work state; it never reads publication freshness or acquires a ProjectQueryLease, so tools assertion 4 classifies it RuntimeHealthObserved. No Refused: it has neither a project selector nor a loading guard (tools.rs:11922-11936).",
    ),
    (
        "tools",
        "conventions",
        &["GenerationLeased", "Refused"],
        "`detect_conventions` reads the indexed publication, so the Ready path is GenerationLeased under tools assertion 3. `loading_guard!` terminates Empty, Loading, and CircuitBreaker states as Refused; having no project argument removes only selector refusal, not runtime-unavailability refusal (tools.rs:3933-3944, :10615-10623).",
    ),
    (
        "tools",
        "find_dependents",
        &["GenerationLeased", "Refused"],
        "tools assertion 3: a generation-backed read over one V11 publication, so it holds the ProjectQueryLease; Refused because this handler calls `foreign_project_refusal` on a non-matching `project` selector, which is INV-SURFACE selection termination.",
    ),
    (
        "tools",
        "find_references",
        &["DiskObserved", "GenerationLeased", "Refused"],
        "The normal reference query is generation-backed. Path-admission degradation can return early from repo-confined disk evidence, selecting DiskObserved; the later Tier-2 disk completeness disclosure is subordinate to its already selected result. Non-matching local-project and unavailable-publication paths select Refused (tools.rs:2560-2634, :9103-9118).",
    ),
    (
        "tools",
        "get_file_content",
        &["DiskObserved", "GenerationLeased", "Refused"],
        "Indexed bytes select GenerationLeased; the admitted raw-disk fallback explicitly returns a disk observation. Foreign-project and unavailable-publication paths select Refused (tools.rs:8882-8915).",
    ),
    (
        "tools",
        "get_file_context",
        &["DiskObserved", "GenerationLeased", "Refused"],
        "Normal context pins one publication. An index miss can return the repo-confined disk-derived admission-tier degradation view; foreign-project, targeted-freshen, and unavailable-publication paths select Refused (tools.rs:2560-2634, :4597-4647).",
    ),
    (
        "tools",
        "get_repo_map",
        &["GenerationLeased", "Refused"],
        "tools assertion 3: a generation-backed read over one V11 publication, so it holds the ProjectQueryLease; Refused because this handler calls `foreign_project_refusal` on a non-matching `project` selector, which is INV-SURFACE selection termination.",
    ),
    (
        "tools",
        "get_symbol",
        &["DiskObserved", "GenerationLeased", "Refused"],
        "Target contract: normal symbol reads pin one publication; a single-path index miss can return the repo-confined disk-derived admission-tier degradation view; foreign-project and unavailable-publication paths select Refused. Current repeat-cache hits lack publication identity (tools.rs:4284-4297; session.rs:207-235, :460-469), so T064/T066/T067 must add the frozen cache fence before activation rather than remove GenerationLeased.",
    ),
    (
        "tools",
        "get_symbol_context",
        &["DiskObserved", "GenerationLeased", "Refused"],
        "Normal symbol context pins one publication. Bundle mode can propagate path-local freshening as `disk-refreshed`, and an index miss can return a repo-confined disk admission view; foreign-project and unavailable-publication paths select Refused (tools.rs:4863-4869, :4875-4885, :4914-4968).",
    ),
    (
        "tools",
        "inspect_match",
        &["GenerationLeased", "Refused"],
        "`inspect_match` captures its view from the indexed publication, so the Ready path is GenerationLeased under tools assertion 3. `loading_guard!` terminates Empty, Loading, and CircuitBreaker states as Refused; the lack of a project field removes only selector refusal (tools.rs:3933-3944, :6047-6064).",
    ),
    (
        "tools",
        "review_knowledge",
        &["GenerationLeased", "Refused"],
        "tools assertion 3: a generation-backed read over one V11 publication, so it holds the ProjectQueryLease; Refused because this handler calls `local_cross_project_refusal` on a non-matching `project` selector, the same selector class (tools.rs:7656), which is INV-SURFACE selection termination.",
    ),
    (
        "tools",
        "search_files",
        &["GenerationLeased", "Refused"],
        "Every source-derived successful mode consumes generation-owned structure. Normal path search is directly generation-backed; `changed_with=` renders current-index parse state with temporal co-change signals; and the untracked-path hint compares Git/worktree paths against a generation-owned NotFound result. Under the frozen mixed-derivation rule these remain GenerationLeased rather than lease-free GitObserved or WorktreeScopeObserved branches. Foreign-project and unavailable-publication paths select Refused (tools.rs:2271-2416, :6100-6104, :6122-6380, :6684-6691).",
    ),
    (
        "tools",
        "search_knowledge",
        &["GenerationLeased", "Refused"],
        "tools assertion 3: a generation-backed read over one V11 publication, so it holds the ProjectQueryLease; Refused because this handler calls `local_cross_project_refusal` on a non-matching `project` selector, the same selector class (tools.rs:7656), which is INV-SURFACE selection termination.",
    ),
    (
        "tools",
        "search_symbols",
        &["GenerationLeased", "Refused"],
        "tools assertion 3: a generation-backed read over one V11 publication, so it holds the ProjectQueryLease; Refused because this handler calls `local_cross_project_refusal` on a non-matching `project` selector, the same selector class (tools.rs:7656), which is INV-SURFACE selection termination.",
    ),
    (
        "tools",
        "search_text",
        &["GenerationLeased", "Refused"],
        "Indexed search is generation-backed. Its zero-hit fallback consults that live-index result while gating untracked worktree text and appends both claims, so the frozen mixed-derivation rule keeps the returned mode GenerationLeased rather than selecting a lease-free WorktreeScopeObserved branch. Non-matching local-project and unavailable-publication paths select Refused (tools.rs:2492-2527, :3407-3491).",
    ),
    (
        "tools",
        "symforge_retrieve",
        &["GenerationLeased", "Refused"],
        "Frozen CCR target: a live handle renders only its generation-bound projection; missing, evicted, or foreign-generation handles select Refused. Current CCR blobs carry no source/publication identity (ccr.rs:72-80, :137-166, :185-203), so T064/T066/T067 must add that fence before activation rather than bless retrieval as authority-free.",
    ),
    // ---- sidecar (24/24): 12 routes, each paired with its session twin ----
    (
        "sidecar",
        "GET /health",
        &["Refused", "RuntimeHealthObserved"],
        "Target contract: health success is RuntimeHealthObserved and caller-root mismatch selects Refused under the unconditional frozen sidecar guard. Current V10 skips the standalone health path and omits the daemon guard; T066/T067 must close that gap before activation (handlers.rs:330-350; daemon.rs:4249-4266, :4304).",
    ),
    (
        "sidecar",
        "GET /v1/sessions/{session_id}/sidecar/health",
        &["Refused", "RuntimeHealthObserved"],
        "Session health has the same target modes: RuntimeHealthObserved success and Refused on caller-root mismatch. Current V10's daemon health handler omits the frozen guard; T066/T067 must add it (daemon.rs:4249-4266, :4304).",
    ),
    (
        "sidecar",
        "GET /impact",
        &["DiskObserved", "GenerationLeased", "Refused"],
        "`impact_handler` can return a path-local disk result, while successful edit/new-file analysis consumes generation-owned pre/post symbols, snapshots, admission tier, or caller graph and therefore selects GenerationLeased under the frozen mixed-derivation rule. It has no independently selected Git mode. Refused is sidecar assertion 3: caller-root mismatch terminates selection before either standalone or daemon-proxied handling (handlers.rs:1013-1046, :1219-1298, :1385-1433, :1500-1528, :1631-1668).",
    ),
    (
        "sidecar",
        "GET /v1/sessions/{session_id}/sidecar/impact",
        &["DiskObserved", "GenerationLeased", "Refused"],
        "Session-scoped twin of `impact_handler`: the same disk/generation mixed-result modes, no independent Git mode, and the same caller-root Refused termination (handlers.rs:1013-1046; daemon.rs:3592-3629, :4315-4336).",
    ),
    (
        "sidecar",
        "GET /outline",
        &["GenerationLeased", "Refused"],
        "`outline_handler` may freshen from disk, but then captures and renders `published.live`; DiskRefreshed is provenance inside a generation-derived result, not an independently selected DiskObserved branch. Caller-root or unavailable-publication termination selects Refused (handlers.rs:719-745).",
    ),
    (
        "sidecar",
        "GET /v1/sessions/{session_id}/sidecar/outline",
        &["GenerationLeased", "Refused"],
        "Session-scoped twin of `outline_handler`: refresh provenance feeds the captured publication, so success is GenerationLeased; caller-root or unavailable-publication termination is Refused (handlers.rs:719-745; daemon.rs:3592-3629, :4249-4266, :4304/:4334/:4364/:4393/:4420).",
    ),
    (
        "sidecar",
        "GET /prompt-context",
        &["GenerationLeased", "Refused"],
        "`prompt_context_handler` freshens as needed and then captures/renders `published.live`; the result is GenerationLeased, with DiskRefreshed retained only as provenance. Caller-root or unavailable-publication termination selects Refused (handlers.rs:2307-2445).",
    ),
    (
        "sidecar",
        "GET /v1/sessions/{session_id}/sidecar/prompt-context",
        &["GenerationLeased", "Refused"],
        "Session-scoped twin of `prompt_context_handler`: refresh provenance feeds a captured generation result; caller-root or unavailable-publication termination selects Refused (handlers.rs:2307-2445; daemon.rs:3592-3629, :4249-4266, :4304/:4334/:4364/:4393/:4420).",
    ),
    (
        "sidecar",
        "GET /repo-map",
        &["GenerationLeased", "Refused"],
        "`repo_map_handler`, the generation-backed repo map; sidecar assertion 1. Refused is sidecar assertion 3: standalone and daemon-proxied routes enforce the same caller-root termination (router.rs:52-56, handlers.rs:330-350; daemon.rs:3592-3629, :4249-4266, :4304/:4334/:4364/:4393/:4420).",
    ),
    (
        "sidecar",
        "GET /v1/sessions/{session_id}/sidecar/repo-map",
        &["GenerationLeased", "Refused"],
        "Session-scoped `repo_map_handler` twin: generation-backed success under assertion 1 and the same caller-root Refused termination under assertion 3 (handlers.rs:330-350; daemon.rs:3592-3629, :4249-4266, :4304/:4334/:4364/:4393/:4420).",
    ),
    (
        "sidecar",
        "GET /stats",
        &["Refused", "RuntimeHealthObserved"],
        "Target contract: token statistics are RuntimeHealthObserved and caller-root mismatch selects Refused under the unconditional frozen sidecar guard. Current V10 skips standalone stats and omits the daemon guard; T066/T067 must close that gap (handlers.rs:330-350, :2452; daemon.rs:4249-4266, :4420).",
    ),
    (
        "sidecar",
        "GET /v1/sessions/{session_id}/sidecar/stats",
        &["Refused", "RuntimeHealthObserved"],
        "Session stats has the same target modes: RuntimeHealthObserved success and Refused on caller-root mismatch. Current V10's daemon stats handler omits the frozen guard; T066/T067 must add it (handlers.rs:2452; daemon.rs:4249-4266, :4420).",
    ),
    (
        "sidecar",
        "GET /symbol-context",
        &["GenerationLeased", "Refused"],
        "`symbol_context_handler` may freshen a path, but then captures and renders `published.live`; DiskRefreshed is provenance inside the generation-derived result. Caller-root or unavailable-publication termination selects Refused (handlers.rs:1752-1800).",
    ),
    (
        "sidecar",
        "GET /v1/sessions/{session_id}/sidecar/symbol-context",
        &["GenerationLeased", "Refused"],
        "Session-scoped twin of `symbol_context_handler`: refresh provenance feeds the captured publication, so success is GenerationLeased; caller-root or unavailable-publication termination is Refused (handlers.rs:1752-1800; daemon.rs:3592-3629, :4249-4266, :4304/:4334/:4364/:4393/:4420).",
    ),
    (
        "sidecar",
        "GET /workflows/post-edit-impact",
        &["DiskObserved", "GenerationLeased", "Refused"],
        "Thin alias (sidecar assertion 4): workflow_post_edit_impact_handler delegates to impact_handler, so it takes `/impact`'s set exactly. Standalone and daemon-proxied routes enforce the same caller-root Refused termination (handlers.rs:1045, :330-350; daemon.rs:3592-3629, :4249-4266, :4304/:4334/:4364/:4393/:4420).",
    ),
    (
        "sidecar",
        "GET /v1/sessions/{session_id}/sidecar/workflows/post-edit-impact",
        &["DiskObserved", "GenerationLeased", "Refused"],
        "Session-scoped post-edit alias: the delegated impact handler preserves `/impact`'s exact Disk/Generation/Refused set and caller-root termination (handlers.rs:1045, :330-350; daemon.rs:3592-3629, :4249-4266, :4304/:4334/:4364/:4393/:4420).",
    ),
    (
        "sidecar",
        "GET /workflows/prompt-context",
        &["GenerationLeased", "Refused"],
        "Thin alias (sidecar assertion 4): workflow_prompt_narrowing_handler delegates to prompt_context_handler, whose refresh provenance feeds a captured generation result. Caller-root or unavailable-publication termination selects Refused (handlers.rs:2296, :2307-2445; daemon.rs:3592-3629, :4249-4266, :4304/:4334/:4364/:4393/:4420).",
    ),
    (
        "sidecar",
        "GET /v1/sessions/{session_id}/sidecar/workflows/prompt-context",
        &["GenerationLeased", "Refused"],
        "Session-scoped prompt-context alias: the delegated handler renders a captured generation after any refresh. Caller-root or unavailable-publication termination selects Refused (handlers.rs:2296, :2307-2445; daemon.rs:3592-3629, :4249-4266, :4304/:4334/:4364/:4393/:4420).",
    ),
    (
        "sidecar",
        "GET /workflows/repo-start",
        &["GenerationLeased", "Refused"],
        "Thin alias (sidecar assertion 4): workflow_repo_start_handler delegates to repo_map_handler, so generation-backed success and caller-root Refused termination match `/repo-map` (handlers.rs:2046, :330-350; daemon.rs:3592-3629, :4249-4266, :4304/:4334/:4364/:4393/:4420).",
    ),
    (
        "sidecar",
        "GET /v1/sessions/{session_id}/sidecar/workflows/repo-start",
        &["GenerationLeased", "Refused"],
        "Session-scoped repo-start alias: the delegated handler preserves `/repo-map`'s Generation/Refused set and caller-root termination (handlers.rs:2046, :330-350; daemon.rs:3592-3629, :4249-4266, :4304/:4334/:4364/:4393/:4420).",
    ),
    (
        "sidecar",
        "GET /workflows/search-hit-expansion",
        &["GenerationLeased", "Refused"],
        "Thin alias (sidecar assertion 4): workflow_search_hit_expansion_handler delegates to symbol_context_handler, whose refresh provenance feeds a captured generation result. Caller-root or unavailable-publication termination selects Refused (handlers.rs:1727, :1752-1800; daemon.rs:3592-3629, :4249-4266, :4304/:4334/:4364/:4393/:4420).",
    ),
    (
        "sidecar",
        "GET /v1/sessions/{session_id}/sidecar/workflows/search-hit-expansion",
        &["GenerationLeased", "Refused"],
        "Session-scoped search-hit alias: the delegated handler renders a captured generation after any refresh. Caller-root or unavailable-publication termination selects Refused (handlers.rs:1727, :1752-1800; daemon.rs:3592-3629, :4249-4266, :4304/:4334/:4364/:4393/:4420).",
    ),
    (
        "sidecar",
        "GET /workflows/source-read",
        &["GenerationLeased", "Refused"],
        "Thin alias (sidecar assertion 4): workflow_source_read_handler delegates to outline_handler, whose refresh provenance feeds a captured generation result. Caller-root or unavailable-publication termination selects Refused (handlers.rs:634, :719-745; daemon.rs:3592-3629, :4249-4266, :4304/:4334/:4364/:4393/:4420).",
    ),
    (
        "sidecar",
        "GET /v1/sessions/{session_id}/sidecar/workflows/source-read",
        &["GenerationLeased", "Refused"],
        "Session-scoped source-read alias: the delegated handler renders a captured generation after any refresh. Caller-root or unavailable-publication termination selects Refused (handlers.rs:634, :719-745; daemon.rs:3592-3629, :4249-4266, :4304/:4334/:4364/:4393/:4420).",
    ),
    // ---- hooks (6 overlay; PreTool is authority-free, below) ----
    // Routing confirmed from `endpoint_for` (cli/hook.rs:944), NOT from
    // `workflow_for_subcommand`, which says it does not change routing yet.
    // Fail-open (`fail_open_json`, empty additionalContext) adds no new branch
    // and carries no false Current. It does not erase a typed sidecar 409/503
    // Refused branch that was selected before presentation fallback.
    (
        "hooks",
        "hook:Read",
        &["GenerationLeased", "Refused"],
        "Source Read routes to `/outline`, whose refresh provenance feeds a captured generation result; typed root-conflict/index-unavailable responses select Refused before fail-open presentation. Non-source Read pass-through is the explicit mode-level authority-free residual (cli/hook.rs:291-315, :811-824, :944-1003).",
    ),
    (
        "hooks",
        "hook:SessionStart",
        &["GenerationLeased", "Refused"],
        "`endpoint_for` routes SessionStart to `/repo-map`, whose success is generation-backed. Typed root-conflict/index-unavailable responses select Refused before the hook renders its empty fail-open presentation (cli/hook.rs:944-1003, :1228-1235, :1331-1337).",
    ),
    (
        "hooks",
        "hook:PromptSubmit",
        &["GenerationLeased", "Refused"],
        "`endpoint_for` routes PromptSubmit to `/prompt-context`, whose refresh provenance feeds a captured generation result. Typed root-conflict/index-unavailable responses select Refused before fail-open presentation (cli/hook.rs:944-1003, :1228-1235, :1331-1337).",
    ),
    (
        "hooks",
        "hook:Grep",
        &["GenerationLeased", "Refused", "RuntimeHealthObserved"],
        "A genuine fork inside one ingress: `endpoint_for` sends a plausible symbol name to `/symbol-context` and everything else to `/health`, so success selects GenerationLeased or RuntimeHealthObserved. Grep supplies no path and cannot select symbol-context's path-triggered disk mode. A typed sidecar refusal remains Refused despite later fail-open presentation.",
    ),
    (
        "hooks",
        "hook:Edit",
        &["GenerationLeased", "Refused"],
        "endpoint_for routes Edit to `/impact`, the `analyze_file_impact` lanes. It CANNOT be \
         MutationPermitted — hooks assertion 2: Edit and Write notifications cannot publish, mint \
         a SourceMutationPermit, or bypass mutation authority. Edit never sets `new_file`; its \
         missing/readable results consume generation-owned pre/post state, so success is \
         GenerationLeased. Read errors and typed root-conflict/index-unavailable termination select \
         Refused before the hook's empty fail-open presentation (cli/hook.rs:960; \
         handlers.rs:1385-1473).",
    ),
    (
        "hooks",
        "hook:Write",
        &["DiskObserved", "GenerationLeased", "Refused"],
        "Write routes to `/impact` with `new_file=true`: stable PathMissing returns before \
         published-symbol consumption and selects DiskObserved; a readable receipt consumes \
         `receipt.published` and selects GenerationLeased; ReadError, root conflict, or unavailable \
         publication selects Refused. Hooks assertion 2 still forbids mutation authority \
         (cli/hook.rs:969; handlers.rs:1247-1272).",
    ),
    // ---- resources (7 overlay; glossary and tools/catalog are authority-free) ----
    // A resource wrapper starts from the set of the tool it wraps, then drops
    // the lanes that invocation cannot take. It never gains one: resources
    // assertion 5 says template expansion preserves the selected branch and
    // never upgrades an observation into Current.
    (
        "resources",
        "symforge://file/content",
        &["DiskObserved", "GenerationLeased", "Refused"],
        "Wraps `get_file_content` at resources.rs:210-243, including its raw-disk result lane (tools.rs:8882-8915). Passing `project: None` removes foreign-selector refusal only; unavailable publication state can still select Refused. Ready remains GenerationLeased under resources assertion 1.",
    ),
    (
        "resources",
        "symforge://file/context",
        &["DiskObserved", "GenerationLeased", "Refused"],
        "Wraps `get_file_context` at resources.rs:199-208, including its disk-derived degradation lane (tools.rs:2560-2634, :4633-4648). Passing `project: None` removes foreign-selector refusal only; unavailable publication state can still select Refused. Ready remains GenerationLeased under resources assertion 1.",
    ),
    (
        "resources",
        "symforge://symbol/detail",
        &["DiskObserved", "GenerationLeased", "Refused"],
        "Wraps `get_symbol` at resources.rs:245-257, including its single-path disk-derived degradation lane (tools.rs:4316-4328). Passing `project: None` removes foreign-selector refusal only; unavailable publication state can still select Refused. Ready remains GenerationLeased under resources assertion 1.",
    ),
    (
        "resources",
        "symforge://symbol/context",
        &["DiskObserved", "GenerationLeased", "Refused"],
        "Wraps `get_symbol_context` at resources.rs:259-273, including its path-local disk freshening/degradation result (tools.rs:4863-4886, :4958-4968). Passing `project: None` removes foreign-selector refusal only; unavailable publication state can still select Refused. Ready remains GenerationLeased under resources assertion 1.",
    ),
    (
        "resources",
        "symforge://repo/map",
        &["GenerationLeased", "Refused"],
        "Wraps `get_repo_map` at resources.rs:170-180. Passing `project: None` removes foreign-selector refusal only; the loading guard at tools.rs:4433-4442 can still refuse an unavailable runtime. Ready remains GenerationLeased under resources assertion 1.",
    ),
    (
        "resources",
        "symforge://repo/outline",
        &["GenerationLeased", "Refused"],
        "Wraps `get_repo_map` at resources.rs:158-169 with detail=full. Passing `project: None` removes foreign-selector refusal only; the loading guard at tools.rs:4433-4442 can still refuse an unavailable runtime. Ready remains GenerationLeased under resources assertion 1; detail does not select a different branch.",
    ),
    (
        "resources",
        "symforge://repo/changes/uncommitted",
        &["Refused", "WorktreeScopeObserved"],
        "Wraps `what_changed` with `git_ref: None`, which is the worktree-diff lane ONLY, so the \
         wrapper drops GitObserved (that lane needs a committed ref this invocation cannot \
         supply). Passing no project removes selector refusal, but the delegated loading guard can \
         still terminate unavailable publication state as Refused (resources.rs:182-195; \
         tools.rs:8130-8133). resources assertion 2 keeps the successful worktree lane lease-free.",
    ),
    // ---- the health trio + its resource (4) ----
    // Runtime health is the one family that reports on the runtime itself
    // rather than on indexed content, so it takes its lease-free observed
    // branch and never a ProjectQueryLease. `status(reset_calibration=true)`
    // additionally takes StateWriteAuthorized for its durable calibration
    // reset. Family membership is a basis, not a default: each member cites
    // why it is in the family and whether it has another reachable mode.
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
        &["RuntimeHealthObserved", "StateWriteAuthorized"],
        "`status` normally reports index/daemon runtime state as RuntimeHealthObserved and takes no ProjectQueryLease. With `reset_calibration=true`, both local and proxy-owned paths call `reset_calibration`, which clears durable calibration tables in the ProjectStateDir STEL ledger, so that mode additionally takes StateWriteAuthorized (tools.rs:11503, :11552-11577, :11616-11627, :11772-11780; protocol/mod.rs:739-745; stel_core/ledger_store.rs:340-357, :964-978).",
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
        &["MutationPermitted", "Refused", "StateWriteAuthorized"],
        "Does both: the source policy write via write_policy (FR-037, repo_root POLICY_FILE) and \
         the ProjectStateDir curation state under state_dir/CURATION_STATE_DIR \
         (knowledge_curation.rs:351), which is the permit-free half of writers assertion 3. \
         Capability and durability checks can terminate with the typed unavailable result before \
         either write, selecting Refused (knowledge_curation.rs:422-430, :457-459, :1568-1624).",
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
        &[
            "GenerationLeased",
            "MutationPermitted",
            "Refused",
            "StateWriteAuthorized",
        ],
        "This writer member is the same `curate_knowledge` tool ingress: preview selects GenerationLeased; a fresh source-changing apply selects MutationPermitted; recovery or post-image-only ProjectStateDir finalization selects StateWriteAuthorized; and a non-matching local project selects Refused. Journal/state writes during mutation remain subordinate. The selector refusal lives on this handler, not on write_policy/apply/durable_replace*.",
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

    // Pin the mode-level replay residual to the exact overlay rows it affects.
    // This does not invent a ninth branch: it makes dropping or silently
    // widening the approved residual executable until Slice 4 replaces it.
    assert_eq!(
        EDIT_REPLAY_AUTHORITY_RESIDUAL.len(),
        8,
        "identical replay affects exactly eight edit-tool ingresses"
    );
    assert_eq!(
        EDIT_REPLAY_AUTHORITY_RESIDUAL_OWNERS,
        &["T058", "T064", "T066", "T072"],
        "replay residual ownership moved"
    );
    let mut replay_tools = BTreeSet::new();
    let mut replay_writers = BTreeSet::new();
    for (tool, writer) in EDIT_REPLAY_AUTHORITY_RESIDUAL {
        assert!(
            replay_tools.insert(*tool),
            "replay residual names tool `{tool}` twice"
        );
        let (tool_allowed, _) = overlay
            .get(&("tools", *tool))
            .unwrap_or_else(|| panic!("replay residual tool `{tool}` lost its overlay row"));
        match writer {
            Some(writer) => {
                assert!(
                    replay_writers.insert(*writer),
                    "replay residual names writer `{writer}` twice"
                );
                assert_eq!(
                    writer.strip_prefix("src/protocol/edit_tools.rs::SymForgeServer::"),
                    Some(*tool),
                    "replay residual tool/writer pairing moved"
                );
                let (writer_allowed, _) = overlay.get(&("writers", *writer)).unwrap_or_else(|| {
                    panic!("replay residual writer `{writer}` lost its overlay row")
                });
                assert_eq!(
                    writer_allowed, tool_allowed,
                    "duplicate replay tool/writer rows must carry the same non-replay modes"
                );
            }
            None => assert_eq!(
                *tool, "symforge_edit",
                "only symforge_edit lacks a duplicate writer slot"
            ),
        }
    }
    assert_eq!(
        replay_tools.len(),
        8,
        "replay residual tool names must be unique"
    );
    assert_eq!(
        replay_writers.len(),
        7,
        "seven granular replay tools must retain duplicate writer rows"
    );

    // Source-free successful MODES on otherwise branch-bearing members. Keep
    // this separate from whole-member authority-free ingress: deleting a mode,
    // moving it to an exempt member, or inventing a ninth branch must fail.
    assert_eq!(
        AUTHORITY_FREE_MODE_RESIDUAL.len(),
        16,
        "the reviewed source-free mode residual has exactly sixteen entries"
    );
    assert_eq!(AUTHORITY_FREE_MODE_SOURCE_ANCHORS.len(), 9);
    let tools_source = include_str!("../src/protocol/tools.rs");
    for (member, anchor) in AUTHORITY_FREE_MODE_SOURCE_ANCHORS {
        assert!(
            tools_source.contains(anchor),
            "reviewed source anchor for tools::{member} moved or disappeared: {anchor}"
        );
    }
    assert_eq!(AUTHORITY_FREE_MODE_RESIDUAL_OWNERS.len(), 2);
    assert_eq!(
        AUTHORITY_FREE_MODE_RESIDUAL_OWNERS[0],
        ("hooks", &["T064", "T066", "T067", "T072"][..]),
        "hook source-free mode ownership moved"
    );
    assert_eq!(
        AUTHORITY_FREE_MODE_RESIDUAL_OWNERS[1],
        ("tools", &["T066", "T067", "T072"][..]),
        "tool source-free mode ownership moved"
    );
    let whole_authority_free: BTreeSet<(&str, &str)> = AUTHORITY_FREE_INGRESS
        .iter()
        .map(|(category, member, _)| (*category, *member))
        .collect();
    let non_ingress: BTreeSet<(&str, &str)> = NON_INGRESS_EXCEPTIONS
        .iter()
        .map(|(category, member, _)| (*category, *member))
        .collect();
    let mut mode_triples = BTreeSet::new();
    let mut mode_category_counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut direct_pre_authority_estimates = BTreeSet::new();
    let mut symforge_modes = BTreeSet::new();
    for (category, member, mode, basis) in AUTHORITY_FREE_MODE_RESIDUAL {
        assert!(
            !mode.trim().is_empty(),
            "source-free residual has an empty mode"
        );
        assert!(
            !basis.trim().is_empty(),
            "{category}::{member}::{mode} has no basis"
        );
        assert!(
            mode_triples.insert((*category, *member, *mode)),
            "source-free mode residual duplicates {category}::{member}::{mode}"
        );
        let key = (*category, *member);
        assert!(
            overlay.contains_key(&key),
            "source-free mode {category}::{member}::{mode} lost its branch-bearing overlay row"
        );
        assert!(
            !whole_authority_free.contains(&key) && !non_ingress.contains(&key),
            "source-free mode {category}::{member}::{mode} moved onto a whole-member exemption"
        );
        *mode_category_counts.entry(*category).or_default() += 1;
        if *mode == "estimate.args_only" {
            direct_pre_authority_estimates.insert(key);
        }
        if *member == "symforge" {
            symforge_modes.insert(*mode);
        }
    }
    assert_eq!(
        mode_category_counts,
        BTreeMap::from([("hooks", 1), ("tools", 15)]),
        "source-free mode categories moved"
    );
    assert_eq!(
        direct_pre_authority_estimates,
        BTreeSet::from([
            ("tools", "analyze_file_impact"),
            ("tools", "diff_symbols"),
            ("tools", "explore"),
            ("tools", "inspect_match"),
            ("tools", "search_files"),
            ("tools", "search_text"),
            ("tools", "what_changed"),
        ]),
        "the exact seven direct estimate modes that return before authority moved"
    );
    assert_eq!(
        symforge_modes,
        BTreeSet::from([
            "economics_bypass.plan_floor",
            "pff_bypass.plan_floor",
            "preview.pff_bypass.plan_floor",
            "preview.plan_floor",
            "probe_relay.search_files.estimate.args_only",
            "probe_relay.search_text.estimate.args_only",
            "serve.ask_tool_help.static_catalog",
        ]),
        "the compact facade's exact seven source-free modes moved"
    );

    // The shared `estimate` spelling does not imply shared semantics. Pin all
    // sixteen advertised ingress dispositions and bind the seven genuinely
    // pre-authority tools back to the mode residual above.
    assert_eq!(ESTIMATE_TRUE_DISPOSITION.len(), 16);
    let estimate_field_counts = [
        include_str!("../src/protocol/read_tools.rs")
            .matches("pub estimate: Option<bool>")
            .count(),
        include_str!("../src/protocol/search_tools.rs")
            .matches("pub estimate: Option<bool>")
            .count(),
        include_str!("../src/protocol/tools.rs")
            .matches("pub estimate: Option<bool>")
            .count(),
    ];
    assert_eq!(
        estimate_field_counts,
        [8, 4, 4],
        "advertised estimate ingress declarations moved across protocol modules"
    );
    assert_eq!(
        estimate_field_counts.into_iter().sum::<usize>(),
        ESTIMATE_TRUE_DISPOSITION.len(),
        "every advertised estimate ingress must have one disposition"
    );
    let mut estimate_members = BTreeSet::new();
    let mut estimate_counts: BTreeMap<EstimateTrueDisposition, usize> = BTreeMap::new();
    let mut disposition_pre_authority = BTreeSet::new();
    for (category, member, disposition, basis) in ESTIMATE_TRUE_DISPOSITION {
        assert!(
            !basis.trim().is_empty(),
            "{category}::{member} estimate disposition has no basis"
        );
        assert!(
            estimate_members.insert((*category, *member)),
            "estimate disposition duplicates {category}::{member}"
        );
        assert!(
            overlay.contains_key(&(*category, *member)),
            "estimate disposition {category}::{member} lost its overlay row"
        );
        *estimate_counts.entry(*disposition).or_default() += 1;
        if *disposition == EstimateTrueDisposition::PreAuthority {
            disposition_pre_authority.insert((*category, *member));
        }
    }
    assert_eq!(
        [
            estimate_counts
                .get(&EstimateTrueDisposition::PreAuthority)
                .copied()
                .unwrap_or_default(),
            estimate_counts
                .get(&EstimateTrueDisposition::SourceDerived)
                .copied()
                .unwrap_or_default(),
            estimate_counts
                .get(&EstimateTrueDisposition::IgnoredSourceDerived)
                .copied()
                .unwrap_or_default(),
            estimate_counts
                .get(&EstimateTrueDisposition::AliasDropsEstimateSourceDerived)
                .copied()
                .unwrap_or_default(),
        ],
        [7, 5, 3, 1],
        "estimate=true disposition counts moved"
    );
    assert_eq!(
        disposition_pre_authority, direct_pre_authority_estimates,
        "pre-authority estimate table and source-free mode residual diverged"
    );

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

    // The authority-free list, held to the same standard as the exceptions and
    // disjoint from both other lists: every surface slot lands in EXACTLY ONE
    // of overlay, non-ingress exception, or authority-free ingress.
    let mut statics: BTreeMap<(&str, &str), &str> = BTreeMap::new();
    for (category, member, basis) in AUTHORITY_FREE_INGRESS {
        assert!(
            surface.contains(category),
            "`{category}::{member}` is pinned as authority-free ingress, but `{category}` is not a \
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
    assert_eq!(overlay.len(), 102, "branch-bearing surface partition moved");
    assert_eq!(exceptions.len(), 3, "non-ingress partition moved");
    assert_eq!(statics.len(), 11, "authority-free ingress partition moved");
    assert_eq!(
        overlay.len() + exceptions.len() + statics.len(),
        116,
        "the reviewed 102 + 3 + 11 surface partition moved"
    );

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
