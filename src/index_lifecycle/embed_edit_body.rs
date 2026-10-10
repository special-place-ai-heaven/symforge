//! MCP's edit answer text for the embedded edit lanes. Every piece is the
//! shared `guidance::edit_body` composition the MCP handlers run; this module
//! only gathers each piece's inputs from an embedded source the way the MCP
//! handler gathers them from its index:
//!
//! - the stale-reference warnings and the impact footer read the index MCP
//!   reads after its own reindex, here the published index with the written
//!   post-images substituted (`post_edit_index`), or for an edit rerouted into
//!   another worktree the bound source's unchanged index, which MCP leaves
//!   untouched for a routed write;
//! - the tee snapshot hint comes from the guarded write, which snapshots the
//!   original into the source's admitted state directory (`<state>/tee`);
//! - the project-config trust suffix evaluates the host's trust store
//!   (`<replay_control_directory>/embed-host/edit-safety/trust.json`, the
//!   embedded counterpart of MCP's process control state). The enforce mode
//!   is the host's typed `project_config_trust_mode` option; process
//!   environment policy (that variable, the CI override) is never read, as
//!   with `SYMFORGE_WORKTREE_AWARE`.

use std::path::Path;
use std::sync::Arc;

use crate::domain::index::LanguageId;
use crate::edit_safety::tee::{TeeRecord, TeeSnapshot};
use crate::edit_safety::trust::ProjectConfigTrust;
use crate::live_index::LiveIndex;
use crate::live_index::git_temporal::{GitTemporalIndex, GitTemporalState};

use super::embed_query::EmbeddedQuerySnapshot;
use super::guidance::edit_body::{
    self as shared, BatchEditAnswer, EditSafetyMode, EditSourceAuthority, EditWriteSemantics,
    MatchType, SingleEditAnswer,
};
use crate::embed::parity::source_options::ProjectConfigTrustMode;

/// The inputs of MCP's stale-reference warning, re-evaluated against the
/// bound index when the edit was rerouted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StaleQuery {
    pub path: String,
    pub name: String,
    pub old_signature: String,
    pub new_signature: String,
    pub parent_type: Option<String>,
    pub language: LanguageId,
}

