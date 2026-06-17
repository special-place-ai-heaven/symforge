//! `HarnessRegistry` — the catalog of known MCP clients and the per-client
//! logic to detect / add / refresh a SymForge **HTTP attach** entry (the `004
//! serve` URL + Bearer key) in each client's config.
//!
//! This factors the client-path knowledge already encoded in
//! [`crate::cli::init`] (`InitPaths`, `claude_desktop_config_path`,
//! `read_config_text`) rather than re-deriving it. The existing `init` flow
//! writes **stdio** (`command`) entries that launch the local binary; this
//! module writes **HTTP** entries (`url` + `Authorization: Bearer`) that attach
//! a client to an already-running `symforge serve`.
//!
//! All transforms here are pure (config-text in → config-text out, no I/O); the
//! backup-then-write machinery lives in [`crate::cli::harness_apply`].

use std::path::{Path, PathBuf};

use anyhow::Context;
use serde_json::{Value, json};
use toml_edit::{DocumentMut, Item, Table, value};

use crate::cli::init::{claude_desktop_config_path, read_config_text};

/// The MCP server entry name written into every client config.
pub const SYMFORGE_SERVER_NAME: &str = "symforge";

/// Stable identifier for a known harness target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarnessId {
    ClaudeCode,
    ClaudeDesktop,
    Codex,
    Gemini,
    KiloCode,
    Cursor,
}

impl HarnessId {
    pub fn slug(self) -> &'static str {
        match self {
            HarnessId::ClaudeCode => "claude",
            HarnessId::ClaudeDesktop => "claude-desktop",
            HarnessId::Codex => "codex",
            HarnessId::Gemini => "gemini",
            HarnessId::KiloCode => "kilo-code",
            HarnessId::Cursor => "cursor",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            HarnessId::ClaudeCode => "Claude Code",
            HarnessId::ClaudeDesktop => "Claude Desktop",
            HarnessId::Codex => "Codex",
            HarnessId::Gemini => "Gemini CLI",
            HarnessId::KiloCode => "Kilo Code",
            HarnessId::Cursor => "Cursor",
        }
    }
}

/// On-disk config shape for a harness target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarnessFormat {
    /// `mcpServers.symforge` JSON object (Claude Code/Desktop, Gemini, Kilo,
    /// Cursor).
    Json,
    /// `[mcp_servers.symforge]` TOML table (Codex).
    Toml,
}

/// The SymForge attach entry: the `004 serve` URL + optional Bearer key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachEntry {
    pub url: String,
    pub bearer_key: Option<String>,
}

impl AttachEntry {
    pub fn new(url: impl Into<String>, bearer_key: Option<String>) -> Self {
        let bearer_key = bearer_key.filter(|k| !k.is_empty());
        Self {
            url: url.into(),
            bearer_key,
        }
    }
}

/// A known MCP client and how its SymForge attach entry is expressed.
#[derive(Debug, Clone)]
pub struct HarnessTarget {
    pub id: HarnessId,
    pub config_path: PathBuf,
    pub format: HarnessFormat,
}

/// Per-client scan result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HarnessState {
    /// Client not detected on the host (config directory absent).
    NotInstalled,
    /// Config present (or its directory exists) but no `symforge` entry.
    Absent,
    /// A `symforge` entry exists and matches the target URL + key.
    PresentCurrent,
    /// A `symforge` entry exists but its URL or key differs from the target.
    PresentStale,
    /// Config exists but does not parse; never overwritten.
    Malformed(String),
}

/// A `HarnessTarget` paired with its current scan state.
#[derive(Debug, Clone)]
pub struct HarnessStatus {
    pub id: HarnessId,
    pub config_path: PathBuf,
    pub format: HarnessFormat,
    pub state: HarnessState,
}

/// The catalog of known harness targets resolved against a host.
#[derive(Debug, Clone)]
pub struct HarnessRegistry {
    targets: Vec<HarnessTarget>,
}

impl HarnessRegistry {
    /// Build the registry for the current host, mirroring the path knowledge in
    /// [`crate::cli::init`] (`InitPaths::from_home_working_dir_*`).
    pub fn known() -> anyhow::Result<Self> {
        let home = dirs::home_dir().context("cannot determine home directory")?;
        let working_dir =
            std::env::current_dir().context("cannot determine current working directory")?;
        Ok(Self::known_with(&home, &working_dir))
    }

