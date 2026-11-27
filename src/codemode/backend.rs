//! Read-only host backend trait and structured JSON projections for Code Mode.
//!
//! Supports three concrete backends with identical JSON output contracts:
//! 1. [`LocalCodeBackend`]: evaluates against an in-process [`PublishedGeneration`]
//!    (for `SYMFORGE_NO_DAEMON=1`, daemon-side `/v1/sessions/{id}/code-query`, or
//!    local MCP `SYMFORGE_SURFACE=code`).
//! 2. [`DaemonCodeBackend`]: proxies each `CodeReadOp` over an already-running
//!    authenticated local SymForge daemon without spawning a background daemon.
//! 3. Closures / mock backends for isolated sandbox containment tests.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::live_index::query::{SymbolSelectorMatch, resolve_symbol_selector};
use crate::live_index::store::{IndexState, LiveIndex, PublishedGeneration, PublishedSourceSet};
use crate::live_index::{
    ContextBundleView, InspectMatchView, SearchFilesResolveView, SearchFilesView, SharedIndex,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use super::catalog::CodeReadOp;

/// Error returned by a [`CodeReadBackend`] when a query is refused or invalid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeBackendError {
    pub kind: String,
    pub message: String,
}

impl CodeBackendError {
    #[must_use]
    pub fn invalid_args(message: impl Into<String>) -> Self {
        Self {
            kind: "invalid_request".to_owned(),
            message: message.into(),
        }
    }

    #[must_use]
    pub fn refusal(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for CodeBackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for CodeBackendError {}

/// Wire payload posted to `POST /v1/sessions/{session_id}/code-query`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DaemonCodeQueryRequest {
    pub op: CodeReadOp,
    #[serde(default)]
    pub kwargs: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

/// Observed omissions from one row-producing call, separate from its script value.
/// `omitted_rows` counts observed eligible rows clipped by limits, not withheld
/// files. It is only a lower bound when `omitted_rows_inexact` is true.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeReadCompleteness {
    pub omitted_rows: usize,
    pub withheld_files: usize,
    pub unparsed_files: usize,
    #[serde(default, skip_serializing_if = "is_false")]
    pub omitted_rows_inexact: bool,
    /// Files whose grammar cannot express the query. Informational: they cannot
    /// hold a match, so they never count as unparsed and never raise a notice alone.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub incompatible_grammar_files: usize,
    /// Returned rows whose source was cut by a token budget.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub truncated_rows: usize,
}

fn is_false(flag: &bool) -> bool {
    !*flag
}

fn is_zero(count: &usize) -> bool {
    *count == 0
}

impl CodeReadCompleteness {
    pub fn is_empty(&self) -> bool {
        Self {
            incompatible_grammar_files: 0,
            ..*self
        } == Self::default()
    }
}

#[derive(Debug, Clone)]
pub struct CodeReadResult {
    pub value: Value,
    pub completeness: CodeReadCompleteness,
}

/// Wire reply returned by `POST /v1/sessions/{session_id}/code-query`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DaemonCodeQueryResponse {
    Ok {
        value: Value,
        #[serde(default, skip_serializing_if = "CodeReadCompleteness::is_empty")]
        completeness: CodeReadCompleteness,
    },
    Error {
        error: CodeBackendError,
    },
}

/// Transport-neutral read-only query backend for `CodeModeRunner` and `symforge q`.
pub trait CodeReadBackend: Send + Sync {
    fn execute_op(
        &self,
        op: CodeReadOp,
        kwargs: Map<String, Value>,
    ) -> Result<Value, CodeBackendError>;

    fn execute_op_with_completeness(
        &self,
        op: CodeReadOp,
        kwargs: Map<String, Value>,
    ) -> Result<CodeReadResult, CodeBackendError> {
        self.execute_op(op, kwargs).map(|value| CodeReadResult {
            value,
            completeness: CodeReadCompleteness::default(),
        })
    }
}

impl<F> CodeReadBackend for F
where
    F: Fn(CodeReadOp, Map<String, Value>) -> Result<Value, CodeBackendError> + Send + Sync,
{
    fn execute_op(
        &self,
        op: CodeReadOp,
        kwargs: Map<String, Value>,
    ) -> Result<Value, CodeBackendError> {
        self(op, kwargs)
    }
}

/// In-process read-only backend backed by a `SharedIndex` and repository root.
pub struct LocalCodeBackend {
    index: SharedIndex,
    repo_root: Option<PathBuf>,
    project_name: String,
    project_state_dir: Option<crate::domain::ProjectStateDir>,
    pinned_snapshot: Mutex<Option<(Arc<PublishedGeneration>, Arc<PublishedSourceSet>)>>,
}

impl LocalCodeBackend {
    #[must_use]
    pub fn new(
        index: SharedIndex,
        repo_root: Option<PathBuf>,
        project_name: impl Into<String>,
        project_state_dir: Option<crate::domain::ProjectStateDir>,
    ) -> Self {
        Self {
            index,
            repo_root,
            project_name: project_name.into(),
            project_state_dir,
            pinned_snapshot: Mutex::new(None),
        }
    }

    fn snapshot_pair(&self) -> (Arc<PublishedGeneration>, Arc<PublishedSourceSet>) {
        let mut slot = self.pinned_snapshot.lock();
        if let Some(pair) = slot.as_ref() {
            return (Arc::clone(&pair.0), Arc::clone(&pair.1));
        }
        let source_set = self.index.published_source_set();
        let generation = source_set.current_generation();
        *slot = Some((Arc::clone(&generation), Arc::clone(&source_set)));
        (generation, source_set)
    }
    /// Loads a repository root in-process without spawning a daemon:
    /// restores `.symforge/index.bin` when valid, or builds in-memory read-only.
    pub fn open_read_only(root: &Path) -> anyhow::Result<Self> {
        let canonical = root.canonicalize()?;
        let binding = match crate::discovery::resolve_root_candidate(
            &canonical,
            crate::domain::RootCandidateSource::LaunchCwd,
            crate::domain::RootRequestMode::Automatic,
        ) {
            crate::domain::RootResolution::Bound(b) => b,
            other => {
                anyhow::bail!(
                    "project root '{}' refused by canonical source guard: {other:?}",
                    canonical.display()
                );
            }
        };
        let state_placement = crate::discovery::resolve_state_placement(&binding);
        let project_state_dir = state_placement.directory().cloned();
        let source_authority =
            crate::index_lifecycle::activation::project_source_authority(&binding.canonical_root);

        let shared: SharedIndex = if let Some(snapshot) =
            crate::live_index::persist::load_snapshot_bound(
                &binding.canonical_root,
                &state_placement,
                &source_authority,
            )
            .map_err(|r| anyhow::anyhow!("snapshot load refused: {r:?}"))?
        {
            let snapshot_mtimes: std::collections::HashMap<String, u64> = snapshot
                .files
                .iter()
                .map(|(path, file)| (path.clone(), file.mtime_secs))
                .collect();
            let (live, code_signals) =
                crate::live_index::persist::snapshot_to_live_index_with_code_signals_bound(
                    snapshot,
                    &binding.canonical_root,
                    &source_authority,
                )
                .map_err(|r| anyhow::anyhow!("snapshot restore refused: {r:?}"))?;
            let restored = crate::live_index::SharedIndexHandle::shared_restored_for_state_placement_with_code_signals(
                live,
                &binding.canonical_root,
                &state_placement,
                code_signals,
            );
            crate::live_index::persist::background_verify_bound(
                &restored,
                &binding.canonical_root,
                &snapshot_mtimes,
                &source_authority,
                || false,
            );
            restored
        } else if matches!(
            state_placement,
            crate::domain::StatePlacement::ProjectLocal { .. }
        ) {
            LiveIndex::load_for_state_placement(&binding.canonical_root, &state_placement)?
        } else {
            let exclusions = crate::discovery::SourceExclusions::for_state_placement(
                &binding.canonical_root,
                &state_placement,
            );
            let idx = LiveIndex::empty();
            idx.reload_for_binding_with_exclusions(
                &binding.canonical_root,
                project_state_dir.clone(),
                exclusions,
            )?;
            idx
        };

        let project_name = binding
            .canonical_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project")
            .to_owned();

        Ok(Self::new(
            shared,
            Some(binding.canonical_root),
            project_name,
            project_state_dir,
        ))
    }

    fn bump_commitment_frecency(&self, path: &str) {
        if let Some(root) = &self.repo_root {
            crate::live_index::frecency::bump(
                root,
                self.project_state_dir.as_ref(),
                &[PathBuf::from(path)],
            );
        }
    }
}

impl CodeReadBackend for LocalCodeBackend {
    fn execute_op(
        &self,
        op: CodeReadOp,
        kwargs: Map<String, Value>,
    ) -> Result<Value, CodeBackendError> {
        self.execute_op_with_completeness(op, kwargs)
            .map(|result| result.value)
    }

    fn execute_op_with_completeness(
        &self,
        op: CodeReadOp,
        kwargs: Map<String, Value>,
    ) -> Result<CodeReadResult, CodeBackendError> {
        let (generation, source_set) = self.snapshot_pair();
        execute_op_on_generation(
            op,
            kwargs,
            &generation,
            &source_set,
            self.repo_root.as_deref(),
            &self.project_name,
            &|path| self.bump_commitment_frecency(path),
        )
    }
}

/// Evaluates one `CodeReadOp` against an immutable [`PublishedGeneration`].
///
/// Shared by `LocalCodeBackend`, the daemon `/code-query` handler, and local MCP
/// `SYMFORGE_SURFACE=code` so warm-daemon and local-fallback modes produce
/// byte-identical structured JSON.
pub fn execute_op_on_generation(
    op: CodeReadOp,
    kwargs: Map<String, Value>,
    generation: &Arc<PublishedGeneration>,
    source_set: &Arc<PublishedSourceSet>,
    repo_root: Option<&Path>,
    project_name: &str,
    on_commitment_read: &dyn Fn(&str),
) -> Result<CodeReadResult, CodeBackendError> {
    let mut completeness = CodeReadCompleteness::default();
    let value = execute_value_on_generation(
        op,
        kwargs,
        generation,
        source_set,
        repo_root,
        project_name,
        on_commitment_read,
        &mut completeness,
    )?;
    Ok(CodeReadResult {
        value,
        completeness,
    })
}

