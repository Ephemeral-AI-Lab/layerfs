//! The product Commit constructor `BoundWorkspace::commit_captured` over a real
//! Disposable Store and the real local owner, without a mount. Every published
//! root is read back completely from a fresh bind and compared with an
//! independent model; the constructor's own receipt says what it released.
#[path = "support/captured_drive.rs"]
mod drive;
#[path = "support/held_writer.rs"]
mod held_writer;
#[path = "support/history_boundary.rs"]
mod history_boundary;
#[path = "support/namespace_model.rs"]
mod model;
#[allow(dead_code)]
#[path = "support/installed_store.rs"]
mod support;
use drive::{evidence, job, live, settled, Do, Rig, Session};
use history_boundary::Boundary;
use layerfs_content::ObjectId;
use layerfs_daemon::{
    store::{
        BoundWorkspace, CapturedConstruction, CommitError, CommitPhase, CommitSuccess, ReadLimits,
        ReleasedOwner, Store, StoreReader,
    },
    Command, Completion, Owner, OwnerConfig, OwnerError, Response,
};
use layerfs_history::{
    BranchSnapshot, CommitHistoryRequest, CommitRecord, CommitStagedOutcome, HistoryCatalog,
    HistoryError,
};
use layerfs_overlay::{Capture, CapturedReader, OperationOwner, ProfileConfig};
use layerfs_persistence::{Handles, SqlitePersistenceProfile};
use layerfs_storage::{ReservationBlocks, Storage, StorageError};
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

