//! Preview-first, ledger-only repository knowledge curation.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::ops::Range;
use std::path::{Component, Path, PathBuf};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::domain::{
    CapabilityStatus, CapabilityUnavailableReason, RepositoryId, SourceId, SourceLocation,
    StatePlacement, UserLocalPlacementReason,
};
use crate::knowledge::guard_query;
use crate::live_index::SharedIndex;
use crate::live_index::knowledge_authority::{
    AuthorityDomain, KnowledgeLifecycle, KnowledgePolicy, KnowledgePolicyEntry,
    KnowledgePolicyTarget, PolicyEvidenceRef, RemediationAction, parse_knowledge_policy,
};
use crate::live_index::knowledge_bridge::CodeAnchorId;

use super::knowledge_review::{CurationReviewPlan, curation_plan_current};
use super::search_tools::{
    CurateKnowledgeInput, KnowledgePolicyActionInput, KnowledgePolicyAuthorityDomainInput,
    KnowledgePolicyEntryInput, KnowledgePolicyEvidenceInput, KnowledgePolicyLifecycleInput,
    KnowledgePolicyMutationInput, KnowledgePolicyTargetInput,
};

const POLICY_FILE: &str = ".symforge-knowledge.toml";
const CURATION_STATE_DIR: &str = "curation";
const REPLAY_DIR: &str = "replay";
const QUARANTINE_DIR: &str = "quarantine";
const LOCK_FILE: &str = "policy.lock";
const RECORD_VERSION: u32 = 1;
const POLICY_VERSION: u32 = 1;
const MAX_ACTIONS: usize = 100;
const MAX_INPUT_BYTES: usize = 1_048_576;
const MAX_POLICY_BYTES: usize = 1_048_576;
const MAX_STRING_BYTES: usize = 4_096;
const MAX_TARGET_BYTES: u64 = 16 * 1_048_576;
const MAX_RECOVERY_RECORDS: usize = 1_024;

