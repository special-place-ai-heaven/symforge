//! MCP `status`, `symforge` and `symforge_edit` for an embedded source, over
//! the shared STEL runtime (`crate::stel::runtime`). The source owns the
//! durable economics ledger MCP's stdio bootstrap opens under the project
//! state directory; each query session owns its in-memory session ledger.

use std::sync::{Arc, OnceLock};

use crate::domain::StatePlacement;
use crate::embed::parity::QueryPolicy;
use crate::embed::parity::session::QuerySession;
use crate::embed::parity::stel::{StelNotApplicable, StelStatusReport, StelStatusRequest};
use crate::stel::ledger::SessionLedger;
use crate::stel::ledger_store::StelLedgerStore;

use super::embedded::EmbeddedSourceHandle;

/// `status` lines that describe the MCP daemon topology. An embedded host
/// answers in-process from its own bound source, so each is reported as not
/// applicable with its reason.
const EMBEDDED_STATUS_SECTIONS: &[(&str, &str)] = &[
    (
        "daemon_env_surface",
        "no daemon: the embedded host serves the surface its request declares",
    ),
    (
        "daemon_instance",
        "no daemon process: the embedded host answers from its own bound source",
    ),
    (
        "daemon_degraded_local_fallback",
        "no daemon: there is no daemon working set to fall back from",
    ),
    (
        "daemon_version",
        "no daemon binary: the embedded engine is the host's own build",
    ),
    (
        "proxy_owned_lines",
        "no proxy: this host owns the session ledger and the durable store",
    ),
];

/// The durable STEL ledger of one bound source, opened on first use.
#[derive(Default)]
pub(super) struct StelStoreSlot(OnceLock<Option<Arc<StelLedgerStore>>>);

impl StelStoreSlot {
    /// MCP's stdio bootstrap opens the durable ledger under the project state
    /// directory and wires none without one. An embedded source does the same
    /// when the host permits derived-state preparation, the policy every other
    /// persisted derived lane of a query is held to.
    pub(super) fn get(
        &self,
        placement: &StatePlacement,
        policy: QueryPolicy,
        session_id: impl FnOnce() -> String,
    ) -> Option<Arc<StelLedgerStore>> {
        if !policy.allow_derived_state_preparation {
            return None;
        }
        self.0
            .get_or_init(|| {
                placement
                    .directory()
                    .map(|dir| Arc::new(StelLedgerStore::open(dir, session_id())))
            })
            .clone()
    }
}

/// The canonical surface label a request declares, else `full`: an embedded
/// host routes every MCP lane, and no process environment selects a profile.
pub(super) fn declared_surface(request: &StelStatusRequest) -> &'static str {
    request
        .connection_surface
        .as_deref()
        .and_then(crate::stel::surface::surface_label_from_str)
        .unwrap_or("full")
}

impl EmbeddedSourceHandle {
    /// MCP `status` parity: the shared STEL readout over this source's
    /// published index, the session's economics ledger and the source's
    /// durable calibration store. `reset_calibration` clears that store's
    /// calibration tables before rendering, as MCP does. `None` when the
    /// source is closed.
    pub fn stel_status(
        &self,
        request: &StelStatusRequest,
        session: Option<&QuerySession>,
        policy: QueryPolicy,
    ) -> Option<StelStatusReport> {
        let (shared, root, _) = self.capture_health_context()?;
        let store = self.stel_store(policy);
        let empty = SessionLedger::new();
        let ledger = session.map_or(&empty, QuerySession::stel_ledger);
        let guard = shared.read();
        let project_name = root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("embedded")
            .to_owned();
        let trailing_lines = crate::index_lifecycle::guidance::health::snapshot_verify_status_line(
            guard.load_source(),
            &guard.snapshot_verify_state(),
        )
        .into_iter()
        .chain(std::iter::once(
            crate::index_lifecycle::guidance::health::secret_dismissals_line(Some(&root)),
        ))
        .collect();
        let mut rendered = crate::stel::runtime::render_status_body(
            request,
            crate::stel::runtime::StatusObservation {
                surface: declared_surface(request),
                daemon_env_surface: None,
                orphaned_daemon_pid: None,
                project_name: &project_name,
                project_root: Some(crate::paths::normalized_path_string(&root)),
                index_ready: guard.is_ready(),
                index_files: guard.file_count(),
                index_symbols: guard.symbol_count(),
                ledger,
                session_tokens: session.map_or(0, QuerySession::served_tokens),
                store: store.as_deref(),
                trailing_lines,
            },
        );
        let not_applicable = EMBEDDED_STATUS_SECTIONS
            .iter()
            .map(|(section, reason)| {
                rendered.push_str(&format!("\n{section}: not_applicable ({reason})"));
                StelNotApplicable {
                    section: (*section).to_owned(),
                    reason: (*reason).to_owned(),
                }
            })
            .collect();
        Some(StelStatusReport {
            rendered,
            not_applicable,
        })
    }
}
