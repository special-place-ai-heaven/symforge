//! The STEL runtime the MCP server and embedded hosts share: the durable
//! calibration store's reads and writes, the `status` body, and the facade's
//! ledger finalization. Moved verbatim from `protocol` so both transports run
//! one engine. Every caller supplies its own ledger, store and index
//! observation; nothing here reads process state.

use std::path::{Path, PathBuf};

use crate::index_lifecycle::guidance::reference_read::symbol_candidate_paths;
use crate::live_index::{SearchFilesTier, SearchFilesView};

use super::calibration::CalibrationVerdict;
use super::ledger::SessionLedger;
use super::ledger_store::{StelLedgerStore, TunedEstimateConstants};
use super::status::{DurableLedgerState, StelStatusContext, format_stel_status};
use super::types::{StelDecision, StelLedgerEvent, StelPlan, StelStatusRequest};

/// The validated tuned-constant set currently IN FORCE for the current
/// estimator version (feature 013, T032 / FR-006), or `None`.
///
/// Loads the active tuning from the durable store and applies the R3 in-force
/// rule via [`active_tuning_in_force`]: a tuning whose `estimator_version`
/// does not match the current estimator is NOT returned, so a stale-version
/// set never silently applies. A `Disabled`/absent store yields `None` and
/// the economics fall back to the static floors — never serves a bad tuning
/// (FR-003). Read-only; no frecency bump (Principle V).
///
/// [`active_tuning_in_force`]: crate::stel::controller::active_tuning_in_force
pub fn active_tuning_for_economics(
    store: Option<&StelLedgerStore>,
) -> Option<TunedEstimateConstants> {
    use super::controller::active_tuning_in_force;
    use super::ledger_store::CURRENT_ESTIMATOR_VERSION;

    let store = store?;
    let loaded = store.load_active_tuning(CURRENT_ESTIMATOR_VERSION).ok()?;
    active_tuning_in_force(loaded, CURRENT_ESTIMATOR_VERSION)
}

/// Compute the honest [`CalibrationVerdict`] from the DURABLE calibration
/// state for the `status` surface (feature 013, T033 / FR-009).
///
/// `Tuned` is returned ONLY when an active tuning is in force AND it carries a
/// before/after held-out error artifact (`error_before > error_after`),
/// reading the artifact straight off the persisted set so the surface never
/// claims `tuned` without it. Otherwise the verdict reflects the durable
/// sample count (`Deferred` / `Accumulating(n/min)`). When no durable store is
/// wired, returns `None` so the caller keeps the in-memory view. When a wired
/// store cannot answer the sample query, pins the verdict to `Deferred` so a
/// stale in-memory session cannot claim `Tuned` while no validated tuning is
/// readable. Read-only; no frecency bump.
pub fn durable_calibration_verdict(store: Option<&StelLedgerStore>) -> Option<CalibrationVerdict> {
    use super::calibration::TUNING_MIN_CORPUS;
    use super::ledger_store::{CURRENT_ESTIMATOR_VERSION, LEDGER_RETENTION_MAX};

    let store = store?;
    // A wired but failing store cannot prove any tuning is in force.
    let records = match store.samples_for_estimator(CURRENT_ESTIMATOR_VERSION, LEDGER_RETENTION_MAX)
    {
        Ok(records) => records,
        Err(_) => return Some(CalibrationVerdict::Deferred),
    };
    let n = records.len();

    // An active, in-force tuning with a real reduction artifact reads `Tuned`.
    if let Some(active) = active_tuning_for_economics(Some(store))
        && active.error_before > active.error_after
    {
        return Some(CalibrationVerdict::Tuned {
            sample_size: active.sample_size as usize,
            error_before: active.error_before,
            error_after: active.error_after,
        });
    }

    if n == 0 {
        Some(CalibrationVerdict::Deferred)
    } else {
        Some(CalibrationVerdict::Accumulating {
            n,
            min: TUNING_MIN_CORPUS,
        })
    }
}

