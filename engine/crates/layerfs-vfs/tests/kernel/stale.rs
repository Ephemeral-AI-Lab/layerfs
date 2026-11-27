//! What the kernel holds across an install that changed it, and listings
//! whose base page is cut short by long names.
//!
//! - A serial the kernel holds that an install dropped is stale for each
//!   request that names it, and the Workspace keeps serving (finding #7).
//! - A kernel rename checks every retained parent of its destination against
//!   the current view, so a directory an install moved cannot be made part of
//!   a cycle through a stale parent row (finding #17).
//! - A directory of maximum-length base names with local names after them
//!   lists every name exactly once, through the SDK and through READDIR
//!   (finding #12).
#[path = "../support/fixture.rs"]
mod common;
#[path = "../support/harness.rs"]
mod harness;
use common::name;
use harness::{rename, unlink, Bench, T1};
use layerfs_content::{
    filesystem::{
        limits::MAXIMUM_NAME_BYTES, update_filesystem, DirectoryUpdate, FilesystemInput,
        FilesystemObjects, FilesystemResources, PathName,
    },
    ContentError,
};
use layerfs_overlay::{Inode, KernelDirectory, KernelMount, OpenFile, OverlayError};
use layerfs_vfs::{CanonicalCache, CanonicalClient, JobOutcome, KernelDirectoryRead, KernelInput, KernelReadDecision, KernelReadOperation, KernelVisitRequest, Operation, Refusal, VisitFacts, WorkspaceError, WorkspaceResult};
use std::sync::Arc;

/// A memory-only client over a cache that holds nothing.
fn empty() -> Arc<CanonicalClient> {
    Arc::new(CanonicalClient::resident(Arc::new(CanonicalCache::new(
        1 << 20,
    ))))
}
fn lookup(parent: u64, child: &str) -> KernelReadOperation {
    KernelReadOperation::Lookup {
        parent,
        name: name(child),
    }
}
/// Installs the present base with `updates` as the next base, through the
/// actual canonical builder and install port, as a merged install does.
fn install(b: &Bench, updates: Vec<DirectoryUpdate>) {
    let store = &b.fixture.store;
    let mut sink = store.clone();
    let mut objects = FilesystemObjects::new(store, &mut sink);
    let next = update_filesystem(
        &mut objects,
        &FilesystemInput {
            base: Some(b.workspace.base().unwrap().identity()),
            scope: b.fixture.scope,
            root_serial: 1,
            directories: &updates,
            inodes: &[],
            new_inodes: &[],
            resources: FilesystemResources::default(),
        },
        None,
    )
    .unwrap()
    .root;
    let capture = b.overlay.capture().unwrap();
    let prepared = b.workspace.prepare_base_install(capture, next).unwrap();
    b.workspace
        .install_prepared_base(&b.overlay, prepared)
        .unwrap();
}
fn changes(parent: u64, names: &[(&str, Option<u64>)]) -> DirectoryUpdate {
    DirectoryUpdate {
        parent,
        changes: names
            .iter()
            .map(|(child, serial)| (name(child), *serial))
            .collect(),
    }
}
#[derive(Debug)]
enum Seen {
    Value(Inode),
    Refused(Refusal),
    Stale,
}
/// One kernel read (LOOKUP, GETATTR, OPEN, OPENDIR) as owner visits until it
/// decides, reading the base facts an undecided visit names; with the
/// descriptor a decided OPEN or OPENDIR recorded.
fn read(
    b: &Bench,
    mount: KernelMount,
    request: u64,
    serial: u64,
    operation: KernelReadOperation,
) -> (Seen, Option<OpenFile>, Option<KernelDirectory>) {
    let mut facts = VisitFacts::default();
    for _ in 0..4 {
        let shared = Arc::new(facts.clone());
        let visit = match operation {
            KernelReadOperation::Open { writable, .. } => {
                b.workspace
                    .kernel_open_visit(empty(), mount, request, serial, writable, shared)
            }
            KernelReadOperation::Opendir { .. } => {
                b.workspace
                    .kernel_opendir_visit(empty(), mount, request, serial, shared)
            }
            _ => b.workspace.kernel_read_visit(
                empty(),
                mount,
                serial,
                None,
                operation.clone(),
                shared,
            ),
        }
        .unwrap();
        let outcome = visit.perform(&b.overlay);
        match (&outcome.result, &outcome.decision) {
            (Err(OverlayError::Stale), None) => {
                assert!(outcome.open_candidate.is_none());
                assert!(outcome.directory_candidate.is_none());
                return (Seen::Stale, None, None);
            }
            (Ok(None), Some(KernelReadDecision::Needs(needs))) => facts
                .supply(&b.workspace.base().unwrap(), needs, None)
                .unwrap(),
            (Ok(None), Some(KernelReadDecision::Value(decided))) => {
                return (
                    Seen::Value(decided.inode.clone()),
                    outcome.open_candidate,
                    outcome.directory_candidate,
                )
            }
            (Ok(None), Some(KernelReadDecision::Refused { refusal, .. })) => {
                return (Seen::Refused(*refusal), None, None)
            }
            _ => panic!("undecided kernel read {operation:?}: {outcome:?}"),
        }
    }
    panic!("kernel read {operation:?} was not decided in four visits")
}
/// The decided inode of a kernel read that must decide one.
fn value(b: &Bench, mount: KernelMount, serial: u64, operation: KernelReadOperation) -> Inode {
    match read(b, mount, 0, serial, operation.clone()).0 {
        Seen::Value(inode) => inode,
        other => panic!("{operation:?}: {other:?}"),
    }
}
/// One kernel mutation named by the directory `serial`, as owner visits
/// until it decides; an applied one has its reply attempted.
fn mutate(
    b: &Bench,
    mount: KernelMount,
    request: u64,
    serial: u64,
    operation: Operation,
) -> WorkspaceResult<JobOutcome> {
    let mut facts = VisitFacts::default();
    for _ in 0..4 {
        let visit = b
            .workspace
            .kernel_mutation_visit(
                empty(),
                KernelVisitRequest {
                    mount,
                    request,
                    serial,
                    handle: None,
                    input: KernelInput::Named(operation.clone()),
                    open: None,
                    now: T1,
                    fresh: None,
                    facts: Arc::new(facts.clone()),
                },
            )
            .unwrap();
        match visit.perform(&b.overlay).result {
            Ok(JobOutcome::Needs(needs)) => facts
                .supply(&b.workspace.base().unwrap(), &needs, None)
                .unwrap(),
            Ok(JobOutcome::Applied { publication, inode }) => {
                b.overlay.reply_attempted(publication).unwrap();
                return Ok(JobOutcome::Applied { publication, inode });
            }
            other => return other,
        }
    }
    panic!("kernel mutation {operation:?} was not decided in four visits")
}

