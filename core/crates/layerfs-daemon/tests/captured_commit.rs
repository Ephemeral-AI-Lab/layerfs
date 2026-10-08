//! The captured namespace producer through the unchanged Store Commit driver,
//! over a real Disposable Store and the real local owner, without a mount.
//! Every published root is read back completely from a fresh bind and compared
//! with an independent model; costs are counted, never timed.
#[path = "support/captured_drive.rs"]
mod drive;
#[path = "support/namespace_model.rs"]
mod model;
#[allow(dead_code)]
#[path = "support/installed_store.rs"]
mod support;
use drive::{evidence, job, live, settled, Do, Rig, Session};
use layerfs_content::{filesystem::validate::ValidationWork, ObjectId};
use layerfs_daemon::{
    store::{BoundWorkspace, CommitError, CommitFailure, CommitPhase, CommitSuccess},
    Command, Completion, OwnerWork, Response,
};
use layerfs_history::CommitStagedOutcome;
use layerfs_telemetry::timer::Timing;
use layerfs_workspace::{
    CapturedNamespace, CapturedNamespaceAttempt, CapturedNamespaceWork, WorkspaceError,
};
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::Path,
};

/// What one producer attempt left for its caller, read before any release.
#[derive(Debug)]
struct Produced {
    work: CapturedNamespaceWork,
    validation: Option<ValidationWork>,
    counters: String,
    failure: Option<String>,
    file_failed: bool,
    record_failure: Option<String>,
    retained_after: (bool, bool),
}
#[derive(Clone, Copy, Eq, PartialEq)]
enum Fault {
    None,
    /// The exact reader is released before the one attempt reads through it.
    ReleasedReader,
}
type Committed = Result<CommitSuccess, Box<CommitFailure>>;