/// Clear accumulated calibration for the current estimator (feature 013,
/// T037 / FR-011 operator reset). Returns the number of sample rows cleared,
/// or `None` when no durable store is wired. Never rebuilds the index.
pub fn reset_calibration(store: Option<&StelLedgerStore>) -> Option<usize> {
    use super::ledger_store::CURRENT_ESTIMATOR_VERSION;
    store?
        .clear_calibration_for_estimator(CURRENT_ESTIMATOR_VERSION)
        .ok()
}

/// The honest `calibration_reset:` receipt for a [`reset_calibration`]
/// result: a real cleared-sample count with a store, or the no-store no-op.
pub fn calibration_reset_note(cleared: Option<usize>) -> String {
    match cleared {
        Some(cleared) => format!(
            "calibration_reset: cleared {cleared} sample(s) + active tuning (state -> deferred)"
        ),
        None => "calibration_reset: no durable store; in-memory calibration is already deferred"
            .to_string(),
    }
}

/// Durable-ledger subsystem state for the `status` tool (US3/T029
/// restart-survival; N-3 / TR-17 / FR-008).
///
/// Maps the wired durable store's [`subsystem_state`] onto the
/// feature-independent surface enum. Reports `Unavailable` when no store is
/// attached, `Disabled { reason }` for a wired-but-failing store (open failed
/// at startup or live query failed — N-3, never swallowed), and `Durable`
/// otherwise.
///
/// [`subsystem_state`]: crate::stel::ledger_store::StelLedgerStore::subsystem_state
pub fn durable_ledger_state(store: Option<&StelLedgerStore>) -> DurableLedgerState {
    use super::ledger_store::LedgerSubsystemState;
    use super::status::DurableLedgerSummary;

    let Some(store) = store else {
        return DurableLedgerState::Unavailable;
    };
    match store.subsystem_state() {
        LedgerSubsystemState::Durable { summary } => {
            DurableLedgerState::Durable(DurableLedgerSummary {
                total_events: summary.total_events,
                total_net_vs_manual: summary.total_net_vs_manual,
                session_count: summary.session_count,
            })
        }
        LedgerSubsystemState::Disabled { reason } => DurableLedgerState::Disabled { reason },
    }
}

/// T031 auto-tune pass: after a new sample lands, derive and persist an
/// accepted correction. Degrades silently on any store error.
pub fn maybe_persist_tuning(store: &StelLedgerStore) {
    use super::calibration::{NO_CORRECTION_FACTOR, PredictionSample, compute_calibration_verdict};
    use super::controller::active_tuning_in_force;
    use super::ledger_store::{CURRENT_ESTIMATOR_VERSION, LEDGER_RETENTION_MAX};

    // Newest-first current-version samples (excludes pre-013). Bounded by the
    // retention cap so the pass is O(cap) at worst.
    let Ok(records) = store.samples_for_estimator(CURRENT_ESTIMATOR_VERSION, LEDGER_RETENTION_MAX)
    else {
        return;
    };
    let samples: Vec<PredictionSample> = records.iter().map(PredictionSample::from).collect();

    // In-force correction factor = the active tuning's if present (D13
    // hysteresis anchor: a re-tune must beat the correction already LIVE),
    // else the identity 1.0 (no tuning). The validate gate scores a candidate
    // against this, so a re-tune must out-perform what is already applied.
    let active = store
        .load_active_tuning(CURRENT_ESTIMATOR_VERSION)
        .ok()
        .flatten();
    let in_force = active_tuning_in_force(active.clone(), CURRENT_ESTIMATOR_VERSION);
    let in_force_factor = in_force
        .as_ref()
        .map_or(NO_CORRECTION_FACTOR, |c| c.response_correction_factor);

    let (verdict, candidate) = compute_calibration_verdict(&samples, in_force_factor);
    if !matches!(verdict, CalibrationVerdict::Tuned { .. }) {
        return;
    }
    let Some(mut candidate) = candidate else {
        return;
    };

    // Idempotence / oscillation guard: if the accepted candidate's correction
    // equals what is already stored, do not re-write (the validate gate already
    // requires a >= margin beat over the in-force factor, so this only fires on
    // an exact-equal stored factor — pure churn avoidance).
    if let Some(existing) = active.as_ref()
        && existing.response_correction_factor == candidate.response_correction_factor
    {
        return;
    }

    candidate.tuned_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    // Audited gated action (FR-008): store_active_tuning persists the new
    // correction factor, sample_size, error_before/after (the held-out real
    // residual baseline + corrected), tuned_at. Degrades silently on a store
    // error (never fails a request; never a bad tuning).
    if let Err(error) = store.store_active_tuning(&candidate) {
        tracing::warn!(error = %error, "stel tuning persist failed; keeping prior constants");
    } else {
        tracing::info!(
            response_correction_factor = candidate.response_correction_factor,
            sample_size = candidate.sample_size,
            error_before = candidate.error_before,
            error_after = candidate.error_after,
            "stel auto-tune accepted: persisted calibrated correction (013 US2)"
        );
    }
}

