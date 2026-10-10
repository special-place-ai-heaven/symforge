//! Shared staged multi-file commit and rollback ordering for MCP and Embed.
//! Callers own source authority and receipts; this kernel never claims an
//! all-or-nothing result when a write or rollback cannot be attested.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use super::atomic_write::lock_for_path;

pub(crate) struct StagedImage {
    pub relative: PathBuf,
    pub absolute: PathBuf,
    /// `None` means the target must still be absent when the source permit is acquired.
    pub original: Option<Vec<u8>>,
    pub replacement: Vec<u8>,
    /// Plaintext targets require a private temp before any bytes are written.
    pub owner_only: bool,
}

pub(crate) trait BatchIo {
    type Report;

    fn matches(&mut self, image: &StagedImage, expected: Option<&[u8]>) -> Result<bool, String>;
    fn write(&mut self, image: &StagedImage, bytes: &[u8]) -> Result<Self::Report, String>;
}

#[derive(Debug)]
pub(crate) enum BatchAbort {
    Conflict {
        index: usize,
    },
    PreflightUnavailable {
        index: usize,
    },
    Cancelled {
        attempted: usize,
        restored: usize,
        uncertain: Vec<usize>,
    },
    WriteFailed {
        index: usize,
        reason: String,
        restored: usize,
        uncertain: Vec<usize>,
    },
}

impl BatchAbort {
    pub(crate) fn no_source_write(&self) -> bool {
        matches!(
            self,
            Self::Conflict { .. } | Self::PreflightUnavailable { .. }
        ) || matches!(
            self,
            Self::Cancelled {
                attempted: 0,
                restored: 0,
                uncertain,
            } if uncertain.is_empty()
        )
    }

    pub(crate) fn rollback_uncertain(&self) -> bool {
        matches!(
            self,
            Self::Cancelled { uncertain, .. } | Self::WriteFailed { uncertain, .. }
                if !uncertain.is_empty()
        )
    }
}

/// Lock every canonical target in sorted order, verify all base images, then
/// write. On any later failure, inspect each attempted path before rollback:
/// an unexpected third image is left untouched and reported uncertain.
pub(crate) fn commit_staged<I: BatchIo>(
    images: &[StagedImage],
    io: &mut I,
    cancel: Option<&AtomicBool>,
) -> Result<Vec<(usize, I::Report)>, BatchAbort> {
    with_staged_locks(images, |order| {
        commit_staged_locked(images, order, io, cancel)
    })?
}

/// Run a staged operation while holding every target mutex in canonical order.
/// The caller may acquire the source mutation permit within this closure,
/// matching the single-file writer's path-lock-before-permit order.
pub(crate) fn with_staged_locks<R>(
    images: &[StagedImage],
    operation: impl FnOnce(&[(PathBuf, usize)]) -> R,
) -> Result<R, BatchAbort> {
    let mut order = images
        .iter()
        .enumerate()
        .map(|(index, image)| {
            canonical_lock_key(&image.absolute)
                .map(|path| (path, index))
                .map_err(|_| BatchAbort::PreflightUnavailable { index })
        })
        .collect::<Result<Vec<_>, _>>()?;
    order.sort_by(|left, right| left.0.cmp(&right.0));
    if let Some(pair) = order.windows(2).find(|pair| pair[0].0 == pair[1].0) {
        return Err(BatchAbort::Conflict { index: pair[1].1 });
    }
    let locks = order
        .iter()
        .map(|(path, _)| lock_for_path(path))
        .collect::<Vec<_>>();
    let _guards = locks
        .iter()
        .map(|lock| lock.lock().expect("batch path lock poisoned"))
        .collect::<Vec<_>>();
    Ok(operation(&order))
}

/// Canonicalize the nearest existing ancestor, then append every absent path
/// component. This only chooses an in-process mutex key; the pinned source
/// lease still verifies all components and creates missing parents at write.
fn canonical_lock_key(absolute: &std::path::Path) -> std::io::Result<PathBuf> {
    let mut cursor = absolute;
    let mut absent = Vec::new();
    loop {
        match dunce::canonicalize(cursor) {
            Ok(mut canonical) => {
                for component in absent.iter().rev() {
                    canonical.push(component);
                }
                return Ok(canonical);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let component = cursor.file_name().ok_or(error)?;
                absent.push(component.to_os_string());
                cursor = cursor.parent().ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::NotFound, "no canonical ancestor")
                })?;
            }
            Err(error) => return Err(error),
        }
    }
}

