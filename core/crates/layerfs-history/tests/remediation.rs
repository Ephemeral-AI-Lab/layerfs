//! Regressions for the independently reproduced pair-2 boundary failures.
mod support;
use layerfs_history::*;
use support::*;

fn init(c: &dyn HistoryCatalog, n: u8, text: &str) -> LayerStackRecord {
    c.initialize_layerstack(&StackInitialization {
        stack: stack(n),
        name: name(text),
        scope: root(200),
        profile: root(201),
        genesis_root: root(0),
    })
    .unwrap()
}
fn fork(c: &dyn HistoryCatalog, s: &LayerStackRecord, n: u8, base: LayerId) -> BranchSnapshot {
    c.fork(&ForkRequest {
        stack: s.id,
        branch: branch_id(n),
        name: name(&format!("b{n}")),
        source: ForkSource::Layer(base),
    })
    .unwrap()
}
fn append(c: &dyn HistoryCatalog, b: BranchId, n: u8) -> CommitRecord {
    let s = c.branch_snapshot(b).unwrap().unwrap();
    let stage = c
        .stage_changes(&StageRequest {
            workspace: workspace(n.max(1)),
            branch: b,
            expected_head: s.branch.head_commit,
            expected_base: s.branch.base_layer,
            expected_root: s.effective_root,
            construction_base_root: s.effective_root,
            intended_commit_base: s.branch.base_layer,
            candidate_root: root(n),
            profile: s.profile,
            scope: s.scope,
            generation: 1,
        })
        .unwrap();
    match c
        .commit_staged(&CommitStagedRequest {
            workspace: stage.workspace,
            token: stage.token,
        })
        .unwrap()
    {
        CommitStagedOutcome::Committed(c) => c,
        _ => panic!("new commit expected"),
    }
}
fn publish(
    c: &dyn HistoryCatalog,
    s: &LayerStackRecord,
    b: BranchId,
    k: &CommitRecord,
    head: LayerId,
) -> Result<AddLayerOutcome, HistoryError> {
    c.add_layer(&AddLayerRequest {
        stack: s.id,
        branch: b,
        commit: k.id,
        expected_stack_head: head,
        expected_branch_base: k.base_layer,
    })
}

#[test]
fn all_legal_name_widths_paginate_by_identity() {
    let t = Temp::new("names");
    let c = create(&t.join("db"));
    let names: Vec<_> = [1, 33, 34, 63, 1]
        .into_iter()
        .enumerate()
        .map(|(i, len)| char::from(b'a' + i as u8).to_string().repeat(len))
        .collect();
    let mut stacks = vec![];
    for (i, name) in names.iter().enumerate() {
        stacks.push(init(&c, i as u8 + 1, name));
    }
    for (i, name) in names.iter().enumerate() {
        c.fork(&ForkRequest {
            stack: stacks[0].id,
            branch: branch_id(i as u8 + 1),
            name: support::name(name),
            source: ForkSource::Layer(stacks[0].head_layer),
        })
        .unwrap();
    }
    for branches in [false, true] {
        let mut cursor = None;
        let mut seen = vec![];
        loop {
            let page = Page { cursor, limit: 1 };
            let next = if branches {
                let p = c.branches(stacks[0].id, &page).unwrap();
                seen.extend(p.records.iter().map(|r| r.name.as_str().to_owned()));
                p.continuation
            } else {
                let p = c.layer_stacks(&page).unwrap();
                seen.extend(p.records.iter().map(|r| r.name.as_str().to_owned()));
                p.continuation
            };
            match next {
                Some(value) => {
                    assert_eq!(value.len(), 160);
                    cursor = Some(value)
                }
                None => break,
            }
        }
        assert_eq!(seen, names);
    }
}

