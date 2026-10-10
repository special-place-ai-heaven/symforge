#![cfg(feature = "embed")]

use std::fs;
use std::sync::{Arc, atomic::AtomicBool};
use std::time::{Duration, Instant};

use symforge::embed::parity::host::OperationControl;
use symforge::embed::parity::remediation::{
    SecretApplyAuthority, SecretApplyStatus, SecretRemediationAction, SecretRemediationError,
    SecretRemediationRefusalKind, SecretRemediationRequest, SecretRemediationScope,
    SecretScanLimits,
};
use symforge::embed::{
    EmbeddedSourceHandle, EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase,
};

const SYNTHETIC_VALUE: &str = "S3cretValue9xAb";

fn fixture() -> (tempfile::TempDir, ProcessIndexRuntime, EmbeddedSourceHandle) {
    let repository = tempfile::tempdir().expect("temporary repository");
    git2::Repository::init(repository.path()).expect("initialize repository");
    fs::write(
        repository.path().join("config.json"),
        format!("{{\n  \"password\": \"{SYNTHETIC_VALUE}\"\n}}\n"),
    )
    .expect("write synthetic source");
    let runtime = ProcessIndexRuntime::acquire().expect("acquire runtime");
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(
            repository.path().to_path_buf(),
        ))
        .expect("open source");
    let deadline = Instant::now() + Duration::from_secs(15);
    while handle.runtime_view().phase != SourceRuntimePhase::Current {
        assert!(Instant::now() < deadline, "source did not publish");
        std::thread::sleep(Duration::from_millis(10));
    }
    (repository, runtime, handle)
}

fn request(handle: &EmbeddedSourceHandle) -> SecretRemediationRequest {
    let scope = SecretRemediationScope::Paths(vec!["config.json".to_owned()]);
    let claim = handle
        .inspect_secret_findings(&scope, SecretScanLimits::default(), None, None)
        .expect("inspect redacted findings");
    assert_eq!(claim.findings.len(), 1);
    assert_eq!(claim.findings[0].path, "config.json");
    let wire = serde_json::to_string(&claim).expect("serialize finding claim");
    assert!(!wire.contains(SYNTHETIC_VALUE));
    SecretRemediationRequest {
        scope,
        finding_ids: vec![claim.findings[0].id.clone()],
        action: SecretRemediationAction::Externalize,
    }
}

#[test]
fn discovery_and_preview_are_redacted_and_cancellation_prevents_write() {
    let (repository, _runtime, handle) = fixture();
    let request = request(&handle);
    let preview = handle
        .preview_secret_remediation(&request, SecretScanLimits::default(), None)
        .expect("preview exact finding");
    let wire = serde_json::to_string(&preview).expect("serialize preview");
    assert!(!wire.contains(SYNTHETIC_VALUE));
    assert_eq!(
        preview.would_write,
        vec![
            ".env".to_owned(),
            ".gitignore".to_owned(),
            "config.json".to_owned()
        ]
    );
    assert!(!repository.path().join(".env").exists());

    let cancelled = Arc::new(AtomicBool::new(true));
    let authority = SecretApplyAuthority::for_source_root(
        fs::canonicalize(repository.path()).expect("canonical root"),
        "fixture-room/source".to_owned(),
        cancelled,
        SecretScanLimits::default(),
        None,
        None,
    )
    .expect("host authority");
    let error = handle
        .apply_secret_remediation(&preview, &authority, "cancelled-op")
        .expect_err("cancelled apply must refuse");
    assert!(matches!(
        error,
        SecretRemediationError::Refused(SecretRemediationRefusalKind::WriteAuthorityRefused)
            | SecretRemediationError::Refused(SecretRemediationRefusalKind::Cancelled)
    ));
    assert!(!repository.path().join(".env").exists());
}

#[test]
fn exact_scoped_apply_replays_only_while_postimages_match() {
    let (repository, _runtime, handle) = fixture();
    let request = request(&handle);
    let preview = handle
        .preview_secret_remediation(&request, SecretScanLimits::default(), None)
        .expect("preview exact finding");
    let root = fs::canonicalize(repository.path()).expect("canonical root");
    let authority = SecretApplyAuthority::for_source_root(
        root.clone(),
        "fixture-room/source".to_owned(),
        Arc::new(AtomicBool::new(false)),
        SecretScanLimits::default(),
        None,
        None,
    )
    .expect("host authority");
    let applied = handle
        .apply_secret_remediation(&preview, &authority, "externalize-op")
        .expect("guarded externalize");
    assert!(!applied.replayed);
    assert_eq!(applied.status, SecretApplyStatus::Clean);
    assert!(
        !fs::read_to_string(root.join("config.json"))
            .expect("read rewritten source")
            .contains(SYNTHETIC_VALUE)
    );
    assert!(
        fs::read_to_string(root.join(".env"))
            .expect("read private environment file")
            .contains(SYNTHETIC_VALUE)
    );
    assert!(
        fs::read_to_string(root.join(".gitignore"))
            .expect("read ignore file")
            .lines()
            .any(|line| line == ".env")
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(root.join(".env"))
                .expect("environment file metadata")
                .permissions()
                .mode()
                & 0o077,
            0,
            "environment file must be owner-only at publication"
        );
    }

    let replayed = handle
        .apply_secret_remediation(&preview, &authority, "externalize-op")
        .expect("verified postimage replay");
    assert!(replayed.replayed);

    fs::write(root.join(".gitignore"), b"changed by another actor\n")
        .expect("change one postimage");
    let error = handle
        .apply_secret_remediation(&preview, &authority, "externalize-op")
        .expect_err("changed postimage must block replay");
    assert!(matches!(
        error,
        SecretRemediationError::Refused(SecretRemediationRefusalKind::ReplayConflict)
    ));
}

