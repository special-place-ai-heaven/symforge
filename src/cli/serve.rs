//! `symforge serve` CLI subcommand — operator server flags.
//!
//! Defines the clap arg surface for `serve` and converts it into the
//! transport-agnostic [`crate::server::serve::ServeArgs`]. The async entrypoint
//! ([`crate::server::serve::run`]) is driven from `main.rs` on a multi-thread
//! tokio runtime (mirroring `run_daemon` / `run_mcp_server`).

use clap::Args;

use crate::server::serve::{DEFAULT_LISTEN, ServeArgs};

/// Flags for `symforge serve` (see `contracts/cli-serve.md`).
#[derive(Args, Debug, Clone)]
pub struct ServeCliArgs {
    /// Bind address `HOST:PORT` (default `127.0.0.1:8787`; `PORT=0` → OS-assigned).
    #[arg(long, default_value = DEFAULT_LISTEN)]
    pub listen: String,

    /// Single static Bearer key (inline). Prefer `--api-key-env` for secrecy.
    #[arg(long)]
    pub api_key: Option<String>,

    /// Name of an environment variable holding the Bearer key.
    #[arg(long)]
    pub api_key_env: Option<String>,
}

impl ServeCliArgs {
    /// Convert CLI flags into the transport-agnostic [`ServeArgs`].
    pub fn into_serve_args(self) -> ServeArgs {
        ServeArgs {
            listen: self.listen,
            api_key: self.api_key,
            api_key_env: self.api_key_env,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn into_serve_args_carries_flags() {
        let cli = ServeCliArgs {
            listen: "0.0.0.0:9000".to_string(),
            api_key: Some("k".to_string()),
            api_key_env: None,
        };
        let args = cli.into_serve_args();
        assert_eq!(args.listen, "0.0.0.0:9000");
        assert_eq!(args.api_key.as_deref(), Some("k"));
        assert_eq!(args.api_key_env, None);
    }
}