#[allow(clippy::too_many_arguments)]
fn execute_value_on_generation(
    op: CodeReadOp,
    mut kwargs: Map<String, Value>,
    generation: &Arc<PublishedGeneration>,
    source_set: &Arc<PublishedSourceSet>,
    repo_root: Option<&Path>,
    project_name: &str,
    on_commitment_read: &dyn Fn(&str),
    completeness: &mut CodeReadCompleteness,
) -> Result<Value, CodeBackendError> {
    // Strip optional caller `project` if present after routing.
    kwargs.remove("project");

    match generation.live.index_state() {
        IndexState::Ready => {}
        IndexState::Loading => {
            return Err(CodeBackendError::refusal(
                "index_loading",
                "index is still loading; retry shortly",
            ));
        }
        IndexState::Empty => {
            return Err(CodeBackendError::refusal(
                "index_empty",
                "index is empty; index the repository first",
            ));
        }
        IndexState::CircuitBreakerTripped { summary } => {
            return Err(CodeBackendError::refusal(
                "index_degraded",
                format!("index degraded: {summary}"),
            ));
        }
    }

    validate_op_contract(op, &mut kwargs)?;

    match op {
        CodeReadOp::SearchSymbols => {
            let input: crate::protocol::tools::SearchSymbolsInput = decode_op_args(op, kwargs)?;
            if let Some(q) = input.query.as_deref()
                && crate::knowledge::guard_query(q).is_err()
            {
                return Err(CodeBackendError::refusal(
                    "policy_withheld",
                    "sensitive query rejected by repository safety policy",
                ));
            }
            let refused = search_refusal_predicate(generation);
            let exec =
                crate::index_lifecycle::guidance::search::execute_symbol_search_for_code_mode(
                    &generation.live,
                    &input,
                    &refused,
                )
                .map_err(CodeBackendError::invalid_args)?;

            completeness.omitted_rows = exec.result.overflow_count;
            completeness.withheld_files = crate::live_index::search::count_code_mode_withheld(
                &generation.live,
                &crate::live_index::search::TextSearchOptions {
                    path_scope: exec.options.path_scope,
                    search_scope: exec.options.search_scope,
                    noise_policy: exec.options.noise_policy,
                    include_personal_tooling: exec.options.include_personal_tooling,
                    language_filter: exec.options.language_filter,
                    ..Default::default()
                },
                code_mode_withheld(generation),
                false,
            )
            .map_err(|err| CodeBackendError::invalid_args(format!("{err:?}")))?;
            let mut out = Vec::with_capacity(exec.result.hits.len());
            for hit in &exec.result.hits {
                let tier = match hit.tier {
                    crate::live_index::search::SymbolMatchTier::Exact => "exact",
                    crate::live_index::search::SymbolMatchTier::Prefix => "prefix",
                    crate::live_index::search::SymbolMatchTier::Substring => "substring",
                };
                let end_line = generation
                    .live
                    .get_file(&hit.path)
                    .and_then(|f| {
                        f.symbols.iter().find(|s| {
                            s.name == hit.name && s.line_range.0.saturating_add(1) == hit.line
                        })
                    })
                    .map_or(hit.line, |s| s.line_range.1.saturating_add(1));
                out.push(json!({
                    "path": hit.path,
                    "name": hit.name,
                    "kind": hit.kind,
                    "start_line": hit.line,
                    "end_line": end_line,
                    "tier": tier,
                }));
            }
            Ok(Value::Array(out))
        }

        CodeReadOp::GetSymbol => {
            let input: crate::protocol::tools::GetSymbolInput = decode_op_args(op, kwargs)?;
            let targets: Vec<crate::protocol::tools::SymbolTarget> =
                if let Some(list) = input.targets {
                    if list.is_empty() {
                        return Err(CodeBackendError::invalid_args(
                            "targets must not be empty when provided",
                        ));
                    }
                    let cap = crate::index_lifecycle::guidance::read_contract::MAX_SYMBOL_TARGETS;
                    if list.len() > cap {
                        return Err(CodeBackendError::invalid_args(format!(
                            "targets length {} exceeds maximum of {cap}",
                            list.len()
                        )));
                    }
                    list
                } else {
                    if input.name.trim().is_empty() {
                        return Err(CodeBackendError::invalid_args(
                            "get_symbol requires `name` (with optional `path`) or `targets=[...]`",
                        ));
                    }
                    let path = if input.path.trim().is_empty() {
                        let candidates =
                        crate::index_lifecycle::guidance::reference_read::symbol_candidate_paths(
                            &generation.live,
                            &input.name,
                        );
                        match candidates.len() {
                            1 => candidates.into_iter().next().expect("1 candidate"),
                            0 => {
                                return Ok(json!([{
                                    "path": "",
                                    "name": input.name,
                                    "kind": input.kind.unwrap_or_default(),
                                    "start_line": 0,
                                    "end_line": 0,
                                    "source": Value::Null,
                                    "refusal": "not_found",
                                    "withheld_reason": Value::Null,
                                }]));
                            }
                            _ => {
                                return Err(CodeBackendError::refusal(
                                    "ambiguous_symbol",
                                    format!(
                                        "symbol '{}' exists in multiple files ({}); pass `path`",
                                        input.name,
                                        candidates.join(", ")
                                    ),
                                ));
                            }
                        }
                    } else {
                        crate::index_lifecycle::guidance::read_gate::normalize_requested_path(
                            &input.path,
                        )
                    };
                    vec![crate::protocol::tools::SymbolTarget {
                        path,
                        name: Some(input.name),
                        kind: input.kind,
                        symbol_line: input.symbol_line,
                        start_byte: None,
                        end_byte: None,
                    }]
                };

            let max_bytes = input
                .max_tokens
                .filter(|t| *t > 0)
                .map(|t| (t as usize).saturating_mul(4));
            let source_row = |path: &str,
                              name: &str,
                              kind: &str,
                              symbol_line: Option<u32>,
                              content: &[u8],
                              (start, end): (usize, usize)| {
                let start = start.min(content.len());
                let end = end.clamp(start, content.len());
                let mut source = String::from_utf8_lossy(&content[start..end]).into_owned();
                let truncated = if let Some(max_bytes) = max_bytes.filter(|&n| n < source.len()) {
                    source.truncate(source.floor_char_boundary(max_bytes));
                    true
                } else {
                    false
                };
                let (start_line, end_line) =
                    crate::index_lifecycle::guidance::symbol_read::displayed_line_range(
                        content,
                        start,
                        source.as_bytes(),
                    );
                let mut row = json!({
                    "path": path,
                    "name": name,
                    "kind": kind,
                    "start_line": start_line,
                    "end_line": end_line,
                    "source": source,
                    "refusal": Value::Null,
                    "withheld_reason": Value::Null,
                });
                if let Some(symbol_line) = symbol_line {
                    row["symbol_line"] = json!(symbol_line);
                }
                if truncated {
                    row["truncated"] = json!(true);
                }
                on_commitment_read(path);
                row
            };

            let mut results = Vec::with_capacity(targets.len());
            for target in targets {
                let norm_path =
                    crate::index_lifecycle::guidance::read_gate::normalize_requested_path(
                        &target.path,
                    );
                if let Some(refusal) = check_secret_or_scope_refusal(&generation.live, &norm_path) {
                    results.push(json!({
                        "path": norm_path,
                        "name": target.name.unwrap_or_default(),
                        "kind": target.kind.unwrap_or_default(),
                        "start_line": 0,
                        "end_line": 0,
                        "source": Value::Null,
                        "refusal": "policy_withheld",
                        "withheld_reason": refusal,
                    }));
                    continue;
                }
                let Some(file) = generation.live.get_file(&norm_path) else {
                    results.push(json!({
                        "path": norm_path,
                        "name": target.name.unwrap_or_default(),
                        "kind": target.kind.unwrap_or_default(),
                        "start_line": 0,
                        "end_line": 0,
                        "source": Value::Null,
                        "refusal": "not_found",
                        "withheld_reason": Value::Null,
                    }));
                    continue;
                };
                if let Some(refusal) =
                    check_bytes_admission(&generation.live, &norm_path, &file.content)
                {
                    results.push(json!({
                        "path": norm_path,
                        "name": target.name.unwrap_or_default(),
                        "kind": target.kind.unwrap_or_default(),
                        "start_line": 0,
                        "end_line": 0,
                        "source": Value::Null,
                        "refusal": "policy_withheld",
                        "withheld_reason": refusal,
                    }));
                    continue;
                }
                if let Some(sym_name) = target.name.as_deref() {
                    match resolve_symbol_selector(
                        file,
                        sym_name,
                        target.kind.as_deref(),
                        target.symbol_line,
                    ) {
                        SymbolSelectorMatch::Selected(_idx, sym) => {
                            let span = sym.display_span(Some(file.language));
                            results.push(source_row(
                                &norm_path,
                                &sym.name,
                                &sym.kind.to_string(),
                                Some(sym.line_range.0 + 1),
                                &file.content,
                                span,
                            ));
                        }
                        SymbolSelectorMatch::NotFound => {
                            results.push(json!({
                                "path": norm_path,
                                "name": sym_name,
                                "kind": target.kind.unwrap_or_default(),
                                "start_line": 0,
                                "end_line": 0,
                                "source": Value::Null,
                                "refusal": "not_found",
                                "withheld_reason": Value::Null,
                            }));
                        }
                        SymbolSelectorMatch::Ambiguous(_) => {
                            results.push(json!({
                                "path": norm_path,
                                "name": sym_name,
                                "kind": target.kind.unwrap_or_default(),
                                "start_line": 0,
                                "end_line": 0,
                                "source": Value::Null,
                                "refusal": "ambiguous_symbol",
                                "withheld_reason": Value::Null,
                            }));
                        }
                    }
                } else if let Some(sb) = target.start_byte {
                    let end = target.end_byte.map_or(file.content.len(), |eb| eb as usize);
                    results.push(source_row(
                        &norm_path,
                        "",
                        "slice",
                        None,
                        &file.content,
                        (sb as usize, end),
                    ));
                } else {
                    return Err(CodeBackendError::invalid_args(
                        "each target requires `name` or `start_byte`",
                    ));
                }
            }
            completeness.truncated_rows = results
                .iter()
                .filter(|row| row.get("truncated") == Some(&Value::Bool(true)))
                .count();
            Ok(Value::Array(results))
        }

        CodeReadOp::FindReferences => {
            let context = take_hit_context(&mut kwargs)?;
            let input: crate::protocol::tools::FindReferencesInput = decode_op_args(op, kwargs)?;
            if input.name.trim().is_empty() {
                return Err(CodeBackendError::invalid_args("name must be non-empty"));
            }
            if crate::knowledge::guard_query(&input.name).is_err() {
                return Err(CodeBackendError::refusal(
                    "policy_withheld",
                    "sensitive query rejected by repository safety policy",
                ));
            }
            let live = &generation.live;
            let refuse = search_refusal_predicate(generation);
            if input.mode.as_deref() == Some("implementations") {
                if context > 0 {
                    return Err(CodeBackendError::invalid_args(
                        "context applies to mode='references' only",
                    ));
                }
                let cap = input.limit.unwrap_or(200).clamp(1, 500) as usize;
                let impls = generation
                    .live
                    .capture_implementations_view(&input.name, input.direction.as_deref());
                let mut refused =
                    withheld_reference_corpus(live, code_mode_withheld(generation), None, None);
                let mut entries = impls.entries;
                entries.retain(|entry| {
                    let admitted = !refuse(&entry.file_path);
                    if !admitted {
                        refused.insert(entry.file_path.clone());
                    }
                    admitted
                });
                completeness.withheld_files = refused.len();
                completeness.omitted_rows = entries.len().saturating_sub(cap);
                let rows: Vec<Value> = entries
                    .into_iter()
                    .take(cap)
                    .map(|entry| {
                        let enc_line = generation.live.get_file(&entry.file_path).and_then(|f| {
                            f.symbols
                                .iter()
                                .find(|s| s.name == entry.implementor)
                                .map(|s| s.line_range.0.saturating_add(1))
                        });
                        json!({
                            "path": entry.file_path,
                            "line": entry.line,
                            "text": format!("impl {} for {}", entry.trait_name, entry.implementor),
                            "enclosing_symbol": entry.implementor,
                            "enclosing_line": enc_line,
                            "caller_kind": "impl",
                        })
                    })
                    .collect();
                return Ok(Value::Array(rows));
            }

            let limits = crate::index_lifecycle::guidance::reference_read::OutputLimits::new(
                input.limit.unwrap_or(20),
                input.max_per_file.unwrap_or(10),
            );
            let kind = crate::live_index::disambiguation::parse_reference_kind_filter(
                input.kind.as_deref(),
            );
            let target = input
                .path
                .as_deref()
                .map(crate::index_lifecycle::guidance::read_gate::normalize_requested_path);
            let refs = if let Some(path) = target.as_deref() {
                if refuse(path) {
                    return Err(CodeBackendError::refusal(
                        "policy_withheld",
                        "target excluded by repository safety policy",
                    ));
                }
                live.find_exact_references_for_symbol(
                    path,
                    &input.name,
                    input.symbol_kind.as_deref(),
                    input.symbol_line,
                    kind,
                )
                .map_err(CodeBackendError::invalid_args)?
            } else {
                live.find_references_for_name(&input.name, kind, false)
            };
            let mut by_file = std::collections::HashMap::<String, Vec<_>>::new();
            for reference in refs {
                by_file
                    .entry(reference.0.to_owned())
                    .or_default()
                    .push(reference);
            }
            let target_lang = target
                .as_deref()
                .and_then(|p| live.get_file(p))
                .map(|f| f.language);
            let mut withheld =
                withheld_reference_corpus(live, code_mode_withheld(generation), target_lang, None);
            let mut selected = Vec::new();
            let mut shown_files = 0;
            for path in
                live.order_find_references_file_paths_fair(by_file.keys().cloned().collect())
            {
                let mut refs = by_file.remove(&path).expect("ordered reference path");
                if refuse(&path) {
                    withheld.insert(path);
                    continue;
                }
                let keep = if shown_files < limits.max_files {
                    shown_files += 1;
                    refs.len().min(limits.max_per_file)
                } else {
                    0
                };
                completeness.omitted_rows += refs.len() - keep;
                refs.sort_by_key(|(_, r)| (r.line_range.0, r.byte_range.0));
                selected.extend(refs.into_iter().take(keep));
            }
            completeness.withheld_files = withheld.len();
            let view = live.build_find_references_view(
                &selected,
                limits.total_hits,
                &input.name,
                context.max(crate::live_index::query::REFERENCE_CONTEXT_LINES),
            );
            let mut out = Vec::new();
            for file in view.files {
                let idx_file = generation.live.get_file(&file.file_path);
                for hit in file.hits {
                    if let Some(ref_line) = hit.context_lines.iter().find(|l| l.is_reference_line) {
                        let (enc_name, enc_kind, enc_line) = idx_file
                            .and_then(|f| {
                                let zb = ref_line.line_number.saturating_sub(1);
                                crate::domain::find_enclosing_symbol(&f.symbols, zb)
                                    .and_then(|i| f.symbols.get(i as usize))
                            })
                            .map(|s| {
                                (
                                    Some(s.name.clone()),
                                    Some(s.kind.to_string()),
                                    Some(s.line_range.0.saturating_add(1)),
                                )
                            })
                            .unwrap_or((None, None, None));
                        let mut row = json!({
                            "path": file.file_path,
                            "line": ref_line.line_number,
                            "text": ref_line.text,
                            "enclosing_symbol": enc_name,
                            "enclosing_line": enc_line,
                            "caller_kind": enc_kind,
                        });
                        if context > 0 {
                            row["context"] = Value::String(numbered_window(
                                hit.context_lines
                                    .iter()
                                    .map(|l| (l.line_number as usize, l.text.as_str())),
                            ));
                        }
                        out.push(row);
                    }
                }
            }
            Ok(Value::Array(out))
        }

        CodeReadOp::FindDependents => {
            let input: crate::protocol::tools::FindDependentsInput = decode_op_args(op, kwargs)?;
            let norm_path =
                crate::index_lifecycle::guidance::read_gate::normalize_requested_path(&input.path);
            if let Some(refusal) = check_secret_or_scope_refusal(&generation.live, &norm_path) {
                return Err(CodeBackendError::refusal("policy_withheld", refusal));
            }
            if let Some(target_file) = generation.live.get_file(&norm_path)
                && let Some(refusal) =
                    check_bytes_admission(&generation.live, &norm_path, &target_file.content)
            {
                return Err(CodeBackendError::refusal("policy_withheld", refusal));
            }
            let max_files = input.limit.unwrap_or(20).clamp(1, 100) as usize;
            let max_per_file = input.max_per_file.unwrap_or(10).clamp(1, 50) as usize;
            let live = &generation.live;
            let Some(target_file) = live.get_file(&norm_path) else {
                if live
                    .metadata_only_skipped_paths()
                    .any(|(path, _)| path == norm_path)
                {
                    completeness.withheld_files = 1;
                    return Ok(Value::Array(Vec::new()));
                }
                return Err(CodeBackendError::refusal(
                    "not_found",
                    format!("File not in the index: {norm_path}"),
                ));
            };
            let refuse = search_refusal_predicate(generation);
            let mut withheld = withheld_reference_corpus(
                live,
                code_mode_withheld(generation),
                Some(target_file.language),
                Some(&norm_path),
            );
            let view = live.capture_find_dependents_view(&norm_path);
            let mut out = Vec::new();
            let mut shown_files = 0;
            let name = input.name.as_deref().filter(|name| !name.is_empty());
            for file in view.files {
                let mut lines = file.lines;
                lines.retain(|line| name.is_none_or(|name| line.name == name));
                if lines.is_empty() {
                    continue;
                }
                if refuse(&file.file_path)
                    || generation
                        .live
                        .get_file(&file.file_path)
                        .is_some_and(|dep_file| {
                            check_bytes_admission(
                                &generation.live,
                                &file.file_path,
                                &dep_file.content,
                            )
                            .is_some()
                        })
                {
                    withheld.insert(file.file_path);
                    continue;
                }
                let keep = if shown_files < max_files {
                    shown_files += 1;
                    lines.len().min(max_per_file)
                } else {
                    0
                };
                completeness.omitted_rows += lines.len() - keep;
                for line in lines.into_iter().take(keep) {
                    out.push(json!({
                        "path": file.file_path,
                        "line": line.line_number,
                        "kind": line.kind,
                        "name": line.name,
                        "text": line.line_content,
                    }));
                }
            }
            completeness.withheld_files = withheld.len();
            Ok(Value::Array(out))
        }

        CodeReadOp::FileContext => {
            let include_tests_flag = kwargs
                .get("include_tests")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let input: crate::protocol::tools::GetFileContextInput = decode_op_args(op, kwargs)?;
            if let Some(secs) = crate::protocol::read_tools::visible_sections(&input.sections) {
                for sec in &secs {
                    if !matches!(sec.as_str(), "outline" | "imports" | "consumers") {
                        return Err(CodeBackendError::invalid_args(
                            "invalid_request: file_context sections in Code Mode must be chosen from: \"outline\", \"imports\", \"consumers\".",
                        ));
                    }
                }
            }
            let norm_path =
                crate::index_lifecycle::guidance::read_gate::normalize_requested_path(&input.path);
            if let Some(refusal) = check_secret_or_scope_refusal(&generation.live, &norm_path) {
                return Err(CodeBackendError::refusal("policy_withheld", refusal));
            }
            let Some(file) = generation.live.get_file(&norm_path) else {
                return Err(CodeBackendError::refusal(
                    "not_found",
                    format!("File not found: {norm_path}"),
                ));
            };
            if let Some(refusal) =
                check_bytes_admission(&generation.live, &norm_path, &file.content)
            {
                return Err(CodeBackendError::refusal("policy_withheld", refusal));
            }
            let raw_tokens =
                crate::stel::handler::estimate_tokens_from_bytes(file.content.len() as u64);
            // Build structured outline + imports + consumers directly from IndexedFile & LiveIndex
            let selected: Vec<String> = crate::protocol::read_tools::visible_sections(
                &input.sections,
            )
            .unwrap_or_else(|| {
                vec![
                    "outline".to_owned(),
                    "imports".to_owned(),
                    "consumers".to_owned(),
                ]
            });
            let mut rendered_parts = Vec::new();
            if selected.iter().any(|s| s == "outline") {
                let mut lines = vec![format!("## {norm_path} ({})", file.language.name())];
                for sym in &file.symbols {
                    if !include_tests_flag && sym.name == "tests" && sym.kind.to_string() == "mod" {
                        continue;
                    }
                    let indent = "  ".repeat(sym.depth as usize);
                    lines.push(format!(
                        "{indent}{} {} (L{}-{})",
                        sym.kind,
                        sym.name,
                        sym.line_range.0 + 1,
                        sym.line_range.1 + 1
                    ));
                }
                rendered_parts.push(lines.join("\n"));
            }
            if selected.iter().any(|s| s == "imports") {
                let imports: Vec<String> = file
                    .references
                    .iter()
                    .filter(|r| r.kind == crate::domain::ReferenceKind::Import)
                    .map(|r| {
                        format!(
                            "L{}: {}",
                            r.line_range.0 + 1,
                            r.qualified_name.as_deref().unwrap_or(&r.name)
                        )
                    })
                    .take(50)
                    .collect();
                if !imports.is_empty() {
                    rendered_parts.push(format!("### Imports\n{}", imports.join("\n")));
                }
            }
            if selected.iter().any(|s| s == "consumers") {
                let deps = generation.live.capture_find_dependents_view(&norm_path);
                if !deps.files.is_empty() {
                    let consumer_paths: Vec<String> = deps
                        .files
                        .into_iter()
                        .take(30)
                        .map(|f| f.file_path)
                        .collect();
                    rendered_parts.push(format!("### Consumers\n{}", consumer_paths.join("\n")));
                }
            }
            let mut rendered = rendered_parts.join("\n\n");
            if let Some(max_tok) = input.max_tokens.filter(|t| *t > 0) {
                rendered = crate::protocol::format::enforce_token_budget(rendered, Some(max_tok));
            }
            let est_tokens =
                crate::stel::handler::estimate_tokens_from_bytes(rendered.len() as u64);
            on_commitment_read(&norm_path);
            Ok(json!({
                "path": norm_path,
                "rendered": rendered,
                "selected_sections": selected,
                "estimated_tokens": est_tokens,
                "raw_file_tokens": raw_tokens,
            }))
        }

        CodeReadOp::FileContent => {
            let mut input: crate::protocol::tools::GetFileContentInput =
                decode_op_args(op, kwargs)?;
            input.path =
                crate::index_lifecycle::guidance::read_gate::normalize_requested_path(&input.path);
            crate::protocol::read_tools::normalize_file_content_aliases(&mut input)
                .map_err(CodeBackendError::invalid_args)?;

            if let Some(refusal) = check_secret_or_scope_refusal(&generation.live, &input.path) {
                return Err(CodeBackendError::refusal("policy_withheld", refusal));
            }
            let options = crate::protocol::read_tools::file_content_options_from_input(&input)
                .map_err(CodeBackendError::invalid_args)?;

            let bytes: Vec<u8> = if let Some(file) = generation.live.get_file(&input.path) {
                if let Some(refusal) =
                    check_bytes_admission(&generation.live, &input.path, &file.content)
                {
                    return Err(CodeBackendError::refusal("policy_withheld", refusal));
                }
                file.content.clone()
            } else if let Some(root) = repo_root {
                let _canon = crate::protocol::edit::safe_repo_path(root, &input.path)
                    .map_err(|msg| CodeBackendError::refusal("invalid_path", msg))?;
                crate::protocol::read_gate::observe_disk_beneath(
                    generation.live.as_ref(),
                    root,
                    &input.path,
                )
                .map_err(|refusal| CodeBackendError::refusal("policy_withheld", refusal))?
            } else {
                return Err(CodeBackendError::refusal(
                    "not_found",
                    format!("File not found: {}", input.path),
                ));
            };

            let content_hash = crate::hash::digest_hex(&bytes);
            let mut rendered =
                crate::index_lifecycle::guidance::file_read::render_file_content_bytes(
                    &input.path,
                    &bytes,
                    options.content_context,
                );
            if let Some(max_tok) = input.max_tokens.filter(|t| *t > 0) {
                rendered = crate::protocol::format::enforce_token_budget(rendered, Some(max_tok));
            }
            let approx_tokens =
                crate::stel::handler::estimate_tokens_from_bytes(rendered.len() as u64);
            on_commitment_read(&input.path);
            Ok(json!({
                "path": input.path,
                "content_hash": content_hash,
                "content": rendered,
                "approximate_tokens": approx_tokens,
            }))
        }

        CodeReadOp::SearchText => {
            let input: crate::protocol::tools::SearchTextInput = decode_op_args(op, kwargs)?;
            if let Some(q) = input.query.as_deref()
                && crate::knowledge::guard_query(q).is_err()
            {
                return Err(CodeBackendError::refusal(
                    "policy_withheld",
                    "sensitive query rejected by repository safety policy",
                ));
            }
            let refused = search_refusal_predicate(generation);
            let exec = crate::index_lifecycle::guidance::search::execute_text_search_for_code_mode(
                generation, &input, &refused,
            )
            .map_err(CodeBackendError::invalid_args)?;
            let res = exec
                .result
                .map_err(|e| CodeBackendError::invalid_args(format!("{e:?}")))?;
            completeness.withheld_files = crate::live_index::search::count_code_mode_withheld(
                &generation.live,
                &exec.options,
                code_mode_withheld(generation),
                exec.structural,
            )
            .map_err(|e| CodeBackendError::invalid_args(format!("{e:?}")))?;
            completeness.unparsed_files = res.unparsed_files;
            completeness.omitted_rows_inexact = res.unparsed_files != 0;
            completeness.incompatible_grammar_files = exec.incompatible_grammar_files;

            if input.group_by.as_deref() == Some("count") {
                let cap = input.limit.unwrap_or(200).clamp(1, 1000) as usize;
                let mut counts: Vec<Value> = Vec::new();
                completeness.omitted_rows = res
                    .files
                    .iter()
                    .filter(|file| !file.matches.is_empty())
                    .count()
                    .saturating_sub(cap);
                let files = res
                    .files
                    .into_iter()
                    .filter(|file| !file.matches.is_empty());
                for file in files.take(cap) {
                    counts.push(json!({
                        "path": file.path,
                        "count": file.matches.len(),
                    }));
                }
                return Ok(Value::Array(counts));
            }

            let cap = input.limit.unwrap_or(50).clamp(1, 500) as usize;
            let available: usize = res.files.iter().map(|file| file.matches.len()).sum();
            completeness.omitted_rows = res.overflow_count + available.saturating_sub(cap);
            let mut out = Vec::new();
            for file in res.files {
                for m in file.matches {
                    if out.len() >= cap {
                        break;
                    }
                    let mut row = json!({
                        "path": file.path,
                        "line": m.line_number,
                        "preview": m.line,
                        "enclosing_symbol": m.enclosing_symbol.map(|s| s.name),
                    });
                    if let Some(n) = input.context.filter(|n| *n > 0) {
                        row["context"] = Value::String(numbered_window(
                            file.rendered_lines
                                .iter()
                                .flatten()
                                .filter_map(|l| match l {
                                    crate::live_index::search::TextDisplayLine::Line(l)
                                        if l.line_number.abs_diff(m.line_number) <= n as usize =>
                                    {
                                        Some((l.line_number, l.line.as_str()))
                                    }
                                    _ => None,
                                }),
                        ));
                    }
                    out.push(row);
                }
                if out.len() >= cap {
                    break;
                }
            }
            Ok(Value::Array(out))
        }

        CodeReadOp::SearchFiles => {
            let input: crate::protocol::tools::SearchFilesInput = decode_op_args(op, kwargs)?;
            let q = input.query.trim();
            if q.is_empty() {
                return Err(CodeBackendError::invalid_args("query must be non-empty"));
            }
            let scope = match input.path_prefix.as_deref().map(str::trim) {
                Some(p) if !p.is_empty() => {
                    crate::live_index::search::PathScope::Prefix(p.to_owned())
                }
                _ => crate::live_index::search::PathScope::Any,
            };
            if input.resolve == Some(true) {
                let view = generation
                    .live
                    .capture_search_files_resolve_view_with_scope(
                        q,
                        input.include_vendor.unwrap_or(false),
                        input.include_personal_tooling.unwrap_or(false),
                        &scope,
                    );
                return Ok(match view {
                    SearchFilesResolveView::Resolved { path } => json!([{
                        "path": path,
                        "tier": "strong_path",
                        "coupling_score": Value::Null,
                    }]),
                    SearchFilesResolveView::ResolvedMetadataOnly { path, .. } => json!([{
                        "path": path,
                        "tier": "metadata_only",
                        "coupling_score": Value::Null,
                    }]),
                    SearchFilesResolveView::Ambiguous { matches, .. } => Value::Array(
                        matches
                            .into_iter()
                            .map(|p| json!({"path": p, "tier": "ambiguous", "coupling_score": Value::Null}))
                            .collect(),
                    ),
                    _ => json!([]),
                });
            }

            let limit = input.limit.unwrap_or(20).clamp(1, 100) as usize;
            let view = generation.live.capture_search_files_view_with_noise(
                q,
                limit,
                input.current_file.as_deref(),
                None,
                input.include_vendor.unwrap_or(false),
                input.include_personal_tooling.unwrap_or(false),
                &scope,
            );
            let out = match view {
                SearchFilesView::Found {
                    hits,
                    overflow_count,
                    ..
                } => {
                    completeness.omitted_rows = overflow_count;
                    hits.into_iter()
                        .map(|h| {
                            let tier = match h.tier {
                                crate::live_index::query::SearchFilesTier::CoChange => "co_change",
                                crate::live_index::query::SearchFilesTier::StrongPath => {
                                    "strong_path"
                                }
                                crate::live_index::query::SearchFilesTier::Basename => "basename",
                                crate::live_index::query::SearchFilesTier::LoosePath => {
                                    "loose_path"
                                }
                                crate::live_index::query::SearchFilesTier::MetadataOnly => {
                                    "metadata_only"
                                }
                            };
                            json!({
                                "path": h.path,
                                "tier": tier,
                                "coupling_score": h.coupling_score,
                            })
                        })
                        .collect()
                }
                _ => Vec::new(),
            };
            Ok(Value::Array(out))
        }

        CodeReadOp::RepoMap => {
            let input: crate::protocol::tools::GetRepoMapInput = decode_op_args(op, kwargs)?;
            let detail = input.detail.as_deref().unwrap_or("compact");
            let max_files = input.max_files.unwrap_or(200) as usize;
            let clean_prefix = input
                .path
                .as_deref()
                .map(|p| p.replace('\\', "/"))
                .map(|p| {
                    let mut s = p.as_str();
                    while let Some(rest) = s.strip_prefix("./") {
                        s = rest;
                    }
                    s.trim_matches('/').to_owned()
                })
                .filter(|p| !p.is_empty() && p != ".");
            let scoped_count = match clean_prefix.as_deref() {
                None => generation.outline.files.len(),
                Some(pfx) => generation
                    .outline
                    .files
                    .iter()
                    .filter(|f| {
                        let rp = f.relative_path.as_str();
                        rp == pfx
                            || (rp.starts_with(pfx) && rp.as_bytes().get(pfx.len()) == Some(&b'/'))
                    })
                    .count(),
            };
            let (rendered, total_files, shown_files) = match detail {
                "full" => {
                    let mut files = generation.outline.files.clone();
                    if let Some(pfx) = clean_prefix.as_deref() {
                        files.retain(|f| {
                            let rp = f.relative_path.as_str();
                            rp == pfx
                                || (rp.starts_with(pfx)
                                    && rp.as_bytes().get(pfx.len()) == Some(&b'/'))
                        });
                    }
                    let total = files.len() as u64;
                    files.truncate(max_files);
                    let shown = files.len() as u64;
                    let total_symbols: usize = files.iter().map(|f| f.symbol_count).sum();
                    let view = crate::live_index::query::RepoOutlineView {
                        total_files: total as usize,
                        total_symbols,
                        files,
                    };
                    (
                        crate::protocol::format::repo_outline_view(&view, project_name),
                        total,
                        shown,
                    )
                }
                "tree" => {
                    let path = clean_prefix.as_deref().unwrap_or("");
                    let depth = input.depth.unwrap_or(2).min(5);
                    let skipped = generation.live.compatibility_skipped_files();
                    let text = crate::protocol::format::file_tree_view_with_skipped(
                        &generation.outline.files,
                        &skipped,
                        path,
                        depth,
                    );
                    let total = scoped_count as u64;
                    (text, total, total)
                }
                _ => {
                    let text = crate::index_lifecycle::guidance::read_context::repo_map_text_scoped(
                        generation,
                        input.path.as_deref(),
                        input.depth,
                    );
                    let total = scoped_count as u64;
                    (text, total, total)
                }
            };
            Ok(json!({
                "detail": detail,
                "rendered": rendered,
                "total_files": total_files,
                "shown_files": shown_files,
            }))
        }

        CodeReadOp::SymbolContext => {
            let include_tests_flag = kwargs
                .get("include_tests")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let input: crate::protocol::tools::GetSymbolContextInput = decode_op_args(op, kwargs)?;
            if input.name.trim().is_empty() {
                return Err(CodeBackendError::invalid_args("name must be non-empty"));
            }
            let target_path = if let Some(p) = input.path.as_deref().or(input.file.as_deref()) {
                crate::index_lifecycle::guidance::read_gate::normalize_requested_path(p)
            } else {
                let candidates =
                    crate::index_lifecycle::guidance::reference_read::symbol_candidate_paths(
                        &generation.live,
                        &input.name,
                    );
                match candidates.len() {
                    1 => candidates.into_iter().next().expect("1 candidate"),
                    0 => {
                        return Err(CodeBackendError::refusal(
                            "not_found",
                            format!("Symbol '{}' not found in index", input.name),
                        ));
                    }
                    _ => {
                        return Err(CodeBackendError::refusal(
                            "ambiguous_symbol",
                            format!(
                                "Symbol '{}' is defined in multiple files ({}); pass `path`",
                                input.name,
                                candidates.join(", ")
                            ),
                        ));
                    }
                }
            };
            if let Some(refusal) = check_secret_or_scope_refusal(&generation.live, &target_path) {
                return Err(CodeBackendError::refusal("policy_withheld", refusal));
            }
            if let Some(target_file) = generation.live.get_file(&target_path)
                && let Some(refusal) =
                    check_bytes_admission(&generation.live, &target_path, &target_file.content)
            {
                return Err(CodeBackendError::refusal("policy_withheld", refusal));
            }
            const CODE_MODE_CALLERS_CAP: usize = 100;
            const CODE_MODE_SECTION_CAP: usize = 20;
            let view = generation.live.capture_context_bundle_view_with_cap(
                &target_path,
                &input.name,
                input.symbol_kind.as_deref(),
                input.symbol_line,
                usize::MAX,
            );
            match view {
                ContextBundleView::Found(found) => {
                    on_commitment_read(&target_path);
                    let is_test_entry = |path: &str| -> bool {
                        generation
                            .live
                            .get_file(path)
                            .map(|f| f.classification.is_test)
                            .unwrap_or_else(|| {
                                crate::domain::index::FileClassification::for_code_path(path)
                                    .is_test
                            })
                    };
                    let filtered_callers: Vec<&crate::live_index::ContextBundleReferenceView> =
                        found
                            .callers
                            .entries
                            .iter()
                            .filter(|e| include_tests_flag || !is_test_entry(&e.file_path))
                            .collect();
                    let total_callers = filtered_callers.len();
                    let section_omitted =
                        |sec: &crate::live_index::ContextBundleSectionView| -> usize {
                            sec.overflow_count
                                + sec
                                    .entries
                                    .iter()
                                    .filter(|e| include_tests_flag || !is_test_entry(&e.file_path))
                                    .count()
                                    .saturating_sub(CODE_MODE_SECTION_CAP)
                        };
                    completeness.omitted_rows = total_callers.saturating_sub(CODE_MODE_CALLERS_CAP)
                        + section_omitted(&found.callees)
                        + section_omitted(&found.type_usages);
                    let callers_json: Vec<Value> = filtered_callers
                        .into_iter()
                        .take(CODE_MODE_CALLERS_CAP)
                        .map(|e| {
                            json!({
                                "name": e.enclosing_name.as_deref().unwrap_or("<module>"),
                                "path": e.file_path,
                                "caller_kind": e.enclosing_kind,
                                "caller_line": e.enclosing_start_line,
                                "call_line": e.line_number,
                                "call_expr": e.display_name,
                                "confidence": e.confidence,
                                "occurrences": e.occurrence_count,
                            })
                        })
                        .collect();
                    let map_refs =
                        |sec: &crate::live_index::ContextBundleSectionView| -> Vec<Value> {
                            sec.entries
                                .iter()
                                .filter(|e| include_tests_flag || !is_test_entry(&e.file_path))
                                .take(CODE_MODE_SECTION_CAP)
                                .map(|e| {
                                    json!({
                                        "name": e.display_name,
                                        "path": e.file_path,
                                        "line": e.line_number,
                                        "enclosing": e.enclosing,
                                        "occurrences": e.occurrence_count,
                                    })
                                })
                                .collect()
                        };
                    let deps: Vec<Value> = if input.bundle.unwrap_or(false) {
                        found
                            .dependencies
                            .iter()
                            .filter(|d| {
                                if check_secret_or_scope_refusal(&generation.live, &d.file_path)
                                    .is_some()
                                {
                                    return false;
                                }
                                if let Some(dep_file) = generation.live.get_file(&d.file_path)
                                    && check_bytes_admission(
                                        &generation.live,
                                        &d.file_path,
                                        &dep_file.content,
                                    )
                                    .is_some()
                                {
                                    return false;
                                }
                                true
                            })
                            .map(|d| {
                                json!({
                                    "name": d.name,
                                    "kind": d.kind_label,
                                    "path": d.file_path,
                                    "start_line": d.line_range.0 + 1,
                                    "end_line": d.line_range.1 + 1,
                                    "body": d.body,
                                })
                            })
                            .collect()
                    } else {
                        Vec::new()
                    };
                    Ok(json!({
                        "path": found.file_path,
                        "name": input.name,
                        "kind": found.kind_label,
                        "body": found.body,
                        "total_callers": total_callers,
                        "callers": callers_json,
                        "callees": map_refs(&found.callees),
                        "type_usages": map_refs(&found.type_usages),
                        "dependencies": deps,
                    }))
                }
                ContextBundleView::AmbiguousSymbol {
                    candidate_lines, ..
                } => Err(CodeBackendError::refusal(
                    "ambiguous_symbol",
                    format!(
                        "Ambiguous symbol '{}' in {} at lines {:?}; pass `symbol_line`",
                        input.name, target_path, candidate_lines
                    ),
                )),
                ContextBundleView::FileNotFound { path } => Err(CodeBackendError::refusal(
                    "not_found",
                    format!("File not found: {path}"),
                )),
                ContextBundleView::SymbolNotFound { name, .. } => Err(CodeBackendError::refusal(
                    "not_found",
                    format!("Symbol '{name}' not found in {target_path}"),
                )),
            }
        }

        CodeReadOp::InspectMatch => {
            let input: crate::protocol::tools::InspectMatchInput = decode_op_args(op, kwargs)?;
            let norm_path =
                crate::index_lifecycle::guidance::read_gate::normalize_requested_path(&input.path);
            if let Some(refusal) = check_secret_or_scope_refusal(&generation.live, &norm_path) {
                return Err(CodeBackendError::refusal("policy_withheld", refusal));
            }
            if let Some(file) = generation.live.get_file(&norm_path)
                && let Some(refusal) =
                    check_bytes_admission(&generation.live, &norm_path, &file.content)
            {
                return Err(CodeBackendError::refusal("policy_withheld", refusal));
            }
            match generation.live.capture_inspect_match_view(
                &norm_path,
                input.line,
                input.context,
                input.sibling_limit,
            ) {
                InspectMatchView::Found(found) => {
                    on_commitment_read(&norm_path);
                    Ok(json!({
                        "path": found.path,
                        "line": found.line,
                        "excerpt": found.excerpt,
                        "enclosing": found.enclosing.map(|e| json!({
                            "name": e.name,
                            "kind": e.kind_label,
                            "start_line": e.line_range.0,
                            "end_line": e.line_range.1,
                        })),
                        "parent_chain": found.parent_chain.into_iter().map(|e| json!({
                            "name": e.name,
                            "kind": e.kind_label,
                            "start_line": e.line_range.0,
                            "end_line": e.line_range.1,
                        })).collect::<Vec<_>>(),
                        "siblings": found.siblings.into_iter().map(|s| json!({
                            "name": s.name,
                            "kind": s.kind_label,
                            "start_line": s.line_range.0,
                            "end_line": s.line_range.1,
                        })).collect::<Vec<_>>(),
                    }))
                }
                InspectMatchView::FileNotFound { path } => Err(CodeBackendError::refusal(
                    "not_found",
                    format!("File not found: {path}"),
                )),
                InspectMatchView::LineOutOfBounds {
                    path,
                    line,
                    total_lines,
                } => Err(CodeBackendError::invalid_args(format!(
                    "Line {line} out of bounds for {path} ({total_lines} lines)"
                ))),
            }
        }

        CodeReadOp::WhatChanged => {
            let input: crate::protocol::tools::WhatChangedInput = decode_op_args(op, kwargs)?;
            let options = crate::index_lifecycle::guidance::changes::WhatChangedOptions {
                since: input.since,
                git_ref: input.git_ref.as_deref(),
                uncommitted: input.uncommitted,
                path_prefix: input.path_prefix.as_deref(),
                language: input.language.as_deref(),
                code_only: input.code_only,
                include_symbol_diff: input.include_symbol_diff,
            };
            let mode = crate::index_lifecycle::guidance::changes::determine_what_changed_mode(
                &options,
                repo_root.is_some(),
            )
            .map_err(|msg| CodeBackendError::refusal("unavailable", msg))?;

            match mode {
                crate::index_lifecycle::guidance::changes::WhatChangedMode::Timestamp(since_ts) => {
                    let view = generation.live.capture_what_changed_timestamp_view();
                    let raw_paths = if since_ts <= view.loaded_secs {
                        view.paths
                    } else {
                        Vec::new()
                    };
                    let total_paths = raw_paths.len();
                    let filtered =
                        crate::index_lifecycle::guidance::changes::filter_paths_by_prefix_and_language(
                            raw_paths,
                            input.path_prefix.as_deref(),
                            input.language.as_deref(),
                            input.code_only.unwrap_or(false),
                        )
                        .map_err(CodeBackendError::invalid_args)?;
                    Ok(json!({
                        "paths": filtered,
                        "total_paths": total_paths,
                        "symbol_diff": Value::Null,
                    }))
                }
                crate::index_lifecycle::guidance::changes::WhatChangedMode::Uncommitted => {
                    let root = repo_root.ok_or_else(|| {
                        CodeBackendError::refusal("git_unavailable", "no repository root attached")
                    })?;
                    let repo = crate::git::GitRepo::open(root)
                        .map_err(|e| CodeBackendError::refusal("git_unavailable", e.to_string()))?;
                    let raw = repo
                        .uncommitted_paths()
                        .map_err(|e| CodeBackendError::refusal("git_unavailable", e.to_string()))?;
                    let total_paths = raw.len();
                    let filtered =
                        crate::index_lifecycle::guidance::changes::filter_paths_by_prefix_and_language(
                            raw,
                            input.path_prefix.as_deref(),
                            input.language.as_deref(),
                            input.code_only.unwrap_or(true),
                        )
                        .map_err(CodeBackendError::invalid_args)?;
                    let sym_diff = if input.include_symbol_diff.unwrap_or(false) {
                        let head = repo.resolve_ref_tree_reference("HEAD").map_err(|e| {
                            CodeBackendError::invalid_args(format!("Invalid git ref HEAD: {e}"))
                        })?;
                        let base_tree = head.tree_oid.to_string();
                        let refs: Vec<&str> = filtered.iter().map(String::as_str).collect();
                        let diff_view =
                            crate::index_lifecycle::guidance::changes::capture_symbol_diff(
                                "HEAD",
                                "",
                                &refs,
                                |reference, path| {
                                    if reference.is_empty() {
                                        crate::protocol::read_gate::admit_worktree_text(
                                            &generation.live,
                                            &repo,
                                            path,
                                        )
                                    } else {
                                        crate::protocol::read_gate::admit_git_text(
                                            &generation.live,
                                            &repo,
                                            &base_tree,
                                            path,
                                        )
                                    }
                                },
                            );
                        Some(symbol_diff_view_to_json(&diff_view))
                    } else {
                        None
                    };
                    Ok(json!({
                        "paths": filtered,
                        "total_paths": total_paths,
                        "symbol_diff": sym_diff,
                    }))
                }
                crate::index_lifecycle::guidance::changes::WhatChangedMode::GitRef(git_ref) => {
                    let root = repo_root.ok_or_else(|| {
                        CodeBackendError::refusal("git_unavailable", "no repository root attached")
                    })?;
                    let repo = crate::git::GitRepo::open(root)
                        .map_err(|e| CodeBackendError::refusal("git_unavailable", e.to_string()))?;
                    let raw = repo.changed_paths_from_ref(&git_ref).map_err(|e| {
                        CodeBackendError::invalid_args(format!("Invalid git ref '{git_ref}': {e}"))
                    })?;
                    let total_paths = raw.len();
                    let filtered =
                        crate::index_lifecycle::guidance::changes::filter_paths_by_prefix_and_language(
                            raw,
                            input.path_prefix.as_deref(),
                            input.language.as_deref(),
                            input.code_only.unwrap_or(false),
                        )
                        .map_err(CodeBackendError::invalid_args)?;
                    let sym_diff = if input.include_symbol_diff.unwrap_or(false) {
                        let ref_name = git_ref.strip_prefix("branch:").unwrap_or(&git_ref);
                        let resolved = repo.resolve_ref_tree_reference(ref_name).map_err(|e| {
                            CodeBackendError::invalid_args(format!("Invalid git ref: {e}"))
                        })?;
                        let base_tree = resolved.tree_oid.to_string();
                        let refs: Vec<&str> = filtered.iter().map(String::as_str).collect();
                        let diff_view =
                            crate::index_lifecycle::guidance::changes::capture_symbol_diff(
                                &git_ref,
                                "",
                                &refs,
                                |reference, path| {
                                    if reference.is_empty() {
                                        crate::protocol::read_gate::admit_worktree_text(
                                            &generation.live,
                                            &repo,
                                            path,
                                        )
                                    } else {
                                        crate::protocol::read_gate::admit_git_text(
                                            &generation.live,
                                            &repo,
                                            &base_tree,
                                            path,
                                        )
                                    }
                                },
                            );
                        Some(symbol_diff_view_to_json(&diff_view))
                    } else {
                        None
                    };
                    Ok(json!({
                        "paths": filtered,
                        "total_paths": total_paths,
                        "symbol_diff": sym_diff,
                    }))
                }
            }
        }

        CodeReadOp::DiffSymbols => {
            let input: crate::protocol::tools::DiffSymbolsInput = decode_op_args(op, kwargs)?;
            let root = repo_root.ok_or_else(|| {
                CodeBackendError::refusal("git_unavailable", "no repository root attached")
            })?;
            let repo = crate::git::GitRepo::open(root)
                .map_err(|e| CodeBackendError::refusal("git_unavailable", e.to_string()))?;
            let base = input
                .base
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("HEAD");
            let target_opt = input
                .target
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let view = if let Some(target) = target_opt {
                let range = repo
                    .comparison_range_between_refs(base, target)
                    .map_err(|e| CodeBackendError::invalid_args(format!("git diff failed: {e}")))?;
                let changed = repo
                    .changed_paths_for_comparison(&range)
                    .map_err(|e| CodeBackendError::invalid_args(format!("git diff failed: {e}")))?;
                let filtered = crate::index_lifecycle::guidance::changes::filter_diff_symbol_paths(
                    &changed,
                    input.path_prefix.as_deref(),
                    input.language.as_deref(),
                    input.code_only.unwrap_or(false),
                )
                .map_err(CodeBackendError::invalid_args)?;
                let base_commit = range.base_commit.to_string();
                let target_commit = range.target_commit.to_string();
                crate::index_lifecycle::guidance::changes::capture_symbol_diff(
                    base,
                    target,
                    &filtered,
                    |reference, path| {
                        let oid = if reference == base {
                            &base_commit
                        } else {
                            &target_commit
                        };
                        crate::protocol::read_gate::admit_git_text(
                            &generation.live,
                            &repo,
                            oid,
                            path,
                        )
                    },
                )
            } else {
                let ref_name = base.strip_prefix("branch:").unwrap_or(base);
                let resolved = repo
                    .resolve_ref_tree_reference(ref_name)
                    .map_err(|e| CodeBackendError::invalid_args(format!("git diff failed: {e}")))?;
                let mut changed = repo
                    .changed_paths_from_tree_oid(resolved.tree_oid)
                    .map_err(|e| CodeBackendError::invalid_args(format!("git diff failed: {e}")))?;
                if let Ok(untracked) = repo.untracked_paths() {
                    changed.extend(untracked);
                    changed.sort();
                    changed.dedup();
                }
                let filtered = crate::index_lifecycle::guidance::changes::filter_diff_symbol_paths(
                    &changed,
                    input.path_prefix.as_deref(),
                    input.language.as_deref(),
                    input.code_only.unwrap_or(false),
                )
                .map_err(CodeBackendError::invalid_args)?;
                let base_tree = resolved.tree_oid.to_string();
                crate::index_lifecycle::guidance::changes::capture_symbol_diff(
                    base,
                    "",
                    &filtered,
                    |reference, path| {
                        if reference.is_empty() {
                            crate::protocol::read_gate::admit_worktree_text(
                                &generation.live,
                                &repo,
                                path,
                            )
                        } else {
                            crate::protocol::read_gate::admit_git_text(
                                &generation.live,
                                &repo,
                                &base_tree,
                                path,
                            )
                        }
                    },
                )
            };
            Ok(symbol_diff_view_to_json(&view))
        }

        CodeReadOp::DetectImpact => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct CodeDetectImpactArgs {
                #[serde(default)]
                base_branch: Option<String>,
                #[serde(default)]
                since: Option<String>,
                #[serde(default)]
                depth: Option<u8>,
                #[serde(default)]
                scope: Option<crate::protocol::tools::ImpactScope>,
                #[serde(default)]
                include_untracked: Option<bool>,
                #[serde(default)]
                include_data: Option<bool>,
                #[serde(default)]
                paths: Option<Vec<String>>,
            }
            let input: CodeDetectImpactArgs = decode_op_args(op, kwargs)?;
            let root = repo_root.ok_or_else(|| {
                CodeBackendError::refusal("git_unavailable", "no repository root attached")
            })?;
            let repo = crate::git::GitRepo::open(root)
                .map_err(|e| CodeBackendError::refusal("git_unavailable", e.to_string()))?;
            let req = crate::index_lifecycle::guidance::impact::ImpactRequest {
                base_branch: input.base_branch.as_deref(),
                since: input.since.as_deref(),
                depth: input.depth.unwrap_or(2),
                files_scope: matches!(
                    input.scope,
                    Some(crate::protocol::tools::ImpactScope::Files)
                ),
                include_untracked: input.include_untracked.unwrap_or(true),
                include_data: input.include_data.unwrap_or(false),
                paths: input.paths.as_deref(),
            };
            let report = crate::index_lifecycle::guidance::impact::compute_detect_impact(
                &generation.live,
                &repo,
                &req,
                &mut |git_ref, path| {
                    crate::protocol::read_gate::admit_git_text(
                        &generation.live,
                        &repo,
                        git_ref,
                        path,
                    )
                },
                &mut |path| {
                    crate::protocol::read_gate::admit_worktree_text(&generation.live, &repo, path)
                },
            )
            .map_err(CodeBackendError::invalid_args)?;

            let cap = crate::index_lifecycle::guidance::impact::DETECT_IMPACT_MAX_RETURNED;
            let changed_symbols: Vec<Value> = report
                .changed_symbols
                .iter()
                .take(cap)
                .map(|s| {
                    json!({
                        "name": s.name,
                        "path": s.path,
                        "kind": s.kind.to_string(),
                    })
                })
                .collect();
            let blast_radius: Vec<Value> = report
                .blast_entries
                .iter()
                .take(cap)
                .map(|b| {
                    json!({
                        "symbol": b.symbol,
                        "hop": b.hop,
                        "risk": b.risk.as_str(),
                    })
                })
                .collect();
            let changed_files: Vec<String> = report.changed_files.into_iter().take(cap).collect();
            Ok(json!({
                "changed_files": changed_files,
                "changed_symbols": changed_symbols,
                "blast_radius": blast_radius,
            }))
        }

        CodeReadOp::SearchKnowledge => {
            let input: crate::protocol::tools::SearchKnowledgeInput = decode_op_args(op, kwargs)?;
            let projection = crate::knowledge::search::search_scoped_projection(source_set, &input)
                .map_err(CodeBackendError::invalid_args)?;
            let rendered =
                crate::knowledge::search::budget_summary(&projection.rendered, input.max_tokens);
            let (hit_count, overflow, withheld_count) = match &projection.retrieval {
                Some(r) => (
                    r.hits.len() as u64,
                    r.overflow as u64,
                    r.withheld_count as u64,
                ),
                None => (0, 0, 0),
            };
            let rendered_hit_count = if hit_count == 0 {
                0
            } else {
                rendered
                    .lines()
                    .filter(|line| {
                        line.split_once(". ")
                            .is_some_and(|(ordinal, _)| ordinal.parse::<usize>().is_ok())
                    })
                    .count() as u64
            };
            Ok(json!({
                "rendered": rendered,
                "hit_count": hit_count,
                "rendered_hit_count": rendered_hit_count,
                "overflow": overflow,
                "withheld_count": withheld_count,
            }))
        }
    }
}

