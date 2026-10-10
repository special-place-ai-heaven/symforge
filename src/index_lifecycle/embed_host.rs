//! Source-proofed lifecycle projections for the transport-neutral host facade.

use std::sync::Arc;

use crate::domain::{CapabilityStatus, SourceAccessMode, StatePlacement};
use crate::embed::parity::host::{
    HostArtifactOutcome, HostCheckpointOutcome, HostCheckpointReceipt, HostEngineIdentity,
    HostHealth, HostHealthNotApplicable, HostHealthProjection, HostHealthReport, HostHealthRequest,
    HostLimits, HostPhase, HostProgress, HostPublishedHealth, HostRefusal, HostRefusalKind,
    HostSourceProof, HostStateLocation, HostStatus, HostVerifyPhase, OperationControl, map_stop,
};
use crate::live_index::store::SnapshotVerifyState;

use super::embedded::EmbeddedSourceHandle;
use super::public_api::SourceRuntimePhase;

pub(crate) fn status(
    source: &EmbeddedSourceHandle,
    room_id: &str,
    engine: &HostEngineIdentity,
) -> HostStatus {
    let view = source.runtime_view();
    let progress = source.index_progress();
    HostStatus {
        room_id: room_id.to_owned(),
        engine: engine.clone(),
        phase: match view.phase {
            SourceRuntimePhase::Blocked => HostPhase::Blocked,
            SourceRuntimePhase::Current => HostPhase::Current,
            SourceRuntimePhase::Loading => HostPhase::Loading,
            SourceRuntimePhase::Refreshing => HostPhase::Refreshing,
            SourceRuntimePhase::Stopped => HostPhase::Stopped,
            SourceRuntimePhase::Stopping => HostPhase::Stopping,
        },
        binding_identity: view.binding_identity,
        source_capture_publication_identity: view.current_publication_identity,
        source_version: view.source_version,
        observer_epoch: view.observer_epoch,
        progress: HostProgress {
            files_discovered: progress.files_discovered,
            files_parsed: progress.files_parsed,
            symbols_found: progress.symbols_found,
        },
    }
}

pub(crate) fn health(
    source: &EmbeddedSourceHandle,
    room_id: &str,
    engine: &HostEngineIdentity,
) -> HostHealth {
    let status = status(source, room_id, engine);
    let current = if status.phase == HostPhase::Current {
        source
            .capture_curation_context(b"embed-host-health")
            .ok()
            .and_then(|(snapshot, _, placement)| {
                let proof = proof(&snapshot).ok()?;
                let health = &snapshot.generation.health;
                let location = state_location(&placement);
                Some(HostPublishedHealth {
                    proof,
                    state_location: location,
                    files: health.file_count as u64,
                    symbols: health.symbol_count as u64,
                    parsed: health.parsed_count as u64,
                    partial_parse: health.partial_parse_count as u64,
                    failed_parse: health.failed_count as u64,
                    load_source: health.load_source.label().to_owned(),
                    snapshot_verify: match &health.snapshot_verify_state {
                        SnapshotVerifyState::NotNeeded => HostVerifyPhase::NotNeeded,
                        SnapshotVerifyState::Pending => HostVerifyPhase::Pending,
                        SnapshotVerifyState::Running(_) => HostVerifyPhase::Running,
                        SnapshotVerifyState::Completed(_) => HostVerifyPhase::Completed,
                        SnapshotVerifyState::Failed(_) => HostVerifyPhase::Failed,
                    },
                })
            })
    } else {
        None
    };
    HostHealth { status, current }
}

/// Health sections the MCP server observes from its own process. An embedded
/// host has none of these processes; each is reported with its reason.
const EMBEDDED_PROCESS_SECTIONS: &[(&str, &str)] = &[
    (
        "daemon_degradation",
        "no daemon: the embedded host serves its source in-process",
    ),
    (
        "project_mismatch",
        "no caller-declared MCP root: the host binds its source explicitly",
    ),
    ("sidecar", "embedded hosts run no hook sidecar process"),
    (
        "token_savings",
        "hook token savings are recorded by the sidecar process",
    ),
    (
        "tool_call_counts",
        "per-tool call counts are recorded by the sidecar process",
    ),
    (
        "hook_adoption",
        "hook adoption describes MCP client hook installs, not a library host",
    ),
];
const EMBEDDED_WORKTREE: (&str, &str) = (
    "worktree_misuse",
    "embedded edit requests carry no working_directory",
);
const EMBEDDED_BINARY_SECTIONS: &[(&str, &str)] = &[
    (
        "version_drift",
        "version drift compares installed server binaries",
    ),
    (
        "path_shadow",
        "PATH shadow compares installed server binaries",
    ),
];
const EMBEDDED_WATCHER: (&str, &str) = (
    "filesystem_watcher",
    "no notify watcher is attached; the embedded observer polls the source tree and republishes on change",
);

