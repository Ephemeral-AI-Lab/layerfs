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
        born: 0,
        entries: 0,
        subdirs: 0,
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
fn terminal_cleanup_waits_for_exact_owners_and_reply_attempts() {
    let temp = Temp::new();
    let db = Overlay::create(&temp.db(), ProfileConfig::default()).unwrap();
    let route = db.open_workspace([101; 32], [9; 32]).unwrap();
    let other = db.open_workspace([102; 32], [9; 32]).unwrap();
    let lease = Lease {
        kind: LeaseKind::Open,
        owner: 1,
        resource: 7,
    };
    db.acquire(route, lease).unwrap();
    let publication = db.publish(route, &inode(7), None, Some(&cell(9))).unwrap();
    db.close(route).unwrap();
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Held);
    assert!(db.reclaim_closed(0).unwrap().is_none());
    assert!(matches!(
        db.publish(route, &inode(8), None, None),
        Err(OverlayError::Closed)
    ));
    db.release(route, lease).unwrap();
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Held);
    // Closed state still accepts the exact earlier send-attempt ticket.
    db.reply_attempted(publication).unwrap();
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Queued);
    let plans = db.explain_closed_reclaim().unwrap();
    assert!(
        plans
            .iter()
            .all(|line| !line.contains("SCAN") && !line.contains("TEMP B-TREE")),
        "{plans:?}"
    );
    let mut steps = 0;
    while let Some(step) = db.reclaim_closed(0).unwrap() {
        assert!(step.rows <= 64);
        assert!(step.data_bytes <= 65536);
        steps += 1;
    }
    assert!(steps > 0);
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Gone);
    assert!(!db.state(other).unwrap().closed);
}

#[test]
fn closed_capture_retains_its_existing_rows_until_explicit_known_release() {
    let temp = Temp::new();
    let db = Overlay::create(&temp.db(), ProfileConfig::default()).unwrap();
    let route = db.open_workspace([103; 32], [9; 32]).unwrap();
    let published = db.publish(route, &inode(1), None, None).unwrap();
    db.reply_attempted(published).unwrap();
    let capture = db.capture(route).unwrap();
    db.close(route).unwrap();
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Held);
    assert!(db.reclaim_closed(0).unwrap().is_none());
    assert_eq!(db.captured_inodes(capture, 0).unwrap(), [inode(1)]);
    db.release_closed_capture(capture).unwrap();
    assert!(matches!(
        db.release_closed_capture(capture),
        Err(OverlayError::Stale)
    ));
    while db.reclaim_closed(0).unwrap().is_some() {}
    assert_eq!(db.cleanup_state(route).unwrap(), CleanupState::Gone);
}

#[test]
fn reclaim_pages_do_not_visit_a_large_held_namespace_or_copy_operation_record_values() {
    let temp = Temp::new();
    let db = Overlay::create(&temp.db(), ProfileConfig::default()).unwrap();
    let held = db.open_workspace([104; 32], [9; 32]).unwrap();
    let ready = db.open_workspace([105; 32], [9; 32]).unwrap();
    db.acquire(
        held,
        Lease {
            kind: LeaseKind::Reader,
            owner: 1,
            resource: 0,
        },
    )
    .unwrap();
    for serial in 1..=1024 {
        let p = db.publish(held, &inode(serial), None, None).unwrap();
        db.reply_attempted(p).unwrap();
    }
    db.close(held).unwrap();
    let op = Lease {
        kind: LeaseKind::Operation,
        owner: 8,
        resource: 0,
    };
    db.acquire(ready, op).unwrap();
    for key in 0..3 {
        db.put_operation_record(
            ready,
            8,
            &OperationRecord {
                kind: 0,
                key,
                value: vec![7; 65536],
            },
        )
        .unwrap();
    }
    db.release(ready, op).unwrap();
    db.close(ready).unwrap();
    let mut operation_record_windows = 0;
    loop {
        let before = kind(db.diagnostics(), StatementKind::Reclaim);
        let Some(step) = db.reclaim_closed(0).unwrap() else {
            break;
        };
        let after = kind(db.diagnostics(), StatementKind::Reclaim);
        assert_eq!(step.namespace, ready.namespace() as u64);
        assert_eq!(after.fullscan_steps - before.fullscan_steps, 0);
        assert_eq!(after.sorts - before.sorts, 0);
        assert!(step.rows <= 64 && step.data_bytes <= 65536);
        if step.data_bytes != 0 {
            assert_eq!(step.rows, 1);
            operation_record_windows += 1;
        }
        println!(
            "CLOSE_RECLAIM step={step:?} vm={} returned={} fullscan={} sorts={}",
            after.vm_steps - before.vm_steps,
            after.rows_returned - before.rows_returned,
            after.fullscan_steps - before.fullscan_steps,
            after.sorts - before.sorts
        );
    }
    assert_eq!(operation_record_windows, 3);
    assert_eq!(db.cleanup_state(held).unwrap(), CleanupState::Held);
    println!(
        "CLOSE_RECLAIM plans={:?}",
        db.explain_closed_reclaim().unwrap()
    );
}