fn protocol_tool_name(op: CodeReadOp) -> &'static str {
    match op {
        CodeReadOp::FileContext => "get_file_context",
        CodeReadOp::FileContent => "get_file_content",
        CodeReadOp::RepoMap => "get_repo_map",
        CodeReadOp::SymbolContext => "get_symbol_context",
        other => other.as_str(),
    }
}

fn validate_op_contract(
    op: CodeReadOp,
    kwargs: &mut Map<String, Value>,
) -> Result<(), CodeBackendError> {
    let tool = protocol_tool_name(op);
    if let Err(refusal) =
        crate::index_lifecycle::guidance::argument_values::check_values(tool, kwargs)
    {
        return Err(CodeBackendError::invalid_args(refusal.text));
    }
    if matches!(op, CodeReadOp::FindReferences | CodeReadOp::SearchText)
        && let Some(refusal) = crate::index_lifecycle::guidance::argument_values::range_refusal(
            tool,
            "context",
            0,
            u64::from(MAX_HIT_CONTEXT_LINES),
            kwargs,
        )
    {
        return Err(CodeBackendError::invalid_args(refusal));
    }
    if let Err(refusal) = crate::protocol::tool_contract::apply_conditions(tool, kwargs) {
        return Err(CodeBackendError::invalid_args(refusal));
    }
    Ok(())
}

