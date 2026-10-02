//! Actionable refusal metadata (`_meta["symforge/withheld"]`) for feature 034.
//!
//! Peers: `result_status`, `project_evidence`, `repeat_notice`. Secret bytes
//! MUST never appear in these types or their serialization.

use rmcp::model::{JsonObject, MetaObject};
use serde::{Deserialize, Serialize};
use std::future::Future;

use crate::hash::digest_hex;
use crate::knowledge::{SECRET_POLICY_VERSION, SecretFindingDescriptor};

/// `_meta` key for withheld-finding payloads (contracts/withheld-meta.md).
pub const WITHHELD_META_KEY: &str = "symforge/withheld";
pub const WITHHELD_META_CONTRACT_VERSION: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemediationActionName {
    Externalize,
    Encrypt,
    Dismiss,
}

impl RemediationActionName {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Externalize => "externalize",
            Self::Encrypt => "encrypt",
            Self::Dismiss => "dismiss",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemediationActionAvailability {
    pub name: RemediationActionName,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WithheldFinding {
    pub id: String,
    pub rule_id: String,
    pub line_start: u32,
    pub line_end: u32,
    pub shape: String,
    pub confidence: String,
    pub actions: Vec<RemediationActionAvailability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WithheldMeta {
    pub contract_version: u8,
    pub path: String,
    pub policy_version: u32,
    pub findings: Vec<WithheldFinding>,
}

impl WithheldMeta {
    pub fn from_content_findings(path: &str, descriptors: &[SecretFindingDescriptor]) -> Self {
        let findings = descriptors
            .iter()
            .map(|d| {
                let id = mint_finding_id(path, d.rule_id, d.line_start, d.line_end, &d.shape);
                WithheldFinding {
                    id,
                    rule_id: d.rule_id.to_string(),
                    line_start: d.line_start,
                    line_end: d.line_end,
                    shape: d.shape.clone(),
                    confidence: "high".to_string(),
                    actions: default_content_actions(),
                }
            })
            .collect();
        Self {
            contract_version: WITHHELD_META_CONTRACT_VERSION,
            path: path.to_string(),
            policy_version: SECRET_POLICY_VERSION,
            findings,
        }
    }

    /// Path-rule / unscanned refusals: no invented content finding ids.
    pub fn path_rule_only(path: &str, rule_id: &str) -> Self {
        Self {
            contract_version: WITHHELD_META_CONTRACT_VERSION,
            path: path.to_string(),
            policy_version: SECRET_POLICY_VERSION,
            findings: vec![WithheldFinding {
                id: format!("path_rule:{rule_id}"),
                rule_id: rule_id.to_string(),
                line_start: 0,
                line_end: 0,
                shape: "path_rule_credential_file".to_string(),
                confidence: "high".to_string(),
                actions: unavailable_all("path_rule_only"),
            }],
        }
    }

    pub fn unscanned(path: &str) -> Self {
        Self {
            contract_version: WITHHELD_META_CONTRACT_VERSION,
            path: path.to_string(),
            policy_version: SECRET_POLICY_VERSION,
            findings: Vec::new(),
        }
    }
}

fn default_content_actions() -> Vec<RemediationActionAvailability> {
    vec![
        RemediationActionAvailability {
            name: RemediationActionName::Externalize,
            available: true,
            reason: None,
        },
        encrypt_action_availability(),
        RemediationActionAvailability {
            name: RemediationActionName::Dismiss,
            available: true,
            reason: None,
        },
    ]
}

/// Encrypt availability for withheld meta: PATH probe only (no `.sops.yaml` read
/// here — recipient is verified on the remediation tool path).
pub fn encrypt_action_availability() -> RemediationActionAvailability {
    if sops_on_path() {
        RemediationActionAvailability {
            name: RemediationActionName::Encrypt,
            available: true,
            reason: Some("recipient_verified_at_tool".to_string()),
        }
    } else {
        RemediationActionAvailability {
            name: RemediationActionName::Encrypt,
            available: false,
            reason: Some("sops_not_installed".to_string()),
        }
    }
}

fn sops_on_path() -> bool {
    // Cached per-process: meta attaches on many refusals; PATH does not thrash.
    use std::sync::OnceLock;
    static CACHED: OnceLock<bool> = OnceLock::new();
    *CACHED.get_or_init(|| {
        std::env::var_os("PATH")
            .map(|paths| {
                std::env::split_paths(&paths).any(|dir| {
                    let candidate = dir.join(if cfg!(windows) { "sops.exe" } else { "sops" });
                    candidate.is_file()
                })
            })
            .unwrap_or(false)
    })
}

fn unavailable_all(reason: &str) -> Vec<RemediationActionAvailability> {
    [
        RemediationActionName::Externalize,
        RemediationActionName::Encrypt,
        RemediationActionName::Dismiss,
    ]
    .into_iter()
    .map(|name| RemediationActionAvailability {
        name,
        available: false,
        reason: Some(reason.to_string()),
    })
    .collect()
}

/// Stable opaque finding id (same scan identity → same id across preview→apply).
pub fn mint_finding_id(
    path: &str,
    rule_id: &str,
    line_start: u32,
    line_end: u32,
    shape: &str,
) -> String {
    let material =
        format!("v{SECRET_POLICY_VERSION}\0{path}\0{rule_id}\0{line_start}\0{line_end}\0{shape}");
    let hex = digest_hex(material.as_bytes());
    format!("wf_{}", &hex[..24])
}

tokio::task_local! {
    /// Pending withheld meta for the tool result currently being built.
    /// Set on admission refusal lanes that already hold finding evidence;
    /// drained when building [`CallToolResult`] so `refuse_by_policy` stays
    /// syscall-free.
    static PENDING_WITHHELD: std::cell::RefCell<Option<WithheldMeta>>;
}

/// Bind the withheld stash for one tools/call dispatch (nested under project evidence).
pub async fn with_withheld_scope<F, T>(future: F) -> T
where
    F: Future<Output = T>,
{
    PENDING_WITHHELD
        .scope(std::cell::RefCell::new(None), future)
        .await
}

pub fn record_pending_withheld(meta: WithheldMeta) {
    let _ = PENDING_WITHHELD.try_with(|cell| *cell.borrow_mut() = Some(meta));
}

pub fn take_pending_withheld() -> Option<WithheldMeta> {
    PENDING_WITHHELD
        .try_with(|cell| cell.borrow_mut().take())
        .ok()
        .flatten()
}

pub fn attach_withheld_meta(meta: &mut Option<MetaObject>, withheld: WithheldMeta) {
    let meta = meta.get_or_insert_with(|| MetaObject(JsonObject::new()));
    if let Ok(value) = serde_json::to_value(&withheld) {
        meta.0.insert(WITHHELD_META_KEY.to_string(), value);
    }
}

/// Drain pending withheld into an existing result meta map (or create one).
pub fn drain_pending_into_meta(meta: &mut Option<MetaObject>) {
    if let Some(withheld) = take_pending_withheld() {
        attach_withheld_meta(meta, withheld);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_and_id_never_embed_secret_bytes() {
        let secret = b"S3cretValue9x!";
        let shape = crate::knowledge::describe_secret_shape(secret, "secret.context-assignment");
        assert!(!shape.contains("S3cretValue9x!"));
        assert!(shape.contains("14-char"));
        let id = mint_finding_id("cfg.json", "secret.context-assignment", 1, 1, &shape);
        assert!(id.starts_with("wf_"));
        assert!(!id.contains("S3cret"));
        let meta = WithheldMeta::from_content_findings(
            "cfg.json",
            &[SecretFindingDescriptor {
                rule_id: "secret.context-assignment",
                line_start: 1,
                line_end: 1,
                shape: shape.clone(),
            }],
        );
        let json = serde_json::to_string(&meta).unwrap();
        assert!(!json.contains("S3cretValue9x!"));
        assert!(json.contains("symforge") || json.contains("findings"));
    }

    #[test]
    fn path_rule_meta_does_not_invent_content_wf_ids() {
        let meta = WithheldMeta::path_rule_only(".env", "path.env");
        assert!(meta.findings.iter().all(|f| !f.id.starts_with("wf_")));
        assert!(
            meta.findings
                .iter()
                .all(|f| f.actions.iter().all(|a| !a.available))
        );
    }
}