#[test]
fn a_serial_an_install_dropped_is_stale_for_each_kernel_request_and_the_mount_serves_on() {
    let b = Bench::new("kernel-stale-dropped");
    let mount = b.overlay.create_kernel_mount(1).unwrap();
    // The kernel holds the directory `cache` (6) and the file `file` (2,
    // also named `alias`), and has both open.
    assert_eq!(value(&b, mount, 1, lookup(1, "cache")).serial, 6);
    assert_eq!(value(&b, mount, 1, lookup(1, "file")).serial, 2);
    let directory = read(&b, mount, 10, 6, KernelReadOperation::Opendir { serial: 6 })
        .2
        .expect("cache is open");
    let file = read(
        &b,
        mount,
        11,
        2,
        KernelReadOperation::Open {
            serial: 2,
            writable: false,
        },
    )
    .1
    .expect("file is open");
    // A merged install drops both serials: no name binds them any more.
    install(
        &b,
        vec![changes(
            1,
            &[("alias", None), ("cache", None), ("file", None)],
        )],
    );
    let base = b.workspace.base().unwrap();
    for serial in [2, 6] {
        assert!(
            matches!(base.inode(serial), Err(ContentError::PathNotFound)),
            "serial {serial} is still in the installed base"
        );
        assert_eq!(b.overlay.inode(serial).unwrap(), None);
    }

    // Every kernel read of either serial is stale: GETATTR, a LOOKUP in the
    // dropped directory, OPENDIR and OPEN.
    for (request, serial, operation) in [
        (20, 6, KernelReadOperation::Getattr { serial: 6 }),
        (21, 6, lookup(6, "state")),
        (22, 6, KernelReadOperation::Opendir { serial: 6 }),
        (23, 2, KernelReadOperation::Getattr { serial: 2 }),
        (
            24,
            2,
            KernelReadOperation::Open {
                serial: 2,
                writable: false,
            },
        ),
    ] {
        let seen = read(&b, mount, request, serial, operation.clone()).0;
        assert!(matches!(seen, Seen::Stale), "{operation:?}: {seen:?}");
    }
    // READ through the descriptor opened before the install: the one base
    // read finds no inode at the root the visit named.
    let window = b
        .workspace
        .kernel_data_visit(empty(), mount, 2, Some(file.owner_id()), 0, 4096)
        .unwrap()
        .perform(&b.overlay)
        .unwrap();
    assert!(window.local.is_none() && window.inherits(0, false));
    assert!(matches!(
        window.finish(Some(&base), 2, 0, 4096, false),
        Err(WorkspaceError::Overlay(OverlayError::Stale))
    ));
    // READDIR through the handle opened before the install.
    let window = b
        .workspace
        .kernel_directory_visit(
            empty(),
            mount,
            6,
            directory.owner_id(),
            KernelDirectoryRead {
                offset: 2,
                after: None,
                attributes: false,
            },
        )
        .unwrap()
        .perform(&b.overlay)
        .unwrap();
    assert!(matches!(
        window.finish(Some(&base)),
        Err(WorkspaceError::Overlay(OverlayError::Stale))
    ));

    // Nothing was retained or poisoned: the same mount keeps serving reads
    // and mutations.
    assert_eq!(
        value(&b, mount, 1, KernelReadOperation::Getattr { serial: 1 }).serial,
        1
    );
    assert_eq!(value(&b, mount, 1, lookup(1, "node_modules")).serial, 5);
    assert!(matches!(
        read(&b, mount, 30, 1, lookup(1, "cache")).0,
        Seen::Refused(Refusal::Missing)
    ));
    assert!(matches!(
        mutate(&b, mount, 31, 1, unlink(1, "symlink")),
        Ok(JobOutcome::Applied { .. })
    ));
    assert_eq!(b.lookup(1, "symlink"), None);
    b.overlay
        .kernel_close_file(mount, 2, file.owner_id())
        .unwrap();
    b.overlay
        .close_kernel_directory(mount, 6, directory.owner_id())
        .unwrap();
    b.overlay.revoke_kernel_mount(mount).unwrap();
}

