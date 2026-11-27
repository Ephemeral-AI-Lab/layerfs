//! #768 independent verification on PostgreSQL: the generation fence of the
//! fused Commit, and the kept id block across failures, drops, reopens and
//! a compaction swap.
//!
//! * A generation that moved after the service opened (what a compaction
//!   swap by another process leaves) refuses the fused Commit at its first
//!   request: a definite `StaleStore`, nothing written, no fallback; the
//!   same Workspace's next Commit is refused too, and a newly opened service
//!   commits the change.
//! * While a fused message is stopped inside its transaction (a blocker on
//!   the `store_totals` row), a swap's `LOCK ... ACCESS EXCLUSIVE NOWAIT` on
//!   `history_meta` and on the payload tables is refused, and a real
//!   compaction is refused: no swap can land inside the message. The Commit
//!   then lands.
//! * Kept ids: many services commit on several Branches (many small files,
//!   so inode values take ordinals from the kept block), some stopped
//!   cleanly, some abandoned, some leaked with their session still open
//!   (their kept block never released), one Commit refused by an injected
//!   abort and one by a race; then a compaction swap with a leaked service
//!   still open, after which that old service's Commit is refused `Stale`
//!   or lands, and new services keep committing. Every Branch read from a
//!   fresh service equals the last view committed on it; no pack id,
//!   object locator or inode ordinal range is used twice; the audit and
//!   amcheck pass.
//!
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "../support/verify/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "../support/verify/core.rs"]
#[allow(dead_code)]
mod verify768;

use adversary::{
    apply, commit, random_bytes, tree_of, verdict, wait_until, write, Op, Service, TestStore, Tree,
    Verdict,
};
use layerfs_history::{BranchId, HistoryError};
use layerfs_persistence::{CompactRequest, Handles};
use layerfs_protocol::sdk::SdkValue;
use std::collections::BTreeMap;
use std::sync::mpsc;
use std::time::Duration;

const SEED: &[(&str, &[u8])] = &[("/seed", b"seed\n")];

fn compact(store: &TestStore, op: u8) -> String {
    let Handles::Postgres(handles) = store.handles() else {
        unreachable!()
    };
    let result = handles.compact(&CompactRequest {
        op_id: [op; 16],
        ..CompactRequest::default()
    });
    handles.close();
    match result {
        Ok(receipt) => format!("Ok({})", receipt.path.as_str()),
        Err(HistoryError::MountsActive(_)) => "MountsActive".into(),
        Err(HistoryError::Busy | HistoryError::BusyOp(_)) => "Busy".into(),
        Err(other) => format!("{other:?}"),
    }
}

#[test]
fn a_moved_generation_refuses_the_fused_commit() {
    let Some(store) = TestStore::postgres("v768_gen", SEED) else {
        return;
    };
    let branch = store.fork("gen");
    let service = store.open();
    let workspace = service.bind(branch);
    write(&workspace, "/warm", b"warm\n");
    let warm = match verdict(&commit(&workspace)) {
        Verdict::Committed(id) => id,
        other => panic!("warm-up {other:?}"),
    };
    write(&workspace, "/x", &random_bytes(5, 150 * 1024));
    let intended = tree_of(&workspace);
    store
        .admin()
        .batch_execute("UPDATE history_meta SET catalog_incarnation = catalog_incarnation + 1")
        .unwrap();
    let before = verify768::written(&store);
    let counts = service.service.store.fused_counts();
    let first = verdict(&commit(&workspace));
    let after = verify768::written(&store);
    let fused = service.service.store.fused_counts();
    let follow = verdict(&commit(&workspace));
    println!("VERIFY_GEN {{\"first\":\"{first:?}\",\"follow\":\"{follow:?}\"}}");
    assert!(
        matches!(&first, Verdict::Refused(text) if text.contains("Stale")),
        "a moved generation: {first:?}"
    );
    assert_eq!(fused, counts, "no fused Commit and no fallback");
    assert_eq!(after, before, "nothing written");
    assert!(
        !matches!(follow, Verdict::Committed(_)),
        "the stale session published: {follow:?}"
    );
    assert_eq!(store.chain(branch).first().map(|r| r.id), Some(warm));
    let _ = service.try_unmount(workspace);
    service.abandon();
    // A new service, at the new generation, commits the same change.
    let service = store.open();
    let workspace = service.bind(branch);
    write(&workspace, "/x", &random_bytes(5, 150 * 1024));
    assert!(matches!(
        verdict(&commit(&workspace)),
        Verdict::Committed(_)
    ));
    service.unmount(workspace);
    service.stop();
    assert_eq!(store.tree(branch), intended);
    let failures = store.integrity_failures();
    assert!(failures.is_empty(), "audit: {failures:#?}");
}

