//! Backing placement and cleanup custody of initial acquisition.
//!
//! A backing inside the source is refused before the source changes or an
//! operation begins. Working rows and the operation record are removed before
//! anything is published; a removal that fails is reported with its own cause
//! and what is still held; an unknown outcome leaves everything untouched.
#![cfg(unix)]
mod support;
use layerfs_history::{HistoryCatalog, HistoryName, LayerStackId};
use layerfs_project::{init, InitRequest, Initialized, ProjectError};
use layerfs_storage::port::{
    acquisition::{AcquisitionError, Phase},
    PersistenceError,
};
use layerfs_telemetry::timer::Timing;
use std::{
    fs,
    os::unix::fs::{symlink, MetadataExt, PermissionsExt},
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use support::{
    memory_acquisition::MemoryAcquisition, memory_history::MemoryHistory,
    memory_metadata::MemoryMetadata, Fixture,
};

fn stack() -> LayerStackId {
    LayerStackId::from_authority([19; 16])
}

fn attempt(
    fixture: &Fixture,
    acquisition: &MemoryAcquisition,
    history: &MemoryHistory,
) -> Result<Initialized, ProjectError> {
    let store = support::storage(Arc::new(MemoryMetadata::default()));
    Timing::disabled("project.init", |scope| {
        init(
            &store,
            history,
            InitRequest {
                source: &fixture.source,
                acquisition,
                stack: stack(),
                name: HistoryName::new("main").unwrap(),
                scope_seed: [29; 32],
                deadline: Instant::now() + Duration::from_secs(60),
            },
            scope,
        )
    })
    .0
}

fn refused() -> AcquisitionError {
    AcquisitionError::Persistence(PersistenceError::Refused {
        status: "injected".into(),
    })
}

/// Names and change evidence of every entry directly inside `directory`.
fn observed(directory: &Path) -> Vec<(std::ffi::OsString, i64, i64, i64, i64)> {
    let mut seen: Vec<_> = fs::read_dir(directory)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            let metadata = fs::symlink_metadata(entry.path()).unwrap();
            (
                entry.file_name(),
                metadata.mtime(),
                metadata.mtime_nsec(),
                metadata.ctime(),
                metadata.ctime_nsec(),
            )
        })
        .collect();
    seen.sort();
    seen
}

#[test]
fn backing_inside_source_is_refused_before_an_operation_begins() {
    let fixture = Fixture::new(0);
    let nested = fixture.source.join("nested/deeper");
    fs::create_dir_all(&nested).unwrap();
    // An outside spelling of a directory inside the source, and one that leaves
    // and re-enters the source through its parent.
    let alias = fixture.path.join("alias");
    symlink(&nested, &alias).unwrap();
    let detour = fixture.source.join("nested/../nested");
    let root_before = fs::symlink_metadata(&fixture.source).unwrap();
    let inside_before = observed(&fixture.source.join("nested"));
    let history = MemoryHistory::default();

    for placement in [&fixture.source, &nested, &alias, &detour] {
        let acquisition = MemoryAcquisition::placed_in(placement.clone());
        let error = attempt(&fixture, &acquisition, &history).unwrap_err();
        assert!(
            matches!(error, ProjectError::BackingInsideSource),
            "{placement:?}: {error:?}"
        );
        assert_eq!(acquisition.issued(), 0, "no operation is begun");
    }
    let root_after = fs::symlink_metadata(&fixture.source).unwrap();
    assert_eq!(
        (root_after.mtime(), root_after.mtime_nsec()),
        (root_before.mtime(), root_before.mtime_nsec()),
        "a refused placement must not touch the source directory"
    );
    assert_eq!(
        (root_after.ctime(), root_after.ctime_nsec()),
        (root_before.ctime(), root_before.ctime_nsec())
    );
    assert_eq!(observed(&fixture.source.join("nested")), inside_before);
    assert!(history.layer_stack(stack()).unwrap().is_none());

    // The same source is acquired once its backing is placed beside it: the
    // root, `nested` and `deeper`.
    let beside = MemoryAcquisition::placed_in(fixture.path.clone());
    let initialized = attempt(&fixture, &beside, &history).unwrap();
    assert_eq!(initialized.entries, 3);
    assert_eq!(initialized.namespace_work.entries, 3);
    assert!(beside.operations().is_empty());
}

