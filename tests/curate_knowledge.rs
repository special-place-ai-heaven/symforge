use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use symforge::domain::{ProjectId, ProjectStateDir, StatePlacement, UserLocalPlacementReason};
use symforge::live_index::{LiveIndex, SharedIndex};
use symforge::protocol::SymForgeServer;
use symforge::watcher::{WatcherInfo, WatcherState, run_watcher_with_stop};

struct CurationFixture {
    dir: tempfile::TempDir,
    index: SharedIndex,
    server: SymForgeServer,
    watcher_info: Arc<Mutex<WatcherInfo>>,
}

impl CurationFixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        fs::create_dir_all(root.join("docs")).expect("docs dir");
        fs::write(
            root.join("docs/current.md"),
            "# Current behavior\nThe repository serves byte-exact source.\n",
        )
        .expect("knowledge fixture");
        fs::create_dir_all(root.join("src")).expect("source dir");
        fs::write(
            root.join("src/lib.rs"),
            "pub fn byte_exact() -> bool { true }\n",
        )
        .expect("source fixture");

        let index = LiveIndex::load(&root).expect("LiveIndex::load curation fixture");
        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let server = SymForgeServer::new(
            Arc::clone(&index),
            "curate_knowledge_test".to_string(),
            Arc::clone(&watcher_info),
            Some(root),
            None,
        );
        Self {
            dir,
            index,
            server,
            watcher_info,
        }
    }

    async fn review_and_action(&self) -> (String, Value) {
        let review = self
            .server
            .dispatch_tool_for_tests(
                "review_knowledge",
                json!({"mode": "remediation", "source_scope": "current", "limit": 20}),
            )
            .await;
        let review_hash = token(&review, "review_hash");
        let manifest_digest = token(&review, "manifest_digest");
        let policy_digest = token(&review, "policy_digest");
        let lines = review.lines().collect::<Vec<_>>();
        let dossier_starts = lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| line.starts_with("dossier unit=").then_some(index))
            .collect::<Vec<_>>();
        let dossier = dossier_starts
            .iter()
            .enumerate()
            .map(|(position, start)| {
                let end = dossier_starts
                    .get(position + 1)
                    .copied()
                    .unwrap_or(lines.len());
                lines[*start..end].join("\n")
            })
            .find(|dossier| {
                dossier.contains("proposal.unmet_preconditions=[]")
                    || dossier.contains("proposal.unmet_preconditions=[requires_user_judgment]")
            })
            .unwrap_or_else(|| panic!("missing approvable curation dossier: {review}"));
        let action_id = token(&dossier, "action_id");
        let path = token(&dossier, "unit");
        let content_hash = token(&dossier, "content_hash");
        let range = token(&dossier, "bytes");
        let (start, end) = range
            .split_once("..")
            .unwrap_or_else(|| panic!("invalid dossier byte range: {dossier}"));
        let start = start.parse::<u32>().expect("start byte");
        let end = end.parse::<u32>().expect("end byte");
        let bytes = fs::read(self.dir.path().join(&path)).expect("target bytes");
        let unit_hash = hex_digest(
            bytes
                .get(start as usize..end as usize)
                .expect("reviewed unit range"),
        );
        let action = json!({
            "action_id": action_id,
            "mutation": {
                "operation": "upsert",
                "entry": {
                    "entry_id": "entry-gate-k-preview",
                    "target": {
                        "path": path,
                        "content_hash": content_hash,
                        "unit_byte_range": [start, end],
                        "unit_hash": unit_hash
                    },
                    "lifecycle": "unknown",
                    "evidence": [],
                    "justification_code": "approved-review"
                }
            }
        });
        (
            review_hash.clone(),
            json!({
                "actions": [action],
                "project": self.dir.path().display().to_string(),
                "if_source_review_hash": review_hash.clone(),
                "if_manifest_digest": manifest_digest,
                "if_policy_digest": policy_digest
            }),
        )
    }
}

fn token(text: &str, name: &str) -> String {
    text.split_whitespace()
        .find_map(|field| field.strip_prefix(&format!("{name}=")))
        .map(|value| value.trim_end_matches([',', ']']).to_string())
        .unwrap_or_else(|| panic!("missing {name}: {text}"))
}

fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn server_with_state_placement(
    root: &std::path::Path,
    state_placement: StatePlacement,
) -> SymForgeServer {
    let index = LiveIndex::load(root).expect("LiveIndex::load alternate placement");
    SymForgeServer::new_with_state_placement(
        index,
        "curate_knowledge_capability_test".to_string(),
        Arc::new(Mutex::new(WatcherInfo::default())),
        Some(root.to_path_buf()),
        Some(state_placement),
        None,
    )
}

#[tokio::test]
async fn preview_is_side_effect_free_and_apply_mutates_only_the_policy_ledger() {
    let fixture = CurationFixture::new();
    let document_path = fixture.dir.path().join("docs/current.md");
    let document_before = fs::read(&document_path).expect("document before curation");
    let policy_path = fixture.dir.path().join(".symforge-knowledge.toml");
    let replay_path = fixture.dir.path().join(".symforge/curation");
    assert!(!policy_path.exists());
    assert!(!replay_path.exists());

    let (_review_hash, input) = fixture.review_and_action().await;
    let preview = fixture
        .server
        .dispatch_tool_for_tests("curate_knowledge", input.clone())
        .await;
    assert!(preview.contains("status=preview"), "{preview}");
    assert!(preview.contains("ledger_diff_v1"), "{preview}");
    assert!(!policy_path.exists(), "preview created the policy ledger");
    assert!(
        !replay_path.exists(),
        "preview reserved durable replay state"
    );
    assert_eq!(
        fs::read(&document_path).expect("document after preview"),
        document_before
    );

    let mut apply = input;
    apply["apply"] = json!(true);
    apply["idempotency_key"] = json!("gate-k-r01");
    let applied = fixture
        .server
        .dispatch_tool_for_tests("curate_knowledge", apply)
        .await;
    assert!(applied.contains("status=applied"), "{applied}");
    assert!(applied.contains("ledger_diff_v1"), "{applied}");
    assert!(
        policy_path.is_file(),
        "apply did not create the policy ledger"
    );
    assert!(
        replay_path.is_dir(),
        "apply did not create durable replay state"
    );
    assert_eq!(
        fs::read(&document_path).expect("document after apply"),
        document_before,
        "curation must not edit a reviewed document"
    );
}

#[tokio::test]
async fn identical_replay_precedes_stale_guards_and_changed_request_conflicts() {
    let fixture = CurationFixture::new();
    let policy_path = fixture.dir.path().join(".symforge-knowledge.toml");
    let (_review_hash, mut apply) = fixture.review_and_action().await;
    apply["apply"] = json!(true);
    apply["idempotency_key"] = json!("gate-k-r02");

    let first = fixture
        .server
        .dispatch_tool_for_tests("curate_knowledge", apply.clone())
        .await;
    assert!(first.contains("status=applied"), "{first}");
    let policy_after_first = fs::read(&policy_path).expect("policy after first apply");

    // The original policy guard is stale as soon as the first apply commits.
    // Same key + same canonical request must return the stored terminal result
    // before consulting that now-stale guard.
    let replay = fixture
        .server
        .dispatch_tool_for_tests("curate_knowledge", apply.clone())
        .await;
    assert_eq!(replay, first, "terminal replay must be byte-identical");
    assert_eq!(
        fs::read(&policy_path).expect("policy after replay"),
        policy_after_first,
        "replay must not apply the policy twice"
    );

    let mut conflict = apply;
    conflict["if_manifest_digest"] = json!("0".repeat(64));
    let conflict_output = fixture
        .server
        .dispatch_tool_for_tests("curate_knowledge", conflict)
        .await;
    assert!(
        conflict_output.contains("idempotency_conflict"),
        "{conflict_output}"
    );
    assert_eq!(
        fs::read(&policy_path).expect("policy after conflict"),
        policy_after_first,
        "idempotency conflict must not mutate the policy"
    );
}

