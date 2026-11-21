//! Post-MCP shell CLI subcommands for Code Mode (`symforge code` and `symforge q`).
//!
//! Designed for shell-capable coding agents (Claude Code, Codex, OMP, OpenCode,
//! Rerun coworkers):
//! - Zero upfront MCP context tokens.
//! - Pure JSON stdout on `run` and `q` (zero stderr noise on success).
//! - Attaches to a warm local SymForge daemon when running (unless
//!   `SYMFORGE_NO_DAEMON=1`), and falls back cleanly to read-only local
//!   `.symforge/index.bin` snapshot load without spawning a background daemon.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::{Args, Subcommand};
use serde_json::{Map, Value, json};

use crate::codemode::{
    CodeModeLimits, CodeReadBackend, CodeReadOp, DaemonCodeBackend, ExecuteRequest,
    LocalCodeBackend, execute_script, render_idioms, render_stubs,
};

/// Arguments for `symforge code <subcommand>`.
#[derive(Args, Debug, Clone)]
pub struct CodeCliArgs {
    #[command(subcommand)]
    pub command: CodeSubcommand,
}

#[derive(Subcommand, Debug, Clone)]
pub enum CodeSubcommand {
    /// Execute a Python 3.14-subset script inside the zero-egress Code Mode sandbox
    Run(CodeRunArgs),
    /// Render keyword-only Python function stubs and TypedDict return types
    Stubs(CodeStubsArgs),
    /// Print the self-contained Code Mode idioms guide and worked examples
    Idioms,
}

#[derive(Args, Debug, Clone)]
pub struct CodeRunArgs {
    /// Script file path, or `-` to read Python source from stdin
    pub file: Option<String>,

    /// Inline Python script to execute (alternative to FILE or `-`)
    #[arg(short = 'c', long = "code")]
    pub code: Option<String>,

    /// Explicit repository root path (defaults to SYMFORGE_WORKSPACE_ROOT or CWD walk)
    #[arg(long)]
    pub root: Option<PathBuf>,

    /// Open-project ID or alias when routing over a multi-project daemon session
    #[arg(long)]
    pub project: Option<String>,

    /// Override maximum interpreter feed budget in milliseconds (clamped to 1..=30000)
    #[arg(long)]
    pub timeout_ms: Option<u64>,

    /// Override maximum host query calls allowed (clamped to 1..=256)
    #[arg(long)]
    pub max_calls: Option<usize>,

    /// Override maximum inline output characters before lossless spill file (min 256)
    #[arg(long)]
    pub max_output_chars: Option<usize>,
}

#[derive(Args, Debug, Clone)]
pub struct CodeStubsArgs {
    /// Optional search topic or function name (omit for compact 1-line index of all ops)
    pub query: Option<String>,

    /// Maximum matching stubs to render when a query is provided (default 5)
    #[arg(long)]
    pub limit: Option<usize>,
}

/// Maximum UTF-8 bytes accepted from stdin or script/args files (2 MiB).
const MAX_CLI_INPUT_BYTES: usize = 2 * 1024 * 1024;

/// Arguments for `symforge q <op> [json_args]` (1-call fast path, alias `query`).
#[derive(Args, Debug, Clone)]
pub struct QueryCliArgs {
    /// Read operation name (e.g. `search_symbols`, `find_references`, `get_symbol`)
    pub op: String,

    /// Optional JSON object of keyword arguments (defaults to `{}`; `-` reads stdin)
    pub args_json: Option<String>,

    /// Quote-free `KEY=VALUE` argument (repeatable; parses JSON scalars/arrays/objects or falls back to string)
    #[arg(short = 'a', long = "arg", value_name = "KEY=VALUE")]
    pub arg: Vec<String>,

    /// Read JSON object arguments from a UTF-8 file (exclusive with positional `ARGS_JSON`)
    #[arg(long = "args-file", value_name = "FILE")]
    pub args_file: Option<PathBuf>,

    /// Explicit repository root path (defaults to SYMFORGE_WORKSPACE_ROOT or CWD walk)
    #[arg(long)]
    pub root: Option<PathBuf>,

    /// Open-project ID or alias when routing over a multi-project daemon session
    #[arg(long)]
    pub project: Option<String>,
}

pub fn run_code_cli(args: CodeCliArgs) -> anyhow::Result<()> {
    match args.command {
        CodeSubcommand::Idioms => {
            print!("{}", render_idioms());
            Ok(())
        }
        CodeSubcommand::Stubs(stubs) => {
            print!("{}", render_stubs(stubs.query.as_deref(), stubs.limit));
            Ok(())
        }
        CodeSubcommand::Run(run_args) => run_code_script(run_args),
    }
}

