mod git_test_helpers {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/git/test_helpers.rs"
    ));
}

#[test]
fn retries_windows_ref_lock_errors_and_returns_success() {
    let mut attempts = 0;

    let result = git_test_helpers::retry_ref_update_for_test(|| {
        attempts += 1;
        if attempts < 3 {
            return Err(git2::Error::from_str(
                "failed to rename lockfile .git/refs/heads/main.lock: The process cannot access the file because it is being used by another process",
            ));
        }

        Ok("committed")
    });

    assert_eq!(
        result.expect("retry should eventually succeed"),
        "committed"
    );
    assert_eq!(attempts, 3);
}

#[test]
fn preserves_final_git_error_when_retry_budget_is_exhausted() {
    let mut attempts = 0;

    let result = git_test_helpers::retry_ref_update_for_test::<_, ()>(|| {
        attempts += 1;
        Err(git2::Error::from_str(
            "failed to rename lockfile .git/refs/heads/main.lock: final locked ref error",
        ))
    });

    let err = result.expect_err("retry budget should preserve the final git error");
    assert!(err.message().contains("final locked ref error"));
    assert_eq!(attempts, git_test_helpers::REF_UPDATE_RETRY_ATTEMPTS);
}