#[test]
fn authority_scope_is_bound_to_replay_and_stale_source_refuses() {
    let (repository, _runtime, handle) = fixture();
    let request = request(&handle);
    let preview = handle
        .preview_secret_remediation(&request, SecretScanLimits::default(), None)
        .expect("preview exact finding");
    let root = fs::canonicalize(repository.path()).expect("canonical root");
    let other_root = tempfile::tempdir().expect("other root");
    let wrong_authority = SecretApplyAuthority::for_source_root(
        fs::canonicalize(other_root.path()).expect("canonical other root"),
        "fixture-room/source".to_owned(),
        Arc::new(AtomicBool::new(false)),
        SecretScanLimits::default(),
        None,
        None,
    )
    .expect("host authority");
    let error = handle
        .apply_secret_remediation(&preview, &wrong_authority, "wrong-root-op")
        .expect_err("another root cannot write");
    assert!(matches!(
        error,
        SecretRemediationError::Refused(SecretRemediationRefusalKind::WriteAuthorityRefused)
    ));

    fs::write(
        root.join("config.json"),
        format!("{{\n  \"password\": \"{SYNTHETIC_VALUE}Z\"\n}}\n"),
    )
    .expect("change guarded source");
    let authority = SecretApplyAuthority::for_source_root(
        root.clone(),
        "fixture-room/source".to_owned(),
        Arc::new(AtomicBool::new(false)),
        SecretScanLimits::default(),
        None,
        None,
    )
    .expect("host authority");
    let error = handle
        .apply_secret_remediation(&preview, &authority, "stale-op")
        .expect_err("stale source cannot write");
    assert!(matches!(
        error,
        SecretRemediationError::Refused(SecretRemediationRefusalKind::StaleContent)
            | SecretRemediationError::Refused(SecretRemediationRefusalKind::StalePublication)
    ));
    assert!(!root.join(".env").exists());
}

#[test]
fn scan_byte_budget_and_deadline_refuse_without_partial_claims_or_writes() {
    let (repository, _runtime, handle) = fixture();
    let scope = SecretRemediationScope::Paths(vec!["config.json".to_owned()]);
    let tiny = SecretScanLimits {
        max_total_bytes: 1,
        ..SecretScanLimits::default()
    };
    let error = handle
        .inspect_secret_findings(&scope, tiny, None, None)
        .expect_err("one-byte total cap must refuse");
    assert!(matches!(
        error,
        SecretRemediationError::Refused(SecretRemediationRefusalKind::ResourceLimit)
    ));

    let request = request(&handle);
    let error = handle
        .preview_secret_remediation(&request, tiny, None)
        .expect_err("preview must share total cap");
    assert!(matches!(
        error,
        SecretRemediationError::Refused(SecretRemediationRefusalKind::ResourceLimit)
    ));
    let preview = handle
        .preview_secret_remediation(&request, SecretScanLimits::default(), None)
        .expect("bounded preview");
    let authority = SecretApplyAuthority::for_source_root(
        fs::canonicalize(repository.path()).expect("canonical root"),
        "fixture-room/limited".to_owned(),
        Arc::new(AtomicBool::new(false)),
        tiny,
        None,
        None,
    )
    .expect("host authority");
    let error = handle
        .apply_secret_remediation(&preview, &authority, "limited-op")
        .expect_err("apply reselection must honor the same cap");
    assert!(matches!(
        error,
        SecretRemediationError::Refused(SecretRemediationRefusalKind::ResourceLimit)
    ));
    assert!(!repository.path().join(".env").exists());

    let control = OperationControl::new(Duration::from_millis(1)).expect("host deadline");
    std::thread::sleep(Duration::from_millis(5));
    let error = handle
        .inspect_secret_findings(&scope, SecretScanLimits::default(), Some(&control), None)
        .expect_err("expired scan control must refuse");
    assert!(matches!(
        error,
        SecretRemediationError::Refused(SecretRemediationRefusalKind::DeadlineExceeded)
    ));
}
