//! Row R4-2 through the product Commit constructor: a second parent of a
//! stored directory, and a stored directory bound only below itself, written
//! as real rows into the real engine and then committed with
//! `BoundWorkspace::commit_captured`, over a real Disposable Store and the
//! real local owner, without a mount.
//!
//! Workspace operations cannot state either shape. The rows are written with
//! the engine's public `Command::Publish`, whose name row is independent of
//! the inode row published with it: each publish repeats, unchanged, the
//! local inode row of one ordinary file the capture already holds, read back
//! with `Command::Inode`, and adds one name row. No provider answer is
//! changed and no product hook exists: the product constructor reads the
//! rows the engine really holds.
//!
//! The refusal is Content's own (`InvalidRecord`), which the driver treats as
//! definite: it resolves the capture locally and the constructor releases its
//! reader and its operation owner. Nothing is published. The rows are then
//! taken back with further publishes and a later Commit is exact.
#[allow(dead_code)]
#[path = "support/captured_drive.rs"]
mod drive;
#[allow(dead_code)]
#[path = "support/namespace_model.rs"]
mod model;
#[allow(dead_code)]
#[path = "support/installed_store.rs"]
mod support;
use drive::{evidence, job, live, settled, window, Do, Rig, Session};
use layerfs_content::{filesystem::PathName, ContentError, ObjectId};
use layerfs_daemon::{
    store::{
        BoundWorkspace, CapturedConstruction, CommitError, CommitPhase, CommitSuccess,
        ReleasedOwner,
    },
    Command, Response,
};
use layerfs_history::{CommitHistoryRequest, CommitRecord, CommitStagedOutcome};
use layerfs_overlay::{Capture, CapturedReader, DirectoryEntry, Inode, OperationOwner};
use std::{fs, path::Path};

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
/// What the engine itself still holds for one request, asked after the Commit.
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
/// reader and then the operation owner were released, nothing is kept.
fn released(namespace: Option<&CapturedConstruction>, capture: Capture) -> u64 {
    let namespace = namespace.expect("the product constructor ran");
    assert!(!namespace.retained(), "{namespace:?}");
    assert!(namespace.reader.is_none() && namespace.operation.is_none());
    assert!(
        namespace.custody.is_none()
            && namespace.release_error.is_none()
            && namespace.release_failure.is_none(),
        "{namespace:?}"
    );
    assert_eq!(
        namespace.released,
        [ReleasedOwner::Reader, ReleasedOwner::Operation],
        "{namespace:?}"
    );
    request(capture)
}
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
/// Stored directories `a`, `a/b` and `keep`, each with a file.
fn tree(source: &Path) {
    for directory in ["a", "a/b", "keep"] {
        fs::create_dir_all(source.join(directory)).unwrap();
    }
    for (path, bytes) in [
        ("top.txt", &b"top level\n"[..]),
        ("a/one", b"one\n"),
        ("a/b/leaf", b"leaf\n"),
        ("keep/k", b"kept\n"),
    ] {
        fs::write(source.join(path), bytes).unwrap();
    }
}
/// The serial the live view binds at a path; the empty path is the root.
fn serial(workspace: &BoundWorkspace, path: &str) -> u64 {
    window(workspace, |operation, view| {
        let mut stat = view.stat(operation.overlay(), view.root_serial()).unwrap();
        for part in path.split('/').filter(|part| !part.is_empty()) {
            stat = view
                .lookup(
                    operation.overlay(),
                    stat.serial,
                    &PathName::new(part).unwrap(),
                )
                .unwrap_or_else(|error| panic!("lookup {part}: {error:?}"));
        }
        stat.serial
    })
}
/// The serials the rows name.
struct Serials {
    root: u64,
    a: u64,
    b: u64,
    keep: u64,
}
fn row(parent: u64, name: &str, serial: Option<u64>, inherited: bool) -> DirectoryEntry {
    DirectoryEntry {
        inherited,
        parent,
        name: name.as_bytes().to_vec(),
        serial,
    }
}
/// One real `Command::Publish`: the unchanged inode row and one name row.
fn publish(rig: &Rig, workspace: &BoundWorkspace, inode: &Inode, name: DirectoryEntry) {
    let client = rig.owner.client();
    let done = job(
        &client,
        workspace.route(),
        Command::Publish {
            inode: inode.clone(),
            name: Some(name),
            cell: None,
        },
    );
    let ticket = match done.result() {
        Ok(Response::Published(ticket)) => *ticket,
        other => panic!("{other:?}"),
    };
    drop(done);
    assert!(
        job(&client, workspace.route(), Command::ReplyAttempted(ticket))
            .result()
            .is_ok()
    );
}

