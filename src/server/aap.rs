//! AAP (Agent Army Professionals) sibling-repo detection, embed-pin comparison,
//! integration-mode classification, and one-click integration presets.
//!
//! SymForge often runs next to an AAP checkout that consumes it through the
//! **library embed** path (`symforge = { path = "../symforge", features =
//! ["embed"] }`). This module gives the operator AAP-aware convenience without
//! changing the AAP repo: it is **read-only** against the sibling checkout and
//! never mutates it.
//!
//! Pipeline:
//! 1. [`AapDetection::resolve`] — find the sibling root via `AAP_ROOT` (highest
//!    precedence) then the conventional sibling `../Agent_Army_Professionals`,
//!    reporting `detected` + the resolved root + the [`DetectionSource`].
//! 2. [`read_symforge_pin`] — parse the sibling's `Cargo.lock` for the pinned
//!    `symforge` package version (or `None` when the lock is missing /
//!    unparseable / has no symforge package — never a panic).
//! 3. [`EmbedPinComparison`] — compare the pinned version against the running
//!    crate version (`env!("CARGO_PKG_VERSION")`): [`Drift`] / [`Match`] /
//!    [`PinUnknown`](EmbedPinComparison::PinUnknown).
//! 4. [`IntegrationMode`] — classify embed / MCP-URL / both / none from the
//!    configured state.
//! 5. Presets: [`embed_cargo_snippet`] (always, when detected) and
//!    [`serve_url_preset`] (only when `serve` is active). Neither ever emits a
//!    stdio-spawn config for the AAP embed dep (the 7.x anti-pattern).
//!
//! All server-gated (`#[cfg(feature = "server")]` at the module mount in
//! [`super`]); the `embed` build compiles none of it (G-045 preserved).

use std::path::{Path, PathBuf};

/// The conventional sibling directory name for an AAP checkout, resolved
/// relative to the SymForge repo's parent (`../Agent_Army_Professionals`).
pub const CONVENTIONAL_SIBLING_DIR: &str = "Agent_Army_Professionals";

/// Environment variable that, when set to an existing directory, takes
/// precedence over the conventional sibling path for AAP detection.
pub const AAP_ROOT_ENV: &str = "AAP_ROOT";

/// The running SymForge crate version (the version an AAP `Cargo.lock` pin is
/// compared against). Resolved at compile time from `Cargo.toml`.
pub fn running_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// How a detected AAP root was resolved (precedence is documented + deterministic).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionSource {
    /// Resolved from the `AAP_ROOT` environment variable (highest precedence).
    EnvVar,
    /// Resolved from the conventional sibling path (`../Agent_Army_Professionals`).
    ConventionalSibling,
}

impl DetectionSource {
    /// Stable label for the JSON view / UI.
    pub fn label(self) -> &'static str {
        match self {
            DetectionSource::EnvVar => "env",
            DetectionSource::ConventionalSibling => "sibling",
        }
    }
}

/// The result of resolving a sibling AAP checkout.
///
/// `detected == false` is a first-class, non-error outcome (no sibling, or an
/// `AAP_ROOT` pointing at a path that does not exist): the operator panel shows
/// a clean "AAP not detected" empty state rather than an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AapDetection {
    /// Whether an AAP root was found.
    pub detected: bool,
    /// The resolved root path when `detected`; `None` otherwise.
    pub root: Option<PathBuf>,
    /// How the root was resolved when `detected`; `None` otherwise.
    pub source: Option<DetectionSource>,
}

impl AapDetection {
    /// A clean not-detected result (no error).
    fn not_detected() -> Self {
        Self {
            detected: false,
            root: None,
            source: None,
        }
    }

