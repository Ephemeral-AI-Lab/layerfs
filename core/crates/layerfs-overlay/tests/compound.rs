//! Public compound namespace-job proofs: atomic finals, lower-row whiteouts
//! and paired plan/runtime evidence. No private source or product fault hooks.
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
            "layerfs-compound-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn db(&self) -> Overlay {
        Overlay::create(&self.0.join("overlay.sqlite"), ProfileConfig::default()).unwrap()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn file(serial: u64, born: u64) -> Inode {
    Inode {
        serial,
        kind: InodeKind::File,
        mode: 0o644,
        mtime_seconds: -3,
        mtime_nanoseconds: 999_999_999,
        nlink: 1,
        size: 0,
        inherited_cutoff: 0,
        born,
        entries: 0,
    }
}
fn directory(serial: u64, entries: u64) -> Inode {
    Inode {
        kind: InodeKind::Directory,
        mode: 0o1755,
        entries,
        ..file(serial, 0)
    }
}
fn bound(parent: u64, name: &[u8], serial: u64) -> NameChange {
    NameChange {
        parent,
        name: name.to_vec(),
        binding: Binding::Bound(serial),
    }
}
fn removed(parent: u64, name: &[u8], inherited: bool) -> NameChange {
    NameChange {
        parent,
        name: name.to_vec(),
        binding: Binding::Removed { inherited },
    }
}
fn names(names: Vec<NameChange>) -> Changes {
    Changes {
        names,
        ..Changes::default()
    }
}
fn settle(db: &Overlay, publication: Publication) {
    db.reply_attempted(publication).unwrap();
}

