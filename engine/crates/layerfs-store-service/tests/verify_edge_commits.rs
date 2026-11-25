//! #768 independent verification: edge-shaped Workspace Commits beyond the
//! author's size matrix, on SQLite and PostgreSQL, compared step by step.
//!
//! One Workspace runs a fixed script of Commits: an empty Commit, net-zero
//! changes (a file created and removed again, so the candidate root is the
//! bound one) with the head unmoved and with the head moved by a second
//! Workspace, delete-only, directory-rename-only, symlink-only (new,
//! retargeted, a long dangling target), empty files and truncation to zero,
//! empty directories, files crossing the whole-file cutoff by append and by
//! truncation, a 4 MiB + 1 byte file, rename over an existing file, a rename
//! with a re-creation at the old name, a whole subtree removed, and a
//! rename chain (a -> b, b -> c) in one Commit.
//!
//! After every step: an UpToDate answer left the head where it was; a
//! Committed answer is the head, one past the previous one, and the Branch
//! read from a fresh service equals the Workspace's live view. The two
//! backends give the same answer kinds and the same roots at every step
//! (content addressing), and each Store passes its audit.
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;

use adversary::{
    apply, assert_linear, commit, random_bytes, text_bytes, tree_of, verdict, write, Op, TestStore,
    Verdict,
};

const SEED: &[(&str, &[u8])] = &[
    ("/seed", b"seed\n"),
    ("/doomed", b"doomed\n"),
    ("/over", b"over me\n"),
    ("/d/one", b"one\n"),
    ("/d/two", b"two\n"),
    ("/d/sub/three", b"three\n"),
];

/// One step: a label, whether the second Workspace commits first, and the
/// changes of the main Workspace.
fn script() -> Vec<(&'static str, bool, Vec<Op>)> {
    let mut big_tree = vec![Op::Mkdir("/tree".into())];
    for n in 0..50 {
        big_tree.push(Op::Write(
            format!("/tree/f{n}"),
            format!("tree {n}\n").into_bytes(),
        ));
    }
    let mut remove_tree: Vec<Op> = (0..50).map(|n| Op::Unlink(format!("/tree/f{n}"))).collect();
    remove_tree.push(Op::Rmdir("/tree".into()));
    vec![
        ("empty", false, vec![]),
        (
            "create-remove-first",
            false,
            vec![
                Op::Write("/t".into(), b"t".to_vec()),
                Op::Unlink("/t".into()),
            ],
        ),
        (
            "create-remove-again",
            false,
            vec![
                Op::Write("/t2".into(), b"t".to_vec()),
                Op::Unlink("/t2".into()),
            ],
        ),
        (
            "net-zero-over-moved-head",
            true,
            vec![
                Op::Write("/t3".into(), b"t".to_vec()),
                Op::Unlink("/t3".into()),
            ],
        ),
        (
            "after-net-zero",
            false,
            vec![Op::Write("/w".into(), b"w\n".to_vec())],
        ),
        ("delete-only", false, vec![Op::Unlink("/doomed".into())]),
        (
            "dir-rename-only",
            false,
            vec![Op::Rename("/d".into(), "/e".into())],
        ),
        (
            "symlink-new",
            false,
            vec![Op::Symlink("/l".into(), b"seed".to_vec())],
        ),
        (
            "symlink-retarget",
            false,
            vec![
                Op::Unlink("/l".into()),
                Op::Symlink("/l".into(), b"e/one".to_vec()),
            ],
        ),
        (
            "symlink-long-dangling",
            false,
            vec![Op::Symlink("/dangling".into(), vec![b'x'; 1000])],
        ),
        (
            "empty-and-truncate",
            false,
            vec![
                Op::Write("/empty".into(), vec![]),
                Op::Write("/seed".into(), vec![]),
            ],
        ),
        ("empty-dir", false, vec![Op::Mkdir("/hollow".into())]),
        ("rmdir-only", false, vec![Op::Rmdir("/hollow".into())]),
        (
            "cutoff-files",
            false,
            vec![
                Op::Write("/c131071".into(), random_bytes(1, 131_071)),
                Op::Write("/c131072".into(), random_bytes(2, 131_072)),
                Op::Write("/c262144".into(), random_bytes(3, 262_144)),
            ],
        ),
        (
            "cross-cutoff-up-and-down",
            false,
            vec![
                Op::Append("/c131071".into(), vec![7]),
                Op::Write("/c131072".into(), random_bytes(2, 131_071)),
            ],
        ),
        (
            "four-mib-plus-one",
            false,
            vec![Op::Write("/huge".into(), text_bytes(4, (4 << 20) + 1))],
        ),
        (
            "rename-over",
            false,
            vec![Op::Rename("/w".into(), "/over".into())],
        ),
        (
            "rename-and-recreate",
            false,
            vec![
                Op::Rename("/over".into(), "/moved".into()),
                Op::Write("/over".into(), b"new over\n".to_vec()),
            ],
        ),
        ("big-tree", false, big_tree),
        ("remove-tree", false, remove_tree),
        (
            "rename-chain",
            false,
            vec![
                Op::Write("/a".into(), b"a\n".to_vec()),
                Op::Rename("/moved".into(), "/b".into()),
                Op::Rename("/b".into(), "/c".into()),
            ],
        ),
        ("final-empty", false, vec![]),
    ]
}