/// The test's Commit closure: acquire the exact reader and one operation
/// owner, run the producer once into the driver's Save, read its custody, drop
/// it, then release the reader and the owner in that order. The driver itself
/// is unchanged and owns Capture, Save, publication and install.
fn commit(
    rig: &Rig,
    workspace: &BoundWorkspace,
    request: u64,
    during: impl FnOnce(),
    fault: Fault,
) -> (Committed, Option<Produced>) {
    let operation = workspace.operation().unwrap();
    let route = workspace.route();
    let policy = rig.store.policy().construction();
    let mut produced = None;
    let result = workspace.commit(|save, capture, _| {
        during();
        let overlay = operation.overlay();
        let reader = match job(
            overlay,
            route,
            Command::AcquireCapturedReader { capture, request },
        )
        .result()
        {
            Ok(Response::CapturedReader(Some(reader))) => *reader,
            other => panic!("{other:?}"),
        };
        let owner = match job(overlay, route, Command::AcquireOperation { request }).result() {
            Ok(Response::Operation(Some(owner))) => *owner,
            other => panic!("{other:?}"),
        };
        if fault == Fault::ReleasedReader {
            assert!(job(overlay, route, Command::ReleaseCapturedReader(reader))
                .result()
                .is_ok());
        }
        let mut sink = save.sink();
        let CapturedNamespaceAttempt { result, custody } =
            Timing::disabled("commit.namespace", |scope| {
                Ok::<_, CommitError>(
                    CapturedNamespace::new(operation.workspace(), overlay, reader, owner)
                        .construct(
                            policy,
                            &policy.capacities(),
                            save,
                            &mut sink,
                            scope.child("construct"),
                        ),
                )
            })
            .0
            .unwrap();
        let retained = |command| match job(overlay, route, command).result() {
            Ok(Response::CapturedReader(kept)) => kept.is_some(),
            Ok(Response::Operation(kept)) => kept.is_some(),
            other => panic!("{other:?}"),
        };
        produced = Some(Produced {
            work: custody.work,
            validation: result.as_ref().ok().map(|built| built.counters.validation),
            counters: format!("{:?}", result.as_ref().map(|built| built.counters)),
            failure: custody.failure.as_ref().map(|error| format!("{error:?}")),
            file_failed: custody.file.is_some(),
            record_failure: custody
                .records
                .failure
                .as_ref()
                .map(|error| format!("{error:?}")),
            // The attempt released nothing: both owners are still retained.
            retained_after: (
                retained(Command::RetainedCapturedReader { request }),
                retained(Command::RetainedOperation { request }),
            ),
        });
        let (reader, owner) = (custody.reader, custody.operation);
        // The Content result of a failed attempt is only a label. The driver
        // must classify the first original cause, so the closure hands it that
        // cause: namespace or reader first, else the failing file's, else the
        // record scope's. An owner completion goes back as the completion.
        let mut custody = custody;
        let original = custody
            .failure
            .take()
            .or_else(|| custody.file.as_mut().and_then(|file| file.failure.take()))
            .or_else(|| custody.records.failure.take());
        drop(custody);
        if fault != Fault::ReleasedReader {
            assert!(job(overlay, route, Command::ReleaseCapturedReader(reader))
                .result()
                .is_ok());
        }
        assert!(job(overlay, route, Command::ReleaseOperation(owner))
            .result()
            .is_ok());
        match (result, original) {
            (Ok(built), _) => Ok(built.root),
            (Err(_), Some(WorkspaceError::Service(service))) => {
                Err(match service.downcast::<Completion>() {
                    Ok(completion) => CommitError::Completion(completion),
                    Err(other) => CommitError::Workspace(WorkspaceError::Service(other)),
                })
            }
            (Err(_), Some(original)) => Err(CommitError::Workspace(original)),
            (Err(label), None) => Err(CommitError::from(label)),
        }
    });
    (result, produced)
}
fn committed(result: &Committed) -> ObjectId {
    match result {
        Ok(success) => match &success.history {
            CommitStagedOutcome::Committed(record) => record.root,
            other => panic!("{other:?}"),
        },
        Err(failure) => panic!("{failure:?}"),
    }
}
/// The published root as a fresh bind sees it, walked completely.
fn rebound(rig: &Rig, tag: u8, root: ObjectId) -> model::Flat {
    let fresh = rig.bind(tag);
    assert_eq!(fresh.snapshot().unwrap().effective_root, root);
    let operation = fresh.operation().unwrap();
    let flat = model::canonical(operation.client(), root);
    drop(operation);
    rig.close(&fresh);
    flat
}
fn pattern(seed: u8, len: usize) -> Vec<u8> {
    (0..len)
        .map(|n| (n as u32).wrapping_mul(31).wrapping_add(u32::from(seed)) as u8)
        .collect()
}
/// Directories, files of several sizes, symlinks and hard links; `bulk` adds
/// `bulk/d-NNN/f-NN` files, fifty to a directory, to grow the base.
fn tree(source: &Path, bulk: usize) {
    for directory in [
        "a",
        "a/b",
        "a/b/c",
        "keep",
        "keep/inner",
        "gone",
        "gone/sub",
        "wide",
        "empty",
    ] {
        fs::create_dir_all(source.join(directory)).unwrap();
    }
    for (path, bytes) in [
        ("top.txt", b"top level\n".to_vec()),
        ("a/one", b"one: sixteen byt\n".to_vec()),
        ("a/b/two", b"two contents\n".to_vec()),
        ("a/b/c/three", pattern(3, 300_000)),
        ("keep/inner/k", pattern(9, 5_000)),
        ("gone/x", b"x".to_vec()),
        ("gone/sub/y", b"y".to_vec()),
        ("empty-file", Vec::new()),
    ] {
        fs::write(source.join(path), bytes).unwrap();
    }
    for n in 0..80 {
        fs::write(source.join(format!("wide/w-{n:06}")), format!("wide {n}\n")).unwrap();
    }
    for n in 0..bulk {
        let directory = source.join(format!("bulk/d-{:03}", n / 50));
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join(format!("f-{:02}", n % 50)),
            format!("bulk {n}\n"),
        )
        .unwrap();
    }
    symlink("a/one", source.join("link")).unwrap();
    symlink("../..", source.join("a/b/back")).unwrap();
    symlink("x", source.join("gone/l")).unwrap();
    fs::hard_link(source.join("top.txt"), source.join("a/alias")).unwrap();
    fs::hard_link(source.join("a/b/two"), source.join("keep/alias2")).unwrap();
    fs::set_permissions(source.join("a/one"), fs::Permissions::from_mode(0o600)).unwrap();
    fs::set_permissions(source.join("keep"), fs::Permissions::from_mode(0o750)).unwrap();
}