fn record(success: &CommitSuccess) -> CommitRecord {
    match &success.history {
        CommitStagedOutcome::Committed(record) => record.clone(),
        other => panic!("{other:?}"),
    }
}
/// The capture's generation is the request number of the product constructor.
fn request(capture: Capture) -> u64 {
    u64::try_from(capture.generation.number()).unwrap()
}
/// What the Overlay itself still holds for one request, asked after the Commit.
fn retained(
    rig: &Rig,
    workspace: &BoundWorkspace,
    request: u64,
) -> (Option<CapturedReader>, Option<OperationOwner>) {
    let client = rig.owner.client();
    let reader = match job(
        &client,
        workspace.route(),
        Command::RetainedCapturedReader { request },
    )
    .result()
    {
        Ok(Response::CapturedReader(kept)) => *kept,
        other => panic!("{other:?}"),
    };
    let owner = match job(
        &client,
        workspace.route(),
        Command::RetainedOperation { request },
    )
    .result()
    {
        Ok(Response::Operation(kept)) => *kept,
        other => panic!("{other:?}"),
    };
    (reader, owner)
}
/// The receipt of an attempt whose known outcome ended its ownership: the
/// reader and then the operation owner were released, each by its own original
/// completion. Returns the request to ask the Overlay about. The two release
/// completions are Lifecycle credits of this Workspace, and the fixture's
/// owner admits two per namespace: the Overlay is asked only after the caller
/// has dropped the outcome that holds them.
fn receipt(namespace: Option<&CapturedConstruction>, capture: Capture) -> u64 {
    let namespace = namespace.expect("the product closure ran");
    assert!(!namespace.retained(), "{namespace:?}");
    assert!(namespace.reader.is_none() && namespace.operation.is_none());
    assert!(
        namespace.custody.is_none() && namespace.release_error.is_none(),
        "{namespace:?}"
    );
    assert_eq!(
        namespace.released,
        [ReleasedOwner::Reader, ReleasedOwner::Operation],
        "{namespace:?}"
    );
    assert!(namespace.release_failure.is_none(), "{namespace:?}");
    request(capture)
}
fn done(completion: Option<&Completion>) -> bool {
    matches!(completion.map(Completion::result), Some(Ok(Response::Done)))
}
/// The Branch's current ancestry from its head, newest first.
fn history(rig: &Rig) -> Vec<CommitRecord> {
    let page = rig
        .store
        .history()
        .commit_history(&CommitHistoryRequest {
            branch: rig.f.branch,
            start: None,
            cursor: None,
            limit: 32,
        })
        .unwrap();
    assert!(page.continuation.is_none());
    page.records
}
fn branch(rig: &Rig) -> BranchSnapshot {
    rig.store
        .history()
        .branch_snapshot(rig.f.branch)
        .unwrap()
        .unwrap()
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
/// Directories, files of several sizes, symlinks and hard links.
fn tree(source: &Path) {
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
    symlink("a/one", source.join("link")).unwrap();
    symlink("../..", source.join("a/b/back")).unwrap();
    symlink("x", source.join("gone/l")).unwrap();
    fs::hard_link(source.join("top.txt"), source.join("a/alias")).unwrap();
    fs::hard_link(source.join("a/b/two"), source.join("keep/alias2")).unwrap();
    fs::set_permissions(source.join("a/one"), fs::Permissions::from_mode(0o600)).unwrap();
    fs::set_permissions(source.join("keep"), fs::Permissions::from_mode(0o750)).unwrap();
}

#[test]
fn every_kind_of_change_commits_through_the_product_constructor() {
    let rig = Rig::new("product-commit-mixed", tree);
    let first = rig.bind(1);
    let bound = first.snapshot().unwrap();
    let base_root = bound.effective_root;
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
    let success = first
        .commit_captured()
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    let published = record(&success);
    assert_ne!(published.root, base_root);
    assert_eq!(published.parent, bound.branch.head_commit);
    let namespace = success.namespace.as_ref().expect("the product closure ran");
    assert!(
        namespace.work.is_some() && namespace.counters.is_some(),
        "{namespace:?}"
    );
    let request = receipt(success.namespace.as_ref(), success.capture);
    assert!(matches!(success.installed.result(), Ok(Response::Done)));
    evidence(format_args!(
        "PRODUCT_COMMIT steps={} paths={} request={request} released={} retained=false work={:?}",
        session.steps,
        expected.len(),
        namespace.released.len(),
        namespace.work
    ));
    evidence(format_args!(
        "PRODUCT_COMMIT_CONTENT {:?}",
        namespace.counters
    ));
    drop(success);
    assert_eq!(retained(&rig, &first, request), (None, None));
    model::assert_same(&expected, &rebound(&rig, 2, published.root), "fresh bind");
    model::assert_same(&expected, &live(&first), "live view after install");
    assert!(!first.commit_in_flight());
    assert!(rig
        .store
        .history()
        .stage(first.identity())
        .unwrap()
        .is_none());
    let after = first.snapshot().unwrap();
    assert_eq!(
        (after.branch.head_commit, after.effective_root),
        (Some(published.id), published.root)
    );
    assert_eq!(history(&rig), vec![published.clone()]);
    evidence(format_args!(
        "PRODUCT_COMMIT_READBACK commits=1 fresh_binds=2 oracle=independent_model live_after_install=exact retained_reader=None retained_operation=None"
    ));
    settled(&rig.owner.client(), 0);
    rig.close(&first);
    drop(first);
    rig.finish();
}

#[test]
fn an_unchanged_workspace_is_up_to_date_and_creates_no_commit() {
    let rig = Rig::new("product-commit-up-to-date", tree);
    let first = rig.bind(1);
    let bound = first.snapshot().unwrap();
    let before = (history(&rig), branch(&rig));
    assert_eq!(before.1, bound);
    let same = first
        .commit_captured()
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    assert_eq!(
        same.history,
        CommitStagedOutcome::UpToDate {
            head: bound.branch.head_commit,
            root: bound.effective_root,
        }
    );
    // The producer ran and found no captured row.
    let work = same.namespace.as_ref().unwrap().work.unwrap();
    assert_eq!((work.inode_rows, work.entry_rows), (0, 0));
    let request = receipt(same.namespace.as_ref(), same.capture);
    drop(same);
    assert_eq!(retained(&rig, &first, request), (None, None));
    assert_eq!((history(&rig), branch(&rig)), before);
    assert_eq!(first.snapshot().unwrap(), bound);
    assert!(!first.commit_in_flight());
    model::assert_same(&rig.base.flat(), &live(&first), "unchanged live view");
    let first_head = bound.branch.head_commit;
    // A change is a Commit whose parent is the head the Workspace was bound at.
    let mut session = Session::new(&first, rig.base.clone());
    session.all(&[
        Do::Write("a/one", 0, b"changed once"),
        Do::Mkdir("added", 0o755),
        Do::Create("added/file", 0o644),
        Do::Remove("gone/x"),
    ]);
    let expected = session.model.flat();
    let changed = first
        .commit_captured()
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    let published = record(&changed);
    assert_eq!(published.parent, first_head);
    assert_ne!(published.root, bound.effective_root);
    let request = receipt(changed.namespace.as_ref(), changed.capture);
    drop(changed);
    assert_eq!(retained(&rig, &first, request), (None, None));
    assert_eq!(history(&rig), vec![published.clone()]);
    model::assert_same(&expected, &rebound(&rig, 2, published.root), "fresh bind");
    // Nothing changed since: the same head, and no further Commit record.
    let again = first
        .commit_captured()
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    assert_eq!(
        again.history,
        CommitStagedOutcome::UpToDate {
            head: Some(published.id),
            root: published.root,
        }
    );
    let work = again.namespace.as_ref().unwrap().work.unwrap();
    assert_eq!((work.inode_rows, work.entry_rows), (0, 0));
    let request = receipt(again.namespace.as_ref(), again.capture);
    drop(again);
    assert_eq!(retained(&rig, &first, request), (None, None));
    assert_eq!(history(&rig), vec![published.clone()]);
    assert_eq!(branch(&rig).branch.head_commit, Some(published.id));
    model::assert_same(&expected, &live(&first), "live view after UpToDate");
    evidence(format_args!(
        "PRODUCT_COMMIT_UP_TO_DATE unchanged_bound=UpToDate(head={first_head:?}) history_records_before=0 after=0 changed=Committed(parent={first_head:?}) repeated=UpToDate(head=committed) history_records=1 owners_released=3of3"
    ));
    settled(&rig.owner.client(), 0);
    rig.close(&first);
    drop(first);
    rig.finish();
}

#[test]
fn a_stale_unchanged_candidate_overwrites_a_moved_head() {
    let rig = Rig::new("product-commit-stale", tree);
    // One Commit first, so the head both Workspaces capture is a real record.
    let seed = rig.bind(1);
    let mut seeded = Session::new(&seed, rig.base.clone());
    seeded.all(&[
        Do::Write("a/one", 0, b"the original head"),
        Do::Mkdir("seeded", 0o755),
    ]);
    let at_bind = seeded.model.flat();
    let first = seed
        .commit_captured()
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    let original = record(&first);
    drop(first);
    // Two Workspaces bound at the same head.
    let (a, b) = (rig.bind(2), rig.bind(3));
    let (bound_a, bound_b) = (a.snapshot().unwrap(), b.snapshot().unwrap());
    assert_eq!(bound_a, bound_b);
    assert_eq!(
        (bound_b.branch.head_commit, bound_b.effective_root),
        (Some(original.id), original.root)
    );
    // A changes and commits: the Branch head moves to A's record.
    let mut one = Session::new(&a, seeded.model.clone());
    one.all(&[
        Do::Write("a/one", 0, b"moved by A"),
        Do::Create("from-a", 0o644),
        Do::Write("from-a", 0, b"only in A's Commit"),
        Do::Rename("keep/inner", "keep/renamed-by-a"),
        Do::Remove("gone/x"),
    ]);
    let expected_a = one.model.flat();
    let moved = a
        .commit_captured()
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    let record_a = record(&moved);
    let request = receipt(moved.namespace.as_ref(), moved.capture);
    drop(moved);
    assert_eq!(retained(&rig, &a, request), (None, None));
    assert_eq!(record_a.parent, Some(original.id));
    assert_ne!(record_a.root, original.root);
    assert_eq!(branch(&rig).branch.head_commit, Some(record_a.id));
    model::assert_same(&at_bind, &live(&b), "B's view before its own Commit");
    assert_eq!(b.snapshot().unwrap(), bound_b);
    // B changed nothing. Its candidate is the root it was bound at, which is
    // no longer the Branch's root: the owner's overwrite decision says this
    // publishes a new Commit that keeps B's captured parent.
    let stale = b
        .commit_captured()
        .unwrap_or_else(|failure| panic!("stale unchanged Commit refused: {failure:?}"));
    evidence(format_args!(
        "PRODUCT_COMMIT_STALE_OBSERVED history={:?}",
        stale.history
    ));
    let record_b = match &stale.history {
        CommitStagedOutcome::Committed(record) => record.clone(),
        other => panic!("the overwrite decision requires Committed; observed {other:?}"),
    };
    assert_eq!(record_b.root, original.root);
    assert_eq!(record_b.parent, Some(original.id));
    assert!(record_b.id != record_a.id && record_b.id != original.id);
    let work = stale.namespace.as_ref().unwrap().work.unwrap();
    assert_eq!((work.inode_rows, work.entry_rows), (0, 0));
    let request = receipt(stale.namespace.as_ref(), stale.capture);
    drop(stale);
    assert_eq!(retained(&rig, &b, request), (None, None));
    // The Branch names B's record; its ancestry is B's, then the original.
    let current = branch(&rig);
    assert_eq!(
        (current.branch.head_commit, current.effective_root),
        (Some(record_b.id), original.root)
    );
    assert_eq!(history(&rig), vec![record_b.clone(), original.clone()]);
    // A's displaced Commit stays stored, and its root stays complete.
    assert_eq!(
        rig.store.history().commit(record_a.id).unwrap(),
        Some(record_a.clone())
    );
    let fresh = rig.bind(4);
    let seen = fresh.snapshot().unwrap();
    assert_eq!(
        (seen.branch.head_commit, seen.effective_root),
        (Some(record_b.id), original.root)
    );
    let operation = fresh.operation().unwrap();
    model::assert_same(
        &at_bind,
        &model::canonical(operation.client(), original.root),
        "a fresh bind reads B's tree",
    );
    model::assert_same(
        &expected_a,
        &model::canonical(operation.client(), record_a.root),
        "A's displaced root read from a fresh bind",
    );
    drop(operation);
    model::assert_same(&at_bind, &live(&fresh), "fresh live view");
    // Each bound Workspace keeps its own installed view and head.
    model::assert_same(&expected_a, &live(&a), "A keeps its own view");
    model::assert_same(&at_bind, &live(&b), "B's view after its install");
    assert_eq!(a.snapshot().unwrap().branch.head_commit, Some(record_a.id));
    assert_eq!(b.snapshot().unwrap().branch.head_commit, Some(record_b.id));
    evidence(format_args!(
        "PRODUCT_COMMIT_STALE outcome=Committed root=bound_root parent=captured_head(original) branch_head=B ancestry=[B,original] displaced_commit=readable displaced_root=exact fresh_bind=B_tree"
    ));
    settled(&rig.owner.client(), 0);
    for workspace in [&seed, &a, &b, &fresh] {
        rig.close(workspace);
    }
    drop((seed, a, b, fresh));
    rig.finish();
}

#[test]
fn repeated_incremental_commits_keep_the_store_open_and_each_root_is_exact() {
    // The fixture selects Disposable explicitly (`support/installed_store.rs`,
    // `with_sqlite_profile`), and `Rig::new` opens that same configuration
    // once; this test never opens, drops or seals the daemon Store again.
    let rig = Rig::new("product-commit-incremental", tree);
    assert_eq!(
        rig.f.config.sqlite_profile,
        SqlitePersistenceProfile::Disposable
    );
    let opened = Arc::as_ptr(&rig.store);
    let first = rig.bind(1);
    let bound = first.snapshot().unwrap();
    let mut session = Session::new(&first, rig.base.clone());
    let tail = pattern(5, 9_000);
    let rounds: [&[Do<'_>]; 6] = [
        &[Do::Write("a/one", 0, b"round one"), Do::Create("r1", 0o644)],
        &[
            Do::Mkdir("r2", 0o755),
            Do::Create("r2/in", 0o600),
            Do::Write("r2/in", 0, b"second round"),
            Do::Remove("r1"),
        ],
        &[
            Do::Rename("r2", "keep/r3"),
            Do::Write("a/b/c/three", 150_000, &tail),
        ],
        &[
            Do::Symlink("r4", b"keep/r3/in"),
            Do::Link("top.txt", "keep/r4-alias"),
            Do::Chmod("empty", 0o711),
        ],
        &[
            Do::Remove("gone/sub/y"),
            Do::Remove("gone/sub"),
            Do::Truncate("keep/inner/k", 10),
        ],
        &[
            Do::Rename("keep/r3/in", "a/one"),
            Do::Write("wide/w-000001", 0, b"sixth"),
            Do::Utimens("wide", (1_650_000_000, 9)),
        ],
    ];
    let mut parent = bound.branch.head_commit;
    let mut roots = vec![bound.effective_root];
    let mut previous = rig.base.flat();
    let mut records = Vec::new();
    for (round, steps) in rounds.iter().enumerate() {
        let tag = 10 + 2 * round as u8;
        session.all(steps);
        let expected = session.model.flat();
        assert_ne!(expected, previous);
        // Mutations made after the last Commit are not in the published head.
        model::assert_same(
            &previous,
            &rebound(&rig, tag, *roots.last().unwrap()),
            "head before the next Commit",
        );
        let success = first
            .commit_captured()
            .unwrap_or_else(|failure| panic!("round {round}: {failure:?}"));
        let published = record(&success);
        assert_eq!(published.parent, parent);
        assert!(!roots.contains(&published.root));
        // One Save on this Commit's own Storage producer.
        assert_eq!(success.storage.initial_reservations, 1);
        let request = receipt(success.namespace.as_ref(), success.capture);
        let work = success.namespace.as_ref().unwrap().work.unwrap();
        evidence(format_args!(
            "PRODUCT_COMMIT_INCREMENTAL round={} steps={} inode_rows={} entry_rows={} initial_reservations={} publish={} same_store={}",
            round + 1,
            steps.len(),
            work.inode_rows,
            work.entry_rows,
            success.storage.initial_reservations,
            success.storage.publish,
            std::ptr::eq(opened, Arc::as_ptr(&rig.store))
        ));
        drop(success);
        assert_eq!(retained(&rig, &first, request), (None, None));
        model::assert_same(
            &expected,
            &rebound(&rig, tag + 1, published.root),
            "fresh bind after the Commit",
        );
        model::assert_same(&expected, &live(&first), "live view after install");
        assert!(!first.commit_in_flight());
        parent = Some(published.id);
        roots.push(published.root);
        records.insert(0, published);
        previous = expected;
    }
    assert_eq!(history(&rig), records);
    assert!(std::ptr::eq(opened, Arc::as_ptr(&rig.store)));
    // Read-back of the same Store file through one more read-only session,
    // whose open verifies the journal mode and synchronous setting it reports.
    let observer =
        Handles::open_read_only(rig.f.config.clone(), support::BINDING, support::CURSOR).unwrap();
    let profile = observer.profile();
    assert_eq!(profile.persistence, SqlitePersistenceProfile::Disposable);
    evidence(format_args!(
        "PRODUCT_COMMIT_STORE commits=6 store=one_arc_never_dropped reopen=none seal=none selected_profile={:?} observer_readback=(persistence={:?} journal_mode={} synchronous={}) durable=NOT_RUN",
        rig.f.config.sqlite_profile, profile.persistence, profile.journal_mode, profile.synchronous
    ));
    drop(observer);
    settled(&rig.owner.client(), 0);
    rig.close(&first);
    drop(first);
    rig.finish();
}

/// The same fixture as `Rig::new`, with the Store built from public
/// constructors around the existing external History boundary. The boundary
/// acts once, at the first publication; it is not a product hook.
fn observed(label: &str, boundary: Boundary, config: OwnerConfig) -> (Rig, Arc<Handles>) {
    let mut base = None;
    let f = support::Fixture::built(label, |source| {
        tree(source);
        let (model, names) = model::Model::native(source);
        base = Some(model);
        names
    });
    assert_eq!(
        f.config.sqlite_profile,
        SqlitePersistenceProfile::Disposable
    );
    let handles = Arc::new(
        Handles::open_writable(f.config.clone(), support::BINDING, support::CURSOR).unwrap(),
    );
    let readers = (0..2)
        .map(|_| {
            let read = Handles::open_read_only(f.config.clone(), support::BINDING, support::CURSOR)
                .unwrap();
            StoreReader::new(Storage::new(read.storage).unwrap(), Arc::new(read.history))
        })
        .collect();
    let history = history_boundary::ObservedHistory {
        handles: handles.clone(),
        database: f.config.path.clone(),
        boundary,
        once: AtomicBool::new(true),
    };
    let store = Arc::new(
        Store::new(
            handles.storage.clone(),
            Arc::new(history),
            readers,
            2 * 1024 * 1024,
            ReservationBlocks::default(),
            ReadLimits::default(),
        )
        .unwrap(),
    );
    let owner = Owner::start(
        &f.directory.join("overlay.sqlite"),
        ProfileConfig::default(),
        config,
    )
    .unwrap();
    let rig = Rig {
        f,
        store,
        owner,
        base: base.unwrap(),
    };
    (rig, handles)
}
/// Nothing reached History and nothing local was lost.
fn unpublished(
    rig: &Rig,
    workspace: &BoundWorkspace,
    original: &BranchSnapshot,
    expected: &model::Flat,
    what: &str,
) {
    assert!(!workspace.commit_in_flight(), "{what}");
    assert!(rig
        .store
        .history()
        .stage(workspace.identity())
        .unwrap()
        .is_none());
    assert_eq!(&branch(rig), original, "{what}");
    assert!(history(rig).is_empty(), "{what}");
    assert_eq!(&workspace.snapshot().unwrap(), original, "{what}");
    model::assert_same(expected, &live(workspace), what);
}
/// Two real definite refusals of one Workspace and a later exact Commit. A
/// real external process holds the Store writer each time: before the Save
/// begins, and when the driver publishes. `config` is the local owner's
/// admission; nothing else differs between the tests that call this.
fn definite_refusal(label: &str, config: OwnerConfig) {
    let slots = config.lifecycle_jobs_per_namespace;
    let (rig, handles) = observed(label, Boundary::HeldWriter, config);
    let workspace = rig.bind(1);
    let original = workspace.snapshot().unwrap();
    let mut session = Session::new(&workspace, rig.base.clone());
    session.all(&[
        Do::Write("a/one", 0, b"kept across both refusals"),
        Do::Mkdir("made", 0o755),
        Do::Rename("a/b", "made/b"),
        Do::Remove("gone/x"),
    ]);
    let expected = session.model.flat();
    // The writer is held before the Save begins: the driver refuses at Begin
    // and the product closure never runs.
    let held = held_writer::HeldWriter::acquire(&rig.f.config.path);
    let begin = workspace.commit_captured().unwrap_err();
    held.release();
    assert_eq!(begin.phase, CommitPhase::Begin);
    assert!(
        matches!(begin.error, CommitError::Storage(StorageError::Busy)),
        "{:?}",
        begin.error
    );
    assert!(
        begin.locally_settled && begin.published.is_none(),
        "{begin:?}"
    );
    assert!(done(begin.local.as_ref()), "{begin:?}");
    assert!(begin.namespace.is_none(), "the closure never ran");
    let first_request = request(begin.capture.unwrap());
    evidence(format_args!(
        "PRODUCT_COMMIT_REFUSAL case=writer_held_before_save lifecycle_slots={slots} phase={:?} error={:?} locally_settled={} namespace=None request={first_request} published=None",
        begin.phase, begin.error, begin.locally_settled
    ));
    drop(begin);
    assert_eq!(retained(&rig, &workspace, first_request), (None, None));
    unpublished(
        &rig,
        &workspace,
        &original,
        &expected,
        "after Busy at Begin",
    );
    // The same real held writer, taken by the external boundary when the
    // driver publishes: the product closure has run to its root, the Save is
    // finished, and History refuses Busy before any effect.
    let publish = workspace.commit_captured().unwrap_err();
    assert_eq!(publish.phase, CommitPhase::Publish);
    assert!(
        matches!(publish.error, CommitError::History(HistoryError::Busy)),
        "{:?}",
        publish.error
    );
    assert!(
        publish.locally_settled && publish.published.is_none(),
        "{publish:?}"
    );
    assert!(done(publish.local.as_ref()), "{publish:?}");
    assert!(publish.saved.is_some() && publish.intent.is_some());
    let namespace = publish.namespace.as_ref().expect("the product closure ran");
    assert!(
        namespace.work.is_some() && namespace.counters.is_some(),
        "{namespace:?}"
    );
    // The observed receipt, printed before it is checked.
    evidence(format_args!(
        "PRODUCT_COMMIT_REFUSAL_OBSERVED case=writer_held_at_publication lifecycle_slots={slots} phase={:?} error={:?} locally_settled={} retained={} reader={:?} operation={:?} released={:?} release_error={:?}",
        publish.phase,
        publish.error,
        publish.locally_settled,
        namespace.retained(),
        namespace.reader.is_some(),
        namespace.operation.is_some(),
        namespace.released,
        namespace.release_error
    ));
    // A definite refusal the driver resolved locally ends the attempt's
    // ownership: both owners released, nothing kept.
    let second_request = receipt(publish.namespace.as_ref(), publish.capture.unwrap());
    assert_ne!(second_request, first_request);
    evidence(format_args!(
        "PRODUCT_COMMIT_REFUSAL case=writer_held_at_publication lifecycle_slots={slots} phase={:?} error={:?} locally_settled={} namespace=Some released={} request={second_request} published=None work={:?}",
        publish.phase,
        publish.error,
        publish.locally_settled,
        namespace.released.len(),
        namespace.work
    ));
    drop(publish);
    assert_eq!(retained(&rig, &workspace, second_request), (None, None));
    unpublished(
        &rig,
        &workspace,
        &original,
        &expected,
        "after Busy at publication",
    );
    // The refused captures were folded forward. More changes, then one later
    // explicit Commit is exact.
    session.all(&[
        Do::Write("a/one", 5, b"again after the refusals"),
        Do::Create("made/later", 0o600),
        Do::Rename("made/b", "a/b"),
        Do::Remove("keep/alias2"),
    ]);
    let expected = session.model.flat();
    model::assert_same(
        &expected,
        &live(&workspace),
        "live view before the later Commit",
    );
    let later = workspace
        .commit_captured()
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    let published = record(&later);
    assert_eq!(published.parent, original.branch.head_commit);
    let request = receipt(later.namespace.as_ref(), later.capture);
    drop(later);
    assert_eq!(retained(&rig, &workspace, request), (None, None));
    model::assert_same(
        &expected,
        &rebound(&rig, 2, published.root),
        "fresh bind after the refusals",
    );
    model::assert_same(&expected, &live(&workspace), "live view after install");
    assert_eq!(history(&rig), vec![published]);
    evidence(format_args!(
        "PRODUCT_COMMIT_REFUSAL_LATER lifecycle_slots={slots} later_commit=Committed fresh_bind=exact live=exact owners_released=true"
    ));
    settled(&rig.owner.client(), 0);
    rig.close(&workspace);
    drop((workspace, handles));
    rig.finish();
}

/// The local owner's default admission, the same as `Rig::new` and the native
/// control fixtures use: two Lifecycle slots for each namespace.
#[test]
fn the_product_constructor_classifies_and_settles_a_definite_refusal() {
    definite_refusal("product-commit-refusal", OwnerConfig::default());
}

/// The same body under the owner admission `native_application.rs` gives the
/// real binary: four Lifecycle slots for each namespace. Kept beside the
/// default case so the two receipts differ in the admission alone.
#[test]
fn a_definite_refusal_under_four_lifecycle_slots_releases_both_owners() {
    definite_refusal(
        "product-commit-refusal-four",
        OwnerConfig {
            bytes: 16 * 1024 * 1024,
            lifecycle_reserve: 2 * 1024 * 1024,
            namespaces: 8,
            jobs_per_namespace: 16,
            lifecycle_jobs_per_namespace: 4,
        },
    );
}

/// Scope: the external catalog boundary reports an unknown outcome after the
/// real publication took effect. This is not a native SQLite I/O failure.
#[test]
fn an_unknown_history_outcome_keeps_the_reader_and_owner_with_the_failure() {
    let (rig, handles) = observed(
        "product-commit-unknown",
        Boundary::LostAcknowledgement,
        OwnerConfig::default(),
    );
    let workspace = rig.bind(1);
    let original = workspace.snapshot().unwrap();
    let mut session = Session::new(&workspace, rig.base.clone());
    session.all(&[
        Do::Write("a/one", 0, b"published, outcome unknown"),
        Do::Mkdir("made", 0o755),
        Do::Remove("gone/x"),
    ]);
    let expected = session.model.flat();
    let failure = workspace.commit_captured().unwrap_err();
    assert_eq!(failure.phase, CommitPhase::Publish);
    assert!(
        matches!(
            failure.error,
            CommitError::History(HistoryError::UnknownOutcome)
        ),
        "{:?}",
        failure.error
    );
    assert!(!failure.locally_settled && failure.published.is_none());
    assert!(failure.local.is_none() && failure.local_error.is_none());
    assert!(failure.intent.is_some() && failure.prepared.is_some());
    assert!(workspace.commit_in_flight());
    // Nothing was released: the failure names the reader and the owner, and
    // the Overlay answers the same two for the same request.
    let namespace = failure.namespace.as_ref().expect("the product closure ran");
    assert!(namespace.retained(), "{namespace:?}");
    assert!(namespace.released.is_empty() && namespace.release_error.is_none());
    let (reader, operation) = (namespace.reader.unwrap(), namespace.operation.unwrap());
    let capture = failure.capture.unwrap();
    assert_eq!(reader.capture(), capture);
    let request = request(capture);
    assert_eq!(
        retained(&rig, &workspace, request),
        (Some(reader), Some(operation))
    );
    // A second Commit is refused before any effect and changes no custody.
    let second = workspace.commit_captured().unwrap_err();
    assert!(
        matches!(second.error, CommitError::InFlight),
        "{:?}",
        second.error
    );
    assert_eq!(second.phase, CommitPhase::Admission);
    assert!(second.namespace.is_none() && second.capture.is_none());
    assert_eq!(
        retained(&rig, &workspace, request),
        (Some(reader), Some(operation))
    );
    // The local binding and the live view are what they were.
    assert_eq!(workspace.snapshot().unwrap(), original);
    model::assert_same(
        &expected,
        &live(&workspace),
        "live view under the retained unknown",
    );
    // The catalog did take the publication; reading that settles nothing.
    let observed = handles
        .history
        .branch_snapshot(rig.f.branch)
        .unwrap()
        .unwrap();
    assert_eq!(
        observed.effective_root,
        failure.intent.as_ref().unwrap().candidate_root
    );
    assert!(workspace.commit_in_flight());
    assert_eq!(
        retained(&rig, &workspace, request),
        (Some(reader), Some(operation))
    );
    evidence(format_args!(
        "PRODUCT_COMMIT_UNKNOWN scope=external catalog boundary scope, not a native SQLite I/O failure phase={:?} error={:?} locally_settled={} retained_reader=Some retained_operation=Some request={request} overlay_answers_same=true second_commit=InFlight observer_does_not_resolve=true teardown=explicit_after_owner_stop",
        failure.phase, failure.error, failure.locally_settled
    ));
    // The unknown has no resolver: explicit fixture teardown after owner stop.
    let Rig {
        f, store, owner, ..
    } = rig;
    owner.stop().unwrap();
    drop((failure, second, workspace, store, handles));
    f.cleanup();
}

/// Admission is a readiness wait before the Commit's one attempt, as it is
/// for a filesystem request. With every ordinary credit of the Workspace held
/// by another caller the Commit neither fails nor finishes; when the credits
/// are released it runs once and is exact.
#[test]
fn a_commit_waits_for_owner_admission_and_is_not_refused() {
    let rig = Rig::new("product-commit-admission", tree);
    let workspace = rig.bind(1);
    let mut session = Session::new(&workspace, rig.base.clone());
    session.all(&[
        Do::Create("waited.txt", 0o644),
        Do::Write("waited.txt", 0, b"committed after the wait"),
        Do::Remove("gone/x"),
    ]);
    let expected = session.model.flat();
    let client = rig.owner.client();
    let slots = rig.owner.configuration().jobs_per_namespace;
    // Each held job is counted from an exactly settled owner: the engine
    // drops its own credit just after a completion is observed.
    settled(&client, 0);
    let mut held: Vec<Completion> = Vec::new();
    for count in 0..slots {
        held.push(job(&client, workspace.route(), Command::Inode(1)));
        settled(&client, count + 1);
    }
    assert!(matches!(
        client.try_submit(Some(workspace.route()), Command::Inode(1)),
        Err((OwnerError::AdmissionFull, Command::Inode(1)))
    ));
    let admitted_before = client.diagnostics().unwrap().admitted;
    let finished = AtomicBool::new(false);
    let success = std::thread::scope(|scope| {
        // Owned here so an assertion failure below releases the credits
        // before the scope joins the Commit thread.
        let held = held;
        let commit = scope.spawn(|| {
            let result = workspace.commit_captured();
            finished.store(true, Ordering::Release);
            result
        });
        // An observation window, not a threshold: nothing may happen in it.
        let until = Instant::now() + Duration::from_millis(400);
        while Instant::now() < until {
            assert!(!finished.load(Ordering::Acquire), "Commit ended early");
            std::thread::yield_now();
        }
        assert_eq!(
            client.diagnostics().unwrap().admitted,
            admitted_before,
            "no Commit job was admitted while the credits were held"
        );
        drop(held);
        commit.join().unwrap()
    })
    .unwrap_or_else(|failure| panic!("{failure:?}"));
    let published = record(&success);
    receipt(success.namespace.as_ref(), success.capture);
    evidence(format_args!(
        "PRODUCT_COMMIT_ADMISSION held_ordinary_credits={slots} refused=false admitted_while_held=0 outcome=Committed"
    ));
    drop(success);
    model::assert_same(&expected, &rebound(&rig, 2, published.root), "fresh bind");
    model::assert_same(&expected, &live(&workspace), "live view after install");
    settled(&client, 0);
    rig.close(&workspace);
    rig.finish();
}
