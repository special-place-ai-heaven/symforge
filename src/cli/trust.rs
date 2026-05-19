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
    fs::write(path, json)
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