#[test]
fn profile_namespace_binary_values_and_atomic_refusals() {
    let temp = Temp::new();
    let db = Overlay::create(&temp.db(), ProfileConfig::default()).unwrap();
    assert!(Overlay::create(&temp.db(), ProfileConfig::default()).is_err());
    let p = db.profile();
    assert_eq!(p.schema_version, 23);
    assert_eq!(p.max_pages, i64::from(u32::MAX - 1));
    assert_eq!(p.explicit_page_quota, None);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(temp.db()).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
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
            Some(&DirectoryEntry {
                inherited: false,
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
        db.directory_entry(a, 1, b"ignored.bin")
            .unwrap()
            .unwrap()
            .serial,
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
fn payload_name_operation_record_and_owner_access_keep_indexed_scope() {
    let temp = Temp::new();
    let db = Overlay::create(&temp.db(), ProfileConfig::default()).unwrap();
    let a = db.open_workspace([121; 32], [9; 32]).unwrap();
    let b = db.open_workspace([122; 32], [9; 32]).unwrap();
    let own = Lease {
        kind: LeaseKind::Operation,
        owner: 7,
        resource: 0,
    };
    db.acquire(a, own).unwrap();
    let name = b"binary-index";
    let p = db
        .publish(
            a,
            &inode(7),
            Some(&DirectoryEntry {
                inherited: false,
                parent: 1,
                name: name.to_vec(),
                serial: Some(7),
            }),
            Some(&cell(0)),
        )
        .unwrap();
    db.reply_attempted(p).unwrap();
    db.put_operation_record(
        a,
        7,
        &OperationRecord {
            kind: 4,
            key: 0,
            value: vec![0, 255, 0, 61],
        },
    )
    .unwrap();
    let plans = [
        db.explain_cell(a, 7, p.generation, 0).unwrap(),
        db.explain_directory_entry(a, 1, name).unwrap(),
        db.explain_operation_record(a, 7, 4, None).unwrap(),
        db.explain_lease(a, own).unwrap(),
    ];
    for plan in &plans {
        assert!(
            plan.iter()
                .all(|row| row.contains("SEARCH") && !row.contains("TEMP B-TREE")),
            "{plan:?}"
        );
    }
    let mut previous = 0;
    for population in [128, 1024, 4096] {
        for serial in previous + 1..=population {
            let other = db
                .publish(
                    b,
                    &inode(serial),
                    Some(&DirectoryEntry {
                        inherited: false,
                        parent: 1,
                        name: format!("n{serial:04}").into_bytes(),
                        serial: Some(serial),
                    }),
                    Some(&cell(255)),
                )
                .unwrap();
            db.reply_attempted(other).unwrap();
            db.acquire(
                b,
                Lease {
                    kind: LeaseKind::Reader,
                    owner: serial,
                    resource: serial,
                },
            )
            .unwrap();
        }
        previous = population;
        let before = db.diagnostics();
        assert_eq!(db.cell(a, 7, p.generation, 0).unwrap(), Some(cell(0)));
        assert_eq!(
            db.directory_entry(a, 1, name).unwrap().unwrap().serial,
            Some(7)
        );
        assert_eq!(
            db.operation_record_page(a, 7, 4, None).unwrap()[0].value,
            [0, 255, 0, 61]
        );
        assert!(db.lease_exists(a, own).unwrap());
        assert!(!db.lease_exists(b, own).unwrap());
        let after = db.diagnostics();
        for family in [
            StatementKind::Payload,
            StatementKind::DirectoryEntry,
            StatementKind::OperationRecord,
            StatementKind::Lease,
        ] {
            let start = kind(before, family);
            let end = kind(after, family);
            assert_eq!(end.fullscan_steps - start.fullscan_steps, 0);
            assert_eq!(end.sorts - start.sorts, 0);
            assert_eq!(end.reprepares - start.reprepares, 0);
            assert!(
                end.vm_steps - start.vm_steps < 128,
                "{family:?} amplification"
            );
            println!("S1_ACCESS other_rows={population} family={family:?} vm={} returned={} runs={} bytes={} fullscan={} sorts={} autoindex={} reprepare={} plans={plans:?}",end.vm_steps-start.vm_steps,end.rows_returned-start.rows_returned,end.executions-start.executions,end.bound_bytes-start.bound_bytes,end.fullscan_steps-start.fullscan_steps,end.sorts-start.sorts,end.autoindex_rows-start.autoindex_rows,end.reprepares-start.reprepares);
        }
    }
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
    // A row first written in a new generation is a new payload layer: bytes it
    // does not write fall through to the sealed layer below, up to its length.
    later.inherited_cutoff = later.size;
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
fn backed_owner_and_operation_record_pages_have_no_total_record_limit() {
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
    let record = OperationRecord {
        kind: 1,
        key: 0,
        value: vec![0, 255, 0],
    };
    assert!(matches!(
        db.put_operation_record(b, 1, &record),
        Err(OverlayError::Missing)
    ));
    for key in 0..193 {
        db.put_operation_record(
            a,
            1,
            &OperationRecord {
                key,
                ..record.clone()
            },
        )
        .unwrap();
    }
    let mut after = None;
    let mut count = 0;
    loop {
        let page = db.operation_record_page(a, 1, 1, after).unwrap();
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
        db.operation_record_page(a, 1, 1, None),
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

#[test]
fn known_install_advances_base_without_replaying_or_losing_later_active_rows() {
    let temp = Temp::new();
    let db = Overlay::create(&temp.db(), ProfileConfig::default()).unwrap();
    let route = db.open_workspace([88; 32], [89; 32]).unwrap();
    for cycle in 0..24 {
        let base_only = db.publish(route, &inode(4), None, None).unwrap();
        db.reply_attempted(base_only).unwrap();
        let captured = db.capture(route).unwrap();
        let mut later = inode(2);
        later.mtime_seconds = cycle;
        let published = db
            .publish(route, &later, None, Some(&cell(cycle as u8)))
            .unwrap();
        let before = kind(db.diagnostics(), StatementKind::Payload);
        let install_before = kind(db.diagnostics(), StatementKind::Capture);
        if cycle == 0 {
            println!(
                "install plan and retirement VM={:?}",
                db.explain_install(captured, [0; 32]).unwrap()
            );
        }
        db.install(captured, [cycle as u8; 32]).unwrap();
        if cycle == 0 {
            let observed = kind(db.diagnostics(), StatementKind::Capture);
            println!(
                "install statements={}; rows_changed={}; vm_steps={}; fullscan_steps={}; sorts={}",
                observed.attempts - install_before.attempts,
                observed.rows_changed - install_before.rows_changed,
                observed.vm_steps - install_before.vm_steps,
                observed.fullscan_steps - install_before.fullscan_steps,
                observed.sorts - install_before.sorts
            );
        }
        assert_eq!(kind(db.diagnostics(), StatementKind::Payload), before);
        let state = db.state(route).unwrap();
        assert_eq!(state.base_root, [cycle as u8; 32]);
        assert_eq!(state.installed, captured.generation.number());
        assert!(state.captured.is_none());
        assert_eq!(
            db.inode(route, 2).unwrap(),
            // The first cycle has no lower local row; afterwards the sealed
            // previous row is the lower view this layer falls through to.
            Some(Inode {
                inherited_cutoff: if cycle == 0 { 0 } else { later.size },
                ..later.clone()
            })
        );
        assert_eq!(
            db.inode(route, 4).unwrap(),
            None,
            "installed-only value now delegates to new base"
        );
        assert_eq!(
            db.cell(route, 2, published.generation, 0).unwrap(),
            Some(cell(cycle as u8))
        );
        assert!(
            !db.capture_ready(route).unwrap(),
            "later pending reply remains owned"
        );
        db.reply_attempted(published).unwrap();
        assert!(db.capture_ready(route).unwrap());
        assert!(matches!(
            db.install(captured, [0; 32]),
            Err(OverlayError::Stale)
        ));
    }
}

#[test]
fn captured_name_keysets_use_fixed_generation_and_do_not_revisit_prefixes() {
    let temp = Temp::new();
    let db = Overlay::create(&temp.db(), ProfileConfig::default()).unwrap();
    let route = db.open_workspace([90; 32], [91; 32]).unwrap();
    for key in 0..193 {
        let name = DirectoryEntry {
            inherited: false,
            parent: 1,
            name: format!("n{key:04}").into_bytes(),
            serial: Some(2),
        };
        let p = db.publish(route, &inode(2), Some(&name), None).unwrap();
        db.reply_attempted(p).unwrap();
    }
    let capture = db.capture(route).unwrap();
    for key in 200..1224 {
        let name = DirectoryEntry {
            inherited: false,
            parent: 1,
            name: format!("n{key:04}").into_bytes(),
            serial: Some(2),
        };
        let p = db.publish(route, &inode(2), Some(&name), None).unwrap();
        db.reply_attempted(p).unwrap();
    }
    let plan = db.explain_directory_entry_capture(capture).unwrap();
    assert!(plan.iter().any(|p| p.contains("directory_entry_capture")));
    let mut after: Option<DirectoryEntry> = None;
    let mut count = 0;
    loop {
        let before = kind(db.diagnostics(), StatementKind::Capture);
        let page = db
            .captured_directory_entries(
                capture,
                after.as_ref().map(|d| (d.parent, d.name.as_slice())),
            )
            .unwrap();
        let observed = kind(db.diagnostics(), StatementKind::Capture);
        println!("directory_entry plan={plan:?}; start={count}; rows={}; vm_steps={}; fullscan_steps={}; sorts={}", page.len(), observed.vm_steps-before.vm_steps, observed.fullscan_steps-before.fullscan_steps, observed.sorts-before.sorts);
        if page.is_empty() {
            break;
        }
        assert!(page.len() <= PAGE_ROWS);
        for name in &page {
            assert_eq!(name.name, format!("n{count:04}").as_bytes());
            count += 1;
        }
        after = page.last().cloned();
    }
    assert_eq!(count, 193);
}

#[test]
fn real_sqlite_full_aborts_one_mutation_without_losing_previous_publication() {
    let temp = Temp::new();
    let db = Overlay::create(
        &temp.db(),
        ProfileConfig {
            pager_kib: 2048,
            // Schema19 itself exceeds64 pages. Leave room for setup
            // while the unchanged64-cell sequence must hit SQLITE_FULL.
            max_pages: Some(96),
        },
    )
    .unwrap();
    let a = db.open_workspace([92; 32], [93; 32]).unwrap();
    let b = db.open_workspace([94; 32], [93; 32]).unwrap();
    let initial = db.publish(a, &inode(2), None, Some(&cell(97))).unwrap();
    db.reply_attempted(initial).unwrap();
    let retained = db.capture(a).unwrap();
    let mut failed = false;
    for index in 0..64 {
        let before = db.state(a).unwrap();
        let prior = db.inode(a, 2).unwrap();
        let mut data = cell(index as u8);
        data.offset = index * CELL_BYTES as u64;
        let mut changed = inode(2);
        changed.size = data.offset + CELL_BYTES as u64;
        match db.publish(a, &changed, None, Some(&data)) {
            Ok(published) => db.reply_attempted(published).unwrap(),
            Err(error) => {
                assert!(
                    matches!(&error, OverlayError::Sql(rusqlite::Error::SqliteFailure(code, _))
                        if code.code == rusqlite::ErrorCode::DiskFull),
                    "original database failure: {error:?}"
                );
                assert_eq!(db.state(a).unwrap(), before);
                assert_eq!(db.retained_capture(a).unwrap(), Some(retained));
                assert_eq!(db.captured_cell(retained, 2, 0).unwrap(), Some(cell(97)));
                assert_eq!(db.inode(a, 2).unwrap(), prior);
                assert!(db.cell(a, 2, before.active, data.offset).unwrap().is_none());
                assert_eq!(db.state(b).unwrap().revision, 0);
                println!(
                    "one-attempt physical quota failure at cell {index}: {error:?}; pages={:?}",
                    db.pages().unwrap()
                );
                failed = true;
                break;
            }
        }
    }
    assert!(failed, "configured real page capacity must be exercised");
}
