//! Helper for spawning child processes without flashing console windows.
//!
//! The daemon runs detached with no console (see `spawn_daemon_process`).
//! On Windows, a console-subsystem child spawned from a console-less parent
//! allocates a brand-new conhost window — so every periodic `tasklist`
//! liveness check, `taskkill`, `git worktree list`, or PATH-shadow
//! `--version` probe used to flash a visible terminal window at the user.
//!
//! Every `Command` built here carries `CREATE_NO_WINDOW` on Windows (the
//! standard flag for background children; no AV heuristic keys on it) and is
//! a plain `std::process::Command` everywhere else.

use std::ffi::OsStr;
use std::process::Command;

/// Build a `Command` that never opens a console window on Windows.
///
/// Use this for EVERY helper-process spawn (`tasklist`, `taskkill`, `git`,
/// `npm`, version probes). The one deliberate exception is
/// `spawn_daemon_process`, which sets its own `DETACHED_PROCESS |
/// CREATE_NO_WINDOW` combination.
pub(crate) fn hidden_command(program: impl AsRef<OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The helper must behave like a normal Command: spawn, capture output,
    /// report exit status. (Window visibility itself is not assertable in a
    /// headless test; this pins the plumbing.)
    #[test]
    fn test_hidden_command_runs_and_captures_output() {
        #[cfg(windows)]
        let output = hidden_command("cmd")
            .args(["/C", "echo hidden-ok"])
            .output()
            .expect("hidden_command should spawn");
        #[cfg(not(windows))]
        let output = hidden_command("sh")
            .args(["-c", "echo hidden-ok"])
            .output()
            .expect("hidden_command should spawn");

        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("hidden-ok"));
    }
}
