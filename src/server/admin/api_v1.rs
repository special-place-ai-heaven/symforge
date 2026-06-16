//! `/api/v1/*` JSON handlers + read adapters for the operator admin UI.
//!
//! The view DTOs ([`LedgerSummaryView`], [`SurfaceView`], [`HarnessStatusView`],
//! [`SystemSnapshot`]) are thin, serde-serializable projections over the data the
//! `004`/`005` subsystems already own:
//!
//! - [`LedgerSummaryView`] ← [`crate::stel::ledger_store::StelLedgerStore::summary`]
//!   (FR-003): when the store is `Disabled`/absent the view reports
//!   `available = false` and **no** fabricated numbers (spec edge case +
//!   GATE-3).
//! - [`SurfaceView`] ← [`crate::protocol::surface_probe::surface_profile_from_env`]
//!   + the advertised tool list.
//! - [`HarnessStatusView`] ← `005`
//!   [`crate::cli::harness::HarnessRegistry::scan`].
//! - [`SystemSnapshot`] ← std-only telemetry (PID, uptime, index file/symbol
//!   counts, project name) per `research.md` D1 (FR-005).
//!
//! Every handler takes `State<ServerRuntime>` and returns `axum::Json<…>`. The
//! router built here is mounted behind the shared Bearer-auth + Origin-gate
//! layers by [`super::router`] / [`crate::server::serve::run`] — there is no
//! per-handler auth (one enforcement point, same as `/mcp`).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Serialize;

use crate::cli::harness::{AttachEntry, HarnessRegistry, HarnessState, HarnessStatus};
use crate::protocol::surface_probe::{
    SurfaceProfile, list_tools_for_profile, surface_profile_from_env,
};
use crate::server::ServerRuntime;

// ---------------------------------------------------------------------------
// View DTOs (T005)
// ---------------------------------------------------------------------------

/// Economics summary projection. When the durable ledger is unavailable
/// (`Disabled`/not opened), `available` is `false` and the numeric fields are
/// `null` — the UI renders an explicit "unavailable" state, never fake zeros
/// (FR-003 / GATE-3 / SC-004).
#[derive(Debug, Clone, Serialize)]
pub struct LedgerSummaryView {
    /// Whether a durable ledger summary could be read.
    pub available: bool,
    /// Total recorded economics events. `None` when unavailable.
    pub total_events: Option<u64>,
    /// Total net-vs-manual token savings across all events. `None` when unavailable.
    pub total_net_vs_manual: Option<i64>,
    /// Count of accepted events. `None` when unavailable.
    pub accepted_count: Option<u64>,
    /// Distinct sessions observed. `None` when unavailable.
    pub session_count: Option<u64>,
}

impl LedgerSummaryView {
    /// Build from the runtime's optional ledger store.
    pub fn from_runtime(runtime: &ServerRuntime) -> Self {
        match runtime.ledger_store().and_then(|s| s.summary()) {
            Some(summary) => Self {
                available: true,
                total_events: Some(summary.total_events),
                total_net_vs_manual: Some(summary.total_net_vs_manual),
                accepted_count: Some(summary.accepted_count),
                session_count: Some(summary.session_count),
            },
            None => Self {
                available: false,
                total_events: None,
                total_net_vs_manual: None,
                accepted_count: None,
                session_count: None,
            },
        }
    }
}

/// Active tool-surface projection.
#[derive(Debug, Clone, Serialize)]
pub struct SurfaceView {
    /// `full` | `compact` | `meta`.
    pub profile: String,
    /// Number of advertised tools on the active surface.
    pub tool_count: usize,
    /// Advertised tool names on the active surface.
    pub tools: Vec<String>,
}

impl SurfaceView {
    /// Build from the live `SYMFORGE_SURFACE` env profile.
    pub fn from_env() -> Self {
        let profile = surface_profile_from_env();
        let tools: Vec<String> = list_tools_for_profile(profile)
            .into_iter()
            .map(|t| t.name.to_string())
            .collect();
        Self {
            profile: profile_label(profile).to_string(),
            tool_count: tools.len(),
            tools,
        }
    }
}

fn profile_label(profile: SurfaceProfile) -> &'static str {
    match profile {
        SurfaceProfile::Full => "full",
        SurfaceProfile::Compact => "compact",
        SurfaceProfile::Meta => "meta",
    }
}