#[test]
fn advancing_the_actual_subject_preserves_both_anchor_forms() {
    let t = Temp::new("growth");
    let c = create(&t.join("db"));
    let s = init(&c, 1, "main");
    let b = fork(&c, &s, 1, s.head_layer);
    let k1 = append(&c, b.branch.id, 1);
    let k2 = append(&c, b.branch.id, 2);
    let p = c
        .commit_history(&CommitHistoryRequest {
            branch: b.branch.id,
            start: None,
            cursor: None,
            limit: 1,
        })
        .unwrap();
    let k3 = append(&c, b.branch.id, 3);
    for start in [None, Some(k2.id)] {
        let rest = c
            .commit_history(&CommitHistoryRequest {
                branch: b.branch.id,
                start,
                cursor: p.continuation.clone(),
                limit: 1,
            })
            .unwrap();
        assert_eq!(rest.records, vec![k1.clone()]);
        assert!(rest.continuation.is_none());
    }
    let l1 = match publish(&c, &s, b.branch.id, &k3, s.head_layer).unwrap() {
        AddLayerOutcome::Added(l) => l,
        _ => panic!(),
    };
    let p = c
        .layer_history(&LayerHistoryRequest {
            stack: s.id,
            start: None,
            cursor: None,
            limit: 1,
        })
        .unwrap();
    let b2 = fork(&c, &s, 2, l1.id);
    let k4 = append(&c, b2.branch.id, 4);
    let l2 = publish(&c, &s, b2.branch.id, &k4, l1.id).unwrap();
    for start in [None, Some(l1.id)] {
        let rest = c
            .layer_history(&LayerHistoryRequest {
                stack: s.id,
                start,
                cursor: p.continuation.clone(),
                limit: 1,
            })
            .unwrap();
        assert_eq!(rest.records[0].id, s.head_layer);
        assert!(rest.continuation.is_none());
    }
    assert!(matches!(l2, AddLayerOutcome::Added(_)));
    assert_eq!(
        publish(&c, &s, b.branch.id, &k3, s.head_layer).unwrap(),
        AddLayerOutcome::UpToDate { layer: l1.id }
    );
}

#[test]
fn forged_sibling_position_and_obsolete_checksum_are_refused() {
    let t = Temp::new("forge");
    let c = create(&t.join("db"));
    let s = init(&c, 1, "main");
    let a = fork(&c, &s, 1, s.head_layer);
    let b = fork(&c, &s, 2, s.head_layer);
    append(&c, a.branch.id, 1);
    let a2 = append(&c, a.branch.id, 2);
    append(&c, b.branch.id, 3);
    let b2 = append(&c, b.branch.id, 4);
    let cursor = c
        .commit_history(&CommitHistoryRequest {
            branch: a.branch.id,
            start: Some(a2.id),
            cursor: None,
            limit: 1,
        })
        .unwrap()
        .continuation
        .unwrap();
    for domain in [
        b"layerfs/history/cursor/v1\0",
        b"layerfs/history/cursor/v2\0",
    ] {
        let mut forged = cursor.clone();
        forged[110] = 33;
        forged[111..144].copy_from_slice(b2.id.as_slice());
        let mut h = blake3::Hasher::new();
        h.update(domain);
        h.update(&forged[..144]);
        forged[144..].copy_from_slice(&h.finalize().as_bytes()[..16]);
        assert_eq!(
            c.commit_history(&CommitHistoryRequest {
                branch: a.branch.id,
                start: Some(a2.id),
                cursor: Some(forged),
                limit: 1,
            })
            .unwrap_err(),
            HistoryError::Integrity("page cursor digest")
        );
    }
    for offset in [0, 1, 2, 34, 43, 77, 111, 159] {
        let mut changed = cursor.clone();
        changed[offset] ^= 1;
        assert!(c
            .commit_history(&CommitHistoryRequest {
                branch: a.branch.id,
                start: None,
                cursor: Some(changed),
                limit: 1
            })
            .is_err());
    }
    assert!(c
        .commit_history(&CommitHistoryRequest {
            branch: b.branch.id,
            start: None,
            cursor: Some(cursor.clone()),
            limit: 1
        })
        .is_err());
    assert!(c
        .commit_history(&CommitHistoryRequest {
            branch: a.branch.id,
            start: Some(b2.id),
            cursor: Some(cursor),
            limit: 1
        })
        .is_err());
}

#[test]
fn read_only_cursor_capability_survives_reopen_and_rejects_wrong_keys() {
    let t = Temp::new("capability");
    let path = t.join("db");
    let c = create(&path);
    init(&c, 1, "a");
    init(&c, 2, "b");
    let cursor = c
        .layer_stacks(&Page {
            cursor: None,
            limit: 1,
        })
        .unwrap()
        .continuation;
    drop(c);
    let reader = sqlite::open_read_only(&path, BINDING, [71; 32]).unwrap();
    assert_eq!(
        reader
            .layer_stacks(&Page {
                cursor: cursor.clone(),
                limit: 1
            })
            .unwrap()
            .records[0]
            .name
            .as_str(),
        "b"
    );
    assert!(sqlite::open_read_only(&path, BINDING, [0; 32]).is_err());
    let wrong = sqlite::open_read_only(&path, BINDING, [72; 32]).unwrap();
    assert_eq!(
        wrong.layer_stacks(&Page { cursor, limit: 1 }).unwrap_err(),
        HistoryError::Integrity("page cursor digest")
    );
    assert_eq!(
        reader
            .reserve_inodes(&ReserveRequest {
                scope: root(200),
                count: 1
            })
            .unwrap_err(),
        HistoryError::ContinuityUnavailable
    );
}