fn read_bounded_utf8(mut reader: impl Read, label: &str) -> anyhow::Result<String> {
    let cap = (MAX_CLI_INPUT_BYTES + 1) as u64;
    let mut bytes = Vec::new();
    (&mut reader).take(cap).read_to_end(&mut bytes)?;
    if bytes.len() > MAX_CLI_INPUT_BYTES {
        anyhow::bail!(
            "{label} exceeds maximum allowed size of {} bytes (2 MiB)",
            MAX_CLI_INPUT_BYTES
        );
    }
    String::from_utf8(bytes).map_err(|e| anyhow::anyhow!("{label} is not valid UTF-8: {e}"))
}

fn read_bounded_file(path: &Path, label: &str) -> anyhow::Result<String> {
    let file = std::fs::File::open(path)?;
    read_bounded_utf8(file, label)
}

fn read_script_source(args: &CodeRunArgs) -> anyhow::Result<String> {
    match (&args.code, &args.file) {
        (Some(_), Some(_)) => {
            anyhow::bail!("pass either `-c <CODE>` or a script `FILE` / `-`, not both")
        }
        (Some(inline), None) => {
            if inline.len() > MAX_CLI_INPUT_BYTES {
                anyhow::bail!(
                    "inline script exceeds maximum allowed size of {} bytes (2 MiB)",
                    MAX_CLI_INPUT_BYTES
                );
            }
            Ok(inline.clone())
        }
        (None, Some(path)) if path == "-" => read_bounded_utf8(std::io::stdin(), "stdin script"),
        (None, Some(path)) => read_bounded_file(Path::new(path), "script file"),
        (None, None) => {
            anyhow::bail!(
                "no script provided: pass `-c \"...\"`, a script path, or `-` for stdin (try `symforge code idioms`)"
            )
        }
    }
}

fn resolve_target_root(explicit_root: Option<&Path>) -> anyhow::Result<PathBuf> {
    if let Some(root) = explicit_root {
        return Ok(root.canonicalize()?);
    }
    if let Some(found) = crate::discovery::find_project_root() {
        return Ok(found);
    }
    Ok(std::env::current_dir()?.canonicalize()?)
}

fn no_daemon_requested() -> bool {
    std::env::var("SYMFORGE_NO_DAEMON")
        .map(|v| {
            let t = v.trim();
            !t.is_empty() && t != "0" && !t.eq_ignore_ascii_case("false")
        })
        .unwrap_or(false)
}

fn run_with_resolved_backend<T>(
    root: &Path,
    project: Option<String>,
    work: impl FnOnce(&dyn CodeReadBackend, &'static str) -> T,
) -> anyhow::Result<T> {
    if !no_daemon_requested() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        let attached = rt.block_on(crate::daemon::connect_existing_session(
            root,
            "symforge-code-cli",
            Some(std::process::id()),
        ));
        if let Ok(Some(session)) = attached {
            let daemon_backend =
                DaemonCodeBackend::new_ephemeral(rt.handle().clone(), session, project);
            let out = work(&daemon_backend, "daemon");
            daemon_backend.close_best_effort();
            return Ok(out);
        }
    }

    let local = LocalCodeBackend::open_read_only(root)?;
    Ok(work(&local, "local"))
}

fn run_code_script(args: CodeRunArgs) -> anyhow::Result<()> {
    let source = read_script_source(&args)?;
    let root = resolve_target_root(args.root.as_deref())?;

    let mut limits = CodeModeLimits::default();
    if let Some(ms) = args.timeout_ms {
        limits.max_feed_duration = Duration::from_millis(ms);
    }
    if let Some(calls) = args.max_calls {
        limits.max_host_calls = calls;
    }
    if let Some(chars) = args.max_output_chars {
        limits.max_output_chars = chars;
    }
    let req = ExecuteRequest {
        code: source,
        limits: limits.clamped(),
        spill_dir: None,
    };

    let outcome = run_with_resolved_backend(&root, args.project, |backend, _lane| {
        execute_script(&req, backend)
    })?;

    println!("{}", serde_json::to_string(&outcome)?);
    Ok(())
}