/// MCP `health` / `health_compact` parity: the typed health plus the report
/// rendered by the same shared assembler the MCP handlers call.
pub(crate) fn health_report(
    source: &EmbeddedSourceHandle,
    room_id: &str,
    engine: &HostEngineIdentity,
    projection: HostHealthProjection,
    request: &HostHealthRequest,
) -> HostHealthReport {
    use super::guidance::health as shared;

    let health = health(source, room_id, engine);
    let mut not_applicable: Vec<HostHealthNotApplicable> = EMBEDDED_PROCESS_SECTIONS
        .iter()
        .chain(std::iter::once(&EMBEDDED_WORKTREE))
        .chain(EMBEDDED_BINARY_SECTIONS)
        .chain(std::iter::once(&EMBEDDED_WATCHER))
        .map(|(section, reason)| HostHealthNotApplicable {
            section: (*section).to_owned(),
            reason: (*reason).to_owned(),
        })
        .collect();
    let report = source
        .capture_health_context()
        .map(|(shared_index, root, placement)| {
            // T046: one capture of the published set for the whole report.
            let source_set = shared_index.published_source_set();
            let generation = source_set.current_generation();
            let runtime_status = shared::RuntimeStatus {
                mode: shared::RuntimeMode::Embedded,
                project_root: Some(crate::paths::normalized_path_string(&root)),
                project_id: crate::discovery::project_id_for_canonical_root(&root).0,
                session_id: format!("embedded-room-{room_id}"),
                index_generation: generation.health.generation,
                project_generation: generation.project_generation,
                reset_project_generation: shared_index.current_reset_project_generation(),
                load_source: generation.health.load_source,
            };
            let persistence = if placement.directory().is_some() {
                CapabilityStatus::Available
            } else {
                CapabilityStatus::Unavailable {
                    reason: crate::domain::CapabilityUnavailableReason::PersistentStateUnavailable,
                }
            };
            let watcher = crate::watcher_state::WatcherInfo::default();
            let inputs =
                shared::HealthReportInputs {
                    source_set: &source_set,
                    watcher: &watcher,
                    rejected_stale_mutations: shared_index.current_rejected_stale_mutations(),
                    runtime_status: &runtime_status,
                    repo_root: Some(&root),
                    placement: Some(&placement),
                    persistence,
                    session_is_daemon: false,
                    worktree_routing: format!("not_applicable ({})", EMBEDDED_WORKTREE.1),
                    curation_health:
                        crate::knowledge::curation::KnowledgeCurationCoordinator::default()
                            .health_line(&shared_index, Some(&root), Some(&placement), persistence),
                };
            let not_applicable_lines = |sections: &[(&str, &str)]| {
                sections
                    .iter()
                    .map(|(section, reason)| {
                        format!(
                            "
{section}: not_applicable ({reason})"
                        )
                    })
                    .collect::<String>()
            };
            let process = shared::HealthProcessSections {
                after_runtime: not_applicable_lines(EMBEDDED_PROCESS_SECTIONS),
                worktree_misuse: Err(EMBEDDED_WORKTREE.1),
                trailing: not_applicable_lines(EMBEDDED_BINARY_SECTIONS),
            };
            match projection {
                HostHealthProjection::Full => shared::render_health_report(
                    &inputs,
                    shared::QuarantineWindow::from_args(
                        request.quarantine_offset.map(|n| n as usize),
                        request.quarantine_limit.map(|n| n as usize),
                    ),
                    &process,
                ),
                HostHealthProjection::Compact => shared::render_health_compact(&inputs, &process),
            }
        });
    if report.is_none() {
        not_applicable.push(HostHealthNotApplicable {
            section: "report".to_owned(),
            reason: "the source is closed; no bound data plane was captured".to_owned(),
        });
    }
    HostHealthReport {
        health,
        projection,
        report,
        not_applicable,
    }
}

fn state_location(placement: &StatePlacement) -> HostStateLocation {
    match placement {
        StatePlacement::ProjectLocal { .. } => HostStateLocation::ProjectLocal,
        StatePlacement::UserLocal { .. } => HostStateLocation::UserLocal,
        StatePlacement::MemoryOnly { .. } => HostStateLocation::MemoryOnly,
    }
}

fn proof(
    snapshot: &super::embed_query::EmbeddedQuerySnapshot,
) -> Result<HostSourceProof, HostRefusal> {
    let envelope = snapshot
        .generation
        .source_response_envelope()
        .ok_or_else(|| HostRefusal::new(HostRefusalKind::SourceUnavailable))?;
    let source_bytes = serde_json::to_vec(&envelope.source)
        .map_err(|_| HostRefusal::new(HostRefusalKind::EngineFailure))?;
    let version_bytes = serde_json::to_vec(&envelope.source_version)
        .map_err(|_| HostRefusal::new(HostRefusalKind::EngineFailure))?;
    Ok(HostSourceProof {
        binding_identity: snapshot.binding_identity.clone(),
        source_capture_publication_identity: snapshot.publication_identity.clone(),
        source_version: snapshot.source_version,
        publication_generation: envelope.publication_generation,
        content_generation: envelope.content_generation,
        source_identity_digest: crate::hash::digest_hex(&source_bytes),
        source_version_digest: crate::hash::digest_hex(&version_bytes),
        manifest_digest: envelope.manifest_digest,
    })
}

