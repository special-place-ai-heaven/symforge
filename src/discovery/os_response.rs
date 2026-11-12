//! File-access failures classified from the operating system's own error code
//! (spec 036, research D6, FR-019 to FR-022).
//!
//! A static table keyed by platform and raw OS code gives each known failure
//! its class, whether it is transient, the follow-up probe that separates
//! look-alike causes, and the remedy shown to the caller. A code absent from
//! the table is reported verbatim and counted, never guessed.
//!
//! The persisted [`AccessErrorKind`] is not extended: [`OsCause::stored_kind`]
//! maps every cause down to the kind the index already stores.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::domain::AccessErrorKind;

/// What a failed file access means for the caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OsClass {
    NotFound,
    Denied,
    /// Windows: the file was deleted while another process still holds it
    /// open; it reports access denied until the last handle closes.
    DeletePending,
    InUse,
    PathTooLong,
    Corrupt,
    ResourceExhausted,
    /// Refused by SymForge's own access rule (a link, not a regular file, a
    /// read bound), not by the operating system.
    Refused,
    /// A raw OS code the table does not know.
    Unknown,
}

/// The table a code is looked up in. Unix codes come from the `libc` crate
/// for the target being built, so each unix target gets its own errno values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Platform {
    Windows,
    Unix,
}

/// The follow-up check that separates causes reporting the same code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Probe {
    None,
    /// Windows access denied: an NT status of delete-pending captured at the
    /// failing call names that cause; otherwise a parent listing that no
    /// longer holds the name means the file is gone.
    NtStatusThenParentListing,
}

/// One row of the table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OsResponse {
    pub(crate) platform: Platform,
    pub(crate) code: i32,
    pub(crate) class: OsClass,
    pub(crate) transient: bool,
    pub(crate) probe: Probe,
    pub(crate) remedy: &'static str,
}

const fn row(
    platform: Platform,
    code: i32,
    class: OsClass,
    transient: bool,
    probe: Probe,
    remedy: &'static str,
) -> OsResponse {
    OsResponse {
        platform,
        code,
        class,
        transient,
        probe,
        remedy,
    }
}

const GONE: &str = "The file no longer exists; re-run the query.";
const DENIED: &str = "Grant read access to the file, or exclude it from the index.";
const IN_USE: &str = "Another process holds the file; retry once it is released.";
const TOO_LONG: &str = "Shorten the path or enable long paths for this volume.";
const CORRUPT: &str = "The file or the disk reports corruption; check the volume.";
const CANT_ACCESS: &str = "The system cannot open this file (often a cloud-only placeholder or an app alias); make it available locally or exclude it.";
const BAD_NAME: &str = "The name is not valid on this volume; rename the file.";
#[cfg(unix)]
const EXHAUSTED: &str = "The process ran out of file handles; retry, or raise the limit.";
#[cfg(unix)]
const LOOP: &str = "Resolve the symbolic link loop at this path.";
#[cfg(unix)]
const STALE: &str = "The network file handle went stale; retry.";
const DELETE_PENDING: &str = "The file was deleted while another process still holds it open; it disappears once that process closes it.";
const UNKNOWN: &str = "Unrecognised operating-system error; report it so the table can grow.";

use OsClass as C;
use Probe::{None as NoProbe, NtStatusThenParentListing as NtProbe};