pub fn run_query_cli(args: QueryCliArgs) -> anyhow::Result<()> {
    let Some(op) = CodeReadOp::from_cli_name(args.op.trim()) else {
        let valid: Vec<&str> = crate::codemode::CODE_READ_OPS
            .iter()
            .map(|d| d.name)
            .collect();
        let err_json = json!({
            "status": "error",
            "error": {
                "kind": "unknown_operation",
                "message": format!(
                    "unknown read operation '{}'; valid operations: {}",
                    args.op,
                    valid.join(", ")
                ),
            }
        });
        println!("{}", serde_json::to_string(&err_json)?);
        return Ok(());
    };

    if args.args_file.is_some() && args.args_json.is_some() {
        let err_json = json!({
            "status": "error",
            "op": op.as_str(),
            "error": {
                "kind": "invalid_request",
                "message": "pass either positional JSON arguments or `--args-file`, not both",
            }
        });
        println!("{}", serde_json::to_string(&err_json)?);
        return Ok(());
    }

    let raw_args = if let Some(file_path) = &args.args_file {
        match read_bounded_file(file_path, "args file") {
            Ok(text) if text.trim().is_empty() => "{}".to_owned(),
            Ok(text) => text,
            Err(e) => {
                let err_json = json!({
                    "status": "error",
                    "op": op.as_str(),
                    "error": {
                        "kind": "invalid_request",
                        "message": format!("cannot read --args-file '{}': {e}", file_path.display()),
                    }
                });
                println!("{}", serde_json::to_string(&err_json)?);
                return Ok(());
            }
        }
    } else {
        match args.args_json.as_deref() {
            None | Some("") => "{}".to_owned(),
            Some("-") => match read_bounded_utf8(std::io::stdin(), "stdin arguments") {
                Ok(buf) if buf.trim().is_empty() => "{}".to_owned(),
                Ok(buf) => buf,
                Err(e) => {
                    let err_json = json!({
                        "status": "error",
                        "op": op.as_str(),
                        "error": {
                            "kind": "invalid_request",
                            "message": e.to_string(),
                        }
                    });
                    println!("{}", serde_json::to_string(&err_json)?);
                    return Ok(());
                }
            },
            Some(inline) => {
                if inline.len() > MAX_CLI_INPUT_BYTES {
                    let err_json = json!({
                        "status": "error",
                        "op": op.as_str(),
                        "error": {
                            "kind": "invalid_request",
                            "message": format!(
                                "inline arguments exceed maximum allowed size of {} bytes (2 MiB)",
                                MAX_CLI_INPUT_BYTES
                            ),
                        }
                    });
                    println!("{}", serde_json::to_string(&err_json)?);
                    return Ok(());
                }
                inline.to_owned()
            }
        }
    };

    let parsed: Value = match serde_json::from_str(&raw_args) {
        Ok(v) => v,
        Err(e) => {
            let err_json = json!({
                "status": "error",
                "op": op.as_str(),
                "error": {
                    "kind": "invalid_json",
                    "message": format!("invalid JSON arguments: {e}"),
                }
            });
            println!("{}", serde_json::to_string(&err_json)?);
            return Ok(());
        }
    };

    let Value::Object(mut kwargs): Value = parsed else {
        let err_json = json!({
            "status": "error",
            "op": op.as_str(),
            "error": {
                "kind": "invalid_json",
                "message": "arguments must be a JSON object (e.g. '{\"query\":\"foo\"}' or `-a query=foo`)",
            }
        });
        println!("{}", serde_json::to_string(&err_json)?);
        return Ok(());
    };

    for entry in &args.arg {
        let Some((raw_key, raw_val)) = entry.split_once('=') else {
            let err_json = json!({
                "status": "error",
                "op": op.as_str(),
                "error": {
                    "kind": "invalid_request",
                    "message": format!("invalid `--arg` '{entry}': expected `KEY=VALUE`"),
                }
            });
            println!("{}", serde_json::to_string(&err_json)?);
            return Ok(());
        };
        let key = raw_key.trim();
        if key.is_empty() {
            let err_json = json!({
                "status": "error",
                "op": op.as_str(),
                "error": {
                    "kind": "invalid_request",
                    "message": format!("invalid `--arg` '{entry}': key must be non-empty"),
                }
            });
            println!("{}", serde_json::to_string(&err_json)?);
            return Ok(());
        }
        let val = serde_json::from_str::<Value>(raw_val)
            .unwrap_or_else(|_| Value::String(raw_val.to_owned()));
        kwargs.insert(key.to_owned(), val);
    }

    let root = resolve_target_root(args.root.as_deref())?;
    let started = std::time::Instant::now();
    let result = run_with_resolved_backend(&root, args.project, |backend, _lane| {
        backend.execute_op(op, kwargs as Map<String, Value>)
    })?;
    let duration_ms = started.elapsed().as_millis() as u64;

    let payload = match result {
        Ok(value) => json!({
            "status": "ok",
            "op": op.as_str(),
            "value": value,
            "duration_ms": duration_ms,
        }),
        Err(err) => json!({
            "status": "error",
            "op": op.as_str(),
            "duration_ms": duration_ms,
            "error": {
                "kind": err.kind,
                "message": err.message,
            }
        }),
    };
    println!("{}", serde_json::to_string(&payload)?);
    Ok(())
}