/// Synchronous durable write-through for a host with no async worker to
/// protect: record the event, then run the tuning pass.
pub fn record_durably_inline(store: &StelLedgerStore, event: &StelLedgerEvent) {
    store.record(event);
    maybe_persist_tuning(store);
}

/// What the answering process observed for one `status` render.
pub struct StatusObservation<'a> {
    /// Canonical label of the surface served on this connection.
    pub surface: &'static str,
    /// The daemon process's own env surface, only when it diverges.
    pub daemon_env_surface: Option<&'static str>,
    /// Pid of an answering daemon that is no longer the recorded one.
    pub orphaned_daemon_pid: Option<u32>,
    pub project_name: &'a str,
    pub project_root: Option<String>,
    pub index_ready: bool,
    pub index_files: usize,
    pub index_symbols: usize,
    pub ledger: &'a SessionLedger,
    pub session_tokens: u64,
    pub store: Option<&'a StelLedgerStore>,
    /// Lines that follow the formatted status: snapshot verification and the
    /// secret-dismissal count.
    pub trailing_lines: Vec<String>,
}

/// Render the `status` body: an operator calibration reset first, when asked,
/// then the STEL readout over the durable verdict, then the trailing lines and
/// the reset receipt.
pub fn render_status_body(request: &StelStatusRequest, observed: StatusObservation<'_>) -> String {
    // T037 / FR-011 operator reset: clear accumulated calibration BEFORE
    // rendering, so the surface returns to `Deferred`. Never rebuilds the
    // index (only the calibration tables are cleared).
    let reset_note = (request.reset_calibration == Some(true))
        .then(|| calibration_reset_note(reset_calibration(observed.store)));
    let mut ctx = StelStatusContext::from_server(
        observed.surface,
        observed.project_name,
        observed.project_root,
        observed.index_ready,
        observed.index_files,
        observed.index_symbols,
        observed.ledger,
        observed.session_tokens,
    )
    // US3/T029: surface the durable ledger subsystem state so
    // restart-survival is observable from `status`.
    .with_durable_ledger(durable_ledger_state(observed.store));
    // T033 / FR-009: override the in-memory verdict with the DURABLE one. After
    // a reset above, the durable read sees zero samples -> `Deferred`.
    if let Some(verdict) = durable_calibration_verdict(observed.store) {
        ctx = ctx.with_calibration_verdict(verdict);
    }
    ctx.daemon_env_surface = observed.daemon_env_surface;
    ctx.orphaned_daemon_pid = observed.orphaned_daemon_pid;

    let mut body = format_stel_status(request, &ctx);
    for line in observed.trailing_lines {
        body.push('\n');
        body.push_str(&line);
    }
    match reset_note {
        Some(note) => format!("{body}\n{note}"),
        None => body,
    }
}

/// One facade answer's economics, as the ledger records it.
pub struct FacadeLedgerInput<'a> {
    pub surface: &'static str,
    pub plan: &'a StelPlan,
    pub decision: &'a StelDecision,
    pub plan_summary: String,
    pub session_tokens_served: i64,
    pub body: &'a str,
    pub legacy_executed: bool,
    pub selected_tool: &'a str,
    pub tools_called: Option<Vec<String>>,
    pub tuned: Option<&'a TunedEstimateConstants>,
}