/// Windows rows: Win32 `ERROR_*` values. Compiled on every target so their
/// classification is tested everywhere.
pub(crate) const WINDOWS_RESPONSES: &[OsResponse] = &[
    row(Platform::Windows, 2, C::NotFound, false, NoProbe, GONE),
    row(Platform::Windows, 3, C::NotFound, false, NoProbe, GONE),
    row(Platform::Windows, 5, C::Denied, false, NtProbe, DENIED),
    row(Platform::Windows, 32, C::InUse, true, NoProbe, IN_USE),
    row(Platform::Windows, 33, C::InUse, true, NoProbe, IN_USE),
    // ERROR_INVALID_NAME: no file can exist under this spelling.
    row(
        Platform::Windows,
        123,
        C::NotFound,
        false,
        NoProbe,
        BAD_NAME,
    ),
    row(
        Platform::Windows,
        206,
        C::PathTooLong,
        false,
        NoProbe,
        TOO_LONG,
    ),
    // ERROR_DIRECTORY: a path component is no longer a directory.
    row(Platform::Windows, 267, C::NotFound, false, NoProbe, GONE),
    row(
        Platform::Windows,
        303,
        C::DeletePending,
        false,
        NoProbe,
        DELETE_PENDING,
    ),
    row(Platform::Windows, 1392, C::Corrupt, false, NoProbe, CORRUPT),
    row(Platform::Windows, 1393, C::Corrupt, false, NoProbe, CORRUPT),
    row(
        Platform::Windows,
        1920,
        C::Denied,
        false,
        NoProbe,
        CANT_ACCESS,
    ),
];

/// Unix rows: errno values of the target being built.
#[cfg(unix)]
pub(crate) const UNIX_RESPONSES: &[OsResponse] = &[
    row(
        Platform::Unix,
        libc::ENOENT,
        C::NotFound,
        false,
        NoProbe,
        GONE,
    ),
    // A directory replaced by a file: the path no longer names a file.
    row(
        Platform::Unix,
        libc::ENOTDIR,
        C::NotFound,
        false,
        NoProbe,
        GONE,
    ),
    row(
        Platform::Unix,
        libc::EACCES,
        C::Denied,
        false,
        NoProbe,
        DENIED,
    ),
    row(
        Platform::Unix,
        libc::EPERM,
        C::Denied,
        false,
        NoProbe,
        DENIED,
    ),
    row(Platform::Unix, libc::EBUSY, C::InUse, true, NoProbe, IN_USE),
    row(Platform::Unix, libc::ESTALE, C::InUse, true, NoProbe, STALE),
    row(
        Platform::Unix,
        libc::ELOOP,
        C::Refused,
        false,
        NoProbe,
        LOOP,
    ),
    row(
        Platform::Unix,
        libc::ENAMETOOLONG,
        C::PathTooLong,
        false,
        NoProbe,
        TOO_LONG,
    ),
    row(
        Platform::Unix,
        libc::EIO,
        C::Corrupt,
        false,
        NoProbe,
        CORRUPT,
    ),
    row(
        Platform::Unix,
        libc::EMFILE,
        C::ResourceExhausted,
        true,
        NoProbe,
        EXHAUSTED,
    ),
    row(
        Platform::Unix,
        libc::ENFILE,
        C::ResourceExhausted,
        true,
        NoProbe,
        EXHAUSTED,
    ),
];

/// `STATUS_DELETE_PENDING`.
pub(crate) const STATUS_DELETE_PENDING: u32 = 0xC000_0056;

/// The table for this target; `None` on a target with neither (classification
/// then falls back to the error's kind).
pub(crate) const fn current_platform() -> Option<Platform> {
    if cfg!(windows) {
        Some(Platform::Windows)
    } else if cfg!(unix) {
        Some(Platform::Unix)
    } else {
        None
    }
}

pub(crate) fn lookup(platform: Platform, code: i32) -> Option<&'static OsResponse> {
    let rows: &[OsResponse] = match platform {
        Platform::Windows => WINDOWS_RESPONSES,
        #[cfg(unix)]
        Platform::Unix => UNIX_RESPONSES,
        #[cfg(not(unix))]
        Platform::Unix => &[],
    };
    rows.iter().find(|row| row.code == code)
}

/// A failed access as captured at the failure site. The Windows NT status is
/// thread-local and valid only right after the failing call, so it is read
/// there and travels with the error.
#[derive(Debug)]
pub(crate) struct AccessFailure {
    pub(crate) error: io::Error,
    pub(crate) nt_status: Option<u32>,
}