/// `rows` gives the name rows that state the shape, and the name rows that
/// take it back, from the serials of the stored directories.
fn refused(
    label: &str,
    what: &str,
    cause: &'static str,
    rows: impl FnOnce(&Serials) -> (Vec<DirectoryEntry>, Vec<DirectoryEntry>),
) {
    let rig = Rig::new(label, tree);
    let first = rig.bind(1);
    let original = first.snapshot().unwrap();
    assert!(history(&rig).is_empty());
    // One ordinary publication, so the capture is an ordinary one.
    let mut session = Session::new(&first, rig.base.clone());
    session.all(&[
        Do::Create("real", 0o644),
        Do::Write("real", 0, b"an ordinary change"),
    ]);
    let expected = session.model.flat();
    model::assert_same(&expected, &live(&first), "live view before the rows");
    let serials = Serials {
        root: serial(&first, ""),
        a: serial(&first, "a"),
        b: serial(&first, "a/b"),
        keep: serial(&first, "keep"),
    };
    // The ordinary file's own local row, exactly as the engine holds it.
    let real = serial(&first, "real");
    let client = rig.owner.client();
    let inode = match job(&client, first.route(), Command::Inode(real)).result() {
        Ok(Response::Inode(Some(inode))) => inode.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(inode.serial, real);
    let (stated, taken_back) = rows(&serials);
    for name in &stated {
        publish(&rig, &first, &inode, name.clone());
    }
    evidence(format_args!(
        "R4-2 {what} STAGED how=Command::Publish of the unchanged inode row of serial {real} with each name row; rows={stated:?} serials(root={} a={} a/b={} keep={})",
        serials.root, serials.a, serials.b, serials.keep
    ));

    // The product constructor reads those rows and Content refuses the shape.
    let failure = first
        .commit_captured()
        .expect_err("the shape must be refused");
    evidence(format_args!(
        "R4-2 {what} FAILURE phase={:?} error={:?} locally_settled={} published={:?} local={} local_error={:?} intent={} namespace={:?}",
        failure.phase,
        failure.error,
        failure.locally_settled,
        failure.published,
        failure.local.is_some(),
        failure.local_error,
        failure.intent.is_some(),
        failure.namespace
    ));
    assert_eq!(failure.phase, CommitPhase::Construct);
    assert!(
        matches!(
            &failure.error,
            CommitError::Content {
                error: ContentError::InvalidRecord(found),
                provider: None
            } if *found == cause
        ),
        "{what}: {:?}",
        failure.error
    );
    // Definite: nothing published, the capture resolved locally, and the
    // constructor's reader and operation owner released in that order.
    assert!(failure.published.is_none() && failure.intent.is_none());
    assert!(
        failure.locally_settled && failure.local_error.is_none(),
        "{failure:?}"
    );
    let namespace = failure.namespace.as_ref().expect("the constructor ran");
    assert!(
        namespace.work.is_some() && namespace.counters.is_none(),
        "{namespace:?}"
    );
    let first_request = released(failure.namespace.as_ref(), failure.capture.unwrap());
    drop(failure);
    assert_eq!(retained(&rig, &first, first_request), (None, None));
    assert!(matches!(
        job(&client, first.route(), Command::RetainedCapture).result(),
        Ok(Response::RetainedCapture(None))
    ));
    assert!(!first.commit_in_flight());
    assert!(rig
        .store
        .history()
        .stage(first.identity())
        .unwrap()
        .is_none());
    assert_eq!(first.snapshot().unwrap(), original);
    assert!(history(&rig).is_empty());
    evidence(format_args!(
        "R4-2 {what} CLASSIFIED cause=Content(InvalidRecord({cause:?})) phase=Construct definite=true settled=true released=reader,operation engine_retains=(None,None) capture_retained=None stage=None head_unchanged=true commits=0"
    ));

    // The rows are taken back; the live view is the ordinary change alone
    // and a later Commit is exact.
    for name in &taken_back {
        publish(&rig, &first, &inode, name.clone());
    }
    model::assert_same(
        &expected,
        &live(&first),
        "live view after the rows are gone",
    );
    let later = first
        .commit_captured()
        .unwrap_or_else(|failure| panic!("{what}: the later Commit: {failure:?}"));
    let published = record(&later);
    assert_eq!(published.parent, original.branch.head_commit);
    let later_request = released(later.namespace.as_ref(), later.capture);
    assert_ne!(later_request, first_request);
    drop(later);
    assert_eq!(retained(&rig, &first, later_request), (None, None));
    model::assert_same(
        &expected,
        &rebound(&rig, 2, published.root),
        "fresh bind after the refusal",
    );
    model::assert_same(&expected, &live(&first), "live view after install");
    assert_eq!(history(&rig), vec![published]);
    evidence(format_args!(
        "R4-2 {what} RESULT rows_taken_back={taken_back:?} later_commit=Committed fresh_bind=exact live=exact paths={}",
        expected.len()
    ));
    settled(&client, 0);
    rig.close(&first);
    drop(first);
    rig.finish();
}

/// The stored directory `a` is bound at the root; the row binds it again
/// under the stored directory `keep`.
#[test]
fn r4_2_a_second_parent_of_a_stored_directory_is_refused_by_the_product_constructor() {
    refused("r4-2-alias", "alias", "multiple parents", |serials| {
        (
            vec![row(serials.keep, "second", Some(serials.a), false)],
            vec![row(serials.keep, "second", None, false)],
        )
    });
}

/// The stored directory `a` loses its name at the root and is bound only
/// below its own child `a/b`.
#[test]
fn r4_2_a_stored_directory_bound_only_below_itself_is_refused_by_the_product_constructor() {
    refused("r4-2-cycle", "cycle", "effective tree cycle", |serials| {
        (
            vec![
                row(serials.root, "a", None, true),
                row(serials.b, "a", Some(serials.a), false),
            ],
            vec![
                row(serials.b, "a", None, false),
                row(serials.root, "a", Some(serials.a), true),
            ],
        )
    });
}
