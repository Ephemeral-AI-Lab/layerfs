//! The daemon aggregate of `Overlay::resources` against a reference counted
//! from the stored rows themselves, at one, three and eight namespaces, live,
//! closed, partly reclaimed and reclaimed. The rows are read through a second
//! plain connection after the engine has closed its database.
mod payload_support;
use layerfs_overlay::*;
use payload_support::*;

/// What one namespace still owns.
struct Held<'a> {
    route: Route,
    source: BaseSource,
    file: File<'a>,
    open: OpenFile,
    read: FileRead,
    capture: Capture,
    reader: CapturedReader,
}
fn maintain(db: &Overlay) {
    let mut cursor = MaintenanceCursor::default();
    for _ in 0..20_000 {
        let Some(step) = db.maintain(cursor).unwrap() else {
            return;
        };
        cursor = step.cursor;
    }
    panic!("maintenance deadline");
}
/// Rows of every accounted kind in one namespace: wide, one-cell and masked
/// payload rows, shrink steps, names, an orphan held by a descriptor, a
/// reader and a retained capture, and queued maintenance.
fn populate(db: &Overlay, index: u8) -> Held<'_> {
    let route = db.open_workspace([index; 32], [index + 100; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let mut file = File::new(db, source, 7, pattern(20_000, index));
    file.write(0, &pattern(WRITE_WINDOW, index));
    file.write(140_000, &pattern(usize::from(index) * 700 + 1, 3));
    file.resize(100_000);
    file.resize(150_000);
    let mut small = File::new(db, source, 8, Vec::new());
    for at in 0..u64::from(index) {
        small.write(at * 8_192, b"x");
    }
    let publication = db
        .apply(
            source,
            &Changes {
                inodes: vec![Inode {
                    serial: 1,
                    kind: InodeKind::Directory,
                    mode: 0o755,
                    entries: 1,
                    ..small.inode(0)
                }],
                directory_entries: vec![DirectoryEntryChange {
                    parent: 1,
                    name: format!("name-{index}").into_bytes(),
                    binding: Binding::Bound {
                        serial: 8,
                        inherited: false,
                    },
                }],
                ..Changes::default()
            },
        )
        .unwrap();
    db.reply_attempted(publication).unwrap();
    let size = file.expect.len() as u64;
    let open = db.open_file(source, 1, &file.inode(size), true).unwrap();
    db.release_base_source(source).unwrap();
    let capture = db.capture(route).unwrap();
    let reader = db.acquire_captured_reader(capture, 1).unwrap();
    let source = db.acquire_base_source(route, 2).unwrap();
    file.source = source;
    let mut removed = file.inode(size);
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
    let read = db.acquire_file_read(source, open, 3).unwrap();
    Held {
        route,
        source,
        file,
        open,
        read,
        capture,
        reader,
    }
}
/// Releases every owner of one namespace and closes it.
fn retire(db: &Overlay, held: Held<'_>) {
    db.release_file_read(held.read).unwrap();
    db.close_file(held.open).unwrap();
    db.release_captured_reader(held.reader).unwrap();
    db.release_base_source(held.source).unwrap();
    db.resolve_failed_capture(held.capture).unwrap();
    db.close(held.route).unwrap();
    drop(held.file);
}
fn counted(raw: &rusqlite::Connection, sql: &str) -> u64 {
    raw.query_row(sql, [], |row| row.get::<_, i64>(0)).unwrap() as u64
}
/// The counts of the stored rows of every namespace.
fn reference(raw: &rusqlite::Connection) -> StoredCounts {
    let rows = |table: &str| counted(raw, &format!("SELECT count(*) FROM {table}"));
    let records = [
        "operation_record",
        "owned_operation_record",
        "indexed_operation_record",
    ];
    let details = [
        "file_handle",
        "file_read",
        "captured_reader",
        "operation_owner",
        "lookup_owner",
        "file_custody",
        "native_mount",
        "native_lookup",
        "native_parent",
        "native_directory",
        "native_cookie",
    ];
    StoredCounts {
        wait_refs: rows("orphan_wait"),
        namespaces: rows("workspace"),
        inode_rows: rows("inode"),
        directory_entry_rows: rows("directory_entry"),
        payload_cells: rows("payload"),
        payload_bytes: counted(
            raw,
            "SELECT ifnull(sum(length(data)+ifnull(length(validity),0)),0) FROM payload",
        ),
        shrink_rows: rows("shrink"),
        operation_record_rows: records.iter().map(|table| rows(table)).sum(),
        operation_record_bytes: records
            .iter()
            .map(|table| {
                counted(
                    raw,
                    &format!("SELECT ifnull(sum(length(value)),0) FROM {table}"),
                )
            })
            .sum(),
        orphan_rows: rows("orphan"),
        owner_rows: rows("lease"),
        source_rows: rows("base_source"),
        owner_details: details.iter().map(|table| rows(table)).sum(),
        reply_tickets: 0,
        retire_targets: rows("reclaim"),
        maintenance_targets: rows("maintenance"),
        ready_targets: counted(raw, "SELECT ifnull(sum(ready),0) FROM maintenance"),
    }
}

#[test]
fn the_aggregate_equals_the_stored_rows_at_one_three_and_eight_namespaces() {
    let mut seen = Vec::new();
    for namespaces in [1_u8, 3, 8] {
        // 0: every namespace live. 1: the odd ones closed, nothing reclaimed.
        // 2: closed and partly reclaimed. 3: all closed and reclaimed.
        for stage in 0..4 {
            let temp = Temp::new();
            let db = temp.db();
            let mut held: Vec<Held<'_>> = (1..=namespaces).map(|i| populate(&db, i)).collect();
            if stage == 0 {
                maintain(&db);
            }
            if stage >= 1 {
                let mut kept = Vec::new();
                for (index, namespace) in held.drain(..).enumerate() {
                    if stage == 3 || index % 2 == 0 {
                        retire(&db, namespace);
                    } else {
                        kept.push(namespace);
                    }
                }
                held = kept;
            }
            if stage == 2 {
                for _ in 0..7 {
                    db.reclaim_closed(0).unwrap();
                    db.maintain(MaintenanceCursor::default()).unwrap();
                }
            }
            if stage == 3 {
                maintain(&db);
                for turn in 0..20_000 {
                    if db.reclaim_closed(0).unwrap().is_none() {
                        break;
                    }
                    assert!(turn < 19_999);
                }
            }
            let total = db.resources(None).unwrap().counts;
            // The aggregate is also the sum of the live namespaces' own rows
            // when no closed namespace is left.
            if stage == 0 {
                let mut sum = StoredCounts::default();
                for namespace in &held {
                    let own = db.resources(Some(namespace.route)).unwrap().counts;
                    sum.namespaces += own.namespaces;
                    sum.inode_rows += own.inode_rows;
                    sum.payload_cells += own.payload_cells;
                    sum.payload_bytes += own.payload_bytes;
                    sum.orphan_rows += own.orphan_rows;
                    sum.owner_details += own.owner_details;
                }
                assert_eq!(
                    (
                        sum.namespaces,
                        sum.inode_rows,
                        sum.payload_cells,
                        sum.payload_bytes,
                        sum.orphan_rows,
                        sum.owner_details
                    ),
                    (
                        total.namespaces,
                        total.inode_rows,
                        total.payload_cells,
                        total.payload_bytes,
                        total.orphan_rows,
                        total.owner_details
                    )
                );
                assert_eq!(total.namespaces, u64::from(namespaces));
                assert_eq!(total.orphan_rows, u64::from(namespaces));
            }
            if stage == 3 {
                assert_eq!(total, StoredCounts::default());
            }
            drop(held);
            drop(db);
            let raw = rusqlite::Connection::open(temp.0.join("overlay.sqlite")).unwrap();
            assert_eq!(
                total,
                reference(&raw),
                "{namespaces} namespaces, stage {stage}"
            );
            // Namespace zero keeps the engine's orphan count and nothing else.
            let zero: (i64, i64, i64) = raw
                .query_row(
                    "SELECT orphan_rows,inode_rows+payload_cells+owner_details+namespaces,
                    (SELECT count(*) FROM accounting) FROM accounting WHERE ns=0",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .unwrap();
            assert_eq!((zero.0 as u64, zero.1), (total.orphan_rows, 0));
            assert_eq!(zero.2 as u64, 1 + total.namespaces);
            seen.push((namespaces, stage, total));
        }
    }
    println!("R7_ACCOUNTING_REFERENCE {seen:?}");
}