impl AccessFailure {
    /// Capture `error` immediately after the call that produced it. Round 3
    /// item 11 (compare, not clear): the thread's NT status is read only when
    /// the thread's last Win32 error is this error's code, i.e. the failure
    /// came from the syscall just made; an error built without one (a
    /// constructed code 5) gets no NT status rather than a stale one.
    pub(crate) fn capture(error: io::Error) -> Self {
        let nt_status = (error.raw_os_error() == Some(5)
            && io::Error::last_os_error().raw_os_error() == Some(5))
        .then(ntdll::last_nt_status)
        .flatten();
        Self { error, nt_status }
    }
}

impl From<io::Error> for AccessFailure {
    fn from(error: io::Error) -> Self {
        Self::capture(error)
    }
}

#[cfg(windows)]
mod ntdll {
    // SAFETY: `RtlGetLastNtStatus` takes no arguments and only reads the
    // calling thread's last NT status from its own TEB; it cannot fault.
    #[allow(unsafe_code)]
    #[link(name = "ntdll")]
    unsafe extern "system" {
        safe fn RtlGetLastNtStatus() -> i32;
    }

    pub(super) fn last_nt_status() -> Option<u32> {
        // NTSTATUS is a 32-bit value; the bit pattern is the status code.
        Some(RtlGetLastNtStatus() as u32)
    }
}

#[cfg(not(windows))]
mod ntdll {
    pub(super) fn last_nt_status() -> Option<u32> {
        None
    }
}

/// A classified failure: what the answer and health report about a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OsCause {
    pub(crate) class: OsClass,
    pub(crate) raw_code: Option<i32>,
    pub(crate) nt_status: Option<u32>,
    pub(crate) transient: bool,
    pub(crate) remedy: &'static str,
    kind: io::ErrorKind,
    /// The error's own text, kept verbatim for unknown codes.
    message: String,
}

impl OsCause {
    /// A refusal by SymForge's own access rule, not by the operating system;
    /// `reason` names it without any absolute path.
    pub(crate) fn refused(reason: &'static str) -> Self {
        Self {
            class: OsClass::Refused,
            raw_code: None,
            nt_status: None,
            transient: false,
            remedy: "",
            kind: io::ErrorKind::Other,
            message: reason.to_string(),
        }
    }

    /// `"; <remedy>"`, or nothing for a cause with no remedy (a refusal by
    /// SymForge's own rule), so a message never ends in a dangling `"; "`.
    pub(crate) fn remedy_suffix(&self) -> String {
        if self.remedy.is_empty() {
            String::new()
        } else {
            format!("; {}", self.remedy)
        }
    }

    /// The persisted kind, mapped exactly as before this table existed so no
    /// stored disposition changes.
    pub(crate) fn stored_kind(&self) -> AccessErrorKind {
        stored_kind(self.kind)
    }

    pub(crate) fn kind(&self) -> io::ErrorKind {
        self.kind
    }

    /// One phrase naming the cause, with the raw OS code when there is one.
    pub(crate) fn describe(&self) -> String {
        let text = match self.class {
            OsClass::NotFound => "not found",
            OsClass::Denied => "access denied",
            OsClass::DeletePending => "pending deletion",
            OsClass::InUse => "in use by another process",
            OsClass::PathTooLong => "path too long",
            OsClass::Corrupt => "reported corrupt",
            OsClass::ResourceExhausted => "out of file handles",
            // The refusal text of `path_must_be_regular_non_link` carries an
            // absolute path; answers name files by their repository path.
            OsClass::Refused if self.kind == io::ErrorKind::InvalidInput => {
                return "not a regular file (a link, directory or device)".to_string();
            }
            OsClass::Refused => return self.message.clone(),
            OsClass::Unknown => return format!("unrecognised error: {}", self.message),
        };
        match self.raw_code {
            Some(code) => format!("{text} (os error {code})"),
            None => text.to_string(),
        }
    }
}

fn stored_kind(kind: io::ErrorKind) -> AccessErrorKind {
    match kind {
        io::ErrorKind::NotFound => AccessErrorKind::NotFound,
        io::ErrorKind::PermissionDenied => AccessErrorKind::PermissionDenied,
        io::ErrorKind::InvalidData | io::ErrorKind::InvalidInput => AccessErrorKind::InvalidData,
        io::ErrorKind::OutOfMemory => AccessErrorKind::ResourceExhausted,
        _ => AccessErrorKind::Other,
    }
}