/// The answers and roots of the script on one Store.
fn run(store: &TestStore) -> Vec<(String, String, Option<[u8; 32]>)> {
    let service = store.open();
    let branch = store.fork("edges");
    let main = service.bind(branch);
    let other = service.bind(branch);
    let mut out = Vec::new();
    let mut moves = 0;
    for (label, move_head, ops) in script() {
        let at = format!("{:?} {label}", store.kind);
        if move_head {
            // The second Workspace catches up, then moves the head.
            let _ = verdict(&commit(&other));
            moves += 1;
            write(&other, &format!("/moved-by-other-{moves}"), b"other\n");
            match verdict(&commit(&other)) {
                Verdict::Committed(_) => {}
                v => panic!("{at}: the other Workspace answered {v:?}"),
            }
        }
        for op in &ops {
            apply(&main, op).unwrap_or_else(|e| panic!("{at}: {op:?}: {e}"));
        }
        let before = store.chain(branch).first().map(|r| r.id);
        let view = tree_of(&main);
        let answer = verdict(&commit(&main));
        let chain = store.chain(branch);
        assert_linear(&chain);
        let head = chain.first();
        let kind = match &answer {
            Verdict::Committed(id) => {
                assert_eq!(head.map(|r| r.id), Some(*id), "{at}: answered head");
                assert_eq!(
                    head.unwrap().parent,
                    before,
                    "{at}: one past the previous head"
                );
                assert_eq!(
                    store.tree(branch),
                    view,
                    "{at}: Branch equals the live view"
                );
                "committed"
            }
            Verdict::UpToDate => {
                assert_eq!(head.map(|r| r.id), before, "{at}: UpToDate moved the head");
                "uptodate"
            }
            other => panic!("{at}: {other:?}"),
        };
        // After any answer the Workspace's view is the Branch plus nothing.
        assert_eq!(
            tree_of(&main),
            store.tree(branch),
            "{at}: view after the Commit"
        );
        out.push((
            label.to_owned(),
            kind.to_owned(),
            head.map(|r| *r.root.as_bytes()),
        ));
    }
    service.unmount(main);
    service.unmount(other);
    service.stop();
    let failures = store.integrity_failures();
    assert!(failures.is_empty(), "{:?}: audit {failures:#?}", store.kind);
    out
}

#[test]
fn edge_commits_agree_on_both_backends() {
    let stores = TestStore::both("v768-edges", SEED);
    let results: Vec<_> = stores.iter().map(run).collect();
    for (label, kind, _) in &results[0] {
        println!("VERIFY_EDGE {label}: {kind}");
    }
    if results.len() == 2 {
        for (sqlite, pg) in results[0].iter().zip(&results[1]) {
            assert_eq!(sqlite, pg, "SQLite and PostgreSQL disagree at {}", sqlite.0);
        }
    }
}