/// Record one facade answer in the session ledger, hand the event to the
/// caller's durable write-through, and prepend the trust envelope.
pub fn finalize_with_ledger(
    ledger: &SessionLedger,
    input: FacadeLedgerInput<'_>,
    persist: impl FnOnce(&StelLedgerEvent),
) -> String {
    let (output, event) = render_facade_answer(input);
    ledger.push(event.clone());
    // US3/T028: durable write-through after the in-memory push. Single
    // ledger path (no double-count); degrades to a logged no-op on a store
    // error and never fails the request (FR-011).
    persist(&event);
    output
}

/// The trust-enveloped facade answer and the ledger event it records. A host
/// that admits its output after rendering records the event only once the
/// answer is delivered.
pub fn render_facade_answer(input: FacadeLedgerInput<'_>) -> (String, StelLedgerEvent) {
    use super::handler::{self, finalize_symforge_output, metrics_for_decision_tuned};
    use super::ledger::{LedgerCaptureInput, capture_ledger, format_ledger_envelope_line};

    let response_tokens = handler::estimate_tokens(input.body);
    // T032: record the prediction the predictor ACTUALLY made for this call —
    // tuned when a validated tuning is in force, static otherwise — so the
    // ledger's predicted-vs-actual residual reflects the live estimator and
    // the next tuning pass measures progress against it (hysteresis).
    let metrics = metrics_for_decision_tuned(
        input.plan_summary,
        input.decision,
        input.plan,
        response_tokens,
        input.session_tokens_served,
        input.tuned,
    );
    let (event, meta) = capture_ledger(&LedgerCaptureInput {
        plan: input.plan,
        decision: input.decision,
        economics: &metrics.economics,
        selected_tool: input.selected_tool,
        tools_called: input.tools_called.as_deref(),
        legacy_executed: input.legacy_executed,
        output_body: input.body,
        surface: input.surface,
    });
    let ledger_line = format_ledger_envelope_line(&event, &meta);
    (
        finalize_symforge_output(metrics, ledger_line, input.body),
        event,
    )
}

/// Canonicalize the deepest existing ancestor of `path`, then append a truly
/// missing suffix without resolving it lexically.
///
/// This preserves filesystem semantics for `link/..` and catches a missing
/// leaf beneath an escaping symlink/junction. Any error other than an ordinary
/// missing component fails closed. A dangling link also fails closed because
/// `symlink_metadata` can observe the link even though `canonicalize` cannot
/// resolve its target.
pub(crate) fn canonicalize_with_missing_tail(path: &Path) -> Option<PathBuf> {
    let mut ancestor = path.to_path_buf();
    let mut missing_tail = Vec::<std::ffi::OsString>::new();

    loop {
        match ancestor.canonicalize() {
            Ok(mut canonical) => {
                for component in missing_tail.iter().rev() {
                    canonical.push(component);
                }
                return Some(canonical);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match std::fs::symlink_metadata(&ancestor) {
                    Ok(_) => return None,
                    Err(metadata_error)
                        if metadata_error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(_) => return None,
                }
                missing_tail.push(ancestor.file_name()?.to_os_string());
                if !ancestor.pop() {
                    return None;
                }
            }
            Err(_) => return None,
        }
    }
}