    /// Resolve a sibling AAP checkout for the current process.
    ///
    /// Precedence (deterministic, documented):
    /// 1. `AAP_ROOT` env var, if set AND the path exists as a directory.
    /// 2. The conventional sibling `<symforge_parent>/Agent_Army_Professionals`,
    ///    if it exists as a directory.
    ///
    /// The SymForge root for the conventional-sibling probe is the current
    /// working directory (the running `serve` process is anchored at the repo
    /// it serves). An `AAP_ROOT` that is set but points at a missing path yields
    /// a clean not-detected result (spec edge case), never an error.
    pub fn resolve() -> Self {
        let cwd = std::env::current_dir().ok();
        Self::resolve_with(std::env::var_os(AAP_ROOT_ENV), cwd.as_deref())
    }

    /// Test/injection seam: resolve from an explicit `AAP_ROOT` value and an
    /// explicit SymForge root (for the conventional-sibling probe). Pure aside
    /// from the directory-existence checks.
    pub fn resolve_with(
        aap_root_env: Option<std::ffi::OsString>,
        symforge_root: Option<&Path>,
    ) -> Self {
        // 1. AAP_ROOT env var wins when it points at an existing directory.
        if let Some(raw) = aap_root_env {
            let candidate = PathBuf::from(&raw);
            if !candidate.as_os_str().is_empty() && candidate.is_dir() {
                return Self {
                    detected: true,
                    root: Some(candidate),
                    source: Some(DetectionSource::EnvVar),
                };
            }
            // AAP_ROOT set but missing/empty: fall through to the sibling probe
            // (an explicit-but-absent root is still cleanly not-detected if no
            // sibling exists either).
        }

        // 2. Conventional sibling `<symforge_parent>/Agent_Army_Professionals`.
        if let Some(root) = symforge_root
            && let Some(parent) = root.parent()
        {
            let candidate = parent.join(CONVENTIONAL_SIBLING_DIR);
            if candidate.is_dir() {
                return Self {
                    detected: true,
                    root: Some(candidate),
                    source: Some(DetectionSource::ConventionalSibling),
                };
            }
        }

        Self::not_detected()
    }
}

/// Read the pinned `symforge` package version from an AAP checkout's
/// `Cargo.lock`.
///
/// Returns `Some(version)` when the lock parses and contains a
/// `[[package]] name = "symforge"` entry with a `version`; `None` when the lock
/// is missing, unreadable, unparseable, or has no symforge package. Never
/// panics — a malformed lock is a clean "pin unknown", not an error (spec edge
/// case).
pub fn read_symforge_pin(root: &Path) -> Option<String> {
    let lock_path = root.join("Cargo.lock");
    let text = std::fs::read_to_string(&lock_path).ok()?;
    parse_symforge_pin(&text)
}

/// Parse the `symforge` package version out of `Cargo.lock` text.
///
/// Cargo.lock is TOML with an array of `[[package]]` tables; we find the one
/// whose `name == "symforge"` and return its `version`. Split out from
/// [`read_symforge_pin`] for direct unit testing without filesystem I/O.
pub fn parse_symforge_pin(lock_text: &str) -> Option<String> {
    let doc = lock_text.parse::<toml_edit::DocumentMut>().ok()?;
    let packages = doc.get("package")?.as_array_of_tables()?;
    for pkg in packages.iter() {
        let name = pkg.get("name").and_then(|v| v.as_str());
        if name == Some(SYMFORGE_PACKAGE_NAME) {
            return pkg
                .get("version")
                .and_then(|v| v.as_str())
                .map(str::to_string);
        }
    }
    None
}

/// The package name pinned in AAP's `Cargo.lock` for the embed dependency.
const SYMFORGE_PACKAGE_NAME: &str = "symforge";

/// The comparison of AAP's pinned `symforge` version against the running crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbedPinComparison {
    /// AAP pins a version that differs from the running crate — the embed
    /// build will not match this SymForge; the operator should re-pin.
    Drift {
        /// The version AAP's `Cargo.lock` pins.
        pinned: String,
        /// The running SymForge crate version.
        running: String,
    },
    /// AAP pins exactly the running crate version — no drift.
    Match {
        /// The shared version (pinned == running).
        version: String,
    },
    /// No symforge pin could be read (lock missing / unparseable / no symforge
    /// package). No false drift warning is raised.
    PinUnknown {
        /// The running SymForge crate version (still reported for context).
        running: String,
    },
}

