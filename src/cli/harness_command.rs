//! Per-harness `symforge-admin` command-file install (009 US3, D2).
//!
//! Installs a `symforge-admin` command file where the harness supports one (e.g.
//! Claude Code `~/.claude/commands/symforge-admin.md`); harnesses without a
//! command-file convention rely on the universal MCP prompt instead (no guessed
//! or broken file — the MCP prompt is the floor). Reuses the restorable-backup
//! write path (FR-016).
//!
//! The universal affordance is the MCP prompt `symforge-admin` (registered in
//! `protocol::prompts`); this module is the *convenience layer* — a one-keystroke
//! slash-command for harnesses that document a command-file format. Across the
//! live [`HarnessId`] set, only **Claude Code** documents one (markdown
//! slash-commands under `~/.claude/commands/`); the rest (Codex, Gemini, Cursor,
//! KiloCode, Claude Desktop) get the MCP prompt only — guessing a format we
//! cannot verify would ship a broken affordance (a nonworking feature, D2).

use std::path::{Path, PathBuf};

use crate::cli::harness::HarnessId;
use crate::cli::harness_apply::{BackupRecord, write_backup};

/// On-disk filename for the Claude Code `symforge-admin` slash-command file
/// installed under `~/.claude/commands/`.
pub const CLAUDE_ADMIN_COMMAND_FILE: &str = "symforge-admin.md";

/// Markdown body of the Claude Code `/symforge-admin` slash-command. Invoking it
/// runs the `symforge admin` CLI verb, which returns a running dashboard's URL
/// immediately or starts a new server in the foreground (contracts/command-file.md).
const CLAUDE_ADMIN_COMMAND_BODY: &str = "\
---
description: Open the SymForge operator dashboard (reuse a running server, or start one in the background)
---

Run the SymForge admin verb to open the operator dashboard.

Execute this command in the project root:

```
symforge admin
```

If a dashboard is already running on the remembered port, `symforge admin` prints
its URL and returns immediately. If none is running it STARTS a new server and
serves it IN THE FOREGROUND until you stop it (Ctrl-C) — so if you are an agent
running this in a shell tool, launch it as a background/detached process (e.g.
append `&` or run it detached) and then read the printed dashboard URL, or the
tool call blocks until it times out and kills the fresh server.

If your environment cannot run shell commands, use the `symforge-admin` MCP
prompt instead, which reports the running dashboard URL.
";

/// Whether a harness got a command file installed, and where — or why it did not.
///
/// Returned per harness from [`install_admin_command_file`] so the caller can
/// report exactly which harness got the one-keystroke command vs which rely on
/// the universal MCP prompt (no harness silently gets nothing — the MCP prompt is
/// the floor for all of them).
///
/// `PartialEq`/`Eq` are intentionally not derived: the `Installed` variant holds
/// a [`BackupRecord`] (a `005` type that does not implement them, and whose
/// timestamped backup path is not a meaningful equality key). Callers match on
/// the variant instead.
#[derive(Debug, Clone)]
pub enum CommandFileOutcome {
    /// A command file was written at `path` (with `backup` if it pre-existed).
    Installed {
        id: HarnessId,
        path: PathBuf,
        backup: Option<BackupRecord>,
    },
    /// The harness documents no command-file format; it relies on the MCP prompt
    /// only. No file was written (no guessed/broken affordance).
    PromptOnly { id: HarnessId },
    /// Writing the command file failed; the harness still has the MCP prompt.
    Failed { id: HarnessId, reason: String },
}

/// Install the `symforge-admin` command file for a single harness under `home`.
///
/// - **Claude Code**: writes `<home>/.claude/commands/symforge-admin.md`, backing
///   up any pre-existing file first (restorable-backup path, FR-016) and writing
///   atomically.
/// - **Any other harness**: writes nothing and returns
///   [`CommandFileOutcome::PromptOnly`] — these harnesses use the MCP prompt
///   (D2, no guessed format).
///
/// `home` is injected (a TempDir in tests, the operator home in production) so no
/// real config is touched in tests (FR-018).
pub fn install_admin_command_file(home: &Path, id: HarnessId) -> CommandFileOutcome {
    match id {
        HarnessId::ClaudeCode => install_claude_admin_command_file(home),
        // No documented command-file format → the MCP prompt is the affordance.
        HarnessId::ClaudeDesktop
        | HarnessId::Codex
        | HarnessId::Gemini
        | HarnessId::KiloCode
        | HarnessId::Cursor => CommandFileOutcome::PromptOnly { id },
    }
}

