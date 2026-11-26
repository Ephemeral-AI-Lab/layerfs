//! #810 F3: the directory-rewrite base reads of a Commit's construction are
//! batched. Two small shapes on one seeded tree (`/t/aII/bJJ/f`, 12 x 8
//! leaf directories, 109 directories under `/t`):
//!
//! * `add`: one new file in each of the 96 existing leaf directories (the
//!   "add 10k files into 3,942 directories" cell in small): 96 touched base
//!   directories, each rewritten from its base page.
//! * `again`: one more new file in each of the same 96 directories, on
//!   the `add` Branch, so the base pages are this Store's own rewrites.
//! * `remove`: `rm -r /t` (the "delete node_modules" cell in small): one
//!   touched directory and 109 removed ones, each traversed for release.
//!
//! Each Commit's SQL work is attributed to its phases
//! (`telemetry::attribute_sql`, the Store's writer plus reader session
//! totals) and the PostgreSQL round trips and SQLite statements of the
//! phases this fix changes are pinned exactly, as are the totals of the
//! whole Commit. The resulting tree is checked against the model on both
//! backends, and the two backends build the same root.
//!
//! The PostgreSQL half needs `LAYERFS_TEST_POSTGRES_URL` and reports
//! NOT_RUN without it.
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;

use adversary::{apply, commit, seed_tree, verdict, Kind, Node, Op, TestStore, Tree, Verdict};
use layerfs_persistence::SqlWork;
use layerfs_store_service::telemetry::{attribute_sql, phase_sql, Recording};
use std::collections::BTreeMap;
use std::fmt::Write;

const OUTER: usize = 12;
const INNER: usize = 8;

fn leaf(a: usize, b: usize) -> String {
    format!("/t/a{a:02}/b{b:02}")
}

fn seed() -> Vec<(String, Vec<u8>)> {
    let mut files = vec![("/keep".to_owned(), b"keep\n".to_vec())];
    for a in 0..OUTER {
        for b in 0..INNER {
            files.push((
                format!("{}/f", leaf(a, b)),
                format!("f {a} {b}\n").into_bytes(),
            ));
        }
    }
    files
}

fn add_ops(name: &str) -> Vec<Op> {
    let mut ops = Vec::new();
    for a in 0..OUTER {
        for b in 0..INNER {
            ops.push(Op::Write(
                format!("{}/{name}", leaf(a, b)),
                format!("{name} {a} {b}\n").into_bytes(),
            ));
        }
    }
    ops
}

fn remove_ops() -> Vec<Op> {
    let mut ops = Vec::new();
    for a in 0..OUTER {
        for b in 0..INNER {
            ops.push(Op::Unlink(format!("{}/f", leaf(a, b))));
            ops.push(Op::Rmdir(leaf(a, b)));
        }
        ops.push(Op::Rmdir(format!("/t/a{a:02}")));
    }
    ops.push(Op::Rmdir("/t".to_owned()));
    ops
}

/// The SQL work of one Commit: the whole call and each phase (inclusive).
struct Counted {
    root: Tree,
    /// The committed filesystem root, in hex.
    object: String,
    total: SqlWork,
    phases: BTreeMap<&'static str, SqlWork>,
    stored: (u64, u64, u64, u64),
}

fn counted(store: &TestStore, branch: &str, ops: &[Op], fork: bool) -> Counted {
    let service = store.open();
    let id = if fork {
        store.fork(branch)
    } else {
        store.branch(branch)
    };
    let workspace = service.bind(id);
    for op in ops {
        apply(&workspace, op).unwrap_or_else(|error| panic!("{op:?}: {error}"));
    }
    let sessions = service.service.opened.sql_sessions();
    let probe = move || {
        let (writer, readers, _) = sessions.totals();
        let mut total = writer.unwrap_or_default();
        total.accumulate(&readers.unwrap_or_default());
        total
    };
    let before = probe();
    let recording = Recording::start();
    let sessions = service.service.opened.sql_sessions();
    attribute_sql(Some(Box::new(move || {
        let (writer, readers, _) = sessions.totals();
        let mut total = writer.unwrap_or_default();
        total.accumulate(&readers.unwrap_or_default());
        total
    })));
    let outcome = commit(&workspace);
    let phases: BTreeMap<&'static str, SqlWork> = phase_sql().into_iter().collect();
    attribute_sql(None);
    drop(recording.finish());
    let total = probe().since(&before);
    assert!(
        matches!(verdict(&outcome), Verdict::Committed(_)),
        "{branch}: {:?}",
        verdict(&outcome)
    );
    let saved = outcome.unwrap().saved;
    service.unmount(workspace);
    service.stop();
    let chain = store.chain(store.branch(branch));
    let object: String =
        chain
            .first()
            .unwrap()
            .root
            .to_bytes()
            .iter()
            .fold(String::new(), |mut out, byte| {
                let _ = write!(out, "{byte:02x}");
                out
            });
    Counted {
        object,
        root: store.tree(store.branch(branch)),
        total,
        phases,
        stored: (
            saved.stored_bytes,
            saved.inserted,
            saved.reused,
            saved.packs,
        ),
    }
}

fn expected_add(names: &[&str]) -> Tree {
    let mut tree = seed_tree(&refs(&seed()));
    for name in names {
        for a in 0..OUTER {
            for b in 0..INNER {
                tree.insert(
                    format!("{}/{name}", leaf(a, b)),
                    Node::File(format!("{name} {a} {b}\n").into_bytes()),
                );
            }
        }
    }
    tree
}