#[tokio::test]
async fn concurrent_curators_serialize_and_revalidate_the_policy_under_one_lock() {
    let fixture = CurationFixture::new();
    let policy_path = fixture.dir.path().join(".symforge-knowledge.toml");
    let (_review_hash, base) = fixture.review_and_action().await;
    let mut left = base.clone();
    left["apply"] = json!(true);
    left["idempotency_key"] = json!("gate-k-r03-left");
    left["actions"][0]["mutation"]["entry"]["entry_id"] = json!("entry-left");
    let mut right = base;
    right["apply"] = json!(true);
    right["idempotency_key"] = json!("gate-k-r03-right");
    right["actions"][0]["mutation"]["entry"]["entry_id"] = json!("entry-right");

    let (left_output, right_output) = tokio::join!(
        fixture
            .server
            .dispatch_tool_for_tests("curate_knowledge", left),
        fixture
            .server
            .dispatch_tool_for_tests("curate_knowledge", right),
    );
    let outputs = [&left_output, &right_output];
    assert_eq!(
        outputs
            .iter()
            .filter(|output| output.contains("status=applied"))
            .count(),
        1,
        "exactly one curator must commit: {left_output}\n{right_output}"
    );
    assert_eq!(
        outputs
            .iter()
            .filter(|output| output.contains("stale_policy_digest"))
            .count(),
        1,
        "the serialized loser must revalidate the on-disk policy: {left_output}\n{right_output}"
    );

    let policy = fs::read_to_string(policy_path).expect("committed policy");
    assert_ne!(
        policy.contains("entry-left"),
        policy.contains("entry-right")
    );
}

#[tokio::test]
async fn sensitive_input_rejects_before_replay_probe_temp_or_policy_state() {
    let fixture = CurationFixture::new();
    let policy_path = fixture.dir.path().join(".symforge-knowledge.toml");
    let replay_path = fixture.dir.path().join(".symforge/curation");
    let (_review_hash, mut apply) = fixture.review_and_action().await;
    let canary = ["runtime", "-", "curation", "-", "canary"].concat();
    apply["apply"] = json!(true);
    apply["idempotency_key"] = json!("gate-k-r04");
    apply["actions"][0]["mutation"]["entry"]["justification_code"] =
        json!(format!("token={canary}"));

    let output = fixture
        .server
        .dispatch_tool_for_tests("curate_knowledge", apply)
        .await;
    assert!(output.contains("sensitive or oversized"), "{output}");
    assert!(!output.contains(&canary), "sensitive input must not echo");
    assert!(!policy_path.exists(), "sensitive input created a policy");
    assert!(
        !replay_path.exists(),
        "sensitive input created replay/probe state"
    );
}

#[tokio::test]
async fn unavailable_sources_are_rejected_before_durability_probe_state() {
    let fixture = CurationFixture::new();
    let (_review_hash, mut apply) = fixture.review_and_action().await;
    apply["apply"] = json!(true);
    apply["idempotency_key"] = json!("gate-k-r05");

    let external_state = tempfile::tempdir().expect("external state tempdir");
    let external_project_state = external_state.path().join("project-state");
    let protected = server_with_state_placement(
        fixture.dir.path(),
        StatePlacement::UserLocal {
            directory: ProjectStateDir::new(external_project_state.clone()),
            root_id: ProjectId("protected-test".to_string()),
            reason: UserLocalPlacementReason::ExplicitProtected,
        },
    );
    let protected_output = protected
        .dispatch_tool_for_tests("curate_knowledge", apply.clone())
        .await;
    assert!(
        protected_output.contains("explicit_protected_source"),
        "{protected_output}"
    );
    assert!(
        !external_project_state.join("curation").exists(),
        "protected source reached durability probe/replay state"
    );
    assert!(!fixture.dir.path().join(".symforge-knowledge.toml").exists());

    let memory_only = server_with_state_placement(
        fixture.dir.path(),
        StatePlacement::MemoryOnly { failures: vec![] },
    );
    let memory_output = memory_only
        .dispatch_tool_for_tests("curate_knowledge", apply.clone())
        .await;
    assert!(
        memory_output.contains("durable_mutation_replay_unavailable"),
        "{memory_output}"
    );
    assert!(
        !fixture.dir.path().join(".symforge/curation").exists(),
        "memory-only source reached durability probe/replay state"
    );

    let policy_path = fixture.dir.path().join(".symforge-knowledge.toml");
    fs::write(&policy_path, "version = 1\n").expect("read-only policy fixture");
    let mut permissions = fs::metadata(&policy_path)
        .expect("policy metadata")
        .permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&policy_path, permissions).expect("set policy read-only");
    let read_only_output = fixture
        .server
        .dispatch_tool_for_tests("curate_knowledge", apply)
        .await;
    assert!(
        read_only_output.contains("source_read_only"),
        "{read_only_output}"
    );
    assert!(
        !fixture.dir.path().join(".symforge/curation").exists(),
        "read-only source reached durability probe/replay state"
    );
    let mut permissions = fs::metadata(&policy_path)
        .expect("policy metadata")
        .permissions();
    // Test cleanup on Windows: clear the read-only attribute so TempDir can
    // remove the file. The cross-platform caveat clippy warns about does not
    // apply to this single-attribute Windows fixture teardown.
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    fs::set_permissions(&policy_path, permissions).expect("clear policy read-only");
}

