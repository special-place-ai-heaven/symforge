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
    ///
    /// Omitting `--listen` selects the default and enables the US1 free-port
    /// fallback: if `127.0.0.1:8787` is occupied, serve binds an OS-assigned
    /// free port instead of failing. Passing `--listen` explicitly honors the
    /// address exactly — an occupied port fails loudly (no substitution).
    #[arg(long)]
    pub listen: Option<String>,

    /// Single static Bearer key (inline). Prefer `--api-key-env` for secrecy.
    #[arg(long)]
    pub api_key: Option<String>,

    /// Name of an environment variable holding the Bearer key.
    #[arg(long)]
    pub api_key_env: Option<String>,
}

impl ServeCliArgs {
    /// Convert CLI flags into the transport-agnostic [`ServeArgs`].
    ///
    /// An explicit `--listen` sets `explicit_listen = true` (honor exactly, fail
    /// loudly if occupied); omitting it uses [`DEFAULT_LISTEN`] with the US1
    /// free-port fallback (`explicit_listen = false`).
    pub fn into_serve_args(self) -> ServeArgs {
        let explicit_listen = self.listen.is_some();
        ServeArgs {
            listen: self.listen.unwrap_or_else(|| DEFAULT_LISTEN.to_string()),
            explicit_listen,
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
            listen: Some("0.0.0.0:9000".to_string()),
            api_key: Some("k".to_string()),
            api_key_env: None,
        };
        let args = cli.into_serve_args();
        assert_eq!(args.listen, "0.0.0.0:9000");
        assert!(
            args.explicit_listen,
            "an explicit --listen is honored exactly"
        );
        assert_eq!(args.api_key.as_deref(), Some("k"));
        assert_eq!(args.api_key_env, None);
    }

    #[test]
    fn into_serve_args_defaults_listen_without_explicit_flag() {
        let cli = ServeCliArgs {
            listen: None,
            api_key: None,
            api_key_env: None,
        };
        let args = cli.into_serve_args();
        assert_eq!(args.listen, DEFAULT_LISTEN);
        assert!(
            !args.explicit_listen,
            "omitting --listen enables the US1 free-port fallback"
        );
    }
}
