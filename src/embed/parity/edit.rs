//! Additive embedded structural-edit contract. A plan binds an exact published
//! generation, file image, and symbol; apply requires a separate write grant.

use std::path::PathBuf;
use std::sync::{Arc, atomic::AtomicBool};

use crate::embed::lifecycle::embed_edit_body::{EditBodyParts, RouteContext};
use crate::embed::lifecycle::public_api::EmbedSourceRefusal;

mod wire;
pub use wire::*;

/// MCP's answer text for one edit: the text the matching MCP edit tool
/// (`replace_symbol_body`, `insert_symbol`, `delete_symbol`,
/// `edit_within_symbol`, `batch_edit`, `batch_insert`, `batch_rename`) returns
/// for the same edit, composed by the shared renderer the MCP handlers use.
/// It serializes as [`EditBody::render`]; `Debug` shows its length only.
#[derive(Clone, PartialEq, Eq)]
pub struct EditBody {
    pub(crate) parts: EditBodyParts,
}

impl EditBody {
    pub(crate) fn new(parts: EditBodyParts) -> Self {
        Self { parts }
    }

    /// The answer for this edit made without `working_directory`. An edit
    /// routed with [`EmbeddedSourceHandle::route_edit`] renders through
    /// [`EditRoute::render_body`] instead, which adds MCP's reroute report.
    ///
    /// [`EmbeddedSourceHandle::route_edit`]: crate::embed::EmbeddedSourceHandle::route_edit
    pub fn render(&self) -> String {
        self.parts.render(None)
    }
}

impl std::fmt::Debug for EditBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditBody")
            .field("rendered_bytes", &self.render().len())
            .finish()
    }
}

impl serde::Serialize for EditBody {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.render())
    }
}

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
    /// The name as the plan's target spelled it (for example `Type::method`
    /// for the resolved `method`). MCP's single-symbol answers report the
    /// requested spelling, so the answers here do too.
    pub(crate) selector: String,
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
    /// MCP's dry-run answer (`batch_edit`, `batch_insert` or `batch_rename`).
    pub body: EditBody,
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
            .field("body", &self.body)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct BatchEditApplied {
    pub files: Vec<(String, String)>,
    pub replayed: bool,
    pub refresh_ticket_identity: Option<String>,
    /// MCP's answer for the whole batch. Each part of a routed batch carries
    /// the same answer as [`RoutedBatchApplied::body`].
    pub body: EditBody,
}

/// One admitted source's share of a batch whose per-action
/// `working_directory` routes files into different worktrees (MCP
/// `batch_edit` / `batch_insert` per-edit overrides): the source, the write
/// authority the host minted for it, and the actions that land there, with
/// guards rebased onto it by `EmbeddedSourceHandle::rebase_guard`.
pub struct RoutedBatchPart<'a, R = BatchEditRequest> {
    pub handle: &'a crate::embed::EmbeddedSourceHandle,
    pub authority: &'a EditApplyAuthority,
    pub request: R,
    /// The `working_directory` this part's actions carried, exactly as given;
    /// `None` when they carried none. It is reported, never resolved: the
    /// host chose `handle` for it.
    pub working_directory: Option<PathBuf>,
}

/// A routed batch's result, one entry per part in request order. All parts
/// share one staged commit, one rollback and one replay record.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct RoutedBatchApplied {
    pub parts: Vec<BatchEditApplied>,
    pub replayed: bool,
    /// MCP's answer for the batch, with each routed file's reroute report.
    pub body: EditBody,
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
    /// MCP's dry-run answer (`insert_symbol`, `delete_symbol` or
    /// `edit_within_symbol`).
    pub body: EditBody,
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
            .field("body", &self.body)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct StructuralApplied {
    pub path: String,
    pub post_image_hash: String,
    pub replayed: bool,
    pub refresh_ticket_identity: Option<String>,
    /// MCP's answer for this edit.
    pub body: EditBody,
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
    /// MCP's `replace_symbol_body` dry-run answer.
    pub body: EditBody,
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
            .field("body", &self.body)
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

/// A host-admitted source an edit may be routed into with
/// `working_directory`, with the write authority the host minted for it after
/// its own room rights check. Like federation's admitted list, this list is the
/// complete authority boundary: routing never discovers or opens another root.
#[derive(Clone, Copy)]
pub struct AdmittedEditTarget<'a> {
    pub handle: &'a crate::embed::EmbeddedSourceHandle,
    pub authority: &'a EditApplyAuthority,
}

/// MCP's resolved edit target for a supplied `working_directory`
/// (`crate::worktree::ResolvedTarget` plus the requested directory).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ResolvedEditTarget {
    pub working_directory: PathBuf,
    /// `true` when the edit lands in another worktree than the bound source.
    pub rerouted: bool,
    /// Absolute path the edit writes (`wrote_to`).
    pub target_path: PathBuf,
    /// Absolute path of the bound source's copy.
    pub indexed_path: PathBuf,
}

impl ResolvedEditTarget {
    /// The exact suffix MCP appends to an edit given `working_directory`.
    pub fn reroute_suffix(&self) -> String {
        crate::embed::lifecycle::guidance::edit_route::format_reroute_suffix(
            Some(&self.working_directory),
            self.rerouted,
            &self.target_path,
            &self.indexed_path,
        )
    }
}

/// Where one edit path lands. `target` is `None` when `working_directory` is
/// the bound source itself; otherwise run the operation on `target.handle`
/// with `target.authority`, after rebasing its guards with
/// `EmbeddedSourceHandle::rebase_guard`.
pub struct EditRoute<'a> {
    pub target: Option<AdmittedEditTarget<'a>>,
    pub resolved: ResolvedEditTarget,
    pub(crate) path: String,
    pub(crate) bound_root: PathBuf,
    pub(crate) context: RouteContext,
}

impl EditRoute<'_> {
    /// MCP's answer for an edit made through this route: the edit's own
    /// answer with MCP's reroute report, and for an edit rerouted into another
    /// worktree the source authority, stale-reference warnings, trust suffix
    /// and impact footer MCP reads from the bound (indexed) project.
    pub fn render_body(&self, body: &EditBody) -> String {
        body.parts.render(Some(&self.context))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ReplaceApplied {
    pub path: String,
    pub post_image_hash: String,
    pub replayed: bool,
    /// Queued refresh identity; publication completion must be observed separately.
    pub refresh_ticket_identity: Option<String>,
    /// MCP's `replace_symbol_body` answer for this edit.
    pub body: EditBody,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditErrorKind {
    InvalidPath,
    FileNotAdmitted,
    SymbolNotFound,
    AmbiguousSymbol {
        candidate_lines: Vec<u32>,
    },
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
    OccurrenceOutOfRange {
        requested: u32,
        total: usize,
    },
    TextNotFound,
    /// `working_directory` names no host-admitted source (MCP
    /// `WorkingDirectoryNotARecognizedWorktree`; embed never opens it).
    WorkingDirectoryNotAdmitted,
    /// The admitted source is not a worktree of the bound repository.
    WorkingDirectoryNotAWorktree,
    /// The routed worktree has no admitted file at the edit path (MCP
    /// `TargetFileMissing`).
    TargetFileMissing,
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
