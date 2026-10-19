//! CLI control surface for project-config trust.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow, bail};
use clap::Subcommand;
use serde::{Deserialize, Serialize};

use crate::edit_safety::trust::{ProjectConfigTrust, TrustEvaluation, TrustRecord, TrustStatus};

#[derive(Subcommand, Debug)]
pub enum TrustSubcommand {
    /// Audit, accept, or revoke trust for project-local `.symforge` config
    #[command(name = "project-config")]
    ProjectConfig {
        #[command(subcommand)]
        command: ProjectConfigCommand,
    },
}

#[derive(Subcommand, Debug)]
pub enum ProjectConfigCommand {
    /// Show current trust status for project-local `.symforge` config
    Status {
        /// Project root to evaluate
        #[arg(long)]
        project: PathBuf,
    },
    /// Record the reviewed current project-config hash as trusted
    Accept {
        /// Project root to evaluate
        #[arg(long)]
        project: PathBuf,
        /// Hash previously shown by `status`
        #[arg(long)]
        hash: String,
    },
    /// Remove this project's trust record from the user-local store
    Revoke {
        /// Project root to revoke
        #[arg(long)]
        project: PathBuf,
    },
}

#[derive(Debug, Deserialize, Serialize)]
struct CliTrustStore {
    schema_version: u32,
    records: BTreeMap<String, TrustRecord>,
}

impl Default for CliTrustStore {
    fn default() -> Self {
        Self {
            schema_version: 1,
            records: BTreeMap::new(),
        }
    }
}

pub fn run_trust(command: &TrustSubcommand) -> anyhow::Result<()> {
    match command {
        TrustSubcommand::ProjectConfig { command } => run_project_config(command),
    }
}

fn run_project_config(command: &ProjectConfigCommand) -> anyhow::Result<()> {
    match command {
        ProjectConfigCommand::Status { project } => status(project),
        ProjectConfigCommand::Accept { project, hash } => accept(project, hash),
        ProjectConfigCommand::Revoke { project } => revoke(project),
    }
}

fn default_trust() -> anyhow::Result<ProjectConfigTrust> {
    ProjectConfigTrust::default_store().ok_or_else(|| {
        anyhow!("ProjectConfigTrustUnavailable: could not determine user-local data directory")
    })
}

fn status(project: &Path) -> anyhow::Result<()> {
    let trust = default_trust()?;
    let evaluation = trust.evaluate(project);
    print_evaluation("ProjectConfigTrustStatus", &evaluation, trust.store_path());
    Ok(())
}

fn accept(project: &Path, reviewed_hash: &str) -> anyhow::Result<()> {
    let trust = default_trust()?;
    let evaluation = trust.evaluate(project);
    if evaluation.actual_hash != reviewed_hash {
        bail!(
            "ProjectConfigTrustHashMismatch: --hash {reviewed_hash} does not match current actual_hash {}; refusing to record trust",
            evaluation.actual_hash
        );
    }

    let record = trust
        .record_trust(&evaluation)
        .context("ProjectConfigTrustAcceptFailed")?;
    println!("ProjectConfigTrustAccepted");
    println!("status: Trusted");
    println!("project_key: {}", record.project_key);
    println!("trusted_hash: {}", record.trusted_hash);
    println!("trusted_at: {}", record.trusted_at);
    println!("writer: {}", record.writer);
    println!("store: {}", trust.store_path().display());
    Ok(())
}

fn revoke(project: &Path) -> anyhow::Result<()> {
    let trust = default_trust()?;
    let evaluation = trust.evaluate(project);
    let project_key = evaluation.project_key.clone().ok_or_else(|| {
        anyhow!("ProjectConfigTrustRevokeUnavailable: cannot determine canonical project key")
    })?;
    let store_path = trust.store_path().to_path_buf();
    let Some(mut store) = read_cli_store(&store_path)? else {
        println!("ProjectConfigTrustRevoked");
        println!("removed: false");
        println!("project_key: {project_key}");
        println!("store: {}", store_path.display());
        return Ok(());
    };

    let removed = store.records.remove(&project_key).is_some();
    if removed {
        write_cli_store(&store_path, &store)?;
    }

    println!("ProjectConfigTrustRevoked");
    println!("removed: {removed}");
    println!("project_key: {project_key}");
    println!("store: {}", store_path.display());
    Ok(())
}

fn read_cli_store(path: &Path) -> anyhow::Result<Option<CliTrustStore>> {
    if !path.exists() {
        return Ok(None);
    }
    let json = fs::read_to_string(path)
        .with_context(|| format!("ProjectConfigTrustStoreReadFailed: {}", path.display()))?;
    let store: CliTrustStore = serde_json::from_str(&json)
        .with_context(|| format!("ProjectConfigTrustStoreCorrupt: {}", path.display()))?;
    if store.schema_version != 1 {
        bail!(
            "ProjectConfigTrustStoreUnsupported: schema version {} is not supported",
            store.schema_version
        );
    }
    Ok(Some(store))
}