impl EmbedPinComparison {
    /// Compare a (possibly absent) pinned version against the running crate.
    pub fn evaluate(pinned: Option<String>, running: &str) -> Self {
        match pinned {
            Some(pinned) if pinned == running => Self::Match { version: pinned },
            Some(pinned) => Self::Drift {
                pinned,
                running: running.to_string(),
            },
            None => Self::PinUnknown {
                running: running.to_string(),
            },
        }
    }

    /// Convenience: read the pin from an AAP root and compare to the running crate.
    pub fn for_root(root: &Path) -> Self {
        Self::evaluate(read_symforge_pin(root), running_version())
    }

    /// True only for [`Drift`](Self::Drift) — the panel raises a warning iff this
    /// is true (no false positive for `PinUnknown`).
    pub fn is_drift(&self) -> bool {
        matches!(self, Self::Drift { .. })
    }

    /// Stable machine label for the JSON view.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Drift { .. } => "drift",
            Self::Match { .. } => "match",
            Self::PinUnknown { .. } => "pin_unknown",
        }
    }

    /// The pinned version, if known.
    pub fn pinned_version(&self) -> Option<&str> {
        match self {
            Self::Drift { pinned, .. } => Some(pinned),
            Self::Match { version } => Some(version),
            Self::PinUnknown { .. } => None,
        }
    }

    /// The running SymForge crate version (always known).
    pub fn running_version(&self) -> &str {
        match self {
            Self::Drift { running, .. } => running,
            Self::Match { version } => version,
            Self::PinUnknown { running } => running,
        }
    }
}

/// How AAP is integrated with this SymForge, classified from the configured
/// state: the embed path (always available when a sibling is detected) and
/// whether an MCP serve URL is configured for the optional secondary path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationMode {
    /// AAP detected; embed path available, no MCP URL configured (the default
    /// AAP integration).
    Embed,
    /// AAP detected; an MCP serve URL is configured but the embed path is not
    /// the primary route (uncommon — secondary only).
    McpUrl,
    /// AAP detected; both the embed path and an MCP serve URL are present.
    Both,
    /// No AAP detected (or no integration configured) — the empty state.
    None,
}

impl IntegrationMode {
    /// Classify the integration mode from the detection result and whether an
    /// MCP serve URL is currently configured (i.e. `serve` is active).
    ///
    /// When AAP is detected the embed path is always the primary, available
    /// route (the sibling consumes `symforge` as a path dep): so `detected`
    /// implies at least [`Embed`](Self::Embed), and a configured serve URL
    /// upgrades it to [`Both`](Self::Both). Not detected is [`None`](Self::None).
    pub fn classify(detected: bool, serve_url_configured: bool) -> Self {
        match (detected, serve_url_configured) {
            (false, _) => Self::None,
            (true, false) => Self::Embed,
            (true, true) => Self::Both,
        }
    }

    /// Stable label for the JSON view / UI.
    pub fn label(self) -> &'static str {
        match self {
            IntegrationMode::Embed => "embed",
            IntegrationMode::McpUrl => "mcp_url",
            IntegrationMode::Both => "both",
            IntegrationMode::None => "none",
        }
    }
}

// ---------------------------------------------------------------------------
// Presets (T011): embed Cargo.toml snippet + serve-URL MCP preset
// ---------------------------------------------------------------------------

/// The relative path AAP's `Cargo.toml` uses for the SymForge embed path dep
/// (AAP lives beside the `symforge` checkout, so `../symforge`).
pub const EMBED_PATH_DEP_REL: &str = "../symforge";

/// The embed `Cargo.toml` snippet AAP uses to consume SymForge as a path
/// dependency with the `embed` feature.
///
/// This is the **only** sanctioned shape for the AAP embed dep — a path dep
/// with `features = ["embed"]`. It is deliberately NOT a stdio-spawn command
/// (the 7.x init anti-pattern): an embedder links the crate, it does not spawn a
/// subprocess. Always offered for a detected AAP (FR-004 / SC-003).
pub fn embed_cargo_snippet() -> String {
    format!("symforge = {{ path = \"{EMBED_PATH_DEP_REL}\", features = [\"embed\"] }}")
}

