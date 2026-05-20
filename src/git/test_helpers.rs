use std::thread;
use std::time::Duration;

pub(crate) const REF_UPDATE_RETRY_ATTEMPTS: usize = 6;

const REF_UPDATE_BACKOFF_MS: [u64; REF_UPDATE_RETRY_ATTEMPTS - 1] = [5, 10, 20, 40, 80];

#[allow(dead_code)]
pub(crate) fn commit_head_with_retry(
    repo: &git2::Repository,
    author: &git2::Signature<'_>,
    committer: &git2::Signature<'_>,
    message: &str,
    tree: &git2::Tree<'_>,
    parents: &[&git2::Commit<'_>],
) -> git2::Oid {
    retry_ref_update(|| repo.commit(Some("HEAD"), author, committer, message, tree, parents), true)
        .unwrap_or_else(|err| {
            panic!(
                "test git commit failed after {REF_UPDATE_RETRY_ATTEMPTS} attempts; final git error: {err}"
            )
        })
}

#[allow(dead_code)]
pub(crate) fn retry_ref_update_for_test<F, T>(operation: F) -> Result<T, git2::Error>
where
    F: FnMut() -> Result<T, git2::Error>,
{
    retry_ref_update(operation, false)
}

fn retry_ref_update<F, T>(
    mut operation: F,
    sleep_between_retries: bool,
) -> Result<T, git2::Error>
where
    F: FnMut() -> Result<T, git2::Error>,
{
    for attempt in 1..=REF_UPDATE_RETRY_ATTEMPTS {
        match operation() {
            Ok(value) => return Ok(value),
            Err(err) => {
                if attempt == REF_UPDATE_RETRY_ATTEMPTS
                    || !is_retryable_ref_lock_error(err.message())
                {
                    return Err(err);
                }

                let delay_ms = REF_UPDATE_BACKOFF_MS[attempt - 1];
                eprintln!(
                    "retrying test git commit after transient ref lock error \
                     (attempt {}/{REF_UPDATE_RETRY_ATTEMPTS}, backoff {delay_ms}ms): {err}",
                    attempt + 1
                );
                if sleep_between_retries {
                    thread::sleep(Duration::from_millis(delay_ms));
                }
            }
        }
    }

    unreachable!("retry loop always returns on success or final error")
}

fn is_retryable_ref_lock_error(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    let mentions_head_ref = message.contains("refs/heads")
        || message.contains("head.lock")
        || message.contains("reference 'head'");
    let mentions_lock_or_rename = message.contains(".lock")
        || message.contains("lockfile")
        || message.contains("lock file")
        || message.contains("failed to lock")
        || message.contains("could not lock")
        || message.contains("failed to rename")
        || message.contains("access is denied")
        || message.contains("being used by another process")
        || message.contains("cannot access the file");

    mentions_head_ref && mentions_lock_or_rename
}