#[test]
fn no_swap_lands_inside_a_fused_message() {
    let Some(store) = TestStore::postgres("v768_noswap", SEED) else {
        return;
    };
    // Dead bytes, so a compaction would rebuild.
    let gc = store.open();
    let trash = store.fork("trash");
    let workspace = gc.bind(trash);
    write(&workspace, "/dead", &random_bytes(9, 2 << 20));
    assert!(matches!(
        verdict(&commit(&workspace)),
        Verdict::Committed(_)
    ));
    gc.unmount(workspace);
    gc.call("branch.remove", vec![("branch", SdkValue::text("trash"))])
        .expect("trash removed");
    gc.stop();
    let branch = store.fork("live");
    let service = store.open();
    let workspace = service.bind(branch);
    write(&workspace, "/warm", b"warm\n");
    assert!(matches!(
        verdict(&commit(&workspace)),
        Verdict::Committed(_)
    ));
    write(&workspace, "/x", &random_bytes(10, 200 * 1024));
    let intended = tree_of(&workspace);
    let mut holder = store.admin();
    let holder_pid: i32 = holder
        .query_one("SELECT pg_backend_pid()", &[])
        .unwrap()
        .get(0);
    holder
        .batch_execute("BEGIN; SELECT 1 FROM store_totals FOR UPDATE")
        .unwrap();
    let mut observer = store.admin();
    let (sent, answer) = mpsc::channel();
    let (first, locks, compaction) = std::thread::scope(|scope| {
        let workspace = &workspace;
        scope.spawn(move || {
            let _ = sent.send(verdict(&commit(workspace)));
        });
        assert!(
            wait_until(Duration::from_secs(10), || {
                observer
                .query_one(
                    "SELECT count(*) FROM pg_stat_activity a WHERE $1 = ANY(pg_blocking_pids(a.pid))",
                    &[&holder_pid],
                )
                .unwrap()
                .get::<_, i64>(0)
                == 1
            }),
            "the fused message waits at the totals row"
        );
        let mut locks = Vec::new();
        for table in ["history_meta", "pack", "object_location", "branch"] {
            let mut swapper = store.admin();
            let result = swapper.batch_execute(&format!(
                "BEGIN; LOCK TABLE {table} IN ACCESS EXCLUSIVE MODE NOWAIT; ROLLBACK"
            ));
            locks.push((
                table,
                result
                    .err()
                    .and_then(|e| e.code().map(|c| c.code().to_owned()))
                    .unwrap_or_else(|| "granted".into()),
            ));
        }
        let compaction = compact(&store, 3);
        holder.batch_execute("ROLLBACK").unwrap();
        let first = answer
            .recv_timeout(Duration::from_secs(30))
            .expect("the Commit answers");
        (first, locks, compaction)
    });
    println!(
        "VERIFY_NOSWAP {{\"first\":\"{first:?}\",\"locks\":\"{locks:?}\",\"compaction\":\"{compaction}\"}}"
    );
    for (table, code) in &locks {
        assert_eq!(
            code, "55P03",
            "a swap lock on {table} inside the fused message"
        );
    }
    assert!(
        ["MountsActive", "Busy"].contains(&compaction.as_str()),
        "compaction beside a fused message: {compaction}"
    );
    assert!(matches!(first, Verdict::Committed(_)), "{first:?}");
    service.unmount(workspace);
    service.stop();
    assert_eq!(store.tree(branch), intended);
    // No mount: the compaction runs now and the Branch survives it.
    assert_eq!(compact(&store, 4), "Ok(rebuild)");
    assert_eq!(store.tree(branch), intended);
    let failures = store.integrity_failures();
    assert!(failures.is_empty(), "audit: {failures:#?}");
}

/// Many small files and one larger one under `/r{round}`.
fn round_ops(round: usize, salt: u64) -> Vec<Op> {
    let dir = format!("/r{round}");
    let mut ops = vec![Op::Mkdir(dir.clone())];
    for n in 0..60 {
        ops.push(Op::Write(
            format!("{dir}/f{n}"),
            format!("round {round} file {n} salt {salt}\n").into_bytes(),
        ));
    }
    ops.push(Op::Write(
        format!("{dir}/big"),
        random_bytes(salt * 1000 + round as u64, 140 * 1024),
    ));
    ops
}

/// Binds `branch` on `service`, applies `ops`, commits; the answer and the
/// view committed.
fn commit_round(
    service: &Service,
    branch: BranchId,
    ops: &[Op],
) -> (Verdict, Tree, adversary::Tree) {
    let workspace = service.bind(branch);
    let base = tree_of(&workspace);
    for op in ops {
        apply(&workspace, op).unwrap();
    }
    let view = tree_of(&workspace);
    let answer = verdict(&commit(&workspace));
    service.unmount(workspace);
    (answer, view, base)
}