    /// Test/injection seam: build the registry against explicit home and
    /// working directories. The Claude Desktop path uses the same resolver
    /// (`claude_desktop_config_path`) the init flow uses.
    pub fn known_with(home: &Path, working_dir: &Path) -> Self {
        let desktop = claude_desktop_config_path(home, None);
        let targets = vec![
            HarnessTarget {
                id: HarnessId::ClaudeCode,
                config_path: home.join(".claude.json"),
                format: HarnessFormat::Json,
            },
            HarnessTarget {
                id: HarnessId::ClaudeDesktop,
                config_path: desktop,
                format: HarnessFormat::Json,
            },
            HarnessTarget {
                id: HarnessId::Codex,
                config_path: home.join(".codex").join("config.toml"),
                format: HarnessFormat::Toml,
            },
            HarnessTarget {
                id: HarnessId::Gemini,
                config_path: home.join(".gemini").join("settings.json"),
                format: HarnessFormat::Json,
            },
            HarnessTarget {
                id: HarnessId::KiloCode,
                config_path: working_dir.join(".kilocode").join("mcp.json"),
                format: HarnessFormat::Json,
            },
            HarnessTarget {
                id: HarnessId::Cursor,
                config_path: home.join(".cursor").join("mcp.json"),
                format: HarnessFormat::Json,
            },
        ];
        Self { targets }
    }

    /// Build a registry from an explicit target list (test seam for fixtures).
    pub fn from_targets(targets: Vec<HarnessTarget>) -> Self {
        Self { targets }
    }

    pub fn targets(&self) -> &[HarnessTarget] {
        &self.targets
    }

    /// Report, per client, whether the SymForge attach entry is absent,
    /// present-current, present-stale, or the client is not installed /
    /// malformed. BOM-safe via the shared `read_config_text`.
    pub fn scan(&self, desired: &AttachEntry) -> Vec<HarnessStatus> {
        self.targets
            .iter()
            .map(|t| HarnessStatus {
                id: t.id,
                config_path: t.config_path.clone(),
                format: t.format,
                state: scan_target(t, desired),
            })
            .collect()
    }
}

/// Resolve the scan state for one target against the desired attach entry.
fn scan_target(target: &HarnessTarget, desired: &AttachEntry) -> HarnessState {
    if !target.config_path.exists() {
        // The client is considered installed only if its config directory
        // exists; otherwise it is NotInstalled (never create in the wrong
        // place). The directory existing but the file missing means the client
        // is installed but has no SymForge entry yet (Absent).
        return match target.config_path.parent() {
            Some(parent) if parent.exists() => HarnessState::Absent,
            _ => HarnessState::NotInstalled,
        };
    }

    let text = match read_config_text(&target.config_path) {
        Ok(text) => text,
        Err(e) => return HarnessState::Malformed(e.to_string()),
    };

    let existing = match target.format {
        HarnessFormat::Json => read_json_entry(&text),
        HarnessFormat::Toml => read_toml_entry(&text),
    };

    match existing {
        Err(e) => HarnessState::Malformed(e),
        Ok(None) => HarnessState::Absent,
        Ok(Some(found)) => {
            if found == *desired {
                HarnessState::PresentCurrent
            } else {
                HarnessState::PresentStale
            }
        }
    }
}

/// Extract the current SymForge attach entry from a JSON `mcpServers` config, if
/// present. Returns `Err` if the document does not parse.
fn read_json_entry(text: &str) -> Result<Option<AttachEntry>, String> {
    let trimmed = text.trim();
    let config: Value = if trimmed.is_empty() {
        json!({})
    } else {
        serde_json::from_str(text).map_err(|e| e.to_string())?
    };

    let entry = config
        .get("mcpServers")
        .and_then(|m| m.get(SYMFORGE_SERVER_NAME));
    let Some(entry) = entry else {
        return Ok(None);
    };

    let url = entry
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let bearer_key = entry
        .get("headers")
        .and_then(|h| h.get("Authorization"))
        .and_then(Value::as_str)
        .and_then(strip_bearer)
        .map(str::to_string);

    Ok(Some(AttachEntry { url, bearer_key }))
}