#[test]
fn every_kind_of_change_commits_through_the_driver_and_reads_back_from_a_fresh_bind() {
    let rig = Rig::new("captured-commit-mixed", |source| tree(source, 0));
    let first = rig.bind(1);
    let base_root = first.snapshot().unwrap().effective_root;
    model::assert_same(&rig.base.flat(), &rebound(&rig, 9, base_root), "base");
    let mut session = Session::new(&first, rig.base.clone());
    let large = pattern(7, 5_000);
    session.all(&[
        // New file, directory and symlink, nested.
        Do::Create("new.txt", 0o644),
        Do::Write("new.txt", 0, b"fresh bytes"),
        Do::Mkdir("made", 0o750),
        Do::Create("made/in", 0o600),
        Do::Write("made/in", 0, b"inside a new directory"),
        Do::Symlink("made/ln", b"../top.txt"),
        Do::Mkdir("made/deep", 0o755),
        Do::Mkdir("made/deep/er", 0o700),
        // Changed content: overwrite, extend, shrink then regrow with a hole.
        Do::Write("a/one", 3, b"CHANGED"),
        Do::Write("a/b/c/three", 200_000, &large),
        Do::Write("a/b/c/three", 300_000, b"appended tail"),
        Do::Truncate("a/b/two", 2),
        Do::Write("a/b/two", 10, b"regrown"),
        Do::Truncate("keep/inner/k", 100),
        // Metadata only.
        Do::Chmod("keep/inner/k", 0o400),
        Do::Utimens("keep", (1_600_000_000, 5)),
        Do::Chmod("empty", 0o700),
        // Deleted file, symlink and directories.
        Do::Remove("gone/x"),
        Do::Remove("gone/l"),
        Do::Remove("gone/sub/y"),
        Do::Remove("gone/sub"),
        Do::Remove("gone"),
        // Renamed in place, moved across parents, each kind.
        Do::Rename("top.txt", "renamed.txt"),
        Do::Rename("keep/inner", "keep/inner2"),
        Do::Rename("a/b", "keep/moved-b"),
        Do::Rename("link", "made/link"),
        Do::Rename("keep/moved-b/c/three", "made/deep/three"),
        // Hard links created and removed; one alias survives each file.
        Do::Link("a/one", "made/one-alias"),
        Do::Link("a/one", "one-alias-2"),
        Do::Remove("a/alias"),
        Do::Remove("one-alias-2"),
        // A fresh file replaces a base name that still has another alias.
        Do::Rename("new.txt", "a/one"),
        // An inherited name removed and recreated is a fresh inode.
        Do::Remove("wide/w-000003"),
        Do::Create("wide/w-000003", 0o644),
        Do::Write("wide/w-000003", 0, b"recreated"),
        Do::Remove("wide/w-000070"),
        // Created and removed inside the capture leaves nothing.
        Do::Mkdir("empty/now", 0o755),
        Do::Create("empty/now/temp", 0o644),
        Do::Remove("empty/now/temp"),
        Do::Remove("empty/now"),
    ]);
    let expected = session.model.flat();
    model::assert_same(&expected, &live(&first), "live view before Commit");
    let (result, produced) = commit(&rig, &first, 1, || {}, Fault::None);
    let produced = produced.unwrap();
    let root = committed(&result);
    assert_ne!(root, base_root);
    assert_eq!(produced.retained_after, (true, true));
    assert!(produced.failure.is_none() && !produced.file_failed);
    drop(result);
    model::assert_same(&expected, &rebound(&rig, 2, root), "fresh bind");
    model::assert_same(&expected, &live(&first), "live view after install");
    assert!(!first.commit_in_flight());
    assert!(rig
        .store
        .history()
        .stage(first.identity())
        .unwrap()
        .is_none());
    evidence(format_args!(
        "CAPTURED_COMMIT steps={} paths={} work={:?}",
        session.steps,
        expected.len(),
        produced.work
    ));
    evidence(format_args!(
        "CAPTURED_COMMIT_CONTENT {}",
        produced.counters
    ));
    // Nothing changed since: the producer returns the installed root itself.
    let (same, idle) = commit(&rig, &first, 2, || {}, Fault::None);
    match &same.as_ref().unwrap().history {
        CommitStagedOutcome::UpToDate {
            root: unchanged, ..
        } => assert_eq!(*unchanged, root),
        other => panic!("{other:?}"),
    }
    let idle = idle.unwrap();
    assert_eq!((idle.work.inode_rows, idle.work.entry_rows), (0, 0));
    drop(same);
    // A further change over the installed root, including a move back.
    session.all(&[
        Do::Rename("keep/moved-b", "a/b"),
        Do::Write("a/b/two", 0, b"again"),
        Do::Remove("made/deep/three"),
    ]);
    let expected = session.model.flat();
    let (result, _) = commit(&rig, &first, 3, || {}, Fault::None);
    let second = committed(&result);
    drop(result);
    model::assert_same(&expected, &rebound(&rig, 3, second), "second fresh bind");
    evidence(format_args!(
        "CAPTURED_COMMIT_READBACK commits=2 up_to_date=1 fresh_binds=3 oracle=independent_model"
    ));
    settled(&rig.owner.client(), 0);
    rig.close(&first);
    drop(first);
    rig.finish();
}