fn write_cli_store(path: &Path, store: &CliTrustStore) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("ProjectConfigTrustStoreCreateFailed: {}", parent.display())
        })?;
    }
    let json =
        serde_json::to_string_pretty(store).context("ProjectConfigTrustStoreSerializeFailed")?;
    crate::idempotency::write_bytes_atomic(path, json.as_bytes())
        .with_context(|| format!("ProjectConfigTrustStoreWriteFailed: {}", path.display()))?;
    Ok(())
}

fn print_evaluation(title: &str, evaluation: &TrustEvaluation, store_path: &Path) {
    println!("{title}");
    println!("status: {}", trust_status_label(&evaluation.status));
    if let TrustStatus::ContentChanged { expected, .. } = &evaluation.status {
        println!("expected_hash: {expected}");
    }
    println!("actual_hash: {}", evaluation.actual_hash);
    match &evaluation.project_key {
        Some(project_key) => println!("project_key: {project_key}"),
        None => println!("project_key: unavailable"),
    }
    println!("store: {}", store_path.display());
    for warning in &evaluation.warnings {
        println!("warning: {}", one_line(warning));
    }
}

fn trust_status_label(status: &TrustStatus) -> &'static str {
    match status {
        TrustStatus::Trusted => "Trusted",
        TrustStatus::Untrusted => "Untrusted",
        TrustStatus::ContentChanged { .. } => "ContentChanged",
        TrustStatus::EnvOverride => "EnvOverride",
    }
}

fn one_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct BlockNewFilesInParent {
        parent: PathBuf,
        #[cfg(unix)]
        previous_mode: u32,
        #[cfg(windows)]
        user: String,
    }

    impl BlockNewFilesInParent {
        fn engage(parent: &Path) -> Self {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let metadata = fs::metadata(parent).expect("parent metadata");
                let previous_mode = metadata.permissions().mode();
                let mut permissions = metadata.permissions();
                permissions.set_mode(0o555);
                fs::set_permissions(parent, permissions).expect("block new files");
                Self {
                    parent: parent.to_path_buf(),
                    previous_mode,
                }
            }
            #[cfg(windows)]
            {
                let user = whoami_user();
                let status = crate::process_util::hidden_command("icacls")
                    .arg(parent)
                    .arg("/deny")
                    .arg(format!("{user}:(WD,AD)"))
                    .output()
                    .expect("icacls deny");
                assert!(
                    status.status.success(),
                    "icacls deny failed: {}",
                    String::from_utf8_lossy(&status.stderr)
                );
                Self {
                    parent: parent.to_path_buf(),
                    user,
                }
            }
        }
    }

    impl Drop for BlockNewFilesInParent {
        fn drop(&mut self) {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(metadata) = fs::metadata(&self.parent) {
                    let mut permissions = metadata.permissions();
                    permissions.set_mode(self.previous_mode);
                    let _ = fs::set_permissions(&self.parent, permissions);
                }
            }
            #[cfg(windows)]
            {
                let _ = crate::process_util::hidden_command("icacls")
                    .arg(&self.parent)
                    .arg("/remove:d")
                    .arg(&self.user)
                    .output();
            }
        }
    }

    #[cfg(windows)]
    fn whoami_user() -> String {
        let output = crate::process_util::hidden_command("whoami")
            .output()
            .expect("whoami");
        assert!(output.status.success(), "whoami failed");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    #[test]
    fn trust_store_atomic() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let path = dir.path().join("edit-safety").join("trust.json");
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(&path, r#"{"schema_version":1,"records":{}}"#).expect("seed previous");

        let record = TrustRecord {
            project_key: "proj-key".to_string(),
            trusted_hash: "cd".repeat(32),
            trusted_at: "2026-09-18T00:00:00Z".to_string(),
            writer: "symforge/test".to_string(),
        };
        let mut store = CliTrustStore::default();
        store
            .records
            .insert(record.project_key.clone(), record.clone());

        {
            let parent = path.parent().expect("parent");
            let _fence = BlockNewFilesInParent::engage(parent);
            write_cli_store(&path, &store)
                .expect_err("replace must not run while new files are blocked");
            let kept = fs::read_to_string(&path).expect("read previous");
            let loaded: CliTrustStore =
                serde_json::from_str(&kept).expect("previous JSON parseable");
            assert!(
                loaded.records.is_empty(),
                "crash before replace must keep the previous store"
            );
            assert_eq!(loaded.schema_version, 1);
        }

        write_cli_store(&path, &store).expect("completed persist");
        let loaded: CliTrustStore =
            serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("json");
        assert_eq!(
            loaded.records.get("proj-key").map(|row| &row.trusted_hash),
            Some(&record.trusted_hash),
            "completed persist is the new store"
        );
    }
}
