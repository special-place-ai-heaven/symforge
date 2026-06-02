//! Version output with a best-effort npm update advisory.

use std::ffi::OsString;
use std::time::Duration;

const UPDATE_CHECK_TIMEOUT: Duration = Duration::from_millis(1500);

pub fn run_version() -> anyhow::Result<()> {
    for line in version_lines(
        env!("CARGO_PKG_VERSION"),
        latest_npm_version_with_timeout(UPDATE_CHECK_TIMEOUT).as_deref(),
    ) {
        println!("{line}");
    }

    Ok(())
}

pub fn is_version_request(args: &[OsString]) -> bool {
    args.len() == 2 && matches!(args[1].to_str(), Some("--version" | "-V"))
}

fn latest_npm_version_with_timeout(timeout: Duration) -> Option<String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .ok()?;

    runtime.block_on(async move {
        let client = reqwest::Client::builder().timeout(timeout).build().ok()?;
        let response = client
            .get("https://registry.npmjs.org/symforge/latest")
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?;
        let body: serde_json::Value = response.json().await.ok()?;
        let version = body.get("version")?.as_str()?;
        parse_latest_version_output(version)
    })
}

/// Resolve the latest published symforge version from the npm registry using a
/// short default timeout. Returns `None` when offline or on any registry error,
/// so callers can degrade gracefully rather than fail.
pub(crate) fn latest_npm_version() -> Option<String> {
    latest_npm_version_with_timeout(std::time::Duration::from_secs(3))
}

pub(crate) fn version_lines(current: &str, latest: Option<&str>) -> Vec<String> {
    let mut lines = vec![format!("symforge {current}")];
    if let Some(latest) = latest
        && is_newer_version(latest, current)
    {
        lines.push(format!(
            "Update available: {latest} (run `symforge update`)"
        ));
    }

    lines
}

pub(crate) fn parse_latest_version_output(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .filter(|version| {
            version
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '+'))
        })
        .map(str::to_string)
}

pub(crate) fn is_newer_version(latest: &str, current: &str) -> bool {
    let Some(latest_parts) = numeric_version_parts(latest) else {
        return false;
    };
    let Some(current_parts) = numeric_version_parts(current) else {
        return false;
    };

    let max_len = latest_parts.len().max(current_parts.len());
    for index in 0..max_len {
        let latest_part = latest_parts.get(index).copied().unwrap_or(0);
        let current_part = current_parts.get(index).copied().unwrap_or(0);
        if latest_part > current_part {
            return true;
        }
        if latest_part < current_part {
            return false;
        }
    }

    false
}

fn numeric_version_parts(version: &str) -> Option<Vec<u64>> {
    let core = version.split(['-', '+']).next()?;
    let parts = core
        .split('.')
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;

    if parts.is_empty() { None } else { Some(parts) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_exact_version_requests() {
        assert!(is_version_request(&[
            OsString::from("symforge"),
            OsString::from("--version")
        ]));
        assert!(is_version_request(&[
            OsString::from("symforge"),
            OsString::from("-V")
        ]));
        assert!(!is_version_request(&[
            OsString::from("symforge"),
            OsString::from("--version"),
            OsString::from("--verbose")
        ]));
    }

    #[test]
    fn parses_latest_version_from_npm_output() {
        assert_eq!(
            parse_latest_version_output("\n7.14.2\n"),
            Some("7.14.2".to_string())
        );
        assert_eq!(parse_latest_version_output("not a version!"), None);
    }

    #[test]
    fn compares_semver_numeric_parts() {
        assert!(is_newer_version("7.14.10", "7.14.2"));
        assert!(is_newer_version("8.0.0", "7.14.2"));
        assert!(!is_newer_version("7.14.2", "7.14.2"));
        assert!(!is_newer_version("7.14.1", "7.14.2"));
    }

    #[test]
    fn version_lines_include_update_advisory_only_when_newer() {
        assert_eq!(
            version_lines("7.14.1", Some("7.14.2")),
            vec![
                "symforge 7.14.1".to_string(),
                "Update available: 7.14.2 (run `symforge update`)".to_string()
            ]
        );
        assert_eq!(
            version_lines("7.14.1", Some("7.14.1")),
            vec!["symforge 7.14.1".to_string()]
        );
        assert_eq!(
            version_lines("7.14.1", None),
            vec!["symforge 7.14.1".to_string()]
        );
    }
}
