#[path = "../../layerfs-overlay/tests/payload_support/mod.rs"]
mod payload_support;
use layerfs_daemon::{
    Command, Completion, Owner, OwnerClient, OwnerConfig, OwnerError, Pending, Response,
};
use layerfs_overlay::*;
use layerfs_workspace::{OverlayCapturedRuns, WorkspaceError};
use payload_support::Temp;
use std::{
    error::Error,
    time::{Duration, Instant},
};

fn wait(pending: Pending) -> Completion {
    let end = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(done) = pending.try_complete().unwrap() {
            return done;
        }
        assert!(Instant::now() < end, "bounded captured-run job");
        std::thread::yield_now();
    }
}
fn job(client: &OwnerClient, route: Option<Route>, command: Command) -> Completion {
    wait(
        client
            .try_submit(route, command)
            .unwrap_or_else(|(e, c)| panic!("{e:?} {c:?}")),
    )
}
fn setup(client: &OwnerClient, tag: u8, size: u64, with_cell: bool) -> (Route, CapturedReader) {
    let opened = job(
        client,
        None,
        Command::Open {
            incarnation: [tag; 32],
            base_root: [tag + 1; 32],
        },
    );
    let route = match opened.result() {
        Ok(Response::Opened(route)) => *route,
        v => panic!("{v:?}"),
    };
    drop(opened);
    let mut cell = Cell {
        offset: 0,
        data: Box::new([0; CELL_BYTES]),
        validity: Box::new([0; MASK_BYTES]),
    };
    cell.data[..4].copy_from_slice(b"port");
    cell.validity[0] = 15;
    let published = job(
        client,
        Some(route),
        Command::Publish {
            inode: Inode {
                serial: 17,
                kind: InodeKind::File,
                mode: 0o644,
                mtime_seconds: 1,
                mtime_nanoseconds: 0,
                nlink: 1,
                size,
                inherited_cutoff: 0,
                born: 0,
                entries: 0,
            },
            name: None,
            cell: with_cell.then_some(cell),
        },
    );
    let publication = match published.result() {
        Ok(Response::Published(p)) => *p,
        v => panic!("{v:?}"),
    };
    drop(published);
    assert!(
        job(client, Some(route), Command::ReplyAttempted(publication))
            .result()
            .is_ok()
    );
    let captured = job(client, Some(route), Command::Capture);
    let capture = match captured.result() {
        Ok(Response::Captured(c)) => *c,
        v => panic!("{v:?}"),
    };
    drop(captured);
    let acquired = job(
        client,
        Some(route),
        Command::AcquireCapturedReader {
            capture,
            request: 1,
        },
    );
    let reader = match acquired.result() {
        Ok(Response::CapturedReader(Some(r))) => *r,
        v => panic!("{v:?}"),
    };
    (route, reader)
}