#[test]
fn a_kernel_rename_checks_each_retained_parent_against_the_current_view() {
    let b = Bench::new("kernel-stale-parents");
    let mount = b.overlay.create_kernel_mount(1).unwrap();
    // The kernel looks up `cache` (6) and `output` (7) at the root.
    assert_eq!(value(&b, mount, 1, lookup(1, "cache")).serial, 6);
    assert_eq!(value(&b, mount, 1, lookup(1, "output")).serial, 7);
    // A merged install moves `output` under `cache` as `y`; it keeps its
    // serial, and nothing tells the kernel's parent index.
    install(
        &b,
        vec![
            changes(1, &[("output", None)]),
            changes(6, &[("y", Some(7))]),
        ],
    );
    assert_eq!(b.lookup(6, "y").unwrap().serial, 7);
    assert_eq!(b.lookup(1, "output"), None);

    // rename(/cache, /cache/y/x2) through a handle on 7 that the kernel
    // still has from the root. The retained row says 7 is `output` at the
    // root; the view disagrees, so the rename is refused and moves nothing.
    let cycle = || rename((1, "cache"), (7, "x2"), false, None);
    assert!(matches!(
        mutate(&b, mount, 40, 1, cycle()),
        Ok(JobOutcome::Refused(Refusal::AncestryMismatch))
    ));
    assert_eq!(b.lookup(1, "cache").unwrap().serial, 6);
    assert_eq!(b.lookup(7, "x2"), None);
    // A LOOKUP of 7 at its new place records it there: the same rename is
    // now a proven cycle.
    assert_eq!(value(&b, mount, 6, lookup(6, "y")).serial, 7);
    assert!(matches!(
        mutate(&b, mount, 41, 1, cycle()),
        Ok(JobOutcome::Refused(Refusal::Invalid))
    ));
    assert_eq!(b.lookup(1, "cache").unwrap().serial, 6);
    // A directory that is not above 7 moves into it.
    assert!(matches!(
        mutate(
            &b,
            mount,
            42,
            1,
            rename((1, "node_modules"), (7, "nm"), false, None)
        ),
        Ok(JobOutcome::Applied { .. })
    ));
    assert_eq!(b.lookup(7, "nm").unwrap().serial, 5);
    // A rename in place moves the retained name with the directory: after
    // `y` becomes `y2`, the chain 7 -> 6 -> root still checks out.
    assert!(matches!(
        mutate(&b, mount, 43, 6, rename((6, "y"), (6, "y2"), false, None)),
        Ok(JobOutcome::Applied { .. })
    ));
    assert!(matches!(
        mutate(
            &b,
            mount,
            44,
            1,
            rename((1, ".git"), (7, "git"), false, None)
        ),
        Ok(JobOutcome::Applied { .. })
    ));
    assert_eq!(b.lookup(7, "git").unwrap().serial, 4);
    // And moving `cache` under its own descendant is still a cycle.
    assert!(matches!(
        mutate(&b, mount, 45, 1, cycle()),
        Ok(JobOutcome::Refused(Refusal::Invalid))
    ));
    b.overlay.revoke_kernel_mount(mount).unwrap();
}