/// One harness client's attach state.
#[derive(Debug, Clone, Serialize)]
pub struct HarnessEntryView {
    /// Stable slug (e.g. `claude`, `cursor`, `codex`).
    pub id: String,
    /// Human-readable client name.
    pub name: String,
    /// Config path the scan inspected.
    pub config_path: String,
    /// One of: `not_installed`, `absent`, `present_current`, `present_stale`,
    /// `malformed`.
    pub state: String,
    /// Detail for the `malformed` state (parse error), else `null`.
    pub detail: Option<String>,
}

/// Attached-harness status projection (005 `HarnessRegistry::scan`).
#[derive(Debug, Clone, Serialize)]
pub struct HarnessStatusView {
    /// Whether the host harness registry could be resolved.
    pub available: bool,
    pub entries: Vec<HarnessEntryView>,
}

impl HarnessStatusView {
    /// Scan the host's known harness configs against the running server's attach
    /// URL + bootstrap key (005). Degrades to `available = false` with an empty
    /// list if the host directories cannot be resolved.
    pub fn from_runtime(runtime: &ServerRuntime) -> Self {
        // The desired attach entry mirrors what `serve` advertises: the /mcp URL
        // (host:port unknown to the read path here, so use the documented default
        // shape) + the bootstrap key if one is configured. Scan only reports
        // present/stale/absent relative to this; the dashboard surfaces it.
        let bearer = runtime.auth().api_key.clone();
        let desired = AttachEntry::new(crate::server::serve::DEFAULT_LISTEN.to_string(), bearer);
        match HarnessRegistry::known() {
            Ok(registry) => {
                let entries = registry
                    .scan(&desired)
                    .into_iter()
                    .map(entry_view)
                    .collect();
                Self {
                    available: true,
                    entries,
                }
            }
            Err(err) => {
                tracing::warn!(error = %err, "harness registry unavailable for admin scan");
                Self {
                    available: false,
                    entries: vec![],
                }
            }
        }
    }
}

fn entry_view(status: HarnessStatus) -> HarnessEntryView {
    let (state, detail) = match &status.state {
        HarnessState::NotInstalled => ("not_installed".to_string(), None),
        HarnessState::Absent => ("absent".to_string(), None),
        HarnessState::PresentCurrent => ("present_current".to_string(), None),
        HarnessState::PresentStale => ("present_stale".to_string(), None),
        HarnessState::Malformed(msg) => ("malformed".to_string(), Some(msg.clone())),
    };
    HarnessEntryView {
        id: status.id.slug().to_string(),
        name: status.id.display_name().to_string(),
        config_path: status.config_path.display().to_string(),
        state,
        detail,
    }
}

/// System/process telemetry projection (std-only; research.md D1 / FR-005).
#[derive(Debug, Clone, Serialize)]
pub struct SystemSnapshot {
    /// SymForge process id.
    pub pid: u32,
    /// Process uptime in seconds since the runtime was built.
    pub uptime_secs: u64,
    /// Active in-process sessions (the serve runtime is one).
    pub active_sessions: u64,
    /// Indexed project names (one for the serve root, empty when no root).
    pub indexed_projects: Vec<String>,
    /// Number of indexed files in the live index.
    pub indexed_file_count: usize,
    /// Number of indexed symbols in the live index.
    pub indexed_symbol_count: usize,
    /// Live index generation counter.
    pub index_generation: u64,
}

impl SystemSnapshot {
    /// Capture the running server's real PID / uptime / index state.
    pub fn from_runtime(runtime: &ServerRuntime) -> Self {
        let published = runtime.index().published_state();
        let project = runtime.project_name().to_string();
        // An empty index over no project root still names the configured project;
        // report it only when there are indexed files OR a non-default name.
        let indexed_projects = if published.file_count > 0 || project != "project" {
            vec![project]
        } else {
            vec![]
        };
        Self {
            pid: std::process::id(),
            uptime_secs: runtime.uptime().as_secs(),
            active_sessions: 1,
            indexed_projects,
            indexed_file_count: published.file_count,
            indexed_symbol_count: published.symbol_count,
            index_generation: published.generation,
        }
    }
}

/// API-key record projection for `/api/v1/keys` (never carries a raw secret).
#[derive(Debug, Clone, Serialize)]
pub struct KeyRecordView {
    pub id: i64,
    pub label: String,
    pub fingerprint: String,
    pub created_ms: u64,
    pub rotated_ms: Option<u64>,
    pub revoked_ms: Option<u64>,
    pub active: bool,
}

