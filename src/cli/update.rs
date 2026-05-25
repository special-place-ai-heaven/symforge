//! Explicit npm-managed self-update command.

use anyhow::{Context, bail};
use std::process::{Command, Stdio};

const UPDATE_ARGS: [&str; 3] = ["install", "-g", "symforge@latest"];

pub fn run_update() -> anyhow::Result<()> {
    run_update_with(std::env::consts::OS, |program, args| {
        let status = Command::new(program)
            .args(args)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .with_context(|| format!("failed to start `{}`", invocation_text(program, args)))?;

        Ok(status.success())
    })
}

pub(crate) fn run_update_with<F>(os: &str, mut run: F) -> anyhow::Result<()>
where
    F: FnMut(&str, &[&str]) -> anyhow::Result<bool>,
{
    let (program, args) = update_invocation(os);
    if run(program, args)? {
        return Ok(());
    }

    bail!(
        "symforge update failed: `{}` exited unsuccessfully",
        invocation_text(program, args)
    );
}

pub(crate) fn update_invocation(os: &str) -> (&'static str, &'static [&'static str]) {
    (npm_executable_for_os(os), &UPDATE_ARGS)
}

pub(crate) fn npm_executable_for_os(os: &str) -> &'static str {
    if os == "windows" { "npm.cmd" } else { "npm" }
}

fn invocation_text(program: &str, args: &[&str]) -> String {
    let mut parts = Vec::with_capacity(args.len() + 1);
    parts.push(program.to_string());
    parts.extend(args.iter().map(|arg| (*arg).to_string()));
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_invocation_uses_npm_cmd_on_windows() {
        let (program, args) = update_invocation("windows");

        assert_eq!(program, "npm.cmd");
        assert_eq!(args, ["install", "-g", "symforge@latest"]);
    }

    #[test]
    fn update_invocation_uses_npm_elsewhere() {
        let (program, args) = update_invocation("linux");

        assert_eq!(program, "npm");
        assert_eq!(args, ["install", "-g", "symforge@latest"]);
    }

    #[test]
    fn run_update_with_invokes_npm_install_global_latest() {
        let mut seen = Vec::new();

        run_update_with("linux", |program, args| {
            seen.push((
                program.to_string(),
                args.iter().map(|arg| arg.to_string()).collect(),
            ));
            Ok(true)
        })
        .expect("update should succeed");

        assert_eq!(
            seen,
            vec![(
                "npm".to_string(),
                vec![
                    "install".to_string(),
                    "-g".to_string(),
                    "symforge@latest".to_string()
                ]
            )]
        );
    }

    #[test]
    fn run_update_with_reports_failed_npm() {
        let err = run_update_with("linux", |_program, _args| Ok(false))
            .expect_err("failed npm update should be reported");

        assert!(
            err.to_string()
                .contains("`npm install -g symforge@latest` exited unsuccessfully"),
            "{err:?}"
        );
    }

    #[test]
    fn invocation_does_not_use_shell_wrappers() {
        let (program, args) = update_invocation("windows");
        let text = invocation_text(program, args).to_ascii_lowercase();

        assert!(!text.contains("powershell"));
        assert!(!text.contains("cmd /c"));
        assert!(!text.contains("executionpolicy"));
    }
}
