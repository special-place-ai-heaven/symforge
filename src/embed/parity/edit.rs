//! Additive embedded structural-edit contract. A plan binds an exact published
//! generation, file image, and symbol; apply requires a separate write grant.

use std::path::PathBuf;
use std::sync::{Arc, atomic::AtomicBool};

use crate::embed::lifecycle::public_api::EmbedSourceRefusal;

mod wire;
pub use wire::*;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditTarget {
    pub path: String,
    pub name: String,
    pub kind: Option<String>,
    /// One-based line selector, matching the MCP edit tools.
    pub symbol_line: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditGuard {
    pub(crate) root: PathBuf,
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) symbol_line: u32,
    pub(crate) source_version: u64,
    pub(crate) publication_identity: String,
    pub(crate) serving_publication_identity: String,
    pub(crate) publication_generation: u64,
    pub(crate) content_generation: u64,
    pub(crate) authority_publication: crate::lifecycle_identity::PublicationIdentity,
    pub(crate) content_hash: String,
    pub(crate) symbol_hash: String,
}

impl EditGuard {
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn source_version(&self) -> u64 {
        self.source_version
    }
    pub fn publication_identity(&self) -> &str {
        &self.publication_identity
    }
    pub fn serving_publication_identity(&self) -> &str {
        &self.serving_publication_identity
    }
    pub fn publication_generation(&self) -> u64 {
        self.publication_generation
    }
    pub fn content_generation(&self) -> u64 {
        self.content_generation
    }
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }
    pub fn symbol_hash(&self) -> &str {
        &self.symbol_hash
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditPlan {
    pub guard: EditGuard,
    pub byte_range: (u32, u32),
    pub line_range: (u32, u32),
    pub reference_count: usize,
}

/// The replacement text is deliberately excluded from `Debug` and receipts.
pub struct ReplaceRequest {
    pub guard: EditGuard,
    pub new_body: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InsertPosition {
    Before,
    After,
}

/// Inserted source text is deliberately excluded from `Debug` and receipts.
pub struct InsertRequest {
    pub guard: EditGuard,
    pub position: InsertPosition,
    pub content: String,
}

pub struct DeleteRequest {
    pub guard: EditGuard,
}

/// Match and replacement text are deliberately excluded from `Debug` and receipts.
pub struct EditWithinRequest {
    pub guard: EditGuard,
    pub old_text: String,
    pub new_text: String,
    pub replace_all: bool,
    pub occurrence: Option<u32>,
    pub near_line: Option<u32>,
}

pub enum BatchEditAction {
    Replace(ReplaceRequest),
    Insert(InsertRequest),
    Delete(DeleteRequest),
    EditWithin(EditWithinRequest),
}

impl BatchEditAction {
    pub(crate) fn guard(&self) -> &EditGuard {
        match self {
            Self::Replace(request) => &request.guard,
            Self::Insert(request) => &request.guard,
            Self::Delete(request) => &request.guard,
            Self::EditWithin(request) => &request.guard,
        }
    }
}

pub struct BatchEditRequest {
    pub actions: Vec<BatchEditAction>,
}

#[cfg(test)]
mod process_replay_tests {
    use super::*;
    use crate::embed::{EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase};
    use std::fs;
    use std::path::Path;
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    fn wait_current(handle: &crate::embed::EmbeddedSourceHandle) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while handle.runtime_view().phase != SourceRuntimePhase::Current {
            assert!(Instant::now() < deadline, "source failed to publish");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn authority(root: &Path) -> EditApplyAuthority {
        EditApplyAuthority::for_source_root(
            fs::canonicalize(root).expect("canonical root"),
            "fixture-room/process-replay".to_string(),
            Arc::new(AtomicBool::new(false)),
        )
        .expect("host authority")
    }

    #[test]
    fn completed_batch_replay_survives_process_restart() {
        let Ok(root_var) = std::env::var("SYMFORGE_EMBED_REPLAY_CHILD_ROOT") else {
            let repository = tempfile::tempdir().expect("temporary repository");
            git2::Repository::init(repository.path()).expect("initialize repository");
            fs::create_dir_all(repository.path().join("src")).unwrap();
            fs::write(
                repository.path().join("src/lib.rs"),
                b"pub fn number() -> u32 { 1 }\n",
            )
            .unwrap();
            let wire = tempfile::tempdir().expect("private fixture directory");
            let wire_path = wire.path().join("request.json");
            let child = crate::process_util::hidden_command(
                std::env::current_exe().expect("test executable"),
            )
            .arg("completed_batch_replay_survives_process_restart")
            .arg("--nocapture")
            .env("SYMFORGE_EMBED_REPLAY_CHILD_ROOT", repository.path())
            .env("SYMFORGE_EMBED_REPLAY_GUARD", &wire_path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("child test process");
            assert!(child.success(), "child failed to complete guarded mutation");
            let request: WireEditRequest =
                serde_json::from_slice(&fs::read(&wire_path).expect("serialized edit request"))
                    .expect("strict wire edit request");
            let runtime = ProcessIndexRuntime::acquire().expect("acquire fresh process runtime");
            let handle = runtime
                .open_embedded_source(EmbeddedSourceSpec::current_worktree(
                    repository.path().to_path_buf(),
                ))
                .expect("open published source");
            wait_current(&handle);
            let authority = authority(repository.path());
            let replayed = handle
                .apply_wire_edit(&request, &authority, "fixture-process-op")
                .expect("completed operation replay");
            assert!(matches!(replayed, WireEditReply::BatchEditApplied(result) if result.replayed));
            assert_eq!(
                fs::read(repository.path().join("src/lib.rs")).unwrap(),
                b"pub fn number() -> u32 { 2 }\n"
            );
            let stale = handle.apply_wire_edit(&request, &authority, "fixture-process-new-op");
            assert!(matches!(
                stale,
                Err(EditError::Edit(EditErrorKind::StaleGeneration))
            ));
            return;
        };

        let root = PathBuf::from(root_var);
        let runtime = ProcessIndexRuntime::acquire().expect("acquire child runtime");
        let handle = runtime
            .open_embedded_source(EmbeddedSourceSpec::current_worktree(root.clone()))
            .expect("open child source");
        wait_current(&handle);
        let plan = handle
            .edit_plan(&EditTarget {
                path: "src/lib.rs".to_string(),
                name: "number".to_string(),
                kind: None,
                symbol_line: None,
            })
            .expect("child plan");
        let request = WireEditRequest::ApplyBatchEdit {
            actions: vec![WireBatchEditAction::Replace {
                guard: WireEditGuard::from(&plan.guard),
                new_body: "pub fn number() -> u32 { 2 }".to_string(),
            }],
        };
        let applied = handle
            .apply_wire_edit(&request, &authority(&root), "fixture-process-op")
            .expect("child apply");
        assert!(matches!(applied, WireEditReply::BatchEditApplied(result) if !result.replayed));
        fs::write(
            std::env::var("SYMFORGE_EMBED_REPLAY_GUARD").expect("wire path"),
            serde_json::to_vec(&request).unwrap(),
        )
        .expect("save serialized request");
    }
}

/// One insertion applied at every guarded anchor as a single staged operation.
/// Content is excluded from `Debug` and persistence receipts.
pub struct BatchInsertRequest {
    pub targets: Vec<EditGuard>,
    pub position: InsertPosition,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BatchRenamePlan {
    pub guard: EditGuard,
    pub code_only: bool,
    pub affected_paths: Vec<String>,
    pub confident_sites: usize,
    pub uncertain_sites: usize,
}

/// The replacement identifier is excluded from `Debug` and replay storage.
pub struct BatchRenameRequest {
    pub plan: BatchRenamePlan,
    pub new_name: String,
}

#[derive(Clone, PartialEq, Eq, serde::Serialize)]
pub struct BatchRenamePreview {
    pub changes: BatchEditPreview,
    /// The shared MCP site preview after the native content-safety gate.
    pub rendered: String,
    pub truncated: bool,
}

impl std::fmt::Debug for BatchRenamePreview {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BatchRenamePreview")
            .field("changes", &self.changes)
            .field("rendered_bytes", &self.rendered.len())
            .field("truncated", &self.truncated)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct BatchFilePreview {
    pub path: String,
    pub old_file_hash: String,
    pub proposed_file_hash: String,
}

#[derive(Clone, PartialEq, Eq, serde::Serialize)]
pub struct BatchEditPreview {
    pub files: Vec<BatchFilePreview>,
    pub action_count: usize,
    pub publication_identity: String,
    pub source_version: u64,
    /// Bounded diff of the exact staged postimages, redacted by source policy.
    pub rendered: String,
    pub truncated: bool,
    pub redacted: bool,
}

impl std::fmt::Debug for BatchEditPreview {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BatchEditPreview")
            .field("files", &self.files)
            .field("action_count", &self.action_count)
            .field("publication_identity", &self.publication_identity)
            .field("source_version", &self.source_version)
            .field("rendered_bytes", &self.rendered.len())
            .field("truncated", &self.truncated)
            .field("redacted", &self.redacted)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct BatchEditApplied {
    pub files: Vec<(String, String)>,
    pub replayed: bool,
    pub refresh_ticket_identity: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct EditWithinPreview {
    pub change: StructuralPreview,
    pub replacement_count: usize,
    pub untargeted_extra: usize,
}

#[derive(Clone, PartialEq, Eq, serde::Serialize)]
pub struct StructuralPreview {
    pub path: String,
    pub source_version: u64,
    pub publication_identity: String,
    pub old_file_hash: String,
    pub proposed_file_hash: String,
    pub old_bytes: usize,
    pub inserted_bytes: usize,
    pub rendered: String,
    pub truncated: bool,
    pub redacted: bool,
}

impl std::fmt::Debug for StructuralPreview {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StructuralPreview")
            .field("path", &self.path)
            .field("source_version", &self.source_version)
            .field("publication_identity", &self.publication_identity)
            .field("old_file_hash", &self.old_file_hash)
            .field("proposed_file_hash", &self.proposed_file_hash)
            .field("old_bytes", &self.old_bytes)
            .field("inserted_bytes", &self.inserted_bytes)
            .field("rendered_bytes", &self.rendered.len())
            .field("truncated", &self.truncated)
            .field("redacted", &self.redacted)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct StructuralApplied {
    pub path: String,
    pub post_image_hash: String,
    pub replayed: bool,
    pub refresh_ticket_identity: Option<String>,
}

#[derive(Clone, PartialEq, Eq, serde::Serialize)]
pub struct ReplacePreview {
    pub path: String,
    pub source_version: u64,
    pub publication_identity: String,
    pub old_file_hash: String,
    pub proposed_file_hash: String,
    pub old_bytes: usize,
    pub inserted_bytes: usize,
    pub rendered: String,
    pub truncated: bool,
    pub redacted: bool,
}

impl std::fmt::Debug for ReplacePreview {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReplacePreview")
            .field("path", &self.path)
            .field("source_version", &self.source_version)
            .field("publication_identity", &self.publication_identity)
            .field("old_file_hash", &self.old_file_hash)
            .field("proposed_file_hash", &self.proposed_file_hash)
            .field("old_bytes", &self.old_bytes)
            .field("inserted_bytes", &self.inserted_bytes)
            .field("rendered_bytes", &self.rendered.len())
            .field("truncated", &self.truncated)
            .field("redacted", &self.redacted)
            .finish()
    }
}

/// Explicit caller authorization scoped to a particular source root. The
/// engine still obtains its own source mutation permit before touching disk.
pub struct EditApplyAuthority {
    pub(crate) root: PathBuf,
    pub(crate) scope: String,
    pub(crate) cancel: Arc<AtomicBool>,
}

impl EditApplyAuthority {
    /// The host mints this only after its own room/source rights check.
    /// `scope` is a stable, non-secret room/source identity for durable replay.
    pub fn for_source_root(
        root: PathBuf,
        scope: String,
        cancel: Arc<AtomicBool>,
    ) -> Result<Self, EditErrorKind> {
        if !root.is_absolute() || scope.trim().is_empty() {
            return Err(EditErrorKind::WriteAuthorityRefused);
        }
        Ok(Self {
            root,
            scope,
            cancel,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ReplaceApplied {
    pub path: String,
    pub post_image_hash: String,
    pub replayed: bool,
    /// Queued refresh identity; publication completion must be observed separately.
    pub refresh_ticket_identity: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditErrorKind {
    InvalidPath,
    FileNotAdmitted,
    SymbolNotFound,
    AmbiguousSymbol { candidate_lines: Vec<u32> },
    UnsafeContent,
    StaleGeneration,
    StaleContent,
    WriteAuthorityRefused,
    WriteConflict,
    WriteUncertain,
    ReplayUnavailable,
    ReplayConflict,
    Cancelled,
    InvalidReplacement,
    ConflictingTargeting,
    OccurrenceOutOfRange { requested: u32, total: usize },
    TextNotFound,
}

#[derive(Debug)]
pub enum EditError {
    Source(EmbedSourceRefusal),
    Edit(EditErrorKind),
}

impl From<EmbedSourceRefusal> for EditError {
    fn from(value: EmbedSourceRefusal) -> Self {
        Self::Source(value)
    }
}