impl StaleQuery {
    pub(crate) fn warnings(&self, index: &LiveIndex) -> String {
        let refs = shared::stale_references(
            index,
            &self.path,
            &self.name,
            &self.old_signature,
            &self.new_signature,
            self.parent_type.as_deref(),
            Some(&self.language),
        );
        shared::format_stale_warnings(&self.path, &self.name, &refs)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SingleParts {
    pub safety: EditSafetyMode,
    pub semantics: EditWriteSemantics,
    pub anchor: String,
    pub path: String,
    pub summary: String,
    pub stale: Option<StaleQuery>,
    pub stale_warnings: String,
    /// The guarded write's tee snapshot; `None` for a preview.
    pub tee: Option<TeeSnapshot>,
    pub tee_before_reroute: bool,
    pub trust: Option<String>,
    pub impact: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BatchFileParts {
    pub path: String,
    pub summaries: Vec<String>,
    pub tee: Option<String>,
    pub reroute: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BatchBody {
    /// `batch_edit` / `batch_insert`: per-file summaries.
    Summaries {
        files: Vec<BatchFileParts>,
        file_count: usize,
    },
    /// `batch_rename`: the rename report (or preview) and, for an apply, the
    /// paths whose reroute report follows it.
    Rename { text: String, paths: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BatchParts {
    pub match_type: MatchType,
    pub semantics: EditWriteSemantics,
    pub evidence: String,
    pub body: BatchBody,
    pub trust: Option<String>,
    pub impact: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EditBodyParts {
    Single(Box<SingleParts>),
    Batch(BatchParts),
    /// A completed same-key apply answered from the replay store, which keeps
    /// digests only (docs/contracts/embed-replay-v1.md), so the original
    /// answer text cannot be returned.
    Replayed {
        tool: &'static str,
        path: String,
        post_image_hash: String,
    },
}

/// The bound source's side of a routed edit, captured by `route_edit`: what
/// MCP reads from its own (indexed) project for an edit it writes elsewhere.
#[derive(Clone)]
pub(crate) struct RouteContext {
    pub rerouted: bool,
    /// The routed target's bytes differ from the bound copy's (MCP's rebase).
    pub rebased: bool,
    pub bound_root: std::path::PathBuf,
    pub target_root: std::path::PathBuf,
    pub working_directory: std::path::PathBuf,
    pub bound_live: Arc<LiveIndex>,
    pub bound_impact: Option<String>,
    pub bound_trust: Option<String>,
}

impl RouteContext {
    fn reroute_for(&self, path: &str) -> String {
        shared_reroute(
            &self.working_directory,
            self.rerouted,
            &self.target_root.join(path),
            &self.bound_root.join(path),
        )
    }
}

pub(crate) fn shared_reroute(
    working_directory: &Path,
    rerouted: bool,
    target_path: &Path,
    indexed_path: &Path,
) -> String {
    super::guidance::edit_route::format_reroute_suffix(
        Some(working_directory),
        rerouted,
        target_path,
        indexed_path,
    )
}

impl EditBodyParts {
    /// MCP's answer for this edit, made without `working_directory` (`route`
    /// `None`) or through the route `route_edit` resolved.
    pub(crate) fn render(&self, route: Option<&RouteContext>) -> String {
        match self {
            Self::Single(parts) => render_single(parts, route),
            Self::Batch(parts) => render_batch(parts, route),
            Self::Replayed {
                tool,
                path,
                post_image_hash,
            } => format!(
                "Idempotency replay: {tool} already applied to {path}; the recorded post-image \
                 ({post_image_hash}) still holds, so nothing was written. The original answer \
                 text is not retained: embedded replay records keep digests only."
            ),
        }
    }
}

fn render_single(parts: &SingleParts, route: Option<&RouteContext>) -> String {
    let committed = parts.semantics != EditWriteSemantics::DryRunNoWrites;
    let rerouted = route.filter(|route| route.rerouted);
    let authority = match rerouted {
        Some(route) if route.rebased => EditSourceAuthority::WorktreeTarget,
        _ => EditSourceAuthority::CurrentIndex,
    };
    let reroute = match route {
        Some(route) if committed => route.reroute_for(&parts.path),
        _ => String::new(),
    };
    let stale_warnings = match (rerouted, &parts.stale) {
        (Some(route), Some(query)) => query.warnings(&route.bound_live),
        _ => parts.stale_warnings.clone(),
    };
    // MCP tees through its indexed project, so it shows the original
    // relative to the bound root: a rerouted target reads as absolute.
    let tee = match (&parts.tee, rerouted) {
        (Some(TeeSnapshot::Created(record)), Some(route)) => {
            tee_hint(&TeeSnapshot::Created(TeeRecord {
                repo_root: route.bound_root.clone(),
                ..record.clone()
            }))
        }
        (Some(snapshot), _) => tee_hint(snapshot),
        (None, _) => String::new(),
    };
    let (trust, impact) = match rerouted {
        Some(route) => (
            route.bound_trust.as_deref(),
            if committed {
                route.bound_impact.as_deref()
            } else {
                None
            },
        ),
        None => (parts.trust.as_deref(), parts.impact.as_deref()),
    };
    shared::render_single_edit_answer(&SingleEditAnswer {
        safety: parts.safety,
        authority,
        semantics: parts.semantics,
        anchor: &parts.anchor,
        summary: &parts.summary,
        stale_warnings: &stale_warnings,
        tee: &tee,
        reroute: &reroute,
        tee_before_reroute: parts.tee_before_reroute,
        trust,
        impact,
    })
}

/// MCP's `format_tee_snapshot_suffix` for one snapshot.
fn tee_hint(snapshot: &TeeSnapshot) -> String {
    snapshot
        .response_hint()
        .map(|hint| format!("\n{hint}"))
        .unwrap_or_default()
}

fn render_batch(parts: &BatchParts, route: Option<&RouteContext>) -> String {
    let committed = parts.semantics != EditWriteSemantics::DryRunNoWrites;
    let body = match &parts.body {
        BatchBody::Summaries { files, file_count } => {
            let mut summaries = Vec::new();
            for file in files {
                let mut file_summaries = file.summaries.clone();
                if let Some(tee) = &file.tee {
                    shared::append_response_suffix_to_first_summary(&mut file_summaries, tee);
                }
                let reroute = match route {
                    Some(route) if committed => route.reroute_for(&file.path),
                    _ => file.reroute.clone(),
                };
                shared::append_response_suffix_to_first_summary(&mut file_summaries, &reroute);
                summaries.extend(file_summaries);
            }
            shared::format_batch_summary(&summaries, *file_count)
        }
        BatchBody::Rename { text, paths } => {
            let mut text = text.clone();
            if let Some(route) = route.filter(|_| committed) {
                for path in paths {
                    text.push_str(&route.reroute_for(path));
                }
            }
            text
        }
    };
    shared::render_batch_edit_answer(&BatchEditAnswer {
        match_type: parts.match_type,
        authority: EditSourceAuthority::CurrentIndex,
        semantics: parts.semantics,
        evidence: &parts.evidence,
        body: &body,
        trust: route.map_or(parts.trust.as_deref(), |route| route.bound_trust.as_deref()),
        impact: parts.impact.as_deref(),
    })
}

/// The published index with each `(path, post-image)` re-parsed and
/// substituted, as MCP's `reindex_after_write` publishes it after a write.
pub(super) fn post_edit_index(
    snapshot: &EmbeddedQuerySnapshot,
    written: &[(&str, &[u8])],
) -> LiveIndex {
    let mut index = (*snapshot.generation.live).clone();
    for (path, bytes) in written {
        let Some(language) = index.get_file(path).map(|file| file.language) else {
            continue;
        };
        let result = crate::parsing::process_file(path, bytes, language);
        index.update_file(
            (*path).to_owned(),
            crate::live_index::IndexedFile::from_parse_result(result, bytes.to_vec()),
        );
    }
    index
}

/// The temporal data MCP's footer reads: the publication's when Ready, the
/// temporal lane's cached walk for the current HEAD otherwise, and no
/// co-changes when neither exists (MCP's footer before its walk completes).
pub(super) fn footer_temporal(snapshot: &EmbeddedQuerySnapshot) -> Arc<GitTemporalIndex> {
    let published = &snapshot.generation.code_signals.temporal;
    if published.state == GitTemporalState::Ready {
        return Arc::clone(published);
    }
    let cached = snapshot
        .temporal_cache
        .lock()
        .expect("embedded temporal cache mutex")
        .clone();
    if let Some((head, temporal)) = cached
        && super::embed_changes::open_repository(&snapshot.root)
            .and_then(|repo| repo.resolve_ref_commit("HEAD"))
            == Some(head)
    {
        return temporal;
    }
    Arc::new(GitTemporalIndex::pending())
}

pub(super) fn impact_footer(
    snapshot: &EmbeddedQuerySnapshot,
    index: &LiveIndex,
    path: &str,
) -> String {
    shared::impact_footer_for(index, &footer_temporal(snapshot), path)
}

/// MCP's project-config trust suffix for this source's root, read from the
/// host's trust store, or its enforced refusal text.
fn trust_verdict(snapshot: &EmbeddedQuerySnapshot) -> Result<Option<String>, String> {
    shared::project_config_trust_response_suffix(
        &snapshot.root,
        || {
            snapshot.control_directory.as_deref().map(|directory| {
                ProjectConfigTrust::for_control_state(&crate::domain::ControlStateDir::new(
                    directory.to_path_buf(),
                ))
            })
        },
        |trust| trust.evaluate_without_env_override(&snapshot.root),
        || match snapshot.trust_mode {
            ProjectConfigTrustMode::LogOnly => shared::ProjectConfigTrustMode::LogOnly,
            ProjectConfigTrustMode::Enforce => shared::ProjectConfigTrustMode::Enforce,
        },
    )
}

/// Refuse an edit before any write when the host chose Enforce and the
/// project's config is untrusted; MCP checks this first in every edit handler.
pub(super) fn enforce_trust(
    snapshot: &EmbeddedQuerySnapshot,
) -> Result<(), crate::embed::parity::edit::EditError> {
    trust_verdict(snapshot).map(|_| ()).map_err(|message| {
        crate::embed::parity::edit::EditError::Edit(
            crate::embed::parity::edit::EditErrorKind::ProjectConfigTrustEnforced { message },
        )
    })
}

/// The suffix an answer carries. Enforce refusals are raised by
/// `enforce_trust` before the write, so only the LogOnly suffix reaches here.
pub(super) fn trust_suffix(snapshot: &EmbeddedQuerySnapshot) -> Option<String> {
    trust_verdict(snapshot).ok().flatten()
}