#[derive(Debug, Default)]
pub(crate) struct KnowledgeCurationCoordinator {
    mutation_lock: Mutex<()>,
    probe_cache: Mutex<BTreeMap<PathBuf, Result<(), CapabilityUnavailableReason>>>,
    #[cfg(test)]
    probe_operations: Mutex<Vec<PathBuf>>,
    #[cfg(test)]
    probe_failures: Mutex<BTreeSet<PathBuf>>,
    #[cfg(test)]
    failpoint: Mutex<Option<CurationWriteStage>>,
    #[cfg(test)]
    temp_corruption: Mutex<Option<Vec<u8>>>,
    #[cfg(test)]
    interposed_policy_bytes: Mutex<Option<Vec<u8>>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
enum CurationWriteStage {
    AfterReservation,
    AfterPendingIntentSync,
    AfterTempWrite,
    AfterTempSync,
    AfterAtomicReplace,
    AfterParentDurability,
    AfterResultSync,
}

#[derive(Clone, Debug)]
struct PreparedMutation {
    pre_image: Vec<u8>,
    post_image: Vec<u8>,
    pre_digest: String,
    post_digest: String,
    diff: String,
    publication_generation: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct CurationSourceBinding {
    repository_id: RepositoryId,
    source_id: SourceId,
    continuity: CurationContinuityProof,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
enum CurationContinuityProof {
    Git {
        object_format: String,
        anchor_tip_object_id: String,
        git_directory_object_identity: String,
    },
    NonGit {
        root_object_identity: String,
        catalog_identity_digest: String,
        publication_generation: u64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ReplayRecord {
    version: u32,
    key_digest: String,
    request_hash: String,
    binding: CurationSourceBinding,
    request: CurateKnowledgeInput,
    state: ReplayState,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum ReplayState {
    Reserved,
    PendingWrite {
        pre_image: Vec<u8>,
        post_image: Vec<u8>,
        receipt: StoredReceipt,
    },
    Succeeded {
        receipt: StoredReceipt,
    },
    Failed {
        output: String,
    },
    Indeterminate {
        output: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct StoredReceipt {
    request_hash: String,
    pre_digest: String,
    post_digest: String,
    publication_generation: u64,
    diff: String,
}

impl StoredReceipt {
    fn render(&self) -> String {
        format!(
            "status=applied\nrequest_hash={}\npre_policy_digest={}\npost_policy_digest={}\npublication_generation={} publication_status=pending\n{}",
            self.request_hash,
            self.pre_digest,
            self.post_digest,
            self.publication_generation,
            self.diff,
        )
    }
}

impl KnowledgeCurationCoordinator {
    pub(crate) fn execute(
        &self,
        index: &SharedIndex,
        repo_root: &Path,
        state_placement: Option<&StatePlacement>,
        persistence_health: CapabilityStatus,
        input: &CurateKnowledgeInput,
    ) -> String {
        if let Err(error) = validate_input(input) {
            return error;
        }
        if !input.apply {
            return self.preview(index, repo_root, input);
        }
        self.apply(index, repo_root, state_placement, persistence_health, input)
    }

    pub(crate) fn health_line(
        &self,
        index: &SharedIndex,
        repo_root: Option<&Path>,
        state_placement: Option<&StatePlacement>,
        persistence_health: CapabilityStatus,
    ) -> String {
        let capability =
            self.capability_status(index, repo_root, state_placement, persistence_health);

        let mut replay_records = 0usize;
        let mut pending = 0usize;
        let mut indeterminate = 0usize;
        let mut malformed = 0usize;
        let mut quarantined = 0usize;
        if let Some(state_dir) = state_placement.and_then(StatePlacement::directory) {
            let curation_dir = state_dir.as_path().join(CURATION_STATE_DIR);
            let replay_dir = curation_dir.join(REPLAY_DIR);
            if let Ok(entries) = fs::read_dir(replay_dir) {
                for path in entries
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .filter(|path| {
                        path.extension().and_then(|value| value.to_str()) == Some("json")
                    })
                    .take(MAX_RECOVERY_RECORDS + 1)
                {
                    replay_records += 1;
                    match read_replay_record(&path) {
                        Ok(Some(record)) => match record.state {
                            ReplayState::Reserved | ReplayState::PendingWrite { .. } => {
                                pending += 1
                            }
                            ReplayState::Indeterminate { .. } => indeterminate += 1,
                            ReplayState::Succeeded { .. } | ReplayState::Failed { .. } => {}
                        },
                        Ok(None) => {}
                        Err(_) => malformed += 1,
                    }
                }
            }
            if let Ok(entries) = fs::read_dir(curation_dir.join(QUARANTINE_DIR)) {
                quarantined = entries
                    .filter_map(Result::ok)
                    .take(MAX_RECOVERY_RECORDS + 1)
                    .count();
            }
        }
        let recovery = if malformed > 0 {
            "malformed_replay_record"
        } else if indeterminate > 0 {
            "indeterminate_conflict"
        } else if pending > 0 {
            "pending_write"
        } else {
            "clean"
        };
        match capability {
            CapabilityStatus::Available => format!(
                "Knowledge curation: capability=available replay_records={replay_records} pending={pending} indeterminate={indeterminate} quarantined={quarantined} recovery={recovery}"
            ),
            CapabilityStatus::Unavailable { reason } => format!(
                "Knowledge curation: capability=unavailable reason={} replay_records={replay_records} pending={pending} indeterminate={indeterminate} quarantined={quarantined} recovery={recovery}",
                capability_reason_code(reason)
            ),
        }
    }

    pub(crate) fn capability_status(
        &self,
        index: &SharedIndex,
        repo_root: Option<&Path>,
        state_placement: Option<&StatePlacement>,
        persistence_health: CapabilityStatus,
    ) -> CapabilityStatus {
        let generation = index.published_source_set().current_generation();
        let source_location = generation.source.as_ref().map(|source| &source.location);
        match (repo_root, source_location) {
            (Some(root), Some(location)) => {
                match apply_capability(root, state_placement, persistence_health, location) {
                    Ok(state_dir) => {
                        let curation_dir = state_dir.join(CURATION_STATE_DIR);
                        let mut directories = vec![root.to_path_buf()];
                        if curation_dir != root {
                            directories.push(curation_dir);
                        }
                        let cache = self.probe_cache.lock();
                        if directories.into_iter().all(|directory| {
                            let canonical = dunce::canonicalize(&directory).unwrap_or(directory);
                            matches!(cache.get(&canonical), Some(Ok(())))
                        }) {
                            CapabilityStatus::Available
                        } else {
                            CapabilityStatus::Unavailable {
                                reason: CapabilityUnavailableReason::AtomicDurabilityUnavailable,
                            }
                        }
                    }
                    Err(reason) => CapabilityStatus::Unavailable { reason },
                }
            }
            _ => CapabilityStatus::Unavailable {
                reason: CapabilityUnavailableReason::PersistentStateUnavailable,
            },
        }
    }

    /// Recover durable pending writes as part of binding a project, before
    /// that project begins serving tool calls. Failures remain represented in
    /// the replay store and health surface; callers may keep read-only service
    /// available while curation stays fail-closed.
    pub(crate) fn recover_on_project_load(
        &self,
        index: &SharedIndex,
        repo_root: &Path,
        state_placement: Option<&StatePlacement>,
        persistence_health: CapabilityStatus,
    ) -> Result<(), String> {
        let generation = index.published_source_set().current_generation();
        let plan = curation_plan_current(&generation)?;
        let state_dir = apply_capability(
            repo_root,
            state_placement,
            persistence_health,
            &plan.source.location,
        )
        .map_err(unavailable)?;
        let curation_dir = state_dir.join(CURATION_STATE_DIR);
        let replay_dir = curation_dir.join(REPLAY_DIR);
        if !replay_dir.is_dir() {
            return Ok(());
        }
        self.probe_apply_directories(repo_root, &curation_dir)
            .map_err(unavailable)?;

        let _in_process_guard = self.mutation_lock.lock();
        let lock_file = open_and_lock(&curation_dir.join(LOCK_FILE))
            .map_err(|error| durable_state_error(&error))?;
        let result = (|| {
            let generation = index.published_source_set().current_generation();
            let plan = curation_plan_current(&generation)?;
            self.recover_pending_records(repo_root, &curation_dir, &replay_dir, &generation, &plan)
        })();
        let unlock_result = unlock_file(&lock_file).map_err(|error| durable_state_error(&error));
        result?;
        unlock_result?;
        index.reload(repo_root).map_err(|_| {
            "Error: curation_startup_publication_failed; live queries remain on the last complete generation."
                .to_string()
        })?;
        Ok(())
    }

    fn preview(
        &self,
        index: &SharedIndex,
        repo_root: &Path,
        input: &CurateKnowledgeInput,
    ) -> String {
        let generation = index.published_source_set().current_generation();
        match prepare_mutation(&generation, repo_root, input) {
            Ok(prepared) => format!(
                "status=preview\nrequest_hash={}\npre_policy_digest={}\npost_policy_digest={}\npublication_generation={} publication_status=not_requested\n{}",
                canonical_request_hash(input),
                prepared.pre_digest,
                prepared.post_digest,
                prepared.publication_generation,
                prepared.diff,
            ),
            Err(error) => error,
        }
    }

    fn apply(
        &self,
        index: &SharedIndex,
        repo_root: &Path,
        state_placement: Option<&StatePlacement>,
        persistence_health: CapabilityStatus,
        input: &CurateKnowledgeInput,
    ) -> String {
        let request_hash = canonical_request_hash(input);
        let key = input
            .idempotency_key
            .as_deref()
            .expect("validated apply has an idempotency key");
        let key_digest = domain_digest("knowledge-curation-key-v1", key.as_bytes());
        let generation = index.published_source_set().current_generation();
        let plan = match curation_plan_current(&generation) {
            Ok(plan) => plan,
            Err(error) => return error,
        };
        let state_dir = match apply_capability(
            repo_root,
            state_placement,
            persistence_health,
            &plan.source.location,
        ) {
            Ok(state_dir) => state_dir,
            Err(reason) => return unavailable(reason),
        };
        let curation_dir = state_dir.join(CURATION_STATE_DIR);
        let replay_dir = curation_dir.join(REPLAY_DIR);
        let record_path = replay_dir.join(format!("{key_digest}.json"));

        if let Some(record) = match read_replay_record(&record_path) {
            Ok(record) => record,
            Err(error) => return error,
        } {
            // Pre-lock fast path is strictly read-only: any binding outcome
            // that would quarantine the record or append catalog lineage is
            // deferred to the locked path below.
            if verify_binding(repo_root, &curation_dir, &record.binding, &plan, false).is_ok() {
                if record.request_hash != request_hash {
                    return "Error: idempotency_conflict; the key is already bound to a different canonical request."
                        .to_string();
                }
                match &record.state {
                    ReplayState::Succeeded { receipt } => return receipt.render(),
                    ReplayState::Failed { output } | ReplayState::Indeterminate { output } => {
                        return output.clone();
                    }
                    ReplayState::Reserved | ReplayState::PendingWrite { .. } => {}
                }
            }
        }

        if let Err(reason) = self.probe_apply_directories(repo_root, &curation_dir) {
            return unavailable(reason);
        }

        let _in_process_guard = self.mutation_lock.lock();
        if let Err(error) = fs::create_dir_all(&replay_dir) {
            return durable_state_error(&error);
        }
        let lock_path = curation_dir.join(LOCK_FILE);
        let lock_file = match open_and_lock(&lock_path) {
            Ok(file) => file,
            Err(error) => return durable_state_error(&error),
        };

        let output = (|| {
            let mut generation = index.published_source_set().current_generation();
            let mut plan = curation_plan_current(&generation)?;
            if let Some(record) = read_replay_record(&record_path)? {
                verify_record_binding(repo_root, &curation_dir, &record_path, &record, &plan)?;
            }
            self.recover_pending_records(
                repo_root,
                &curation_dir,
                &replay_dir,
                &generation,
                &plan,
            )?;
            generation = index.published_source_set().current_generation();
            plan = curation_plan_current(&generation)?;
            let mut record = if let Some(record) = read_replay_record(&record_path)? {
                verify_record_binding(repo_root, &curation_dir, &record_path, &record, &plan)?;
                if record.request_hash != request_hash {
                    return Ok("Error: idempotency_conflict; the key is already bound to a different canonical request."
                        .to_string());
                }
                if !matches!(record.state, ReplayState::Reserved) {
                    return Ok(self.handle_existing_record(
                        repo_root,
                        &curation_dir,
                        &record_path,
                        record,
                        &generation,
                        &plan,
                        &request_hash,
                    ));
                }
                record
            } else {
                let binding = capture_binding(repo_root, &plan)?;
                let mut stored_request = input.clone();
                stored_request.idempotency_key = None;
                stored_request.project = None;
                let record = ReplayRecord {
                    version: RECORD_VERSION,
                    key_digest: key_digest.clone(),
                    request_hash: request_hash.clone(),
                    binding,
                    request: stored_request,
                    state: ReplayState::Reserved,
                };
                write_replay_record(&record_path, &record)?;
                self.maybe_fail(CurationWriteStage::AfterReservation)?;
                record
            };

            let prepared = match prepare_mutation(&generation, repo_root, input) {
                Ok(prepared) => prepared,
                Err(error) => {
                    record.state = ReplayState::Failed {
                        output: error.clone(),
                    };
                    write_replay_record(&record_path, &record)?;
                    return Ok(error);
                }
            };
            let receipt = StoredReceipt {
                request_hash: request_hash.clone(),
                pre_digest: prepared.pre_digest.clone(),
                post_digest: prepared.post_digest.clone(),
                publication_generation: prepared.publication_generation,
                diff: prepared.diff.clone(),
            };
            record.state = ReplayState::PendingWrite {
                pre_image: prepared.pre_image.clone(),
                post_image: prepared.post_image.clone(),
                receipt: receipt.clone(),
            };
            write_replay_record(&record_path, &record)?;
            self.maybe_fail(CurationWriteStage::AfterPendingIntentSync)?;
            #[cfg(test)]
            if let Some(bytes) = self.interposed_policy_bytes.lock().take() {
                fs::write(repo_root.join(POLICY_FILE), bytes)
                    .map_err(|error| durable_state_error(&error))?;
            }
            let policy_path = repo_root.join(POLICY_FILE);
            let live_image = read_policy_bytes(&policy_path)?;
            if live_image != prepared.pre_image && live_image != prepared.post_image {
                let output = "Error: indeterminate_conflict; policy bytes match neither the recorded pre-image nor post-image."
                    .to_string();
                record.state = ReplayState::Indeterminate {
                    output: output.clone(),
                };
                write_replay_record(&record_path, &record)?;
                return Ok(output);
            }
            self.write_policy(&policy_path, &prepared.post_image)?;
            record.state = ReplayState::Succeeded {
                receipt: receipt.clone(),
            };
            write_replay_record(&record_path, &record)?;
            self.maybe_fail(CurationWriteStage::AfterResultSync)?;
            Ok(receipt.render())
        })();

        let _ = unlock_file(&lock_file);
        match output {
            Ok(output) => output,
            Err(error) => error,
        }
    }

    fn handle_existing_record(
        &self,
        repo_root: &Path,
        curation_dir: &Path,
        record_path: &Path,
        mut record: ReplayRecord,
        generation: &crate::live_index::PublishedGeneration,
        plan: &CurationReviewPlan,
        request_hash: &str,
    ) -> String {
        if let Err(error) =
            verify_record_binding(repo_root, curation_dir, record_path, &record, plan)
        {
            return error;
        }
        if record.request_hash != request_hash {
            return "Error: idempotency_conflict; the key is already bound to a different canonical request."
                .to_string();
        }
        match &record.state {
            ReplayState::Succeeded { receipt } => receipt.render(),
            ReplayState::Failed { output } | ReplayState::Indeterminate { output } => {
                output.clone()
            }
            ReplayState::Reserved => {
                "Error: pending_reserved_request; retry the identical request to resume validation."
                    .to_string()
            }
            ReplayState::PendingWrite {
                pre_image,
                post_image,
                receipt,
            } => {
                let policy_path = repo_root.join(POLICY_FILE);
                let current = match read_policy_bytes(&policy_path) {
                    Ok(bytes) => bytes,
                    Err(error) => return error,
                };
                if current == *post_image {
                    if let Err(error) = sync_committed_file(&policy_path) {
                        return durable_state_error(&error);
                    }
                } else if current == *pre_image {
                    if let Err(error) = reauthorize_pending_write(
                        generation, repo_root, &record, pre_image, post_image, receipt,
                    ) {
                        record.state = ReplayState::Failed {
                            output: error.clone(),
                        };
                        if let Err(write_error) = write_replay_record(record_path, &record) {
                            return write_error;
                        }
                        return error;
                    }
                    if let Err(error) =
                        durable_replace(&policy_path, post_image, ".symforge-curation-recovery-")
                    {
                        return error;
                    }
                } else {
                    let output = "Error: indeterminate_conflict; policy bytes match neither the recorded pre-image nor post-image."
                        .to_string();
                    record.state = ReplayState::Indeterminate {
                        output: output.clone(),
                    };
                    let _ = write_replay_record(record_path, &record);
                    return output;
                }
                let receipt = receipt.clone();
                record.state = ReplayState::Succeeded {
                    receipt: receipt.clone(),
                };
                if let Err(error) = write_replay_record(record_path, &record) {
                    return error;
                }
                receipt.render()
            }
        }
    }

    fn recover_pending_records(
        &self,
        repo_root: &Path,
        curation_dir: &Path,
        replay_dir: &Path,
        generation: &crate::live_index::PublishedGeneration,
        plan: &CurationReviewPlan,
    ) -> Result<(), String> {
        let entries = match fs::read_dir(replay_dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(durable_state_error(&error)),
        };
        let mut paths = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
            .collect::<Vec<_>>();
        paths.sort();
        if paths.len() > MAX_RECOVERY_RECORDS {
            return Err(
                "Error: durable replay recovery capacity exceeded; no new mutation was attempted."
                    .to_string(),
            );
        }
        for path in paths {
            let Some(record) = read_replay_record(&path)? else {
                continue;
            };
            if !matches!(record.state, ReplayState::PendingWrite { .. }) {
                continue;
            }
            let request_hash = record.request_hash.clone();
            let output = self.handle_existing_record(
                repo_root,
                curation_dir,
                &path,
                record,
                generation,
                plan,
                &request_hash,
            );
            if output.contains("status=applied") {
                continue;
            }
            if output.contains("foreign_source_conflict") && !path.exists() {
                continue;
            }
            return Err(output);
        }
        Ok(())
    }

    fn probe_apply_directories(
        &self,
        repo_root: &Path,
        curation_dir: &Path,
    ) -> Result<(), CapabilityUnavailableReason> {
        fs::create_dir_all(curation_dir)
            .map_err(|_| CapabilityUnavailableReason::DurableMutationReplayUnavailable)?;
        let mut directories = vec![repo_root.to_path_buf()];
        if curation_dir != repo_root {
            directories.push(curation_dir.to_path_buf());
        }
        for directory in directories {
            let canonical = dunce::canonicalize(&directory).unwrap_or(directory);
            if let Some(result) = self.probe_cache.lock().get(&canonical).cloned() {
                result?;
                continue;
            }
            #[cfg(test)]
            self.probe_operations.lock().push(canonical.clone());
            #[cfg(test)]
            let result = if self.probe_failures.lock().contains(&canonical) {
                Err(CapabilityUnavailableReason::AtomicDurabilityUnavailable)
            } else {
                durability_probe(&canonical)
                    .map_err(|_| CapabilityUnavailableReason::AtomicDurabilityUnavailable)
            };
            #[cfg(not(test))]
            let result = durability_probe(&canonical)
                .map_err(|_| CapabilityUnavailableReason::AtomicDurabilityUnavailable);
            self.probe_cache.lock().insert(canonical, result);
            result?;
        }
        Ok(())
    }

    fn write_policy(&self, path: &Path, bytes: &[u8]) -> Result<(), String> {
        let parent = path
            .parent()
            .ok_or_else(|| "Error: policy path has no durable parent.".to_string())?;
        let mut temp = tempfile::Builder::new()
            .prefix(".symforge-curation-policy-")
            .tempfile_in(parent)
            .map_err(|error| durable_state_error(&error))?;
        temp.write_all(bytes)
            .map_err(|error| durable_state_error(&error))?;
        temp.as_file_mut()
            .flush()
            .map_err(|error| durable_state_error(&error))?;
        self.maybe_fail(CurationWriteStage::AfterTempWrite)?;
        #[cfg(test)]
        if let Some(corrupt) = self.temp_corruption.lock().take() {
            fs::write(temp.path(), corrupt).map_err(|error| durable_state_error(&error))?;
        }
        let written = fs::read(temp.path()).map_err(|error| durable_state_error(&error))?;
        if crate::hash::digest_hex(&written) != crate::hash::digest_hex(bytes) {
            return Err(
                "Error: policy temp image failed digest verification; no policy mutation was acknowledged."
                    .to_string(),
            );
        }
        temp.as_file()
            .sync_all()
            .map_err(|error| durable_state_error(&error))?;
        self.maybe_fail(CurationWriteStage::AfterTempSync)?;
        persist_temp_file(temp, path).map_err(|error| durable_state_error(&error))?;
        self.maybe_fail(CurationWriteStage::AfterAtomicReplace)?;
        sync_committed_file(path).map_err(|error| durable_state_error(&error))?;
        sync_parent(parent).map_err(|error| durable_state_error(&error))?;
        self.maybe_fail(CurationWriteStage::AfterParentDurability)
    }

    fn maybe_fail(&self, stage: CurationWriteStage) -> Result<(), String> {
        #[cfg(test)]
        {
            let mut configured = self.failpoint.lock();
            if configured.as_ref() == Some(&stage) {
                configured.take();
                return Err(format!("Error: injected_curation_crash at {stage:?}."));
            }
        }
        #[cfg(not(test))]
        let _ = stage;
        Ok(())
    }

    #[cfg(test)]
    fn set_failpoint_for_tests(&self, stage: CurationWriteStage) {
        self.failpoint.lock().replace(stage);
    }

    #[cfg(test)]
    fn corrupt_temp_for_tests(&self, bytes: Vec<u8>) {
        self.temp_corruption.lock().replace(bytes);
    }

    #[cfg(test)]
    fn interpose_policy_bytes_for_tests(&self, bytes: Vec<u8>) {
        self.interposed_policy_bytes.lock().replace(bytes);
    }

    #[cfg(test)]
    fn fail_probe_for_tests(&self, directory: &Path) {
        let canonical = dunce::canonicalize(directory).unwrap_or_else(|_| directory.to_path_buf());
        self.probe_failures.lock().insert(canonical);
    }

    #[cfg(test)]
    fn probe_operations_for_tests(&self) -> Vec<PathBuf> {
        self.probe_operations.lock().clone()
    }
}

pub(crate) fn validate_input(input: &CurateKnowledgeInput) -> Result<(), String> {
    if input.actions.is_empty() {
        return Err("Error: actions must contain at least one explicit review action.".to_string());
    }
    if input.actions.len() > MAX_ACTIONS {
        return Err(format!(
            "Error: actions exceeds the maximum of {MAX_ACTIONS}."
        ));
    }
    if input.apply {
        if input
            .idempotency_key
            .as_deref()
            .is_none_or(|key| key.trim().is_empty())
        {
            return Err("Error: apply requires a non-empty idempotency_key.".to_string());
        }
    } else if input.idempotency_key.is_some() {
        return Err("Error: preview does not accept an idempotency_key.".to_string());
    }
    let serialized = serde_json::to_vec(input)
        .map_err(|_| "Error: curation request could not be canonicalized.".to_string())?;
    if serialized.len() > MAX_INPUT_BYTES {
        return Err("Error: curation request exceeds the bounded input budget.".to_string());
    }
    let strings = input_strings(input);
    if strings
        .iter()
        .any(|value| value.len() > MAX_STRING_BYTES || guard_query(value).is_err())
    {
        return Err(
            "Error: sensitive or oversized curation input rejected by repository safety policy."
                .to_string(),
        );
    }
    if input
        .project
        .as_deref()
        .is_none_or(|project| project.trim().is_empty())
    {
        return Err("status=unavailable\nreason=non_project_local_placement".to_string());
    }
    for digest in [
        input.if_source_review_hash.as_str(),
        input.if_manifest_digest.as_str(),
        input.if_policy_digest.as_str(),
    ] {
        if !is_hex_digest(digest) {
            return Err(
                "Error: review, manifest, and policy guards must be exact digests.".to_string(),
            );
        }
    }
    let mut action_ids = BTreeSet::new();
    for action in &input.actions {
        if !action.action_id.starts_with("action-") || !action_ids.insert(&action.action_id) {
            return Err("Error: action IDs must be unique stable review action IDs.".to_string());
        }
        validate_mutation_shape(&action.mutation)?;
    }
    Ok(())
}

fn prepare_mutation(
    generation: &crate::live_index::PublishedGeneration,
    repo_root: &Path,
    input: &CurateKnowledgeInput,
) -> Result<PreparedMutation, String> {
    let plan = curation_plan_current(generation)?;
    if input.if_source_review_hash != plan.review_hash {
        return Err("Error: stale_review_hash; capture a fresh current-source review.".to_string());
    }
    if input.if_manifest_digest != plan.manifest_digest {
        return Err(
            "Error: stale_manifest_digest; capture a fresh current-source review.".to_string(),
        );
    }
    if input.if_policy_digest != plan.policy_digest {
        return Err(
            "Error: stale_policy_digest; capture a fresh current-source review.".to_string(),
        );
    }
    if !generation.authority.curation_eligible {
        return Err("Error: policy_authority_unavailable; repair the malformed, unsupported, or stale policy before curation."
            .to_string());
    }

    let policy_path = repo_root.join(POLICY_FILE);
    let pre_image = read_policy_bytes(&policy_path)?;
    if !pre_image.is_empty() {
        let text = std::str::from_utf8(&pre_image)
            .map_err(|_| "Error: policy_authority_unavailable; policy is not UTF-8.".to_string())?;
        if guard_query(text).is_err() {
            return Err(
                "Error: policy_authority_unavailable; policy matched repository safety policy."
                    .to_string(),
            );
        }
    }
    let pre_digest = crate::hash::digest_hex(&pre_image);
    if pre_digest != input.if_policy_digest {
        return Err("Error: stale_policy_digest; on-disk policy changed after review.".to_string());
    }
    let policy = if pre_image.is_empty() {
        KnowledgePolicy {
            version: POLICY_VERSION,
            entries: Vec::new(),
        }
    } else {
        parse_knowledge_policy(&pre_image).map_err(|_| {
            "Error: policy_authority_unavailable; policy is not valid v1.".to_string()
        })?
    };
    let mut entries = policy
        .entries
        .into_iter()
        .map(|entry| (entry.entry_id.clone(), entry))
        .collect::<BTreeMap<_, _>>();
    let mut actions = input.actions.iter().collect::<Vec<_>>();
    actions.sort_by(|left, right| left.action_id.cmp(&right.action_id));
    for action in actions {
        apply_reviewed_mutation(repo_root, &plan, action, &mut entries)?;
    }
    let post_policy = KnowledgePolicy {
        version: POLICY_VERSION,
        entries: entries.into_values().collect(),
    };
    let post_image = render_policy(&post_policy)?;
    if post_image.len() > MAX_POLICY_BYTES {
        return Err("Error: policy write exceeds the bounded ledger budget.".to_string());
    }
    let reparsed = parse_knowledge_policy(&post_image)
        .map_err(|_| "Error: canonical policy image failed validation.".to_string())?;
    if reparsed != post_policy {
        return Err("Error: canonical policy image failed round-trip validation.".to_string());
    }
    let post_digest = crate::hash::digest_hex(&post_image);
    let diff = replacement_diff(&pre_image, &post_image)?;
    if guard_query(&diff).is_err() {
        return Err("Error: policy diff matched repository safety policy.".to_string());
    }
    Ok(PreparedMutation {
        pre_image,
        post_image,
        pre_digest,
        post_digest,
        diff,
        publication_generation: plan.publication_generation,
    })
}

fn apply_reviewed_mutation(
    repo_root: &Path,
    plan: &CurationReviewPlan,
    action: &KnowledgePolicyActionInput,
    entries: &mut BTreeMap<String, KnowledgePolicyEntry>,
) -> Result<(), String> {
    let reviewed = plan.actions.get(&action.action_id).ok_or_else(|| {
        "Error: unknown_action_id; the selected action is absent from the fresh review.".to_string()
    })?;
    if reviewed.action_id != action.action_id {
        return Err("Error: unknown_action_id; action identity did not reproduce.".to_string());
    }
    if reviewed
        .unmet_preconditions
        .iter()
        .any(|precondition| precondition != "requires_user_judgment")
    {
        return Err(
            "Error: action_preconditions_unmet; the fresh review still reports blockers."
                .to_string(),
        );
    }
    match &action.mutation {
        KnowledgePolicyMutationInput::Upsert { entry } => {
            let target = convert_target(&entry.target)?;
            if target != reviewed.target {
                return Err("Error: stale_target_guard; mutation target does not match the reviewed action."
                    .to_string());
            }
            validate_target_on_disk(repo_root, &target)?;
            validate_proposal_compatibility(&reviewed.proposal_action, entry)?;
            let entry = convert_entry(entry)?;
            for evidence in &entry.evidence {
                if !reviewed.proposal_evidence_ids.contains(&evidence.rule_id) {
                    return Err("Error: unreproduced_evidence; policy evidence is absent from the fresh review."
                        .to_string());
                }
            }
            entries.insert(entry.entry_id.clone(), entry);
        }
        KnowledgePolicyMutationInput::Remove {
            entry_id,
            expected_target,
        } => {
            validate_removal_compatibility(&reviewed.proposal_action)?;
            let target = convert_target(expected_target)?;
            if target != reviewed.target {
                return Err(
                    "Error: stale_target_guard; removal target does not match the reviewed action."
                        .to_string(),
                );
            }
            validate_target_on_disk(repo_root, &target)?;
            let existing = entries.get(entry_id).ok_or_else(|| {
                "Error: stale_policy_entry; removal target no longer exists.".to_string()
            })?;
            if existing.target != target {
                return Err(
                    "Error: stale_policy_entry; removal target changed after review.".to_string(),
                );
            }
            entries.remove(entry_id);
        }
    }
    Ok(())
}

fn reauthorize_pending_write(
    generation: &crate::live_index::PublishedGeneration,
    repo_root: &Path,
    record: &ReplayRecord,
    pre_image: &[u8],
    post_image: &[u8],
    receipt: &StoredReceipt,
) -> Result<(), String> {
    if canonical_request_hash(&record.request) != record.request_hash
        || receipt.request_hash != record.request_hash
    {
        return Err(
            "Error: durable_replay_record_mismatch; no policy mutation was acknowledged."
                .to_string(),
        );
    }
    let prepared = prepare_mutation(generation, repo_root, &record.request)?;
    if prepared.pre_image != pre_image
        || prepared.post_image != post_image
        || prepared.pre_digest != receipt.pre_digest
        || prepared.post_digest != receipt.post_digest
        || prepared.publication_generation != receipt.publication_generation
        || prepared.diff != receipt.diff
    {
        return Err(
            "Error: stale_replay_image; durable replay record no longer matches a fresh curation authorization."
                .to_string(),
        );
    }
    Ok(())
}

fn validate_removal_compatibility(proposal: &RemediationAction) -> Result<(), String> {
    if matches!(proposal, RemediationAction::DeletionCandidate { .. }) {
        Ok(())
    } else {
        Err("Error: mutation_action_mismatch; policy removal is not authorized by the fresh review action."
            .to_string())
    }
}

fn validate_proposal_compatibility(
    proposal: &RemediationAction,
    entry: &KnowledgePolicyEntryInput,
) -> Result<(), String> {
    let compatible = match entry.lifecycle {
        KnowledgePolicyLifecycleInput::Unknown => true,
        KnowledgePolicyLifecycleInput::Superseded => matches!(
            proposal,
            RemediationAction::MergeInto { .. }
                | RemediationAction::MarkSuperseded { .. }
                | RemediationAction::DeletionCandidate { .. }
        ),
        KnowledgePolicyLifecycleInput::Archived => matches!(
            proposal,
            RemediationAction::Archive | RemediationAction::DeletionCandidate { .. }
        ),
        KnowledgePolicyLifecycleInput::Proposed => matches!(
            proposal,
            RemediationAction::RelabelIntent
                | RemediationAction::Keep
                | RemediationAction::NeedsReview
        ),
        _ => !matches!(
            proposal,
            RemediationAction::Archive
                | RemediationAction::DeletionCandidate { .. }
                | RemediationAction::MarkSuperseded { .. }
        ),
    };
    if compatible {
        Ok(())
    } else {
        Err("Error: mutation_action_mismatch; policy lifecycle is not authorized by the fresh review action."
            .to_string())
    }
}

fn convert_entry(input: &KnowledgePolicyEntryInput) -> Result<KnowledgePolicyEntry, String> {
    let mut evidence = input
        .evidence
        .iter()
        .map(convert_evidence)
        .collect::<Result<Vec<_>, _>>()?;
    evidence.sort_by_key(evidence_sort_key);
    Ok(KnowledgePolicyEntry {
        entry_id: input.entry_id.clone(),
        target: convert_target(&input.target)?,
        lifecycle: convert_lifecycle(input.lifecycle),
        authority_domain: input.authority_domain.map(convert_authority_domain),
        superseded_by: input
            .superseded_by
            .as_ref()
            .map(convert_target)
            .transpose()?,
        evidence,
        justification_code: input.justification_code.clone(),
    })
}

fn convert_evidence(input: &KnowledgePolicyEvidenceInput) -> Result<PolicyEvidenceRef, String> {
    Ok(PolicyEvidenceRef {
        rule_id: input.rule_id.clone(),
        knowledge: input.knowledge.as_ref().map(convert_target).transpose()?,
        code: input
            .code_path
            .as_ref()
            .map(|path| CodeAnchorId::File { path: path.clone() }),
    })
}

fn convert_target(input: &KnowledgePolicyTargetInput) -> Result<KnowledgePolicyTarget, String> {
    validate_target_shape(input)?;
    Ok(KnowledgePolicyTarget {
        path: input.path.clone(),
        content_hash: input.content_hash.clone(),
        unit_byte_range: input.unit_byte_range.map(|range| Range {
            start: range[0],
            end: range[1],
        }),
        unit_hash: input.unit_hash.clone(),
    })
}

fn convert_lifecycle(input: KnowledgePolicyLifecycleInput) -> KnowledgeLifecycle {
    match input {
        KnowledgePolicyLifecycleInput::Active => KnowledgeLifecycle::Active,
        KnowledgePolicyLifecycleInput::Proposed => KnowledgeLifecycle::Proposed,
        KnowledgePolicyLifecycleInput::Accepted => KnowledgeLifecycle::Accepted,
        KnowledgePolicyLifecycleInput::Implemented => KnowledgeLifecycle::Implemented,
        KnowledgePolicyLifecycleInput::Deferred => KnowledgeLifecycle::Deferred,
        KnowledgePolicyLifecycleInput::Rejected => KnowledgeLifecycle::Rejected,
        KnowledgePolicyLifecycleInput::Withdrawn => KnowledgeLifecycle::Withdrawn,
        KnowledgePolicyLifecycleInput::Deprecated => KnowledgeLifecycle::Deprecated,
        KnowledgePolicyLifecycleInput::Superseded => KnowledgeLifecycle::Superseded,
        KnowledgePolicyLifecycleInput::Archived => KnowledgeLifecycle::Archived,
        KnowledgePolicyLifecycleInput::Historical => KnowledgeLifecycle::Historical,
        KnowledgePolicyLifecycleInput::Unknown => KnowledgeLifecycle::Unknown,
    }
}

fn convert_authority_domain(input: KnowledgePolicyAuthorityDomainInput) -> AuthorityDomain {
    match input {
        KnowledgePolicyAuthorityDomainInput::CurrentImplementation => {
            AuthorityDomain::CurrentImplementation
        }
        KnowledgePolicyAuthorityDomainInput::NormativeIntent => AuthorityDomain::NormativeIntent,
        KnowledgePolicyAuthorityDomainInput::Decision => AuthorityDomain::Decision,
        KnowledgePolicyAuthorityDomainInput::Operations => AuthorityDomain::Operations,
        KnowledgePolicyAuthorityDomainInput::Governance => AuthorityDomain::Governance,
        KnowledgePolicyAuthorityDomainInput::HistoricalRecord => AuthorityDomain::HistoricalRecord,
        KnowledgePolicyAuthorityDomainInput::Unknown => AuthorityDomain::Unknown,
    }
}

fn validate_mutation_shape(mutation: &KnowledgePolicyMutationInput) -> Result<(), String> {
    match mutation {
        KnowledgePolicyMutationInput::Upsert { entry } => {
            if entry.entry_id.trim().is_empty() || entry.justification_code.trim().is_empty() {
                return Err(
                    "Error: policy entry_id and justification_code must be non-empty.".to_string(),
                );
            }
            validate_target_shape(&entry.target)?;
            if let Some(target) = &entry.superseded_by {
                validate_target_shape(target)?;
            }
            if entry.lifecycle == KnowledgePolicyLifecycleInput::Superseded
                && entry.superseded_by.is_none()
            {
                return Err("Error: superseded policy entries require superseded_by.".to_string());
            }
            for evidence in &entry.evidence {
                if evidence.rule_id.trim().is_empty() {
                    return Err("Error: policy evidence rule_id must be non-empty.".to_string());
                }
                if let Some(target) = &evidence.knowledge {
                    validate_target_shape(target)?;
                    if target.unit_byte_range.is_some() {
                        return Err(
                            "Error: policy evidence knowledge targets are whole-file references."
                                .to_string(),
                        );
                    }
                }
                if let Some(path) = &evidence.code_path
                    && !is_safe_relative_path(path)
                {
                    return Err("Error: policy evidence code_path is unsafe.".to_string());
                }
            }
        }
        KnowledgePolicyMutationInput::Remove {
            entry_id,
            expected_target,
        } => {
            if entry_id.trim().is_empty() {
                return Err("Error: removal entry_id must be non-empty.".to_string());
            }
            validate_target_shape(expected_target)?;
        }
    }
    Ok(())
}

fn validate_target_shape(target: &KnowledgePolicyTargetInput) -> Result<(), String> {
    if !is_safe_relative_path(&target.path) || target.path == POLICY_FILE {
        return Err("Error: policy mutation target path is unsafe or reserved.".to_string());
    }
    if !is_hex_digest(&target.content_hash) {
        return Err("Error: policy target content_hash must be an exact digest.".to_string());
    }
    match (target.unit_byte_range, target.unit_hash.as_deref()) {
        (None, None) => {}
        (Some([start, end]), Some(hash)) if start < end && is_hex_digest(hash) => {}
        _ => {
            return Err(
                "Error: unit_byte_range and unit_hash must be present together with start < end."
                    .to_string(),
            );
        }
    }
    Ok(())
}

fn validate_target_on_disk(repo_root: &Path, target: &KnowledgePolicyTarget) -> Result<(), String> {
    let path = repo_root.join(&target.path);
    reject_symlink_path(repo_root, &target.path)?;
    let metadata = fs::metadata(&path)
        .map_err(|_| "Error: stale_target_guard; target is unavailable.".to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_TARGET_BYTES {
        return Err("Error: stale_target_guard; target is not a bounded regular file.".to_string());
    }
    let bytes = fs::read(&path)
        .map_err(|_| "Error: stale_target_guard; target could not be read.".to_string())?;
    if crate::hash::digest_hex(&bytes) != target.content_hash {
        return Err("Error: stale_target_guard; target content hash changed.".to_string());
    }
    if let Some(range) = &target.unit_byte_range {
        let start = usize::try_from(range.start)
            .map_err(|_| "Error: stale_target_guard; unit range is invalid.".to_string())?;
        let end = usize::try_from(range.end)
            .map_err(|_| "Error: stale_target_guard; unit range is invalid.".to_string())?;
        let unit = bytes
            .get(start..end)
            .ok_or_else(|| "Error: stale_target_guard; unit range is invalid.".to_string())?;
        if target.unit_hash.as_deref() != Some(crate::hash::digest_hex(unit).as_str()) {
            return Err("Error: stale_target_guard; unit hash changed.".to_string());
        }
    }
    Ok(())
}

fn reject_symlink_path(repo_root: &Path, relative: &str) -> Result<(), String> {
    let canonical_root = dunce::canonicalize(repo_root)
        .map_err(|_| "Error: source root is unavailable.".to_string())?;
    let mut current = canonical_root.clone();
    for component in Path::new(relative).components() {
        let Component::Normal(component) = component else {
            return Err("Error: policy mutation target path is unsafe.".to_string());
        };
        current.push(component);
        let metadata = fs::symlink_metadata(&current)
            .map_err(|_| "Error: stale_target_guard; target is unavailable.".to_string())?;
        if metadata.file_type().is_symlink() {
            return Err(
                "Error: symlink_target_rejected; policy targets must be direct source files."
                    .to_string(),
            );
        }
    }
    let canonical_target = dunce::canonicalize(&current)
        .map_err(|_| "Error: stale_target_guard; target is unavailable.".to_string())?;
    if !canonical_target.starts_with(&canonical_root) {
        return Err("Error: policy mutation target escapes the repository root.".to_string());
    }
    Ok(())
}

fn input_strings(input: &CurateKnowledgeInput) -> Vec<&str> {
    let mut strings = vec![
        input.if_source_review_hash.as_str(),
        input.if_manifest_digest.as_str(),
        input.if_policy_digest.as_str(),
    ];
    strings.extend(input.idempotency_key.as_deref());
    strings.extend(input.project.as_deref());
    for action in &input.actions {
        strings.push(&action.action_id);
        match &action.mutation {
            KnowledgePolicyMutationInput::Upsert { entry } => {
                strings.push(&entry.entry_id);
                strings.push(&entry.justification_code);
                push_target_strings(&mut strings, &entry.target);
                if let Some(target) = &entry.superseded_by {
                    push_target_strings(&mut strings, target);
                }
                for evidence in &entry.evidence {
                    strings.push(&evidence.rule_id);
                    strings.extend(evidence.code_path.as_deref());
                    if let Some(target) = &evidence.knowledge {
                        push_target_strings(&mut strings, target);
                    }
                }
            }
            KnowledgePolicyMutationInput::Remove {
                entry_id,
                expected_target,
            } => {
                strings.push(entry_id);
                push_target_strings(&mut strings, expected_target);
            }
        }
    }
    strings
}

fn push_target_strings<'a>(strings: &mut Vec<&'a str>, target: &'a KnowledgePolicyTargetInput) {
    strings.push(&target.path);
    strings.push(&target.content_hash);
    strings.extend(target.unit_hash.as_deref());
}

fn is_safe_relative_path(path: &str) -> bool {
    if path.is_empty() || path.contains('\\') || path.contains('\0') || path.contains(':') {
        return false;
    }
    let mut saw_component = false;
    for component in Path::new(path).components() {
        match component {
            Component::Normal(value) => {
                if !saw_component && value == ".symforge" {
                    return false;
                }
                saw_component = true;
            }
            _ => return false,
        }
    }
    saw_component
}

fn is_hex_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn canonical_request_hash(input: &CurateKnowledgeInput) -> String {
    let mut canonical = input.clone();
    canonical.apply = true;
    canonical.idempotency_key = None;
    canonical.project = None;
    canonical
        .actions
        .sort_by(|left, right| left.action_id.cmp(&right.action_id));
    for action in &mut canonical.actions {
        if let KnowledgePolicyMutationInput::Upsert { entry } = &mut action.mutation {
            entry.evidence.sort_by(|left, right| {
                evidence_input_sort_key(left).cmp(&evidence_input_sort_key(right))
            });
        }
    }
    let bytes = serde_json::to_vec(&canonical).expect("validated curation request serializes");
    domain_digest("knowledge-curation-request-v1", &bytes)
}

fn evidence_input_sort_key(evidence: &KnowledgePolicyEvidenceInput) -> (String, String, String) {
    (
        evidence.rule_id.clone(),
        evidence.code_path.clone().unwrap_or_default(),
        evidence
            .knowledge
            .as_ref()
            .map(|target| target.path.clone())
            .unwrap_or_default(),
    )
}

fn evidence_sort_key(evidence: &PolicyEvidenceRef) -> (String, String, String) {
    (
        evidence.rule_id.clone(),
        match &evidence.code {
            Some(CodeAnchorId::File { path }) => path.clone(),
            Some(CodeAnchorId::Symbol { .. }) | None => String::new(),
        },
        evidence
            .knowledge
            .as_ref()
            .map(|target| target.path.clone())
            .unwrap_or_default(),
    )
}

fn domain_digest(domain: &str, bytes: &[u8]) -> String {
    let mut input = Vec::with_capacity(domain.len() + 1 + bytes.len());
    input.extend_from_slice(domain.as_bytes());
    input.push(0);
    input.extend_from_slice(bytes);
    crate::hash::digest_hex(&input)
}

fn render_policy(policy: &KnowledgePolicy) -> Result<Vec<u8>, String> {
    let mut entries = policy.entries.clone();
    entries.sort_by(|left, right| left.entry_id.cmp(&right.entry_id));
    let mut output = format!("version = {}\n", policy.version);
    for entry in entries {
        output.push_str("\n[[entries]]\n");
        output.push_str(&format!("entry_id = {}\n", toml_string(&entry.entry_id)));
        output.push_str(&format!(
            "lifecycle = {}\n",
            toml_string(lifecycle_label(entry.lifecycle))
        ));
        if let Some(domain) = entry.authority_domain {
            output.push_str(&format!(
                "authority_domain = {}\n",
                toml_string(authority_domain_label(domain))
            ));
        }
        output.push_str(&format!(
            "justification_code = {}\n",
            toml_string(&entry.justification_code)
        ));
        render_target(&mut output, "entries.target", &entry.target);
        if let Some(target) = &entry.superseded_by {
            render_target(&mut output, "entries.superseded_by", target);
        }
        let mut evidence = entry.evidence;
        evidence.sort_by_key(evidence_sort_key);
        for evidence in evidence {
            output.push_str("\n[[entries.evidence]]\n");
            output.push_str(&format!("rule_id = {}\n", toml_string(&evidence.rule_id)));
            if let Some(CodeAnchorId::File { path }) = evidence.code {
                output.push_str(&format!("code_path = {}\n", toml_string(&path)));
            }
            if let Some(knowledge) = evidence.knowledge {
                output.push_str(&format!(
                    "knowledge_path = {}\nknowledge_content_hash = {}\n",
                    toml_string(&knowledge.path),
                    toml_string(&knowledge.content_hash),
                ));
            }
        }
    }
    Ok(output.into_bytes())
}

fn render_target(output: &mut String, heading: &str, target: &KnowledgePolicyTarget) {
    output.push_str(&format!("\n[{heading}]\n"));
    output.push_str(&format!("path = {}\n", toml_string(&target.path)));
    output.push_str(&format!(
        "content_hash = {}\n",
        toml_string(&target.content_hash)
    ));
    if let Some(range) = &target.unit_byte_range {
        output.push_str(&format!(
            "unit_byte_range = [{}, {}]\n",
            range.start, range.end
        ));
    }
    if let Some(unit_hash) = &target.unit_hash {
        output.push_str(&format!("unit_hash = {}\n", toml_string(unit_hash)));
    }
}

fn toml_string(value: &str) -> String {
    toml_edit::Value::from(value).to_string()
}

fn lifecycle_label(value: KnowledgeLifecycle) -> &'static str {
    match value {
        KnowledgeLifecycle::Active => "active",
        KnowledgeLifecycle::Proposed => "proposed",
        KnowledgeLifecycle::Accepted => "accepted",
        KnowledgeLifecycle::Implemented => "implemented",
        KnowledgeLifecycle::Deferred => "deferred",
        KnowledgeLifecycle::Rejected => "rejected",
        KnowledgeLifecycle::Withdrawn => "withdrawn",
        KnowledgeLifecycle::Deprecated => "deprecated",
        KnowledgeLifecycle::Superseded => "superseded",
        KnowledgeLifecycle::Archived => "archived",
        KnowledgeLifecycle::Historical => "historical",
        KnowledgeLifecycle::Unknown => "unknown",
    }
}

fn authority_domain_label(value: AuthorityDomain) -> &'static str {
    match value {
        AuthorityDomain::CurrentImplementation => "current_implementation",
        AuthorityDomain::NormativeIntent => "normative_intent",
        AuthorityDomain::Decision => "decision",
        AuthorityDomain::Operations => "operations",
        AuthorityDomain::Governance => "governance",
        AuthorityDomain::HistoricalRecord => "historical_record",
        AuthorityDomain::Unknown => "unknown",
    }
}

fn replacement_diff(pre_image: &[u8], post_image: &[u8]) -> Result<String, String> {
    let pre = std::str::from_utf8(pre_image)
        .map_err(|_| "Error: policy pre-image is not UTF-8.".to_string())?;
    let post = std::str::from_utf8(post_image)
        .map_err(|_| "Error: policy post-image is not UTF-8.".to_string())?;
    let pre = serde_json::to_string(pre)
        .map_err(|_| "Error: policy pre-image could not be rendered.".to_string())?;
    let post = serde_json::to_string(post)
        .map_err(|_| "Error: policy post-image could not be rendered.".to_string())?;
    Ok(format!(
        "ledger_diff_v1\npath={POLICY_FILE}\npre_image={pre}\npost_image={post}"
    ))
}

fn apply_capability(
    repo_root: &Path,
    state_placement: Option<&StatePlacement>,
    persistence_health: CapabilityStatus,
    source_location: &SourceLocation,
) -> Result<PathBuf, CapabilityUnavailableReason> {
    if matches!(
        state_placement,
        Some(StatePlacement::UserLocal {
            reason: UserLocalPlacementReason::ExplicitProtected,
            ..
        })
    ) {
        return Err(CapabilityUnavailableReason::ExplicitProtectedSource);
    }
    if !matches!(source_location, SourceLocation::WorkingTree { .. }) {
        return Err(CapabilityUnavailableReason::NonProjectLocalPlacement);
    }
    if !matches!(persistence_health, CapabilityStatus::Available) {
        return Err(CapabilityUnavailableReason::DurableMutationReplayUnavailable);
    }
    let state_dir = state_placement
        .and_then(StatePlacement::directory)
        .map(|directory| directory.as_path().to_path_buf())
        .ok_or(CapabilityUnavailableReason::DurableMutationReplayUnavailable)?;
    let root_metadata =
        fs::metadata(repo_root).map_err(|_| CapabilityUnavailableReason::SourceReadOnly)?;
    if root_metadata.permissions().readonly() {
        return Err(CapabilityUnavailableReason::SourceReadOnly);
    }
    let policy_path = repo_root.join(POLICY_FILE);
    if let Ok(metadata) = fs::metadata(policy_path)
        && metadata.permissions().readonly()
    {
        return Err(CapabilityUnavailableReason::SourceReadOnly);
    }
    Ok(state_dir)
}

fn unavailable(reason: CapabilityUnavailableReason) -> String {
    format!(
        "status=unavailable\nreason={}",
        capability_reason_code(reason)
    )
}

fn capability_reason_code(reason: CapabilityUnavailableReason) -> &'static str {
    match reason {
        CapabilityUnavailableReason::ExplicitProtectedSource => "explicit_protected_source",
        CapabilityUnavailableReason::SourceReadOnly => "source_read_only",
        CapabilityUnavailableReason::PersistentStateUnavailable => "persistent_state_unavailable",
        CapabilityUnavailableReason::DurableMutationReplayUnavailable => {
            "durable_mutation_replay_unavailable"
        }
        CapabilityUnavailableReason::NonProjectLocalPlacement => "non_project_local_placement",
        CapabilityUnavailableReason::AtomicDurabilityUnavailable => "atomic_durability_unavailable",
    }
}

fn capture_binding(
    repo_root: &Path,
    plan: &CurationReviewPlan,
) -> Result<CurationSourceBinding, String> {
    let continuity = match git2::Repository::open(repo_root) {
        Ok(repository) => {
            let tip = repository
                .head()
                .ok()
                .and_then(|head| head.target())
                .ok_or_else(|| {
                    "Error: curation_continuity_unavailable; Git HEAD has no commit anchor."
                        .to_string()
                })?;
            CurationContinuityProof::Git {
                object_format: object_format_for_id(&tip.to_string()).to_string(),
                anchor_tip_object_id: tip.to_string(),
                git_directory_object_identity: root_object_identity(repository.path())?,
            }
        }
        Err(_) => CurationContinuityProof::NonGit {
            root_object_identity: root_object_identity(repo_root)?,
            catalog_identity_digest: plan.manifest_digest.clone(),
            publication_generation: plan.publication_generation,
        },
    };
    Ok(CurationSourceBinding {
        repository_id: plan.source.repository_id.clone(),
        source_id: plan.source.source_id.clone(),
        continuity,
    })
}

fn verify_binding(
    repo_root: &Path,
    curation_dir: &Path,
    binding: &CurationSourceBinding,
    plan: &CurationReviewPlan,
    append_lineage: bool,
) -> Result<(), String> {
    if binding.repository_id != plan.source.repository_id
        || binding.source_id != plan.source.source_id
    {
        return Err(
            "Error: foreign_source_conflict; repository/source identity changed.".to_string(),
        );
    }
    match &binding.continuity {
        CurationContinuityProof::Git {
            object_format,
            anchor_tip_object_id,
            git_directory_object_identity,
        } => {
            let repository = git2::Repository::open(repo_root).map_err(|_| {
                "Error: foreign_source_conflict; recorded Git source is unavailable.".to_string()
            })?;
            if &root_object_identity(repository.path())? != git_directory_object_identity {
                return Err(
                    "Error: foreign_source_conflict; Git repository identity changed.".to_string(),
                );
            }
            if object_format_for_id(anchor_tip_object_id) != object_format {
                return Err(
                    "Error: foreign_source_conflict; Git object format changed.".to_string()
                );
            }
            let oid = git2::Oid::from_str(anchor_tip_object_id).map_err(|_| {
                "Error: foreign_source_conflict; recorded Git anchor is invalid.".to_string()
            })?;
            repository.find_commit(oid).map_err(|_| {
                "Error: foreign_source_conflict; recorded Git anchor is no longer resolvable."
                    .to_string()
            })?;
        }
        CurationContinuityProof::NonGit {
            root_object_identity: recorded_root,
            catalog_identity_digest,
            publication_generation,
        } => {
            if &root_object_identity(repo_root)? != recorded_root {
                return Err(
                    "Error: foreign_source_conflict; non-Git root identity changed.".to_string(),
                );
            }
            verify_or_append_lineage(
                curation_dir,
                catalog_identity_digest,
                *publication_generation,
                &plan.manifest_digest,
                plan.publication_generation,
                append_lineage,
            )?;
        }
    }
    Ok(())
}

fn verify_record_binding(
    repo_root: &Path,
    curation_dir: &Path,
    record_path: &Path,
    record: &ReplayRecord,
    plan: &CurationReviewPlan,
) -> Result<(), String> {
    if let Err(foreign) = verify_binding(repo_root, curation_dir, &record.binding, plan, true) {
        quarantine_record(curation_dir, record_path, record)?;
        return Err(foreign);
    }
    Ok(())
}

fn object_format_for_id(object_id: &str) -> &'static str {
    if object_id.len() == 64 {
        "sha256"
    } else {
        "sha1"
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct CatalogLineageEdge {
    from_digest: String,
    from_generation: u64,
    to_digest: String,
    to_generation: u64,
}

fn verify_or_append_lineage(
    curation_dir: &Path,
    from_digest: &str,
    from_generation: u64,
    to_digest: &str,
    to_generation: u64,
    append: bool,
) -> Result<(), String> {
    if from_digest == to_digest {
        return Ok(());
    }
    let lineage_path = curation_dir.join("catalog-lineage.json");
    let mut edges = if lineage_path.is_file() {
        let bytes = fs::read(&lineage_path).map_err(|_| {
            "Error: foreign_source_conflict; catalog lineage is unreadable.".to_string()
        })?;
        serde_json::from_slice::<Vec<CatalogLineageEdge>>(&bytes).map_err(|_| {
            "Error: foreign_source_conflict; catalog lineage is malformed.".to_string()
        })?
    } else {
        Vec::new()
    };
    if edges.iter().any(|edge| {
        edge.from_digest == from_digest
            && edge.from_generation == from_generation
            && edge.to_digest == to_digest
            && edge.to_generation == to_generation
    }) {
        return Ok(());
    }
    if !append {
        return Err("Error: catalog lineage verification requires the mutation lock.".to_string());
    }
    if to_generation != from_generation.saturating_add(1) {
        return Err(
            "Error: foreign_source_conflict; required non-Git catalog lineage is missing."
                .to_string(),
        );
    }
    edges.push(CatalogLineageEdge {
        from_digest: from_digest.to_string(),
        from_generation,
        to_digest: to_digest.to_string(),
        to_generation,
    });
    edges.sort_by(|left, right| {
        left.from_generation
            .cmp(&right.from_generation)
            .then_with(|| left.from_digest.cmp(&right.from_digest))
            .then_with(|| left.to_generation.cmp(&right.to_generation))
            .then_with(|| left.to_digest.cmp(&right.to_digest))
    });
    let bytes = serde_json::to_vec(&edges)
        .map_err(|_| "Error: catalog lineage could not be serialized.".to_string())?;
    durable_replace(&lineage_path, &bytes, ".symforge-curation-lineage-")
}

#[cfg(unix)]
fn root_object_identity(repo_root: &Path) -> Result<String, String> {
    use std::os::unix::fs::MetadataExt;

    let file = File::open(repo_root).map_err(|_| {
        "Error: curation_continuity_unavailable; source root could not be opened.".to_string()
    })?;
    let metadata = file.metadata().map_err(|_| {
        "Error: curation_continuity_unavailable; source root identity is unavailable.".to_string()
    })?;
    let bytes = [metadata.dev().to_le_bytes(), metadata.ino().to_le_bytes()].concat();
    Ok(domain_digest("non-git-root-object-v1", &bytes))
}

#[cfg(windows)]
fn root_object_identity(repo_root: &Path) -> Result<String, String> {
    use std::os::windows::fs::MetadataExt;

    let canonical = dunce::canonicalize(repo_root).map_err(|_| {
        "Error: curation_continuity_unavailable; source root identity is unavailable.".to_string()
    })?;
    let metadata = fs::metadata(repo_root).map_err(|_| {
        "Error: curation_continuity_unavailable; source root identity is unavailable.".to_string()
    })?;
    // Stable Rust does not yet expose Windows by-handle volume/file-index
    // identity. Bind the canonical native root to its immutable creation-time
    // metadata instead; ordinary content edits preserve this value, while a
    // same-path root replacement receives a new creation time. This value is
    // continuity evidence only and never becomes logical source identity.
    let mut bytes = canonical.to_string_lossy().as_bytes().to_vec();
    bytes.extend_from_slice(&metadata.creation_time().to_le_bytes());
    bytes.extend_from_slice(&metadata.file_attributes().to_le_bytes());
    Ok(domain_digest("non-git-root-object-v1", &bytes))
}

#[cfg(not(any(unix, windows)))]
fn root_object_identity(_repo_root: &Path) -> Result<String, String> {
    Err(
        "Error: curation_continuity_unavailable; platform root identity is unsupported."
            .to_string(),
    )
}

fn read_policy_bytes(path: &Path) -> Result<Vec<u8>, String> {
    match fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(_) => Err("Error: policy_authority_unavailable; policy could not be read.".to_string()),
    }
}

fn read_replay_record(path: &Path) -> Result<Option<ReplayRecord>, String> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("Error: durable replay record is unreadable.".to_string()),
    };
    if bytes.len() > MAX_POLICY_BYTES {
        return Err("Error: durable replay record exceeds its bounded size.".to_string());
    }
    let record: ReplayRecord = serde_json::from_slice(&bytes)
        .map_err(|_| "Error: durable replay record is malformed.".to_string())?;
    if record.version != RECORD_VERSION {
        return Err("Error: durable replay record version is unsupported.".to_string());
    }
    Ok(Some(record))
}

fn write_replay_record(path: &Path, record: &ReplayRecord) -> Result<(), String> {
    let bytes = serde_json::to_vec(record)
        .map_err(|_| "Error: durable replay record could not be serialized.".to_string())?;
    if bytes.len() > MAX_POLICY_BYTES {
        return Err("Error: durable replay record exceeds its bounded size.".to_string());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| durable_state_error(&error))?;
    }
    durable_replace(path, &bytes, ".symforge-curation-record-")
}

fn quarantine_record(
    curation_dir: &Path,
    record_path: &Path,
    record: &ReplayRecord,
) -> Result<(), String> {
    let quarantine_dir = curation_dir.join(QUARANTINE_DIR);
    fs::create_dir_all(&quarantine_dir).map_err(|error| durable_state_error(&error))?;
    let destination = quarantine_dir.join(format!("foreign-{}.json", record.key_digest));
    let bytes = serde_json::to_vec(record)
        .map_err(|_| "Error: pending curation record could not be quarantined.".to_string())?;
    durable_replace(&destination, &bytes, ".symforge-curation-quarantine-")?;
    if record_path.is_file() {
        fs::remove_file(record_path).map_err(|error| durable_state_error(&error))?;
        if let Some(parent) = record_path.parent() {
            sync_parent(parent).map_err(|error| durable_state_error(&error))?;
        }
    }
    Ok(())
}

fn open_and_lock(path: &Path) -> io::Result<File> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)?;
    file.lock()?;
    Ok(file)
}