pub(crate) fn checkpoint(
    source: &EmbeddedSourceHandle,
    limits: HostLimits,
    verify_after_write: bool,
    export_artifact: bool,
    control: &OperationControl,
) -> Result<HostCheckpointReceipt, HostRefusal> {
    control.check().map_err(map_stop)?;
    let (snapshot, shared, placement) =
        source
            .capture_curation_context(b"embed-host-checkpoint")
            .map_err(|_| HostRefusal::new(HostRefusalKind::SourceUnavailable))?;
    if placement.directory().is_none() {
        return Err(HostRefusal::new(HostRefusalKind::PersistenceUnavailable));
    }
    let file_count = snapshot.generation.live.files.len() as u64;
    let byte_count = snapshot
        .generation
        .live
        .files
        .values()
        .try_fold(0_u64, |sum, file| {
            sum.checked_add(file.content.len() as u64)
        })
        .ok_or_else(|| HostRefusal::new(HostRefusalKind::ResourceLimit))?;
    if file_count > limits.max_indexed_files || byte_count > limits.max_indexed_bytes {
        return Err(HostRefusal::new(HostRefusalKind::ResourceLimit));
    }
    let captured_proof = proof(&snapshot)?;
    if !Arc::ptr_eq(&shared.published_generation(), &snapshot.generation) {
        return Err(HostRefusal::new(HostRefusalKind::SourceUnavailable));
    }
    control.check().map_err(map_stop)?;

    // The existing MCP writer snapshots one published generation atomically.
    // It may capture a successor if publication moves just before its read;
    // below we refuse to label that write with the earlier source proof.
    let written = match crate::live_index::persist::checkpoint_shared_index(
        &shared,
        &snapshot.root,
        &placement,
    ) {
        Ok(report) => report,
        Err(_) => {
            return Ok(HostCheckpointReceipt {
                outcome: HostCheckpointOutcome::Uncertain,
                written_files: 0,
                written_bytes: 0,
                artifact: HostArtifactOutcome::NotRequested,
                proof: None,
                completed_after_deadline: control.remaining().is_zero(),
            });
        }
    };
    let mut receipt = HostCheckpointReceipt {
        outcome: HostCheckpointOutcome::WrittenCurrent,
        written_files: written.files as u64,
        written_bytes: written.bytes as u64,
        artifact: HostArtifactOutcome::NotRequested,
        proof: Some(captured_proof),
        completed_after_deadline: control.remaining().is_zero(),
    };
    if !same_publication(source, &shared, &snapshot) {
        receipt.outcome = HostCheckpointOutcome::WrittenButSourceMoved;
        receipt.proof = None;
        return Ok(receipt);
    }
    if verify_after_write {
        let expected = snapshot
            .generation
            .source_response_envelope()
            .ok_or_else(|| HostRefusal::new(HostRefusalKind::SourceUnavailable))?;
        let verified = crate::live_index::persist::load_snapshot(&snapshot.root, &placement)
            .is_some_and(|loaded| {
                loaded.manifest.digest == expected.manifest_digest
                    && loaded.source_identity.source_id == expected.source.source_id
                    && loaded.source_identity.repository_id == expected.source.repository_id
                    && loaded.source_identity.source_version == expected.source_version
            });
        if !verified {
            receipt.outcome = HostCheckpointOutcome::VerificationFailed;
            receipt.proof = None;
            return Ok(receipt);
        }
    }
    if export_artifact {
        receipt.artifact = if matches!(placement, StatePlacement::ProjectLocal { .. }) {
            match crate::live_index::persist::export_artifact(
                snapshot.generation.live.as_ref(),
                &snapshot.root,
                SourceAccessMode::NormalProject,
                &placement,
                &CapabilityStatus::Available,
            ) {
                Ok(report) => HostArtifactOutcome::Written {
                    compressed_bytes: report.compressed_bytes as u64,
                },
                Err(_) => HostArtifactOutcome::Failed,
            }
        } else {
            HostArtifactOutcome::Unavailable
        };
    }
    if !same_publication(source, &shared, &snapshot) {
        receipt.outcome = HostCheckpointOutcome::WrittenButSourceMoved;
        receipt.proof = None;
    }
    receipt.completed_after_deadline = control.remaining().is_zero();
    Ok(receipt)
}

fn same_publication(
    source: &EmbeddedSourceHandle,
    shared: &crate::live_index::store::SharedIndex,
    snapshot: &super::embed_query::EmbeddedQuerySnapshot,
) -> bool {
    let view = source.runtime_view();
    Arc::ptr_eq(&shared.published_generation(), &snapshot.generation)
        && view.phase == SourceRuntimePhase::Current
        && view.current_publication_identity.as_deref()
            == Some(snapshot.publication_identity.as_str())
}