/// Whether `path` (a `symforge` `path:` filter) resolves WITHIN the bound
/// project `root` (012 D6 / contracts §3c). `path:` is a within-project filter,
/// never a project selector, so a path that escapes the bound root is a caller
/// error.
///
/// Resolution: a relative `path` is joined onto `root`; an absolute `path` is
/// used as-is. The bound root must canonicalize. The target is resolved through
/// its deepest existing ancestor so symlinks/junctions and `..` retain filesystem
/// semantics even when the final leaf does not exist. Containment is the
/// canonical root being a component prefix of the resolved target (equal counts
/// as within); observation failures reject the filter.
pub(crate) fn path_is_within_bound_project(path: &str, root: &Path) -> bool {
    let Ok(canonical_root) = root.canonicalize() else {
        return false;
    };

    let raw = Path::new(path);
    let resolved = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        canonical_root.join(raw)
    };
    let Some(resolved) = canonicalize_with_missing_tail(&resolved) else {
        return false;
    };

    // Compare with the Windows verbatim (`\\?\`) prefix stripped from BOTH sides
    // so a canonicalized root (which gains `\\?\`) and a lexically-normalized
    // not-yet-existing target (which does not) still compare correctly.
    let resolved_cmp = strip_verbatim_prefix(&resolved);
    let root_cmp = strip_verbatim_prefix(&canonical_root);
    resolved_cmp.starts_with(&root_cmp)
}

