mod payload_support;
use layerfs_overlay::*;
use payload_support::*;
use std::sync::Arc;
fn maintain(db: &Overlay) {
    let mut cursor = MaintenanceCursor::default();
    for _ in 0..2000 {
        let Some(step) = db.maintain(cursor).unwrap() else {
            return;
        };
        assert!(step.work.rows <= 64 && step.work.data_bytes <= 65536);
        cursor = step.cursor;
    }
    panic!("maintenance deadline");
}
fn bytes(db: &Overlay, read: FileRead, base: &[u8]) -> Vec<u8> {
    let plan = db.read_file(read, 0, READ_WINDOW as u32).unwrap().unwrap();
    let mut data = plan.data;
    for (slot, byte) in data.iter_mut().enumerate() {
        if plan
            .inherited
            .get(slot / 8)
            .is_some_and(|b| b & (1 << (slot % 8)) != 0)
        {
            *byte = base.get(slot).copied().unwrap_or(0)
        }
    }
    data
}
#[test]
fn independent_orphan_survives_many_installs_and_failures_and_last_read_owner() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([111; 32], [112; 32]).unwrap();
    let mut source = db.acquire_base_source(route, 1).unwrap();
    let mut file = File::new(&db, source, i64::MAX as u64, pattern(8199, 3));
    file.write(4000, b"before unlink");
    let open = db
        .open_file(source, 1, &file.inode(file.expect.len() as u64), true)
        .unwrap();
    assert_eq!(db.retained_file(route, 1).unwrap(), Some(open));
    assert!(db.open_file(source, 1, &file.inode(8199), true).is_err());
    let mut removed = file.inode(file.expect.len() as u64);
    removed.nlink = 0;
    let p = db
        .apply(
            source,
            &Changes {
                inodes: vec![removed],
                ..Changes::default()
            },
        )
        .unwrap();
    db.reply_attempted(p).unwrap();
    let mut expect = file.expect.clone();
    for round in 0..20 {
        let read = db.acquire_file_read(source, open, 100 + round).unwrap();
        assert_eq!(bytes(&db, read, &file.base), expect);
        assert_eq!(
            db.retained_file_read(route, 100 + round).unwrap(),
            Some(read)
        );
        db.release_file_read(read).unwrap();
        assert!(db.read_file(read, 0, 1).is_err());
        let mut inode = db.source_inode(source, i64::MAX as u64).unwrap().unwrap();
        let tail = format!("round{round:02}").into_bytes();
        let offset = inode.size;
        inode.size += tail.len() as u64;
        let p = db
            .apply(
                source,
                &Changes {
                    open: Some(open),
                    inodes: vec![inode],
                    write: Some(PayloadWrite {
                        serial: i64::MAX as u64,
                        offset,
                        data: Arc::from(tail.clone()),
                    }),
                    ..Changes::default()
                },
            )
            .unwrap();
        db.reply_attempted(p).unwrap();
        expect.extend(tail);
        db.release_base_source(source).unwrap();
        let capture = db.capture(route).unwrap();
        if round % 2 == 1 {
            db.install(capture, [round as u8; 32]).unwrap();
        } else {
            db.resolve_failed_capture(capture).unwrap();
        }
        maintain(&db);
        assert!(db.capture_ready(route).unwrap());
        source = db.acquire_base_source(route, round + 2).unwrap();
    }
    let read = db.acquire_file_read(source, open, 999).unwrap();
    db.close_file(open).unwrap();
    assert!(db.check_file(open, false).is_err());
    maintain(&db);
    assert_eq!(bytes(&db, read, &file.base), expect);
    db.close(route).unwrap();
    db.release_base_source(source).unwrap();
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Held);
    assert_eq!(bytes(&db, read, &file.base), expect);
    db.release_file_read(read).unwrap();
    maintain(&db);
    for turn in 0..1000 {
        let step = db.reclaim_closed(0).unwrap().unwrap();
        if step.done {
            assert!(db.state(route).is_err());
            return;
        }
        assert!(turn < 999);
    }
}
#[test]
fn captured_reader_preserves_sealed_bytes_across_install_and_blocks_failed_fold() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([113; 32], [114; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let mut f = File::new(&db, source, 9, Vec::new());
    f.write(0, b"sealed");
    db.release_base_source(source).unwrap();
    let capture = db.capture(route).unwrap();
    let reader = db.acquire_captured_reader(capture, 1).unwrap();
    assert_eq!(db.retained_captured_reader(route, 1).unwrap(), Some(reader));
    let sealed_rows = db.reader_inodes(reader, 0).unwrap();
    let sealed_names = db.reader_directory_entries(reader, None).unwrap();
    db.install(capture, [115; 32]).unwrap();
    maintain(&db);
    assert_eq!(db.reader_inodes(reader, 0).unwrap(), sealed_rows);
    assert_eq!(
        db.reader_directory_entries(reader, None).unwrap(),
        sealed_names
    );
    assert_eq!(
        db.read_captured(reader, 9, 0, 32).unwrap().unwrap().data,
        b"sealed"
    );
    db.release_captured_reader(reader).unwrap();
    assert!(db.read_captured(reader, 9, 0, 32).is_err());
    maintain(&db);
    let source = db.acquire_base_source(route, 2).unwrap();
    f.source = source;
    f.write(0, b"second");
    db.release_base_source(source).unwrap();
    let capture = db.capture(route).unwrap();
    let reader = db.acquire_captured_reader(capture, 2).unwrap();
    db.resolve_failed_capture(capture).unwrap();
    maintain(&db);
    assert!(!db.capture_ready(route).unwrap());
    assert_eq!(
        db.read_captured(reader, 9, 0, 32).unwrap().unwrap().data,
        b"second"
    );
    db.release_captured_reader(reader).unwrap();
    maintain(&db);
    assert!(db.capture_ready(route).unwrap());
}

#[test]
fn operation_custody_never_reuses_an_exposed_owner_and_cleanup_cannot_delete_new_operation_record()
{
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([121; 32], [122; 32]).unwrap();
    let first = db.acquire_operation(route, 1).unwrap();
    assert_eq!(db.retained_operation(route, 1).unwrap(), Some(first));
    let record = OperationRecord {
        kind: 4,
        key: 1,
        value: vec![3; OPERATION_RECORD_BYTES],
    };
    for key in 0..8 {
        db.put_owned_operation_record(
            first,
            &OperationRecord {
                key,
                ..record.clone()
            },
        )
        .unwrap();
    }
    assert!(db.acquire_operation(route, 1).is_err());
    db.release_operation(first).unwrap();
    let second = db.acquire_operation(route, 1).unwrap();
    assert_ne!(first, second);
    assert!(db.release_operation(first).is_err());
    assert!(db.put_owned_operation_record(first, &record).is_err());
    db.put_owned_operation_record(second, &record).unwrap();
    maintain(&db);
    assert_eq!(
        db.owned_operation_record_page(second, 4, None).unwrap(),
        vec![record.clone()]
    );
    db.close(route).unwrap();
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Held);
    assert_eq!(
        db.owned_operation_record_page(second, 4, None).unwrap(),
        vec![record.clone()]
    );
    assert!(db.put_owned_operation_record(second, &record).is_err());
    db.release_operation(second).unwrap();
    maintain(&db);
    for _ in 0..1000 {
        if db.reclaim_closed(0).unwrap().unwrap().done {
            return;
        }
    }
    panic!("terminal cleanup stalled");
}

