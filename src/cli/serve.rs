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

    /// Tool allow-list for this server: comma-separated tool or group names
    /// (`read`, `edit`, `knowledge`, `impact`, `service`). Same as `SYMFORGE_TOOLS`,
    /// which it overrides.
    #[arg(long, value_parser = clap::builder::NonEmptyStringValueParser::new())]
    pub tools: Option<String>,

    /// Serve this repository under ALIAS; repeat for several. Clients pick one
    /// with the `project` parameter. Without it, serve indexes the working
    /// directory's repository as before.
    #[arg(long = "project", value_name = "ALIAS=PATH")]
    pub project: Vec<String>,

    /// Let clients call `index_folder` (refused by default: the repositories
    /// are fixed by the operator). Only with at most one `--project`.
    #[arg(long)]
    pub allow_index_folder: bool,
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
            bound_addr_tx: None,
            // The operator CLI discovers the workspace from its CWD unless
            // `--project` names the repositories.
            workspace_root: None,
            projects: self.project,
            allow_index_folder: self.allow_index_folder,
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
            tools: None,
            project: Vec::new(),
            allow_index_folder: false,
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
    fn serve_tools_flag_parses_and_rejects_an_empty_value() {
        use clap::Parser;
        let cli = crate::cli::Cli::try_parse_from(["symforge", "serve", "--tools", "read,status"])
            .expect("--tools parses");
        match cli.command {
            Some(crate::cli::Commands::Serve(args)) => {
                assert_eq!(args.tools.as_deref(), Some("read,status"));
            }
            _ => panic!("expected the serve subcommand"),
        }
        assert!(crate::cli::Cli::try_parse_from(["symforge", "serve", "--tools", ""]).is_err());
    }

    #[test]
    fn serve_project_flags_parse_repeatedly() {
        use clap::Parser;
        let cli = crate::cli::Cli::try_parse_from([
            "symforge",
            "serve",
            "--project",
            r"a=C:\repos\a",
            "--project",
            "b=/srv/b=1",
            "--allow-index-folder",
        ])
        .expect("--project parses");
        match cli.command {
            Some(crate::cli::Commands::Serve(args)) => {
                let args = args.into_serve_args();
                assert_eq!(args.projects, vec![r"a=C:\repos\a", "b=/srv/b=1"]);
                assert!(args.allow_index_folder);
            }
            _ => panic!("expected the serve subcommand"),
        }
    }

    #[test]
    fn into_serve_args_defaults_listen_without_explicit_flag() {
        let cli = ServeCliArgs {
            listen: None,
            api_key: None,
            api_key_env: None,
            tools: None,
            project: Vec::new(),
            allow_index_folder: false,
        };
        let args = cli.into_serve_args();
        assert_eq!(args.listen, DEFAULT_LISTEN);
        assert!(
            !args.explicit_listen,
            "omitting --listen enables the US1 free-port fallback"
        );
    }
}
