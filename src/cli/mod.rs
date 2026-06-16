//! CLI module — clap Parser types and subcommand dispatch for the `symforge` binary.
//!
//! Subcommands:
//!   `symforge init`               — configure Claude, Codex, or both
//!   `symforge analytics <cmd>`     — inspect or reset local analytics storage
//!   `symforge hook <subcommand>`  — hook scripts called by Claude Code
//!   `symforge daemon`             — shared project/session backend
//!   `symforge trust project-config` — audit, accept, or revoke project-config trust
//!   `symforge update`             — update the npm-managed SymForge install
//!
//! Plan 03 wires these into main.rs and handles the top-level dispatch.

pub mod analytics;
pub mod harness;
pub mod harness_apply;
pub mod hook;
pub mod init;
pub mod onboarding;
pub mod serve;
pub mod trust;
pub mod update;
pub mod version;

use clap::{Parser, Subcommand, ValueEnum};

/// Top-level CLI parser for the `symforge` binary.
#[derive(Parser)]
#[command(
    name = "symforge",
    about = "SymForge MCP server and hook system",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Top-level subcommands.
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Inspect, export, or reset local analytics storage
    Analytics {
        #[command(subcommand)]
        command: analytics::AnalyticsCommand,
    },
    /// Install SymForge integration for Claude, Codex, Gemini, Kilo Code, or all
    Init {
        /// Client to configure
        #[arg(long, value_enum, default_value_t = InitClient::All)]
        client: InitClient,

        /// Scan known MCP harness configs and report per-client SymForge attach
        /// status instead of running the stdio install. Non-destructive: with
        /// `--scan` alone (no `--apply`) nothing is written.
        #[arg(long)]
        scan: bool,

        /// With `--scan`, write/refresh the SymForge HTTP attach entry into each
        /// discovered config (backup-then-atomic-write). Requires
        /// `--serve-url`. Without `--apply`, `--scan` is a dry-run preview.
        #[arg(long, requires = "scan")]
        apply: bool,

        /// The `symforge serve` attach URL to write (e.g. `http://127.0.0.1:8787/mcp`).
        #[arg(long, requires = "scan")]
        serve_url: Option<String>,

        /// The Bearer key for the attach URL (omit for a keyless loopback serve).
        #[arg(long, requires = "serve_url")]
        serve_key: Option<String>,
    },
    /// Run the shared local daemon that tracks project and session state
    Daemon,
    /// Serve the MCP surface over Streamable HTTP at `/mcp` (operator server)
    Serve(serve::ServeCliArgs),
    /// Hook subcommands called by Claude Code (PostToolUse / SessionStart / UserPromptSubmit)
    Hook {
        #[command(subcommand)]
        subcommand: Option<HookSubcommand>,
    },
    /// Trust-control commands for project-local SymForge configuration
    Trust {
        #[command(subcommand)]
        subcommand: trust::TrustSubcommand,
    },
    /// Update the npm-managed SymForge install
    Update,
}

/// Supported `symforge init` targets.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum InitClient {
    Claude,
    #[value(name = "claude-desktop", alias = "desktop")]
    ClaudeDesktop,
    Codex,
    Gemini,
    #[value(name = "kilo-code", alias = "kilo")]
    KiloCode,
    All,
}

/// Hook subcommands — one per Claude Code tool event type.
#[derive(Subcommand, Debug, Clone)]
pub enum HookSubcommand {
    /// PostToolUse hook for the Read tool — returns outline for the read file
    Read,
    /// PostToolUse hook for Edit/Write tools — returns impact (dependents) for the edited file
    Edit,
    /// PostToolUse hook for the Write tool — confirms indexing of new file
    Write,
    /// PostToolUse hook for the Grep tool — returns symbol-context for the search query
    Grep,
    /// SessionStart hook — returns repo map for the project
    #[command(name = "session-start")]
    SessionStart,
    /// UserPromptSubmit hook — injects targeted context from file/symbol hints in the prompt
    #[command(name = "prompt-submit")]
    PromptSubmit,
    /// PreToolUse hook — suggests SymForge alternatives before built-in tools execute
    #[command(name = "pre-tool")]
    PreTool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_defaults_to_all_clients() {
        let cli = Cli::parse_from(["symforge", "init"]);

        match cli.command {
            Some(Commands::Init { client, .. }) => assert_eq!(client, InitClient::All),
            _ => panic!("expected init command"),
        }
    }

    #[test]
    fn test_init_accepts_codex_client() {
        let cli = Cli::parse_from(["symforge", "init", "--client", "codex"]);

        match cli.command {
            Some(Commands::Init { client, .. }) => assert_eq!(client, InitClient::Codex),
            _ => panic!("expected init command"),
        }
    }