#[test]
fn mutations_after_the_capture_stay_out_of_its_root_and_survive_install() {
    let rig = Rig::new("captured-commit-later", |source| tree(source, 0));
    let first = rig.bind(1);
    let mut session = Session::new(&first, rig.base.clone());
    session.all(&[
        Do::Write("a/one", 0, b"before the capture"),
        Do::Mkdir("early", 0o755),
        Do::Create("early/file", 0o644),
        Do::Rename("keep/inner", "early/inner"),
        Do::Remove("gone/x"),
    ]);
    let at_capture = session.model.flat();
    // The driver has captured when the closure runs; these arrive after it and
    // touch the same inodes, names and directories the capture holds.
    let (result, produced) = commit(
        &rig,
        &first,
        1,
        || {
            session.all(&[
                Do::Write("a/one", 0, b"AFTER"),
                Do::Create("early/later", 0o600),
                Do::Write("early/later", 0, b"not in the capture"),
                Do::Remove("early/file"),
                Do::Rename("early/inner", "keep/inner"),
                Do::Mkdir("late", 0o700),
                Do::Rename("a/b", "late/b"),
                Do::Remove("gone/sub/y"),
            ]);
        },
        Fault::None,
    );
    let root = committed(&result);
    drop(result);
    assert!(produced.unwrap().failure.is_none());
    let later = session.model.flat();
    assert_ne!(at_capture, later);
    model::assert_same(&at_capture, &rebound(&rig, 2, root), "captured root");
    model::assert_same(&later, &live(&first), "live view keeps later mutations");
    let (result, _) = commit(&rig, &first, 2, || {}, Fault::None);
    let next = committed(&result);
    drop(result);
    model::assert_same(&later, &rebound(&rig, 3, next), "next captured root");
    evidence(format_args!(
        "CAPTURED_COMMIT_LATER captured_paths={} later_paths={} later_absent=true preserved_after_install=true next_commit=exact",
        at_capture.len(),
        later.len()
    ));
    settled(&rig.owner.client(), 0);
    rig.close(&first);
    drop(first);
    rig.finish();
}