/// The compact facade forwards `path` as an index key/filter, never as an OS
/// path. Reject every rooted/prefixed spelling, including Windows drive-relative
/// (`C:foo`) and root-relative (`\foo`) forms that `Path::is_absolute` does not
/// classify uniformly across platforms.
pub(crate) fn facade_path_is_repo_relative(path: &str) -> bool {
    let bytes = path.as_bytes();
    if path.starts_with('/')
        || path.starts_with('\\')
        || (bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
    {
        return false;
    }
    !Path::new(path).components().any(|component| {
        matches!(
            component,
            std::path::Component::Prefix(_) | std::path::Component::RootDir
        )
    })
}

/// Strip the Windows verbatim/UNC `\\?\` prefix from a path for comparison,
/// returning a plain comparable `PathBuf`. On a path without the prefix (or on
/// non-Windows) this is an allocation-light passthrough.
///
/// We rebuild the path from its components: a `Prefix` component that is verbatim
/// (`\\?\C:`, `\\?\UNC\...`) is replaced by its plain disk/UNC form so it lines
/// up with a non-canonicalized (lexically normalized) sibling path.
pub(crate) fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    use std::path::{Component, Prefix};
    let mut out = PathBuf::new();
    let mut rebuilt_prefix_already_rooted = false;
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => match prefix.kind() {
                Prefix::VerbatimDisk(disk) => {
                    out.push(format!("{}:\\", disk as char));
                    rebuilt_prefix_already_rooted = true;
                }
                Prefix::VerbatimUNC(server, share) => {
                    let mut unc = std::ffi::OsString::from(r"\\");
                    unc.push(server);
                    unc.push(r"\");
                    unc.push(share);
                    out.push(unc);
                    rebuilt_prefix_already_rooted = true;
                }
                _ => {
                    out.push(component.as_os_str());
                    rebuilt_prefix_already_rooted = false;
                }
            },
            Component::RootDir => {
                // Rebuilt verbatim disk/UNC prefixes already include their root
                // separator. Ordinary disk prefixes (`C:`) do not: preserve the
                // following RootDir so an absolute path never becomes drive-
                // relative during comparison.
                if !rebuilt_prefix_already_rooted {
                    out.push(component.as_os_str());
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// US5 economics grounding (010 FR-014, D2): stamp real target byte sizes onto
/// the single-file read steps of a plan from the answering index, so the L2
/// gate predicts from actual work instead of the plan-only constants. Moved
/// from the MCP handler; see `SymForgeServer::ground_plan_economics` for the
/// grounding rule.
pub fn ground_plan_economics(guard: &crate::live_index::LiveIndex, plan: &mut StelPlan) {
    if !guard.is_ready() {
        return;
    }
    for step in &mut plan.steps {
        if !step.index_refs.is_empty() {
            continue;
        }
        // Only single-file read tools have a "read this one file" manual
        // baseline. Symbol-name resolution applies to `get_symbol` only;
        // `find_references` shares the `name` arg but is a multi-file trace.
        let resolve_symbol = step.tool == "get_symbol";
        if !matches!(
            step.tool.as_str(),
            "get_file_context" | "get_file_content" | "get_symbol"
        ) {
            continue;
        }
        let resolved_path = step
            .args
            .get("path")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(str::to_string)
            .or_else(|| {
                if !resolve_symbol {
                    return None;
                }
                // Resolve a path-less `get_symbol` step to the file the symbol
                // is defined in, but only when exactly one file defines it —
                // an ambiguous symbol has no single manual baseline, so we
                // leave it on the plan-only floor.
                let name = step
                    .args
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|name| !name.is_empty())?;
                let candidates = symbol_candidate_paths(guard, name);
                if candidates.len() == 1 {
                    candidates.into_iter().next()
                } else {
                    None
                }
            });
        let Some(path) = resolved_path else {
            continue;
        };
        if let Some(file) = guard.capture_shared_file(&path) {
            step.index_refs
                .push(super::controller::index_ref_for_target(
                    path,
                    file.content.len() as u64,
                ));
        }
    }
}

/// The best path-like anchor for a fused find: the top non-metadata
/// `search_files` hit for the whole query, else for its first token that has
/// one.
pub fn resolve_find_fusion_cochange_anchor(
    guard: &crate::live_index::LiveIndex,
    query: &str,
) -> Option<String> {
    let top_hit = |q: &str| -> Option<String> {
        match guard.capture_search_files_view(q, 5, None, None) {
            SearchFilesView::Found { hits, .. } => hits
                .into_iter()
                .find(|hit| hit.tier != SearchFilesTier::MetadataOnly)
                .map(|hit| hit.path),
            _ => None,
        }
    };
    if let Some(hit) = top_hit(query) {
        return Some(hit);
    }
    query
        .split_whitespace()
        .filter(|tok| tok.chars().any(char::is_alphanumeric))
        .find_map(top_hit)
}

/// Inject the resolved co-change anchor into a fused-find `search_files` step
/// (see `SymForgeServer::inject_find_fusion_cochange_anchor`).
pub fn inject_find_fusion_cochange_anchor(
    live: &crate::live_index::LiveIndex,
    tool: &str,
    args: &serde_json::Value,
    may_use_local_project_state: bool,
) -> serde_json::Value {
    let is_fusion_path_step = tool == "search_files"
        && args.get("rank_by").and_then(serde_json::Value::as_str) == Some("path+cochange")
        && args.get("anchor_path").is_none();
    if !is_fusion_path_step {
        return args.clone();
    }
    let query = args
        .get("query")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let mut args = args.clone();
    let Some(map) = args.as_object_mut() else {
        return args;
    };
    if !may_use_local_project_state {
        // A healthy daemon can route the primitive to a foreign project,
        // but this adapter's index still belongs to its home project. Drop
        // the speculative co-change mode rather than deriving target args
        // from a home-only anchor.
        map.remove("rank_by");
        return args;
    }
    match resolve_find_fusion_cochange_anchor(live, query) {
        Some(anchor) => {
            // Retarget the path side to the anchor's basename STEM. The stem
            // names the anchor's own basename, so the anchor clears the
            // `CO_CHANGE_ANCHOR_CONFIDENCE_FLOOR=basename` gate (via the
            // SF-006 stem-equals-basename anchor promotion), while files
            // that share the stem prefix (a common co-change-partner naming
            // pattern) remain candidates the boost can promote. Falls back
            // to the full anchor path when it has no usable stem.
            let path_query = std::path::Path::new(&anchor)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .filter(|stem| stem.len() >= 3)
                .map(str::to_string)
                .unwrap_or_else(|| anchor.clone());
            map.insert("query".to_string(), serde_json::Value::String(path_query));
            map.insert("anchor_path".to_string(), serde_json::Value::String(anchor));
        }
        None => {
            // No path-like anchor → pure path ranking on the original query.
            // Drop the co-change request so search_files does not emit a
            // fallback-evidence note for a speculative request.
            map.remove("rank_by");
        }
    }
    args
}

/// Append git co-change partners to an impact-intent envelope from the
/// answering publication's temporal snapshot.
pub fn append_impact_intent_cochanges(
    temporal: &crate::live_index::git_temporal::GitTemporalIndex,
    body: &mut String,
    args: &serde_json::Value,
) {
    const IMPACT_INTENT_COCHANGE_LIMIT: usize = 5;

    let Some(path) = args.get("path").and_then(|p| p.as_str()) else {
        return;
    };

    match temporal.state {
        crate::live_index::git_temporal::GitTemporalState::Ready => {
            let normalized = path.replace('\\', "/");
            match temporal.files.get(&normalized) {
                Some(history) => {
                    body.push_str("\n\n");
                    body.push_str(
                        &crate::index_lifecycle::guidance::file_impact::co_changes_result_view(
                            &normalized,
                            history,
                            IMPACT_INTENT_COCHANGE_LIMIT,
                        ),
                    );
                }
                None => {
                    body.push_str("\n\nNo git co-change data found for this file.");
                }
            }
        }
        crate::live_index::git_temporal::GitTemporalState::Pending
        | crate::live_index::git_temporal::GitTemporalState::Computing => {
            body.push_str("\n\nGit temporal data is still loading. Co-changes unavailable.");
        }
        crate::live_index::git_temporal::GitTemporalState::Unavailable(ref reason) => {
            body.push_str(&format!("\n\nGit temporal data unavailable: {reason}"));
        }
    }
}

/// Task 4 (outstanding-work hardening): the read/guidance verbs that accept ONE
/// optional `project` selector, resolved by `DaemonState::runtime_for_target`
/// in `call_tool_handler` before decode. Exactly the plan's parity table minus
/// the set-valued discovery verbs above (which own `project`/`projects` in
/// `execute_tool_call`) and minus `context_inventory` (session-scoped, no
/// selector). `search_files` is deliberately in BOTH sets: a lone `project`
/// routes here to the FULL single-project handler (resolve/coupling modes),
/// while `projects` fans out via the cross-project read route. Structural edits route the same way (Task 5): the selector is
/// batch-level only — each call stays one single-project transaction, and the
/// existing worktree/`working_directory` validation then runs against the
/// SELECTED project's repository, so an unrelated root rejects before preview
/// or apply.
pub fn single_project_routed_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "get_symbol"
            | "get_symbol_context"
            | "get_file_context"
            | "get_file_content"
            | "get_repo_map"
            | "search_files"
            | "find_dependents"
            | "diff_symbols"
            | "what_changed"
            | "analyze_file_impact"
            | "validate_file_syntax"
            | "explore"
            | "ask"
            | "conventions"
            | "edit_plan"
            | "investigation_suggest"
            | "replace_symbol_body"
            | "edit_within_symbol"
            | "insert_symbol"
            | "delete_symbol"
            | "batch_edit"
            | "batch_insert"
            | "batch_rename"
            | "symforge_edit"
            | "curate_knowledge"
    )
}

/// Task 4 Step 5: whether a planned facade step's tool accepts the single
/// `project` selector the facade routes: the daemon's single-project routed
/// set plus the set-valued discovery verbs.
pub fn facade_step_accepts_project(tool: &str) -> bool {
    single_project_routed_tool(tool)
        || matches!(
            tool,
            "search_symbols"
                | "search_text"
                | "search_knowledge"
                | "find_references"
                | "search_files"
        )
}

/// Classify a `symforge_edit` refusal body. A target that does not exist is
/// `NotFound`, a selector that matches several is `Ambiguous`, anything else
/// the caller asked for wrongly is `InvalidRequest`. Every arm is a mutation
/// that applied nothing.
pub fn mutation_refusal_outcome(
    text: &str,
) -> crate::index_lifecycle::guidance::outcome::OutcomeClass {
    let lower = text.to_ascii_lowercase();
    if lower.contains("symbol not found")
        || lower.contains("file not found")
        || lower.contains("file not indexed:")
        || lower.contains(" not found within symbol ")
    {
        crate::index_lifecycle::guidance::outcome::OutcomeClass::NotFound
    } else if text.contains("Ambiguous:") {
        crate::index_lifecycle::guidance::outcome::OutcomeClass::Ambiguous
    } else {
        crate::index_lifecycle::guidance::outcome::OutcomeClass::InvalidRequest
    }
}
