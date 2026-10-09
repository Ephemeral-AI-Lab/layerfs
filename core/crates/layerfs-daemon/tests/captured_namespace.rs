//! Captured namespace ports use the original retained reader, never live rows.
#[path = "../../layerfs-overlay/tests/payload_support/mod.rs"]
mod payload_support;
use layerfs_daemon::{Command, Completion, Owner, OwnerClient, OwnerConfig, OwnerError, Response};
use layerfs_overlay::{
    CapturedReader, Cell, DirectoryEntry, Inode, InodeKind, OverlayError, ProfileConfig, Route,
    CELL_BYTES, MASK_BYTES, PAGE_ROWS,
};
use layerfs_workspace::{OverlayCapturedNamespace, OverlayCapturedRuns, WorkspaceError};
use payload_support::Temp;
use std::time::{Duration, Instant};

fn inode(serial: u64) -> Inode {
    Inode {
        serial,
        kind: InodeKind::File,
        mode: 0o640,
        mtime_seconds: -2,
        mtime_nanoseconds: 3,
        nlink: u64::from(serial % 9 != 0),
        size: serial,
        inherited_cutoff: 0,
        born: 0,
        entries: 0,
        subdirs: 0,
    }
}
fn entry(serial: u64) -> DirectoryEntry {
    let mut name = vec![b'x'; 253];
    name.extend_from_slice(format!("{:02x}", serial % 256).as_bytes());
    DirectoryEntry {
        parent: 10_000 + serial / 65,
        name,
        serial: (serial % 9 != 0).then_some(serial),
        inherited: true,
    }
}
fn job(client: &OwnerClient, route: Option<Route>, command: Command) -> Completion {
    let pending = client
        .try_submit(route, command)
        .unwrap_or_else(|(e, c)| panic!("{e:?}: {c:?}"));
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(done) = pending.try_complete().unwrap() {
            return done;
        }
        assert!(Instant::now() < until, "bounded namespace setup job");
        std::thread::yield_now();
    }
}
fn publish(client: &OwnerClient, route: Route, serial: u64) {
    publish_row(client, route, inode(serial), Some(entry(serial)), None);
}
fn publish_row(
    client: &OwnerClient,
    route: Route,
    inode: Inode,
    name: Option<DirectoryEntry>,
    cell: Option<Cell>,
) {
    let done = job(client, Some(route), Command::Publish { inode, name, cell });
    let ticket = match done.result() {
        Ok(Response::Published(p)) => *p,
        other => panic!("{other:?}"),
    };
    drop(done);
    assert!(job(client, Some(route), Command::ReplyAttempted(ticket))
        .result()
        .is_ok());
}
fn assert_pages(provider: &impl OverlayCapturedNamespace, reader: CapturedReader) {
    let mut after = 0;
    let mut inodes = Vec::new();
    let mut pages = 0;
    loop {
        let page = provider.captured_inode_page(reader, after).unwrap();
        assert!(page.len() <= PAGE_ROWS && page.capacity() <= PAGE_ROWS);
        if page.is_empty() {
            break;
        }
        pages += 1;
        for row in &page {
            assert!(row.serial > after);
            after = row.serial;
            assert_eq!(
                provider
                    .captured_inode(reader, row.serial)
                    .unwrap()
                    .as_ref(),
                Some(row)
            );
        }
        inodes.extend(page);
    }
    assert_eq!(pages, 3);
    assert_eq!(inodes, (1..=130).map(inode).collect::<Vec<_>>());
    assert!(provider.captured_inode(reader, 1000).unwrap().is_none());
    let mut cursor = None;
    let mut entries = Vec::new();
    let mut pages = 0;
    loop {
        let page = provider
            .captured_directory_entry_page(reader, cursor.clone())
            .unwrap();
        assert!(page.len() <= PAGE_ROWS && page.capacity() <= PAGE_ROWS);
        if page.is_empty() {
            break;
        }
        pages += 1;
        for row in &page {
            let key = (row.parent, row.name.clone());
            assert!(cursor.as_ref().is_none_or(|old| *old < key));
            assert_eq!(row.name.len(), 255);
            assert!(row.name.capacity() <= 255);
            cursor = Some(key);
        }
        entries.extend(page);
    }
    let mut expected = (1..=130).map(entry).collect::<Vec<_>>();
    expected.sort_by(|a, b| (a.parent, &a.name).cmp(&(b.parent, &b.name)));
    assert_eq!(pages, 3);
    assert_eq!(entries, expected);
    assert_eq!(entries.iter().filter(|e| e.serial.is_none()).count(), 14);
    // Parent-local cursors partition that same sealed sequence, whiteouts
    // included, and the name point answers every row of it identically.
    let mut joined = Vec::new();
    for (parent, windows) in [(10_000, 1), (10_001, 2), (10_002, 1)] {
        let mut after = None;
        let mut seen = 0;
        for _ in 0..8 {
            let page = provider
                .captured_directory_entries(reader, parent, after.clone())
                .unwrap();
            assert!(page.len() <= PAGE_ROWS && page.capacity() <= PAGE_ROWS);
            let Some(last) = page.last() else {
                break;
            };
            assert!(page.iter().all(|row| row.parent == parent));
            after = Some(last.name.clone());
            seen += 1;
            joined.extend(page);
        }
        assert_eq!(seen, windows, "parent {parent}");
    }
    assert_eq!(joined, entries);
    for row in &entries {
        assert_eq!(
            provider
                .captured_directory_entry(reader, row.parent, &row.name)
                .unwrap()
                .as_ref(),
            Some(row)
        );
    }
    assert!(provider
        .captured_directory_entries(reader, 9_999, None)
        .unwrap()
        .is_empty());
    // A later active name under a captured parent has no sealed row.
    let later = entry(131);
    assert_eq!(later.parent, 10_002);
    assert_eq!(
        provider
            .captured_directory_entry(reader, later.parent, &later.name)
            .unwrap(),
        None
    );
}
/// The engine thread drops its own final credit slightly after the consumer
/// observes a completion: bound the wait for the expected count.
fn outstanding(client: &OwnerClient, expected: usize) {
    let until = Instant::now() + Duration::from_secs(3);
    while client.diagnostics().unwrap().outstanding != expected {
        assert!(Instant::now() < until, "owner credit did not settle");
        std::thread::yield_now();
    }
}
/// A failed attempted job: the original completion with its SQL receipt and
/// credit stays inside the error until the caller drops it.
fn attempted(client: &OwnerClient, error: WorkspaceError, expected: &str) {
    let WorkspaceError::Service(original) = error else {
        panic!("original completion expected")
    };
    let completion = original.downcast_ref::<Completion>().unwrap();
    match completion.result() {
        Err(OwnerError::Overlay(cause)) => assert_eq!(format!("{cause:?}"), expected),
        other => panic!("{other:?}"),
    }
    assert!(completion.work().sql.total().attempts > 0);
    outstanding(client, 1);
    assert!(client.diagnostics().unwrap().credited_bytes > 0);
    drop(original);
    outstanding(client, 0);
    assert_eq!(client.diagnostics().unwrap().credited_bytes, 0);
}
fn link(serial: u64, target: &[u8], born: u64) -> (Inode, Cell) {
    let mut data = Box::new([0_u8; CELL_BYTES]);
    data[..target.len()].copy_from_slice(target);
    let mut validity = Box::new([0_u8; MASK_BYTES]);
    for bit in 0..target.len() {
        validity[bit / 8] |= 1 << (bit % 8);
    }
    (
        Inode {
            serial,
            kind: InodeKind::Symlink,
            mode: 0o777,
            mtime_seconds: 5,
            mtime_nanoseconds: 6,
            nlink: 1,
            size: target.len() as u64,
            inherited_cutoff: 0,
            born,
            entries: 0,
            subdirs: 0,
        },
        Cell {
            offset: 0,
            data,
            validity,
        },
    )
}
fn named(parent: u64, name: &[u8], serial: Option<u64>, inherited: bool) -> DirectoryEntry {
    DirectoryEntry {
        inherited,
        parent,
        name: name.to_vec(),
        serial,
    }
}

