//! Redacted secret-remediation requests and source-bound host authority.
//! The request/preview may cross a host transport; apply authority never can.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::embed::SourceRefusal;
use crate::embed::parity::host::OperationControl;

/// Per-operation scan ceilings chosen by the trusted room host.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretScanLimits {
    pub max_files: u32,
    pub max_findings: u32,
    pub max_total_bytes: u64,
}

impl SecretScanLimits {
    pub(crate) fn valid(self) -> bool {
        self.max_files > 0
            && self.max_files <= 100_000
            && self.max_findings > 0
            && self.max_findings <= 10_000
            && self.max_total_bytes > 0
    }
}

impl Default for SecretScanLimits {
    fn default() -> Self {
        Self {
            max_files: 4_096,
            max_findings: 256,
            max_total_bytes: 64 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecretRemediationAction {
    Externalize,
    Encrypt,
    Dismiss,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SecretRemediationScope {
    Paths(Vec<String>),
    Repo,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretRemediationRequest {
    pub scope: SecretRemediationScope,
    pub finding_ids: Vec<String>,
    pub action: SecretRemediationAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecretActionUnavailable {
    ExternalToolUnavailable,
    RecipientUnavailable,
    UnsupportedFormat,
    RuleCannotBeDismissed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretActionAvailability {
    pub action: SecretRemediationAction,
    pub unavailable: Option<SecretActionUnavailable>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretFindingSummary {
    pub id: String,
    pub path: String,
    pub content_hash: String,
    pub rule_id: String,
    pub line_start: u32,
    pub line_end: u32,
    pub shape: String,
    pub actions: Vec<SecretActionAvailability>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretFindingsClaim {
    pub publication_identity: String,
    pub source_version: u64,
    pub findings: Vec<SecretFindingSummary>,
    pub scanned_files: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretFileGuard {
    pub path: String,
    pub content_hash: String,
}

/// Preview carries hashes and masked text only; it never carries source bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretRemediationPreview {
    pub request: SecretRemediationRequest,
    pub publication_identity: String,
    pub source_version: u64,
    pub files: Vec<SecretFileGuard>,
    pub would_write: Vec<String>,
    pub masked_summary: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecretApplyStatus {
    Clean,
    StillSensitive,
    Indeterminate,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretRemediationApplied {
    pub status: SecretApplyStatus,
    pub written: Vec<String>,
    pub replayed: bool,
    pub refresh_ticket_identity: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecretRemediationRefusalKind {
    InvalidRequest,
    InvalidScope,
    FindingNotFound,
    ScanIndeterminate,
    RuleCannotBeDismissed,
    ExternalToolUnavailable,
    RecipientUnavailable,
    SourceUnavailable,
    StalePublication,
    StaleContent,
    WriteAuthorityRefused,
    ReplayUnavailable,
    ReplayConflict,
    WriteConflict,
    WriteUncertain,
    Cancelled,
    DeadlineExceeded,
    ResourceLimit,
}

#[derive(Debug)]
pub enum SecretRemediationError {
    Source(SourceRefusal),
    Refused(SecretRemediationRefusalKind),
}

impl From<SourceRefusal> for SecretRemediationError {
    fn from(value: SourceRefusal) -> Self {
        Self::Source(value)
    }
}

/// Host-selected local SOPS executable and public age recipient.
/// This descriptor is never deserialized from a request.
#[derive(Clone)]
pub struct SecretExternalTool {
    pub(crate) binary: PathBuf,
    pub(crate) recipient: String,
    pub(crate) timeout: Duration,
}

impl SecretExternalTool {
    pub fn new(
        binary: PathBuf,
        recipient: String,
        timeout: Duration,
    ) -> Result<Self, SecretRemediationRefusalKind> {
        if !binary.is_absolute() || !binary.is_file() {
            return Err(SecretRemediationRefusalKind::ExternalToolUnavailable);
        }
        if !recipient.starts_with("age1") || recipient.len() <= 16 {
            return Err(SecretRemediationRefusalKind::RecipientUnavailable);
        }
        if timeout.is_zero() {
            return Err(SecretRemediationRefusalKind::DeadlineExceeded);
        }
        Ok(Self {
            binary,
            recipient,
            timeout,
        })
    }
}

/// Trusted host capability. Root and stable room/source scope are never read
/// from a deserialized request; cancellation is checked before side effects.
pub struct SecretApplyAuthority {
    pub(crate) root: PathBuf,
    pub(crate) scope: String,
    pub(crate) cancel: Arc<AtomicBool>,
    pub(crate) scan_limits: SecretScanLimits,
    pub(crate) control: Option<OperationControl>,
    pub(crate) external_tool: Option<SecretExternalTool>,
}

impl SecretApplyAuthority {
    pub fn for_source_root(
        root: PathBuf,
        scope: String,
        cancel: Arc<AtomicBool>,
        scan_limits: SecretScanLimits,
        control: Option<OperationControl>,
        external_tool: Option<SecretExternalTool>,
    ) -> Result<Self, SecretRemediationRefusalKind> {
        if !root.is_absolute()
            || scope.trim().is_empty()
            || scope.len() > 256
            || !scan_limits.valid()
        {
            return Err(SecretRemediationRefusalKind::WriteAuthorityRefused);
        }
        Ok(Self {
            root,
            scope,
            cancel,
            scan_limits,
            control,
            external_tool,
        })
    }
}