/// Extract the current SymForge attach entry from a Codex TOML config, if
/// present. Returns `Err` if the document does not parse.
fn read_toml_entry(text: &str) -> Result<Option<AttachEntry>, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let doc = text.parse::<DocumentMut>().map_err(|e| e.to_string())?;

    let entry = doc
        .get("mcp_servers")
        .and_then(Item::as_table)
        .and_then(|t| t.get(SYMFORGE_SERVER_NAME))
        .and_then(Item::as_table);
    let Some(entry) = entry else {
        return Ok(None);
    };

    let url = entry
        .get("url")
        .and_then(Item::as_str)
        .unwrap_or_default()
        .to_string();
    let bearer_key = entry
        .get("bearer_token")
        .and_then(Item::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    Ok(Some(AttachEntry { url, bearer_key }))
}

fn strip_bearer(header: &str) -> Option<&str> {
    header
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

// ---------------------------------------------------------------------------
// Pure add/refresh transforms (config-in → config-out, no I/O)
// ---------------------------------------------------------------------------

/// Apply the desired attach entry to a config's text, returning the rewritten
/// config. Pure: no filesystem access. De-dups any pre-existing SymForge
/// entries to a single refreshed one and preserves the rest of the document.
///
/// `existing_text` is `None` when the config file does not exist yet (a fresh
/// document is created in the client's expected shape).
pub fn apply_attach_entry(
    format: HarnessFormat,
    existing_text: Option<&str>,
    entry: &AttachEntry,
) -> anyhow::Result<String> {
    match format {
        HarnessFormat::Json => apply_json(existing_text, entry),
        HarnessFormat::Toml => apply_toml(existing_text, entry),
    }
}

fn apply_json(existing_text: Option<&str>, entry: &AttachEntry) -> anyhow::Result<String> {
    let mut config: Value = match existing_text {
        Some(text) if !text.trim().is_empty() => {
            serde_json::from_str(text).context("parsing client JSON config")?
        }
        _ => json!({}),
    };

    if !config["mcpServers"].is_object() {
        config["mcpServers"] = json!({});
    }

    let mut server = json!({
        "type": "http",
        "url": entry.url,
    });
    if let Some(key) = &entry.bearer_key {
        server["headers"] = json!({ "Authorization": format!("Bearer {key}") });
    }

    // Single-keyed `mcpServers.symforge` object — a fresh insert overwrites any
    // prior value, so re-apply is naturally de-duped (there can be only one key
    // of that name in a JSON object).
    config["mcpServers"][SYMFORGE_SERVER_NAME] = server;

    let mut pretty = serde_json::to_string_pretty(&config)?;
    pretty.push('\n');
    Ok(pretty)
}

fn apply_toml(existing_text: Option<&str>, entry: &AttachEntry) -> anyhow::Result<String> {
    let mut doc: DocumentMut = match existing_text {
        Some(text) if !text.trim().is_empty() => text
            .parse::<DocumentMut>()
            .context("parsing Codex TOML config")?,
        _ => DocumentMut::new(),
    };

    if !doc.as_table().contains_key("mcp_servers") || !doc["mcp_servers"].is_table() {
        doc["mcp_servers"] = Item::Table(Table::new());
    }
    let mcp_servers = doc["mcp_servers"]
        .as_table_mut()
        .expect("mcp_servers is a table");

    // Replace any prior entry wholesale (de-dup to one current entry).
    let mut server = Table::new();
    server["url"] = value(entry.url.clone());
    if let Some(key) = &entry.bearer_key {
        server["bearer_token"] = value(key.clone());
    }
    mcp_servers.insert(SYMFORGE_SERVER_NAME, Item::Table(server));

    Ok(doc.to_string())
}

// ---------------------------------------------------------------------------
// AAP-typed harness target (008 US3 / FR-005)
// ---------------------------------------------------------------------------
//
// AAP is NOT a generic MCP-client JSON file: it consumes SymForge through the
// library **embed path** (`symforge = { path = "../symforge", features =
// ["embed"] }`) and only optionally through an HTTP MCP attach. The harness hub
// therefore surfaces AAP as its own distinct entry (not mis-scanned as a
// Cursor/Claude JSON) with AAP-appropriate presets, and a write to AAP's config
// must NEVER overwrite the embed path dependency with a stdio-spawn config.
//
// All of this is `server`-gated; the `embed` build compiles none of it
// (G-045 invariant preserved) — it depends on `crate::server::aap`.

/// The integration preset offered for an AAP target.
///
/// `EmbedOnly` is the default (AAP links SymForge as a path dep); `Http` is the
/// optional secondary path (register the running `serve` URL in AAP's MCP
/// settings). Neither is ever a stdio-spawn config for the embed dep.
#[cfg(feature = "server")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AapPresetChoice {
    /// The default: AAP consumes SymForge via the embed path dependency. No MCP
    /// client config is written; the operator copies the Cargo.toml snippet.
    EmbedOnly,
    /// The optional secondary path: register the running `serve` URL as an HTTP
    /// MCP server in AAP's settings (an attach entry, NOT a stdio spawn).
    Http,
}