#[test]
fn kept_ids_survive_drops_failures_reopens_and_a_swap() {
    let Some(store) = TestStore::postgres("v768_kept", SEED) else {
        return;
    };
    let branches: Vec<BranchId> = (0..3).map(|n| store.fork(&format!("k{n}"))).collect();
    let mut expected: BTreeMap<BranchId, Tree> = BTreeMap::new();
    let mut leaked: Vec<Service> = Vec::new();
    let mut log = Vec::new();
    let mut round = 0;
    let mut salt = 0u64;
    // Phase one: services that stop, abandon and leak, with an injected
    // abort and a race among them.
    for step in 0..9 {
        let service = store.open();
        for (b, branch) in branches.iter().enumerate() {
            round += 1;
            salt += 1;
            let ops = round_ops(round, salt);
            if step == 4 && b == 1 {
                // A definite abort at the publication rows: the Save's kept
                // ids were carried by a message that wrote nothing.
                let target = verify768::TARGETS[1];
                verify768::arm(&store, target, "40001");
                let workspace = service.bind(*branch);
                for op in &ops {
                    apply(&workspace, op).unwrap();
                }
                let view = tree_of(&workspace);
                let first = verdict(&commit(&workspace));
                verify768::disarm(&store, target);
                assert!(matches!(first, Verdict::Refused(_)), "injected: {first:?}");
                let again = verdict(&commit(&workspace));
                assert!(
                    matches!(again, Verdict::Committed(_)),
                    "after injected: {again:?}"
                );
                service.unmount(workspace);
                expected.insert(*branch, view);
                log.push(format!("{step}/{b}: injected {first:?} then {again:?}"));
                continue;
            }
            if step == 6 && b == 2 {
                // A race: another service moves the head between this
                // Workspace's bind and its Commit.
                let workspace = service.bind(*branch);
                for op in &ops {
                    apply(&workspace, op).unwrap();
                }
                let mover = store.open();
                let (moved, _, _) = commit_round(
                    &mover,
                    *branch,
                    &[Op::Write(format!("/mover-{round}"), b"mover\n".to_vec())],
                );
                assert!(matches!(moved, Verdict::Committed(_)), "mover: {moved:?}");
                mover.stop();
                let counts = service.service.store.fused_counts();
                let answer = verdict(&commit(&workspace));
                let fused = service.service.store.fused_counts();
                assert!(matches!(answer, Verdict::Committed(_)), "raced: {answer:?}");
                assert_eq!(fused.1 - counts.1, 1, "one fallback");
                service.unmount(workspace);
                expected.insert(*branch, store.tree(*branch));
                log.push(format!("{step}/{b}: raced {answer:?}"));
                continue;
            }
            let (answer, view, _) = commit_round(&service, *branch, &ops);
            assert!(
                matches!(answer, Verdict::Committed(_)),
                "{step}/{b}: {answer:?}"
            );
            expected.insert(*branch, view);
        }
        match step % 3 {
            0 => service.stop(),
            1 => service.abandon(),
            _ => leaked.push(service),
        }
    }
    // Dead bytes for the compaction, then the swap with leaked services
    // still open (their kept blocks unreleased).
    {
        let service = store.open();
        let trash = store.fork("trash");
        let (answer, _, _) = commit_round(
            &service,
            trash,
            &[Op::Write("/dead".into(), random_bytes(77, 2 << 20))],
        );
        assert!(matches!(answer, Verdict::Committed(_)));
        service
            .call("branch.remove", vec![("branch", SdkValue::text("trash"))])
            .expect("trash removed");
        service.stop();
    }
    let swap = compact(&store, 5);
    log.push(format!("compaction: {swap}"));
    assert_eq!(
        swap, "Ok(rebuild)",
        "the compaction with idle leaked services"
    );
    // Phase two: the leaked (pre-swap) services, then new ones.
    for (n, service) in leaked.iter().enumerate() {
        let branch = branches[n % branches.len()];
        round += 1;
        salt += 1;
        let ops = round_ops(round, salt);
        match service.try_bind(branch) {
            Ok(workspace) => {
                for op in &ops {
                    apply(&workspace, op).unwrap();
                }
                let view = tree_of(&workspace);
                let answer = verdict(&commit(&workspace));
                log.push(format!("leaked {n} after swap: {answer:?}"));
                match &answer {
                    Verdict::Committed(_) => {
                        expected.insert(branch, view);
                    }
                    Verdict::Refused(text) => assert!(
                        text.contains("Stale") || text.contains("Busy"),
                        "leaked service after a swap: {text}"
                    ),
                    other => panic!("leaked service after a swap: {other:?}"),
                }
                let _ = service.try_unmount(workspace);
            }
            Err(error) => {
                log.push(format!("leaked {n} bind after swap: {error}"));
                assert!(error.contains("Stale") || error.contains("Busy"), "{error}");
            }
        }
    }
    for service in leaked {
        service.abandon();
    }
    for step in 0..3 {
        let service = store.open();
        for branch in &branches {
            round += 1;
            salt += 1;
            let (answer, view, _) = commit_round(&service, *branch, &round_ops(round, salt));
            assert!(
                matches!(answer, Verdict::Committed(_)),
                "post-swap {step}: {answer:?}"
            );
            expected.insert(*branch, view);
        }
        service.stop();
    }
    for line in &log {
        println!("VERIFY_KEPT {line}");
    }
    for (branch, tree) in &expected {
        assert_eq!(&store.tree(*branch), tree, "Branch {branch:?}");
    }
    let failures = verify768::duplicate_allocations(&store);
    assert!(failures.is_empty(), "allocation: {failures:#?}");
    let failures = store.integrity_failures();
    assert!(failures.is_empty(), "audit: {failures:#?}");
    let failures = verify768::amcheck(&store);
    assert!(failures.is_empty(), "amcheck: {failures:#?}");
}