#[test]
fn unowned_unlinked_payload_is_reclaimed_live_but_its_namespace_tombstone_remains() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([125; 32], [126; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let mut f = File::new(&db, source, 4, Vec::new());
    f.write(0, &pattern(131072, 3));
    let allocated = db.pages().unwrap();
    let mut inode = f.inode(131072);
    inode.nlink = 0;
    let p = db
        .apply(
            source,
            &Changes {
                inodes: vec![inode],
                ..Changes::default()
            },
        )
        .unwrap();
    db.reply_attempted(p).unwrap();
    maintain(&db);
    assert!(db.source_read(source, 4, 0, 1).is_err());
    assert_eq!(db.inode(route, 4).unwrap().unwrap().nlink, 0);
    assert!(db.pages().unwrap().1 > allocated.1);
    db.release_base_source(source).unwrap();
    let capture = db.capture(route).unwrap();
    db.resolve_failed_capture(capture).unwrap();
    maintain(&db);
    assert_eq!(db.inode(route, 4).unwrap().unwrap().nlink, 0);
    let source = db.acquire_base_source(route, 2).unwrap();
    assert!(db.source_read(source, 4, 0, 1).is_err());
}

#[test]
fn custody_and_orphan_jobs_keep_point_work_beside_unrelated_owners() {
    let mut reference = None;
    for scale in [128, 1024, 4096] {
        // Closed fixture per scale: only the unrelated owner population varies.
        // Prior files would add a range-end index neighbor to the read program.
        let temp = Temp::new();
        let db = temp.db();
        let route = db.open_workspace([131; 32], [132; 32]).unwrap();
        let source = db.acquire_base_source(route, 1).unwrap();
        for owner in 0..scale {
            db.acquire_operation(route, owner + 10000).unwrap();
        }
        let serial = 17;
        let mut f = File::new(&db, source, serial, Vec::new());
        f.write(0, &pattern(8199, 3));
        let mut open = None;
        let a = work(&db, || {
            open = Some(db.open_file(source, scale, &f.inode(8199), true).unwrap());
        });
        let open = open.unwrap();
        let mut inode = f.inode(8199);
        inode.nlink = 0;
        let b = work(&db, || {
            let p = db
                .apply(
                    source,
                    &Changes {
                        inodes: vec![inode.clone()],
                        ..Changes::default()
                    },
                )
                .unwrap();
            db.reply_attempted(p).unwrap();
        });
        let mut read = None;
        let c = work(&db, || {
            read = Some(db.acquire_file_read(source, open, scale).unwrap());
        });
        let read = read.unwrap();
        let d = work(&db, || {
            assert_eq!(bytes(&db, read, &[]), f.expect);
        });
        inode.size += 4;
        let e = work(&db, || {
            let p = db
                .apply(
                    source,
                    &Changes {
                        open: Some(open),
                        inodes: vec![inode],
                        write: Some(PayloadWrite {
                            serial,
                            offset: 8199,
                            data: Arc::from(&b"tail"[..]),
                        }),
                        ..Changes::default()
                    },
                )
                .unwrap();
            db.reply_attempted(p).unwrap();
        });
        let row = [a, b, c, d, e];
        println!("S6_CUSTODY_PROFILE unrelated={scale} open={a:?} unlink={b:?} acquire_read={c:?} read={d:?} descriptor_append={e:?} fullscan=0 sorts=0 autoindex=0 reprepare=0");
        if let Some(expected) = reference {
            assert_eq!(row, expected)
        } else {
            reference = Some(row)
        }
        db.close_file(open).unwrap();
        db.release_file_read(read).unwrap();
        maintain(&db);
    }
}

#[test]
fn logical_close_refuses_new_captured_owners_and_keeps_existing_consumers() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([135; 32], [136; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let mut f = File::new(&db, source, 4, Vec::new());
    f.write(0, b"sealed before close");
    db.release_base_source(source).unwrap();
    let capture = db.capture(route).unwrap();
    let reader = db.acquire_captured_reader(capture, 1).unwrap();
    db.close(route).unwrap();
    assert!(matches!(
        db.acquire_captured_reader(capture, 2),
        Err(OverlayError::Closed)
    ));
    assert_eq!(
        db.read_captured(reader, 4, 0, 64).unwrap().unwrap().data,
        b"sealed before close"
    );
    db.release_closed_capture(capture).unwrap();
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Held);
    db.release_captured_reader(reader).unwrap();
    for _ in 0..1000 {
        if db.reclaim_closed(0).unwrap().unwrap().done {
            return;
        }
    }
    panic!("terminal cleanup stalled");
}

#[test]
fn three_layer_orphan_composition_preserves_cutoffs_and_an_independent_pre_unlink_reader() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([141; 32], [142; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let mut f = File::new(&db, source, 7, pattern(8199, 3));
    f.write(4000, b"sealed crossing");
    let sealed = f.expect.clone();
    db.release_base_source(source).unwrap();
    let capture = db.capture(route).unwrap();
    let reader = db.acquire_captured_reader(capture, 1).unwrap();
    let source = db.acquire_base_source(route, 2).unwrap();
    f.source = source;
    f.resize(5000);
    f.resize(9000);
    f.write(8199, b"new tail");
    let open = db.open_file(source, 1, &f.inode(9000), true).unwrap();
    let mut inode = f.inode(9000);
    inode.nlink = 0;
    let p = db
        .apply(
            source,
            &Changes {
                inodes: vec![inode],
                ..Changes::default()
            },
        )
        .unwrap();
    db.reply_attempted(p).unwrap();
    let read = db.acquire_file_read(source, open, 1).unwrap();
    assert_eq!(bytes(&db, read, &f.base), f.expect);
    db.resolve_failed_capture(capture).unwrap();
    let mut cursor = MaintenanceCursor::default();
    for _ in 0..1000 {
        assert_eq!(bytes(&db, read, &f.base), f.expect);
        let Some(step) = db.maintain(cursor).unwrap() else {
            break;
        };
        cursor = step.cursor;
    }
    assert!(!db.capture_ready(route).unwrap());
    let p = db.read_captured(reader, 7, 0, 131072).unwrap().unwrap();
    let mut got = p.data;
    for (slot, byte) in got.iter_mut().enumerate() {
        if p.inherited
            .get(slot / 8)
            .is_some_and(|b| b & (1 << (slot % 8)) != 0)
        {
            *byte = f.base[slot]
        }
    }
    assert_eq!(got, sealed);
    db.release_captured_reader(reader).unwrap();
    maintain(&db);
    assert!(db.capture_ready(route).unwrap());
    assert_eq!(bytes(&db, read, &f.base), f.expect);
    db.close_file(open).unwrap();
    db.release_file_read(read).unwrap();
    maintain(&db);
    assert_eq!(db.inode(route, 7).unwrap().unwrap().nlink, 0);
}

#[test]
fn a_last_release_under_a_generation_hold_parks_the_layer_for_the_queue() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([151; 32], [152; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let mut f = File::new(&db, source, 7, Vec::new());
    f.write(0, &pattern(8199, 3));
    let sealed = f.expect.clone();
    db.release_base_source(source).unwrap();
    // The capture and its reader hold the generation the file was written in.
    let capture = db.capture(route).unwrap();
    let reader = db.acquire_captured_reader(capture, 1).unwrap();
    let source = db.acquire_base_source(route, 2).unwrap();
    f.source = source;
    let open = db.open_file(source, 1, &f.inode(8199), true).unwrap();
    let mut inode = f.inode(8199);
    inode.nlink = 0;
    let p = db
        .apply(
            source,
            &Changes {
                inodes: vec![inode],
                ..Changes::default()
            },
        )
        .unwrap();
    db.reply_attempted(p).unwrap();
    let counts = || db.resources(None).unwrap().counts;
    let (cells, queued) = (counts().payload_cells, counts().maintenance_targets);
    // Two whole cells in one row and the tail in another.
    assert_eq!((counts().orphan_rows, cells, queued), (1, 2, 1));

    // The last reference leaves with no owner step in between. Its job
    // releases both lower layers and deletes the orphan and its queued
    // item; the held layer keeps its cells and is parked, not ready.
    db.close_file(open).unwrap();
    let after = counts();
    assert_eq!(
        (
            after.orphan_rows,
            after.payload_cells,
            after.maintenance_targets,
            after.ready_targets
        ),
        (0, 2, 1, 0)
    );
    assert_eq!(db.maintain(MaintenanceCursor::default()).unwrap(), None);
    assert!(!db.maintenance_pending());
    let p = db.read_captured(reader, 7, 0, 131072).unwrap().unwrap();
    assert_eq!(p.data, sealed);

    // The generation's release wakes the parked retirement and the queue
    // finishes it.
    db.resolve_failed_capture(capture).unwrap();
    db.release_captured_reader(reader).unwrap();
    maintain(&db);
    let end = counts();
    assert_eq!((end.payload_cells, end.maintenance_targets), (0, 0));
    assert!(db.capture_ready(route).unwrap());
    assert_eq!(db.inode(route, 7).unwrap().unwrap().nlink, 0);
}

#[test]
fn inode_reads_probe_the_orphan_domain_only_while_an_orphan_exists() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([141; 32], [142; 32]).unwrap();
    let other = db.open_workspace([143; 32], [144; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let elsewhere = db.acquire_base_source(other, 1).unwrap();
    // One inode observation and the Inode-family statements it attempted.
    let read = |source: BaseSource, serial: u64| {
        let before = db.diagnostics();
        let inode = db.source_inode(source, serial).unwrap();
        let work = db.diagnostics().since(&before);
        (
            inode,
            work.statements[StatementKind::Inode as usize].attempts,
        )
    };
    let unlink = |file: &File<'_>| {
        let mut removed = file.inode(file.expect.len() as u64);
        removed.nlink = 0;
        let publication = db
            .apply(
                source,
                &Changes {
                    inodes: vec![removed],
                    ..Changes::default()
                },
            )
            .unwrap();
        db.reply_attempted(publication).unwrap();
    };
    let mut kept = File::new(&db, source, 7, Vec::new());
    kept.write(0, b"kept");
    let mut unowned = File::new(&db, source, 8, Vec::new());
    unowned.write(0, b"gone");

    // No orphan has been created by this engine: one seek per inode.
    assert_eq!(read(source, 7).1, 1);
    assert_eq!(read(source, 9), (None, 1));
    assert_eq!(read(elsewhere, 7), (None, 1));
    // An unlink with no owner creates none either: its row is a tombstone.
    unlink(&unowned);
    let (tombstone, attempts) = read(source, 8);
    assert_eq!((tombstone.unwrap().nlink, attempts), (0, 1));
    assert_eq!(read(source, 7).1, 1);

    // The last unlink of an open file creates the first orphan.
    let open = db.open_file(source, 1, &kept.inode(4), true).unwrap();
    unlink(&kept);
    // Its orphan-domain row is found by the probe, which answers alone.
    let (orphan, attempts) = read(source, 7);
    let orphan = orphan.expect("the orphan keeps its metadata");
    assert_eq!((orphan.nlink, orphan.size, attempts), (0, 4, 1));
    // While it exists every inode read of this engine probes first, in
    // every namespace: a miss costs the probe and the seek.
    assert_eq!(read(source, 9), (None, 2));
    assert_eq!(read(source, 8).1, 2);
    assert_eq!(read(elsewhere, 7), (None, 2));

    // A descriptor write lands in the orphan domain and is read back from it.
    let publication = db
        .apply(
            source,
            &Changes {
                open: Some(open),
                inodes: vec![Inode { size: 6, ..orphan }],
                write: Some(PayloadWrite {
                    serial: 7,
                    offset: 4,
                    data: Arc::from(b"+2".as_slice()),
                }),
                ..Changes::default()
            },
        )
        .unwrap();
    db.reply_attempted(publication).unwrap();
    let (orphan, attempts) = read(source, 7);
    assert_eq!((orphan.unwrap().size, attempts), (6, 1));
    let window = db.acquire_file_read(source, open, 50).unwrap();
    assert_eq!(bytes(&db, window, &[]), b"kept+2");
    db.release_file_read(window).unwrap();

    // A second orphan keeps the probe on when the first is reclaimed after
    // its last owner.
    let mut also = File::new(&db, source, 10, Vec::new());
    also.write(0, b"also");
    let held = db.open_file(source, 2, &also.inode(4), true).unwrap();
    unlink(&also);
    db.close_file(open).unwrap();
    maintain(&db);
    assert_eq!(db.resources(None).unwrap().counts.orphan_rows, 1);
    assert_eq!(read(source, 9), (None, 2));
    assert_eq!(read(elsewhere, 7), (None, 2));
    // The engine's last orphan is reclaimed: the probe stops in every
    // namespace, one seek per inode.
    db.close_file(held).unwrap();
    maintain(&db);
    assert_eq!(db.resources(None).unwrap().counts.orphan_rows, 0);
    assert_eq!(read(source, 7).1, 1);
    assert_eq!(read(source, 9), (None, 1));
    assert_eq!(read(elsewhere, 7), (None, 1));
}

#[test]
fn a_closed_namespace_that_takes_the_last_orphan_stops_the_probe() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([145; 32], [146; 32]).unwrap();
    let other = db.open_workspace([147; 32], [148; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let elsewhere = db.acquire_base_source(other, 1).unwrap();
    // Inode-family statements one inode read of the other Workspace attempts.
    let attempts = || {
        let before = db.diagnostics();
        assert_eq!(db.source_inode(elsewhere, 9).unwrap(), None);
        db.diagnostics().since(&before).statements[StatementKind::Inode as usize].attempts
    };
    // More cells than the one page its last owner's release drops itself:
    // rows of eight and six cells are exactly that page, and one more row.
    const SIZE: u64 = 15 * 4096;
    let mut file = File::new(&db, source, 7, Vec::new());
    file.write(0, &pattern(14 * 4096, 3));
    file.write(14 * 4096, &pattern(4096, 4));
    let open = db.open_file(source, 1, &file.inode(SIZE), true).unwrap();
    let mut removed = file.inode(SIZE);
    removed.nlink = 0;
    let publication = db
        .apply(
            source,
            &Changes {
                inodes: vec![removed],
                ..Changes::default()
            },
        )
        .unwrap();
    db.reply_attempted(publication).unwrap();
    assert_eq!(attempts(), 2);

    // The Workspace closes and its last owner leaves with no live
    // maintenance turn in between. The release drops one page and leaves
    // the orphan: terminal reclamation deletes the orphan row, after the
    // namespace's inode rows.
    db.close(route).unwrap();
    db.release_base_source(source).unwrap();
    db.close_file(open).unwrap();
    assert_eq!(db.resources(None).unwrap().counts.orphan_rows, 1);
    for turn in 0..1000 {
        let step = db.reclaim_closed(0).unwrap().unwrap();
        if db.resources(None).unwrap().counts.orphan_rows != 0 {
            assert_eq!(attempts(), 2);
        }
        if step.done {
            break;
        }
        assert!(turn < 999);
    }
    assert_eq!(db.resources(None).unwrap().counts.orphan_rows, 0);
    assert_eq!(attempts(), 1);
}

fn full(result: OverlayResult<()>) -> bool {
    match result {
        Ok(()) => false,
        Err(OverlayError::Sql(rusqlite::Error::SqliteFailure(code, _)))
            if code.code == rusqlite::ErrorCode::DiskFull =>
        {
            true
        }
        Err(error) => panic!("original database failure: {error:?}"),
    }
}
fn plain(serial: u64, size: u64, nlink: u64) -> Inode {
    Inode {
        serial,
        kind: InodeKind::File,
        mode: 0o644,
        mtime_seconds: 1,
        mtime_nanoseconds: 2,
        nlink,
        size,
        inherited_cutoff: 0,
        born: 0,
        entries: 0,
        subdirs: 0,
    }
}
/// One compound job of one inode row and at most one write, its reply attempted.
fn put(
    db: &Overlay,
    source: BaseSource,
    inode: Inode,
    write: Option<(u64, Vec<u8>)>,
) -> OverlayResult<()> {
    let write = write.map(|(offset, data)| PayloadWrite {
        serial: inode.serial,
        offset,
        data: Arc::from(data),
    });
    let publication = db.apply(
        source,
        &Changes {
            inodes: vec![inode],
            write,
            ..Changes::default()
        },
    )?;
    db.reply_attempted(publication)
}

/// The releasing job's only growth is the queue row of a layer it parks, so
/// its failure is staged with the file's real page quota and no hook: the
/// queue's last leaves are filled by the same kind of row until one more
/// needs a page the quota does not have.
#[test]
fn a_release_that_fails_leaves_the_last_orphan_as_it_was_and_the_queue_reclaims_it_later() {
    const ORPHAN: u64 = 1 << 20;
    const SIZE: u64 = 8199;
    let temp = Temp::new();
    let db = Overlay::create(
        &temp.0.join("overlay.sqlite"),
        ProfileConfig {
            max_pages: Some(1024),
            ..ProfileConfig::default()
        },
    )
    .unwrap();
    let route = db.open_workspace([153; 32], [154; 32]).unwrap();
    // A second Workspace whose terminal reclamation frees pages later
    // without adding a row.
    let spare = db.open_workspace([155; 32], [156; 32]).unwrap();
    let held = db.acquire_base_source(spare, 1).unwrap();
    put(
        &db,
        held,
        plain(3, 131072, 1),
        Some((0, pattern(131072, 5))),
    )
    .unwrap();
    db.release_base_source(held).unwrap();

    let source = db.acquire_base_source(route, 1).unwrap();
    let mut file = File::new(&db, source, ORPHAN, Vec::new());
    file.write(0, &pattern(SIZE as usize, 3));
    // Files removed with no owner in the generation the capture seals: their
    // queue rows separate the orphan's own queue row from the last leaves.
    for serial in 1000..2000 {
        put(&db, source, plain(serial, 0, 1), None).unwrap();
        put(&db, source, plain(serial, 0, 0), None).unwrap();
    }
    db.release_base_source(source).unwrap();
    let capture = db.capture(route).unwrap();
    let reader = db.acquire_captured_reader(capture, 1).unwrap();
    let source = db.acquire_base_source(route, 2).unwrap();
    file.source = source;
    let open = db.open_file(source, 1, &file.inode(SIZE), true).unwrap();
    put(&db, source, plain(ORPHAN, SIZE, 0), None).unwrap();
    for serial in 2000..4000 {
        put(&db, source, plain(serial, 0, 1), None).unwrap();
    }

    // The quota is reached by payload, then by rows that need a page only
    // when a leaf splits, then by removals: each adds one queue row after
    // every earlier one and changes no other row's size.
    let cell = |at: u64| (at * 32768, pattern(32768, 7));
    let filled = (0..4096)
        .find(|at| {
            full(put(
                &db,
                source,
                plain(5, (at + 1) * 32768, 1),
                Some(cell(*at)),
            ))
        })
        .expect("the page quota is reached by payload");
    let rows = (0..65536)
        .find(|at| full(put(&db, source, plain(10_000 + at, 0, 1), None)))
        .expect("the page quota is reached by rows");
    let removed = (2000..4000)
        .find(|serial| full(put(&db, source, plain(*serial, 0, 0), None)))
        .expect("the page quota is reached by a queue row");

    // Inode-family statements of one read of a serial that has no row: the
    // seek, and before it the orphan-domain probe while an orphan exists.
    let probes = |db: &Overlay| {
        let before = db.diagnostics();
        assert_eq!(db.source_inode(source, 9).unwrap(), None);
        db.diagnostics().since(&before).statements[StatementKind::Inode as usize].attempts
    };
    let view = |db: &Overlay| {
        let read = db.acquire_file_read(source, open, 77).unwrap();
        let data = bytes(db, read, &[]);
        db.release_file_read(read).unwrap();
        data
    };
    let observed = |db: &Overlay| {
        let all = db.resources(None).unwrap();
        (
            all.counts,
            all.database_pages,
            all.free_pages,
            db.state(route).unwrap(),
        )
    };
    assert_eq!(probes(&db), 2);
    assert!(view(&db) == file.expect);
    let before = observed(&db);
    assert_eq!((before.0.orphan_rows, before.2), (1, 0));
    let pending = db.maintenance_pending();

    // The last reference leaves. Its job deletes the engine's only orphan,
    // then fails on the queue row of the layer the capture holds.
    let sql = db.diagnostics();
    assert!(full(db.close_file(open)));
    let work = db.diagnostics().since(&sql);
    let reclaim = work.statements[StatementKind::Reclaim as usize];
    assert!(reclaim.rows_changed >= 3, "{reclaim:?}");
    assert_eq!(
        work.statements[StatementKind::Rollback as usize].attempts,
        1
    );
    // Every row is as it was: the descriptor, the orphan and its bytes, the
    // queue. The orphan-domain probe is on again: without it the next
    // release would not find the orphan and would leave its rows.
    assert_eq!(observed(&db), before);
    assert_eq!(db.maintenance_pending(), pending);
    db.check_file(open, true).unwrap();
    assert_eq!(db.retained_file(route, 1).unwrap(), Some(open));
    assert_eq!(probes(&db), 2);
    assert!(view(&db) == file.expect);
    let sealed = db
        .read_captured(reader, ORPHAN, 0, 131072)
        .unwrap()
        .unwrap();
    assert_eq!(sealed.data, file.expect);

    // Pages return and the queue runs: the orphan's own item is the one the
    // failed job would have deleted, and it steps the held orphan.
    db.close(spare).unwrap();
    for turn in 0..1000 {
        if db.reclaim_closed(0).unwrap().unwrap().done {
            break;
        }
        assert!(turn < 999);
    }
    let run = |db: &Overlay| {
        let mut cursor = MaintenanceCursor::default();
        for _ in 0..20_000 {
            let Some(step) = db.maintain(cursor).unwrap() else {
                return;
            };
            cursor = step.cursor;
        }
        panic!("maintenance deadline");
    };
    run(&db);
    assert_eq!(db.resources(None).unwrap().counts.orphan_rows, 1);
    assert_eq!(view(&db), file.expect);

    // The same release succeeds, and the capture's release wakes the layer
    // it parked: the queue reclaims the orphan's cells.
    let cells = db.resources(None).unwrap().counts.payload_cells;
    db.close_file(open).unwrap();
    assert!(db.check_file(open, false).is_err());
    let after = db.resources(None).unwrap().counts;
    assert_eq!((after.orphan_rows, after.payload_cells), (0, cells));
    assert_eq!(probes(&db), 1);
    db.resolve_failed_capture(capture).unwrap();
    db.release_captured_reader(reader).unwrap();
    run(&db);
    let end = db.resources(None).unwrap().counts;
    assert_eq!((end.orphan_rows, end.payload_cells), (0, cells - 2));
    assert_eq!((end.maintenance_targets, end.ready_targets), (0, 0));
    assert_eq!(db.inode(route, ORPHAN).unwrap().unwrap().nlink, 0);
    println!(
        "R7_RELEASE_ROLLBACK payload_writes={filled} rows={rows} removals={} failed={reclaim:?}",
        removed - 2000
    );
}
