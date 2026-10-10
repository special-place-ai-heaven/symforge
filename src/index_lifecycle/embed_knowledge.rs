//! Embedded knowledge review through the same dossier engine as MCP.

use super::embedded::EmbeddedSourceHandle;
use crate::domain::{CapabilityStatus, ProjectStateDir, StatePlacement};
use crate::embed::parity::edit::EditApplyAuthority;
#[cfg(test)]
use crate::embed::parity::knowledge::CurateKnowledgeInput;
use crate::embed::parity::knowledge::{
    BoundCurateKnowledgeInput, KnowledgeCurationError, KnowledgeCurationResult,
    KnowledgeCurationStatus, KnowledgeReviewError, KnowledgeReviewResult, ReviewKnowledgeInput,
    WireKnowledgeError, WireKnowledgeReply, WireKnowledgeRequest,
};
use crate::knowledge::curation::KnowledgeCurationCoordinator;
use std::sync::atomic::Ordering;

const MAX_REVIEW_BYTES: usize = 1_048_576;
const MAX_CURATION_RECEIPT_BYTES: usize = 3 * 1_048_576;

fn checked_curation_receipt(
    rendered: String,
    status: KnowledgeCurationStatus,
) -> Result<String, KnowledgeCurationError> {
    if rendered.len() > MAX_CURATION_RECEIPT_BYTES
        || crate::knowledge::guard_query(&rendered).is_err()
    {
        return Err(match status {
            KnowledgeCurationStatus::Preview => KnowledgeCurationError::EngineRejected,
            KnowledgeCurationStatus::Applied => KnowledgeCurationError::Uncertain,
        });
    }
    Ok(rendered)
}

impl EmbeddedSourceHandle {
    pub fn preview_wire_knowledge(
        &self,
        request: &WireKnowledgeRequest,
    ) -> Result<WireKnowledgeReply, WireKnowledgeError> {
        match request {
            WireKnowledgeRequest::Review { input } => self
                .review_knowledge(input)
                .map(WireKnowledgeReply::Review)
                .map_err(WireKnowledgeError::Review),
            WireKnowledgeRequest::CuratePreview {
                input,
                if_serving_publication_identity,
            } if !input.apply && input.idempotency_key.is_none() => self
                .curate_knowledge(
                    &BoundCurateKnowledgeInput {
                        input: input.clone(),
                        if_serving_publication_identity: if_serving_publication_identity.clone(),
                    },
                    None,
                )
                .map(WireKnowledgeReply::Curation)
                .map_err(WireKnowledgeError::Curation),
            _ => Err(WireKnowledgeError::InvalidKind),
        }
    }

    pub fn apply_wire_knowledge(
        &self,
        request: &WireKnowledgeRequest,
        authority: &EditApplyAuthority,
        operation_key: &str,
    ) -> Result<WireKnowledgeReply, WireKnowledgeError> {
        let WireKnowledgeRequest::CurateApply {
            input,
            if_serving_publication_identity,
        } = request
        else {
            return Err(WireKnowledgeError::InvalidKind);
        };
        if !input.apply || input.idempotency_key.as_deref() != Some(operation_key) {
            return Err(WireKnowledgeError::InvalidKind);
        }
        self.curate_knowledge(
            &BoundCurateKnowledgeInput {
                input: input.clone(),
                if_serving_publication_identity: if_serving_publication_identity.clone(),
            },
            Some(authority),
        )
        .map(WireKnowledgeReply::Curation)
        .map_err(WireKnowledgeError::Curation)
    }

    pub fn review_knowledge(
        &self,
        request: &ReviewKnowledgeInput,
    ) -> Result<KnowledgeReviewResult, KnowledgeReviewError> {
        if request.project.is_some() || request.projects.is_some() {
            return Err(KnowledgeReviewError::InvalidSelection);
        }
        crate::knowledge::review::validate_input(request)
            .map_err(|_| KnowledgeReviewError::InvalidInput)?;
        let snapshot = self.capture_query_snapshot(b"review-knowledge")?;
        let output = crate::knowledge::review::review_scoped(&snapshot.source_set, request)
            .map_err(|_| KnowledgeReviewError::Readiness)?;
        let budget = request
            .max_tokens
            .filter(|tokens| *tokens > 0)
            .map(|tokens| tokens.saturating_mul(4).min(MAX_REVIEW_BYTES as u64) as usize)
            .unwrap_or(MAX_REVIEW_BYTES);
        let summarized = output.rendered.len() > budget;
        let rendered = if summarized {
            output.budget_rendered
        } else {
            output.rendered
        };
        if rendered.len() > budget {
            return Err(KnowledgeReviewError::BoundExceeded);
        }
        Ok(KnowledgeReviewResult {
            rendered,
            source_key: output.source_key,
            review_hash: output.review_hash,
            result_hash: output.result_hash,
            publication_identity: snapshot.publication_identity,
            serving_publication_identity: snapshot.serving_publication_identity,
            source_version: snapshot.source_version,
            publication_generation: snapshot.generation.publication_generation,
            summarized,
        })
    }