/// The serve-URL MCP preset for registering this running `serve` in AAP's MCP
/// settings (the optional secondary path). Only meaningful when `serve` is
/// active — callers pass `Some` only then (so the panel offers it conditionally).
///
/// Placeholder for the bootstrap Bearer secret in admin-panel presets (P2-5).
/// The real key is never echoed in `/api/v1/aap` JSON — operators paste their key.
pub const ADMIN_SERVE_KEY_PLACEHOLDER: &str = "<API_KEY>";

/// Produces a JSON `mcpServers.symforge` HTTP entry (the same attach shape the
/// 005 harness writes), carrying the attach URL and, when present, the Bearer
/// key. This is an MCP-client registration, NOT a replacement for the embed dep:
/// it never emits a `command`/stdio-spawn entry.
pub fn serve_url_preset(attach_url: &str, key: Option<&str>) -> String {
    let mut server = serde_json::json!({
        "type": "http",
        "url": attach_url,
    });
    if let Some(key) = key.filter(|k| !k.is_empty()) {
        server["headers"] = serde_json::json!({ "Authorization": format!("Bearer {key}") });
    }
    let doc = serde_json::json!({ "mcpServers": { "symforge": server } });
    serde_json::to_string_pretty(&doc).unwrap_or_else(|_| "{}".to_string())
}

/// Serve-URL preset for the admin panel: when a bootstrap key is configured,
/// emit [`ADMIN_SERVE_KEY_PLACEHOLDER`] instead of the real secret (P2-5).
pub fn serve_url_preset_for_admin(attach_url: &str, bootstrap_key_configured: bool) -> String {
    if bootstrap_key_configured {
        serve_url_preset(attach_url, Some(ADMIN_SERVE_KEY_PLACEHOLDER))
    } else {
        serve_url_preset(attach_url, None)
    }
}

/// The AAP integration presets surfaced in the panel.
///
/// `embed_snippet` is always present for a detected AAP. `serve_url_snippet` is
/// `Some` only when a `serve` attach URL is available (active serve) — the panel
/// renders the serve preset conditionally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AapPresets {
    /// The embed `Cargo.toml` snippet (always, for a detected AAP).
    pub embed_snippet: String,
    /// The serve-URL MCP registration preset (only when serve is active).
    pub serve_url_snippet: Option<String>,
}

impl AapPresets {
    /// Build the presets. `serve` carries `Some((attach_url, key))` only when a
    /// serve is active; pass `None` otherwise (embed-only).
    pub fn build(serve: Option<(&str, Option<&str>)>) -> Self {
        Self {
            embed_snippet: embed_cargo_snippet(),
            serve_url_snippet: serve.map(|(url, key)| serve_url_preset(url, key)),
        }
    }