#[tokio::test]
async fn implicit_worktree_without_project_selector_is_rejected_before_probe_io() {
    let fixture = CurationFixture::new();
    let (_review_hash, mut apply) = fixture.review_and_action().await;
    apply["apply"] = json!(true);
    apply["idempotency_key"] = json!("gate-k-r16-implicit-worktree");
    apply
        .as_object_mut()
        .expect("curation request object")
        .remove("project");

    let output = fixture
        .server
        .dispatch_tool_for_tests("curate_knowledge", apply)
        .await;

    assert!(output.contains("non_project_local_placement"), "{output}");
    assert!(
        !fixture.dir.path().join(".symforge/curation").exists(),
        "implicit worktree reached durability probe/replay state"
    );
    assert!(!fixture.dir.path().join(".symforge-knowledge.toml").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn successful_apply_publishes_policy_and_voice_through_the_ordinary_watcher() {
    let fixture = CurationFixture::new();
    let stop_token = Arc::new(AtomicBool::new(false));
    let watcher_task = tokio::spawn(run_watcher_with_stop(
        fixture.dir.path().to_path_buf(),
        Arc::clone(&fixture.index),
        Arc::clone(&fixture.watcher_info),
        Arc::clone(&stop_token),
    ));
    // The guard is intentionally held across the await: it throttles the
    // background watcher during registration so it does not republish (and
    // stale the captured review hash) before the apply below. This is a bounded
    // test wait loop, not production async, so await-holding-lock is benign.
    #[allow(clippy::await_holding_lock)]
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let watcher = fixture.watcher_info.lock();
            if watcher.last_reconcile_at.is_some() && watcher.state == WatcherState::Active {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("watcher must complete its registration reconcile");

    let (_review_hash, mut apply) = fixture.review_and_action().await;
    let target_path = apply["actions"][0]["mutation"]["entry"]["target"]["path"]
        .as_str()
        .expect("target path")
        .to_string();
    apply["apply"] = json!(true);
    apply["idempotency_key"] = json!("gate-k-r11");
    let captured_before = fixture.index.published_generation();
    let policy_digest_before = captured_before.authority.policy_digest.clone();

    let applied = fixture
        .server
        .dispatch_tool_for_tests("curate_knowledge", apply)
        .await;
    assert!(applied.contains("status=applied"), "{applied}");
    assert!(
        applied.contains(&format!(
            "publication_generation={} publication_status=pending",
            captured_before.publication_generation
        )),
        "{applied}"
    );
    let expected_policy_digest = token(&applied, "post_policy_digest");

    let published_after = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let candidate = fixture.index.published_generation();
            if candidate.publication_generation > captured_before.publication_generation
                && candidate.authority.policy_digest == expected_policy_digest
            {
                break candidate;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("ordinary watcher must publish the committed policy");

    stop_token.store(true, Ordering::Release);
    tokio::time::timeout(Duration::from_secs(2), watcher_task)
        .await
        .expect("watcher must stop promptly")
        .expect("watcher task must not panic");

    assert_eq!(
        captured_before.authority.policy_digest, policy_digest_before,
        "an already captured generation must remain immutable"
    );
    assert_ne!(
        published_after.authority.policy_digest,
        policy_digest_before
    );
    let curated = published_after
        .authority
        .records
        .iter()
        .find(|record| record.unit.path == target_path)
        .expect("curated unit must exist in the new authority view");
    assert_eq!(
        curated.lifecycle,
        symforge::live_index::knowledge_authority::KnowledgeLifecycle::Unknown
    );
    assert_eq!(
        curated.voice,
        symforge::live_index::knowledge_authority::KnowledgeVoice::Unknown
    );
    assert_eq!(
        published_after.authority.content_generation, published_after.content_generation,
        "policy and derived voice must publish in one immutable generation"
    );
}

#[tokio::test]
async fn invalid_stale_or_mixed_actions_never_mutate_the_policy_ledger() {
    let fixture = CurationFixture::new();
    let policy_path = fixture.dir.path().join(".symforge-knowledge.toml");
    let document_path = fixture.dir.path().join("docs/current.md");
    let document_before = fs::read(&document_path).expect("document before invalid attempts");
    let (_review_hash, base) = fixture.review_and_action().await;

    let mut cases = Vec::new();

    let mut stale_review = base.clone();
    stale_review["if_source_review_hash"] = json!("0".repeat(64));
    cases.push(("stale-review", stale_review, "stale_review_hash"));

    let mut stale_manifest = base.clone();
    stale_manifest["if_manifest_digest"] = json!("0".repeat(64));
    cases.push(("stale-manifest", stale_manifest, "stale_manifest_digest"));

    let mut stale_policy = base.clone();
    stale_policy["if_policy_digest"] = json!("0".repeat(64));
    cases.push(("stale-policy", stale_policy, "stale_policy_digest"));

    let mut stale_target = base.clone();
    stale_target["actions"][0]["mutation"]["entry"]["target"]["content_hash"] =
        json!("0".repeat(64));
    cases.push(("stale-target", stale_target, "stale_target_guard"));

    let mut unknown_action = base.clone();
    unknown_action["actions"][0]["action_id"] = json!("action-unknown");
    cases.push(("unknown-action", unknown_action, "unknown_action_id"));

    let mut mixed_batch = base.clone();
    let mut invalid_second = mixed_batch["actions"][0].clone();
    invalid_second["action_id"] = json!("action-unknown-in-mixed-batch");
    invalid_second["mutation"]["entry"]["entry_id"] = json!("entry-invalid-second");
    mixed_batch["actions"]
        .as_array_mut()
        .expect("actions array")
        .push(invalid_second);
    cases.push(("mixed-batch", mixed_batch, "unknown_action_id"));

    for operation in ["move", "delete", "schema_invalid"] {
        let mut invalid_shape = base.clone();
        invalid_shape["actions"][0]["mutation"]["operation"] = json!(operation);
        cases.push((operation, invalid_shape, "Error"));
    }

    for (case_name, mut input, expected) in cases {
        input["apply"] = json!(true);
        input["idempotency_key"] = json!(format!("gate-k-r12-{case_name}"));
        let output = fixture
            .server
            .dispatch_tool_for_tests("curate_knowledge", input)
            .await;
        assert!(
            output.contains(expected)
                || (expected == "Error" && !output.contains("status=applied")),
            "{case_name}: {output}"
        );
        assert!(
            !policy_path.exists(),
            "{case_name} mutated the policy ledger"
        );
        assert_eq!(
            fs::read(&document_path).expect("document after invalid attempt"),
            document_before,
            "{case_name} mutated a target document"
        );
    }
}

#[tokio::test]
async fn health_separates_live_readiness_from_curation_replay_and_durability() {
    let fixture = CurationFixture::new();
    let before = fixture
        .server
        .dispatch_tool_for_tests("health", json!({}))
        .await;
    assert!(before.contains("Status: Ready"), "{before}");
    assert!(
        before.contains(
            "Knowledge curation: capability=unavailable reason=atomic_durability_unavailable"
        ),
        "{before}"
    );
    assert!(before.contains("recovery=clean"), "{before}");

    let (_review_hash, mut apply) = fixture.review_and_action().await;
    apply["apply"] = json!(true);
    apply["idempotency_key"] = json!("gate-k-g06-health");
    let applied = fixture
        .server
        .dispatch_tool_for_tests("curate_knowledge", apply)
        .await;
    assert!(applied.contains("status=applied"), "{applied}");

    let after = fixture
        .server
        .dispatch_tool_for_tests("health", json!({}))
        .await;
    assert!(after.contains("Status: Ready"), "{after}");
    assert!(
        after.contains("Knowledge curation: capability=available"),
        "{after}"
    );
    assert!(after.contains("replay_records=1"), "{after}");
    assert!(after.contains("pending=0"), "{after}");
    assert!(after.contains("recovery=clean"), "{after}");
}