    /// Run the MCP curation engine against this bound source. Preview requires
    /// no write grant; apply requires a host grant and the engine's own source
    /// mutation permit, durable replay, and exact review guards.
    pub fn curate_knowledge(
        &self,
        request: &BoundCurateKnowledgeInput,
        authority: Option<&EditApplyAuthority>,
    ) -> Result<KnowledgeCurationResult, KnowledgeCurationError> {
        let (snapshot, index, placement) = self.capture_curation_context(b"curate-knowledge")?;
        if request
            .input
            .project
            .as_deref()
            .is_some_and(|project| project != snapshot.binding_identity.as_str())
        {
            return Err(KnowledgeCurationError::InvalidInput);
        }
        let mut input = request.input.clone();
        input.project = Some(snapshot.binding_identity.clone());
        crate::knowledge::curation::validate_input(&input)
            .map_err(|_| KnowledgeCurationError::InvalidInput)?;
        let coordinator = KnowledgeCurationCoordinator::default();
        if !input.apply {
            if request.if_serving_publication_identity != snapshot.serving_publication_identity {
                return Err(KnowledgeCurationError::StalePublication);
            }
            let rendered = coordinator.preview_pinned(&snapshot.generation, &snapshot.root, &input);
            if !rendered.starts_with("status=preview\n") {
                return Err(KnowledgeCurationError::EngineRejected);
            }
            let rendered = checked_curation_receipt(rendered, KnowledgeCurationStatus::Preview)?;
            return Ok(KnowledgeCurationResult {
                status: KnowledgeCurationStatus::Preview,
                rendered,
                refresh_ticket_identity: None,
            });
        }
        let authority = authority.ok_or(KnowledgeCurationError::WriteAuthorityRefused)?;
        if authority.root != snapshot.root || authority.cancel.load(Ordering::Acquire) {
            return Err(KnowledgeCurationError::WriteAuthorityRefused);
        }
        let scoped_placement = scoped_curation_placement(&placement, &authority.scope)
            .ok_or(KnowledgeCurationError::DurableStateUnavailable)?;
        if request.if_serving_publication_identity != snapshot.serving_publication_identity {
            let state_anchor = self
                .bound_state_anchor()
                .ok_or(KnowledgeCurationError::DurableStateUnavailable)?;
            let rendered = coordinator
                .replay_completed_guarded(
                    &index,
                    &snapshot.root,
                    Some(&scoped_placement),
                    CapabilityStatus::Available,
                    &input,
                    &authority.cancel,
                    &snapshot.authority,
                    (state_anchor.lease(), state_anchor.stable_key()),
                )
                .map_err(|error| {
                    if error.starts_with("Error: stale_publication") {
                        KnowledgeCurationError::StalePublication
                    } else if error.starts_with("Error: idempotency_conflict")
                        || error.starts_with("Error: stale_replay_image")
                        || error.starts_with("Error: foreign_source_conflict")
                    {
                        KnowledgeCurationError::ReplayConflict
                    } else {
                        KnowledgeCurationError::Uncertain
                    }
                })?;
            return Ok(KnowledgeCurationResult {
                status: KnowledgeCurationStatus::Applied,
                rendered: checked_curation_receipt(rendered, KnowledgeCurationStatus::Applied)?,
                refresh_ticket_identity: None,
            });
        }
        let state_anchor = snapshot
            .state_anchor
            .as_ref()
            .ok_or(KnowledgeCurationError::DurableStateUnavailable)?;
        let rendered = coordinator.execute_guarded(
            &index,
            &snapshot.root,
            Some(&scoped_placement),
            CapabilityStatus::Available,
            &input,
            snapshot.authority_publication,
            &authority.cancel,
            &snapshot.authority,
            (state_anchor.lease(), state_anchor.stable_key()),
        );
        // An engine error can occur after its durable policy write. Always
        // schedule a fresh source observation before classifying the result.
        let refresh = self.request_refresh();
        if !rendered.starts_with("status=applied\n") {
            return Err(if rendered.starts_with("status=unavailable\n") {
                KnowledgeCurationError::DurableStateUnavailable
            } else {
                KnowledgeCurationError::Uncertain
            });
        }
        let refresh = refresh.map_err(|_| KnowledgeCurationError::Uncertain)?;
        let rendered = checked_curation_receipt(rendered, KnowledgeCurationStatus::Applied)?;
        if authority.cancel.load(Ordering::Acquire) {
            return Err(KnowledgeCurationError::Uncertain);
        }
        Ok(KnowledgeCurationResult {
            status: KnowledgeCurationStatus::Applied,
            rendered,
            refresh_ticket_identity: Some(refresh.ticket_identity().to_owned()),
        })
    }
}

