//! The one-statement fence of a native visit and the custody writes behind
//! it: everything the separate Workspace, mount, reference and descriptor
//! reads used to refuse is still refused, with the same outcome, and nothing
//! is written by a refused visit.
use layerfs_overlay::{
    Binding, Changes, CleanupState, DirectoryEntryChange, Inode, InodeKind, MaintenanceCursor,
    NativeDecision, NativeEffect, NativeMount, OpenFile, Overlay, OverlayError, PayloadWrite,
    ProfileConfig, Publication, Route, StatementKind, StoredCounts, WorkspaceState, READ_WINDOW,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct World {
    db: Overlay,
    route: Route,
    mount: NativeMount,
    path: PathBuf,
}
impl Drop for World {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.path).unwrap();
    }
}
fn world() -> World {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let path = std::env::temp_dir().join(format!(
        "layerfs-visit-fence-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&path).unwrap();
    let db = Overlay::create(&path.join("overlay"), ProfileConfig::default()).unwrap();
    let route = db.open_workspace([31; 32], [32; 32]).unwrap();
    let mount = db.create_native_mount(route, 1).unwrap();
    World {
        db,
        route,
        mount,
        path,
    }
}
fn inode(serial: u64) -> Inode {
    Inode {
        serial,
        kind: InodeKind::File,
        mode: 0o644,
        mtime_seconds: 7,
        mtime_nanoseconds: 9,
        nlink: 1,
        size: 0,
        inherited_cutoff: 0,
        born: 0,
        entries: 0,
        subdirs: 0,
    }
}
/// The file `serial` created under the root at the active generation.
fn created(active: i64, serial: u64, name: &[u8]) -> Changes {
    Changes {
        created: Some(serial),
        inodes: vec![
            Inode {
                kind: InodeKind::Directory,
                mode: 0o755,
                entries: 1,
                ..inode(1)
            },
            Inode {
                born: active as u64,
                ..inode(serial)
            },
        ],
        directory_entries: vec![DirectoryEntryChange {
            parent: 1,
            name: name.to_vec(),
            binding: Binding::Bound {
                serial,
                inherited: false,
            },
        }],
        ..Changes::default()
    }
}
impl World {
    /// CREATE with an open descriptor through one visit, reply attempted.
    fn open(&self, mount: NativeMount, request: u64, serial: u64, writable: bool) -> OpenFile {
        let applied = self
            .db
            .mutate_native_visit(mount, request, 1, None, |rows, _| {
                Ok(Some((
                    created(
                        rows.active().number(),
                        serial,
                        format!("f{serial}").as_bytes(),
                    ),
                    NativeEffect::Open {
                        serial,
                        parent: 1,
                        writable,
                    },
                )))
            })
            .unwrap()
            .expect("published");
        self.db.reply_attempted(applied.publication).unwrap();
        applied.file.expect("the created file is open")
    }
    /// The Workspace row, its exact counts and the transactions begun so far.
    fn snapshot(&self, route: Route) -> (WorkspaceState, StoredCounts, u64) {
        (
            self.db.state(route).unwrap(),
            self.db.resources(Some(route)).unwrap().counts,
            self.db.diagnostics().statements[StatementKind::Begin as usize].executions,
        )
    }
    /// A visit addressed by `handle` that would write one more byte of size.
    fn grow(
        &self,
        mount: NativeMount,
        serial: u64,
        handle: Option<u64>,
        open: Option<OpenFile>,
    ) -> Result<Option<Publication>, OverlayError> {
        self.db
            .mutate_native_visit(mount, 900, serial, handle, |_, file| {
                Ok(Some((
                    Changes {
                        open: open.or(file),
                        inodes: vec![Inode {
                            size: 1,
                            ..inode(open.or(file).map_or(serial, |file| file.serial()))
                        }],
                        ..Changes::default()
                    },
                    NativeEffect::None,
                )))
            })
            .map(|applied| applied.map(|applied| applied.publication))
    }
    /// The refusal of a GETATTR visit. A READ window of the same inode
    /// through the same reference is refused the same way.
    fn read(&self, mount: NativeMount, serial: u64, handle: Option<u64>) -> OverlayError {
        let window = self
            .db
            .read_native_visit(mount, serial, handle, 0, 1)
            .expect_err("a refused visit read a window");
        let refusal = self.attributes(mount, serial, handle);
        assert_eq!(
            std::mem::discriminant(&window),
            std::mem::discriminant(&refusal),
            "window {window:?}, attributes {refusal:?}"
        );
        refusal
    }
    /// The refusal of an OPEN visit of `serial` under the kernel's lookup
    /// reference: its fence refuses before any decision.
    fn opening(&self, mount: NativeMount, serial: u64) -> OverlayError {
        let refused = self.db.open_native_visit(
            mount,
            901,
            serial,
            true,
            |_, _| -> Result<NativeDecision<()>, OverlayError> {
                panic!("a refused OPEN reached its decision")
            },
        );
        assert!(refused.decision.is_none() && refused.open_candidate.is_none());
        refused.result.unwrap_err()
    }
    fn attributes(&self, mount: NativeMount, serial: u64, handle: Option<u64>) -> OverlayError {
        self.db
            .observe_native_visit(
                mount,
                serial,
                handle,
                false,
                |_, _| -> Result<NativeDecision<()>, OverlayError> {
                    panic!("a refused visit reached its decision")
                },
            )
            .result
            .unwrap_err()
    }
    fn maintain(&self) {
        let mut cursor = MaintenanceCursor::default();
        for _ in 0..200 {
            let Some(step) = self.db.maintain(cursor).unwrap() else {
                return;
            };
            cursor = step.cursor;
        }
        panic!("maintenance did not settle");
    }
}

#[test]
fn every_visit_statement_is_an_indexed_seek() {
    let w = world();
    let plans = w.db.explain_native_visit(w.mount).unwrap();
    println!("NATIVE_VISIT_PLANS {plans:#?}");
    for label in [
        "fence-lookup",
        "fence-file",
        "fence-handle",
        "fence-directory",
        "drop-open",
        "drop-lookup",
        "drop-reader",
        "open-and-lookup",
    ] {
        assert!(
            plans.iter().any(|plan| plan.starts_with(label)),
            "{label} is not explained: {plans:?}"
        );
    }
    let (program, queries) = plans.split_last().unwrap();
    for plan in queries {
        assert!(
            !plan.contains("SCAN") && !plan.contains("TEMP B-TREE"),
            "{plan}"
        );
    }
    assert!(program.ends_with("btree-scan-opcodes=0"), "{program}");
    // Every table a fence touches is reached by a key: the Workspace row,
    // the mount row and the reference's own rows.
    let seeks = |label: &str| {
        queries
            .iter()
            .filter(|plan| plan.starts_with(label) && plan.contains("SEARCH"))
            .count()
    };
    assert_eq!(
        [
            seeks("fence-lookup"),
            seeks("fence-file"),
            seeks("fence-handle"),
            seeks("drop-open"),
            seeks("drop-lookup"),
            seeks("drop-reader")
        ],
        [3, 4, 5, 1, 1, 1],
        "{plans:#?}"
    );
    // The merged name seek of a visit's evaluation and publication.
    let source = w.db.acquire_base_source(w.route, 1).unwrap();
    let compound = w.db.explain_compound(source).unwrap();
    let name = compound
        .iter()
        .find(|plan| plan.starts_with("name-layers"))
        .expect("the name seek is explained");
    assert!(
        name.contains("SEARCH") && !name.contains("SCAN") && !name.contains("TEMP B-TREE"),
        "{name}"
    );
}

#[test]
fn a_descriptor_of_another_mount_or_inode_and_a_missing_reference_are_stale() {
    let w = world();
    let file = w.open(w.mount, 1, 50, true);
    let other = w.db.open_workspace([33; 32], [34; 32]).unwrap();
    let elsewhere = w.db.create_native_mount(other, 1).unwrap();
    let theirs = w.open(elsewhere, 2, 50, true);
    assert_ne!(file.owner_id(), theirs.owner_id());

    let before = (w.snapshot(w.route), w.snapshot(other));
    // This mount's descriptor through the other mount, and the reverse.
    for (mount, handle) in [(elsewhere, file.owner_id()), (w.mount, theirs.owner_id())] {
        assert!(matches!(
            w.grow(mount, 50, Some(handle), None),
            Err(OverlayError::Stale)
        ));
        assert!(matches!(
            w.read(mount, 50, Some(handle)),
            OverlayError::Stale
        ));
        assert!(matches!(
            w.db.close_native_file(mount, 50, handle),
            Err(OverlayError::Stale)
        ));
    }
    // The descriptor on another inode, and a handle that was never issued.
    for (serial, handle) in [(1, file.owner_id()), (50, u64::MAX >> 1)] {
        assert!(matches!(
            w.grow(w.mount, serial, Some(handle), None),
            Err(OverlayError::Stale)
        ));
        assert!(matches!(
            w.db.close_native_file(w.mount, serial, handle),
            Err(OverlayError::Stale)
        ));
    }
    // An inode the kernel holds no lookup count on.
    assert!(matches!(
        w.grow(w.mount, 77, None, None),
        Err(OverlayError::Stale)
    ));
    assert!(matches!(w.read(w.mount, 77, None), OverlayError::Stale));
    assert_eq!((w.snapshot(w.route), w.snapshot(other)), before);

    // The kernel forgets the created inode while its descriptor stays open:
    // a visit by that inode is stale, a visit by the descriptor is not.
    w.db.forget_native(w.mount, 50, 1).unwrap();
    assert_eq!(w.db.native_lookup_count(w.mount, 50).unwrap(), None);
    assert!(matches!(
        w.grow(w.mount, 50, None, None),
        Err(OverlayError::Stale)
    ));
    assert!(matches!(w.read(w.mount, 50, None), OverlayError::Stale));
    let publication = w
        .grow(w.mount, 50, Some(file.owner_id()), None)
        .unwrap()
        .expect("the descriptor still writes");
    w.db.reply_attempted(publication).unwrap();

    // One close; the second finds no descriptor and changes nothing.
    w.db.close_native_file(w.mount, 50, file.owner_id())
        .unwrap();
    let closed = w.snapshot(w.route);
    assert!(matches!(
        w.db.close_native_file(w.mount, 50, file.owner_id()),
        Err(OverlayError::Stale)
    ));
    // A publication that names a descriptor its fence did not read is
    // checked against the stored row: this one is gone.
    w.db.observe_native_visit(w.mount, 1, None, true, |_, _| {
        Ok(NativeDecision::Finished {
            inode: Some(inode(50)),
            value: (),
        })
    })
    .result
    .unwrap();
    let held = w.snapshot(w.route);
    assert!(matches!(
        w.grow(w.mount, 50, None, Some(file)),
        Err(OverlayError::Stale)
    ));
    assert_eq!(w.snapshot(w.route), held);
    assert_ne!(held.1, closed.1);
}

#[test]
fn a_read_only_descriptor_cannot_publish_and_begins_no_transaction() {
    let w = world();
    let reading = w.open(w.mount, 1, 50, false);
    let writing = w.open(w.mount, 2, 51, true);
    assert!(!reading.writable() && writing.writable());

    let before = w.snapshot(w.route);
    assert!(matches!(
        w.grow(w.mount, 50, Some(reading.owner_id()), None),
        Err(OverlayError::Invalid("read-only descriptor"))
    ));
    assert_eq!(w.snapshot(w.route), before);
    assert_eq!(w.db.inode(w.route, 50).unwrap().unwrap().size, 0);
    // The same descriptor named by a visit whose fence read another one is
    // refused by the stored row, as before.
    assert!(matches!(
        w.grow(w.mount, 51, Some(writing.owner_id()), Some(reading)),
        Err(OverlayError::Invalid("read-only descriptor"))
    ));
    assert_eq!(w.snapshot(w.route), before);
    // A descriptor that was released is not a descriptor at all.
    let released = w.open(w.mount, 3, 52, true);
    w.db.close_native_file(w.mount, 52, released.owner_id())
        .unwrap();
    let before = w.snapshot(w.route);
    assert!(matches!(
        w.grow(w.mount, 51, Some(writing.owner_id()), Some(released)),
        Err(OverlayError::Stale)
    ));
    assert_eq!(w.snapshot(w.route), before);

    // The writable descriptor publishes; the read-only one still closes.
    let publication = w
        .grow(w.mount, 51, Some(writing.owner_id()), None)
        .unwrap()
        .expect("published");
    w.db.reply_attempted(publication).unwrap();
    assert_eq!(w.db.inode(w.route, 51).unwrap().unwrap().size, 1);
    for file in [reading, writing] {
        w.db.close_native_file(w.mount, file.serial(), file.owner_id())
            .unwrap();
    }
}

#[test]
fn a_closed_workspace_refuses_visits_and_still_releases_descriptors() {
    let w = world();
    let file = w.open(w.mount, 1, 50, true);
    w.db.close(w.route).unwrap();
    let before = w.snapshot(w.route);
    assert!(before.0.closed);

    // Closed is reported before the mount or the reference is considered.
    assert!(matches!(
        w.grow(w.mount, 1, None, None),
        Err(OverlayError::Closed)
    ));
    assert!(matches!(
        w.grow(w.mount, 50, Some(file.owner_id()), None),
        Err(OverlayError::Closed)
    ));
    for (serial, handle) in [
        (1, None),
        (50, Some(file.owner_id())),
        (77, None),
        (50, Some(file.owner_id() + 1)),
    ] {
        assert!(matches!(
            w.read(w.mount, serial, handle),
            OverlayError::Closed
        ));
    }
    assert!(matches!(w.opening(w.mount, 50), OverlayError::Closed));
    assert_eq!(w.snapshot(w.route).1, before.1);

    // RELEASE of an open descriptor is still served, exactly once.
    w.db.close_native_file(w.mount, 50, file.owner_id())
        .unwrap();
    assert!(matches!(
        w.db.close_native_file(w.mount, 50, file.owner_id()),
        Err(OverlayError::Stale)
    ));
    // The mount and its lookup counts still hold the closed namespace.
    assert_eq!(w.db.cleanup_state(w.route).unwrap(), CleanupState::Held);
    w.db.revoke_native_mount(w.mount).unwrap();
    w.maintain();
    assert_eq!(w.db.cleanup_state(w.route).unwrap(), CleanupState::Queued);
}

#[test]
fn a_revoked_or_replaced_mount_is_stale_for_every_fenced_job() {
    let w = world();
    let file = w.open(w.mount, 1, 50, true);
    w.db.revoke_native_mount(w.mount).unwrap();
    let before = w.snapshot(w.route);
    assert!(matches!(
        w.grow(w.mount, 1, None, None),
        Err(OverlayError::Stale)
    ));
    assert!(matches!(
        w.grow(w.mount, 50, Some(file.owner_id()), None),
        Err(OverlayError::Stale)
    ));
    assert!(matches!(w.read(w.mount, 1, None), OverlayError::Stale));
    assert!(matches!(
        w.read(w.mount, 50, Some(file.owner_id())),
        OverlayError::Stale
    ));
    // The kernel's RELEASE can no longer arrive; maintenance retires it.
    assert!(matches!(
        w.db.close_native_file(w.mount, 50, file.owner_id()),
        Err(OverlayError::Stale)
    ));
    assert_eq!(w.snapshot(w.route), before);
    assert!(matches!(w.opening(w.mount, 50), OverlayError::Stale));
    assert_eq!(w.snapshot(w.route).1, before.1);
    w.maintain();
    assert!(matches!(
        w.db.check_file(file, false),
        Err(OverlayError::Stale)
    ));

    // A new connection of the same Workspace: the old token names a mount
    // that is gone, and its handle is not the new mount's.
    let again = w.db.create_native_mount(w.route, 1).unwrap();
    assert_ne!(again.owner_id(), w.mount.owner_id());
    let before = w.snapshot(w.route);
    assert!(matches!(
        w.grow(w.mount, 1, None, None),
        Err(OverlayError::Stale)
    ));
    assert!(matches!(w.read(w.mount, 1, None), OverlayError::Stale));
    assert!(matches!(
        w.grow(again, 50, Some(file.owner_id()), None),
        Err(OverlayError::Stale)
    ));
    assert_eq!(w.snapshot(w.route), before);
    let reopened = w.open(again, 2, 60, true);
    w.db.close_native_file(again, 60, reopened.owner_id())
        .unwrap();
}

#[test]
fn a_read_window_writes_nothing_and_follows_its_descriptor_past_unlink_and_install() {
    let w = world();
    let file = w.open(w.mount, 1, 50, true);
    let handle = file.owner_id();
    let publish = |request: u64, serial: u64, by: Option<u64>, changes: Changes| {
        let applied =
            w.db.mutate_native_visit(w.mount, request, serial, by, |_, open| {
                Ok(Some((Changes { open, ..changes }, NativeEffect::None)))
            })
            .unwrap()
            .expect("published");
        w.db.reply_attempted(applied.publication).unwrap();
    };
    let written = |text: &'static [u8], offset: u64, size: u64, nlink: u64| Changes {
        inodes: vec![Inode {
            size,
            nlink,
            ..inode(50)
        }],
        write: Some(PayloadWrite {
            serial: 50,
            offset,
            data: text.into(),
        }),
        ..Changes::default()
    };
    publish(2, 50, Some(handle), written(b"local bytes", 0, 11, 1));
    // A kernel lookup count on an inode with no local row.
    w.db.observe_native_visit(w.mount, 1, None, true, |_, _| {
        Ok(NativeDecision::Finished {
            inode: Some(inode(60)),
            value: (),
        })
    })
    .result
    .unwrap();

    let base = w.db.state(w.route).unwrap().base_root;
    let before = w.snapshot(w.route);
    let window = |serial: u64, by: Option<u64>, offset: u64, length: u32| {
        w.db.read_native_visit(w.mount, serial, by, offset, length)
    };
    let (root, local) = window(50, Some(handle), 0, 4096).unwrap();
    let local = local.expect("the file has a local row");
    assert_eq!(root, base);
    assert_eq!(
        (local.kind, local.size, local.offset, local.data.as_slice()),
        (InodeKind::File, 11, 0, b"local bytes".as_slice())
    );
    assert_eq!((local.span, local.base_root), (None, None));
    // A window is clamped to the local row's size; one at the end is empty.
    let (_, tail) = window(50, Some(handle), 6, 4096).unwrap();
    assert_eq!(tail.unwrap().data, b"bytes");
    let (_, end) = window(50, Some(handle), 11, 4096).unwrap();
    assert!(end.unwrap().data.is_empty());
    // Under the lookup reference alone the same window is read: READLINK
    // names no descriptor.
    assert_eq!(
        window(50, None, 0, 4096).unwrap().1.unwrap().data,
        b"local bytes"
    );
    // No local row: the whole inode is the base's, at the current root.
    assert_eq!(window(60, None, 0, 4096).unwrap(), (base, None));
    // A directory answers its kind and no bytes.
    let (_, directory) = window(1, None, 0, 4096).unwrap();
    assert_eq!(directory.unwrap().kind, InodeKind::Directory);
    assert!(matches!(
        window(50, Some(handle), 0, READ_WINDOW as u32 + 1),
        Err(OverlayError::Invalid("read window"))
    ));
    // Nothing was written, and no transaction was begun.
    assert_eq!(w.snapshot(w.route), before);

    // Unlinked while open: the descriptor still reads the file, and names
    // the root the orphan retained. Without a descriptor the file is gone.
    publish(3, 50, Some(handle), written(b"L", 0, 11, 0));
    let before = w.snapshot(w.route);
    let (root, local) = window(50, Some(handle), 0, 4096).unwrap();
    let local = local.unwrap();
    assert_eq!((root, local.base_root), (base, Some(base)));
    assert_eq!(local.data, b"Local bytes");
    assert!(matches!(
        window(50, None, 0, 4096),
        Err(OverlayError::Missing)
    ));
    // Another inode now pays the orphan probe and is read as before.
    assert_eq!(window(60, None, 0, 4096).unwrap(), (base, None));
    assert_eq!(w.snapshot(w.route), before);

    // An install replaces the Workspace's base. The orphan keeps its root;
    // an inode with no local row now names the installed one.
    let capture = w.db.capture(w.route).unwrap();
    w.db.install(capture, [35; 32]).unwrap();
    assert_eq!(w.db.state(w.route).unwrap().base_root, [35; 32]);
    let (root, local) = window(50, Some(handle), 0, 4096).unwrap();
    assert_eq!(
        (root, local.unwrap().data.as_slice()),
        (base, b"Local bytes".as_slice())
    );
    assert_eq!(window(60, None, 0, 4096).unwrap(), ([35; 32], None));
    // It is still written and read through its descriptor.
    publish(4, 50, Some(handle), written(b"!", 10, 11, 0));
    assert_eq!(
        window(50, Some(handle), 0, 4096).unwrap().1.unwrap().data,
        b"Local byte!"
    );
    w.db.close_native_file(w.mount, 50, handle).unwrap();
    assert!(matches!(
        window(50, Some(handle), 0, 4096),
        Err(OverlayError::Stale)
    ));
}

#[test]
fn an_open_visit_writes_its_descriptor_alone_and_nothing_unless_it_decides_a_file() {
    let w = world();
    let first = w.open(w.mount, 1, 50, true);
    let open = |request: u64, serial: u64, writable: bool, decided: Option<Option<Inode>>| {
        w.db.open_native_visit(w.mount, request, serial, writable, |_, _| {
            Ok(match decided {
                None => NativeDecision::Needs("needs"),
                Some(inode) => NativeDecision::Finished {
                    inode,
                    value: "finished",
                },
            })
        })
    };
    let rows = || (w.snapshot(w.route).0, w.snapshot(w.route).1);
    let before = rows();

    // Undecided, and decided without an inode (a refusal): nothing written.
    let undecided = open(10, 50, false, None);
    assert!(matches!(undecided.result, Ok(None)));
    assert_eq!(
        (undecided.decision, undecided.open_candidate),
        (Some("needs"), None)
    );
    let refused = open(10, 50, false, Some(None));
    assert!(matches!(refused.result, Ok(None)));
    assert_eq!(
        (refused.decision, refused.open_candidate),
        (Some("finished"), None)
    );
    assert_eq!(rows(), before);
    // An inode the kernel holds no lookup count on: stale before a decision.
    assert!(matches!(w.opening(w.mount, 77), OverlayError::Stale));
    // A decision that is not this regular file fails the whole job.
    let directory = Inode {
        kind: InodeKind::Directory,
        mode: 0o755,
        ..inode(50)
    };
    let wrong = open(10, 50, false, Some(Some(directory)));
    assert!(matches!(wrong.result, Err(OverlayError::Missing)));
    let other = open(10, 50, false, Some(Some(inode(51))));
    assert!(matches!(
        other.result,
        Err(OverlayError::Invalid("native observation serial"))
    ));
    assert_eq!((wrong.open_candidate, other.open_candidate), (None, None));
    assert_eq!(rows(), before);
    assert_eq!(w.db.retained_native_file(w.mount, 10).unwrap(), None);

    // Decided: one read-only descriptor of the file, recorded for the
    // request that receives it, beside the first one. No source, no read.
    let opened = open(10, 50, false, Some(Some(inode(50))));
    assert!(matches!(opened.result, Ok(None)));
    assert!(opened.candidate.is_none() && opened.directory_candidate.is_none());
    let second = opened.open_candidate.expect("the OPEN's descriptor");
    assert_eq!((second.serial(), second.writable()), (50, false));
    assert_ne!(second.owner_id(), first.owner_id());
    assert_eq!(
        w.db.retained_native_file(w.mount, 10).unwrap(),
        Some(second)
    );
    let after = rows();
    assert_eq!(after.0, before.0, "the Workspace row is unchanged");
    assert_eq!(
        (
            after.1.owner_rows - before.1.owner_rows,
            after.1.source_rows,
            after.1.inode_rows,
            after.1.payload_cells
        ),
        (
            1,
            before.1.source_rows,
            before.1.inode_rows,
            before.1.payload_cells
        )
    );
    // The descriptor reads, and being read-only it cannot publish.
    assert!(w
        .db
        .read_native_visit(w.mount, 50, Some(second.owner_id()), 0, 1)
        .is_ok());
    assert!(matches!(
        w.grow(w.mount, 50, Some(second.owner_id()), None),
        Err(OverlayError::Invalid("read-only descriptor"))
    ));
    // A second OPEN recorded for the same kernel request fails whole.
    let again = open(10, 50, true, Some(Some(inode(50))));
    assert!(again.result.is_err(), "{:?}", again.result);
    assert_eq!(rows(), after);
    assert_eq!(
        w.db.retained_native_file(w.mount, 10).unwrap(),
        Some(second)
    );

    // A file whose last name is gone is still opened under the kernel's
    // reference, and no longer once the kernel has forgotten the inode.
    let unlinked = Inode {
        nlink: 0,
        ..inode(50)
    };
    let third = open(11, 50, true, Some(Some(unlinked)))
        .open_candidate
        .expect("opened under the kernel's reference");
    assert!(third.writable());
    w.db.forget_native(w.mount, 50, 1).unwrap();
    let held = rows();
    assert!(matches!(w.opening(w.mount, 50), OverlayError::Stale));
    assert_eq!(rows(), held);
    // Each descriptor is released once, by RELEASE alone.
    for file in [first, second, third] {
        w.db.close_native_file(w.mount, 50, file.owner_id())
            .unwrap();
        assert!(matches!(
            w.db.close_native_file(w.mount, 50, file.owner_id()),
            Err(OverlayError::Stale)
        ));
    }
    assert_eq!(w.db.retained_native_file(w.mount, 10).unwrap(), None);
}

#[test]
fn an_opendir_visit_writes_its_descriptor_alone_and_releasedir_closes_exactly_that_directory() {
    let w = world();
    let file = w.open(w.mount, 1, 50, true);
    let root = Inode {
        kind: InodeKind::Directory,
        mode: 0o755,
        ..inode(1)
    };
    let open = |request: u64, serial: u64, decided: Option<Option<Inode>>| {
        w.db.opendir_native_visit(w.mount, request, serial, |_, _| {
            Ok(match decided {
                None => NativeDecision::Needs("needs"),
                Some(inode) => NativeDecision::Finished {
                    inode,
                    value: "finished",
                },
            })
        })
    };
    let rows = || (w.snapshot(w.route).0, w.snapshot(w.route).1);
    let before = rows();

    // Undecided, and decided without an inode (a refusal): nothing written.
    let undecided = open(20, 1, None);
    assert!(matches!(undecided.result, Ok(None)));
    assert_eq!(
        (undecided.decision, undecided.directory_candidate),
        (Some("needs"), None)
    );
    let refused = open(20, 1, Some(None));
    assert!(matches!(refused.result, Ok(None)));
    assert_eq!(
        (refused.decision, refused.directory_candidate),
        (Some("finished"), None)
    );
    // An inode the kernel holds no lookup count on: stale before a decision.
    let stale =
        w.db.opendir_native_visit(w.mount, 20, 77, |_, _| -> Result<NativeDecision<()>, _> {
            panic!("a refused OPENDIR reached its decision")
        });
    assert!(matches!(stale.result, Err(OverlayError::Stale)));
    // A decision that is not this directory, or a removed one, fails whole.
    let wrong = open(20, 50, Some(Some(inode(50))));
    assert!(matches!(wrong.result, Err(OverlayError::Missing)));
    let other = open(
        20,
        1,
        Some(Some(Inode {
            serial: 2,
            ..root.clone()
        })),
    );
    assert!(matches!(
        other.result,
        Err(OverlayError::Invalid("native observation serial"))
    ));
    assert_eq!(
        (wrong.directory_candidate, other.directory_candidate),
        (None, None)
    );
    assert_eq!(rows(), before);
    assert_eq!(w.db.retained_native_directory(w.mount, 20).unwrap(), None);

    // Decided: one open descriptor of the directory, recorded for the request
    // that receives it. No source and no read; the Workspace row is unchanged.
    let opened = open(20, 1, Some(Some(root.clone())));
    assert!(matches!(opened.result, Ok(None)));
    assert!(opened.candidate.is_none() && opened.open_candidate.is_none());
    let directory = opened
        .directory_candidate
        .expect("the OPENDIR's descriptor");
    assert_eq!(directory.serial(), 1);
    assert_eq!(
        w.db.retained_native_directory(w.mount, 20).unwrap(),
        Some(directory)
    );
    let after = rows();
    assert_eq!(after.0, before.0, "the Workspace row is unchanged");
    assert_eq!(
        (after.1.source_rows, after.1.inode_rows),
        (before.1.source_rows, before.1.inode_rows)
    );
    assert_eq!(w.db.state(w.route).unwrap().base_readers, 0);
    // A second OPENDIR recorded for the same kernel request fails whole.
    let again = open(20, 1, Some(Some(root.clone())));
    assert!(again.result.is_err(), "{:?}", again.result);
    assert_eq!(rows(), after);
    // The descriptor serves a handle-addressed visit of its directory.
    let seen = w.db.observe_native_visit(
        w.mount,
        1,
        Some(directory.owner_id()),
        false,
        |_, _| -> Result<NativeDecision<()>, OverlayError> {
            Ok(NativeDecision::Finished {
                inode: None,
                value: (),
            })
        },
    );
    assert!(matches!(seen.result, Ok(None)));

    // RELEASEDIR closes exactly an open directory descriptor of that inode:
    // a file's descriptor, another inode and an unknown handle are stale and
    // change nothing.
    for (serial, handle) in [
        (50, file.owner_id()),
        (50, directory.owner_id()),
        (1, file.owner_id()),
        (1, directory.owner_id() + 1000),
    ] {
        assert!(matches!(
            w.db.close_native_directory(w.mount, serial, handle),
            Err(OverlayError::Stale)
        ));
    }
    assert_eq!(rows(), after);
    // A closed Workspace refuses OPENDIR and still serves RELEASEDIR, once.
    let second = open(21, 1, Some(Some(root.clone())))
        .directory_candidate
        .expect("a second descriptor of the same directory");
    assert_ne!(second.owner_id(), directory.owner_id());
    w.db.close(w.route).unwrap();
    let closed = open(22, 1, Some(Some(root)));
    assert!(matches!(closed.result, Err(OverlayError::Closed)));
    w.db.close_native_directory(w.mount, 1, directory.owner_id())
        .unwrap();
    assert!(matches!(
        w.db.close_native_directory(w.mount, 1, directory.owner_id()),
        Err(OverlayError::Stale)
    ));
    // The other descriptor is untouched by that release, and a revoked
    // mount's RELEASEDIR is stale: revocation retires what is left.
    assert_eq!(
        w.db.retained_native_directory(w.mount, 21)
            .unwrap()
            .map(|d| d.owner_id()),
        Some(second.owner_id())
    );
    w.db.close_native_file(w.mount, 50, file.owner_id())
        .unwrap();
    w.db.revoke_native_mount(w.mount).unwrap();
    assert!(matches!(
        w.db.close_native_directory(w.mount, 1, second.owner_id()),
        Err(OverlayError::Stale)
    ));
    w.maintain();
    assert_eq!(w.db.cleanup_state(w.route).unwrap(), CleanupState::Queued);
}

#[test]
fn a_created_inode_takes_its_custody_without_a_read_and_a_duplicate_fails_whole() {
    let w = world();
    let lease = |w: &World| w.db.diagnostics().statements[StatementKind::Lease as usize];

    // CREATE with an open descriptor: the lookup row and its owner row, the
    // descriptor row and its owner row, one custody row for both references
    // and the mount's association. No row is read first.
    let before = lease(&w);
    let file = w.open(w.mount, 1, 50, true);
    let after = lease(&w);
    assert_eq!(
        (
            after.attempts - before.attempts,
            after.rows_returned - before.rows_returned
        ),
        (6, 0)
    );
    assert_eq!(w.db.native_lookup_count(w.mount, 50).unwrap(), Some(1));

    // Closing returns what remains in the same statement: the kernel's
    // lookup count still holds the file, so nothing is queued for it.
    let before = lease(&w);
    w.db.close_native_file(w.mount, 50, file.owner_id())
        .unwrap();
    let after = lease(&w);
    assert_eq!(
        (
            after.attempts - before.attempts,
            after.rows_returned - before.rows_returned
        ),
        (3, 1)
    );
    assert!(w.db.maintenance_idle(w.route).unwrap());
    // The last reference of a file with no orphan deletes its custody row
    // itself and queues nothing.
    w.db.forget_native(w.mount, 50, 1).unwrap();
    assert!(w.db.maintenance_idle(w.route).unwrap());
    w.maintain();

    // An entry without a descriptor.
    let applied =
        w.db.mutate_native_visit(w.mount, 2, 1, None, |rows, _| {
            Ok(Some((
                created(rows.active().number(), 51, b"entry"),
                NativeEffect::Entry {
                    serial: 51,
                    parent: 1,
                    directory: false,
                },
            )))
        })
        .unwrap()
        .expect("published");
    w.db.reply_attempted(applied.publication).unwrap();
    assert_eq!(applied.file, None);
    assert_eq!(w.db.native_lookup_count(w.mount, 51).unwrap(), Some(1));

    // An entry for an inode the job did not create (LINK) reads the count
    // the kernel already holds and adds to it.
    let applied =
        w.db.mutate_native_visit(w.mount, 3, 1, None, |_, _| {
            Ok(Some((
                Changes {
                    inodes: vec![Inode {
                        nlink: 2,
                        ..inode(51)
                    }],
                    ..Changes::default()
                },
                NativeEffect::Entry {
                    serial: 51,
                    parent: 1,
                    directory: false,
                },
            )))
        })
        .unwrap()
        .expect("published");
    w.db.reply_attempted(applied.publication).unwrap();
    assert_eq!(w.db.native_lookup_count(w.mount, 51).unwrap(), Some(2));

    // A job that claims to create an inode the kernel already references
    // fails at that insert and is rolled back whole.
    let before = w.snapshot(w.route);
    let rollbacks = w.db.diagnostics().statements[StatementKind::Rollback as usize].executions;
    let collided = w.db.mutate_native_visit(w.mount, 4, 1, None, |rows, _| {
        Ok(Some((
            created(rows.active().number(), 51, b"again"),
            NativeEffect::Open {
                serial: 51,
                parent: 1,
                writable: true,
            },
        )))
    });
    assert!(
        matches!(collided, Err(OverlayError::Sql(_))),
        "{collided:?}"
    );
    assert_eq!(
        w.db.diagnostics().statements[StatementKind::Rollback as usize].executions,
        rollbacks + 1
    );
    let after = w.snapshot(w.route);
    assert_eq!((after.0, after.1), (before.0, before.1));
    assert_eq!(w.db.native_lookup_count(w.mount, 51).unwrap(), Some(2));
    assert_eq!(w.db.inode(w.route, 51).unwrap().unwrap().nlink, 2);
    assert_eq!(w.db.retained_native_file(w.mount, 4).unwrap(), None);
    assert!(w.db.pending_publications(w.route, 0).unwrap().is_empty());

    // The created serial must be one of the job's own live finals.
    let before = w.snapshot(w.route);
    assert!(matches!(
        w.db.mutate_native_visit(w.mount, 5, 1, None, |rows, _| {
            Ok(Some((
                Changes {
                    created: Some(70),
                    ..created(rows.active().number(), 71, b"other")
                },
                NativeEffect::None,
            )))
        }),
        Err(OverlayError::Invalid("created inode final"))
    ));
    assert_eq!(w.snapshot(w.route), before);
}