#[test]
fn direct_provider_pages_and_points_keep_the_exact_capture_after_install_and_close() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([191; 32], [192; 32]).unwrap();
    for serial in 1..=130 {
        let ticket = db
            .publish(route, &inode(serial), Some(&entry(serial)), None)
            .unwrap();
        db.reply_attempted(ticket).unwrap();
    }
    let capture = db.capture(route).unwrap();
    let reader = db.acquire_captured_reader(capture, 1).unwrap();
    for serial in 131..=260 {
        let ticket = db
            .publish(route, &inode(serial), Some(&entry(serial)), None)
            .unwrap();
        db.reply_attempted(ticket).unwrap();
    }
    assert_pages(&db, reader);
    db.install(capture, [193; 32]).unwrap();
    db.close(route).unwrap();
    assert_pages(&db, reader);
    assert_eq!(reader.root(), [192; 32]);
    // The direct symlink port: the exact local target, or the engine's refusal.
    let other = db.open_workspace([188; 32], [189; 32]).unwrap();
    let born = db.active_generation(other).unwrap().number() as u64;
    let (fresh, cell) = link(7, b"direct/target", born);
    let ticket = db.publish(other, &fresh, None, Some(&cell)).unwrap();
    db.reply_attempted(ticket).unwrap();
    let linked = db
        .acquire_captured_reader(db.capture(other).unwrap(), 1)
        .unwrap();
    assert_eq!(db.captured_symlink(linked, 7).unwrap(), b"direct/target");
    assert!(matches!(
        db.captured_symlink(reader, 1),
        Err(WorkspaceError::Overlay(OverlayError::Missing))
    ));
    db.release_captured_reader(linked).unwrap();
    assert!(matches!(
        db.captured_symlink(linked, 7),
        Err(WorkspaceError::Overlay(OverlayError::Stale))
    ));
    db.release_captured_reader(reader).unwrap();
    assert!(matches!(
        db.captured_inode_page(reader, 0),
        Err(WorkspaceError::Overlay(OverlayError::Stale))
    ));
}

