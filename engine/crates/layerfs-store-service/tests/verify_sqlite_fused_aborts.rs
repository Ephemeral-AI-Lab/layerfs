//! #768 independent verification on SQLite: the one write transaction of a
//! Workspace Commit (the Save's held last publication, the hints and the
//! Commit) aborted at each table it writes.
//!
//! A trigger installed through a separate connection raises
//! `RAISE(ABORT)` on the first row the Commit writes to one table; it is
//! dropped right after that Commit answers, so the follow-up Commit of the
//! same Workspace writes freely. The aborted Commit must be a definite
//! refusal with nothing written (raw counts, totals and Branch heads), the
//! follow-up must land with the same tree, a second round reusing the
//! attempt's bytes must land, and the Store must pass its audit.
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;

use adversary::{
    apply, assert_linear, commit, random_bytes, text_bytes, tree_of, verdict, Op, TestStore,
    Verdict,
};
use layerfs_persistence::StoreSelection;

const TABLES: &[(&str, &str)] = &[
    ("pack", "INSERT"),
    ("object_location", "INSERT"),
    ("inode_value_block", "INSERT"),
    ("store_totals", "UPDATE"),
    ("binding_hint_block", "INSERT"),
    ("history_meta", "UPDATE"),
    ("\"commit\"", "INSERT"),
    ("commit_lineage", "INSERT"),
    ("branch", "UPDATE"),
];

fn connection(store: &TestStore) -> rusqlite::Connection {
    let StoreSelection::Sqlite(config) = &store.selection else {
        unreachable!()
    };
    let connection = rusqlite::Connection::open(&config.path).unwrap();
    connection
        .busy_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    connection
}

fn written(store: &TestStore) -> String {
    connection(store)
        .query_row(
            "SELECT (SELECT count(*) FROM \"commit\") || '/' || \
             (SELECT count(*) FROM commit_lineage) || '/' || \
             (SELECT count(*) FROM object_location) || '/' || \
             (SELECT count(*) FROM pack) || '/' || \
             (SELECT count(*) FROM inode_value_block) || '/' || \
             (SELECT count(*) FROM binding_hint_block) || '/' || \
             (SELECT next_history_serial FROM history_meta WHERE id = 1) || '/' || \
             (SELECT group_concat(hex(branch_id) || '=' || coalesce(hex(head_commit_id), '-')) \
              FROM (SELECT * FROM branch ORDER BY branch_id))",
            [],
            |row| row.get(0),
        )
        .unwrap()
}

fn seed() -> Vec<(String, Vec<u8>)> {
    let mut seed = vec![
        ("/seed".to_owned(), b"seed\n".to_vec()),
        ("/doc.txt".to_owned(), text_bytes(7680, 300 * 1024)),
    ];
    for n in 0..5 {
        seed.push((format!("/dir/f{n}"), format!("dir file {n}\n").into_bytes()));
    }
    seed
}

fn changes(k: u64) -> Vec<Op> {
    let mut doc = text_bytes(7680, 300 * 1024);
    doc[100_000..100_064].copy_from_slice(&text_bytes(k + 1, 64));
    let mut ops = vec![
        Op::Write(
            format!("/big-{k}"),
            random_bytes(k + 100, 200 * 1024 + k as usize),
        ),
        Op::Write("/doc.txt".into(), doc),
        Op::Mkdir(format!("/many-{k}")),
    ];
    for n in 0..40 {
        ops.push(Op::Write(
            format!("/many-{k}/f{n}"),
            format!("case {k} small {n}\n").into_bytes(),
        ));
    }
    ops.push(Op::Symlink(format!("/link-{k}"), b"doc.txt".to_vec()));
    ops.push(Op::Rename("/dir/f0".into(), format!("/dir/renamed-{k}")));
    ops
}

#[test]
fn a_sqlite_commit_aborted_at_each_table() {
    let seed = seed();
    let seed: Vec<(&str, &[u8])> = seed
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_slice()))
        .collect();
    let store = TestStore::sqlite("v768-sqlite-abort", &seed);
    let service = store.open();
    let mut lines = Vec::new();
    for (n, (table, event)) in TABLES.iter().enumerate() {
        let k = n as u64 + 1;
        let name = format!("c{k}");
        let branch = store.fork(&name);
        let workspace = service.bind(branch);
        apply(&workspace, &Op::Write("/warm".into(), b"warm\n".to_vec())).unwrap();
        let warm = match verdict(&commit(&workspace)) {
            Verdict::Committed(id) => id,
            other => panic!("{name}: warm-up {other:?}"),
        };
        for op in changes(k) {
            apply(&workspace, &op).unwrap();
        }
        let intended = tree_of(&workspace);
        let before = written(&store);
        connection(&store)
            .execute_batch(&format!(
                "CREATE TRIGGER v768_t BEFORE {event} ON {table} \
                 BEGIN SELECT RAISE(ABORT, 'v768 injected abort'); END;"
            ))
            .unwrap();
        let first = verdict(&commit(&workspace));
        connection(&store)
            .execute_batch("DROP TRIGGER v768_t")
            .unwrap();
        let after = written(&store);
        let label = format!("{name} {table}/{event}: first {first:?}");
        lines.push(label.clone());
        match &first {
            Verdict::Refused(_) => {
                assert_eq!(after, before, "{label}: nothing written");
                assert_eq!(
                    store.chain(branch).first().map(|r| r.id),
                    Some(warm),
                    "{label}"
                );
                match verdict(&commit(&workspace)) {
                    Verdict::Committed(_) => {}
                    other => panic!("{label}: follow-up {other:?}"),
                }
            }
            Verdict::Committed(_) => {
                // The table was not written by this Commit.
                assert_ne!(after, before, "{label}: the Commit wrote");
                lines.push(format!("{name}: NOT_EXERCISED"));
            }
            other => panic!("{label}: {other:?}"),
        }
        let chain = store.chain(branch);
        assert_linear(&chain);
        assert_eq!(chain.len(), 2, "{label}: warm-up and the change");
        assert_eq!(store.tree(branch), intended, "{label}: landed tree");
        apply(
            &workspace,
            &Op::Write(
                format!("/copy-{k}"),
                random_bytes(k + 100, 200 * 1024 + k as usize),
            ),
        )
        .unwrap();
        apply(
            &workspace,
            &Op::Append("/doc.txt".into(), text_bytes(k, 3000)),
        )
        .unwrap();
        let intended = tree_of(&workspace);
        assert!(
            matches!(verdict(&commit(&workspace)), Verdict::Committed(_)),
            "{label}: round two"
        );
        assert_eq!(store.tree(branch), intended, "{label}: round two tree");
        service.unmount(workspace);
    }
    service.stop();
    for line in &lines {
        println!("VERIFY_SQLITE_ABORT {line}");
    }
    let failures = store.integrity_failures();
    assert!(failures.is_empty(), "audit: {failures:#?}");
}
