//! Exact transactions and statements of reclaiming a closed namespace: N and
//! 4N small files, and a payload of N and 4N cells stored as rows of one
//! cell and as rows of several cells. A live namespace with rows of every
//! reclaimed table stands beside it and keeps every row.
mod payload_support;
use layerfs_overlay::*;
use payload_support::*;

/// What reclaiming every ready namespace costs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Cost {
    /// Steps; each is one transaction.
    steps: u64,
    /// Statements of every family, with those that begin and end a step.
    all: u64,
    /// Reclaim statements the engine ran, without trigger programs.
    attempts: u64,
    /// Statements and trigger programs.
    executions: u64,
    /// Rows the statements changed themselves.
    rows: u64,
    /// The most rows and the most payload, name and record bytes of one step.
    widest: (u64, u64),
}
fn reclaim(db: &Overlay) -> Cost {
    let before = db.diagnostics();
    let mut cost = Cost {
        steps: 0,
        all: 0,
        attempts: 0,
        executions: 0,
        rows: 0,
        widest: (0, 0),
    };
    for _ in 0..100_000 {
        let Some(step) = db.reclaim_closed(0).unwrap() else {
            let work = db.diagnostics().since(&before);
            let total = work.total();
            assert_eq!(
                total.fullscan_steps + total.sorts + total.autoindex_rows + total.reprepares,
                0
            );
            let reclaim = work.statements[StatementKind::Reclaim as usize];
            cost.all = total.attempts;
            cost.attempts = reclaim.attempts;
            cost.executions = reclaim.executions;
            cost.rows = reclaim.direct_rows_changed;
            return cost;
        };
        cost.steps += 1;
        cost.widest = (
            cost.widest.0.max(step.rows),
            cost.widest.1.max(step.data_bytes),
        );
    }
    panic!("reclamation deadline");
}
fn directory(entries: u64) -> Inode {
    Inode {
        serial: 1,
        kind: InodeKind::Directory,
        mode: 0o755,
        mtime_seconds: 1,
        mtime_nanoseconds: 2,
        nlink: 2,
        size: 0,
        inherited_cutoff: 0,
        born: 0,
        entries,
        subdirs: 0,
    }
}
/// A namespace of `files` named one-byte files.
fn small_files(db: &Overlay, index: u8, files: u64) -> (Route, BaseSource) {
    let route = db.open_workspace([index; 32], [index + 100; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    for file in 0..files {
        let serial = 10 + file;
        let publication = db
            .apply(
                source,
                &Changes {
                    inodes: vec![
                        directory(file + 1),
                        File::new(db, source, serial, Vec::new()).inode(1),
                    ],
                    directory_entries: vec![DirectoryEntryChange {
                        parent: 1,
                        name: format!("file-{file}").into_bytes(),
                        binding: Binding::Bound {
                            serial,
                            inherited: false,
                        },
                    }],
                    write: Some(PayloadWrite {
                        serial,
                        offset: 0,
                        data: std::sync::Arc::from(&b"x"[..]),
                    }),
                    ..Changes::default()
                },
            )
            .unwrap();
        db.reply_attempted(publication).unwrap();
    }
    (route, source)
}
/// A namespace of one file of `cells` cells, written a cell or a window at
/// a time: rows of one cell, or rows of several.
fn payload(db: &Overlay, index: u8, cells: u64, piece: usize) -> (Route, BaseSource) {
    let route = db.open_workspace([index; 32], [index + 100; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let mut file = File::new(db, source, 7, Vec::new());
    let data = pattern(piece, index);
    for at in 0..cells * CELL_BYTES as u64 / piece as u64 {
        file.write(at * piece as u64, &data);
    }
    (route, source)
}
fn close(db: &Overlay, (route, source): (Route, BaseSource)) {
    db.release_base_source(source).unwrap();
    db.close(route).unwrap();
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Queued);
}
/// Closes and reclaims one namespace beside a live one with rows of the
/// same tables, which must keep every row and every byte.
fn beside_a_live_namespace(build: impl Fn(&Overlay, u8) -> (Route, BaseSource)) -> (Cost, u64) {
    let temp = Temp::new();
    let db = temp.db();
    let (live, live_source) = small_files(&db, 1, 70);
    let mut kept = File::new(&db, live_source, 7, Vec::new());
    for window in 0..3 {
        kept.write(window * WRITE_WINDOW as u64, &pattern(WRITE_WINDOW, 9));
    }
    kept.write(3 * WRITE_WINDOW as u64, b"tail!");
    kept.resize(2 * WRITE_WINDOW as u64 + 100);
    let closed = build(&db, 2);
    let stored = db.resources(Some(closed.0)).unwrap().counts;
    let rows = stored.inode_rows + stored.directory_entry_rows + stored.payload_cells;
    close(&db, closed);
    let before = db.resources(Some(live)).unwrap().counts;
    let cost = reclaim(&db);
    assert_eq!(db.cleanup_state(closed.0).unwrap(), CleanupState::Gone);
    assert_eq!(db.resources(Some(live)).unwrap().counts, before);
    assert_eq!(db.resources(None).unwrap().counts, before);
    kept.check();
    assert_eq!(db.source_inode(live_source, 79).unwrap().unwrap().size, 1);
    assert!(db.reclaim_closed(0).unwrap().is_none());
    (cost, rows)
}

#[test]
fn a_closed_namespace_is_reclaimed_in_pages_of_one_delete_beside_a_live_namespace() {
    const N: u64 = 64;
    let files =
        [N, 4 * N].map(|files| beside_a_live_namespace(|db, at| small_files(db, at, files)));
    let cells = [N, 4 * N]
        .map(|cells| beside_a_live_namespace(|db, at| payload(db, at, cells, CELL_BYTES)));
    let runs = [N, 4 * N]
        .map(|cells| beside_a_live_namespace(|db, at| payload(db, at, cells, WRITE_WINDOW)));
    println!("R7_RECLAIM_COST files={files:?}");
    println!("R7_RECLAIM_COST cells={cells:?}");
    println!("R7_RECLAIM_COST runs={runs:?}");
    let cost = |steps, all, attempts, executions, rows, widest| Cost {
        steps,
        all,
        attempts,
        executions,
        rows,
        widest,
    };
    // Before the pages were one delete: files (21, 312, 249, 444, 207) and
    // (41, 988, 865, 1636, 783); cells (19, 174, 117, 184, 79) and
    // (33, 436, 337, 596, 271); rows of several cells (22, 133, 67, 78, 23)
    // and (46, 277, 139, 174, 47).
    assert_eq!(
        files,
        [
            (cost(9, 69, 42, 237, 197, (64, 438)), 193),
            (cost(29, 189, 102, 873, 773, (64, 504)), 769)
        ]
    );
    // A step drops one page of payload: fourteen cells of bytes.
    let page = 14 * CELL_BYTES as u64;
    assert_eq!(
        cells,
        [
            (cost(7, 56, 35, 102, 68, (14, page)), 65),
            (cost(21, 140, 77, 336, 260, (14, page)), 257)
        ]
    );
    // A row of eight cells is a page alone while the next row is another.
    assert_eq!(
        runs,
        [
            (cost(10, 74, 44, 55, 12, (2, RUN_BYTES as u64)), 9),
            (cost(34, 218, 116, 151, 36, (2, RUN_BYTES as u64)), 33)
        ]
    );
    // Three statements for each further step, whatever its rows: the ready
    // queue, the page and its one delete.
    for [small, large] in [files, cells, runs] {
        assert_eq!(
            large.0.attempts - small.0.attempts,
            3 * (large.0.steps - small.0.steps)
        );
    }
}

/// Every `Column` instruction of one statement that reads a payload value
/// column through a cursor on the payload table: whether it only takes the
/// value's length. Trigger programs are listed after the statement, each
/// with its own cursors.
fn value_reads(raw: &rusqlite::Connection, statement: &str) -> Vec<bool> {
    let root: i64 = raw
        .query_row(
            "SELECT rootpage FROM sqlite_schema WHERE name='payload'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let values: Vec<i64> = raw
        .prepare("SELECT cid FROM pragma_table_info('payload') WHERE name IN('data','validity')")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(values.len(), 2);
    let mut listing = raw.prepare(&format!("EXPLAIN {statement}")).unwrap();
    let program = listing
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(6)?,
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    let mut cursors = Vec::new();
    let mut reads = Vec::new();
    // The cursors of a program are opened anywhere in it: two passes.
    let mut start = 0;
    for end in 1..=program.len() {
        if end != program.len() && program[end].0 != 0 {
            continue;
        }
        let part = &program[start..end];
        cursors.clear();
        for (_, opcode, cursor, page, _) in part {
            if matches!(opcode.as_str(), "OpenRead" | "OpenWrite") && *page == root {
                cursors.push(*cursor);
            }
        }
        for (_, opcode, cursor, column, flags) in part {
            if opcode == "Column" && cursors.contains(cursor) && values.contains(column) {
                // OPFLAG_LENGTHARG: the value is used by length() alone.
                reads.push(flags & 0x40 != 0);
            }
        }
        start = end;
    }
    reads
}

#[test]
fn deleting_payload_rows_reads_their_lengths_and_never_their_bytes() {
    let temp = Temp::new();
    let db = temp.db();
    let built = payload(&db, 2, 64, WRITE_WINDOW);
    let stored = db.resources(Some(built.0)).unwrap().counts;
    assert_eq!(
        (stored.payload_cells, stored.payload_bytes),
        (8, 64 * CELL_BYTES as u64)
    );
    drop(db);
    let raw = rusqlite::Connection::open(temp.0.join("overlay.sqlite")).unwrap();
    // The statements that delete payload rows: by row, by a range of one
    // file's cells, and the terminal page of a closed namespace.
    for statement in [
        "DELETE FROM payload WHERE rowid=1",
        "DELETE FROM payload WHERE ns=1 AND serial=7 AND gen=-1 AND cell_offset>=0 AND cell_offset<32768",
        "DELETE FROM payload WHERE ns=1 AND (serial,gen,cell_offset)<=(7,1,0)",
    ] {
        let reads = value_reads(&raw, statement);
        // The trigger takes both lengths; nothing loads a value.
        assert_eq!(reads, [true, true], "{statement}");
    }
    // The same listing does see a statement that loads a value.
    assert!(value_reads(&raw, "SELECT data FROM payload WHERE rowid=1").contains(&false));
    // The counts follow the rows: deleting one row of eight cells.
    raw.execute("DELETE FROM payload WHERE rowid=1", [])
        .unwrap();
    let left: (i64, i64) = raw
        .query_row(
            "SELECT payload_cells,payload_bytes FROM accounting WHERE ns=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(left, (7, 56 * CELL_BYTES as i64));
    // The trigger this one replaced named the old values, and the delete
    // then loaded both in full.
    raw.execute_batch(
        "DROP TRIGGER payload_account_delete;
        CREATE TRIGGER payload_account_delete AFTER DELETE ON payload BEGIN
            UPDATE accounting SET payload_cells=payload_cells-1,payload_bytes=payload_bytes-(length(OLD.data)+ifnull(length(OLD.validity),0)) WHERE ns=OLD.ns;
        END;",
    )
    .unwrap();
    assert_eq!(
        value_reads(&raw, "DELETE FROM payload WHERE rowid=2"),
        [false, false]
    );
}
