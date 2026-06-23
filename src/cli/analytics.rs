use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use serde::Serialize;

use crate::analytics::{
    AnalyticsSummary, DEFAULT_ANALYTICS_EXPORT_LIMIT, MAX_ANALYTICS_EXPORT_LIMIT,
    SqliteAnalyticsStore, StoredAnalyticsRecord,
};

const DEFAULT_ANALYTICS_SUMMARY_GROUPS: usize = 20;

#[derive(Subcommand, Debug, Clone)]
pub enum AnalyticsCommand {
    /// Show whether local analytics storage exists and can be read
    Status {
        /// Path to the local analytics SQLite database
        #[arg(long)]
        db_path: Option<PathBuf>,
    },
    /// Summarize local analytics records without exporting event rows
    Summary {
        /// Path to the local analytics SQLite database
        #[arg(long)]
        db_path: Option<PathBuf>,
    },
    /// Export recent local analytics rows as bounded redacted JSON
    Export {
        /// Path to the local analytics SQLite database
        #[arg(long)]
        db_path: Option<PathBuf>,
        /// Maximum records to export, capped by SymForge
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Delete only the local analytics database and SQLite sidecar files
    Reset {
        /// Path to the local analytics SQLite database
        #[arg(long)]
        db_path: Option<PathBuf>,
    },
}

#[derive(Serialize)]
struct AnalyticsStatusOutput {
    mode: &'static str,
    db_path: String,
    db_exists: bool,
    schema_version: Option<u32>,
    total_records: Option<u64>,
}

#[derive(Serialize)]
struct AnalyticsSummaryOutput {
    mode: &'static str,
    db_path: String,
    db_exists: bool,
    summary: Option<AnalyticsSummary>,
}

#[derive(Serialize)]
struct AnalyticsExportOutput {
    mode: &'static str,
    db_path: String,
    db_exists: bool,
    limit: usize,
    records: Vec<StoredAnalyticsRecord>,
}

#[derive(Serialize)]
struct AnalyticsResetOutput {
    mode: &'static str,
    db_path: String,
    removed: Vec<String>,
    missing: Vec<String>,
}

pub fn run_analytics(command: &AnalyticsCommand) -> Result<()> {
    match command {
        AnalyticsCommand::Status { db_path } => {
            let db_path = resolve_db_path(db_path)?;
            print_json(&status_output(db_path)?)
        }
        AnalyticsCommand::Summary { db_path } => {
            let db_path = resolve_db_path(db_path)?;
            print_json(&summary_output(db_path)?)
        }
        AnalyticsCommand::Export { db_path, limit } => {
            let db_path = resolve_db_path(db_path)?;
            let limit = limit.unwrap_or(DEFAULT_ANALYTICS_EXPORT_LIMIT);
            print_json(&export_output(db_path, limit)?)
        }
        AnalyticsCommand::Reset { db_path } => {
            let db_path = resolve_db_path(db_path)?;
            print_json(&reset_output(db_path)?)
        }
    }
}

fn resolve_db_path(db_path: &Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = db_path {
        return Ok(path.clone());
    }

    let root = match crate::discovery::find_project_root() {
        Some(root) => root,
        None => {
            std::env::current_dir().context("resolving current directory for analytics path")?
        }
    };
    Ok(crate::paths::symforge_db_path(
        &root,
        crate::paths::ANALYTICS_DB_NAME,
    ))
}

fn status_output(db_path: PathBuf) -> Result<AnalyticsStatusOutput> {
    if !db_path.exists() {
        return Ok(AnalyticsStatusOutput {
            mode: "disabled",
            db_path: display_path(&db_path),
            db_exists: false,
            schema_version: None,
            total_records: None,
        });
    }

    let store = SqliteAnalyticsStore::open(&db_path)?;
    let summary = store.summary(DEFAULT_ANALYTICS_SUMMARY_GROUPS)?;
    Ok(AnalyticsStatusOutput {
        mode: "enabled",
        db_path: display_path(&db_path),
        db_exists: true,
        schema_version: Some(store.schema_version()?),
        total_records: Some(summary.total_records),
    })
}

fn summary_output(db_path: PathBuf) -> Result<AnalyticsSummaryOutput> {
    if !db_path.exists() {
        return Ok(AnalyticsSummaryOutput {
            mode: "disabled",
            db_path: display_path(&db_path),
            db_exists: false,
            summary: None,
        });
    }

    let store = SqliteAnalyticsStore::open(&db_path)?;
    Ok(AnalyticsSummaryOutput {
        mode: "enabled",
        db_path: display_path(&db_path),
        db_exists: true,
        summary: Some(store.summary(DEFAULT_ANALYTICS_SUMMARY_GROUPS)?),
    })
}

fn export_output(db_path: PathBuf, limit: usize) -> Result<AnalyticsExportOutput> {
    let limit = limit.min(MAX_ANALYTICS_EXPORT_LIMIT);
    if !db_path.exists() {
        return Ok(AnalyticsExportOutput {
            mode: "disabled",
            db_path: display_path(&db_path),
            db_exists: false,
            limit,
            records: Vec::new(),
        });
    }

    let store = SqliteAnalyticsStore::open(&db_path)?;
    Ok(AnalyticsExportOutput {
        mode: "enabled",
        db_path: display_path(&db_path),
        db_exists: true,
        limit,
        records: store.export_records(limit)?,
    })
}

fn reset_output(db_path: PathBuf) -> Result<AnalyticsResetOutput> {
    ensure_reset_target(&db_path)?;

    let mut removed = Vec::new();
    let mut missing = Vec::new();
    for path in analytics_storage_paths(&db_path) {
        if path.exists() {
            if path.is_dir() {
                bail!(
                    "analytics reset refuses to remove directory {}",
                    path.display()
                );
            }
            std::fs::remove_file(&path)
                .with_context(|| format!("removing analytics storage {}", path.display()))?;
            removed.push(display_path(&path));
        } else {
            missing.push(display_path(&path));
        }
    }

    Ok(AnalyticsResetOutput {
        mode: "reset",
        db_path: display_path(&db_path),
        removed,
        missing,
    })
}

fn ensure_reset_target(db_path: &Path) -> Result<()> {
    if db_path.file_name().and_then(|name| name.to_str()) == Some("analytics.db") {
        return Ok(());
    }

    bail!(
        "analytics reset refuses to delete non-analytics file {}",
        db_path.display()
    )
}

fn analytics_storage_paths(db_path: &Path) -> Vec<PathBuf> {
    vec![
        db_path.to_path_buf(),
        path_with_suffix(db_path, "-wal"),
        path_with_suffix(db_path, "-shm"),
        path_with_suffix(db_path, "-journal"),
    ]
}

fn path_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    serde_json::to_writer_pretty(&mut lock, value)?;
    writeln!(&mut lock)?;
    Ok(())
}