/// The persisted kind for a full error. The stored kind is the error's own
/// kind (as before the table existed), so no classification runs and a
/// non-file error (control-state placement) is never counted as an unknown
/// OS code (round 3 item 10).
pub(crate) fn stored_kind_of(error: &io::Error) -> AccessErrorKind {
    stored_kind(error.kind())
}

/// Classify `error` for the current platform. `path` enables the parent
/// listing probe for Windows access denied.
pub(crate) fn classify(error: &io::Error, nt_status: Option<u32>, path: Option<&Path>) -> OsCause {
    classify_on_target(current_platform(), error, nt_status, path)
}

#[cfg(test)]
pub(crate) fn classify_on(
    platform: Platform,
    error: &io::Error,
    nt_status: Option<u32>,
    path: Option<&Path>,
) -> OsCause {
    classify_on_target(Some(platform), error, nt_status, path)
}

fn classify_on_target(
    platform: Option<Platform>,
    error: &io::Error,
    nt_status: Option<u32>,
    path: Option<&Path>,
) -> OsCause {
    let cause = |class, transient, remedy| OsCause {
        class,
        raw_code: error.raw_os_error(),
        nt_status,
        transient,
        remedy,
        kind: error.kind(),
        message: error.to_string(),
    };
    let (Some(code), Some(platform)) = (error.raw_os_error(), platform) else {
        // No raw code, or a target with no table: the error's kind decides.
        let class = match error.kind() {
            io::ErrorKind::NotFound => OsClass::NotFound,
            io::ErrorKind::PermissionDenied => OsClass::Denied,
            io::ErrorKind::OutOfMemory => OsClass::ResourceExhausted,
            _ => OsClass::Refused,
        };
        return cause(class, false, "");
    };
    let Some(row) = lookup(platform, code) else {
        unknown_codes()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .insert((platform, code));
        return cause(OsClass::Unknown, false, UNKNOWN);
    };
    let class = match row.probe {
        Probe::NtStatusThenParentListing if nt_status == Some(STATUS_DELETE_PENDING) => {
            OsClass::DeletePending
        }
        // Any other NT status (normally access denied) still gets the listing.
        Probe::NtStatusThenParentListing if path.is_some_and(parent_listing_lacks) => {
            OsClass::NotFound
        }
        _ => row.class,
    };
    let remedy = match class {
        OsClass::DeletePending => DELETE_PENDING,
        OsClass::NotFound => GONE,
        _ => row.remedy,
    };
    cause(class, row.transient, remedy)
}

/// True when the parent directory lists without `path`'s name.
fn parent_listing_lacks(path: &Path) -> bool {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return false;
    };
    std::fs::read_dir(parent).is_ok_and(|entries| {
        !entries
            .filter_map(Result::ok)
            .any(|entry| entry.file_name() == name)
    })
}

fn unknown_codes() -> &'static Mutex<std::collections::BTreeSet<(Platform, i32)>> {
    static UNKNOWN: std::sync::OnceLock<Mutex<std::collections::BTreeSet<(Platform, i32)>>> =
        std::sync::OnceLock::new();
    UNKNOWN.get_or_init(|| Mutex::new(std::collections::BTreeSet::new()))
}

/// How many distinct raw OS codes the table did not know, seen by this process
/// (test support; health counts them per project from the failure log).
#[cfg(test)]
pub(crate) fn unknown_code_count() -> u64 {
    unknown_codes()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .len() as u64
}

// ponytail: one process-wide map keyed by canonical absolute path, capped;
// a per-project store if health ever needs causes beyond the cap.
const MAX_RECORDED_FAILURES: usize = 4_096;

/// Classified causes of failed reads, keyed by canonical absolute path so one
/// project's record never answers for another's (review M12).
struct FailureLog {
    cap: usize,
    map: HashMap<PathBuf, OsCause>,
    /// A record was refused because the log was full.
    overflowed: bool,
}

