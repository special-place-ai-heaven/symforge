//! Pure-Rust Python 3.14-subset Code Mode sandbox (`monty` + `monty-types`).
//!
//! Exposes read-only repository intelligence queries over `CodeReadBackend`,
//! compiling cleanly behind `feature = "server"` with zero C/C++ or JS/V8
//! dependencies and zero filesystem/network egress.

pub mod backend;
pub mod catalog;
pub mod runner;
pub mod value;

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub use backend::{
    CodeBackendError, CodeReadBackend, DaemonCodeBackend, DaemonCodeQueryRequest,
    DaemonCodeQueryResponse, LocalCodeBackend, execute_op_on_generation,
};
pub use catalog::{CODE_READ_OPS, CodeReadOp, render_idioms, render_stubs};
pub use runner::{CodeModeRunner, execute_script};

/// Default maximum total interpreter CPU/wall feed time per script execution (10s).
pub const DEFAULT_MAX_FEED_DURATION: Duration = Duration::from_secs(10);
/// Hard ceiling on interpreter feed duration (30s).
pub const MAX_FEED_DURATION_CEILING: Duration = Duration::from_secs(30);
/// Default interpreter memory cap (64 MiB).
pub const DEFAULT_MAX_MEMORY_BYTES: usize = 64 * 1024 * 1024;
/// Hard ceiling on interpreter memory cap (256 MiB).
pub const MAX_MEMORY_BYTES_CEILING: usize = 256 * 1024 * 1024;
/// Default recursion depth cap (safe on standard OS worker stacks without `stacker`).
pub const DEFAULT_MAX_RECURSION_DEPTH: usize = 150;
/// Hard ceiling on recursion depth.
pub const MAX_RECURSION_DEPTH_CEILING: usize = 200;
/// Default maximum host query invocations per script execution.
pub const DEFAULT_MAX_HOST_CALLS: usize = 64;
/// Hard ceiling on host query invocations per script execution.
pub const MAX_HOST_CALLS_CEILING: usize = 256;
/// Default maximum inline UTF-8 characters before spilling full output to disk.
pub const DEFAULT_MAX_OUTPUT_CHARS: usize = 16_000;
/// Hard ceiling on inline UTF-8 characters (256 Ki chars).
pub const MAX_OUTPUT_CHARS_CEILING: usize = 256 * 1024;

/// Resource limits enforced on a single stateless `CodeModeRunner` invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeModeLimits {
    pub max_feed_duration: Duration,
    pub max_memory_bytes: usize,
    pub max_recursion_depth: usize,
    pub max_host_calls: usize,
    pub max_output_chars: usize,
    /// Whether oversized outputs may spill full JSON payloads to local temp files.
    /// True for CLI and local stdio MCP; false on shared/HTTP MCP transports.
    pub allow_spill_files: bool,
}

impl Default for CodeModeLimits {
    fn default() -> Self {
        Self {
            max_feed_duration: DEFAULT_MAX_FEED_DURATION,
            max_memory_bytes: DEFAULT_MAX_MEMORY_BYTES,
            max_recursion_depth: DEFAULT_MAX_RECURSION_DEPTH,
            max_host_calls: DEFAULT_MAX_HOST_CALLS,
            max_output_chars: DEFAULT_MAX_OUTPUT_CHARS,
            allow_spill_files: true,
        }
    }
}

impl CodeModeLimits {
    /// Clamps caller-supplied overrides to hard ceilings and non-zero floors.
    #[must_use]
    pub fn clamped(self) -> Self {
        Self {
            max_feed_duration: self
                .max_feed_duration
                .clamp(Duration::from_millis(1), MAX_FEED_DURATION_CEILING),
            max_memory_bytes: self
                .max_memory_bytes
                .clamp(64 * 1024, MAX_MEMORY_BYTES_CEILING),
            max_recursion_depth: self
                .max_recursion_depth
                .clamp(8, MAX_RECURSION_DEPTH_CEILING),
            max_host_calls: self.max_host_calls.clamp(1, MAX_HOST_CALLS_CEILING),
            max_output_chars: self.max_output_chars.clamp(256, MAX_OUTPUT_CHARS_CEILING),
            allow_spill_files: self.allow_spill_files,
        }
    }
}

/// Input to a stateless script execution.
#[derive(Debug, Clone)]
pub struct ExecuteRequest {
    pub code: String,
    pub limits: CodeModeLimits,
    /// Optional custom spill directory (defaults to `std::env::temp_dir()`).
    pub spill_dir: Option<PathBuf>,
}

impl ExecuteRequest {
    #[must_use]
    pub fn new(code: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            limits: CodeModeLimits::default(),
            spill_dir: None,
        }
    }
}

/// Execution status tag in the JSON envelope returned by `symforge code run` and `code_execute`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteStatus {
    Ok,
    Error,
}

/// Structured error details when a script fails at syntax, runtime, or resource-limit enforcement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecuteErrorInfo {
    pub kind: String,
    pub exception_type: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traceback: Option<String>,
}

/// Structured JSON outcome of one `CodeModeRunner` execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecuteOutcome {
    pub status: ExecuteStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    pub stdout: String,
    pub calls: usize,
    pub duration_ms: u64,
    pub output_bytes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spill_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ExecuteErrorInfo>,
}
