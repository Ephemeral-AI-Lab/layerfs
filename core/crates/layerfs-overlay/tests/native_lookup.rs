//! Public engine native ownership, atomicity and bounded indexed retirement.
use layerfs_overlay::{
    Binding, Changes, CleanupState, DatabaseWork, DirectoryEntryChange, Inode, InodeKind,
    MaintenanceCursor, NativeDecision, NativeEffect, NativeMount, NativeMountState, Overlay,
    OverlayError, ProfileConfig, StatementKind, StoredCounts,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
struct Fixture {
    db: Overlay,
    mount: NativeMount,
    _temp: Temp,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let path = std::env::temp_dir().join(format!(
            "layerfs-native-count-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        let db = Overlay::create(&path.join("overlay"), ProfileConfig::default()).unwrap();
        let route = db.open_workspace([21; 32], [22; 32]).unwrap();
        let mount = db.create_native_mount(route, 1).unwrap();
        Self {
            db,
            mount,
            _temp: Temp(path),
        }
    }
    /// One positive LOOKUP under the root: its kernel reference on `serial`.
    fn lookup(&self, serial: u64) {
        let outcome = self
            .db
            .observe_native_visit(self.mount, 1, None, true, |_, _| {
                Ok(NativeDecision::Finished {
                    inode: Some(inode(serial)),
                    value: serial,
                })
            });
        assert_eq!(outcome.decision, Some(serial));
        assert_eq!(outcome.result.unwrap(), None);
    }
}
fn inode(serial: u64) -> Inode {
    Inode {
        serial,
        kind: InodeKind::File,
        mode: 0o644,
        mtime_seconds: -2,
        mtime_nanoseconds: 3,
        nlink: 1,
        size: 0,
        inherited_cutoff: 0,
        born: 0,
        entries: 0,
        subdirs: 0,
    }
}

#[test]
fn native_file_keys_retain_exact_open_owners() {
    let f = Fixture::new();
    f.lookup(2);
    let mut files = Vec::new();
    for (request, writable) in [(u64::MAX, false), (u64::MAX - 1, true)] {
        let outcome =
            f.db.open_native_visit(f.mount, request, 2, writable, |_, _| {
                Ok(NativeDecision::Finished {
                    inode: Some(inode(2)),
                    value: 2,
                })
            });
        assert_eq!(outcome.result.unwrap(), None);
        let file = outcome.open_candidate.unwrap();
        assert_eq!(
            f.db.retained_native_file(f.mount, request).unwrap(),
            Some(file)
        );
        assert_eq!(f.db.native_file(f.mount, 2, file.owner_id()).unwrap(), file);
        assert_eq!(file.writable(), writable);
        assert_eq!(f.db.check_file(file, true).is_ok(), writable);
        assert!(f.db.native_file(f.mount, 3, file.owner_id()).is_err());
        files.push(file);
    }
    assert_ne!(files[0].owner_id(), files[1].owner_id());
    f.db.forget_native(f.mount, 2, 1).unwrap();
    // Without the kernel's lookup reference a visit of the inode is stale.
    assert!(matches!(
        f.db.observe_native_visit(
            f.mount,
            2,
            None,
            false,
            |_, _| -> Result<NativeDecision<()>, OverlayError> {
                panic!("unreferenced inode reached semantics")
            }
        )
        .result,
        Err(OverlayError::Stale)
    ));
    let other = Fixture::new();
    assert!(f
        .db
        .native_file(other.mount, 2, files[0].owner_id())
        .is_err());
    f.db.close_native_file(f.mount, 2, files[0].owner_id())
        .unwrap();
    assert!(f
        .db
        .close_native_file(f.mount, 2, files[0].owner_id())
        .is_err());
    assert_eq!(f.db.retained_native_file(f.mount, u64::MAX).unwrap(), None);
    // The existing exact file close removes the request's descriptor too.
    f.db.close_file(files[1]).unwrap();
    assert_eq!(
        f.db.retained_native_file(f.mount, u64::MAX - 1).unwrap(),
        None
    );
    assert!(f.db.native_file(f.mount, 2, files[1].owner_id()).is_err());
    f.db.revoke_native_mount(f.mount).unwrap();
}

#[test]
fn native_open_refuses_nonregular_and_opens_a_removed_file_under_lookup_custody() {
    let f = Fixture::new();
    f.lookup(2);
    // A directory answer acquires nothing.
    let before = f.db.resources(Some(f.mount.route())).unwrap().counts;
    let outcome = f.db.open_native_visit(f.mount, 2, 2, true, |_, _| {
        Ok(NativeDecision::Finished {
            inode: Some(Inode {
                kind: InodeKind::Directory,
                nlink: 1,
                ..inode(2)
            }),
            value: 2_u64,
        })
    });
    assert!(matches!(outcome.result, Err(OverlayError::Missing)));
    assert_eq!(outcome.decision, Some(2));
    assert!(outcome.open_candidate.is_none());
    assert_eq!(
        f.db.resources(Some(f.mount.route())).unwrap().counts,
        before
    );
    // A file whose last name is gone is still referenced by the kernel
    // lookup that protects this request: it opens, with exact open custody.
    let outcome = f.db.open_native_visit(f.mount, 3, 2, true, |_, _| {
        Ok(NativeDecision::Finished {
            inode: Some(Inode {
                kind: InodeKind::File,
                nlink: 0,
                ..inode(2)
            }),
            value: 3_u64,
        })
    });
    assert_eq!(outcome.result.unwrap(), None);
    let file = outcome.open_candidate.expect("removed file opened");
    assert_eq!(f.db.retained_native_file(f.mount, 3).unwrap(), Some(file));
    f.db.close_native_file(f.mount, 2, file.owner_id()).unwrap();
    assert_eq!(f.db.retained_native_file(f.mount, 3).unwrap(), None);
    f.db.forget_native(f.mount, 2, 1).unwrap();
    f.db.revoke_native_mount(f.mount).unwrap();
}

#[test]
fn aggregate_forget_is_checked_and_implicit_root_is_independent() {
    let f = Fixture::new();
    assert_eq!(f.db.native_lookup_count(f.mount, 1).unwrap(), Some(0));
    assert!(matches!(
        f.db.forget_native(f.mount, 1, 1),
        Err(OverlayError::Invalid("native lookup underflow"))
    ));
    f.lookup(2);
    let rows = f.db.resources(Some(f.mount.route())).unwrap().counts;
    for _ in 2..=32 {
        f.lookup(2);
    }
    assert_eq!(f.db.native_lookup_count(f.mount, 2).unwrap(), Some(32));
    assert_eq!(
        f.db.resources(Some(f.mount.route())).unwrap().counts,
        rows,
        "repeated references add no ownership rows"
    );
    assert!(matches!(
        f.db.forget_native(f.mount, 2, 33),
        Err(OverlayError::Invalid("native lookup underflow"))
    ));
    assert_eq!(f.db.native_lookup_count(f.mount, 2).unwrap(), Some(32));
    f.db.forget_native(f.mount, 2, 31).unwrap();
    assert_eq!(f.db.native_lookup_count(f.mount, 2).unwrap(), Some(1));
    f.db.forget_native(f.mount, 2, 1).unwrap();
    assert_eq!(f.db.native_lookup_count(f.mount, 2).unwrap(), None);
    assert_eq!(f.db.native_lookup_count(f.mount, 1).unwrap(), Some(0));
    assert!(matches!(
        f.db.forget_native(f.mount, 2, 1),
        Err(OverlayError::Stale)
    ));
}

#[test]
fn revocation_is_once_and_a_revoked_mount_admits_no_visit() {
    let f = Fixture::new();
    f.lookup(2);
    f.db.forget_native(f.mount, 2, 1).unwrap();
    f.db.revoke_native_mount(f.mount).unwrap();
    assert_eq!(
        f.db.native_mount_state(f.mount).unwrap(),
        NativeMountState::Revoked
    );
    assert!(matches!(
        f.db.observe_native_visit(
            f.mount,
            1,
            None,
            true,
            |_, _| -> Result<NativeDecision<()>, OverlayError> {
                panic!("revoked mount reached semantics")
            }
        )
        .result,
        Err(OverlayError::Stale)
    ));
    assert!(matches!(
        f.db.revoke_native_mount(f.mount),
        Err(OverlayError::Stale)
    ));
}

#[test]
fn need_rounds_do_not_acquire_and_failed_atomic_decisions_keep_original_evidence() {
    let f = Fixture::new();
    let before = f.db.resources(Some(f.mount.route())).unwrap().counts;
    let need = f.db.observe_native_visit(f.mount, 1, None, true, |_, _| {
        Ok(NativeDecision::Needs("base fact"))
    });
    assert_eq!(need.decision, Some("base fact"));
    assert!(matches!(need.result, Ok(None)));
    assert_eq!(
        f.db.resources(Some(f.mount.route())).unwrap().counts,
        before
    );
    let original = Arc::new("original semantic value");
    let retained = original.clone();
    let start = f.db.diagnostics();
    let failed = f.db.observe_native_visit(f.mount, 1, None, true, |_, _| {
        Ok(NativeDecision::Finished {
            inode: Some(inode(0)),
            value: original,
        })
    });
    assert!(failed.result.is_err());
    assert!(Arc::ptr_eq(failed.decision.as_ref().unwrap(), &retained));
    assert_eq!(
        f.db.resources(Some(f.mount.route())).unwrap().counts,
        before
    );
    // The failed decision committed nothing: whatever the job began, it
    // rolled back.
    let work = f.db.diagnostics().since(&start);
    let ran = |kind: StatementKind| work.statements[kind as usize].executions;
    assert_eq!(ran(StatementKind::Commit), 0);
    assert_eq!(ran(StatementKind::Rollback), ran(StatementKind::Begin));
}

#[test]
fn foreign_engine_or_mount_cannot_consume_ownership() {
    let f = Fixture::new();
    let other = Fixture::new();
    assert!(matches!(
        other.db.forget_native(f.mount, 1, 1),
        Err(OverlayError::Stale)
    ));
    assert!(matches!(
        f.db.observe_native_visit(
            other.mount,
            1,
            None,
            false,
            |_, _| -> Result<NativeDecision<()>, OverlayError> {
                panic!("foreign mount reached semantics")
            }
        )
        .result,
        Err(OverlayError::Stale)
    ));
}

#[test]
fn revoked_lookup_retirement_is_bounded_indexed_and_allows_closed_cleanup() {
    let f = Fixture::new();
    for serial in 2..=131 {
        f.lookup(serial);
    }
    let plans = f.db.explain_native(f.mount).unwrap();
    assert!(
        plans.iter().all(|plan| plan.contains("SEARCH")),
        "{plans:?}"
    );
    assert!(plans
        .iter()
        .any(|plan| plan.contains("native_lookup") && plan.contains("serial>?")));
    println!("NATIVE_PLANS {plans:?}");
    f.db.close(f.mount.route()).unwrap();
    assert_eq!(
        f.db.cleanup_state(f.mount.route()).unwrap(),
        CleanupState::Held
    );
    f.db.revoke_native_mount(f.mount).unwrap();
    let start = f.db.diagnostics();
    let mut cursor = MaintenanceCursor::default();
    let mut maximum_rows = 0;
    for _ in 0..1000 {
        match f.db.maintain(cursor).unwrap() {
            Some(step) => {
                cursor = step.cursor;
                maximum_rows = maximum_rows.max(step.work.rows);
            }
            None => break,
        }
    }
    assert!(maximum_rows <= 64);
    assert_eq!(
        f.db.native_mount_state(f.mount).unwrap(),
        NativeMountState::Gone
    );
    assert_eq!(
        f.db.cleanup_state(f.mount.route()).unwrap(),
        CleanupState::Queued
    );
    for _ in 0..1000 {
        if f.db.reclaim_closed(0).unwrap().is_none() {
            break;
        }
    }
    assert_eq!(
        f.db.cleanup_state(f.mount.route()).unwrap(),
        CleanupState::Gone
    );
    assert_eq!(f.db.resources(None).unwrap().counts, Default::default());
    println!(
        "NATIVE_RETIRE lookups=131 maximum_window={maximum_rows} work={:?}",
        f.db.diagnostics().since(&start)
    );
}

/// What one owner visit may leave behind: transaction statements, ownership
/// rows and the Workspace's reader count.
struct Before {
    work: DatabaseWork,
    counts: StoredCounts,
    revision: i64,
}
impl Fixture {
    fn before(&self) -> Before {
        let route = self.mount.route();
        Before {
            counts: self.db.resources(Some(route)).unwrap().counts,
            revision: self.db.state(route).unwrap().revision,
            work: self.db.diagnostics(),
        }
    }
    /// (BEGIN, COMMIT, ROLLBACK) statements executed since `before`.
    fn transactions(&self, before: &Before) -> (u64, u64, u64) {
        let work = self.db.diagnostics().since(&before.work);
        let ran = |kind: StatementKind| work.statements[kind as usize].executions;
        (
            ran(StatementKind::Begin),
            ran(StatementKind::Commit),
            ran(StatementKind::Rollback),
        )
    }
    /// A visit that wrote nothing: no transaction, no row, no reader count.
    fn unchanged(&self, before: &Before) {
        assert_eq!(self.transactions(before), (0, 0, 0));
        let route = self.mount.route();
        assert_eq!(
            self.db.resources(Some(route)).unwrap().counts,
            before.counts
        );
        let state = self.db.state(route).unwrap();
        assert_eq!((state.revision, state.base_readers), (before.revision, 0));
    }
    /// The file `serial` created under the root by one publishing visit at
    /// the active generation `active`.
    fn created(&self, active: i64, serial: u64, name: &[u8]) -> Changes {
        let active = active as u64;
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
                    born: active,
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
}

#[test]
fn a_read_visit_records_no_source_and_writes_only_a_positive_lookup_reference() {
    let f = Fixture::new();
    let route = f.mount.route();
    assert_eq!(f.db.resources(Some(route)).unwrap().counts.source_rows, 0);

    // A negative LOOKUP: decided, with nothing to acquire.
    let before = f.before();
    let missing = f.db.observe_native_visit(f.mount, 1, None, true, |_, _| {
        Ok(NativeDecision::Finished {
            inode: None,
            value: "missing",
        })
    });
    assert_eq!(missing.decision, Some("missing"));
    assert!(matches!(missing.result, Ok(None)));
    assert!(missing.open_candidate.is_none());
    assert!(missing.directory_candidate.is_none());
    f.unchanged(&before);

    // GETATTR of the inode the kernel holds.
    let before = f.before();
    let attributes = f.db.observe_native_visit(f.mount, 1, None, false, |_, _| {
        Ok(NativeDecision::Finished {
            inode: Some(Inode {
                kind: InodeKind::Directory,
                ..inode(1)
            }),
            value: "attributes",
        })
    });
    assert_eq!(attributes.decision, Some("attributes"));
    assert!(matches!(attributes.result, Ok(None)));
    f.unchanged(&before);
    assert_eq!(f.db.native_lookup_count(f.mount, 1).unwrap(), Some(0));

    // An undecided visit: the needed fact is its whole outcome.
    let before = f.before();
    let undecided = f.db.observe_native_visit(f.mount, 1, None, true, |_, _| {
        Ok(NativeDecision::Needs("base fact"))
    });
    assert_eq!(undecided.decision, Some("base fact"));
    assert!(matches!(undecided.result, Ok(None)));
    f.unchanged(&before);

    // A positive LOOKUP takes exactly one kernel reference, in the one
    // transaction of the visit that answered, and still records no source.
    for held in 1..=2 {
        let before = f.before();
        let found = f.db.observe_native_visit(f.mount, 1, None, true, |_, _| {
            Ok(NativeDecision::Finished {
                inode: Some(inode(9)),
                value: 9_u64,
            })
        });
        assert_eq!(found.decision, Some(9));
        assert!(matches!(found.result, Ok(None)));
        assert_eq!(f.transactions(&before), (1, 1, 0));
        assert_eq!(f.db.native_lookup_count(f.mount, 9).unwrap(), Some(held));
        let counts = f.db.resources(Some(route)).unwrap().counts;
        assert_eq!(counts.source_rows, before.counts.source_rows);
        assert_eq!(counts.reply_tickets, 0);
        // Only the first reference adds its ownership rows.
        assert_eq!(counts == before.counts, held == 2);
        assert_eq!(f.db.state(route).unwrap().base_readers, 0);
    }

    // A decision the visit refuses leaves its original value and no write:
    // attributes of another inode, and a lookup answer that has no name left.
    let before = f.before();
    let other = f.db.observe_native_visit(f.mount, 1, None, false, |_, _| {
        Ok(NativeDecision::Finished {
            inode: Some(inode(9)),
            value: "other",
        })
    });
    assert_eq!(other.decision, Some("other"));
    assert!(matches!(
        other.result,
        Err(OverlayError::Invalid("native observation serial"))
    ));
    let removed = f.db.observe_native_visit(f.mount, 1, None, true, |_, _| {
        Ok(NativeDecision::Finished {
            inode: Some(Inode {
                nlink: 0,
                ..inode(9)
            }),
            value: "removed",
        })
    });
    assert!(matches!(removed.result, Err(OverlayError::Missing)));
    f.unchanged(&before);
    assert_eq!(f.db.native_lookup_count(f.mount, 9).unwrap(), Some(2));

    // Nothing was recorded for a request, so nothing fences revocation.
    f.db.forget_native(f.mount, 9, 2).unwrap();
    f.db.revoke_native_mount(f.mount).unwrap();
}

#[test]
fn a_mutation_visit_publishes_with_its_kernel_custody_and_records_no_source() {
    let f = Fixture::new();
    let route = f.mount.route();
    const REQUEST: u64 = u64::MAX - 7;

    // A callback that decides without changes writes nothing.
    let before = f.before();
    let declined =
        f.db.mutate_native_visit(f.mount, REQUEST, 1, None, |_, file| {
            assert_eq!(file, None);
            Ok(None)
        })
        .unwrap();
    assert_eq!(declined, None);
    f.unchanged(&before);
    assert_eq!(f.db.retained_native_file(f.mount, REQUEST).unwrap(), None);

    // CREATE with an open descriptor: publication, lookup reference,
    // descriptor and reply ticket in the visit's one transaction.
    let before = f.before();
    let applied =
        f.db.mutate_native_visit(f.mount, REQUEST, 1, None, |rows, file| {
            assert_eq!(file, None);
            Ok(Some((
                f.created(rows.active().number(), 50, b"made"),
                NativeEffect::Open {
                    serial: 50,
                    parent: 1,
                    writable: true,
                },
            )))
        })
        .unwrap()
        .expect("published");
    assert_eq!(f.transactions(&before), (1, 1, 0));
    let file = applied.file.expect("the created file is open");
    assert_eq!((file.serial(), file.writable()), (50, true));
    // native_lookup and the file_handle row of the mount's request.
    assert_eq!(f.db.native_lookup_count(f.mount, 50).unwrap(), Some(1));
    f.db.check_file(file, true).unwrap();
    assert_eq!(
        f.db.native_file(f.mount, 50, file.owner_id()).unwrap(),
        file
    );
    assert_eq!(
        f.db.retained_native_file(f.mount, REQUEST).unwrap(),
        Some(file)
    );
    // The reply ticket.
    assert_eq!(
        f.db.pending_publications(route, 0).unwrap(),
        vec![applied.publication]
    );
    let state = f.db.state(route).unwrap();
    assert_eq!(
        (state.revision, state.base_readers),
        (before.revision + 1, 0)
    );
    // No request source: no base_source row.
    let counts = f.db.resources(Some(route)).unwrap().counts;
    assert_eq!(counts.source_rows, before.counts.source_rows);
    assert_eq!(counts.reply_tickets, before.counts.reply_tickets + 1);
    assert_eq!(f.db.inode(route, 50).unwrap().unwrap().nlink, 1);

    // A handle-addressed visit is given its exact descriptor.
    let before = f.before();
    let seen =
        f.db.mutate_native_visit(
            f.mount,
            REQUEST - 1,
            50,
            Some(file.owner_id()),
            |_, open| {
                assert_eq!(open, Some(file));
                Ok(None)
            },
        )
        .unwrap();
    assert_eq!(seen, None);
    // Another handle, or this handle on another inode, is not that
    // descriptor: the callback is never reached.
    for (serial, handle) in [(50, file.owner_id() + 1), (1, file.owner_id())] {
        assert!(matches!(
            f.db.mutate_native_visit(f.mount, REQUEST - 2, serial, Some(handle), |_, _| {
                panic!("a wrong handle reached the decision")
            }),
            Err(OverlayError::Stale)
        ));
    }
    // Without the kernel's reference on the named inode there is no visit.
    assert!(matches!(
        f.db.mutate_native_visit(f.mount, REQUEST - 3, 77, None, |_, _| {
            panic!("an unreferenced inode reached the decision")
        }),
        Err(OverlayError::Stale)
    ));
    let unreferenced = f.db.observe_native_visit(
        f.mount,
        77,
        None,
        false,
        |_, _| -> Result<NativeDecision<()>, OverlayError> {
            panic!("an unreferenced inode reached the decision")
        },
    );
    assert!(matches!(unreferenced.result, Err(OverlayError::Stale)));
    assert_eq!(unreferenced.decision, None);
    // A read visit through a handle needs that handle on that inode.
    let through =
        f.db.observe_native_visit(f.mount, 50, Some(file.owner_id()), false, |_, _| {
            Ok(NativeDecision::Finished {
                inode: Some(inode(50)),
                value: (),
            })
        });
    assert!(matches!(through.result, Ok(None)));
    let wrong = f.db.observe_native_visit(
        f.mount,
        1,
        Some(file.owner_id()),
        false,
        |_, _| -> Result<NativeDecision<()>, OverlayError> {
            panic!("a wrong handle reached the decision")
        },
    );
    assert!(matches!(wrong.result, Err(OverlayError::Stale)));
    f.unchanged(&before);

    // A publication that fails after its first write is rolled back whole:
    // the effect names an inode the changes do not create.
    let before = f.before();
    assert!(matches!(
        f.db.mutate_native_visit(f.mount, REQUEST - 4, 1, None, |rows, _| {
            Ok(Some((
                f.created(rows.active().number(), 51, b"unmade"),
                NativeEffect::Entry {
                    serial: 52,
                    parent: 1,
                    directory: false,
                },
            )))
        }),
        Err(OverlayError::Invalid("native entry final"))
    ));
    assert_eq!(f.transactions(&before), (1, 0, 1));
    assert_eq!(f.db.resources(Some(route)).unwrap().counts, before.counts);
    assert_eq!(f.db.state(route).unwrap().revision, before.revision);
    assert_eq!(f.db.inode(route, 51).unwrap(), None);
    assert_eq!(f.db.native_lookup_count(f.mount, 51).unwrap(), None);
    // The ticket the failed visit issued in memory at its frontier advance
    // is withdrawn with it: only the earlier publication is still owed.
    assert_eq!(
        f.db.pending_publications(route, 0).unwrap(),
        vec![applied.publication]
    );

    // The reply attempt is the only release a publishing visit owes.
    f.db.reply_attempted(applied.publication).unwrap();
    assert!(f.db.pending_publications(route, 0).unwrap().is_empty());
    // A revoked mount is visited by nothing.
    f.db.revoke_native_mount(f.mount).unwrap();
    assert!(matches!(
        f.db.mutate_native_visit(f.mount, REQUEST - 5, 1, None, |_, _| {
            panic!("a revoked mount reached the decision")
        }),
        Err(OverlayError::Stale)
    ));
    let revoked = f.db.observe_native_visit(
        f.mount,
        1,
        None,
        true,
        |_, _| -> Result<NativeDecision<()>, OverlayError> {
            panic!("a revoked mount reached the decision")
        },
    );
    assert!(matches!(revoked.result, Err(OverlayError::Stale)));
}

#[test]
fn a_visit_source_names_no_row_and_is_stale_once_the_base_or_frontier_moves() {
    let f = Fixture::new();
    let route = f.mount.route();
    // The turn-local source of a visit, carried out of its callback.
    let smuggle = || {
        let mut source = None;
        let outcome =
            f.db.observe_native_visit(f.mount, 1, None, false, |rows, own| {
                assert_eq!(rows.source(), own);
                source = Some(own);
                Ok(NativeDecision::Needs(()))
            });
        assert!(matches!(outcome.result, Ok(None)));
        source.expect("the visit reached its decision")
    };
    let before = f.before();
    let first = smuggle();
    assert_eq!(first.route(), route);
    assert_eq!(first.root(), f.db.state(route).unwrap().base_root);
    // It is not a recorded source: it cannot be released, and nothing changed.
    assert!(matches!(
        f.db.release_base_source(first),
        Err(OverlayError::Invalid(_))
    ));
    assert_eq!(
        f.db.retained_base_source(route, first.owner()).unwrap(),
        None
    );
    f.unchanged(&before);
    // Each visit has its own, and neither fences the install below.
    let second = smuggle();
    assert_ne!(first.owner(), second.owner());
    // While the Workspace still has that base and frontier it reads as the
    // current base.
    assert_eq!(f.db.source_inode(first, 1).unwrap(), None);

    // The install frontier moves under the same base root.
    let capture = f.db.capture(route).unwrap();
    assert!(f.db.install_ready(capture).unwrap());
    f.db.install(capture, first.root()).unwrap();
    assert_eq!(f.db.state(route).unwrap().base_root, first.root());
    for stale in [first, second] {
        assert!(matches!(
            f.db.source_inode(stale, 1),
            Err(OverlayError::Stale)
        ));
        assert!(matches!(f.db.source_rows(stale), Err(OverlayError::Stale)));
        assert!(matches!(
            f.db.apply(
                stale,
                &f.created(f.db.state(route).unwrap().active.number(), 60, b"stale")
            ),
            Err(OverlayError::Stale)
        ));
    }
    assert_eq!(f.db.inode(route, 60).unwrap(), None);

    // The base root moves.
    let third = smuggle();
    assert_eq!(f.db.source_inode(third, 1).unwrap(), None);
    let capture = f.db.capture(route).unwrap();
    f.db.install(capture, [23; 32]).unwrap();
    assert!(matches!(
        f.db.source_inode(third, 1),
        Err(OverlayError::Stale)
    ));
    assert!(matches!(
        f.db.release_base_source(third),
        Err(OverlayError::Invalid(_))
    ));
    // A visit after the install is over the new base.
    let current = smuggle();
    assert_eq!(current.root(), [23; 32]);
    assert_eq!(f.db.source_inode(current, 1).unwrap(), None);
    assert_eq!(f.db.state(route).unwrap().base_readers, 0);
    f.db.revoke_native_mount(f.mount).unwrap();
}