/// What health can say about a file's failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RecordedCause {
    Found(OsCause),
    /// This process never recorded a failure for the file.
    NotRecorded,
    /// The file may have failed after the record limit was reached.
    LimitReached,
}

fn failure_key(path: &Path) -> PathBuf {
    dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

// Round 3 item 5: callers compute the key (a canonicalize, file I/O) before
// they take the process-wide lock, so the lock never waits on the disk.
impl FailureLog {
    fn record(&mut self, key: PathBuf, cause: &OsCause) {
        if self.map.len() < self.cap || self.map.contains_key(&key) {
            self.map.insert(key, cause.clone());
        } else {
            self.overflowed = true;
        }
    }

    fn get(&self, key: &Path) -> RecordedCause {
        match self.map.get(key) {
            Some(cause) => RecordedCause::Found(cause.clone()),
            None if self.overflowed => RecordedCause::LimitReached,
            None => RecordedCause::NotRecorded,
        }
    }
}

fn recorded() -> &'static Mutex<FailureLog> {
    static RECORDED: std::sync::OnceLock<Mutex<FailureLog>> = std::sync::OnceLock::new();
    RECORDED.get_or_init(|| {
        Mutex::new(FailureLog {
            cap: MAX_RECORDED_FAILURES,
            map: HashMap::new(),
            overflowed: false,
        })
    })
}

/// Remember the classified cause of a failed read so health can name it; the
/// persisted disposition keeps only the coarse kind.
pub(crate) fn record_failure(path: &Path, cause: &OsCause) {
    let key = failure_key(path);
    recorded()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .record(key, cause);
}

pub(crate) fn recorded_failure(path: &Path) -> Option<OsCause> {
    let key = failure_key(path);
    match recorded()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .get(&key)
    {
        RecordedCause::Found(cause) => Some(cause),
        RecordedCause::NotRecorded | RecordedCause::LimitReached => None,
    }
}

/// The recorded cause for `relative` under exactly `root`.
pub(crate) fn recorded_failure_for(root: Option<&Path>, relative: &str) -> RecordedCause {
    let Some(root) = root else {
        return RecordedCause::NotRecorded;
    };
    let key = failure_key(&root.join(relative));
    recorded()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .get(&key)
}

/// The time one tool call may spend retrying transient failures, shared by
/// every file the call touches (FR-020).
pub(crate) const CALL_RETRY_BUDGET: Duration = Duration::from_secs(1);

/// A retry deadline. Shared by reference, so one budget spans many files.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RetryBudget {
    deadline: Instant,
}

impl RetryBudget {
    pub(crate) fn new(span: Duration) -> Self {
        Self {
            deadline: Instant::now() + span,
        }
    }

    pub(crate) fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }
}

thread_local! {
    static CALL_BUDGET: std::cell::Cell<Option<RetryBudget>> = const { std::cell::Cell::new(None) };
}

/// Holds one retry budget for every file access on this thread until dropped,
/// including when the work panics (review M10). Nested guards keep the outer
/// budget.
#[must_use]
pub(crate) struct CallBudgetGuard {
    installed: bool,
}

pub(crate) fn enter_call_budget() -> CallBudgetGuard {
    let installed = CALL_BUDGET.with(|slot| {
        if slot.get().is_none() {
            slot.set(Some(RetryBudget::new(CALL_RETRY_BUDGET)));
            true
        } else {
            false
        }
    });
    CallBudgetGuard { installed }
}

impl Drop for CallBudgetGuard {
    fn drop(&mut self) {
        if self.installed {
            CALL_BUDGET.with(|slot| slot.set(None));
        }
    }
}

/// Run `f` under a call budget that is already spent, as after a locked file
/// used it up (test support).
#[cfg(test)]
pub(crate) fn with_spent_call_budget<T>(f: impl FnOnce() -> T) -> T {
    CALL_BUDGET.with(|slot| slot.set(Some(RetryBudget::new(Duration::ZERO))));
    let result = f();
    CALL_BUDGET.with(|slot| slot.set(None));
    result
}