#[test]
fn empty_history_checks_page_and_cursor_before_returning_empty() {
    let t = Temp::new("empty");
    let c = create(&t.join("db"));
    let s = init(&c, 1, "main");
    let b = fork(&c, &s, 1, s.head_layer);
    for (limit, cursor) in [
        (0, None),
        (129, None),
        (1, Some(vec![])),
        (1, Some(vec![0; 160])),
    ] {
        assert!(c
            .commit_history(&CommitHistoryRequest {
                branch: b.branch.id,
                start: None,
                cursor,
                limit
            })
            .is_err());
    }
}

#[test]
fn last_representable_reservation_is_consumed_and_terminal_refusal_is_atomic() {
    let t = Temp::new("terminal");
    let path = t.join("db");
    let c = create(&path);
    let scope = root(200);
    c.reserve_inodes(&ReserveRequest { scope, count: 1 })
        .unwrap();
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute("UPDATE scope_allocator SET highwater=?1", [i64::MAX - 3])
        .unwrap();
    let r = c
        .reserve_inodes(&ReserveRequest { scope, count: 2 })
        .unwrap();
    assert_eq!(r.start, i64::MAX as u64 - 2);
    assert_eq!(r.end().unwrap(), i64::MAX as u64);
    assert_eq!(
        c.reserve_inodes(&ReserveRequest { scope, count: 1 })
            .unwrap_err(),
        HistoryError::Capacity("inode serials")
    );
    let high: i64 = sql
        .query_row("SELECT highwater FROM scope_allocator", [], |r| r.get(0))
        .unwrap();
    assert_eq!(high, i64::MAX - 1);
}

#[test]
fn reopen_refuses_each_independent_schema_or_binding_corruption() {
    for (i,mutation) in [
        "DROP INDEX layers_child;",
        "CREATE TABLE sqliteX (id INTEGER) STRICT;",
        "CREATE TRIGGER sqliteY AFTER INSERT ON history_meta BEGIN SELECT 1; END;",
        "UPDATE history_meta SET binding_key=X'78';",
        "PRAGMA writable_schema=ON; UPDATE sqlite_schema SET sql=replace(sql,'CHECK (id = 1)','CHECK (id > 0)') WHERE name='history_meta';",
        "PRAGMA writable_schema=ON; UPDATE sqlite_schema SET sql=replace(sql,'DEFERRABLE INITIALLY DEFERRED','') WHERE name='branches';",
        "PRAGMA writable_schema=ON; UPDATE sqlite_schema SET sql=replace(sql,') STRICT',')') WHERE name='history_meta';",
        "PRAGMA writable_schema=ON; UPDATE sqlite_schema SET sql=replace(sql,'REFERENCES layers(layer_stack_id, layer_id)','REFERENCES commits(layer_stack_id, commit_id)') WHERE name='branches';",
        "PRAGMA writable_schema=ON; UPDATE sqlite_schema SET sql=replace(sql,'binding_key BLOB','binding_key TEXT') WHERE name='history_meta';",
        "PRAGMA ignore_check_constraints=ON; UPDATE history_meta SET catalog_incarnation=0;",
    ].iter().enumerate() {
        let t=Temp::new(&format!("schema{i}"));let path=t.join("db");drop(create(&path));
        let sql=rusqlite::Connection::open(&path).unwrap();sql.set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_DEFENSIVE,false).unwrap();sql.execute_batch(mutation).unwrap();drop(sql);
        assert!(sqlite::open_read_only(&path,BINDING,[71;32]).is_err(),"mutation {i}");
    }
}