#[test]
fn owner_pages_preserve_original_failed_completion_and_unattempted_command() {
    let temp = Temp::new();
    let owner = Owner::start(
        &temp.0.join("db"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let done = job(
        &client,
        None,
        Command::Open {
            incarnation: [194; 32],
            base_root: [195; 32],
        },
    );
    let route = match done.result() {
        Ok(Response::Opened(r)) => *r,
        other => panic!("{other:?}"),
    };
    drop(done);
    for serial in 1..=130 {
        publish(&client, route, serial);
    }
    let done = job(&client, Some(route), Command::Capture);
    let capture = match done.result() {
        Ok(Response::Captured(c)) => *c,
        other => panic!("{other:?}"),
    };
    drop(done);
    let done = job(
        &client,
        Some(route),
        Command::AcquireCapturedReader {
            capture,
            request: 1,
        },
    );
    let reader = match done.result() {
        Ok(Response::CapturedReader(Some(r))) => *r,
        other => panic!("{other:?}"),
    };
    drop(done);
    for serial in 131..=260 {
        publish(&client, route, serial);
    }
    assert_pages(&client, reader);
    assert!(job(
        &client,
        Some(route),
        Command::Install {
            capture,
            root: [196; 32]
        }
    )
    .result()
    .is_ok());
    assert!(job(&client, Some(route), Command::Close).result().is_ok());
    assert_pages(&client, reader);
    let error = client
        .captured_directory_entry_page(reader, Some((1, vec![b'x'; 256])))
        .unwrap_err();
    let WorkspaceError::Service(original) = error else {
        panic!("original completion expected")
    };
    let completion = original.downcast_ref::<Completion>().unwrap();
    assert!(matches!(
        completion.result(),
        Err(OwnerError::Overlay(OverlayError::Invalid(
            "captured name cursor"
        )))
    ));
    assert!(completion.work().sql.total().attempts > 0);
    assert_eq!(client.diagnostics().unwrap().outstanding, 1);
    drop(original);
    // The failed read did not dispose the reader or turn later observation into
    // a replay. This is a distinct explicit original point call.
    assert_eq!(client.captured_inode(reader, 1).unwrap(), Some(inode(1)));
    assert!(
        job(&client, Some(route), Command::ReleaseCapturedReader(reader))
            .result()
            .is_ok()
    );
    owner.stop().unwrap();
    let error = client.captured_inode_page(reader, 64).unwrap_err();
    let WorkspaceError::Service(original) = error else {
        panic!("unattempted command expected")
    };
    match original.downcast_ref::<OwnerError>().unwrap() {
        OwnerError::Unattempted { cause, command } => {
            assert!(matches!(**cause, OwnerError::Stopped));
            assert!(
                matches!(command.as_ref(),Command::ReaderInodes{reader:r,after:64} if *r==reader)
            );
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(client.diagnostics().unwrap().credited_bytes, 0);
}

#[test]
fn owner_parent_names_points_and_symlinks_keep_original_custody() {
    let temp = Temp::new();
    let owner = Owner::start(
        &temp.0.join("db"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let done = job(
        &client,
        None,
        Command::Open {
            incarnation: [197; 32],
            base_root: [198; 32],
        },
    );
    let route = match done.result() {
        Ok(Response::Opened(r)) => *r,
        other => panic!("{other:?}"),
    };
    drop(done);
    let done = job(&client, Some(route), Command::State);
    let born = match done.result() {
        Ok(Response::State(state)) => state.active.number() as u64,
        other => panic!("{other:?}"),
    };
    drop(done);
    // One parent over a full window, a whiteout, and two fresh symlinks: a
    // short target and the largest one cell holds.
    let mut expected = Vec::new();
    for n in 0..70_u64 {
        let row = named(7, format!("n{n:02}").as_bytes(), Some(100 + n), n % 2 == 0);
        publish_row(&client, route, inode(100 + n), Some(row.clone()), None);
        expected.push(row);
    }
    let whiteout = named(7, b"w", None, true);
    publish_row(&client, route, inode(99), Some(whiteout.clone()), None);
    expected.push(whiteout);
    let full: Vec<u8> = (0..CELL_BYTES).map(|i| 1 + (i % 251) as u8).collect();
    for (serial, target) in [(50, b"sealed/target".as_slice()), (51, full.as_slice())] {
        let (inode, cell) = link(serial, target, born);
        let name = named(8, format!("l{serial}").as_bytes(), Some(serial), false);
        publish_row(&client, route, inode, Some(name), Some(cell));
    }
    let done = job(&client, Some(route), Command::Capture);
    let capture = match done.result() {
        Ok(Response::Captured(c)) => *c,
        other => panic!("{other:?}"),
    };
    drop(done);
    let done = job(
        &client,
        Some(route),
        Command::AcquireCapturedReader {
            capture,
            request: 1,
        },
    );
    let reader = match done.result() {
        Ok(Response::CapturedReader(Some(r))) => *r,
        other => panic!("{other:?}"),
    };
    drop(done);
    // Later active rows under the same keys and beside them.
    for n in 0..70_u64 {
        let row = named(7, format!("n{n:02}a").as_bytes(), Some(300 + n), false);
        publish_row(&client, route, inode(300 + n), Some(row), None);
    }
    publish_row(
        &client,
        route,
        inode(400),
        Some(named(7, b"n05", Some(400), true)),
        None,
    );
    publish_row(
        &client,
        route,
        inode(401),
        Some(named(7, b"n06", None, true)),
        None,
    );
    let (mut removed, _) = link(50, b"sealed/target", born);
    removed.nlink = 0;
    publish_row(
        &client,
        route,
        removed,
        Some(named(8, b"l50", None, true)),
        None,
    );
    let (inode52, cell52) = link(52, b"active/only", born + 1);
    publish_row(
        &client,
        route,
        inode52,
        Some(named(8, b"l52", Some(52), false)),
        Some(cell52),
    );

    let sealed = |client: &OwnerClient| {
        let first = client.captured_directory_entries(reader, 7, None).unwrap();
        assert!(first.len() == PAGE_ROWS && first.capacity() <= PAGE_ROWS);
        let rest = client
            .captured_directory_entries(reader, 7, Some(first[PAGE_ROWS - 1].name.clone()))
            .unwrap();
        assert!(client
            .captured_directory_entries(reader, 7, Some(b"w".to_vec()))
            .unwrap()
            .is_empty());
        let rows = [first, rest].concat();
        assert_eq!(rows, expected);
        for row in &rows {
            let point = client
                .captured_directory_entry(reader, 7, &row.name)
                .unwrap();
            assert_eq!(point.as_ref(), Some(row));
        }
        for (parent, name) in [
            (7, &b"n05a"[..]),
            (8, &b"l52"[..]),
            (9, &b"n05"[..]),
            (7, &b""[..]),
        ] {
            assert_eq!(
                client
                    .captured_directory_entry(reader, parent, name)
                    .unwrap(),
                None
            );
        }
        let short = client.captured_symlink(reader, 50).unwrap();
        let long = client.captured_symlink(reader, 51).unwrap();
        assert!(short.capacity() <= CELL_BYTES && long.capacity() <= CELL_BYTES);
        assert_eq!(
            (short.as_slice(), long.as_slice()),
            (&b"sealed/target"[..], full.as_slice())
        );
    };
    sealed(&client);
    // A successful call leaves no credit behind.
    outstanding(&client, 0);
    assert_eq!(client.diagnostics().unwrap().credited_bytes, 0);

    // Each failed job keeps its original completion; none disposes the reader.
    attempted(
        &client,
        client
            .captured_directory_entries(reader, 7, Some(vec![b'x'; 256]))
            .unwrap_err(),
        "Invalid(\"captured name cursor\")",
    );
    attempted(
        &client,
        client
            .captured_directory_entry(reader, 7, &[b'x'; 256])
            .unwrap_err(),
        "Invalid(\"captured name\")",
    );
    // A regular file, an active-only symlink and a serial with no row.
    for serial in [100, 52, 9_999] {
        attempted(
            &client,
            client.captured_symlink(reader, serial).unwrap_err(),
            "Missing",
        );
    }
    sealed(&client);

    assert!(job(
        &client,
        Some(route),
        Command::Install {
            capture,
            root: [199; 32]
        }
    )
    .result()
    .is_ok());
    sealed(&client);
    assert!(job(&client, Some(route), Command::Close).result().is_ok());
    sealed(&client);
    assert!(
        job(&client, Some(route), Command::ReleaseCapturedReader(reader))
            .result()
            .is_ok()
    );
    attempted(
        &client,
        client
            .captured_directory_entries(reader, 7, None)
            .unwrap_err(),
        "Stale",
    );
    attempted(
        &client,
        client
            .captured_directory_entry(reader, 7, b"n00")
            .unwrap_err(),
        "Stale",
    );
    attempted(
        &client,
        client.captured_symlink(reader, 50).unwrap_err(),
        "Stale",
    );

    // A stopped owner returns each exact command, never attempted.
    owner.stop().unwrap();
    let unattempted = |error: WorkspaceError, check: &dyn Fn(&Command) -> bool| {
        let WorkspaceError::Service(original) = error else {
            panic!("unattempted command expected")
        };
        match original.downcast_ref::<OwnerError>().unwrap() {
            OwnerError::Unattempted { cause, command } => {
                assert!(matches!(**cause, OwnerError::Stopped));
                assert!(check(command), "{command:?}");
            }
            other => panic!("{other:?}"),
        }
    };
    unattempted(
        client
            .captured_directory_entries(reader, 7, Some(b"n10".to_vec()))
            .unwrap_err(),
        &|command| {
            matches!(command, Command::ReaderParentDirectoryEntries { reader: r, parent: 7, after: Some(after) }
                if *r == reader && after == b"n10")
        },
    );
    unattempted(
        client
            .captured_directory_entry(reader, 7, b"n11")
            .unwrap_err(),
        &|command| {
            matches!(command, Command::ReaderDirectoryEntry { reader: r, parent: 7, name }
                if *r == reader && name == b"n11")
        },
    );
    unattempted(
        client.captured_symlink(reader, 50).unwrap_err(),
        &|command| matches!(command, Command::ReaderSymlink { reader: r, serial: 50 } if *r == reader),
    );
    assert_eq!(client.diagnostics().unwrap().outstanding, 0);
    assert_eq!(client.diagnostics().unwrap().credited_bytes, 0);
}
