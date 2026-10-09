//! The one-statement fence of a native visit and the custody writes behind
//! it: everything the separate Workspace, mount, reference and descriptor
//! reads used to refuse is still refused, with the same outcome, and nothing
//! is written by a refused visit.
use layerfs_overlay::{
    Binding, Changes, CleanupState, DirectoryEntryChange, Inode, InodeKind, MaintenanceCursor,
    NativeDecision, NativeEffect, NativeMount, OpenFile, Overlay, OverlayError, ProfileConfig,
    Publication, Route, StatementKind, StoredCounts, WorkspaceState,
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
    fn read(&self, mount: NativeMount, serial: u64, handle: Option<u64>) -> OverlayError {
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
    assert_eq!(w.snapshot(w.route), before);

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
    // The last reference queues the inode's reclamation, as before.
    w.db.forget_native(w.mount, 50, 1).unwrap();
    assert!(!w.db.maintenance_idle(w.route).unwrap());
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
