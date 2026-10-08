//! Captured namespace ports use the original retained reader, never live rows.
#[path = "../../layerfs-overlay/tests/payload_support/mod.rs"]
mod payload_support;
use layerfs_daemon::{Command, Completion, Owner, OwnerClient, OwnerConfig, OwnerError, Response};
use layerfs_overlay::{
    CapturedReader, DirectoryEntry, Inode, InodeKind, OverlayError, ProfileConfig, Route, PAGE_ROWS,
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
    let done = job(
        client,
        Some(route),
        Command::Publish {
            inode: inode(serial),
            name: Some(entry(serial)),
            cell: None,
        },
    );
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
