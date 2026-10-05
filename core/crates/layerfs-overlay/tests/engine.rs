//! Public engine proofs; no private source compilation or product fault hooks.
use layerfs_overlay::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let path = std::env::temp_dir().join(format!(
            "layerfs-overlay-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn db(&self) -> PathBuf {
        self.0.join("overlay.sqlite")
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
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
        size: 4096,
        inherited_cutoff: 0,
    }
}
fn cell(value: u8) -> Cell {
    let mut data = Box::new([value; CELL_BYTES]);
    data[0] = 0;
    data[1] = 255;
    Cell {
        offset: 0,
        data,
        validity: Box::new([255; MASK_BYTES]),
    }
}
fn kind(work: DatabaseWork, kind: StatementKind) -> StatementWork {
    work.statements[kind as usize]
}

#[test]
fn profile_namespace_binary_values_and_atomic_refusals() {
    let temp = Temp::new();
    let db = Overlay::create(&temp.db(), ProfileConfig::default()).unwrap();
    assert!(Overlay::create(&temp.db(), ProfileConfig::default()).is_err());
    let p = db.profile();
    assert_eq!(p.schema_version, 2);
    assert_eq!(
        (
            &*p.journal_mode,
            &*p.locking_mode,
            p.synchronous,
            p.mmap_size,
            p.busy_timeout
        ),
        ("memory", "exclusive", 0, 0, 0)
    );
    assert_eq!(
        (p.page_size, p.cache_size, p.foreign_keys, p.temp_store),
        (4096, -2048, 1, 1)
    );
    assert!(!p.compile_options.is_empty());
    let a = db.open_workspace([1; 32], [9; 32]).unwrap();
    let b = db.open_workspace([2; 32], [8; 32]).unwrap();
    assert_ne!(a.namespace(), b.namespace());
    assert!(db.open_workspace([1; 32], [9; 32]).is_err());
    let before = kind(db.diagnostics(), StatementKind::Commit).attempts;
    let publication = db
        .publish(
            a,
            &inode(2),
            Some(&Dentry {
                parent: 1,
                name: b"ignored.bin".to_vec(),
                serial: Some(2),
            }),
            Some(&cell(61)),
        )
        .unwrap();
    assert_eq!(
        kind(db.diagnostics(), StatementKind::Commit).attempts - before,
        1
    );
    assert_eq!(db.inode(a, 2).unwrap(), Some(inode(2)));
    assert_eq!(
        db.cell(a, 2, publication.generation, 0).unwrap(),
        Some(cell(61))
    );
    assert!(db.inode(b, 2).unwrap().is_none());
    assert!(db.cell(b, 2, publication.generation, 0).unwrap().is_none());
    assert_eq!(
        db.dentry(a, 1, b"ignored.bin").unwrap().unwrap().serial,
        Some(2)
    );
    let mut bad = inode(2);
    bad.mode = 0o4777;
    let state = db.state(a).unwrap();
    let before = kind(db.diagnostics(), StatementKind::Begin).attempts;
    assert!(matches!(
        db.publish(a, &bad, None, None),
        Err(OverlayError::Invalid(_))
    ));
    assert_eq!(
        kind(db.diagnostics(), StatementKind::Begin).attempts,
        before
    );
    assert_eq!(db.state(a).unwrap(), state);
    assert_eq!(db.inode(a, 2).unwrap(), Some(inode(2)));
    println!("readback={p:?}; pages={:?}", db.pages().unwrap());
    println!(
        "point EXPLAIN={:?}; runtime={:?}",
        db.explain_inode(a, 2).unwrap(),
        kind(db.diagnostics(), StatementKind::Inode)
    );
}

#[test]
fn local_publication_survives_lost_reply_and_capture_does_not_copy_cells() {
    let temp = Temp::new();
    let db = Overlay::create(&temp.db(), ProfileConfig::default()).unwrap();
    let w = db.open_workspace([3; 32], [4; 32]).unwrap();
    let published = db.publish(w, &inode(2), None, Some(&cell(11))).unwrap();
    assert!(matches!(
        db.capture(w),
        Err(OverlayError::ReplyAttemptsPending)
    ));
    assert_eq!(db.inode(w, 2).unwrap(), Some(inode(2)));
    // A failed transport reply still constitutes a send attempt, not rollback.
    db.reply_attempted(published).unwrap();
    let before = kind(db.diagnostics(), StatementKind::Payload);
    let captured = db.capture(w).unwrap();
    assert_eq!(kind(db.diagnostics(), StatementKind::Payload), before);
    let mut later = inode(2);
    later.mtime_seconds = 22;
    let active = db.publish(w, &later, None, Some(&cell(22))).unwrap();
    db.reply_attempted(active).unwrap();
    assert_eq!(db.captured_inodes(captured, 0).unwrap(), vec![inode(2)]);
    assert_eq!(db.inode(w, 2).unwrap(), Some(later));
    assert_eq!(
        db.cell(w, 2, captured.generation, 0).unwrap(),
        Some(cell(11))
    );
    assert_eq!(db.cell(w, 2, active.generation, 0).unwrap(), Some(cell(22)));
    assert!(matches!(
        db.reply_attempted(published),
        Err(OverlayError::Stale)
    ));
    assert!(matches!(db.capture(w), Err(OverlayError::CaptureInFlight)));
}