/// Run `f` under one call budget ([`enter_call_budget`]).
pub(crate) fn with_call_budget<T>(f: impl FnOnce() -> T) -> T {
    let _guard = enter_call_budget();
    f()
}

/// The enclosing call's budget, or a fresh one for an access made outside a
/// tool call.
pub(crate) fn current_budget() -> RetryBudget {
    CALL_BUDGET
        .with(std::cell::Cell::get)
        .unwrap_or_else(|| RetryBudget::new(CALL_RETRY_BUDGET))
}

/// Run `op`, retrying transient failures with a doubling backoff until
/// `budget` runs out. Returns the classified cause of the last failure.
pub(crate) fn retry_transient<T>(
    budget: &RetryBudget,
    path: Option<&Path>,
    mut op: impl FnMut() -> Result<T, AccessFailure>,
) -> Result<T, OsCause> {
    let mut backoff = Duration::from_millis(10);
    loop {
        let failure = match op() {
            Ok(value) => return Ok(value),
            Err(failure) => failure,
        };
        let cause = classify(&failure.error, failure.nt_status, path);
        let remaining = budget.remaining();
        if !cause.transient || remaining.is_zero() {
            return Err(cause);
        }
        std::thread::sleep(backoff.min(remaining));
        backoff = backoff.saturating_mul(2);
    }
}

#[cfg(test)]
mod tests {
    use super::Platform::Windows;
    use super::*;

    fn raw(code: i32) -> io::Error {
        io::Error::from_raw_os_error(code)
    }

    #[test]
    fn every_row_is_unique_per_platform_and_code() {
        for (index, row) in WINDOWS_RESPONSES.iter().enumerate() {
            assert!(
                WINDOWS_RESPONSES[index + 1..]
                    .iter()
                    .all(|other| (other.platform, other.code) != (row.platform, row.code)),
                "duplicate row {row:?}"
            );
            assert!(!row.remedy.is_empty(), "row without remedy {row:?}");
        }
    }

    #[test]
    fn windows_access_denied_is_told_apart_from_delete_pending() {
        let denied = classify_on(Windows, &raw(5), Some(0xC000_0022), None);
        assert_eq!(denied.class, OsClass::Denied);
        let pending = classify_on(Windows, &raw(5), Some(STATUS_DELETE_PENDING), None);
        assert_eq!(pending.class, OsClass::DeletePending);
        assert_eq!(pending.stored_kind(), AccessErrorKind::PermissionDenied);
        assert!(pending.describe().starts_with("pending deletion"));
    }

    #[test]
    fn parent_listing_without_the_name_reads_as_gone() {
        let dir = tempfile::tempdir().unwrap();
        let present = dir.path().join("present.rs");
        std::fs::write(&present, b"x").unwrap();
        let absent = dir.path().join("absent.rs");
        assert_eq!(
            classify_on(Windows, &raw(5), None, Some(&present)).class,
            OsClass::Denied
        );
        assert_eq!(
            classify_on(Windows, &raw(5), None, Some(&absent)).class,
            OsClass::NotFound
        );
    }

    #[test]
    fn unknown_code_is_reported_verbatim_and_counted() {
        let before = unknown_code_count();
        let cause = classify_on(Platform::Unix, &raw(777_777), None, None);
        assert_eq!(cause.class, OsClass::Unknown);
        assert!(cause.describe().contains("777777"), "{}", cause.describe());
        assert!(unknown_code_count() > before);
        let known = classify_on(Windows, &raw(5), Some(0), None);
        assert_eq!(known.class, OsClass::Denied);
    }