#[test]
fn a_failed_attempt_keeps_its_custody_publishes_nothing_and_a_later_commit_is_exact() {
    let rig = Rig::new("captured-commit-failure", |source| tree(source, 0));
    let first = rig.bind(1);
    let original = first.snapshot().unwrap();
    let mut session = Session::new(&first, rig.base.clone());
    session.all(&[
        Do::Write("a/one", 0, b"kept across the failure"),
        Do::Mkdir("made", 0o755),
        Do::Rename("a/b", "made/b"),
    ]);
    let expected = session.model.flat();
    let (result, produced) = commit(&rig, &first, 1, || {}, Fault::ReleasedReader);
    let failure = result.unwrap_err();
    let produced = produced.unwrap();
    assert_eq!(failure.phase, CommitPhase::Construct);
    // The closure returned the original refused owner completion, which the
    // driver classifies as a definite local refusal and settles once.
    assert!(
        matches!(failure.error, CommitError::Completion(_)),
        "{:?}",
        failure.error
    );
    assert!(
        failure.locally_settled && failure.published.is_none(),
        "{failure:?}"
    );
    assert!(!first.commit_in_flight());
    assert!(rig
        .store
        .history()
        .stage(first.identity())
        .unwrap()
        .is_none());
    // The first original cause is the reader refusal, in exactly one slot, and
    // the operation owner was still retained when the attempt returned.
    assert!(produced.failure.is_some(), "{produced:?}");
    assert!(!produced.file_failed && produced.record_failure.is_none());
    assert_eq!(produced.retained_after, (false, true));
    evidence(format_args!(
        "CAPTURED_COMMIT_FAILURE phase={:?} error={:?} original={:?} work={:?}",
        failure.phase, failure.error, produced.failure, produced.work
    ));
    drop(failure);
    assert_eq!(first.snapshot().unwrap(), original);
    model::assert_same(&expected, &live(&first), "live view after the failure");
    let (result, produced) = commit(&rig, &first, 2, || {}, Fault::None);
    let root = committed(&result);
    drop(result);
    assert!(produced.unwrap().failure.is_none());
    model::assert_same(
        &expected,
        &rebound(&rig, 2, root),
        "fresh bind after the failure",
    );
    settled(&rig.owner.client(), 0);
    rig.close(&first);
    drop(first);
    rig.finish();
}

/// Counted work of one Commit of the fixed change over one base size.
#[derive(Debug)]
struct Cost {
    entries: usize,
    work: CapturedNamespaceWork,
    validation: ValidationWork,
    counters: String,
    jobs: u64,
    statements: u64,
    vm_steps: u64,
    sql_rows: u64,
    sql_changed: u64,
    fullscan: u64,
    payload: String,
    client: String,
    storage: String,
}
fn sql(work: &OwnerWork) -> (u64, u64, u64, u64, u64) {
    work.sql_foreground
        .statements
        .iter()
        .fold((0, 0, 0, 0, 0), |sum, s| {
            (
                sum.0 + s.executions,
                sum.1 + s.vm_steps,
                sum.2 + s.rows_returned,
                sum.3 + s.rows_changed,
                sum.4 + s.fullscan_steps,
            )
        })
}
fn cost(bulk: usize) -> Cost {
    let rig = Rig::new(&format!("captured-commit-cost-{bulk}"), |source| {
        tree(source, bulk)
    });
    let first = rig.bind(1);
    let mut session = Session::new(&first, rig.base.clone());
    // The fixed small change: one of each kind, none of it under `bulk`.
    session.all(&[
        Do::Write("a/one", 3, b"CHANGED"),
        Do::Create("a/new", 0o644),
        Do::Write("a/new", 0, b"new"),
        Do::Remove("gone/x"),
        Do::Rename("keep/inner", "keep/inner2"),
        Do::Rename("a/b", "keep/moved-b"),
        Do::Link("top.txt", "keep/top-alias"),
        Do::Symlink("keep/ln", b"inner2"),
        Do::Chmod("empty", 0o700),
    ]);
    let expected = session.model.flat();
    settled(&rig.owner.client(), 0);
    let before = rig.owner.client().diagnostics().unwrap();
    let client_before = first.operation().unwrap().client().diagnostics().unwrap();
    let (result, produced) = commit(&rig, &first, 1, || {}, Fault::None);
    let root = committed(&result);
    let storage = format!("{:?}", result.as_ref().unwrap().storage);
    // The Commit's own completions are caller-held credits until dropped.
    drop(result);
    settled(&rig.owner.client(), 0);
    let after = rig.owner.client().diagnostics().unwrap();
    // Read before the oracle's own complete walk of the published root.
    let client_after = first.operation().unwrap().client().diagnostics().unwrap();
    let produced = produced.unwrap();
    model::assert_same(&expected, &rebound(&rig, 2, root), "fresh bind");
    let (a, b) = (sql(&before), sql(&after));
    let cost = Cost {
        entries: expected.len(),
        work: produced.work,
        validation: produced.validation.unwrap(),
        counters: produced.counters,
        jobs: after.completed.iter().sum::<u64>() - before.completed.iter().sum::<u64>(),
        statements: b.0 - a.0,
        vm_steps: b.1 - a.1,
        sql_rows: b.2 - a.2,
        sql_changed: b.3 - a.3,
        fullscan: b.4 - a.4,
        payload: format!("{:?}", after.payload_foreground),
        client: format!("before={client_before:?} after={client_after:?}"),
        storage,
    };
    rig.close(&first);
    drop(first);
    rig.finish();
    cost
}