#[test]
fn fixed_capture_uses_generation_index_while_active_namespace_grows() {
    let temp = Temp::new();
    let db = Overlay::create(&temp.db(), ProfileConfig::default()).unwrap();
    // Distinct count-driven cases, no latency/cold-cache or performance verdict.
    let mut correct_plan = true;
    for (tag, population) in [(10, 128), (11, 1024), (12, 4096)] {
        let w = db.open_workspace([tag; 32], [20; 32]).unwrap();
        let p = db.publish(w, &inode(2), None, None).unwrap();
        db.reply_attempted(p).unwrap();
        let c = db.capture(w).unwrap();
        for serial in 3..population + 3 {
            let p = db.publish(w, &inode(serial), None, None).unwrap();
            db.reply_attempted(p).unwrap();
        }
        let plan = db.explain_capture(c).unwrap();
        println!("capture plan before verification: {plan:?}");
        correct_plan &= plan
            .iter()
            .any(|p| p.contains("inode_capture") && p.contains("SEARCH"));
        let before = kind(db.diagnostics(), StatementKind::Capture);
        assert_eq!(db.captured_inodes(c, 0).unwrap(), vec![inode(2)]);
        let after = kind(db.diagnostics(), StatementKind::Capture);
        assert_eq!(after.attempts - before.attempts, 1);
        assert_eq!(after.rows_returned - before.rows_returned, 1);
        assert_eq!(after.fullscan_steps - before.fullscan_steps, 0);
        assert_eq!(after.sorts - before.sorts, 0);
        assert_eq!(after.reprepares - before.reprepares, 0);
        assert!(after.vm_steps > before.vm_steps);
        assert!(db.captured_inodes(c, 2).unwrap().is_empty());
        println!("active_population={population}; plan={plan:?}; captured_rows=1; vm_steps={}; fullscan_steps=0; sorts=0; reprepare=0",after.vm_steps-before.vm_steps);
    }
    assert!(
        correct_plan,
        "capture must use its generation-selective index"
    );
}

#[test]
fn backed_owner_and_scratch_pages_have_no_total_record_limit() {
    let temp = Temp::new();
    let db = Overlay::create(&temp.db(), ProfileConfig::default()).unwrap();
    let a = db.open_workspace([21; 32], [22; 32]).unwrap();
    let b = db.open_workspace([23; 32], [22; 32]).unwrap();
    let lease = Lease {
        kind: LeaseKind::Operation,
        owner: 1,
        resource: 0,
    };
    db.acquire(a, lease).unwrap();
    assert!(db.acquire(a, lease).is_err());
    let record = ScratchRecord {
        kind: 1,
        key: 0,
        value: vec![0, 255, 0],
    };
    assert!(matches!(
        db.put_scratch(b, 1, &record),
        Err(OverlayError::Missing)
    ));
    for key in 0..193 {
        db.put_scratch(
            a,
            1,
            &ScratchRecord {
                key,
                ..record.clone()
            },
        )
        .unwrap();
    }
    let mut after = None;
    let mut count = 0;
    loop {
        let page = db.scratch_page(a, 1, 1, after).unwrap();
        if page.is_empty() {
            break;
        }
        assert!(page.len() <= PAGE_ROWS);
        for item in &page {
            assert_eq!(item.value, record.value);
            assert_eq!(item.key, count);
            count += 1;
        }
        after = Some(page.last().unwrap().key);
    }
    assert_eq!(count, 193);
    db.release(a, lease).unwrap();
    assert!(matches!(
        db.scratch_page(a, 1, 1, None),
        Err(OverlayError::Missing)
    ));
    assert!(matches!(db.release(a, lease), Err(OverlayError::Missing)));
}

#[test]
fn portable_time_range_and_directory_sticky_mode_are_preserved() {
    let temp = Temp::new();
    let db = Overlay::create(&temp.db(), ProfileConfig::default()).unwrap();
    let w = db.open_workspace([77; 32], [78; 32]).unwrap();
    let mut value = inode(8);
    value.kind = InodeKind::Directory;
    value.mode = 0o1777;
    value.mtime_seconds = i64::MAX;
    value.mtime_nanoseconds = 999_999_999;
    let publication = db.publish(w, &value, None, None).unwrap();
    db.reply_attempted(publication).unwrap();
    assert_eq!(db.inode(w, 8).unwrap(), Some(value.clone()));
    value.mtime_seconds = i64::MIN;
    let publication = db.publish(w, &value, None, None).unwrap();
    db.reply_attempted(publication).unwrap();
    assert_eq!(db.inode(w, 8).unwrap(), Some(value.clone()));
    value.kind = InodeKind::File;
    assert!(matches!(
        db.publish(w, &value, None, None),
        Err(OverlayError::Invalid(_))
    ));
    value.mode = 0o644;
    value.mtime_nanoseconds = 1_000_000_000;
    assert!(matches!(
        db.publish(w, &value, None, None),
        Err(OverlayError::Invalid(_))
    ));
    assert_eq!(db.inode(w, 8).unwrap().unwrap().kind, InodeKind::Directory);
}
