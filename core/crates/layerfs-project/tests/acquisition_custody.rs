//! Scratch placement and cleanup custody of initial acquisition.
//!
//! A scratch parent inside the source is refused before the source changes, and
//! a failed scratch release reports its own host cause with what it retained.
#![cfg(unix)]
mod support;
use layerfs_history::{HistoryCatalog, HistoryName, LayerStackId};
use layerfs_project::{init, InitRequest, Initialized, ProjectError};
use layerfs_telemetry::timer::Timing;
use std::{
    fs, io,
    os::unix::fs::{symlink, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use support::{memory_history::MemoryHistory, memory_metadata::MemoryMetadata, Fixture};

fn stack() -> LayerStackId {
    LayerStackId::from_authority([19; 16])
}

fn attempt(
    fixture: &Fixture,
    scratch_parent: &Path,
    history: &MemoryHistory,
) -> Result<Initialized, ProjectError> {
    let store = support::storage(Arc::new(MemoryMetadata::default()));
    Timing::disabled("project.init", |scope| {
        init(
            &store,
            history,
            InitRequest {
                source: &fixture.source,
                scratch_parent,
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
fn scratch_inside_source_is_refused_before_the_source_changes() {
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

    for parent in [&fixture.source, &nested, &alias, &detour] {
        let error = attempt(&fixture, parent, &history).unwrap_err();
        assert!(
            matches!(error, ProjectError::ScratchInsideSource),
            "{parent:?}: {error:?}"
        );
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
    assert!(observed(&nested).is_empty());
    assert_eq!(
        observed(&fixture.source)
            .into_iter()
            .map(|entry| entry.0)
            .collect::<Vec<_>>(),
        ["nested"]
    );
    assert!(history.layer_stack(stack()).unwrap().is_none());

    // The same source is acquired once its scratch is placed beside it: the
    // root, `nested` and `deeper`, with no scratch entry among them.
    let initialized = attempt(&fixture, &fixture.path, &history).unwrap();
    assert_eq!(initialized.entries, 3);
    assert_eq!(initialized.namespace_work.entries, 3);
}

/// Makes the operation's scratch directory unwritable once it holds a run.
///
/// Waiting for the first run file matters: a directory denied while still empty
/// only refuses the first run, and an empty scratch is then removed cleanly.
/// The thread stops at its own deadline or when told to, so its exit never
/// depends on the acquiring thread finishing.
fn deny_scratch_writes(parent: PathBuf, stop: Arc<AtomicBool>) -> Option<PathBuf> {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !stop.load(Ordering::Relaxed) && Instant::now() < deadline {
        let scratch = fs::read_dir(&parent).ok()?.flatten().find(|entry| {
            let name = entry.file_name();
            name.to_string_lossy().starts_with("import-")
        });
        let holds_a_run = scratch.as_ref().is_some_and(|entry| {
            fs::read_dir(entry.path()).is_ok_and(|mut runs| runs.next().is_some())
        });
        if let (Some(entry), true) = (scratch, holds_a_run) {
            fs::set_permissions(entry.path(), fs::Permissions::from_mode(0o500)).ok()?;
            return Some(entry.path());
        }
        std::hint::spin_loop();
    }
    None
}

#[test]
fn failed_scratch_release_reports_its_cause_and_retained_runs() {
    let fixture = Fixture::new(1500);
    if fs::metadata(&fixture.path).unwrap().uid() == 0 {
        // The superuser is not refused by directory permissions, so this host
        // cannot produce the failure. Nothing is asserted here.
        println!("ACQUISITION_CUSTODY skipped: directory permissions do not bind uid 0");
        return;
    }
    let history = MemoryHistory::default();
    let stop = Arc::new(AtomicBool::new(false));
    let watcher = {
        let (parent, stop) = (fixture.path.clone(), stop.clone());
        std::thread::spawn(move || deny_scratch_writes(parent, stop))
    };
    let outcome = attempt(&fixture, &fixture.path, &history);
    stop.store(true, Ordering::Relaxed);
    let scratch = watcher
        .join()
        .unwrap()
        .expect("the scratch directory was observed while acquisition ran");

    let Err(ProjectError::Cleanup {
        cause,
        error,
        retained,
    }) = outcome
    else {
        panic!("expected a cleanup failure, found {outcome:?}");
    };
    // The deciding cause is the refused run removal, not the non-empty
    // directory removal it would lead to.
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied, "{error:?}");
    assert_eq!(retained.directory, scratch);
    assert!(retained.runs >= 1);
    let on_disk: Vec<u64> = fs::read_dir(&scratch)
        .unwrap()
        .map(|entry| entry.unwrap().metadata().unwrap().len())
        .collect();
    assert_eq!(on_disk.len() as u64, retained.runs);
    assert_eq!(on_disk.iter().sum::<u64>(), retained.run_bytes);
    assert!(history.layer_stack(stack()).unwrap().is_none());
    println!(
        "ACQUISITION_CUSTODY cause={cause:?} error={error:?} runs={} bytes={}",
        retained.runs, retained.run_bytes
    );
    fs::set_permissions(&scratch, fs::Permissions::from_mode(0o700)).unwrap();
}