#[test]
fn compound_job_publishes_every_final_value_with_one_ticket_or_nothing() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([1; 32], [2; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let active = db.source_rows(source).unwrap().active().number() as u64;
    let mut data = Box::new([0_u8; CELL_BYTES]);
    data[..3].copy_from_slice(b"a\0\xff");
    let mut validity = Box::new([0_u8; MASK_BYTES]);
    validity[0] = 0b111;
    let cell = Cell {
        offset: 0,
        data,
        validity,
    };
    let changes = Changes {
        inodes: vec![directory(10, 1), file(20, active)],
        names: vec![bound(10, b"bin\xff\0name", 20)],
        cell: Some((20, cell.clone())),
    };
    let publication = db.apply(source, &changes).unwrap();
    let state = db.state(route).unwrap();
    assert_eq!(
        (state.revision, state.dirty_inodes, state.dirty_names),
        (1, 2, 1)
    );
    assert_eq!(
        db.pending_publications(route, 0).unwrap(),
        vec![publication]
    );
    let rows = db.source_rows(source).unwrap();
    assert_eq!(rows.inode(10).unwrap(), Some(directory(10, 1)));
    assert_eq!(rows.inode(20).unwrap(), Some(file(20, active)));
    assert_eq!(rows.inode(30).unwrap(), None);
    assert_eq!(
        rows.name(10, b"bin\xff\0name").unwrap(),
        NameLayers {
            active: Some(Some(20)),
            lower: None
        }
    );
    assert_eq!(db.source_cell(source, 20, active, 0).unwrap(), Some(cell));
    assert!(source.created_above(active));
    assert!(!source.created_above(0));

    // A value refused inside the transaction, after an earlier inode write,
    // leaves no inode, name, ticket, revision or dirty-count change.
    let refused = Changes {
        inodes: vec![file(30, active), file(31, active + 1)],
        names: vec![bound(10, b"later", 30)],
        cell: None,
    };
    assert!(matches!(
        db.apply(source, &refused),
        Err(OverlayError::Invalid("inode creation generation"))
    ));
    assert_eq!(db.state(route).unwrap(), state);
    assert_eq!(db.source_inode(source, 30).unwrap(), None);
    assert_eq!(
        db.source_rows(source)
            .unwrap()
            .name(10, b"later")
            .unwrap()
            .active,
        None
    );
    assert_eq!(db.pending_publications(route, 0).unwrap().len(), 1);

    // Window, duplicate-key and ownership refusals precede any transaction.
    let before = db.diagnostics();
    for invalid in [
        Changes::default(),
        Changes {
            inodes: (40..45).map(|serial| file(serial, 0)).collect(),
            ..Changes::default()
        },
        Changes {
            inodes: vec![file(40, 0), file(40, 0)],
            ..Changes::default()
        },
        names(vec![
            bound(10, b"a", 40),
            bound(10, b"b", 40),
            bound(10, b"c", 40),
        ]),
        names(vec![bound(10, b"a", 40), removed(10, b"a", false)]),
        names(vec![bound(10, b"a", 0)]),
        names(vec![bound(0, b"a", 40)]),
        names(vec![bound(10, b"", 40)]),
        names(vec![bound(10, &[b'n'; 256], 40)]),
        Changes {
            inodes: vec![Inode {
                entries: 1,
                ..file(40, 0)
            }],
            ..Changes::default()
        },
        Changes {
            inodes: vec![file(40, 0)],
            cell: Some((
                41,
                Cell {
                    offset: 0,
                    data: Box::new([0; CELL_BYTES]),
                    validity: Box::new([0; MASK_BYTES]),
                },
            )),
            ..Changes::default()
        },
    ] {
        assert!(matches!(
            db.apply(source, &invalid),
            Err(OverlayError::Invalid(_))
        ));
    }
    let after = db.diagnostics();
    assert_eq!(
        after.statements[StatementKind::Begin as usize],
        before.statements[StatementKind::Begin as usize]
    );

    // Logical close refuses new mutation while existing source reads remain.
    settle(&db, publication);
    db.close(route).unwrap();
    assert!(matches!(
        db.apply(source, &names(vec![bound(10, b"closed", 20)])),
        Err(OverlayError::Closed)
    ));
    assert_eq!(db.source_inode(source, 20).unwrap(), Some(file(20, active)));
    db.release_base_source(source).unwrap();
    assert!(matches!(
        db.apply(source, &names(vec![bound(10, b"stale", 20)])),
        Err(OverlayError::Stale)
    ));
    assert!(matches!(db.source_rows(source), Err(OverlayError::Stale)));
}

#[test]
fn removed_names_keep_a_whiteout_only_where_a_lower_binding_exists() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([3; 32], [4; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let layers = |name: &[u8]| db.source_rows(source).unwrap().name(1, name).unwrap();
    let apply = |change: NameChange| settle(&db, db.apply(source, &names(vec![change])).unwrap());

    // Created and removed inside one generation, with nothing below: no row.
    apply(bound(1, b"temporary", 20));
    assert_eq!(db.state(route).unwrap().dirty_names, 1);
    apply(removed(1, b"temporary", false));
    assert_eq!(
        layers(b"temporary"),
        NameLayers {
            active: None,
            lower: None
        }
    );
    assert_eq!(db.state(route).unwrap().dirty_names, 0);
    // Removing an absent local name over nothing is also row-free.
    apply(removed(1, b"temporary", false));
    assert_eq!(db.state(route).unwrap().dirty_names, 0);

    // An inherited base binding needs a whiteout, replaced in place afterwards.
    apply(removed(1, b"inherited", true));
    apply(removed(1, b"inherited", true));
    assert_eq!(layers(b"inherited").active, Some(None));
    assert_eq!(db.state(route).unwrap().dirty_names, 1);
    apply(bound(1, b"captured", 21));
    assert_eq!(db.state(route).unwrap().dirty_names, 2);

    // Sealed rows are never rewritten; the next generation shadows them.
    let capture = db.capture(route).unwrap();
    assert_eq!(db.state(route).unwrap().dirty_names, 0);
    apply(removed(1, b"captured", false));
    assert_eq!(
        layers(b"captured"),
        NameLayers {
            active: Some(None),
            lower: Some(Some(21))
        }
    );
    // A lower whiteout already hides the base name: rebinding then removing
    // returns to no active row even though the base fact says inherited.
    apply(bound(1, b"inherited", 22));
    assert_eq!(
        layers(b"inherited"),
        NameLayers {
            active: Some(Some(22)),
            lower: Some(None)
        }
    );
    apply(removed(1, b"inherited", true));
    assert_eq!(
        layers(b"inherited"),
        NameLayers {
            active: None,
            lower: Some(None)
        }
    );
    assert_eq!(db.state(route).unwrap().dirty_names, 1);
    let sealed: Vec<_> = db
        .captured_dentries(capture, None)
        .unwrap()
        .into_iter()
        .map(|row| (row.name, row.serial))
        .collect();
    assert_eq!(
        sealed,
        vec![
            (b"captured".to_vec(), Some(21)),
            (b"inherited".to_vec(), None)
        ]
    );

    // Known install folds the sealed generation into the base: its rows leave
    // the view, so the same removal now depends on the new base fact alone.
    db.release_base_source(source).unwrap();
    db.install(capture, [5; 32]).unwrap();
    let source = db.acquire_base_source(route, 2).unwrap();
    assert_eq!(
        db.source_rows(source)
            .unwrap()
            .name(1, b"captured")
            .unwrap(),
        NameLayers {
            active: Some(None),
            lower: None
        }
    );
    settle(
        &db,
        db.apply(source, &names(vec![removed(1, b"captured", false)]))
            .unwrap(),
    );
    assert_eq!(
        db.source_rows(source)
            .unwrap()
            .name(1, b"captured")
            .unwrap(),
        NameLayers {
            active: None,
            lower: None
        }
    );
}

#[test]
fn compound_statements_keep_point_work_as_the_namespace_grows() {
    let temp = Temp::new();
    let db = temp.db();
    let route = db.open_workspace([6; 32], [7; 32]).unwrap();
    let other = db.open_workspace([8; 32], [9; 32]).unwrap();
    let source = db.acquire_base_source(route, 1).unwrap();
    let plans = db.explain_compound(source).unwrap();
    assert_eq!(plans.len(), 6);
    for plan in &plans[..4] {
        assert!(
            plan.contains("SEARCH") && !plan.contains("SCAN") && !plan.contains("TEMP"),
            "{plan}"
        );
    }
    for plan in &plans[4..] {
        assert!(plan.ends_with("btree-scan-opcodes=0"), "{plan}");
    }
    // Both parents already have active rows, so every scale replaces the same
    // row shapes: the comparison isolates growth from insert-versus-update.
    settle(
        &db,
        db.apply(
            source,
            &Changes {
                inodes: vec![directory(1, 0), directory(2, 0)],
                ..Changes::default()
            },
        )
        .unwrap(),
    );
    let mut filled = 0_u64;
    let mut totals = Vec::new();
    for count in [128_u64, 1024, 4096] {
        while filled < count {
            // Same-directory siblings, unrelated inodes and another namespace.
            let serial = 1000 + filled;
            for target in [route, other] {
                let publication = db
                    .publish(
                        target,
                        &file(serial, 0),
                        Some(&Dentry {
                            parent: 1,
                            name: format!("sibling-{filled:05}").into_bytes(),
                            serial: Some(serial),
                        }),
                        None,
                    )
                    .unwrap();
                settle(&db, publication);
            }
            filled += 1;
        }
        // One complete replacing-rename-shaped job: its consistent reads and
        // the single atomic publication of three inodes and two names.
        let from = format!("from-{count}").into_bytes();
        let to = format!("to-{count}").into_bytes();
        settle(
            &db,
            db.apply(
                source,
                &Changes {
                    inodes: vec![file(count, 0), file(count + 1, 0)],
                    names: vec![bound(1, &from, count), bound(1, &to, count + 1)],
                    cell: None,
                },
            )
            .unwrap(),
        );
        let before = db.diagnostics();
        let rows = db.source_rows(source).unwrap();
        for serial in [1, 2, count, count + 1] {
            rows.inode(serial).unwrap();
        }
        assert_eq!(rows.name(1, &from).unwrap().active, Some(Some(count)));
        assert_eq!(rows.name(1, &to).unwrap().active, Some(Some(count + 1)));
        let publication = db
            .apply(
                source,
                &Changes {
                    inodes: vec![
                        directory(1, count),
                        directory(2, 1),
                        Inode {
                            nlink: 0,
                            ..file(count + 1, 0)
                        },
                    ],
                    names: vec![removed(1, &from, false), bound(1, &to, count)],
                    cell: None,
                },
            )
            .unwrap();
        let after = db.diagnostics();
        settle(&db, publication);
        let mut vm = 0;
        let mut runs = 0;
        let mut changed = 0;
        for (index, (b, a)) in before.statements.iter().zip(&after.statements).enumerate() {
            assert_eq!(a.fullscan_steps, b.fullscan_steps, "family {index}");
            assert_eq!(a.sorts, b.sorts, "family {index}");
            assert_eq!(a.autoindex_rows, b.autoindex_rows, "family {index}");
            assert_eq!(a.reprepares, b.reprepares, "family {index}");
            vm += a.vm_steps - b.vm_steps;
            runs += a.executions - b.executions;
            changed += a.rows_changed - b.rows_changed;
        }
        println!(
            "S4_COMPOUND siblings={count} complete_job_vm={vm} statements={runs} rows_changed={changed} fullscan=0 sorts=0 autoindex=0 reprepare=0 plans={plans:?}"
        );
        totals.push((vm, runs, changed));
    }
    assert!(
        totals.windows(2).all(|pair| pair[0] == pair[1]),
        "work must not grow with unrelated rows: {totals:?}"
    );
    // Three inodes, one dropped name, one rebound name, one ticket, one state row.
    assert_eq!(totals[0].2, 7);
}