fn unlock_file(file: &File) -> io::Result<()> {
    file.unlock()
}

fn durable_replace(path: &Path, bytes: &[u8], prefix: &str) -> Result<(), String> {
    durable_replace_io(path, bytes, prefix).map_err(|error| durable_state_error(&error))
}

fn durable_replace_io(path: &Path, bytes: &[u8], prefix: &str) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("durable path has no parent"))?;
    let mut temp = tempfile::Builder::new()
        .prefix(prefix)
        .tempfile_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file_mut().flush()?;
    let written = fs::read(temp.path())?;
    if crate::hash::digest_hex(&written) != crate::hash::digest_hex(bytes) {
        return Err(io::Error::other(
            "durable temp image failed digest verification",
        ));
    }
    temp.as_file().sync_all()?;

    persist_temp_file(temp, path)?;

    sync_committed_file(path)?;
    sync_parent(parent)
}

fn persist_temp_file(temp: tempfile::NamedTempFile, path: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        let (temp_file, temp_path) = temp.keep().map_err(|error| error.error)?;
        drop(temp_file);
        if let Err(error) = windows_write_through_replace(&temp_path, path) {
            let _ = fs::remove_file(temp_path);
            return Err(error);
        }
    }

    #[cfg(not(windows))]
    temp.persist(path).map_err(|error| error.error)?;

    Ok(())
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn windows_write_through_replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    use windows::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };
    use windows::core::PCWSTR;

    let source_wide = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination_wide = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();

    // SAFETY: both pointers reference live, NUL-terminated UTF-16 buffers for
    // the duration of the call. The source is a private same-directory temp
    // file, and the destination is the validated durable-record path.
    unsafe {
        MoveFileExW(
            PCWSTR(source_wide.as_ptr()),
            PCWSTR(destination_wide.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
        .map_err(io::Error::other)
    }
}

fn sync_committed_file(path: &Path) -> io::Result<()> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?
        .sync_all()
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> io::Result<()> {
    File::open(parent)?.sync_all()
}

#[cfg(windows)]
fn sync_parent(_parent: &Path) -> io::Result<()> {
    // Windows durability is supplied by the synced temp plus same-directory
    // MoveFileExW(REPLACE_EXISTING | WRITE_THROUGH) above, followed by a sync
    // of the committed destination. The first-use production probe executes
    // this exact primitive and disables apply when it cannot complete.
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn sync_parent(_parent: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "parent durability unsupported",
    ))
}