    #[test]
    fn test_init_accepts_gemini_client() {
        let cli = Cli::parse_from(["symforge", "init", "--client", "gemini"]);

        match cli.command {
            Some(Commands::Init { client, .. }) => assert_eq!(client, InitClient::Gemini),
            _ => panic!("expected init command"),
        }
    }

    #[test]
    fn test_init_accepts_kilo_code_client() {
        let cli = Cli::parse_from(["symforge", "init", "--client", "kilo-code"]);

        match cli.command {
            Some(Commands::Init { client, .. }) => assert_eq!(client, InitClient::KiloCode),
            _ => panic!("expected init command"),
        }
    }

    #[test]
    fn test_init_accepts_kilo_alias() {
        let cli = Cli::parse_from(["symforge", "init", "--client", "kilo"]);

        match cli.command {
            Some(Commands::Init { client, .. }) => assert_eq!(client, InitClient::KiloCode),
            _ => panic!("expected init command"),
        }
    }

    #[test]
    fn test_daemon_command_parses() {
        let cli = Cli::parse_from(["symforge", "daemon"]);

        match cli.command {
            Some(Commands::Daemon) => {}
            _ => panic!("expected daemon command"),
        }
    }

    #[test]
    fn test_serve_command_parses_with_defaults() {
        let cli = Cli::parse_from(["symforge", "serve"]);

        match cli.command {
            Some(Commands::Serve(args)) => {
                assert_eq!(args.listen, "127.0.0.1:8787");
                assert_eq!(args.api_key, None);
                assert_eq!(args.api_key_env, None);
            }
            _ => panic!("expected serve command"),
        }
    }

    #[test]
    fn test_serve_command_parses_flags() {
        let cli = Cli::parse_from([
            "symforge",
            "serve",
            "--listen",
            "0.0.0.0:9000",
            "--api-key-env",
            "SYMFORGE_KEY",
        ]);

        match cli.command {
            Some(Commands::Serve(args)) => {
                assert_eq!(args.listen, "0.0.0.0:9000");
                assert_eq!(args.api_key, None);
                assert_eq!(args.api_key_env.as_deref(), Some("SYMFORGE_KEY"));
            }
            _ => panic!("expected serve command"),
        }
    }

    #[test]
    fn test_analytics_status_command_parses() {
        let cli = Cli::parse_from([
            "symforge",
            "analytics",
            "status",
            "--db-path",
            ".symforge/analytics.db",
        ]);

        match cli.command {
            Some(Commands::Analytics {
                command:
                    analytics::AnalyticsCommand::Status {
                        db_path: Some(db_path),
                    },
            }) => assert_eq!(db_path, std::path::PathBuf::from(".symforge/analytics.db")),
            _ => panic!("expected analytics status command"),
        }
    }

    #[test]
    fn test_hook_prompt_submit_command_parses() {
        let cli = Cli::parse_from(["symforge", "hook", "prompt-submit"]);

        match cli.command {
            Some(Commands::Hook {
                subcommand: Some(HookSubcommand::PromptSubmit),
            }) => {}
            _ => panic!("expected prompt-submit hook command"),
        }
    }

    #[test]
    fn test_hook_pre_tool_command_parses() {
        let cli = Cli::parse_from(["symforge", "hook", "pre-tool"]);

        match cli.command {
            Some(Commands::Hook {
                subcommand: Some(HookSubcommand::PreTool),
            }) => {}
            _ => panic!("expected pre-tool hook command"),
        }
    }

    #[test]
    fn test_trust_project_config_status_command_parses() {
        let cli = Cli::parse_from([
            "symforge",
            "trust",
            "project-config",
            "status",
            "--project",
            ".",
        ]);

        match cli.command {
            Some(Commands::Trust {
                subcommand:
                    trust::TrustSubcommand::ProjectConfig {
                        command: trust::ProjectConfigCommand::Status { project },
                    },
            }) => assert_eq!(project, std::path::PathBuf::from(".")),
            _ => panic!("expected trust project-config status command"),
        }
    }

    #[test]
    fn test_trust_project_config_accept_command_parses() {
        let hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let cli = Cli::parse_from([
            "symforge",
            "trust",
            "project-config",
            "accept",
            "--project",
            ".",
            "--hash",
            hash,
        ]);

        match cli.command {
            Some(Commands::Trust {
                subcommand:
                    trust::TrustSubcommand::ProjectConfig {
                        command:
                            trust::ProjectConfigCommand::Accept {
                                project,
                                hash: parsed_hash,
                            },
                    },
            }) => {
                assert_eq!(project, std::path::PathBuf::from("."));
                assert_eq!(parsed_hash, hash);
            }
            _ => panic!("expected trust project-config accept command"),
        }
    }

    #[test]
    fn test_update_command_parses() {
        let cli = Cli::parse_from(["symforge", "update"]);

        match cli.command {
            Some(Commands::Update) => {}
            _ => panic!("expected update command"),
        }
    }
}