/// Install the Claude Code command file under `<home>/.claude/commands/`,
/// backing up an existing file first (restorable-backup path) and writing
/// atomically.
fn install_claude_admin_command_file(home: &Path) -> CommandFileOutcome {
    let id = HarnessId::ClaudeCode;
    let path = claude_admin_command_path(home);

    if let Some(parent) = path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return CommandFileOutcome::Failed {
            id,
            reason: format!("creating {}: {e}", parent.display()),
        };
    }

    // Back up any pre-existing command file before overwriting (restorable
    // backup, FR-016) so an operator's custom command is never lost unbacked.
    let backup = if path.exists() {
        match write_backup(&path) {
            Ok(record) => Some(record),
            Err(e) => {
                return CommandFileOutcome::Failed {
                    id,
                    reason: format!("backing up {}: {e}", path.display()),
                };
            }
        }
    } else {
        None
    };

    if let Err(e) = atomic_write(&path, CLAUDE_ADMIN_COMMAND_BODY.as_bytes()) {
        return CommandFileOutcome::Failed {
            id,
            reason: format!("writing {}: {e}", path.display()),
        };
    }

    CommandFileOutcome::Installed { id, path, backup }
}

/// The Claude Code `symforge-admin` command-file path under `home`.
fn claude_admin_command_path(home: &Path) -> PathBuf {
    home.join(".claude")
        .join("commands")
        .join(CLAUDE_ADMIN_COMMAND_FILE)
}

/// Atomically write `content` to `path` (temp file in the same dir + rename),
/// mirroring `harness_apply::atomic_write` (private to that module) and
/// `operator_profile::atomic_write_in`.
fn atomic_write(path: &Path, content: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "command-file path has no parent directory",
        )
    })?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
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

    #[test]
    fn installs_claude_code_command_file() {
        let home = tempfile::tempdir().expect("temp home");
        let outcome = install_admin_command_file(home.path(), HarnessId::ClaudeCode);

        let path = match outcome {
            CommandFileOutcome::Installed { path, backup, id } => {
                assert_eq!(id, HarnessId::ClaudeCode);
                assert!(backup.is_none(), "fresh install has no backup");
                path
            }
            other => panic!("expected Installed, got {other:?}"),
        };

        assert_eq!(path, claude_admin_command_path(home.path()));
        let body = std::fs::read_to_string(&path).expect("command file written");
        assert!(
            body.contains("symforge admin"),
            "body invokes the verb: {body}"
        );
        assert!(
            body.contains("description:"),
            "body has slash-command front matter"
        );
    }

    #[test]
    fn reinstall_backs_up_existing_command_file() {
        let home = tempfile::tempdir().expect("temp home");
        let path = claude_admin_command_path(home.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let prior = "my custom symforge-admin command\n";
        std::fs::write(&path, prior).unwrap();

        let outcome = install_admin_command_file(home.path(), HarnessId::ClaudeCode);
        let backup = match outcome {
            CommandFileOutcome::Installed {
                backup: Some(b), ..
            } => b,
            other => panic!("expected Installed with a backup, got {other:?}"),
        };

        // The new body is in place; the prior body is preserved byte-exact in the
        // backup (restorable).
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.contains("symforge admin"));
        assert_eq!(std::fs::read_to_string(&backup.backup).unwrap(), prior);
    }

    #[test]
    fn no_format_harness_installs_nothing() {
        let home = tempfile::tempdir().expect("temp home");
        for id in [
            HarnessId::Codex,
            HarnessId::Gemini,
            HarnessId::Cursor,
            HarnessId::KiloCode,
            HarnessId::ClaudeDesktop,
        ] {
            let outcome = install_admin_command_file(home.path(), id);
            match outcome {
                CommandFileOutcome::PromptOnly { id: got } => assert_eq!(got, id),
                other => panic!(
                    "{id:?} has no documented command-file format; \
                     expected PromptOnly, got {other:?}"
                ),
            }
        }
        // No `.claude` tree was created for the no-format harnesses (the Claude
        // Code path is the only writer).
        assert!(!home.path().join(".claude").exists());
    }
}