    /// Build presets for `/api/v1/aap`: embed snippet always; serve-URL preset
    /// only when `serve_active`, with a redacted Authorization placeholder when
    /// a bootstrap key is configured (never echo the real secret — P2-5).
    pub fn build_for_admin_panel(
        serve_active: bool,
        attach_url: &str,
        bootstrap_key_configured: bool,
    ) -> Self {
        Self {
            embed_snippet: embed_cargo_snippet(),
            serve_url_snippet: serve_active
                .then(|| serve_url_preset_for_admin(attach_url, bootstrap_key_configured)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Detection (T003) ---

    #[test]
    fn resolve_env_var_wins_when_dir_exists() {
        let dir = tempfile::tempdir().unwrap();
        let det = AapDetection::resolve_with(Some(dir.path().as_os_str().to_owned()), None);
        assert!(det.detected);
        assert_eq!(det.source, Some(DetectionSource::EnvVar));
        assert_eq!(det.root.as_deref(), Some(dir.path()));
    }

    #[test]
    fn resolve_env_var_missing_path_is_not_detected_without_sibling() {
        // AAP_ROOT set to a non-existent path + no sibling => clean not-detected.
        let det = AapDetection::resolve_with(
            Some(std::ffi::OsString::from("/no/such/aap/root/xyz")),
            None,
        );
        assert!(!det.detected);
        assert!(det.root.is_none());
        assert!(det.source.is_none());
    }

    #[test]
    fn resolve_conventional_sibling_when_present() {
        // Layout: <tmp>/symforge (the "symforge root") and
        // <tmp>/Agent_Army_Professionals (the sibling).
        let tmp = tempfile::tempdir().unwrap();
        let symforge_root = tmp.path().join("symforge");
        let sibling = tmp.path().join(CONVENTIONAL_SIBLING_DIR);
        std::fs::create_dir_all(&symforge_root).unwrap();
        std::fs::create_dir_all(&sibling).unwrap();

        let det = AapDetection::resolve_with(None, Some(&symforge_root));
        assert!(det.detected);
        assert_eq!(det.source, Some(DetectionSource::ConventionalSibling));
        assert_eq!(det.root.as_deref(), Some(sibling.as_path()));
    }

    #[test]
    fn resolve_env_takes_precedence_over_sibling() {
        let tmp = tempfile::tempdir().unwrap();
        let symforge_root = tmp.path().join("symforge");
        let sibling = tmp.path().join(CONVENTIONAL_SIBLING_DIR);
        let env_root = tmp.path().join("explicit-aap");
        std::fs::create_dir_all(&symforge_root).unwrap();
        std::fs::create_dir_all(&sibling).unwrap();
        std::fs::create_dir_all(&env_root).unwrap();

        let det =
            AapDetection::resolve_with(Some(env_root.as_os_str().to_owned()), Some(&symforge_root));
        assert!(det.detected);
        assert_eq!(det.source, Some(DetectionSource::EnvVar));
        assert_eq!(det.root.as_deref(), Some(env_root.as_path()));
    }

    #[test]
    fn resolve_no_env_no_sibling_is_not_detected() {
        let tmp = tempfile::tempdir().unwrap();
        let symforge_root = tmp.path().join("symforge");
        std::fs::create_dir_all(&symforge_root).unwrap();
        let det = AapDetection::resolve_with(None, Some(&symforge_root));
        assert!(!det.detected);
    }

    // --- Pin parsing (T003) ---

    #[test]
    fn parse_pin_extracts_symforge_version() {
        let lock = r#"
version = 4

[[package]]
name = "aap-code-intel"
version = "0.1.0"

[[package]]
name = "symforge"
version = "7.0.0"
"#;
        assert_eq!(parse_symforge_pin(lock).as_deref(), Some("7.0.0"));
    }

    #[test]
    fn parse_pin_none_when_no_symforge_package() {
        let lock = r#"
version = 4

[[package]]
name = "serde"
version = "1.0.0"
"#;
        assert!(parse_symforge_pin(lock).is_none());
    }

    #[test]
    fn parse_pin_none_when_unparseable() {
        assert!(parse_symforge_pin("{ this is not toml").is_none());
    }

    #[test]
    fn read_pin_none_when_lock_missing() {
        let dir = tempfile::tempdir().unwrap();
        // No Cargo.lock in the dir.
        assert!(read_symforge_pin(dir.path()).is_none());
    }

    // --- Pin comparison (T004) ---

    #[test]
    fn evaluate_drift_when_versions_differ() {
        let cmp = EmbedPinComparison::evaluate(Some("7.0.0".to_string()), "7.29.0");
        assert!(cmp.is_drift());
        assert_eq!(cmp.label(), "drift");
        assert_eq!(cmp.pinned_version(), Some("7.0.0"));
        assert_eq!(cmp.running_version(), "7.29.0");
    }

    #[test]
    fn evaluate_match_when_versions_equal() {
        let cmp = EmbedPinComparison::evaluate(Some("7.29.0".to_string()), "7.29.0");
        assert!(!cmp.is_drift());
        assert_eq!(cmp.label(), "match");
        assert_eq!(cmp.pinned_version(), Some("7.29.0"));
    }

    #[test]
    fn evaluate_pin_unknown_when_no_pin() {
        let cmp = EmbedPinComparison::evaluate(None, "7.29.0");
        assert!(
            !cmp.is_drift(),
            "no pin must NOT raise a false drift warning"
        );
        assert_eq!(cmp.label(), "pin_unknown");
        assert!(cmp.pinned_version().is_none());
        assert_eq!(cmp.running_version(), "7.29.0");
    }

    // --- Integration mode (T004) ---

    #[test]
    fn classify_integration_mode() {
        assert_eq!(
            IntegrationMode::classify(false, false),
            IntegrationMode::None
        );
        assert_eq!(
            IntegrationMode::classify(false, true),
            IntegrationMode::None
        );
        assert_eq!(
            IntegrationMode::classify(true, false),
            IntegrationMode::Embed
        );
        assert_eq!(IntegrationMode::classify(true, true), IntegrationMode::Both);
    }

    // --- Presets (T011) ---

    #[test]
    fn embed_snippet_is_path_dep_with_embed_feature_never_stdio() {
        let snippet = embed_cargo_snippet();
        assert!(snippet.contains("path = \"../symforge\""));
        assert!(snippet.contains("features = [\"embed\"]"));
        // Hard rule: the embed dep is NEVER a stdio-spawn config.
        assert!(!snippet.contains("command"));
        assert!(!snippet.contains("stdio"));
        assert!(!snippet.contains("args"));
    }

    #[test]
    fn serve_url_preset_is_http_with_key_never_stdio() {
        let preset = serve_url_preset("http://127.0.0.1:8787/mcp", Some("sf_key"));
        let v: serde_json::Value = serde_json::from_str(&preset).unwrap();
        assert_eq!(v["mcpServers"]["symforge"]["type"], "http");
        assert_eq!(
            v["mcpServers"]["symforge"]["url"],
            "http://127.0.0.1:8787/mcp"
        );
        assert_eq!(
            v["mcpServers"]["symforge"]["headers"]["Authorization"],
            "Bearer sf_key"
        );
        // Never a stdio-spawn config.
        assert!(v["mcpServers"]["symforge"]["command"].is_null());
    }

    #[test]
    fn serve_url_preset_omits_headers_without_key() {
        let preset = serve_url_preset("http://127.0.0.1:8787/mcp", None);
        let v: serde_json::Value = serde_json::from_str(&preset).unwrap();
        assert!(v["mcpServers"]["symforge"]["headers"].is_null());
        // Empty key is treated as absent.
        let preset2 = serve_url_preset("http://127.0.0.1:8787/mcp", Some(""));
        let v2: serde_json::Value = serde_json::from_str(&preset2).unwrap();
        assert!(v2["mcpServers"]["symforge"]["headers"].is_null());
    }

    #[test]
    fn serve_url_preset_for_admin_redacts_bootstrap_key() {
        let preset = serve_url_preset_for_admin("http://127.0.0.1:8787/mcp", true);
        let v: serde_json::Value = serde_json::from_str(&preset).unwrap();
        assert_eq!(
            v["mcpServers"]["symforge"]["headers"]["Authorization"],
            "Bearer <API_KEY>"
        );
        assert!(!preset.contains("sf_real_secret"));
    }

    #[test]
    fn presets_embed_always_serve_only_when_active() {
        let embed_only = AapPresets::build(None);
        assert!(embed_only.embed_snippet.contains("embed"));
        assert!(embed_only.serve_url_snippet.is_none());

        let with_serve = AapPresets::build(Some(("http://127.0.0.1:8787/mcp", Some("k"))));
        assert!(with_serve.embed_snippet.contains("embed"));
        assert!(with_serve.serve_url_snippet.is_some());
    }
}