fn durability_probe(directory: &Path) -> io::Result<()> {
    let mut destination = tempfile::Builder::new()
        .prefix(".symforge-curation-probe-destination-")
        .tempfile_in(directory)?;
    destination.write_all(b"before")?;
    destination.as_file_mut().flush()?;
    destination.as_file().sync_all()?;
    let (destination_file, destination_path) = destination.keep().map_err(|error| error.error)?;
    drop(destination_file);

    let result = (|| {
        durable_replace_io(
            &destination_path,
            b"after",
            ".symforge-curation-probe-replacement-",
        )?;
        if fs::read(&destination_path)? != b"after" {
            return Err(io::Error::other("durability probe readback mismatch"));
        }
        Ok(())
    })();
    let cleanup = fs::remove_file(&destination_path).and_then(|()| sync_parent(directory));
    result.and(cleanup)
}

fn durable_state_error(_error: &impl std::fmt::Display) -> String {
    "Error: durable_mutation_replay_unavailable; no policy mutation was acknowledged.".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AccessErrorKind, ProjectId, ProjectStateDir};
    use crate::live_index::LiveIndex;

    struct CrashFixture {
        dir: tempfile::TempDir,
        index: SharedIndex,
        placement: StatePlacement,
        input: CurateKnowledgeInput,
    }

    impl CrashFixture {
        fn new(key: &str) -> Self {
            Self::new_with_git(key, false)
        }

        fn new_git(key: &str) -> Self {
            Self::new_with_git(key, true)
        }

        fn new_with_git(key: &str, initialize_git: bool) -> Self {
            let dir = tempfile::tempdir().expect("curation crash fixture");
            let root = dir.path();
            fs::create_dir_all(root.join("docs")).expect("docs dir");
            fs::write(
                root.join("docs/current.md"),
                "# Current behavior\nThe source is byte exact.\n",
            )
            .expect("knowledge file");
            fs::create_dir_all(root.join("src")).expect("src dir");
            fs::write(root.join("src/lib.rs"), "pub fn exact() -> bool { true }\n")
                .expect("source file");
            if initialize_git {
                initialize_git_repository(root);
            }
            let index = LiveIndex::load(root).expect("load crash fixture");
            let generation = index.published_source_set().current_generation();
            let plan = curation_plan_current(&generation).expect("curation plan");
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
                            entry_id: "entry-crash-recovery".to_string(),
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
                idempotency_key: Some(key.to_string()),
                apply: true,
                project: Some("current-project".to_string()),
            };
            let placement = StatePlacement::ProjectLocal {
                directory: ProjectStateDir::new(root.join(".symforge")),
            };
            Self {
                dir,
                index,
                placement,
                input,
            }
        }

        fn execute(&self, coordinator: &KnowledgeCurationCoordinator) -> String {
            coordinator.execute(
                &self.index,
                self.dir.path(),
                Some(&self.placement),
                CapabilityStatus::Available,
                &self.input,
            )
        }
    }

    fn initialize_git_repository(root: &Path) {
        let repository = git2::Repository::init(root).expect("initialize Git fixture");
        let mut index = repository.index().expect("Git fixture index");
        index
            .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
            .expect("stage initial files");
        index.write().expect("write Git fixture index");
        let tree_id = index.write_tree().expect("write initial tree");
        let tree = repository.find_tree(tree_id).expect("initial tree");
        let signature = git2::Signature::now("SymForge Test", "symforge@example.invalid")
            .expect("test signature");
        repository
            .commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
            .expect("initial commit");
    }

    fn target_input(target: &KnowledgePolicyTarget) -> KnowledgePolicyTargetInput {
        let range = target
            .unit_byte_range
            .as_ref()
            .expect("reviewed target has a unit range");
        KnowledgePolicyTargetInput {
            path: target.path.clone(),
            content_hash: target.content_hash.clone(),
            unit_byte_range: Some([range.start, range.end]),
            unit_hash: target.unit_hash.clone(),
        }
    }

    fn commit_file(root: &Path, relative_path: &str, bytes: &[u8], message: &str) {
        fs::write(root.join(relative_path), bytes).expect("write committed fixture file");
        let repository = git2::Repository::open(root).expect("open Git fixture");
        let mut index = repository.index().expect("Git fixture index");
        index
            .add_path(Path::new(relative_path))
            .expect("stage committed fixture file");
        index.write().expect("write Git fixture index");
        let tree_id = index.write_tree().expect("write committed tree");
        let tree = repository.find_tree(tree_id).expect("committed tree");
        let parent = repository
            .head()
            .expect("Git HEAD")
            .peel_to_commit()
            .expect("Git parent commit");
        let signature = git2::Signature::now("SymForge Test", "symforge@example.invalid")
            .expect("test signature");
        repository
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                message,
                &tree,
                &[&parent],
            )
            .expect("fixture commit");
    }

    fn switch_to_identical_branch(root: &Path) {
        let repository = git2::Repository::open(root).expect("open Git fixture");
        let head = repository
            .head()
            .expect("Git HEAD")
            .peel_to_commit()
            .expect("Git head commit");
        repository
            .branch("identical-bytes", &head, false)
            .expect("create identical branch");
        repository
            .set_head("refs/heads/identical-bytes")
            .expect("switch HEAD ref");
    }

    fn use_external_state(fixture: &mut CrashFixture, state_root: &Path) {
        fixture.placement = StatePlacement::UserLocal {
            directory: ProjectStateDir::new(state_root.join("project-state")),
            root_id: ProjectId("curation-continuity-test".to_string()),
            reason: UserLocalPlacementReason::ProjectLocalUnavailable {
                safe_reason: AccessErrorKind::PermissionDenied,
            },
        };
    }

    fn replace_repository_root(fixture: &CrashFixture) -> PathBuf {
        let root = fixture.dir.path();
        let parent = root.parent().expect("temp root parent");
        let placeholder = tempfile::Builder::new()
            .prefix("symforge-displaced-root-")
            .tempdir_in(parent)
            .expect("displaced root placeholder");
        let displaced = placeholder.path().to_path_buf();
        placeholder.close().expect("remove displaced placeholder");
        fs::rename(root, &displaced).expect("move original repository aside");

        fs::create_dir_all(root.join("docs")).expect("replacement docs dir");
        fs::write(
            root.join("docs/current.md"),
            "# Current behavior\nThe source is byte exact.\n",
        )
        .expect("replacement knowledge file");
        fs::create_dir_all(root.join("src")).expect("replacement src dir");
        fs::write(root.join("src/lib.rs"), "pub fn exact() -> bool { true }\n")
            .expect("replacement source file");
        displaced
    }

    #[test]
    fn every_durable_write_boundary_recovers_to_one_terminal_policy() {
        let stages = [
            CurationWriteStage::AfterReservation,
            CurationWriteStage::AfterPendingIntentSync,
            CurationWriteStage::AfterTempWrite,
            CurationWriteStage::AfterTempSync,
            CurationWriteStage::AfterAtomicReplace,
            CurationWriteStage::AfterParentDurability,
            CurationWriteStage::AfterResultSync,
        ];
        for (index, stage) in stages.into_iter().enumerate() {
            let fixture = CrashFixture::new(&format!("crash-stage-{index}"));
            let coordinator = KnowledgeCurationCoordinator::default();
            coordinator.set_failpoint_for_tests(stage);
            let interrupted = fixture.execute(&coordinator);
            assert!(
                interrupted.contains("injected_curation_crash"),
                "stage {stage:?}: {interrupted}"
            );

            let recovered = fixture.execute(&coordinator);
            assert!(
                recovered.contains("status=applied"),
                "stage {stage:?}: {recovered}"
            );
            let policy_bytes =
                fs::read(fixture.dir.path().join(POLICY_FILE)).expect("complete recovered policy");
            let policy = parse_knowledge_policy(&policy_bytes).expect("valid recovered policy");
            assert_eq!(policy.entries.len(), 1, "stage {stage:?}");

            let replay = fixture.execute(&coordinator);
            assert_eq!(replay, recovered, "stage {stage:?} terminal replay");
            assert_eq!(
                fs::read(fixture.dir.path().join(POLICY_FILE)).expect("policy after replay"),
                policy_bytes,
                "stage {stage:?} replay changed policy"
            );
        }
    }

    #[test]
    fn third_state_policy_bytes_fail_closed_without_overwrite() {
        let fixture = CrashFixture::new("indeterminate-third-state");
        let coordinator = KnowledgeCurationCoordinator::default();
        coordinator.set_failpoint_for_tests(CurationWriteStage::AfterPendingIntentSync);
        let interrupted = fixture.execute(&coordinator);
        assert!(interrupted.contains("injected_curation_crash"));

        let third_state = b"version = 1\n# independently changed\n";
        fs::write(fixture.dir.path().join(POLICY_FILE), third_state).expect("third-state policy");
        let recovered = fixture.execute(&coordinator);
        assert!(recovered.contains("indeterminate_conflict"), "{recovered}");
        assert_eq!(
            fs::read(fixture.dir.path().join(POLICY_FILE))
                .expect("third-state policy after recovery"),
            third_state,
            "indeterminate recovery must not overwrite third-state bytes"
        );
        assert_eq!(fixture.execute(&coordinator), recovered);
    }

    #[test]
    fn pending_write_recovery_reauthorizes_current_review_before_write() {
        let fixture = CrashFixture::new("stale-pending-write-review");
        let coordinator = KnowledgeCurationCoordinator::default();
        coordinator.set_failpoint_for_tests(CurationWriteStage::AfterPendingIntentSync);
        let interrupted = fixture.execute(&coordinator);
        assert!(interrupted.contains("injected_curation_crash"));

        fs::write(
            fixture.dir.path().join("docs/current.md"),
            "# Current behavior\nThe source changed after review.\n",
        )
        .expect("change reviewed knowledge source");
        fixture
            .index
            .reload(fixture.dir.path())
            .expect("reload changed knowledge source");

        let recovered = fixture.execute(&coordinator);
        assert!(recovered.contains("stale_review_hash"), "{recovered}");
        assert_eq!(
            read_policy_bytes(&fixture.dir.path().join(POLICY_FILE))
                .expect("read policy after failed recovery"),
            Vec::<u8>::new(),
            "stale pending replay must not write the stored policy post-image"
        );
        assert_eq!(fixture.execute(&coordinator), recovered);
    }

    #[test]
    fn remove_mutation_must_match_a_deletion_candidate_review_action() {
        let fixture = CrashFixture::new("remove-action-mismatch");
        let root = fixture.dir.path();
        let seed_plan =
            curation_plan_current(&fixture.index.published_source_set().current_generation())
                .expect("seed curation plan");
        let seed_action = seed_plan
            .actions
            .values()
            .find(|action| {
                !matches!(
                    action.proposal_action,
                    RemediationAction::DeletionCandidate { .. }
                )
            })
            .expect("non-delete review action")
            .clone();
        let entry_id = "entry-remove-mismatch".to_string();
        let policy = KnowledgePolicy {
            version: POLICY_VERSION,
            entries: vec![KnowledgePolicyEntry {
                entry_id: entry_id.clone(),
                target: seed_action.target.clone(),
                lifecycle: KnowledgeLifecycle::Unknown,
                authority_domain: None,
                superseded_by: None,
                evidence: Vec::new(),
                justification_code: "seed-policy-entry".to_string(),
            }],
        };
        fs::write(
            root.join(POLICY_FILE),
            render_policy(&policy).expect("policy render"),
        )
        .expect("seed policy");
        fixture.index.reload(root).expect("reload seeded policy");

        let generation = fixture.index.published_source_set().current_generation();
        let plan = curation_plan_current(&generation).expect("fresh curation plan");
        let reviewed = plan
            .actions
            .values()
            .find(|action| {
                action.target == seed_action.target
                    && !matches!(
                        action.proposal_action,
                        RemediationAction::DeletionCandidate { .. }
                    )
            })
            .expect("fresh non-delete action for seeded target");
        let input = CurateKnowledgeInput {
            actions: vec![KnowledgePolicyActionInput {
                action_id: reviewed.action_id.clone(),
                mutation: KnowledgePolicyMutationInput::Remove {
                    entry_id,
                    expected_target: target_input(&reviewed.target),
                },
            }],
            if_source_review_hash: plan.review_hash,
            if_manifest_digest: plan.manifest_digest,
            if_policy_digest: plan.policy_digest,
            idempotency_key: None,
            apply: false,
            project: Some("current-project".to_string()),
        };

        let output = KnowledgeCurationCoordinator::default().execute(
            &fixture.index,
            root,
            Some(&fixture.placement),
            CapabilityStatus::Available,
            &input,
        );

        assert!(output.contains("mutation_action_mismatch"), "{output}");
        let policy_after = fs::read(root.join(POLICY_FILE)).expect("policy after rejected preview");
        assert_eq!(
            parse_knowledge_policy(&policy_after)
                .expect("policy remains valid")
                .entries
                .len(),
            1,
            "rejected preview must not remove the policy entry"
        );
    }

    #[test]
    fn pending_intent_under_same_path_replacement_is_quarantined_without_writing() {
        let mut fixture = CrashFixture::new("foreign-pending-intent");
        let state = tempfile::tempdir().expect("external state root");
        use_external_state(&mut fixture, state.path());
        let coordinator = KnowledgeCurationCoordinator::default();
        coordinator.set_failpoint_for_tests(CurationWriteStage::AfterPendingIntentSync);
        let interrupted = fixture.execute(&coordinator);
        assert!(interrupted.contains("injected_curation_crash"));

        let displaced = replace_repository_root(&fixture);
        let recovered = fixture.execute(&coordinator);
        assert!(recovered.contains("foreign_source_conflict"), "{recovered}");
        assert!(
            !fixture.dir.path().join(POLICY_FILE).exists(),
            "foreign pending intent wrote ledger bytes into the replacement repository"
        );
        let quarantine = state.path().join("project-state/curation/quarantine");
        assert!(
            fs::read_dir(&quarantine)
                .expect("foreign intent quarantine")
                .next()
                .is_some(),
            "attributable foreign intent must be quarantined"
        );
        fs::remove_dir_all(displaced).expect("remove displaced repository");
    }

    #[test]
    fn terminal_replay_under_same_path_replacement_is_foreign_not_applied() {
        let mut fixture = CrashFixture::new("foreign-terminal-replay");
        let state = tempfile::tempdir().expect("external state root");
        use_external_state(&mut fixture, state.path());
        let coordinator = KnowledgeCurationCoordinator::default();
        let applied = fixture.execute(&coordinator);
        assert!(applied.contains("status=applied"), "{applied}");

        let displaced = replace_repository_root(&fixture);
        let replay = fixture.execute(&coordinator);
        assert!(replay.contains("foreign_source_conflict"), "{replay}");
        assert!(!replay.contains("status=applied"), "{replay}");
        assert!(
            !fixture.dir.path().join(POLICY_FILE).exists(),
            "foreign terminal replay wrote ledger bytes into the replacement repository"
        );
        fs::remove_dir_all(displaced).expect("remove displaced repository");
    }

    #[test]
    fn terminal_replay_under_same_path_clone_with_same_history_is_foreign() {
        let mut fixture = CrashFixture::new_git("same-path-clone-replay");
        let state = tempfile::tempdir().expect("external curation state");
        use_external_state(&mut fixture, state.path());
        let coordinator = KnowledgeCurationCoordinator::default();
        let applied = fixture.execute(&coordinator);
        assert!(applied.contains("status=applied"), "{applied}");

        let root = fixture.dir.path();
        let parent = root.parent().expect("temp root parent");
        let clone_placeholder = tempfile::Builder::new()
            .prefix("symforge-same-history-clone-")
            .tempdir_in(parent)
            .expect("clone placeholder");
        let clone_path = clone_placeholder.path().to_path_buf();
        clone_placeholder.close().expect("remove clone placeholder");
        git2::Repository::clone(root.to_str().expect("UTF-8 test root"), &clone_path)
            .expect("clone same history");

        let displaced = tempfile::Builder::new()
            .prefix("symforge-original-root-")
            .tempdir_in(parent)
            .expect("original root placeholder");
        let displaced_path = displaced.path().to_path_buf();
        displaced.close().expect("remove original placeholder");
        fs::rename(root, &displaced_path).expect("move original root aside");
        fs::rename(&clone_path, root).expect("move clone to original path");
        fixture.index.reload(root).expect("reload same-path clone");

        let replay = fixture.execute(&coordinator);
        assert!(replay.contains("foreign_source_conflict"), "{replay}");
        assert!(!replay.contains("status=applied"), "{replay}");
        assert!(!root.join(POLICY_FILE).exists());
        fs::remove_dir_all(displaced_path).expect("remove displaced repository");
    }

    #[test]
    fn ledger_parent_durability_probe_failure_precedes_reservation_and_mutation() {
        let fixture = CrashFixture::new("ledger-probe-failure");
        let coordinator = KnowledgeCurationCoordinator::default();
        coordinator.fail_probe_for_tests(fixture.dir.path());

        let output = fixture.execute(&coordinator);
        assert!(output.contains("atomic_durability_unavailable"), "{output}");
        assert_eq!(
            coordinator.probe_operations_for_tests(),
            vec![dunce::canonicalize(fixture.dir.path()).expect("canonical root")]
        );
        assert!(!fixture.dir.path().join(POLICY_FILE).exists());
        assert!(
            !fixture
                .dir
                .path()
                .join(".symforge/curation/replay")
                .exists(),
            "failed ledger probe must precede idempotency reservation"
        );
    }

    #[test]
    fn intent_journal_durability_probe_failure_gates_apply_after_ledger_probe() {
        let fixture = CrashFixture::new("journal-probe-failure");
        let coordinator = KnowledgeCurationCoordinator::default();
        let curation_dir = fixture.dir.path().join(".symforge/curation");
        coordinator.fail_probe_for_tests(&curation_dir);

        let output = fixture.execute(&coordinator);
        assert!(output.contains("atomic_durability_unavailable"), "{output}");
        assert_eq!(
            coordinator.probe_operations_for_tests(),
            vec![
                dunce::canonicalize(fixture.dir.path()).expect("canonical root"),
                dunce::canonicalize(&curation_dir).expect("canonical curation dir"),
            ],
            "the ledger parent must pass before the journal-parent failure gates apply"
        );
        assert!(!fixture.dir.path().join(POLICY_FILE).exists());
        assert!(
            !curation_dir.join(REPLAY_DIR).exists(),
            "failed journal probe must precede idempotency reservation"
        );
    }

    #[test]
    fn durability_probe_writes_nothing_into_non_available_sources() {
        let mut explicit_protected = CrashFixture::new("no-probe-explicit-protected");
        let protected_state = tempfile::tempdir().expect("protected state root");
        explicit_protected.placement = StatePlacement::UserLocal {
            directory: ProjectStateDir::new(protected_state.path().join("project-state")),
            root_id: ProjectId("explicit-protected-test".to_string()),
            reason: UserLocalPlacementReason::ExplicitProtected,
        };
        let protected_coordinator = KnowledgeCurationCoordinator::default();
        let protected_output = explicit_protected.execute(&protected_coordinator);
        assert!(
            protected_output.contains("explicit_protected_source"),
            "{protected_output}"
        );
        assert!(
            protected_coordinator
                .probe_operations_for_tests()
                .is_empty()
        );

        let mut memory_only = CrashFixture::new("no-probe-memory-only");
        memory_only.placement = StatePlacement::MemoryOnly {
            failures: Vec::new(),
        };
        let memory_coordinator = KnowledgeCurationCoordinator::default();
        let memory_output = memory_only.execute(&memory_coordinator);
        assert!(
            memory_output.contains("durable_mutation_replay_unavailable"),
            "{memory_output}"
        );
        assert!(memory_coordinator.probe_operations_for_tests().is_empty());

        let read_only = CrashFixture::new("no-probe-read-only");
        let mut root_permissions = fs::metadata(read_only.dir.path())
            .expect("root metadata")
            .permissions();
        root_permissions.set_readonly(true);
        fs::set_permissions(read_only.dir.path(), root_permissions)
            .expect("set source root read-only");
        let read_only_coordinator = KnowledgeCurationCoordinator::default();
        let read_only_output = read_only.execute(&read_only_coordinator);
        assert!(
            read_only_output.contains("source_read_only"),
            "{read_only_output}"
        );
        assert!(
            read_only_coordinator
                .probe_operations_for_tests()
                .is_empty()
        );
        let mut root_permissions = fs::metadata(read_only.dir.path())
            .expect("root metadata")
            .permissions();
        // Windows fixture teardown: clear the read-only attribute so TempDir
        // can remove the tree. clippy's cross-platform caveat does not apply.
        #[allow(clippy::permissions_set_readonly_false)]
        root_permissions.set_readonly(false);
        fs::set_permissions(read_only.dir.path(), root_permissions)
            .expect("restore source root permissions");

        let reference = CrashFixture::new("no-probe-reference");
        let generation = reference.index.published_source_set().current_generation();
        let mut reference_plan = curation_plan_current(&generation).expect("reference plan");
        reference_plan.source.location = SourceLocation::GitRef {
            name: "refs/heads/review-fixture".to_string(),
        };
        assert_eq!(
            apply_capability(
                reference.dir.path(),
                Some(&reference.placement),
                CapabilityStatus::Available,
                &reference_plan.source.location,
            ),
            Err(CapabilityUnavailableReason::NonProjectLocalPlacement)
        );
        let reference_coordinator = KnowledgeCurationCoordinator::default();
        assert!(
            reference_coordinator
                .probe_operations_for_tests()
                .is_empty(),
            "ref capability failure must precede every durability probe"
        );
    }

    #[test]
    fn curation_replay_after_intervening_commit_is_not_foreign() {
        let git_fixture = CrashFixture::new_git("replay-after-git-commit");
        let git_coordinator = KnowledgeCurationCoordinator::default();
        let git_applied = git_fixture.execute(&git_coordinator);
        assert!(git_applied.contains("status=applied"), "{git_applied}");
        commit_file(
            git_fixture.dir.path(),
            "src/after.rs",
            b"pub fn after() {}\n",
            "ordinary commit",
        );
        git_fixture
            .index
            .reload(git_fixture.dir.path())
            .expect("reload after Git commit");
        assert_eq!(
            git_fixture.execute(&git_coordinator),
            git_applied,
            "an ordinary commit is source drift, not foreign identity"
        );

        let non_git_fixture = CrashFixture::new("replay-after-non-git-edit");
        let non_git_coordinator = KnowledgeCurationCoordinator::default();
        let non_git_applied = non_git_fixture.execute(&non_git_coordinator);
        assert!(
            non_git_applied.contains("status=applied"),
            "{non_git_applied}"
        );
        fs::write(
            non_git_fixture.dir.path().join("src/after.rs"),
            "pub fn after() {}\n",
        )
        .expect("non-Git edit");
        non_git_fixture
            .index
            .reload(non_git_fixture.dir.path())
            .expect("reload after non-Git edit");
        assert_eq!(
            non_git_fixture.execute(&non_git_coordinator),
            non_git_applied,
            "one retained catalog-lineage transition must preserve replay"
        );

        let missing_lineage = CrashFixture::new("replay-missing-non-git-lineage");
        let missing_coordinator = KnowledgeCurationCoordinator::default();
        assert!(
            missing_lineage
                .execute(&missing_coordinator)
                .contains("status=applied")
        );
        for (name, body) in [
            ("src/first.rs", "pub fn first() {}\n"),
            ("src/second.rs", "pub fn second() {}\n"),
        ] {
            fs::write(missing_lineage.dir.path().join(name), body).expect("lineage edit");
            missing_lineage
                .index
                .reload(missing_lineage.dir.path())
                .expect("lineage reload");
        }
        let missing = missing_lineage.execute(&missing_coordinator);
        assert!(missing.contains("foreign_source_conflict"), "{missing}");
        assert!(missing.contains("catalog lineage is missing"), "{missing}");
    }

    #[test]
    fn identical_replay_immediately_after_apply_matches_stored_binding() {
        let fixture = CrashFixture::new_git("immediate-binding-replay");
        let coordinator = KnowledgeCurationCoordinator::default();
        let applied = fixture.execute(&coordinator);
        assert!(applied.contains("status=applied"), "{applied}");
        assert_eq!(fixture.execute(&coordinator), applied);
    }

    #[test]
    fn curation_recovery_after_intervening_commit_terminalizes_post_image() {
        let committed = CrashFixture::new_git("recover-after-git-commit");
        let commit_coordinator = KnowledgeCurationCoordinator::default();
        commit_coordinator.set_failpoint_for_tests(CurationWriteStage::AfterAtomicReplace);
        let interrupted = committed.execute(&commit_coordinator);
        assert!(interrupted.contains("injected_curation_crash"));
        let post_image = fs::read(committed.dir.path().join(POLICY_FILE))
            .expect("post-image after atomic replace");
        commit_file(
            committed.dir.path(),
            "src/after-crash.rs",
            b"pub fn after_crash() {}\n",
            "commit after replace",
        );
        committed
            .index
            .reload(committed.dir.path())
            .expect("reload after intervening commit");
        let recovered = committed.execute(&commit_coordinator);
        assert!(recovered.contains("status=applied"), "{recovered}");
        assert_eq!(
            fs::read(committed.dir.path().join(POLICY_FILE)).expect("recovered post-image"),
            post_image
        );

        let switched = CrashFixture::new_git("recover-after-branch-switch");
        let branch_coordinator = KnowledgeCurationCoordinator::default();
        branch_coordinator.set_failpoint_for_tests(CurationWriteStage::AfterAtomicReplace);
        let interrupted = switched.execute(&branch_coordinator);
        assert!(interrupted.contains("injected_curation_crash"));
        let post_image = fs::read(switched.dir.path().join(POLICY_FILE))
            .expect("post-image before branch switch");
        switch_to_identical_branch(switched.dir.path());
        switched
            .index
            .reload(switched.dir.path())
            .expect("reload after identical-byte branch switch");
        let recovered = switched.execute(&branch_coordinator);
        assert!(recovered.contains("status=applied"), "{recovered}");
        assert_eq!(
            fs::read(switched.dir.path().join(POLICY_FILE)).expect("branch recovery post-image"),
            post_image
        );
    }

    #[test]
    fn next_use_recovers_an_orphaned_pending_request_before_new_validation() {
        let mut fixture = CrashFixture::new("orphaned-pending-key");
        let coordinator = KnowledgeCurationCoordinator::default();
        coordinator.set_failpoint_for_tests(CurationWriteStage::AfterAtomicReplace);
        let interrupted = fixture.execute(&coordinator);
        assert!(interrupted.contains("injected_curation_crash"));

        fixture.input.idempotency_key = Some("different-next-use-key".to_string());
        let _ = fixture.execute(&coordinator);

        let old_key_digest = domain_digest("knowledge-curation-key-v1", b"orphaned-pending-key");
        let record_path = fixture
            .placement
            .directory()
            .expect("durable state placement")
            .as_path()
            .join(CURATION_STATE_DIR)
            .join(REPLAY_DIR)
            .join(format!("{old_key_digest}.json"));
        let record = read_replay_record(&record_path)
            .expect("read orphaned record")
            .expect("orphaned record must remain durably represented");
        assert!(
            matches!(record.state, ReplayState::Succeeded { .. }),
            "next use must terminalize a recoverable pending record"
        );
    }

    #[test]
    fn project_startup_recovers_pending_write_before_serving_tools() {
        let fixture = CrashFixture::new("startup-pending-key");
        let interrupted_coordinator = KnowledgeCurationCoordinator::default();
        interrupted_coordinator.set_failpoint_for_tests(CurationWriteStage::AfterAtomicReplace);
        let interrupted = fixture.execute(&interrupted_coordinator);
        assert!(interrupted.contains("injected_curation_crash"));
        let generation_before_recovery = fixture
            .index
            .published_source_set()
            .current_generation()
            .publication_generation;

        let restarted_coordinator = KnowledgeCurationCoordinator::default();
        restarted_coordinator
            .recover_on_project_load(
                &fixture.index,
                fixture.dir.path(),
                Some(&fixture.placement),
                CapabilityStatus::Available,
            )
            .expect("startup recovery");

        let key_digest = domain_digest("knowledge-curation-key-v1", b"startup-pending-key");
        let record_path = fixture
            .placement
            .directory()
            .expect("durable state placement")
            .as_path()
            .join(CURATION_STATE_DIR)
            .join(REPLAY_DIR)
            .join(format!("{key_digest}.json"));
        let record = read_replay_record(&record_path)
            .expect("read startup record")
            .expect("startup record remains durably represented");
        assert!(
            matches!(record.state, ReplayState::Succeeded { .. }),
            "startup must terminalize a recoverable pending write"
        );
        let generation_after_recovery = fixture
            .index
            .published_source_set()
            .current_generation()
            .publication_generation;
        assert!(
            generation_after_recovery > generation_before_recovery,
            "startup recovery must publish recovered policy before tools are served"
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_persisted_temp_uses_write_through_replace() {
        let directory = tempfile::tempdir().expect("write-through fixture");
        let destination = directory.path().join("destination.json");
        fs::write(&destination, b"before").expect("write destination");
        let mut source = tempfile::Builder::new()
            .prefix("source-")
            .tempfile_in(directory.path())
            .expect("create source temp");
        source.write_all(b"after").expect("write source");
        source.as_file_mut().flush().expect("flush source");
        source.as_file().sync_all().expect("sync source");
        let source_path = source.path().to_path_buf();

        persist_temp_file(source, &destination).expect("write-through replace");

        assert_eq!(fs::read(&destination).expect("read destination"), b"after");
        assert!(
            !source_path.exists(),
            "successful replacement consumes the temp file"
        );
    }

    #[test]
    fn corrupted_temp_policy_image_fails_digest_verification_before_replace() {
        let fixture = CrashFixture::new("temp-digest-corruption");
        let coordinator = KnowledgeCurationCoordinator::default();
        coordinator.corrupt_temp_for_tests(b"version = 1\n# corrupted temp image\n".to_vec());
        let output = fixture.execute(&coordinator);
        assert!(output.contains("digest verification"), "{output}");
        assert!(
            !fixture.dir.path().join(POLICY_FILE).exists(),
            "corrupted temp image must never replace the policy"
        );

        let recovered = fixture.execute(&coordinator);
        assert!(recovered.contains("status=applied"), "{recovered}");
        let policy_bytes =
            fs::read(fixture.dir.path().join(POLICY_FILE)).expect("recovered policy bytes");
        let policy = parse_knowledge_policy(&policy_bytes).expect("valid recovered policy");
        assert_eq!(policy.entries.len(), 1);
    }

    #[test]
    fn live_third_state_policy_before_write_is_fenced_not_overwritten() {
        let fixture = CrashFixture::new("live-pre-image-fence");
        let coordinator = KnowledgeCurationCoordinator::default();
        let third_state = b"version = 1\n# independent live change\n";
        coordinator.interpose_policy_bytes_for_tests(third_state.to_vec());
        let output = fixture.execute(&coordinator);
        assert!(output.contains("indeterminate_conflict"), "{output}");
        assert_eq!(
            fs::read(fixture.dir.path().join(POLICY_FILE)).expect("live third-state policy"),
            third_state,
            "live fence must not overwrite an independent policy change"
        );
        assert_eq!(
            fixture.execute(&coordinator),
            output,
            "indeterminate outcome must replay terminally"
        );
    }

    #[test]
    fn foreign_record_quarantine_waits_for_the_mutation_lock() {
        let mut fixture = CrashFixture::new("foreign-quarantine-under-lock");
        let state = tempfile::tempdir().expect("external state root");
        use_external_state(&mut fixture, state.path());
        let coordinator = KnowledgeCurationCoordinator::default();
        let applied = fixture.execute(&coordinator);
        assert!(applied.contains("status=applied"), "{applied}");
        let displaced = replace_repository_root(&fixture);

        let curation_dir = state.path().join("project-state").join(CURATION_STATE_DIR);
        let quarantine_dir = curation_dir.join(QUARANTINE_DIR);
        let lock = open_and_lock(&curation_dir.join(LOCK_FILE)).expect("hold curation lock");

        let replay = std::thread::scope(|scope| {
            let handle = scope.spawn(|| fixture.execute(&coordinator));
            std::thread::sleep(std::time::Duration::from_millis(300));
            let premature = fs::read_dir(&quarantine_dir)
                .map(|entries| entries.count())
                .unwrap_or(0);
            assert_eq!(
                premature, 0,
                "attributable state was quarantined before the mutation lock was acquired"
            );
            unlock_file(&lock).expect("release curation lock");
            handle.join().expect("replay thread")
        });
        assert!(replay.contains("foreign_source_conflict"), "{replay}");
        assert!(
            fs::read_dir(&quarantine_dir)
                .expect("quarantine after lock release")
                .next()
                .is_some(),
            "foreign record must still be quarantined once the lock is held"
        );
        fs::remove_dir_all(displaced).expect("remove displaced repository");
    }
}