#[test]
fn stale_base_precedes_no_changes_and_derived_provenance_collision() {
    let t = Temp::new("base");
    let c = create(&t.join("db"));
    let s = init(&c, 1, "main");
    let a = fork(&c, &s, 1, s.head_layer);
    let b = fork(&c, &s, 2, s.head_layer);
    let k1 = append(&c, a.branch.id, 1);
    let sibling = append(&c, b.branch.id, 1);
    assert_eq!(k1, sibling);
    let l1 = match publish(&c, &s, a.branch.id, &k1, s.head_layer).unwrap() {
        AddLayerOutcome::Added(l) => l,
        _ => panic!(),
    };
    let expected = HistoryError::StackMoved {
        expected: s.head_layer,
        actual: l1.id,
    };
    assert_eq!(
        publish(&c, &s, b.branch.id, &sibling, l1.id).unwrap_err(),
        expected
    );
    let k2 = append(&c, a.branch.id, 2);
    assert_eq!(
        publish(&c, &s, a.branch.id, &k2, l1.id).unwrap_err(),
        expected
    );
    let k0 = append(&c, a.branch.id, 0);
    assert_eq!(
        publish(&c, &s, a.branch.id, &k0, l1.id).unwrap_err(),
        expected
    );
    assert_eq!(c.layer(l1.id).unwrap().unwrap(), l1);
    assert_eq!(
        publish(&c, &s, a.branch.id, &k1, s.head_layer).unwrap(),
        AddLayerOutcome::UpToDate { layer: l1.id }
    );
}