    /// Review M13: a directory replaced by a file reads as gone; Windows
    /// delete-pending has its own code; 1920 is not an access-rights problem.
    #[test]
    fn review_m13_table_rows() {
        #[cfg(unix)]
        assert_eq!(
            classify_on(Platform::Unix, &raw(libc::ENOTDIR), None, None).class,
            OsClass::NotFound
        );
        assert_eq!(
            classify_on(Windows, &raw(303), None, None).class,
            OsClass::DeletePending
        );
        assert_eq!(
            classify_on(Windows, &raw(267), None, None).class,
            OsClass::NotFound
        );
        assert_eq!(
            classify_on(Windows, &raw(123), None, None).class,
            OsClass::NotFound
        );
        assert!(
            !classify_on(Windows, &raw(1920), None, None)
                .remedy
                .contains("Grant read access")
        );
        // Control: access denied keeps its class and remedy.
        assert_eq!(
            classify_on(Windows, &raw(5), Some(0), None).class,
            OsClass::Denied
        );
    }

    /// Review M12: one project's recorded cause never answers for another's.
    #[test]
    fn recorded_causes_are_keyed_by_root() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let cause = classify_on(Windows, &raw(32), None, None);
        record_failure(&a.path().join("src/m12_only_in_a.rs"), &cause);
        assert!(matches!(
            recorded_failure_for(Some(a.path()), "src/m12_only_in_a.rs"),
            RecordedCause::Found(_)
        ));
        assert!(
            recorded_failure_for(Some(b.path()), "src/m12_only_in_a.rs")
                == RecordedCause::NotRecorded,
            "project B must not see project A's cause"
        );
    }

    /// Review L15: unknown codes are counted once per distinct code.
    #[test]
    fn unknown_codes_count_distinct_codes() {
        let before = unknown_code_count();
        let _ = classify_on(Windows, &raw(777_015), None, None);
        let _ = classify_on(Windows, &raw(777_015), None, None);
        assert_eq!(unknown_code_count(), before + 1);
    }

    /// Review M10: a panic inside a call leaves no budget on the thread.
    #[test]
    fn call_budget_is_cleared_after_a_panic() {
        let result = std::panic::catch_unwind(|| with_call_budget(|| panic!("inside the call")));
        assert!(result.is_err());
        assert!(
            CALL_BUDGET.with(std::cell::Cell::get).is_none(),
            "an expired budget must not outlive the call"
        );
        // Control: a normal call also leaves none.
        with_call_budget(|| ());
        assert!(CALL_BUDGET.with(std::cell::Cell::get).is_none());
    }

    /// Review round 3 item 10: storing the kind of a non-file error (control
    /// state placement) never counts as an unknown OS code.
    #[test]
    fn storing_a_kind_counts_no_unknown_code() {
        let before = unknown_code_count();
        let _ = stored_kind_of(&raw(777_001));
        assert_eq!(unknown_code_count(), before);
        // Control: classifying the same error does count it.
        let _ = classify(&raw(777_001), None, None);
        assert_eq!(unknown_code_count(), before + 1);
    }

    /// Review M12: a full log says so instead of "not observed".
    #[test]
    fn a_full_log_reports_the_limit() {
        let mut log = FailureLog {
            cap: 1,
            map: HashMap::new(),
            overflowed: false,
        };
        let cause = classify_on(Windows, &raw(32), None, None);
        log.record(PathBuf::from("first_m12.rs"), &cause);
        assert_eq!(
            log.get(Path::new("never_m12.rs")),
            RecordedCause::NotRecorded
        );
        log.record(PathBuf::from("second_m12.rs"), &cause);
        assert_eq!(
            log.get(Path::new("second_m12.rs")),
            RecordedCause::LimitReached
        );
        assert!(matches!(
            log.get(Path::new("first_m12.rs")),
            RecordedCause::Found(_)
        ));
    }

    #[test]
    fn stored_kind_matches_the_pre_table_mapping() {
        for (kind, stored) in [
            (io::ErrorKind::NotFound, AccessErrorKind::NotFound),
            (
                io::ErrorKind::PermissionDenied,
                AccessErrorKind::PermissionDenied,
            ),
            (io::ErrorKind::InvalidInput, AccessErrorKind::InvalidData),
            (
                io::ErrorKind::OutOfMemory,
                AccessErrorKind::ResourceExhausted,
            ),
            (io::ErrorKind::Other, AccessErrorKind::Other),
        ] {
            assert_eq!(stored_kind_of(&io::Error::new(kind, "x")), stored);
        }
    }
}