/// Must only be called from `with_staged_locks` for these exact images.
pub(crate) fn commit_staged_locked<I: BatchIo>(
    images: &[StagedImage],
    order: &[(PathBuf, usize)],
    io: &mut I,
    cancel: Option<&AtomicBool>,
) -> Result<Vec<(usize, I::Report)>, BatchAbort> {
    for (_, index) in order {
        match io.matches(&images[*index], images[*index].original.as_deref()) {
            Ok(true) => {}
            Ok(false) => return Err(BatchAbort::Conflict { index: *index }),
            Err(_) => return Err(BatchAbort::PreflightUnavailable { index: *index }),
        }
    }
    let mut reports = Vec::with_capacity(images.len());
    let mut attempted = Vec::new();
    for (_, index) in order {
        if cancel.is_some_and(|signal| signal.load(Ordering::Acquire)) {
            let (restored, uncertain) = rollback(images, io, &attempted);
            return Err(BatchAbort::Cancelled {
                attempted: attempted.len(),
                restored,
                uncertain,
            });
        }
        attempted.push(*index);
        match io.write(&images[*index], &images[*index].replacement) {
            Ok(report) => {
                reports.push((*index, report));
                if !matches!(
                    io.matches(&images[*index], Some(&images[*index].replacement)),
                    Ok(true)
                ) {
                    let (restored, uncertain) = rollback(images, io, &attempted);
                    return Err(BatchAbort::WriteFailed {
                        index: *index,
                        reason: "post_image_unverified".to_string(),
                        restored,
                        uncertain,
                    });
                }
            }
            Err(reason) => {
                let (restored, uncertain) = rollback(images, io, &attempted);
                return Err(BatchAbort::WriteFailed {
                    index: *index,
                    reason,
                    restored,
                    uncertain,
                });
            }
        }
    }
    // A later write or an external actor can change an earlier target after its
    // per-write check. Re-attest the complete postimage set before committing.
    for (_, index) in order {
        if !matches!(
            io.matches(&images[*index], Some(&images[*index].replacement)),
            Ok(true)
        ) {
            let (restored, uncertain) = rollback(images, io, &attempted);
            return Err(BatchAbort::WriteFailed {
                index: *index,
                reason: "batch_post_image_unverified".to_string(),
                restored,
                uncertain,
            });
        }
    }
    Ok(reports)
}

fn rollback<I: BatchIo>(
    images: &[StagedImage],
    io: &mut I,
    attempted: &[usize],
) -> (usize, Vec<usize>) {
    let mut restored = 0;
    let mut uncertain = Vec::new();
    for index in attempted.iter().rev() {
        let image = &images[*index];
        if matches!(io.matches(image, image.original.as_deref()), Ok(true)) {
            continue;
        }
        if !matches!(io.matches(image, Some(&image.replacement)), Ok(true)) {
            uncertain.push(*index);
            continue;
        }
        let Some(original) = image.original.as_deref() else {
            // A created file cannot be restored with a replacement write. Keep
            // the known postimage and report an explicit partial outcome.
            uncertain.push(*index);
            continue;
        };
        let _ = io.write(image, original);
        if matches!(io.matches(image, Some(original)), Ok(true)) {
            restored += 1;
        } else {
            uncertain.push(*index);
        }
    }
    (restored, uncertain)
}

#[cfg(feature = "server")]
pub(crate) struct ProtocolBatchIo<'a> {
    pub repo_root: &'a std::path::Path,
    pub project_state_dir: Option<&'a crate::domain::ProjectStateDir>,
}

#[cfg(feature = "server")]
impl BatchIo for ProtocolBatchIo<'_> {
    type Report = super::atomic_write::AtomicWriteReport;

    fn matches(&mut self, image: &StagedImage, expected: Option<&[u8]>) -> Result<bool, String> {
        let expected = expected.ok_or_else(|| "batch_absent_target_unsupported".to_string())?;
        std::fs::read(&image.absolute)
            .map(|bytes| bytes == expected)
            .map_err(|_| "batch_preimage_unreadable".to_string())
    }

    fn write(&mut self, image: &StagedImage, bytes: &[u8]) -> Result<Self::Report, String> {
        super::atomic_write::atomic_write_file(
            self.repo_root,
            self.project_state_dir,
            &image.absolute,
            bytes,
        )
        .map_err(|error| error.to_string())
    }
}

pub(crate) struct EmbeddedBatchIo {
    write: crate::live_index::index_lifecycle::activation::WriteAuthority,
    last_receipt: Option<crate::live_index::index_lifecycle::physical_root::WriteReceipt>,
}

impl EmbeddedBatchIo {
    pub(crate) fn new(
        write: crate::live_index::index_lifecycle::activation::WriteAuthority,
    ) -> Self {
        Self {
            write,
            last_receipt: None,
        }
    }

    pub(crate) fn finish(mut self) -> Result<(), String> {
        match self.last_receipt.take() {
            Some(receipt) => self.write.finish_committed(receipt),
            None => self.write.finish_no_side_effect(),
        }
        .map(|_| ())
        .map_err(|_| "batch_source_publication_uncertain".to_string())
    }
}

impl BatchIo for EmbeddedBatchIo {
    type Report = ();

