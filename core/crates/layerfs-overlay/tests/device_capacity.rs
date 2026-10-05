//! Explicit owned-filesystem resource proof, never a timing/cache benchmark.
#![cfg(target_os = "linux")]
mod payload_support;
use layerfs_overlay::*;
use payload_support::*;
use std::{fs::OpenOptions, path::PathBuf, sync::Arc};
#[test]
#[ignore = "requires exclusively owned Linux ext4 loop fixture; use device_fixture.sh"]
#[cfg(target_os = "linux")]
fn real_device_enospc_refuses_before_sql_and_retains_cleanup_headroom() {
    let root =
        PathBuf::from(std::env::var_os("LAYERFS_PROOF_DEVICE_ROOT").expect("owned proof root"));
    let path = root.join("overlay.sqlite");
    let db = Overlay::create(&path, ProfileConfig::default()).unwrap();
    let a = db.open_workspace([171; 32], [172; 32]).unwrap();
    let b = db.open_workspace([173; 32], [172; 32]).unwrap();
    let source = db.acquire_base_source(a, 1).unwrap();
    let mut f = payload_support::File::new(&db, source, 7, Vec::new());
    f.write(0, &pattern(131072, 3));
    db.release_base_source(source).unwrap();
    let capture = db.capture(a).unwrap();
    let source = db.acquire_base_source(a, 2).unwrap();
    // Fixture allocation is outside the product. No failed allocation is retried.
    let filler = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(root.join("fixture-fill"))
        .unwrap();
    let fs = nix::sys::statvfs::statvfs(root.as_path()).unwrap();
    let free = fs.blocks_free() * fs.fragment_size();
    let fill = nix::fcntl::fallocate(
        &filler,
        nix::fcntl::FallocateFlags::empty(),
        0,
        i64::try_from(free).unwrap(),
    );
    if let Err(error) = fill {
        assert_eq!(
            error,
            nix::errno::Errno::ENOSPC,
            "fixture allocation failure"
        );
    }
    let full = nix::sys::statvfs::statvfs(root.as_path()).unwrap();
    println!("S6_DEVICE_FIXTURE requested_bytes={free} fill={fill:?} residual_free_blocks={} available_blocks={}",full.blocks_free(),full.blocks_available());
    let before = db.state(a).unwrap();
    let other = db.state(b).unwrap();
    let work = db.diagnostics();
    let physical = db.resources(Some(a)).unwrap();
    let mut inode = f.inode(262144);
    inode.born = 0;
    let result = db.apply(
        source,
        &Changes {
            inodes: vec![inode],
            write: Some(PayloadWrite {
                serial: 7,
                offset: 131072,
                data: Arc::from(pattern(131072, 5)),
            }),
            ..Changes::default()
        },
    );
    // Depending on whether the already-owned reserve covers this growth window,
    // exactly one bounded write may be accepted. The following new distinct
    // window then needs replenishment and must refuse without entering SQL.
    if let Ok(publication) = result {
        db.reply_attempted(publication).unwrap();
        inode = f.inode(393216);
        let work = db.diagnostics();
        let prior = db.state(a).unwrap();
        let error = db
            .apply(
                source,
                &Changes {
                    inodes: vec![inode],
                    write: Some(PayloadWrite {
                        serial: 7,
                        offset: 262144,
                        data: Arc::from(pattern(131072, 7)),
                    }),
                    ..Changes::default()
                },
            )
            .unwrap_err();
        reservation_enospc(&error);
        assert_eq!(db.state(a).unwrap(), prior);
        assert_eq!(
            db.diagnostics().statements[StatementKind::Begin as usize].executions,
            work.statements[StatementKind::Begin as usize].executions
        );
    } else {
        reservation_enospc(&result.unwrap_err());
        assert_eq!(db.state(a).unwrap(), before);
        assert_eq!(
            db.diagnostics().statements[StatementKind::Begin as usize].executions,
            work.statements[StatementKind::Begin as usize].executions
        );
    }
    assert_eq!(db.state(b).unwrap(), other);
    assert_eq!(db.retained_capture(a).unwrap(), Some(capture));
    assert_eq!(
        db.captured_read(capture, 7, 0, 131072)
            .unwrap()
            .unwrap()
            .data,
        f.expect
    );
    // Ordinary admission is closed, but exact releases/capture disposition and
    // bounded deletion are supported by the separate reserved headroom.
    db.release_base_source(source).unwrap();
    db.close(a).unwrap();
    db.release_closed_capture(capture).unwrap();
    for turn in 0..2000 {
        let step = db.reclaim_closed(0).unwrap().unwrap();
        if step.done {
            break;
        }
        assert!(turn < 1999);
    }
    assert_eq!(db.cleanup_state(a).unwrap(), CleanupState::Gone);
    assert_eq!(db.state(b).unwrap(), other);
    let after = db.resources(None).unwrap();
    assert!(after.free_pages > physical.free_pages);
    assert!(after.allocation.reserved_tail_bytes >= CLEANUP_HEADROOM);
    println!("S6_DEVICE_ENOSPC fixture_requested_bytes={free} before={physical:?} after={after:?} unrelated_namespace_unchanged=true cleanup_headroom=true");
    drop(filler);
    drop(db);
    std::fs::remove_file(root.join("fixture-fill")).unwrap();
    std::fs::remove_file(path).unwrap();
}

fn reservation_enospc(error: &OverlayError) {
    assert!(
        matches!(error, OverlayError::Reservation { cause, .. }
        if matches!(cause.as_ref(), OverlayError::Io(io) if io.raw_os_error()==Some(28))),
        "original allocation ENOSPC: {error:?}"
    );
}