impl From<crate::server::ApiKeyRecord> for KeyRecordView {
    fn from(r: crate::server::ApiKeyRecord) -> Self {
        let active = r.is_active();
        Self {
            id: r.id,
            label: r.label,
            fingerprint: r.fingerprint,
            created_ms: r.created_ms,
            rotated_ms: r.rotated_ms,
            revoked_ms: r.revoked_ms,
            active,
        }
    }
}

/// The list view returned by `GET /api/v1/keys`. `available` is `false` when the
/// key store could not open (the bootstrap `--api-key` still works).
#[derive(Debug, Clone, Serialize)]
pub struct KeyListView {
    pub available: bool,
    pub keys: Vec<KeyRecordView>,
}

/// The mint/rotate response: the new record plus the raw secret shown **once**.
#[derive(Debug, Clone, Serialize)]
pub struct MintedKeyView {
    pub key: KeyRecordView,
    /// The raw bearer secret — present only in this response, never again.
    pub raw_secret: String,
}

/// Request body for minting a key.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct MintRequest {
    #[serde(default)]
    pub label: Option<String>,
}

/// A small machine-readable error body for the JSON API.
#[derive(Debug, Clone, Serialize)]
pub struct ApiError {
    pub error: String,
}

fn api_error(status: StatusCode, message: impl Into<String>) -> axum::response::Response {
    (
        status,
        Json(ApiError {
            error: message.into(),
        }),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// Handlers (T007 / T013 / T017)
// ---------------------------------------------------------------------------

/// `GET /api/v1/summary` — durable economics summary (FR-003).
pub async fn get_summary(State(runtime): State<ServerRuntime>) -> Json<LedgerSummaryView> {
    Json(LedgerSummaryView::from_runtime(&runtime))
}

/// `GET /api/v1/surface` — active tool surface.
pub async fn get_surface(State(_runtime): State<ServerRuntime>) -> Json<SurfaceView> {
    Json(SurfaceView::from_env())
}

/// `GET /api/v1/harness` — attached-harness status (005).
pub async fn get_harness(State(runtime): State<ServerRuntime>) -> Json<HarnessStatusView> {
    Json(HarnessStatusView::from_runtime(&runtime))
}

/// `GET /api/v1/system` — process/index telemetry (FR-005).
pub async fn get_system(State(runtime): State<ServerRuntime>) -> Json<SystemSnapshot> {
    Json(SystemSnapshot::from_runtime(&runtime))
}

/// `GET /api/v1/keys` — list keys (never raw; FR-004).
pub async fn list_keys(State(runtime): State<ServerRuntime>) -> Json<KeyListView> {
    match runtime.key_store() {
        Some(store) => {
            let keys = store
                .list()
                .unwrap_or_default()
                .into_iter()
                .map(KeyRecordView::from)
                .collect();
            Json(KeyListView {
                available: store.is_enabled(),
                keys,
            })
        }
        None => Json(KeyListView {
            available: false,
            keys: vec![],
        }),
    }
}

/// `POST /api/v1/keys` — mint a key; the raw secret is returned **once** (FR-004).
pub async fn mint_key(
    State(runtime): State<ServerRuntime>,
    body: Option<Json<MintRequest>>,
) -> axum::response::Response {
    let label = body
        .and_then(|Json(req)| req.label)
        .unwrap_or_else(|| "api key".to_string());
    let Some(store) = runtime.key_store() else {
        return api_error(StatusCode::SERVICE_UNAVAILABLE, "api-key store unavailable");
    };
    match store.mint(&label) {
        Ok(minted) => (
            StatusCode::CREATED,
            Json(MintedKeyView {
                key: KeyRecordView::from(minted.record),
                raw_secret: minted.raw_secret,
            }),
        )
            .into_response(),
        Err(err) => api_error(StatusCode::SERVICE_UNAVAILABLE, err.to_string()),
    }
}

/// `POST /api/v1/keys/{id}/rotate` — rotate a key; new raw secret returned once.
pub async fn rotate_key(
    State(runtime): State<ServerRuntime>,
    Path(id): Path<i64>,
) -> axum::response::Response {
    let Some(store) = runtime.key_store() else {
        return api_error(StatusCode::SERVICE_UNAVAILABLE, "api-key store unavailable");
    };
    match store.rotate(id) {
        Ok(minted) => Json(MintedKeyView {
            key: KeyRecordView::from(minted.record),
            raw_secret: minted.raw_secret,
        })
        .into_response(),
        Err(err) => api_error(StatusCode::NOT_FOUND, err.to_string()),
    }
}

/// `DELETE /api/v1/keys/{id}` — revoke a key (FR-004 / SC-003).
pub async fn revoke_key(
    State(runtime): State<ServerRuntime>,
    Path(id): Path<i64>,
) -> axum::response::Response {
    let Some(store) = runtime.key_store() else {
        return api_error(StatusCode::SERVICE_UNAVAILABLE, "api-key store unavailable");
    };
    match store.revoke(id) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => api_error(StatusCode::NOT_FOUND, err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_index::LiveIndex;
    use crate::protocol::SymForgeServer;
    use crate::sidecar::governor::RequestGovernor;
    use crate::stel::ledger_store::StelLedgerStore;
    use crate::watcher::WatcherInfo;
    use parking_lot::Mutex;
    use std::sync::Arc;

    fn runtime_with_ledger(ledger: Option<StelLedgerStore>) -> ServerRuntime {
        let index = LiveIndex::empty();
        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let protocol = Arc::new(SymForgeServer::new(
            Arc::clone(&index),
            "admin-view-test".to_string(),
            watcher_info,
            None,
            None,
        ));
        let governor = Arc::new(RequestGovernor::new());
        ServerRuntime::build_runtime(
            index,
            protocol,
            governor,
            crate::server::AuthConfig::new(None),
            ledger,
        )
    }

    #[test]
    fn ledger_view_unavailable_when_no_store() {
        let view = LedgerSummaryView::from_runtime(&runtime_with_ledger(None));
        assert!(!view.available);
        assert!(view.total_events.is_none());
        assert!(view.total_net_vs_manual.is_none());
    }

    #[test]
    fn ledger_view_unavailable_when_disabled() {
        let view =
            LedgerSummaryView::from_runtime(&runtime_with_ledger(Some(StelLedgerStore::Disabled)));
        assert!(!view.available, "Disabled store renders unavailable");
        assert!(view.total_events.is_none());
    }

    #[test]
    fn ledger_view_reports_real_values_when_seeded() {
        let store = StelLedgerStore::open_in_memory("admin-seed").expect("store");
        store.record(&crate::stel::types::StelLedgerEvent {
            ts_ms: 1,
            plan_id: "p".into(),
            surface: "symforge".into(),
            intent: crate::stel::types::IntentBucket::Trace,
            decision: crate::stel::types::AdmissionDecision::Serve,
            tools_called: vec!["find_references".into()],
            predicted_response_tokens: 100,
            actual_response_tokens: 90,
            manual_baseline_tokens: 300,
            net_vs_manual: 210,
            equivalence: None,
            route_confidence: crate::stel::types::RouteConfidence::Exact,
            pff_bypass: None,
            cache_hit: None,
            degrade_flags: vec![],
        });
        let view = LedgerSummaryView::from_runtime(&runtime_with_ledger(Some(store)));
        assert!(view.available);
        assert_eq!(view.total_events, Some(1));
        assert_eq!(view.total_net_vs_manual, Some(210));
    }

    #[test]
    fn surface_view_lists_active_surface() {
        let view = SurfaceView::from_env();
        assert!(!view.tools.is_empty());
        assert_eq!(view.tool_count, view.tools.len());
        assert!(["full", "compact", "meta"].contains(&view.profile.as_str()));
    }

    #[test]
    fn system_snapshot_reports_real_pid() {
        let runtime = runtime_with_ledger(None);
        let snap = SystemSnapshot::from_runtime(&runtime);
        assert_eq!(snap.pid, std::process::id());
        assert_eq!(snap.active_sessions, 1);
    }

    #[test]
    fn key_record_view_omits_raw_secret() {
        let store = crate::server::ApiKeyStore::open_in_memory().expect("store");
        let minted = store.mint("k").expect("mint");
        let view = KeyRecordView::from(minted.record);
        let json = serde_json::to_string(&view).expect("serialize");
        assert!(
            !json.contains(&minted.raw_secret),
            "raw secret must not leak"
        );
        assert!(json.contains("fingerprint"));
    }
}
