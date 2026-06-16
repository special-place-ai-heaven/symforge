// US2 (T010): backup-then-apply safety — dry-run writes nothing; apply creates
// a backup; restore reproduces prior bytes exactly; second apply is a no-op;
// malformed / permission-denied targets are reported and never corrupt the file
// or abort the run.
#![cfg(feature = "server")]

use std::path::PathBuf;

use symforge::cli::harness::{
    AttachEntry, HarnessFormat, HarnessId, HarnessRegistry, HarnessTarget,
};
use symforge::cli::harness_apply::{self, ApplyOutcome, PlannedAction};

fn entry() -> AttachEntry {
    AttachEntry::new("http://127.0.0.1:8787/mcp", Some("sf_key".to_string()))
}

fn json_target(path: PathBuf) -> HarnessTarget {
    HarnessTarget {
        id: HarnessId::ClaudeCode,
        config_path: path,
        format: HarnessFormat::Json,
    }
}

fn registry(path: PathBuf) -> HarnessRegistry {
    HarnessRegistry::from_targets(vec![json_target(path)])
}

#[test]
fn dry_run_plan_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("config.json");
    let original = "{}";
    std::fs::write(&cfg, original).unwrap();

    let reg = registry(cfg.clone());
    let plan = harness_apply::plan(&reg, &entry());

    // A plan is computed (Add), but the file is byte-identical and no .bak exists.
    assert!(matches!(plan.changes[0].action, PlannedAction::Add));
    assert_eq!(std::fs::read_to_string(&cfg).unwrap(), original);

    let backups: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "bak"))
        .collect();
    assert!(backups.is_empty(), "dry-run must not create a backup");
}

#[test]
fn apply_creates_backup_and_restore_is_byte_exact() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("config.json");
    // Deliberately quirky whitespace so a byte-exact restore is meaningful.
    let original = "{\r\n  \"mcpServers\": {  }\r\n}\r\n";
    std::fs::write(&cfg, original.as_bytes()).unwrap();

    let reg = registry(cfg.clone());
    let plan = harness_apply::plan(&reg, &entry());
    let outcomes = harness_apply::apply(&plan);

    let backup = match &outcomes[0] {
        ApplyOutcome::Wrote { backup, .. } => backup.clone().expect("backup recorded"),
        other => panic!("expected Wrote, got {other:?}"),
    };
    assert!(backup.backup.exists(), "backup file must exist on disk");

    // The live file changed (entry added).
    let after = std::fs::read_to_string(&cfg).unwrap();
    assert!(after.contains("symforge"));
    assert!(after.contains("http://127.0.0.1:8787/mcp"));

    // Restore returns the exact prior bytes.
    harness_apply::restore(&backup).unwrap();
    let restored = std::fs::read(&cfg).unwrap();
    assert_eq!(restored, original.as_bytes());
}

#[test]
fn second_apply_is_a_noop() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("config.json");
    std::fs::write(&cfg, "{}").unwrap();
    let reg = registry(cfg.clone());

    harness_apply::apply(&harness_apply::plan(&reg, &entry()));
    let after_first = std::fs::read(&cfg).unwrap();

    let plan2 = harness_apply::plan(&reg, &entry());
    assert!(matches!(plan2.changes[0].action, PlannedAction::Skip(_)));
    let outcomes2 = harness_apply::apply(&plan2);
    assert!(matches!(outcomes2[0], ApplyOutcome::Skipped { .. }));

    let after_second = std::fs::read(&cfg).unwrap();
    assert_eq!(
        after_first, after_second,
        "second apply must not change bytes"
    );
}

#[test]
fn stale_entry_is_refreshed_without_duplicate() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("config.json");
    std::fs::write(
        &cfg,
        r#"{"mcpServers":{"symforge":{"type":"http","url":"http://old:1/mcp","headers":{"Authorization":"Bearer old"}}}}"#,
    )
    .unwrap();
    let reg = registry(cfg.clone());

    let plan = harness_apply::plan(&reg, &entry());
    assert!(matches!(plan.changes[0].action, PlannedAction::Refresh));
    harness_apply::apply(&plan);

    let after: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&cfg).unwrap()).unwrap();
    assert_eq!(
        after["mcpServers"]["symforge"]["url"],
        "http://127.0.0.1:8787/mcp"
    );
    let servers = after["mcpServers"].as_object().unwrap();
    assert_eq!(servers.keys().filter(|k| *k == "symforge").count(), 1);
}

#[test]
fn malformed_target_is_reported_and_file_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("config.json");
    let bad = "{ this is not json";
    std::fs::write(&cfg, bad).unwrap();
    let reg = registry(cfg.clone());

    let plan = harness_apply::plan(&reg, &entry());
    assert!(matches!(plan.changes[0].action, PlannedAction::Error(_)));
    let outcomes = harness_apply::apply(&plan);
    assert!(matches!(outcomes[0], ApplyOutcome::Failed { .. }));

    // File is untouched and no backup was created.
    assert_eq!(std::fs::read_to_string(&cfg).unwrap(), bad);
    let bak_count = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "bak"))
        .count();
    assert_eq!(bak_count, 0);
}

#[test]
fn one_bad_target_does_not_abort_the_others() {
    let dir = tempfile::tempdir().unwrap();
    let good = dir.path().join("good.json");
    let bad = dir.path().join("bad.json");
    std::fs::write(&good, "{}").unwrap();
    std::fs::write(&bad, "{ broken").unwrap();

    let reg = HarnessRegistry::from_targets(vec![
        HarnessTarget {
            id: HarnessId::Codex, // arbitrary distinct id; format Json for both
            config_path: bad.clone(),
            format: HarnessFormat::Json,
        },
        json_target(good.clone()),
    ]);

    let outcomes = harness_apply::apply(&harness_apply::plan(&reg, &entry()));
    assert_eq!(outcomes.len(), 2);
    assert!(matches!(outcomes[0], ApplyOutcome::Failed { .. }));
    assert!(matches!(outcomes[1], ApplyOutcome::Wrote { .. }));

    // The good target was still written despite the bad one failing.
    assert!(std::fs::read_to_string(&good).unwrap().contains("symforge"));
    // The bad target is untouched.
    assert_eq!(std::fs::read_to_string(&bad).unwrap(), "{ broken");
}

#[cfg(unix)]
#[test]
fn permission_denied_target_is_reported_not_fatal() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("config.json");
    std::fs::write(&cfg, "{}").unwrap();
    // Make the directory read-only so the atomic write (temp + rename) fails.
    let mut perms = std::fs::metadata(dir.path()).unwrap().permissions();
    perms.set_mode(0o500); // r-x, no write
    std::fs::set_permissions(dir.path(), perms.clone()).unwrap();

    let reg = registry(cfg.clone());
    let outcomes = harness_apply::apply(&harness_apply::plan(&reg, &entry()));

    // Restore writability so the tempdir can clean up.
    let mut restore = perms;
    restore.set_mode(0o700);
    std::fs::set_permissions(dir.path(), restore).unwrap();

    assert!(
        matches!(outcomes[0], ApplyOutcome::Failed { .. }),
        "write into a read-only dir must be reported as Failed, got {:?}",
        outcomes[0]
    );
}
