use std::time::Duration;
use symforge::edit_safety::tee::{TEE_MAX_FILE_BYTES, Tee, TeeRetention, TeeSnapshot};

#[test]
fn tee_snapshot_creates_recovery_copy_under_symforge_dir() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join(".git")).unwrap();
    let file_path = temp.path().join("src/lib.rs");
    std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    let original = b"pub fn original() {}\n";
    std::fs::write(&file_path, original).unwrap();

    let snapshot = Tee::for_repo(temp.path()).snapshot(&file_path).unwrap();
    let record = match snapshot {
        TeeSnapshot::Created(record) => record,
        other => panic!("expected created snapshot, got {other:?}"),
    };

    assert_eq!(record.original_path, file_path);
    assert!(
        record
            .tee_path
            .starts_with(temp.path().join(".symforge").join("tee"))
    );
    assert_eq!(std::fs::read(&record.tee_path).unwrap(), original);
    assert!(record.recovery_hint().contains(".symforge/tee/"));
    assert!(record.recovery_hint().contains("src/lib.rs"));
}

#[test]
fn tee_snapshot_retains_at_most_max_count_records() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join(".git")).unwrap();
    let file_path = temp.path().join("src/lib.rs");
    std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    std::fs::write(&file_path, b"pub fn original() {}\n").unwrap();

    // Small explicit cap keeps the test fast and independent of env defaults.
    const CAP: usize = 5;
    let tee = Tee::with_retention(
        temp.path(),
        TeeRetention {
            max_count: CAP,
            max_age: None,
        },
    );
    let mut created_paths = Vec::new();
    for i in 0..=CAP {
        std::fs::write(&file_path, format!("pub fn version_{i}() {{}}\n")).unwrap();
        let record = match tee.snapshot(&file_path).unwrap() {
            TeeSnapshot::Created(record) => record,
            other => panic!("expected created snapshot, got {other:?}"),
        };
        created_paths.push(record.tee_path);
    }

    let tee_dir = temp.path().join(".symforge").join("tee");
    let retained = std::fs::read_dir(&tee_dir).unwrap().count();
    assert_eq!(retained, CAP);
    assert!(
        !created_paths[0].exists(),
        "oldest snapshot should be evicted"
    );
    assert!(
        created_paths.last().unwrap().exists(),
        "newest snapshot should be retained"
    );
}

#[test]
fn tee_snapshot_prunes_by_age_at_write_time() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join(".git")).unwrap();
    let tee_dir = temp.path().join(".symforge").join("tee");
    std::fs::create_dir_all(&tee_dir).unwrap();

    // Plant a stale snapshot far in the past, with a synthetic old mtime
    // applied via a backdated copy through SystemTime arithmetic on the file.
    let stale = tee_dir.join("0000000000000-000000-old.rs");
    std::fs::write(&stale, b"stale\n").unwrap();
    set_mtime_days_ago(&stale, 30);

    // A current file to snapshot. With a 7-day age cap the write should prune
    // the 30-day-old stale snapshot but keep the fresh one.
    let file_path = temp.path().join("src/lib.rs");
    std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    std::fs::write(&file_path, b"pub fn fresh() {}\n").unwrap();

    let tee = Tee::with_retention(
        temp.path(),
        TeeRetention {
            max_count: 0, // disable count; isolate age pruning
            max_age: Some(Duration::from_secs(7 * 86_400)),
        },
    );
    let record = match tee.snapshot(&file_path).unwrap() {
        TeeSnapshot::Created(record) => record,
        other => panic!("expected created snapshot, got {other:?}"),
    };

    assert!(!stale.exists(), "30-day-old snapshot should be age-pruned");
    assert!(
        record.tee_path.exists(),
        "fresh snapshot should be retained"
    );
}

#[test]
fn tee_snapshot_pruning_disabled_when_both_limits_zero() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join(".git")).unwrap();
    let tee_dir = temp.path().join(".symforge").join("tee");
    std::fs::create_dir_all(&tee_dir).unwrap();

    // An ancient snapshot that age-pruning would normally remove.
    let ancient = tee_dir.join("0000000000000-000000-ancient.rs");
    std::fs::write(&ancient, b"ancient\n").unwrap();
    set_mtime_days_ago(&ancient, 365);

    let file_path = temp.path().join("src/lib.rs");
    std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    std::fs::write(&file_path, b"pub fn fresh() {}\n").unwrap();

    // Both dimensions disabled (env=0 equivalent): no pruning at all.
    let tee = Tee::with_retention(
        temp.path(),
        TeeRetention {
            max_count: 0,
            max_age: None,
        },
    );
    let record = match tee.snapshot(&file_path).unwrap() {
        TeeSnapshot::Created(record) => record,
        other => panic!("expected created snapshot, got {other:?}"),
    };

    assert!(
        ancient.exists(),
        "365-day-old snapshot must survive when pruning is disabled"
    );
    assert!(record.tee_path.exists(), "fresh snapshot should exist");
}

/// Backdate a file's modified time by `days` using the cross-platform
/// `filetime` crate, so age-based pruning can be exercised deterministically.
fn set_mtime_days_ago(path: &std::path::Path, days: u64) {
    let target = std::time::SystemTime::now()
        .checked_sub(Duration::from_secs(days * 86_400))
        .unwrap();
    filetime::set_file_mtime(path, filetime::FileTime::from_system_time(target))
        .expect("backdate mtime");
}

#[test]
fn tee_snapshot_skips_files_larger_than_size_cap() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join(".git")).unwrap();
    let file_path = temp.path().join("src/large.rs");
    std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    std::fs::write(&file_path, vec![b'x'; TEE_MAX_FILE_BYTES + 1]).unwrap();

    let snapshot = Tee::for_repo(temp.path()).snapshot(&file_path).unwrap();

    assert!(matches!(
        snapshot,
        TeeSnapshot::SkippedTooLarge {
            size,
            max_size
        } if size == TEE_MAX_FILE_BYTES + 1 && max_size == TEE_MAX_FILE_BYTES
    ));
    assert!(!temp.path().join(".symforge").join("tee").exists());
}