fn expected_remove() -> Tree {
    let mut tree = Tree::new();
    tree.insert("/keep".into(), Node::File(b"keep\n".to_vec()));
    tree
}

fn refs(seed: &[(String, Vec<u8>)]) -> Vec<(&str, &[u8])> {
    seed.iter()
        .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
        .collect()
}

/// (round trips on PostgreSQL, statements on SQLite) of one phase.
fn of(counted: &Counted, kind: Kind, phase: &str) -> u64 {
    let work = counted.phases.get(phase).copied().unwrap_or_default();
    match kind {
        Kind::Postgres => work.round_trips,
        Kind::Sqlite => work.statements,
    }
}

fn report(kind: Kind, shape: &str, counted: &Counted) {
    let phases: Vec<String> = counted
        .phases
        .iter()
        .map(|(name, work)| format!("\"{name}\":[{},{}]", work.round_trips, work.statements))
        .collect();
    println!(
        "F3_COUNTS {{\"backend\":\"{kind:?}\",\"shape\":\"{shape}\",\"round_trips\":{},\"statements\":{},\"stored\":{:?},\"root\":\"{}\",\"phases\":{{{}}}}}",
        counted.total.round_trips,
        counted.total.statements,
        counted.stored,
        counted.object,
        phases.join(",")
    );
}

/// The committed roots and the Saves' (stored bytes, inserted, reused,
/// packs), identical on both backends and before and after F3.
const ADD_ROOT: &str = "55fdd167f554b2729ee955a44e3fb18c63131035e9b92c9fe4c80b46ab762fb5";
const ADD_STORED: (u64, u64, u64, u64) = (3_280, 200, 0, 2);
const AGAIN_ROOT: &str = "994be659e5c0a889deaf8e195d5c17a0b82cd222a536ea5fb944291c4a72da5f";
const AGAIN_STORED: (u64, u64, u64, u64) = (3_370, 199, 0, 2);
const REMOVE_ROOT: &str = "5c99a77b1f47bd51b2890dfd838684b9818c22f47c3747ca03269980e668c2ea";
const REMOVE_STORED: (u64, u64, u64, u64) = (301, 4, 108, 1);

/// The pinned counts of one backend: (construct, whole Commit) for `add`
/// and `remove`, round trips on PostgreSQL and statements on SQLite.
struct Pins {
    add: (u64, u64),
    again: (u64, u64),
    remove: (u64, u64),
}

fn shapes(store: &TestStore, pins: Pins) {
    let kind = store.kind;
    let add = counted(store, "add", &add_ops("n"), true);
    report(kind, "add", &add);
    assert_eq!(add.root, expected_add(&["n"]), "{kind:?} add: tree");
    // The same 96 directories again: their stored pages now have chains.
    let again = counted(store, "add", &add_ops("m"), false);
    report(kind, "again", &again);
    assert_eq!(
        again.root,
        expected_add(&["n", "m"]),
        "{kind:?} again: tree"
    );
    let remove = counted(store, "remove", &remove_ops(), true);
    report(kind, "remove", &remove);
    assert_eq!(remove.root, expected_remove(), "{kind:?} remove: tree");
    // A read-only change: the same root and the same stored objects as
    // before the reads were grouped (recorded at e11d8c8d0f), both backends.
    assert_eq!(add.object, ADD_ROOT, "{kind:?} add: root");
    assert_eq!(add.stored, ADD_STORED, "{kind:?} add: stored");
    assert_eq!(again.object, AGAIN_ROOT, "{kind:?} again: root");
    assert_eq!(again.stored, AGAIN_STORED, "{kind:?} again: stored");
    assert_eq!(remove.object, REMOVE_ROOT, "{kind:?} remove: root");
    assert_eq!(remove.stored, REMOVE_STORED, "{kind:?} remove: stored");
    let total = |counted: &Counted| match kind {
        Kind::Postgres => counted.total.round_trips,
        Kind::Sqlite => counted.total.statements,
    };
    assert_eq!(
        (of(&add, kind, "construct"), total(&add)),
        pins.add,
        "{kind:?} add: (construct, Commit)"
    );
    assert_eq!(
        (of(&again, kind, "construct"), total(&again)),
        pins.again,
        "{kind:?} again: (construct, Commit)"
    );
    assert_eq!(
        (of(&remove, kind, "construct"), total(&remove)),
        pins.remove,
        "{kind:?} remove: (construct, Commit)"
    );
}

#[test]
fn directory_rewrites_are_batched_sqlite() {
    let seed = seed();
    let store = TestStore::sqlite("f3-sqlite", &refs(&seed));
    // Before F3: add (107, 140), again (111, 147), remove (120, 147).
    shapes(
        &store,
        Pins {
            add: (8, 41),
            again: (10, 46),
            remove: (8, 35),
        },
    );
}

#[test]
fn directory_rewrites_are_batched_postgresql() {
    let seed = seed();
    let Some(store) = TestStore::postgres("f3-pg", &refs(&seed)) else {
        return;
    };
    // Before F3: add (110, 125), again (116, 132), remove (123, 135).
    shapes(
        &store,
        Pins {
            add: (4, 22),
            again: (4, 27),
            remove: (4, 21),
        },
    );
}