    fn matches(&mut self, image: &StagedImage, expected: Option<&[u8]>) -> Result<bool, String> {
        self.write
            .matches_optional_beneath(&image.relative, expected)
            .map_err(|_| "batch_source_image_unavailable".to_string())
    }

    fn write(&mut self, image: &StagedImage, bytes: &[u8]) -> Result<Self::Report, String> {
        let receipt = if image.owner_only {
            self.write.write_owner_only(&image.relative, bytes)
        } else {
            self.write.write(&image.relative, bytes)
        }
        .map_err(|_| "batch_source_write_uncertain".to_string())?;
        self.last_receipt = Some(receipt);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use super::*;

    struct MemoryIo {
        files: BTreeMap<PathBuf, Vec<u8>>,
        fail_on: PathBuf,
        leave_third_image: bool,
    }

    impl BatchIo for MemoryIo {
        type Report = ();

        fn matches(
            &mut self,
            image: &StagedImage,
            expected: Option<&[u8]>,
        ) -> Result<bool, String> {
            Ok(self.files.get(&image.absolute).map(Vec::as_slice) == expected)
        }

        fn write(&mut self, image: &StagedImage, bytes: &[u8]) -> Result<Self::Report, String> {
            if image.absolute == self.fail_on && bytes == image.replacement {
                if self.leave_third_image {
                    self.files
                        .insert(image.absolute.clone(), b"third image".to_vec());
                }
                return Err("injected failure".to_string());
            }
            self.files.insert(image.absolute.clone(), bytes.to_vec());
            Ok(())
        }
    }

    fn image(root: &Path, name: &str) -> StagedImage {
        let absolute = root.join(name);
        std::fs::write(&absolute, b"original").expect("create canonical target");
        StagedImage {
            relative: PathBuf::from(name),
            absolute,
            original: Some(b"original".to_vec()),
            replacement: b"replacement".to_vec(),
            owner_only: false,
        }
    }

    #[test]
    fn second_write_failure_rolls_back_first_without_claiming_commit() {
        let root = tempfile::tempdir().expect("temporary root");
        let first = image(root.path(), "first.rs");
        let second = image(root.path(), "second.rs");
        let mut io = MemoryIo {
            files: [
                (first.absolute.clone(), first.original.clone().unwrap()),
                (second.absolute.clone(), second.original.clone().unwrap()),
            ]
            .into(),
            fail_on: second.absolute.clone(),
            leave_third_image: false,
        };
        let result =
            commit_staged(&[first, second], &mut io, None).expect_err("second write must fail");
        assert!(
            matches!(result, BatchAbort::WriteFailed { restored: 1, ref uncertain, .. } if uncertain.is_empty())
        );
        assert!(
            io.files
                .values()
                .all(|bytes| bytes.as_slice() == b"original")
        );
    }

    #[test]
    fn third_image_is_never_overwritten_during_rollback() {
        let root = tempfile::tempdir().expect("temporary root");
        let first = image(root.path(), "first.rs");
        let second = image(root.path(), "second.rs");
        let mut io = MemoryIo {
            files: [
                (first.absolute.clone(), first.original.clone().unwrap()),
                (second.absolute.clone(), second.original.clone().unwrap()),
            ]
            .into(),
            fail_on: second.absolute.clone(),
            leave_third_image: true,
        };
        let second_path = second.absolute.clone();
        let result =
            commit_staged(&[first, second], &mut io, None).expect_err("second write must fail");
        assert!(result.rollback_uncertain());
        assert_eq!(
            io.files.get(&second_path).expect("second image").as_slice(),
            b"third image"
        );
    }

    #[test]
    fn created_target_is_preflighted_and_partial_creation_is_reported_uncertain() {
        let root = tempfile::tempdir().expect("temporary root");
        let created = StagedImage {
            relative: PathBuf::from("a-new-dir/a-new-file"),
            absolute: root.path().join("a-new-dir/a-new-file"),
            original: None,
            replacement: b"created".to_vec(),
            owner_only: false,
        };
        let failing = image(root.path(), "z-existing-file");
        let mut io = MemoryIo {
            files: [(failing.absolute.clone(), failing.original.clone().unwrap())].into(),
            fail_on: failing.absolute.clone(),
            leave_third_image: false,
        };
        let created_path = created.absolute.clone();
        let error = commit_staged(&[created, failing], &mut io, None)
            .expect_err("second write fails after creating absent target");
        assert!(
            matches!(error, BatchAbort::WriteFailed { ref uncertain, .. } if uncertain == &[0])
        );
        assert_eq!(
            io.files.get(&created_path).map(Vec::as_slice),
            Some(b"created".as_slice())
        );
        assert_eq!(
            canonical_lock_key(&created_path).expect("absent parent lock key"),
            dunce::canonicalize(root.path())
                .expect("canonical root")
                .join("a-new-dir")
                .join("a-new-file")
        );
    }
}