#[test]
fn a_fixed_small_change_costs_the_same_counted_work_as_the_base_grows() {
    let costs = [200, 1_600, 12_800].map(cost);
    for cost in &costs {
        evidence(format_args!(
            "CAPTURED_COMMIT_COST entries={} jobs={} statements={} vm_steps={} sql_rows={} sql_changed={} fullscan={} work={:?}",
            cost.entries,
            cost.jobs,
            cost.statements,
            cost.vm_steps,
            cost.sql_rows,
            cost.sql_changed,
            cost.fullscan,
            cost.work
        ));
        evidence(format_args!(
            "CAPTURED_COMMIT_COST_VALIDATION entries={} {:?}",
            cost.entries, cost.validation
        ));
        evidence(format_args!(
            "CAPTURED_COMMIT_COST_CONTENT entries={} {}",
            cost.entries, cost.counters
        ));
        evidence(format_args!(
            "CAPTURED_COMMIT_COST_PAYLOAD entries={} {}",
            cost.entries, cost.payload
        ));
        evidence(format_args!(
            "CAPTURED_COMMIT_COST_CLIENT entries={} {}",
            cost.entries, cost.client
        ));
        evidence(format_args!(
            "CAPTURED_COMMIT_COST_STORE entries={} {}",
            cost.entries, cost.storage
        ));
    }
    let [small, middle, large] = &costs;
    assert!(small.entries * 4 < middle.entries && middle.entries * 4 < large.entries);
    for other in [middle, large] {
        // The producer's captured rows, pages, points and file work are equal.
        // Its record jobs alone vary, by which sealed point answers share a
        // slot of its 64-slot window: that depends on serial values, never on
        // the base, and is bounded by one job per point.
        let shape = |work: &CapturedNamespaceWork| CapturedNamespaceWork {
            record_jobs: 0,
            ..*work
        };
        assert_eq!(shape(&other.work), shape(&small.work));
        let points = |work: &CapturedNamespaceWork| {
            work.header_points + work.value_points + work.fresh_points + work.parent_points
        };
        assert!(other.work.record_jobs <= points(&other.work) + 64);
        // Owner jobs and SQL: every statement seeks its own sealed domain, so
        // the only difference between sizes is those record point jobs.
        let extra = other.work.record_jobs as i64 - small.work.record_jobs as i64;
        assert_eq!(other.jobs as i64 - small.jobs as i64, extra);
        assert_eq!(other.statements as i64 - small.statements as i64, 3 * extra);
        assert_eq!(other.sql_changed, small.sql_changed);
        assert!((other.sql_rows as i64 - small.sql_rows as i64).abs() <= 4 * extra.abs());
        assert_eq!((other.fullscan, small.fullscan), (0, 0));
        // Validation and topology: demands, placements, walks and windows. Page
        // and wave totals follow the base tables' height and are printed above.
        let topology = |v: &ValidationWork| {
            (
                v.inode_demands,
                v.placements,
                v.ancestry_steps,
                v.territory_directories,
                v.territory_entries,
                v.entries_examined,
                v.peak_window_rows,
            )
        };
        assert_eq!(topology(&other.validation), topology(&small.validation));
    }
    assert!(small.work.largest_page <= 64 && small.validation.peak_window_rows <= 64);
}