#[test]
fn actual_owner_returns_bounded_window_with_original_root_after_install_and_close() {
    let temp = Temp::new();
    let owner = Owner::start(
        &temp.0.join("db"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let (route, reader) = setup(&client, 161, CELL_BYTES as u64, true);
    let cursor = CapturedRunCursor::new(reader, 17, 0, CELL_BYTES as u64).unwrap();
    for phase in 0..3 {
        let reply = client.captured_run_step(cursor).unwrap();
        let CapturedRunStep::Window { read, next } = reply.step else {
            panic!("window expected")
        };
        assert_eq!(read.base_root, Some(reader.root()));
        assert_eq!(read.data.len(), CELL_BYTES);
        assert!(read.data.capacity() <= CELL_BYTES && read.inherited.capacity() <= MASK_BYTES);
        assert_eq!(next.reader(), reader);
        assert_eq!(&read.data[..4], b"port");
        assert!(read.data[4..].iter().all(|b| *b == 0));
        assert!(matches!(
            client.captured_run_step(next).unwrap().step,
            CapturedRunStep::End
        ));
        if phase == 0 {
            assert!(job(
                &client,
                Some(route),
                Command::Install {
                    capture: reader.capture(),
                    root: [199; 32]
                }
            )
            .result()
            .is_ok());
        }
        if phase == 1 {
            assert!(job(&client, Some(route), Command::Close).result().is_ok());
        }
    }
    assert!(
        job(&client, Some(route), Command::ReleaseCapturedReader(reader))
            .result()
            .is_ok()
    );
    assert_eq!(client.diagnostics().unwrap().credited_bytes, 0);
    owner.stop().unwrap();
}

#[test]
fn captured_inode_point_retains_old_metadata_and_forward_seek_bounds() {
    let temp = Temp::new();
    let owner = Owner::start(
        &temp.0.join("db"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let (route, reader) = setup(&client, 167, CELL_BYTES as u64, true);
    let original = client.captured_inode(reader, 17).unwrap().unwrap();
    assert_eq!((original.size, original.mode), (CELL_BYTES as u64, 0o644));
    assert!(!reader.created_above(original.born));
    assert!(client.captured_inode(reader, 999).unwrap().is_none());
    assert!(job(
        &client,
        Some(route),
        Command::Install {
            capture: reader.capture(),
            root: [198; 32]
        }
    )
    .result()
    .is_ok());
    let mut current = original.clone();
    current.size = 9;
    current.mode = 0o600;
    let published = job(
        &client,
        Some(route),
        Command::Publish {
            inode: current,
            name: None,
            cell: None,
        },
    );
    let publication = match published.result() {
        Ok(Response::Published(p)) => *p,
        v => panic!("{v:?}"),
    };
    drop(published);
    assert!(
        job(&client, Some(route), Command::ReplyAttempted(publication))
            .result()
            .is_ok()
    );
    assert_eq!(
        client.captured_inode(reader, 17).unwrap(),
        Some(original.clone())
    );
    let changed = job(&client, Some(route), Command::Inode(17));
    assert!(matches!(changed.result(),Ok(Response::Inode(Some(i))) if i.size==9 && i.mode==0o600));
    drop(changed);
    let cursor = CapturedRunCursor::new(reader, 17, 0, CELL_BYTES as u64).unwrap();
    let forward = cursor.seek_forward(200).unwrap();
    assert!(matches!(
        forward.seek_forward(199),
        Err(OverlayError::Invalid("captured run seek"))
    ));
    assert!(matches!(
        forward.seek_forward(CELL_BYTES as u64 + 1),
        Err(OverlayError::Invalid("captured run seek"))
    ));
    let mut cursor = forward;
    let mut gap = None;
    for _ in 0..3 {
        match client.captured_run_step(cursor).unwrap().step {
            CapturedRunStep::Continue(next) => cursor = next,
            CapturedRunStep::Gap {
                kind,
                offset,
                length,
                next,
            } => {
                gap = Some((kind, offset, length, next));
                break;
            }
            other => panic!("trimmed tail gap: {other:?}"),
        }
    }
    let (kind, offset, length, next) = gap.expect("bounded normal metadata progress");
    assert_eq!(
        (kind, offset, length),
        (CapturedGap::Zero, 200, CELL_BYTES as u64 - 200)
    );
    assert_eq!(next.reader(), reader);
    assert!(job(&client, Some(route), Command::Close).result().is_ok());
    assert_eq!(
        client.captured_inode(reader, 17).unwrap(),
        Some(original.clone())
    );
    assert!(
        job(&client, Some(route), Command::ReleaseCapturedReader(reader))
            .result()
            .is_ok()
    );
    assert!(client.captured_inode(reader, 17).is_err());
    owner.stop().unwrap();
}

#[test]
fn original_attempted_and_unattempted_run_errors_keep_custody_and_other_scope_progress() {
    let temp = Temp::new();
    let owner = Owner::start(
        &temp.0.join("db"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let (route, reader) = setup(&client, 163, 3, false);
    let cursor = CapturedRunCursor::new(reader, 17, 0, 4).unwrap();
    let error = client.captured_run_step(cursor).unwrap_err();
    let done = error
        .source()
        .unwrap()
        .downcast_ref::<Completion>()
        .expect("original completion");
    assert!(matches!(
        done.result(),
        Err(OwnerError::Overlay(OverlayError::Invalid(
            "captured run size"
        )))
    ));
    assert!(client.diagnostics().unwrap().credited_bytes > 0);
    let (other, other_reader) = setup(&client, 165, (1_u64 << 32) + 9, false);
    let other_cursor = CapturedRunCursor::new(other_reader, 17, 0, (1_u64 << 32) + 9).unwrap();
    assert!(matches!(
        client.captured_run_step(other_cursor).unwrap().step,
        CapturedRunStep::Gap {
            kind: CapturedGap::Zero,
            ..
        }
    ));
    drop(error);
    for (route, reader) in [(route, reader), (other, other_reader)] {
        assert!(
            job(&client, Some(route), Command::ReleaseCapturedReader(reader))
                .result()
                .is_ok()
        );
        assert!(job(&client, Some(route), Command::Close).result().is_ok());
    }
    owner.stop().unwrap();
    // A fresh request position, never submitted while the owner was live.
    let stopped_input = CapturedRunCursor::new(other_reader, 17, 1, (1_u64 << 32) + 9).unwrap();
    let error = client.captured_run_step(stopped_input).unwrap_err();
    let WorkspaceError::Service(original) = error else {
        panic!("original service failure")
    };
    assert!(
        matches!(original.downcast_ref::<OwnerError>(),Some(OwnerError::Unattempted{cause,command})
        if matches!(cause.as_ref(),OwnerError::Stopped)
            && matches!(command.as_ref(),Command::CapturedRun(input) if input.reader()==other_reader && input.offset()==1 && input.logical_size()==(1_u64<<32)+9))
    );
}
