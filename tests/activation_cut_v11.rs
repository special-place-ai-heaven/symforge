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
         GenerationLeased ONLY for a complete Current publication — the word `only` bounds the \
         lease to that case, so an incomplete publication is an unavailability this ingress \
         terminates on rather than a lease it may take",
    ),
    // ---- writers (22/25; 3 brought back, see the T050 decision list) ----
    // Split per writers assertion 3, which draws the line the branches encode:
    // repository-source bytes are source-authorized (MutationPermitted), while
    // ProjectStateDir and post-image team-artifact writes remain permit-free
    // (StateWriteAuthorized). Family membership is cited per member, not
    // inherited from the module.
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
    (
        "writers",
        "src/protocol/knowledge_curation.rs::KnowledgeCurationCoordinator::apply",
        &["StateWriteAuthorized"],
        "writers assertion 3: ProjectStateDir and post-image team-artifact writes remain \
         permit-free — apply writes under state_dir/CURATION_STATE_DIR (knowledge_curation.rs:351)",
    ),
    (
        "writers",
        "src/protocol/knowledge_curation.rs::KnowledgeCurationCoordinator::write_policy",
        &["StateWriteAuthorized"],
        "writers assertion 3: post-image team-artifact write — the `.symforge-knowledge.toml` \
         policy post-image (knowledge_curation.rs:31, :629)",
    ),
    (
        "writers",
        "src/protocol/knowledge_curation.rs::apply_reviewed_mutation",
        &["StateWriteAuthorized"],
        "writers assertion 3: ProjectStateDir curation state write, permit-free",
    ),
    (
        "writers",
        "src/protocol/knowledge_curation.rs::durable_replace",
        &["StateWriteAuthorized"],
        "writers assertion 3: the durable writer for curation state and the policy post-image; \
         every call site (knowledge_curation.rs:629, :1808, :1888, :1901) is state or team artifact",
    ),
    (
        "writers",
        "src/protocol/knowledge_curation.rs::durable_replace_io",
        &["StateWriteAuthorized"],
        "writers assertion 3: the io half of durable_replace, same call sites, same ground",
    ),
    (
        "writers",
        "src/protocol/tools.rs::SymForgeServer::curate_knowledge",
        &["StateWriteAuthorized"],
        "writers assertion 3: tool ingress for curation; its writes are ProjectStateDir and the \
         post-image team artifact, both permit-free",
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

    let mut missing = Vec::new();
    let mut wrongly_present = Vec::new();
    for (category, member) in &inventory.slots {
        let key = (category.as_str(), member.as_str());
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