#[cfg(feature = "server")]
impl AapPresetChoice {
    /// Stable label for diagnostics / the panel.
    pub fn label(self) -> &'static str {
        match self {
            AapPresetChoice::EmbedOnly => "embed_only",
            AapPresetChoice::Http => "http",
        }
    }
}

/// A distinct, AAP-typed harness target — surfaced separately from the generic
/// MCP-client scan so AAP is never mis-handled as a Cursor/Claude JSON file
/// (FR-005 / SC-004).
///
/// Carries the AAP detection result, the canonical embed path dependency (the
/// dep that must never be overwritten), and the available presets (embed-only
/// default, HTTP optional only when a serve attach URL is available).
#[cfg(feature = "server")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AapHarnessTarget {
    /// Whether a sibling AAP checkout was detected.
    pub detected: bool,
    /// The resolved AAP root when detected; `None` otherwise.
    pub root: Option<PathBuf>,
    /// How the root was resolved (`env` | `sibling`); `None` when not detected.
    pub source: Option<&'static str>,
    /// The canonical embed path dependency snippet — the line that must NEVER be
    /// replaced by a stdio-spawn config (FR-004 / SC-003).
    pub embed_path_dep: String,
    /// The presets offered for this target. `EmbedOnly` is always present for a
    /// detected AAP; `Http` is included only when a serve attach URL is available.
    pub presets: Vec<AapPresetChoice>,
}

#[cfg(feature = "server")]
impl AapHarnessTarget {
    /// True when the target offers the HTTP (serve-URL) preset in addition to the
    /// embed-only default.
    pub fn offers_http(&self) -> bool {
        self.presets.contains(&AapPresetChoice::Http)
    }

    /// True when the embed path dep is preserved (never a stdio-spawn config).
    /// The embed dep is, by construction, a Cargo path dependency — this guards
    /// that invariant against a future regression.
    pub fn embed_dep_is_path_not_stdio(&self) -> bool {
        let dep = &self.embed_path_dep;
        dep.contains("path =")
            && dep.contains("features = [\"embed\"]")
            && !dep.contains("command")
            && !dep.contains("stdio")
            && !dep.contains("args")
    }
}

/// Resolve the AAP harness target for the current process.
///
/// Detection precedence matches [`crate::server::aap::AapDetection`] (`AAP_ROOT`
/// env, then the conventional sibling). `serve_attach` carries `Some(url)` only
/// when a `serve` attach URL is available — then the HTTP preset is offered in
/// addition to the always-present embed-only default; otherwise embed-only.
#[cfg(feature = "server")]
pub fn aap_target(serve_attach: Option<&str>) -> AapHarnessTarget {
    aap_target_from(&crate::server::aap::AapDetection::resolve(), serve_attach)
}