const MAX_HIT_CONTEXT_LINES: u32 = 20;

fn take_hit_context(kwargs: &mut Map<String, Value>) -> Result<usize, CodeBackendError> {
    kwargs
        .remove("context")
        .map_or(
            Ok(None),
            crate::index_lifecycle::guidance::serde_input::lenient_u32,
        )
        .map(|n| n.unwrap_or(0) as usize)
        .map_err(|e| CodeBackendError::invalid_args(e.to_string()))
}

fn numbered_window<'a>(lines: impl Iterator<Item = (usize, &'a str)>) -> String {
    lines
        .map(|(number, text)| format!("{number}: {text}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn decode_op_args<T: serde::de::DeserializeOwned>(
    op: CodeReadOp,
    kwargs: Map<String, Value>,
) -> Result<T, CodeBackendError> {
    use crate::index_lifecycle::guidance::request_refusal::{
        StrictError, decode_strict, unknown_arguments_refusal,
    };

    match decode_strict::<T>(Value::Object(kwargs)) {
        Ok(decoded) => Ok(decoded),
        Err(StrictError::Unknown(unknown)) => {
            let valid: Vec<String> = op
                .descriptor()
                .params
                .iter()
                .map(|p| p.name.to_owned())
                .collect();
            Err(CodeBackendError::invalid_args(format!(
                "{} Signature: {}",
                unknown_arguments_refusal(op.as_str(), &unknown, &valid),
                op.signature_one_liner()
            )))
        }
        Err(StrictError::Decode(err)) => Err(CodeBackendError::invalid_args(err.to_string())),
    }
}

fn symbol_diff_view_to_json(
    view: &crate::index_lifecycle::guidance::changes::SymbolDiffView,
) -> Value {
    let files: Vec<Value> = view
        .files
        .iter()
        .map(|f| {
            json!({
                "path": f.path,
                "added": f.added,
                "removed": f.removed,
                "modified": f.modified,
                "withheld": f.withheld,
            })
        })
        .collect();
    json!({
        "base": view.base,
        "target": view.target,
        "added_count": view.added_count,
        "removed_count": view.removed_count,
        "modified_count": view.modified_count,
        "withheld_count": view.withheld_count,
        "files": files,
    })
}

// A target path disambiguates the relationship, not the caller's directory.
// Exact targets restrict caller languages; name-based lookups do not.
fn withheld_reference_corpus(
    live: &LiveIndex,
    catalog: &crate::live_index::search::CodeModeWithheld,
    target_language: Option<crate::domain::LanguageId>,
    exclude_path: Option<&str>,
) -> std::collections::HashSet<String> {
    catalog
        .manifest
        .iter()
        .filter_map(|&i| {
            let entry = live.manifest_entries.get(i)?;
            let path = entry.path.normalized_utf8.as_deref()?;
            let language = entry.language.as_ref()?;
            (exclude_path != Some(path)
                && language.is_code_language()
                && target_language.as_ref().is_none_or(|target| {
                    crate::live_index::query::import_languages_compatible(language, target)
                }))
            .then(|| path.to_owned())
        })
        .collect()
}

fn path_refused_without_catalog(live: &LiveIndex, path: &str) -> bool {
    live.unverified_since_restore(path).is_some()
        || crate::knowledge::sensitive_path_rule(path).is_some()
        || crate::discovery::path_is_hard_scope_excluded(Path::new(path))
}

// Bulk equivalent of the read gate's path, manifest and restore checks.
// Index the sensitive dispositions once: the single-file lookup scans the manifest.
fn build_code_mode_withheld(live: &LiveIndex) -> crate::live_index::search::CodeModeWithheld {
    let sensitive = live.capture_sensitive_admission_paths();
    let rule = |path: &str| sensitive.contains(path) || path_refused_without_catalog(live, path);
    let mut catalog = crate::live_index::search::CodeModeWithheld::default();
    let mut refused_targets = std::collections::HashMap::new();
    for (i, entry) in live.manifest_entries.iter().enumerate() {
        let Some(path) = entry.path.normalized_utf8.as_deref() else {
            continue;
        };
        let path = crate::live_index::query::normalize_path_query(path);
        let refused = rule(&path);
        if refused || crate::live_index::search::catalog_content_is_withheld(&entry.disposition) {
            catalog.manifest.push(i);
        }
        if refused {
            if let crate::domain::FileDisposition::Indexed { targets, .. } = &entry.disposition {
                refused_targets.insert(path.clone(), *targets);
            }
            catalog.refused.insert(path.clone());
        }
        catalog.known.insert(path);
    }
    for (key, _) in live.all_files() {
        let path = crate::live_index::query::normalize_path_query(key);
        let refused = if catalog.known.contains(&path) {
            catalog.refused.contains(&path)
        } else {
            let refused = rule(&path);
            if refused {
                catalog.refused.insert(path.clone());
            }
            catalog.known.insert(path.clone());
            refused
        };
        if refused {
            catalog
                .refused_files
                .push((key.clone(), refused_targets.get(&path).copied()));
        }
    }
    catalog
}

fn code_mode_withheld(
    generation: &PublishedGeneration,
) -> &crate::live_index::search::CodeModeWithheld {
    generation
        .code_mode_withheld
        .get_or_init(|| build_code_mode_withheld(&generation.live))
}

fn search_refusal_predicate(generation: &PublishedGeneration) -> impl Fn(&str) -> bool + '_ {
    let catalog = code_mode_withheld(generation);
    let live = &*generation.live;
    move |path| {
        catalog.known_refusal(path).unwrap_or_else(|| {
            path_refused_without_catalog(
                live,
                &crate::live_index::query::normalize_path_query(path),
            )
        })
    }
}

fn check_secret_or_scope_refusal(live: &LiveIndex, norm_path: &str) -> Option<String> {
    if let Some(scope_err) =
        crate::index_lifecycle::guidance::read_gate::hard_scope_refusal(norm_path)
    {
        return Some(scope_err);
    }
    let mut noop = |_meta| {};
    crate::index_lifecycle::guidance::read_gate::refuse_by_policy_with(live, norm_path, &mut noop)
}

fn check_bytes_admission(live: &LiveIndex, norm_path: &str, bytes: &[u8]) -> Option<String> {
    let mut noop = |_meta| {};
    crate::index_lifecycle::guidance::read_gate::classify_admitted_bytes_with(
        live, norm_path, bytes, &mut noop,
    )
}

/// Synchronous HTTP client wrapper routing `CodeReadOp` calls over a warm local
/// SymForge daemon (`POST /v1/sessions/{session_id}/code-query`).
pub struct DaemonCodeBackend {
    handle: tokio::runtime::Handle,
    session: crate::daemon::DaemonSessionClient,
    project_override: Option<String>,
    owns_session: bool,
    closed: Mutex<bool>,
    last_evidence: Mutex<Option<crate::protocol::result_status::ProjectEvidence>>,
}

impl DaemonCodeBackend {
    /// Creates a borrowed backend over an existing shared `DaemonSessionClient`.
    /// Dropping this backend NEVER closes the underlying daemon session.
    #[must_use]
    pub fn new(
        handle: tokio::runtime::Handle,
        session: crate::daemon::DaemonSessionClient,
        project_override: Option<String>,
    ) -> Self {
        Self::new_borrowed(handle, session, project_override)
    }

    /// Creates a borrowed backend over an existing shared `DaemonSessionClient`
    /// (used by MCP `SymForgeServer::code_execute_tool`).
    #[must_use]
    pub fn new_borrowed(
        handle: tokio::runtime::Handle,
        session: crate::daemon::DaemonSessionClient,
        project_override: Option<String>,
    ) -> Self {
        Self {
            handle,
            session,
            project_override,
            owns_session: false,
            closed: Mutex::new(false),
            last_evidence: Mutex::new(None),
        }
    }

    /// Creates an ephemeral backend that owns its `DaemonSessionClient` (used by
    /// one-shot CLI invocations) and closes the session on drop or `close_best_effort`.
    #[must_use]
    pub fn new_ephemeral(
        handle: tokio::runtime::Handle,
        session: crate::daemon::DaemonSessionClient,
        project_override: Option<String>,
    ) -> Self {
        Self {
            handle,
            session,
            project_override,
            owns_session: true,
            closed: Mutex::new(false),
            last_evidence: Mutex::new(None),
        }
    }

    /// Returns the most recent `ProjectEvidence` receipt observed from `/code-query`.
    pub fn take_last_evidence(&self) -> Option<crate::protocol::result_status::ProjectEvidence> {
        self.last_evidence.lock().take()
    }

    pub fn close_best_effort(&self) {
        if !self.owns_session {
            return;
        }
        let mut guard = self.closed.lock();
        if *guard {
            return;
        }
        *guard = true;
        let session = self.session.clone();
        let _ = self.handle.block_on(async move {
            tokio::time::timeout(std::time::Duration::from_secs(2), session.close()).await
        });
    }
}

impl Drop for DaemonCodeBackend {
    fn drop(&mut self) {
        if self.owns_session {
            self.close_best_effort();
        }
    }
}
impl CodeReadBackend for DaemonCodeBackend {
    fn execute_op(
        &self,
        op: CodeReadOp,
        kwargs: Map<String, Value>,
    ) -> Result<Value, CodeBackendError> {
        self.execute_op_with_completeness(op, kwargs)
            .map(|result| result.value)
    }

    fn execute_op_with_completeness(
        &self,
        op: CodeReadOp,
        kwargs: Map<String, Value>,
    ) -> Result<CodeReadResult, CodeBackendError> {
        let req = DaemonCodeQueryRequest {
            op,
            kwargs,
            project: self.project_override.clone(),
        };
        let session = self.session.clone();
        let (result, evidence) = self.handle.block_on(async move {
            session
                .call_code_query_with_completeness(&req)
                .await
                .map_err(|e| CodeBackendError::refusal("daemon_transport", e.to_string()))
        })?;
        if let Some(ev) = evidence {
            *self.last_evidence.lock() = Some(ev);
        }
        result
    }
}

#[cfg(test)]
mod refusal_predicate_tests {
    use super::*;
    use crate::domain::{FileDisposition, MetadataOnlyReason};
    use crate::live_index::store::{SnapshotVerifyReport, SnapshotVerifyState};

    #[test]
    fn bulk_search_refusal_agrees_with_the_read_gate_for_every_refusal_reason() {
        let repo = tempfile::tempdir().unwrap();
        let key_marker = format!("// -----BEGIN {} KEY-----\n", "PRIVATE");
        let files = [
            ("src/lib.rs", "pub fn ok() {}\n"),
            ("src/held.rs", "pub fn held() {}\n"),
            ("src/indeterminate.rs", "pub fn unsure() {}\n"),
            ("src/key.rs", key_marker.as_str()),
            (".env", "NAME=placeholder\n"),
            ("config/id_rsa", "placeholder\n"),
            ("docs/plain.md", "# plain\n"),
        ];
        for (path, body) in files {
            let path = repo.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body).unwrap();
        }
        let shared = LiveIndex::load(repo.path()).unwrap();
        let mut live = shared.write();
        live.snapshot_verify_state = SnapshotVerifyState::Completed(
            SnapshotVerifyReport::from_mismatched_paths(vec!["src/held.rs".to_string()])
                .with_unverified(std::collections::BTreeMap::from([(
                    "src/held.rs".to_string(),
                    "it could not be read".to_string(),
                )])),
        );
        for entry in live.manifest_entries.iter_mut() {
            if entry.path.normalized_utf8.as_deref() == Some("src/indeterminate.rs") {
                entry.disposition = FileDisposition::MetadataOnly {
                    reason: MetadataOnlyReason::SensitiveContent {
                        rule_ids: vec![crate::knowledge::INDETERMINATE_RULE_ID.to_string()],
                        finding_count: 0,
                    },
                };
            }
        }
        drop(live);
        let generation = shared.published_generation();
        let live: &LiveIndex = &generation.live;
        let mut probes: std::collections::BTreeSet<String> = live
            .manifest_entries
            .iter()
            .filter_map(|entry| entry.path.normalized_utf8.clone())
            .chain(live.all_files().map(|(path, _)| path.clone()))
            .collect();
        probes.extend(
            [
                ".git/config",
                ".symforge/state.json",
                "src/.env",
                "src/missing.rs",
                "./src/key.rs",
                "src\\key.rs",
            ]
            .map(str::to_string),
        );
        let refuse = search_refusal_predicate(&generation);
        for probe in &probes {
            let gate_path =
                crate::index_lifecycle::guidance::read_gate::normalize_requested_path(probe);
            assert_eq!(
                refuse(probe),
                check_secret_or_scope_refusal(live, &gate_path).is_some(),
                "bulk predicate and read gate disagree on {probe}"
            );
        }
        for refused in [
            "src/held.rs",
            "src/indeterminate.rs",
            "src/key.rs",
            ".env",
            "config/id_rsa",
            ".git/config",
        ] {
            assert!(refuse(refused), "{refused} must be refused");
        }
        assert!(!refuse("src/lib.rs"));
    }

    #[test]
    fn withheld_paths_are_recomputed_for_each_published_generation() {
        let repo = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(repo.path().join("src")).unwrap();
        std::fs::write(repo.path().join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
        std::fs::write(repo.path().join("src/later.rs"), "pub fn later() {}\n").unwrap();
        let shared = LiveIndex::load(repo.path()).unwrap();
        let first = shared.published_generation();
        assert!(!search_refusal_predicate(&first)("src/later.rs"));
        assert!(code_mode_withheld(&first).manifest.is_empty());
        {
            let mut live = shared.write();
            live.snapshot_verify_state = SnapshotVerifyState::Completed(
                SnapshotVerifyReport::from_mismatched_paths(vec!["src/later.rs".to_string()])
                    .with_unverified(std::collections::BTreeMap::from([(
                        "src/later.rs".to_string(),
                        "it could not be read".to_string(),
                    )])),
            );
        }
        let second = shared.published_generation();
        assert!(!Arc::ptr_eq(&first, &second));
        assert!(search_refusal_predicate(&second)("src/later.rs"));
        assert!(!search_refusal_predicate(&first)("src/later.rs"));
        {
            let mut live = shared.write();
            for entry in live.manifest_entries.iter_mut() {
                if entry.path.normalized_utf8.as_deref() == Some("src/lib.rs") {
                    entry.disposition = FileDisposition::MetadataOnly {
                        reason: MetadataOnlyReason::SensitiveContent {
                            rule_ids: vec!["fixture.rule".to_string()],
                            finding_count: 1,
                        },
                    };
                }
            }
        }
        let third = shared.published_generation();
        assert!(search_refusal_predicate(&third)("src/lib.rs"));
        assert!(!search_refusal_predicate(&second)("src/lib.rs"));
        assert!(code_mode_withheld(&third).manifest.iter().any(|&i| {
            third.live.manifest_entries[i]
                .path
                .normalized_utf8
                .as_deref()
                == Some("src/lib.rs")
        }));
    }
}