/// `count` base names of the maximum length, all sorting after `.git` and
/// `alias` and before every other fixture name.
fn long_names(count: usize) -> Vec<String> {
    (0..count)
        .map(|n| format!("b{n:0>width$}", width = MAXIMUM_NAME_BYTES - 1))
        .collect()
}

#[test]
fn maximum_length_base_names_with_local_names_after_them_are_each_listed_once() {
    let b = Bench::new("kernel-stale-long-names");
    // More maximum-length names than one base page of PAGE_ROWS rows would
    // hold if a row were charged less than its full width.
    let long = long_names(1_100);
    install(
        &b,
        vec![DirectoryUpdate {
            parent: 1,
            changes: long
                .iter()
                .map(|child| (PathName::new(child).unwrap(), Some(2)))
                .collect(),
        }],
    );
    // Local names after every long one, and one removed among them.
    for child in ["zz-local-a", "zz-local-b"] {
        b.applied(harness::create(1, child), T1);
    }
    b.applied(unlink(1, &long[1_050]), T1);
    let mut expected: Vec<String> = [
        ".git",
        "alias",
        "cache",
        "file",
        "node_modules",
        "output",
        "symlink",
        "zz-local-a",
        "zz-local-b",
    ]
    .into_iter()
    .map(String::from)
    .chain(long.iter().cloned())
    .filter(|child| *child != long[1_050])
    .collect();
    expected.sort();

    // SDK listing: bounded windows, each name exactly once, in order.
    assert_eq!(b.names(1), expected);

    // Kernel READDIR: each reply publishes what it lists, and the next
    // request resumes at the last offset handed out.
    let mount = b.overlay.create_kernel_mount(1).unwrap();
    let directory = read(&b, mount, 50, 1, KernelReadOperation::Opendir { serial: 1 })
        .2
        .expect("the root is open");
    let base = b.workspace.base().unwrap();
    let mut listed = Vec::new();
    let (mut offset, mut after) = (2, None);
    for _ in 0..64 {
        let batch = b
            .workspace
            .kernel_directory_visit(
                empty(),
                mount,
                1,
                directory.owner_id(),
                KernelDirectoryRead {
                    offset,
                    after: after.take(),
                    attributes: false,
                },
            )
            .unwrap()
            .perform(&b.overlay)
            .unwrap()
            .finish(Some(&base))
            .unwrap();
        if batch.entries.is_empty() {
            match batch.continuation {
                Some(next) => {
                    after = Some(next);
                    continue;
                }
                None => break,
            }
        }
        let names: Vec<Vec<u8>> = batch.entries.iter().map(|e| e.name.clone()).collect();
        let offer = batch.publish.expect("a fresh reply publishes its names");
        b.overlay.publish_kernel_cookies(&offer, &names).unwrap();
        listed.extend(names.iter().map(|n| String::from_utf8(n.clone()).unwrap()));
        if batch.exhausted {
            break;
        }
        offset = batch.first + names.len() as u64 - 1;
    }
    assert_eq!(listed, expected);
    b.overlay
        .close_kernel_directory(mount, 1, directory.owner_id())
        .unwrap();
    b.overlay.revoke_kernel_mount(mount).unwrap();
}
