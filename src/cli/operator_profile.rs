//! `OperatorSetupProfile` — operator-local setup convenience state (009, D5).
//!
//! Persisted to `<project>/.symforge/operator-setup.json`, mirroring the
//! `OnboardingState` load/save pattern (`cli::onboarding`). Drives reuse-if-running
//! and idempotent re-run (FR-012/013). This is operator convenience state, not an
//! index (Constitution I unaffected).
//!
//! **No secret material is persisted.** `auth_posture` records only *whether* a key
//! is required (`NetworkKeyed`) or not (`LoopbackNoKey`); the key bytes live in the
//! operator's env/keystore, never in this file (contract: operator-profile.md).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::cli::harness::HarnessId;
use crate::cli::setup::InstallationType;

/// On-disk filename for the operator setup profile inside the SymForge data dir.
pub const OPERATOR_SETUP_PROFILE_FILE: &str = "operator-setup.json";

/// The auth shape the operator server was last started with (FR-007).
///
/// Records only the *posture*, never the key: a `NetworkKeyed` bind requires a
/// Bearer key, but the key bytes are never serialized here — they live in the
/// operator's environment/keystore (contract: operator-profile.md, "No secret
/// material").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthPosture {
    /// Loopback bind, no key required.
    #[serde(rename = "loopback-no-key")]
    LoopbackNoKey,
    /// Non-loopback (network) bind, a key is required (stored elsewhere).
    #[serde(rename = "network-keyed")]
    NetworkKeyed,
}

/// Persisted operator setup convenience state (E1).
///
/// Serialized to `<project>/.symforge/operator-setup.json`. `harnesses` is stored
/// as the stable [`HarnessId::slug`] strings (e.g. `"claude"`, `"cursor"`) so the
/// on-disk shape is human-readable and stable across `HarnessId` reordering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatorSetupProfile {
    /// The chosen setup shape (FR-005).
    pub installation_type: InstallationType,
    /// The last verified-bound operator-server port (FR-012); re-runs + admin
    /// reuse it.
    pub port: u16,
    /// The auth posture of the last bind (FR-007); never the key bytes.
    pub auth_posture: AuthPosture,
    /// Which harnesses were configured, by stable slug (for idempotent re-run).
    pub harnesses: Vec<String>,
    /// Last-write stamp (epoch milliseconds).
    pub updated_ms: i64,
}

impl OperatorSetupProfile {
    /// Build a profile from a typed harness-id list (slugs are stored on disk).
    pub fn new(
        installation_type: InstallationType,
        port: u16,
        auth_posture: AuthPosture,
        harnesses: &[HarnessId],
        updated_ms: i64,
    ) -> Self {
        Self {
            installation_type,
            port,
            auth_posture,
            harnesses: harnesses.iter().map(|h| h.slug().to_string()).collect(),
            updated_ms,
        }
    }

    /// Resolve the profile path under `<base>/.symforge/`.
    pub fn path(base: &Path) -> PathBuf {
        crate::paths::resolve_symforge_dir(base).join(OPERATOR_SETUP_PROFILE_FILE)
    }

    /// Load the profile for project `base`. A missing **or** malformed file yields
    /// `None` (a fresh run) — setup is best-effort and must never fail the caller
    /// on a corrupt/old profile (D5, contract: operator-profile.md).
    pub fn load(base: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(Self::path(base)).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Persist this profile for project `base` atomically (temp file in the same
    /// dir + rename), mirroring `harness_apply::atomic_write`. The `.symforge`
    /// directory is created if needed.
    pub fn save(&self, base: &Path) -> std::io::Result<()> {
        let path = Self::path(base);
        let dir = crate::paths::ensure_symforge_dir(base)?;
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        atomic_write_in(&dir, &path, json.as_bytes())
    }
}

/// Atomically write `content` to `path`, staging the temp file in `dir` (the same
/// directory as `path`, so `persist` is a same-filesystem rename). Mirrors
/// `harness_apply::atomic_write` (which is private to that module).
fn atomic_write_in(dir: &Path, path: &Path, content: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(content)?;
    tmp.flush()?;
    tmp.as_file().sync_all()?;
    // rename(2) on Unix / MoveFileExW(MOVEFILE_REPLACE_EXISTING) on Windows.
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> OperatorSetupProfile {
        OperatorSetupProfile::new(
            InstallationType::Both,
            8787,
            AuthPosture::LoopbackNoKey,
            &[HarnessId::ClaudeCode, HarnessId::Cursor],
            1_750_000_000_000,
        )
    }

    #[test]
    fn load_missing_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        // No .symforge/operator-setup.json under a fresh temp project.
        assert_eq!(OperatorSetupProfile::load(dir.path()), None);
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let profile = sample();
        profile.save(dir.path()).expect("save profile");

        let loaded = OperatorSetupProfile::load(dir.path()).expect("load saved profile");
        assert_eq!(loaded, profile);
        // Harnesses persist as stable slugs.
        assert_eq!(loaded.harnesses, vec!["claude", "cursor"]);
        assert_eq!(loaded.auth_posture, AuthPosture::LoopbackNoKey);
    }

    #[test]
    fn malformed_file_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        // Write garbage into the profile path; load must degrade to None, not error.
        crate::paths::ensure_symforge_dir(dir.path()).unwrap();
        std::fs::write(
            OperatorSetupProfile::path(dir.path()),
            b"{ this is not valid json",
        )
        .unwrap();
        assert_eq!(OperatorSetupProfile::load(dir.path()), None);
    }

    #[test]
    fn network_keyed_posture_persists_without_key_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let profile = OperatorSetupProfile::new(
            InstallationType::Server,
            9000,
            AuthPosture::NetworkKeyed,
            &[HarnessId::Codex],
            1,
        );
        profile.save(dir.path()).expect("save");
        let text = std::fs::read_to_string(OperatorSetupProfile::path(dir.path())).unwrap();
        // The posture is recorded; no key material is present in the file.
        assert!(text.contains("network-keyed"));
        let loaded = OperatorSetupProfile::load(dir.path()).expect("load");
        assert_eq!(loaded.auth_posture, AuthPosture::NetworkKeyed);
    }
}