fn scoped_curation_placement(placement: &StatePlacement, scope: &str) -> Option<StatePlacement> {
    if scope.trim().is_empty() {
        return None;
    }
    let directory = placement.directory()?;
    let scoped = ProjectStateDir::new(
        directory
            .as_path()
            .join("embed")
            .join("curation-scopes")
            .join(crate::hash::digest_hex(scope.as_bytes())),
    );
    Some(match placement {
        StatePlacement::ProjectLocal { .. } => StatePlacement::ProjectLocal { directory: scoped },
        StatePlacement::UserLocal {
            root_id, reason, ..
        } => StatePlacement::UserLocal {
            directory: scoped,
            root_id: root_id.clone(),
            reason: reason.clone(),
        },
        StatePlacement::MemoryOnly { .. } => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embed::parity::knowledge::{
        KnowledgePolicyActionInput, KnowledgePolicyEntryInput, KnowledgePolicyLifecycleInput,
        KnowledgePolicyMutationInput, KnowledgePolicyTargetInput, WireKnowledgeReply,
        WireKnowledgeRequest,
    };
    use crate::embed::{EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase};
    use std::fs;
    use std::sync::{Arc, atomic::AtomicBool};
    use std::time::{Duration, Instant};

    #[test]
    fn curation_completed_replay_survives_process_restart() {
        let Ok(child_root) = std::env::var("SYMFORGE_KNOWLEDGE_REPLAY_CHILD_ROOT") else {
            let repository = tempfile::tempdir().expect("temporary repository");
            let root = repository.path();
            fs::create_dir_all(root.join("docs")).unwrap();
            fs::create_dir_all(root.join("src")).unwrap();
            fs::write(
                root.join("docs/current.md"),
                "# Current behavior\nThe source is byte exact.\n",
            )
            .unwrap();
            fs::write(root.join("src/lib.rs"), "pub fn exact() -> bool { true }\n").unwrap();
            let git = git2::Repository::init(root).unwrap();
            let mut git_index = git.index().unwrap();
            git_index
                .add_path(std::path::Path::new("docs/current.md"))
                .unwrap();
            git_index
                .add_path(std::path::Path::new("src/lib.rs"))
                .unwrap();
            git_index.write().unwrap();
            let tree_oid = git_index.write_tree().unwrap();
            let tree = git.find_tree(tree_oid).unwrap();
            let signature =
                git2::Signature::now("SymForge fixture", "fixture@example.invalid").unwrap();
            git.commit(Some("HEAD"), &signature, &signature, "fixture", &tree, &[])
                .unwrap();
            let wire = tempfile::tempdir().unwrap();
            let wire_path = wire.path().join("curation-request.json");
            let child = crate::process_util::hidden_command(std::env::current_exe().unwrap())
                .arg("curation_completed_replay_survives_process_restart")
                .env("SYMFORGE_KNOWLEDGE_REPLAY_CHILD_ROOT", root)
                .env("SYMFORGE_KNOWLEDGE_REPLAY_REQUEST", &wire_path)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .expect("child test process");
            assert!(child.success(), "child failed to complete curation");
            let request: WireKnowledgeRequest =
                serde_json::from_slice(&fs::read(&wire_path).unwrap())
                    .expect("strict serialized curation request");
            let runtime = ProcessIndexRuntime::acquire().unwrap();
            let handle = runtime
                .open_embedded_source(EmbeddedSourceSpec::current_worktree(root.to_path_buf()))
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(15);
            while handle.runtime_view().phase != SourceRuntimePhase::Current {
                assert!(
                    Instant::now() < deadline,
                    "source did not publish after restart"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            let authority = EditApplyAuthority::for_source_root(
                fs::canonicalize(root).unwrap(),
                "fixture-room/knowledge-process".to_string(),
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();
            let WireKnowledgeReply::Curation(replayed) = handle
                .apply_wire_knowledge(&request, &authority, "fixture-knowledge-process-op")
                .expect("cross-process completed replay")
            else {
                panic!("expected curation reply");
            };
            assert_eq!(replayed.status, KnowledgeCurationStatus::Applied);
            assert!(replayed.refresh_ticket_identity.is_none());
            let policy = root.join(".symforge-knowledge.toml");
            assert!(policy.exists());
            fs::write(&policy, b"# changed policy image\n").unwrap();
            assert!(matches!(
                handle.apply_wire_knowledge(&request, &authority, "fixture-knowledge-process-op"),
                Err(WireKnowledgeError::Curation(
                    KnowledgeCurationError::ReplayConflict
                ))
            ));
            return;
        };

        let root = std::path::PathBuf::from(child_root);
        let runtime = ProcessIndexRuntime::acquire().unwrap();
        let handle = runtime
            .open_embedded_source(EmbeddedSourceSpec::current_worktree(root.clone()))
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while handle.runtime_view().phase != SourceRuntimePhase::Current {
            assert!(Instant::now() < deadline, "child source did not publish");
            std::thread::sleep(Duration::from_millis(10));
        }
        let snapshot = handle
            .capture_query_snapshot(b"curation-process-fixture")
            .unwrap();
        let plan = crate::knowledge::review::curation_plan_current(&snapshot.generation).unwrap();
        let reviewed = plan
            .actions
            .values()
            .find(|action| {
                action
                    .unmet_preconditions
                    .iter()
                    .all(|precondition| precondition == "requires_user_judgment")
            })
            .expect("approvable action");
        let range = reviewed
            .target
            .unit_byte_range
            .as_ref()
            .expect("unit range");
        let request = WireKnowledgeRequest::CurateApply {
            input: CurateKnowledgeInput {
                actions: vec![KnowledgePolicyActionInput {
                    action_id: reviewed.action_id.clone(),
                    mutation: KnowledgePolicyMutationInput::Upsert {
                        entry: KnowledgePolicyEntryInput {
                            entry_id: "entry-process-curation".to_string(),
                            target: KnowledgePolicyTargetInput {
                                path: reviewed.target.path.clone(),
                                content_hash: reviewed.target.content_hash.clone(),
                                unit_byte_range: Some([range.start, range.end]),
                                unit_hash: reviewed.target.unit_hash.clone(),
                            },
                            lifecycle: KnowledgePolicyLifecycleInput::Unknown,
                            authority_domain: None,
                            superseded_by: None,
                            evidence: Vec::new(),
                            justification_code: "approved-review".to_string(),
                        },
                    },
                }],
                if_source_review_hash: plan.review_hash,
                if_manifest_digest: plan.manifest_digest,
                if_policy_digest: plan.policy_digest,
                idempotency_key: Some("fixture-knowledge-process-op".to_string()),
                apply: true,
                project: None,
            },
            if_serving_publication_identity: snapshot.serving_publication_identity,
        };
        let authority = EditApplyAuthority::for_source_root(
            fs::canonicalize(&root).unwrap(),
            "fixture-room/knowledge-process".to_string(),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        let WireKnowledgeReply::Curation(applied) = handle
            .apply_wire_knowledge(&request, &authority, "fixture-knowledge-process-op")
            .expect("child curation apply")
        else {
            panic!("expected curation reply");
        };
        assert_eq!(applied.status, KnowledgeCurationStatus::Applied);
        fs::write(
            std::env::var("SYMFORGE_KNOWLEDGE_REPLAY_REQUEST").unwrap(),
            serde_json::to_vec(&request).unwrap(),
        )
        .expect("persist strict request fixture");
    }

    #[test]
    fn copied_curation_state_cannot_replay_under_replaced_worktree_root() {
        fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
            fs::create_dir_all(to).unwrap();
            for entry in fs::read_dir(from).unwrap() {
                let entry = entry.unwrap();
                let name = entry.file_name();
                if name == ".git" {
                    continue;
                }
                let destination = to.join(name);
                if entry.file_type().unwrap().is_dir() {
                    copy_tree(&entry.path(), &destination);
                } else {
                    fs::copy(entry.path(), destination).unwrap();
                }
            }
        }

        let outer = tempfile::tempdir().unwrap();
        let root = outer.path().join("source");
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("docs")).unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("docs/current.md"), "# Current behavior\nThe source is byte exact.\n").unwrap();
        fs::write(root.join("src/lib.rs"), "pub fn exact() -> bool { true }\n").unwrap();
        {
            let git = git2::Repository::init(&root).unwrap();
            let mut git_index = git.index().unwrap();
            git_index.add_path(std::path::Path::new("docs/current.md")).unwrap();
            git_index.add_path(std::path::Path::new("src/lib.rs")).unwrap();
            git_index.write().unwrap();
            let tree_oid = git_index.write_tree().unwrap();
            let tree = git.find_tree(tree_oid).unwrap();
            let signature = git2::Signature::now("SymForge fixture", "fixture@example.invalid").unwrap();
            git.commit(Some("HEAD"), &signature, &signature, "fixture", &tree, &[]).unwrap();
        }
        let wire_path = outer.path().join("curation-request.json");
        let child = crate::process_util::hidden_command(std::env::current_exe().unwrap())
            .arg("curation_completed_replay_survives_process_restart")
            .env("SYMFORGE_KNOWLEDGE_REPLAY_CHILD_ROOT", &root)
            .env("SYMFORGE_KNOWLEDGE_REPLAY_REQUEST", &wire_path)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(child.success(), "initial process did not complete curation");
        let request: WireKnowledgeRequest = serde_json::from_slice(&fs::read(&wire_path).unwrap()).unwrap();
        let displaced = outer.path().join("original-source");
        fs::rename(&root, &displaced).unwrap();
        copy_tree(&displaced, &root);
        fs::write(root.join(".git"), format!("gitdir: {}\n", displaced.join(".git").display())).unwrap();
        let repository = git2::Repository::open(&root).expect("replacement reuses the original gitdir");
        assert_eq!(
            fs::canonicalize(repository.path()).unwrap(),
            fs::canonicalize(displaced.join(".git")).unwrap()
        );

        let runtime = ProcessIndexRuntime::acquire().unwrap();
        let handle = runtime
            .open_embedded_source(EmbeddedSourceSpec::current_worktree(root.clone()))
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while handle.runtime_view().phase != SourceRuntimePhase::Current {
            assert!(Instant::now() < deadline, "replacement source did not publish");
            std::thread::sleep(Duration::from_millis(10));
        }
        let authority = EditApplyAuthority::for_source_root(
            fs::canonicalize(&root).unwrap(),
            "fixture-room/knowledge-process".to_string(),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        let result = handle.apply_wire_knowledge(&request, &authority, "fixture-knowledge-process-op");
        assert!(
            matches!(result, Err(WireKnowledgeError::Curation(KnowledgeCurationError::ReplayConflict))),
            "copied curation state was accepted for a different physical root"
        );
        handle.close().unwrap();
    }

    #[test]
    fn curation_receipt_refuses_secret_shaped_policy_text_without_echo() {
        let marker = ["-----BEGIN ", "PRIVATE KEY", "-----"].concat();
        let rendered = format!("status=preview\npre_image={marker}\n");
        assert!(crate::knowledge::guard_query(&rendered).is_err());
        assert!(matches!(
            checked_curation_receipt(rendered, KnowledgeCurationStatus::Preview),
            Err(KnowledgeCurationError::EngineRejected)
        ));
    }

    #[test]
    fn bound_curation_previews_then_applies_with_source_authority() {
        let repository = tempfile::tempdir().expect("temporary repository");
        let root = repository.path();
        fs::create_dir_all(root.join("docs")).expect("docs directory");
        fs::create_dir_all(root.join("src")).expect("source directory");
        fs::write(
            root.join("docs/current.md"),
            "# Current behavior\nThe source is byte exact.\n",
        )
        .expect("knowledge source");
        fs::write(root.join("src/lib.rs"), "pub fn exact() -> bool { true }\n")
            .expect("code source");
        let git = git2::Repository::init(root).expect("initialize repository");
        let mut git_index = git.index().expect("git index");
        git_index
            .add_path(std::path::Path::new("docs/current.md"))
            .expect("stage knowledge");
        git_index
            .add_path(std::path::Path::new("src/lib.rs"))
            .expect("stage code");
        git_index.write().expect("write git index");
        let tree_oid = git_index.write_tree().expect("write git tree");
        let tree = git.find_tree(tree_oid).expect("git tree");
        let signature = git2::Signature::now("SymForge fixture", "fixture@example.invalid")
            .expect("fixture signature");
        git.commit(Some("HEAD"), &signature, &signature, "fixture", &tree, &[])
            .expect("fixture commit");
        let runtime = ProcessIndexRuntime::acquire().expect("runtime");
        let handle = runtime
            .open_embedded_source(EmbeddedSourceSpec::current_worktree(root.to_path_buf()))
            .expect("bound source");
        let deadline = Instant::now() + Duration::from_secs(15);
        while handle.runtime_view().phase != SourceRuntimePhase::Current {
            assert!(Instant::now() < deadline, "source did not become current");
            std::thread::sleep(Duration::from_millis(10));
        }
        let snapshot = handle
            .capture_query_snapshot(b"curation-fixture")
            .expect("snapshot");
        let plan = crate::knowledge::review::curation_plan_current(&snapshot.generation)
            .expect("curation plan");
        let reviewed = plan
            .actions
            .values()
            .find(|action| {
                action
                    .unmet_preconditions
                    .iter()
                    .all(|precondition| precondition == "requires_user_judgment")
            })
            .expect("approvable action");
        let range = reviewed
            .target
            .unit_byte_range
            .as_ref()
            .expect("unit range");
        let target = KnowledgePolicyTargetInput {
            path: reviewed.target.path.clone(),
            content_hash: reviewed.target.content_hash.clone(),
            unit_byte_range: Some([range.start, range.end]),
            unit_hash: reviewed.target.unit_hash.clone(),
        };
        let input = CurateKnowledgeInput {
            actions: vec![KnowledgePolicyActionInput {
                action_id: reviewed.action_id.clone(),
                mutation: KnowledgePolicyMutationInput::Upsert {
                    entry: KnowledgePolicyEntryInput {
                        entry_id: "entry-embed-curation".to_string(),
                        target,
                        lifecycle: KnowledgePolicyLifecycleInput::Unknown,
                        authority_domain: None,
                        superseded_by: None,
                        evidence: Vec::new(),
                        justification_code: "approved-review".to_string(),
                    },
                },
            }],
            if_source_review_hash: plan.review_hash,
            if_manifest_digest: plan.manifest_digest,
            if_policy_digest: plan.policy_digest,
            idempotency_key: None,
            apply: false,
            project: None,
        };
        let mut request = BoundCurateKnowledgeInput {
            input,
            if_serving_publication_identity: snapshot.serving_publication_identity.clone(),
        };
        let preview = handle.curate_knowledge(&request, None).expect("preview");
        assert_eq!(preview.status, KnowledgeCurationStatus::Preview);
        assert!(!root.join(".symforge-knowledge.toml").exists());

        request.input.apply = true;
        request.input.idempotency_key = Some("fixture-curation-op".to_string());
        let authority = EditApplyAuthority::for_source_root(
            fs::canonicalize(root).expect("canonical root"),
            "fixture-room/source".to_string(),
            Arc::new(AtomicBool::new(false)),
        )
        .expect("host authority");
        let current_serving_identity = request.if_serving_publication_identity.clone();
        request.if_serving_publication_identity = "obsolete-serving-publication".to_string();
        assert!(matches!(
            handle.curate_knowledge(&request, Some(&authority)),
            Err(KnowledgeCurationError::StalePublication)
        ));
        assert!(!root.join(".symforge-knowledge.toml").exists());
        request.if_serving_publication_identity = current_serving_identity;
        let applied = handle
            .curate_knowledge(&request, Some(&authority))
            .expect("apply");
        assert_eq!(applied.status, KnowledgeCurationStatus::Applied);
        assert!(applied.refresh_ticket_identity.is_some());
        assert!(root.join(".symforge-knowledge.toml").exists());
    }
}