#[test]
fn a_failed_cleanup_reports_its_cause_and_the_rows_still_held() {
    // 3011 entries and 3000 native identities: two removal jobs.
    let fixture = Fixture::new(3000);
    let history = MemoryHistory::default();
    let acquisition = MemoryAcquisition::default();
    acquisition.fail("discard", 2, refused());
    let outcome = attempt(&fixture, &acquisition, &history);
    let Err(ProjectError::Cleanup {
        cause,
        error,
        retained,
    }) = outcome
    else {
        panic!("expected a cleanup failure, found {outcome:?}");
    };
    assert!(cause.is_none(), "construction itself succeeded: {cause:?}");
    assert_eq!(error, refused());
    assert_eq!((retained.owner.operation, retained.owner.epoch), (1, 1));
    let work = retained
        .work
        .expect("the backing still reports its charges");
    assert_eq!(work.held_rows, 6011 - 4096);
    assert_eq!(work.removed_rows, 4096);
    // The failed job is the last unit: no repeat, no release, nothing published.
    assert_eq!(acquisition.calls("discard"), 2);
    assert_eq!(acquisition.calls("release"), 0);
    assert_eq!(acquisition.calls("advance"), 0);
    assert_eq!(
        acquisition.operations(),
        [(1, Phase::Scanning, work.held_rows)]
    );
    assert!(history.layer_stack(stack()).unwrap().is_none());
    println!("ACQUISITION_CUSTODY cleanup error={error:?} retained={retained:?}");
}

/// The portable regular-file grammar has no set-id bits; acquisition refuses
/// the entry after earlier rows are already in backing.
fn with_refused_entry() -> Fixture {
    let fixture = Fixture::new(20);
    let refused = fixture.source.join("d9/f0019");
    fs::set_permissions(&refused, fs::Permissions::from_mode(0o4644)).unwrap();
    fixture
}

#[test]
fn a_definite_failure_removes_its_rows_and_its_record() {
    let fixture = with_refused_entry();
    let history = MemoryHistory::default();
    let acquisition = MemoryAcquisition::default();
    let error = attempt(&fixture, &acquisition, &history).unwrap_err();
    assert!(matches!(error, ProjectError::InvalidInput), "{error:?}");
    assert_eq!(acquisition.issued(), 1);
    assert!(
        acquisition.calls("put_entries") >= 1,
        "rows were in backing"
    );
    assert_eq!(acquisition.calls("advance"), 1);
    assert_eq!(acquisition.calls("release"), 1);
    assert!(acquisition.operations().is_empty());
    let left: Vec<_> = fs::read_dir(&fixture.path)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(left, [std::ffi::OsString::from("input")]);
    assert!(history.layer_stack(stack()).unwrap().is_none());
}

#[test]
fn a_failed_cleanup_after_a_failure_keeps_both_causes() {
    let fixture = with_refused_entry();
    let history = MemoryHistory::default();
    let acquisition = MemoryAcquisition::default();
    acquisition.fail("discard", 1, refused());
    let outcome = attempt(&fixture, &acquisition, &history);
    let Err(ProjectError::Cleanup {
        cause,
        error,
        retained,
    }) = outcome
    else {
        panic!("expected a cleanup failure, found {outcome:?}");
    };
    assert!(
        matches!(cause.as_deref(), Some(ProjectError::InvalidInput)),
        "{cause:?}"
    );
    assert_eq!(error, refused());
    let held = retained.work.expect("charges").held_rows;
    assert!(held > 0);
    // The operation recorded its own failure before its removal was refused.
    assert_eq!(acquisition.operations(), [(1, Phase::Failed, held)]);
    assert_eq!(acquisition.calls("discard"), 1);
    assert_eq!(acquisition.calls("release"), 0);
    assert!(history.layer_stack(stack()).unwrap().is_none());
}

#[test]
fn an_unknown_outcome_leaves_the_operation_untouched() {
    let fixture = Fixture::new(40);
    let history = MemoryHistory::default();
    let acquisition = MemoryAcquisition::default();
    let unknown = AcquisitionError::Persistence(PersistenceError::Uncertain);
    acquisition.fail("complete_files", 1, unknown.clone());
    let outcome = attempt(&fixture, &acquisition, &history);
    let Err(ProjectError::Uncertain { cause, retained }) = outcome else {
        panic!("expected an unknown outcome, found {outcome:?}");
    };
    assert!(
        matches!(&*cause, ProjectError::Acquisition(error) if *error == unknown),
        "{cause:?}"
    );
    assert_eq!(retained.owner.operation, 1);
    assert!(retained.work.is_none(), "no further unit was attempted");
    for unit in ["discard", "release", "advance", "work"] {
        assert_eq!(acquisition.calls(unit), 0, "{unit}");
    }
    // 51 entries and 40 native identities, exactly as the failed unit left them.
    assert_eq!(acquisition.operations(), [(1, Phase::Scanning, 91)]);
    assert!(history.layer_stack(stack()).unwrap().is_none());
}

#[test]
fn a_backing_read_failure_is_definite_and_is_cleaned_up() {
    let fixture = Fixture::new(40);
    let history = MemoryHistory::default();
    let acquisition = MemoryAcquisition::default();
    acquisition.fail("file_roots", 1, refused());
    let error = attempt(&fixture, &acquisition, &history).unwrap_err();
    assert!(
        matches!(&error, ProjectError::Acquisition(held) if *held == refused()),
        "{error:?}"
    );
    assert!(acquisition.operations().is_empty());
    assert!(history.layer_stack(stack()).unwrap().is_none());
}
