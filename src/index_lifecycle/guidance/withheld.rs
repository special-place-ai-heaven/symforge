//! Shared actionable admission metadata; never contains source bytes.
use crate::knowledge::{SECRET_POLICY_VERSION, SecretFindingDescriptor};
use serde::{Deserialize, Serialize};

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
    crate::knowledge::secret_remediation::mint_finding_id(
        path, rule_id, line_start, line_end, shape,
    )
}