#[test]
fn unknown_statement_outcome_quarantines_without_implicit_rollback() {
    let t = Temp::new("unknown");
    let path = t.join("db");
    let c = create(&path);
    let s = init(&c, 1, "main");
    let b = fork(&c, &s, 1, s.head_layer);
    let stage = c
        .stage_changes(&StageRequest {
            workspace: workspace(1),
            branch: b.branch.id,
            expected_head: None,
            expected_base: s.head_layer,
            expected_root: root(0),
            construction_base_root: root(0),
            intended_commit_base: s.head_layer,
            candidate_root: root(1),
            profile: s.profile,
            scope: s.scope,
            generation: 1,
        })
        .unwrap();
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.busy_timeout(std::time::Duration::ZERO).unwrap();
    // External disposable-database corruption, not a product fault-injection path.
    sql.execute_batch("CREATE TRIGGER broken_delete BEFORE DELETE ON workspace_stages BEGIN SELECT absent_function(); END;").unwrap();
    let err = c
        .commit_staged(&CommitStagedRequest {
            workspace: stage.workspace,
            token: stage.token,
        })
        .unwrap_err();
    assert!(err.unknown());
    assert!(
        matches!(err,HistoryError::WithStage{stage:layerfs_history::error::StageDisposition::AcknowledgedUnknown(ref known),..} if **known==stage)
    );
    assert_eq!(
        c.branch(b.branch.id).unwrap_err(),
        HistoryError::UnknownOutcome
    );
    assert_eq!(
        c.reserve_inodes(&ReserveRequest {
            scope: s.scope,
            count: 1
        })
        .unwrap_err(),
        HistoryError::UnknownOutcome
    );
    // Pending writes remain isolated; a second writer cannot enter. RAII did not roll back.
    assert!(sql.execute_batch("BEGIN IMMEDIATE").is_err());
    let count: i64 = sql
        .query_row("SELECT count(*) FROM commits", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
    drop(c);
    // Connection teardown is resource release, not an acknowledged operation rollback.
    sql.execute_batch("BEGIN IMMEDIATE; ROLLBACK;").unwrap();
}

#[test]
fn concurrent_reservations_never_overlap_and_discard_does_not_refund() {
    use std::sync::{Arc, Barrier};
    let t = Temp::new("reserve-race");
    let c = Arc::new(create(&t.join("db")));
    let scope = root(200);
    let barrier = Arc::new(Barrier::new(4));
    let results = std::thread::scope(|threads| {
        let jobs: Vec<_> = (0..4)
            .map(|_| {
                let c = Arc::clone(&c);
                let barrier = Arc::clone(&barrier);
                threads.spawn(move || {
                    barrier.wait();
                    c.reserve_inodes(&ReserveRequest { scope, count: 3 })
                })
            })
            .collect();
        jobs.into_iter()
            .map(|job| job.join().unwrap())
            .collect::<Vec<_>>()
    });
    let mut ranges = vec![];
    for result in results {
        match result {
            Ok(range) => {
                assert_eq!(range.end().unwrap(), range.start + 3);
                ranges.push(range)
            }
            Err(error) => assert_eq!(error, HistoryError::Busy),
        }
    }
    assert!(!ranges.is_empty());
    ranges.sort_by_key(|range| range.start);
    assert!(ranges
        .windows(2)
        .all(|pair| pair[0].end().unwrap() <= pair[1].start));
    let after = c
        .reserve_inodes(&ReserveRequest { scope, count: 1 })
        .unwrap();
    assert_eq!(after.start, ranges.last().unwrap().end().unwrap());
    let s = init(c.as_ref(), 1, "main");
    let b = fork(c.as_ref(), &s, 1, s.head_layer);
    let stage = c
        .stage_changes(&StageRequest {
            workspace: workspace(1),
            branch: b.branch.id,
            expected_head: None,
            expected_base: s.head_layer,
            expected_root: root(0),
            construction_base_root: root(0),
            intended_commit_base: s.head_layer,
            candidate_root: root(1),
            profile: s.profile,
            scope,
            generation: 1,
        })
        .unwrap();
    c.discard_stage(&DiscardRequest {
        workspace: stage.workspace,
        token: stage.token,
    })
    .unwrap();
    assert_eq!(
        c.reserve_inodes(&ReserveRequest { scope, count: 1 })
            .unwrap()
            .start,
        after.end().unwrap()
    );
}

#[test]
fn explicit_membership_ceiling_is_capacity_but_old_authenticated_anchor_stays_valid() {
    let t = Temp::new("membership-bound");
    let c = create(&t.join("db"));
    let s = init(&c, 1, "main");
    let b = fork(&c, &s, 1, s.head_layer);
    let first = append(&c, b.branch.id, 1);
    append(&c, b.branch.id, 2);
    let cursor = c
        .commit_history(&CommitHistoryRequest {
            branch: b.branch.id,
            start: None,
            cursor: None,
            limit: 1,
        })
        .unwrap()
        .continuation;
    for index in 0..4352u64 {
        let snapshot = c.branch_snapshot(b.branch.id).unwrap().unwrap();
        let candidate = layerfs_content::ObjectId::for_bytes(&index.to_be_bytes());
        let stage = c
            .stage_changes(&StageRequest {
                workspace: workspace(3),
                branch: b.branch.id,
                expected_head: snapshot.branch.head_commit,
                expected_base: s.head_layer,
                expected_root: snapshot.effective_root,
                construction_base_root: snapshot.effective_root,
                intended_commit_base: s.head_layer,
                candidate_root: candidate,
                profile: s.profile,
                scope: s.scope,
                generation: index,
            })
            .unwrap();
        c.commit_staged(&CommitStagedRequest {
            workspace: stage.workspace,
            token: stage.token,
        })
        .unwrap();
    }
    assert_eq!(
        c.commit_history(&CommitHistoryRequest {
            branch: b.branch.id,
            start: Some(first.id),
            cursor: None,
            limit: 1
        })
        .unwrap_err(),
        HistoryError::Capacity("lineage work")
    );
    let resumed = c
        .commit_history(&CommitHistoryRequest {
            branch: b.branch.id,
            start: None,
            cursor,
            limit: 1,
        })
        .unwrap();
    assert_eq!(resumed.records, vec![first]);
    let mut cursor = None;
    let mut seen = std::collections::BTreeSet::new();
    let mut last = None;
    loop {
        let page = c
            .commit_history(&CommitHistoryRequest {
                branch: b.branch.id,
                start: None,
                cursor,
                limit: 128,
            })
            .unwrap();
        for record in page.records {
            if let Some(expected) = last {
                assert_eq!(record.id, expected);
            }
            assert!(seen.insert(record.id));
            last = record.parent;
        }
        match page.continuation {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    assert_eq!(seen.len(), 4354);
    assert_eq!(last, None);
}

#[test]
fn exact_source_publication_revalidates_immutable_provenance() {
    let t = Temp::new("provenance");
    let path = t.join("db");
    let c = create(&path);
    let s = init(&c, 1, "main");
    let b = fork(&c, &s, 1, s.head_layer);
    let k = append(&c, b.branch.id, 1);
    let l = match publish(&c, &s, b.branch.id, &k, s.head_layer).unwrap() {
        AddLayerOutcome::Added(l) => l,
        _ => panic!(),
    };
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute(
        "UPDATE layers SET root_id=?1 WHERE layer_id=?2",
        rusqlite::params![root(9).as_bytes().as_slice(), l.id.as_slice()],
    )
    .unwrap();
    assert_eq!(
        publish(&c, &s, b.branch.id, &k, s.head_layer).unwrap_err(),
        HistoryError::Integrity("Layer provenance")
    );
    let stored: Vec<u8> = sql
        .query_row(
            "SELECT root_id FROM layers WHERE layer_id=?1",
            [l.id.as_slice()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored, root(9).as_bytes());
}
