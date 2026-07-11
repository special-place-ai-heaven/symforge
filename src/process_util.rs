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
pub fn hidden_command(program: impl AsRef<OsStr>) -> Command {
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

    /// INCIDENT GUARD (2026-07-11): SymForge must NEVER spawn a visible
    /// console window. On Windows, any console-subsystem child spawned from a
    /// console-less parent allocates a brand-new conhost window that steals
    /// focus — during the 2026-07-11 fork-bomb incident this flooded the
    /// desktop and made the machine unusable. Every process spawn must go
    /// through [`hidden_command`] (CREATE_NO_WINDOW); the ONE deliberate
    /// exception is `spawn_daemon_process` in `src/daemon.rs`, which sets its
    /// own `DETACHED_PROCESS | CREATE_NO_WINDOW` combination. This tripwire
    /// scans the source tree and fails on any new raw `Command::new(` call.
    #[test]
    fn test_no_raw_command_spawns_outside_hidden_command() {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut offenders: Vec<String> = Vec::new();
        for tree in ["src", "tests"] {
            let tree_root = manifest.join(tree);
            let mut stack = vec![tree_root.clone()];
            while let Some(dir) = stack.pop() {
                for entry in std::fs::read_dir(&dir).expect("read source dir") {
                    let entry = entry.expect("dir entry");
                    let path = entry.path();
                    if path.is_dir() {
                        // Vendored test corpora are third-party code, not ours.
                        if tree == "tests"
                            && path.file_name().is_some_and(|name| name == "fixtures")
                        {
                            continue;
                        }
                        stack.push(path);
                        continue;
                    }
                    if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                        continue;
                    }
                    let relative = path
                        .strip_prefix(&tree_root)
                        .expect("under tree root")
                        .to_string_lossy()
                        .replace('\\', "/");
                    if tree == "src" && relative == "process_util.rs" {
                        continue; // the one place allowed to build raw Commands
                    }
                    let contents = std::fs::read_to_string(&path).expect("read source file");
                    let mut hits = 0usize;
                    for (number, line) in contents.lines().enumerate() {
                        let trimmed = line.trim_start();
                        if trimmed.starts_with("//") || trimmed.starts_with("//!") {
                            continue; // docs/comments may mention the pattern
                        }
                        if line.contains("Command::new(") {
                            hits += 1;
                            offenders.push(format!("{tree}/{relative}:{}", number + 1));
                        }
                    }
                    if tree == "src" && relative == "daemon.rs" && hits == 1 {
                        // The single spawn_daemon_process site carries its own
                        // DETACHED_PROCESS | CREATE_NO_WINDOW flags.
                        offenders.pop();
                    }
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "raw Command::new( spawns found — route them through \
             process_util::hidden_command so no console window can ever open: {offenders:?}"
        );
    }

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