/// Build the AAP harness target from an explicit detection result (test seam:
/// fixtures drive detection without depending on the host's real sibling).
///
/// The embed path dep is the canonical [`crate::server::aap::embed_cargo_snippet`]
/// (a path dep with `features=["embed"]`) — never a stdio config. The HTTP preset
/// is offered only when `serve_attach` is present.
#[cfg(feature = "server")]
pub fn aap_target_from(
    detection: &crate::server::aap::AapDetection,
    serve_attach: Option<&str>,
) -> AapHarnessTarget {
    // The embed-only preset is always offered for a detected AAP; HTTP only when
    // a serve attach URL is available. For a not-detected AAP no presets apply.
    let mut presets = Vec::new();
    if detection.detected {
        presets.push(AapPresetChoice::EmbedOnly);
        if serve_attach.is_some_and(|u| !u.is_empty()) {
            presets.push(AapPresetChoice::Http);
        }
    }

    AapHarnessTarget {
        detected: detection.detected,
        root: detection.root.clone(),
        source: detection.source.map(|s| s.label()),
        embed_path_dep: crate::server::aap::embed_cargo_snippet(),
        presets,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> AttachEntry {
        AttachEntry::new("http://127.0.0.1:8787/mcp", Some("sf_key_123".to_string()))
    }

    #[test]
    fn json_absent_adds_entry() {
        let out = apply_json(Some("{}"), &entry()).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            v["mcpServers"]["symforge"]["url"],
            "http://127.0.0.1:8787/mcp"
        );
        assert_eq!(
            v["mcpServers"]["symforge"]["headers"]["Authorization"],
            "Bearer sf_key_123"
        );
        assert_eq!(v["mcpServers"]["symforge"]["type"], "http");
    }

    #[test]
    fn json_preserves_siblings() {
        let input = r#"{"mcpServers":{"other":{"command":"x"}},"numStartups":3}"#;
        let out = apply_json(Some(input), &entry()).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["mcpServers"]["other"]["command"], "x");
        assert_eq!(v["numStartups"], 3);
        assert!(v["mcpServers"]["symforge"].is_object());
    }

    #[test]
    fn json_stale_refreshes_no_dup() {
        let input = r#"{"mcpServers":{"symforge":{"type":"http","url":"http://old/mcp","headers":{"Authorization":"Bearer old"}}}}"#;
        let out = apply_json(Some(input), &entry()).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            v["mcpServers"]["symforge"]["url"],
            "http://127.0.0.1:8787/mcp"
        );
        // Exactly one symforge key (JSON objects cannot duplicate keys).
        let servers = v["mcpServers"].as_object().unwrap();
        assert_eq!(servers.keys().filter(|k| *k == "symforge").count(), 1);
    }

    #[test]
    fn json_keyless_omits_headers() {
        let e = AttachEntry::new("http://127.0.0.1:8787/mcp", None);
        let out = apply_json(Some("{}"), &e).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert!(v["mcpServers"]["symforge"]["headers"].is_null());
    }

    #[test]
    fn json_apply_is_idempotent() {
        let once = apply_json(Some("{}"), &entry()).unwrap();
        let twice = apply_json(Some(&once), &entry()).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn toml_absent_adds_entry() {
        let out = apply_toml(Some("model = \"gpt-5\"\n"), &entry()).unwrap();
        let doc = out.parse::<DocumentMut>().unwrap();
        assert_eq!(
            doc["mcp_servers"]["symforge"]["url"].as_str().unwrap(),
            "http://127.0.0.1:8787/mcp"
        );
        assert_eq!(
            doc["mcp_servers"]["symforge"]["bearer_token"]
                .as_str()
                .unwrap(),
            "sf_key_123"
        );
        // Preserves the unrelated top-level key.
        assert_eq!(doc["model"].as_str().unwrap(), "gpt-5");
    }

    #[test]
    fn toml_preserves_other_servers() {
        let input = "[mcp_servers.other]\ncommand = \"x\"\n";
        let out = apply_toml(Some(input), &entry()).unwrap();
        let doc = out.parse::<DocumentMut>().unwrap();
        assert_eq!(
            doc["mcp_servers"]["other"]["command"].as_str().unwrap(),
            "x"
        );
        assert!(doc["mcp_servers"]["symforge"].is_table());
    }

    #[test]
    fn toml_apply_is_idempotent() {
        let once = apply_toml(None, &entry()).unwrap();
        let twice = apply_toml(Some(&once), &entry()).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn read_json_entry_roundtrip() {
        let out = apply_json(Some("{}"), &entry()).unwrap();
        let read = read_json_entry(&out).unwrap().unwrap();
        assert_eq!(read, entry());
    }

    #[test]
    fn read_toml_entry_roundtrip() {
        let out = apply_toml(None, &entry()).unwrap();
        let read = read_toml_entry(&out).unwrap().unwrap();
        assert_eq!(read, entry());
    }

    #[test]
    fn read_json_entry_absent_is_none() {
        assert!(read_json_entry("{}").unwrap().is_none());
        assert!(read_json_entry("").unwrap().is_none());
    }

    #[test]
    fn read_json_entry_malformed_errors() {
        assert!(read_json_entry("{ not json").is_err());
    }
}
