//! Independent writer-reopen tests: custody during the quarantine and the
//! reopen, races with removals and compaction from other sessions, queued
//! calls, uncertain Commits overtaken by another Workspace, a server that
//! goes away, and unmounts around a reopen. Every outcome is read from an
//! independent admin connection.
//!
//! PostgreSQL only. Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "../support/verify/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "../support/fault/proxy.rs"]
#[allow(dead_code)]
mod fault_proxy;
#[path = "../support/verify/reopen_rig.rs"]
#[allow(dead_code)]
mod testerwr;

use adversary::{assert_linear, commit, verdict, wait_until, write, Gate, Node, Verdict};
use fault_proxy::Fault;
use layerfs_history::CommitOutcome;
use std::time::Duration;
use testerwr::*;

/// With the writer's connection gone and no call yet, every live mount's
/// keys are gone with it; the rows alone must still refuse. After the
/// reopen, two mounts of one Branch keep two levels of its key: one unmount
/// still refuses the removal, the second admits it.
#[test]
fn rows_refuse_while_quarantined_and_key_levels_survive_the_reopen() {
    let Some(rig) = rig("wr_levels") else {
        return;
    };
    let (a, b) = (rig.store.fork("a"), rig.store.fork("b"));
    let (a1, a2, wb) = (
        rig.service.bind(a),
        rig.service.bind(a),
        rig.service.bind(b),
    );
    assert_eq!(rows(&rig.store, a), (2, 0));
    kill(&rig.store, rig.writer);
    assert_eq!(key_holders(&rig), vec![], "the keys went with the backend");
    assert_eq!(remove(&rig.store, "a"), "MountsActive", "rows refuse");
    assert_eq!(compact(&rig.store), "MountsActive", "rows refuse");
    write(&wb, "/b", b"b\n");
    committed(&wb, "the reopening Commit");
    assert_eq!(reopens(&rig.service), 1);
    let holders = key_holders(&rig);
    assert_eq!(holders.len(), 1, "{holders:?}");
    assert_eq!(holders[0].1, 4, "Store, project, a, b");
    rig.service.unmount(a1);
    assert_eq!(rows(&rig.store, a), (1, 0));
    assert_eq!(remove(&rig.store, "a"), "MountsActive", "one level left");
    assert_eq!(compact(&rig.store), "MountsActive");
    rig.service.unmount(a2);
    assert_eq!(remove(&rig.store, "a"), "Ok", "the last level released");
    assert_eq!(compact(&rig.store), "MountsActive", "b still mounted");
    rig.service.unmount(wb);
    assert_eq!(key_holders(&rig), vec![]);
    assert_eq!(compact(&rig.store), "Ok");
    assert_eq!(reopens(&rig.service), 1);
    rig.service.stop();
}

/// The reopen paused at every server message of its own settle steps
/// (and the first messages of the Commit after them): meanwhile a removal
/// and a compaction from other sessions are refused, and the keys are
/// either none or all of them, never a partial set.
#[test]
fn a_reopen_paused_at_every_message_never_admits_a_removal_or_compaction() {
    let Some(rig) = rig("wr_pause") else {
        return;
    };
    let (a, b) = (rig.store.fork("a"), rig.store.fork("b"));
    let held = rig.service.bind(a);
    let workspace = rig.service.bind(b);
    write(&workspace, "/warm", b"warm\n");
    committed(&workspace, "warm-up");
    // The Store, project, a and b keys, learned from the healthy writer;
    // the holders are found by these keys from then on.
    let keys = keys_of(&rig.store, writer_pid(&rig));
    assert_eq!(keys.len(), 4);
    let writer = |label: &str| -> i32 {
        let mut holders = Vec::new();
        wait_until(Duration::from_secs(10), || {
            holders = holders_of(&rig.store, &keys);
            holders.len() == 1
        });
        assert_eq!(holders.len(), 1, "{label}: one holder {holders:?}");
        assert_eq!(holders[0].1, 4, "{label}: all four keys {holders:?}");
        holders[0].0
    };
    // Calibration: the reopen's flight and the Commit's.
    kill(&rig.store, writer("calibration"));
    write(&workspace, "/cal", b"cal\n");
    rig.proxy.arm(None);
    committed(&workspace, "calibration");
    let n = rig.proxy.disarm().seen.min(56);
    let mut answers = std::collections::BTreeMap::<String, u64>::new();
    for step in 0..n {
        let label = format!("pause {step}");
        kill(&rig.store, writer(&label));
        write(&workspace, &format!("/p{step}"), b"p\n");
        rig.proxy.arm(Some((step, Fault::Pause)));
        let first = std::thread::scope(|scope| {
            let committer = scope.spawn(|| verdict(&commit(&workspace)));
            assert!(rig.proxy.wait_fired(Duration::from_secs(10)), "{label}");
            let holders = holders_of(&rig.store, &keys);
            let partial = holders.iter().any(|(_, held)| *held != 4) || holders.len() > 1;
            assert!(!partial, "{label}: a partial key set {holders:?}");
            let removal = remove(&rig.store, "a");
            let compaction = compact(&rig.store);
            *answers
                .entry(format!("remove {removal}, compact {compaction}"))
                .or_default() += 1;
            assert_ne!(removal, "Ok", "{label}: a removal under a live mount");
            assert_ne!(compaction, "Ok", "{label}: a compaction under a live mount");
            rig.proxy.release();
            committer.join().unwrap()
        });
        rig.proxy.disarm();
        assert!(matches!(first, Verdict::Committed(_)), "{label}: {first:?}");
    }
    println!("TESTERWR_PAUSE N={n} {answers:?}");
    assert_eq!(reopens(&rig.service), n + 1);
    assert_eq!(rows(&rig.store, a), (1, 0));
    rig.service.unmount(held);
    rig.service.unmount(workspace);
    assert_eq!(compact(&rig.store), "Ok");
    rig.service.stop();
}

/// Six calls from six threads hit a writer whose backend died: three
/// Commits, two mounts and an unmount. Exactly one reopen and one
/// connection; every call answers; the keys end on one backend.
#[test]
fn six_queued_calls_reopen_once() {
    let Some(rig) = rig("wr_queue") else {
        return;
    };
    let branches: Vec<_> = (0..6).map(|n| rig.store.fork(&format!("q{n}"))).collect();
    let workspaces: Vec<_> = branches[..3]
        .iter()
        .map(|branch| rig.service.bind(*branch))
        .collect();
    let leaving = rig.service.bind(branches[5]);
    for (n, workspace) in workspaces.iter().enumerate() {
        write(workspace, &format!("/w{n}"), b"w\n");
    }
    kill(&rig.store, rig.writer);
    let connects = writer_connects(&rig.service);
    let gate = Gate::new(6);
    let _watchdog = Watchdog::arm("six queued calls", Duration::from_secs(60));
    let (commits, mounts, unmounted) = std::thread::scope(|scope| {
        let gate = &gate;
        let commits: Vec<_> = (workspaces.iter())
            .map(|workspace| {
                scope.spawn(move || {
                    assert!(gate.wait());
                    verdict(&commit(workspace))
                })
            })
            .collect();
        let mounts: Vec<_> = (branches[3..5].iter())
            .map(|branch| {
                let service = &rig.service;
                scope.spawn(move || {
                    assert!(gate.wait());
                    service.try_bind(*branch)
                })
            })
            .collect();
        let service = &rig.service;
        let unmount = scope.spawn(move || {
            assert!(gate.wait());
            service.try_unmount(leaving)
        });
        (
            commits
                .into_iter()
                .map(|t| t.join().unwrap())
                .collect::<Vec<_>>(),
            mounts
                .into_iter()
                .map(|t| t.join().unwrap())
                .collect::<Vec<_>>(),
            unmount.join().unwrap(),
        )
    });
    for (n, answer) in commits.iter().enumerate() {
        assert!(
            matches!(answer, Verdict::Committed(_)),
            "commit {n}: {answer:?}"
        );
    }
    let mounts: Vec<_> = (mounts.into_iter())
        .map(|bound| bound.unwrap_or_else(|error| panic!("mount: {error}")))
        .collect();
    unmounted.unwrap();
    assert_eq!(reopens(&rig.service), 1, "one reopen for six calls");
    // The writer session's connects include its separate lane connection,
    // which a Commit racing to the separate path opens: printed, not judged.
    println!(
        "TESTERWR_QUEUE writer_connects +{} gate_attempts={}",
        writer_connects(&rig.service) - connects,
        rig.gate.attempts()
    );
    let holders = key_holders(&rig);
    assert_eq!(holders.len(), 1, "{holders:?}");
    assert_eq!(holders[0].1, 2 + 5, "Store, project and five Branches");
    assert_eq!(
        rows(&rig.store, branches[5]),
        (0, 0),
        "the unmount released"
    );
    for (n, branch) in branches[..3].iter().enumerate() {
        assert_eq!(
            rig.store.tree(*branch).get(&format!("/w{n}")),
            Some(&Node::File(b"w\n".to_vec()))
        );
    }
    for workspace in workspaces.into_iter().chain(mounts) {
        rig.service.unmount(workspace);
    }
    assert_eq!(all_rows(&rig.store), 0);
    assert_eq!(key_holders(&rig), vec![]);
    rig.service.stop();
}

/// The same six calls while the server refuses connections: each answers a
/// definite refusal and nothing is reopened; once the server is back, one
/// call reopens. Records how many connection attempts the calls made.
#[test]
fn queued_calls_against_a_refusing_server_fail_cleanly() {
    let Some(rig) = rig("wr_queue_down") else {
        return;
    };
    let branches: Vec<_> = (0..4).map(|n| rig.store.fork(&format!("d{n}"))).collect();
    let workspaces: Vec<_> = branches[..3]
        .iter()
        .map(|branch| rig.service.bind(*branch))
        .collect();
    for (n, workspace) in workspaces.iter().enumerate() {
        write(workspace, &format!("/w{n}"), b"w\n");
    }
    let heads: Vec<_> = branches[..3]
        .iter()
        .map(|b| rig.store.chain(*b).first().map(|c| c.id))
        .collect();
    rig.gate.refuse(true);
    kill(&rig.store, rig.writer);
    let attempts = rig.gate.attempts();
    let gate = Gate::new(4);
    let _watchdog = Watchdog::arm("refused queued calls", Duration::from_secs(60));
    let (commits, mount) = std::thread::scope(|scope| {
        let gate = &gate;
        let commits: Vec<_> = (workspaces.iter())
            .map(|workspace| {
                scope.spawn(move || {
                    assert!(gate.wait());
                    verdict(&commit(workspace))
                })
            })
            .collect();
        let service = &rig.service;
        let branch = branches[3];
        let mount = scope.spawn(move || {
            assert!(gate.wait());
            service.try_bind(branch)
        });
        (
            commits
                .into_iter()
                .map(|t| t.join().unwrap())
                .collect::<Vec<_>>(),
            mount.join().unwrap(),
        )
    });
    let made = rig.gate.attempts() - attempts;
    println!("TESTERWR_REFUSED_ATTEMPTS calls=4 attempts={made} commits={commits:?}");
    for (n, answer) in commits.iter().enumerate() {
        assert!(
            matches!(answer, Verdict::Refused(_)),
            "commit {n}: {answer:?}"
        );
    }
    assert!(mount.is_err(), "the mount is refused");
    assert_eq!(reopens(&rig.service), 0);
    assert_eq!(
        rows(&rig.store, branches[3]),
        (0, 0),
        "no row for a refused mount"
    );
    for (n, branch) in branches[..3].iter().enumerate() {
        assert_eq!(rig.store.chain(*branch).first().map(|c| c.id), heads[n]);
        assert_eq!(rows(&rig.store, *branch), (1, 0));
    }
    assert!(made >= 4, "every call tried: {made}");
    rig.gate.refuse(false);
    for workspace in &workspaces {
        committed(workspace, "after the server is back");
    }
    assert_eq!(reopens(&rig.service), 1);
    let holders = key_holders(&rig);
    assert_eq!(holders.len(), 1);
    assert_eq!(holders[0].1, 2 + 3);
    for workspace in workspaces {
        rig.service.unmount(workspace);
    }
    rig.service.stop();
}

/// Workspace `x` loses its Commit's reply after the server committed; then
/// Workspace `y` on the same Branch commits first on the reopened writer.
/// `x`'s Commit resolves installed once, `y`'s Commit is on top of it, and
/// no Commit id appears twice.
#[test]
fn a_lost_commit_overtaken_on_the_same_branch_is_installed_once() {
    let Some(rig) = rig("wr_overtake") else {
        return;
    };
    let k = commit_flight(&rig, rig.store.fork("twin"));
    let branch = rig.store.fork("shared");
    let x = rig.service.bind(branch);
    let y = rig.service.bind(branch);
    write(&x, "/warm", b"warm\n");
    let warm = committed(&x, "warm-up");
    write(&x, "/x", b"x\n");
    rig.proxy.arm(Some((k - 1, Fault::Drop)));
    let lost = verdict(&commit(&x));
    assert!(rig.proxy.disarm().fired.is_some());
    assert!(matches!(lost, Verdict::Unknown(_)), "{lost:?}");
    let head = rig.store.chain(branch)[0].clone();
    assert_eq!(head.parent, Some(warm), "the server committed x");
    write(&y, "/y", b"y\n");
    let yid = committed(&y, "y overtakes on the reopened writer");
    assert_eq!(reopens(&rig.service), 1);
    let resolution = x
        .resolve_retained()
        .unwrap_or_else(|unresolved| panic!("{unresolved:?}"))
        .expect("retained");
    let Some(CommitOutcome::Committed { record, .. }) = resolution.outcome else {
        panic!("x resolved: {:?}", resolution.outcome);
    };
    assert_eq!(record.id, head.id, "x's own Commit, found not resent");
    let follow = verdict(&commit(&x));
    let chain = rig.store.chain(branch);
    assert_linear(&chain);
    println!(
        "TESTERWR_OVERTAKE follow={follow:?} chain={:?}",
        chain.iter().map(|c| c.id).collect::<Vec<_>>()
    );
    assert_eq!(chain[0].id, yid, "y is the head: {follow:?}");
    assert_eq!(chain[1].id, record.id);
    assert_eq!(chain[2].id, warm);
    assert!(
        matches!(follow, Verdict::UpToDate),
        "x publishes nothing again"
    );
    let rows_total = commit_rows(&rig.store) as usize;
    let twin = rig.store.chain(rig.store.branch("twin")).len();
    assert_eq!(rows_total, chain.len() + twin, "no stray Commit row");
    let tree = rig.store.tree(branch);
    assert_eq!(tree.get("/x"), Some(&Node::File(b"x\n".to_vec())));
    assert_eq!(tree.get("/y"), Some(&Node::File(b"y\n".to_vec())));
    rig.service.unmount(x);
    rig.service.unmount(y);
    rig.service.stop();
}

/// Workspace `x`'s Commit is cut while its message waits before the lane
/// (the server never commits it); `y` on the same Branch commits first on
/// the reopened writer, whose reopen ends `x`'s old backend. `x` folds back
/// and its next Commit lands its change once, on top of `y`.
#[test]
fn a_cut_commit_overtaken_on_the_same_branch_folds_back_and_lands_once() {
    let Some(rig) = rig("wr_overtake_cut") else {
        return;
    };
    let (_, schema) = rig.store.postgres.clone().unwrap();
    let branch = rig.store.fork("shared");
    let x = rig.service.bind(branch);
    let y = rig.service.bind(branch);
    write(&x, "/warm", b"warm\n");
    let warm = committed(&x, "warm-up");
    let old = writer_pid(&rig);
    let payload = adversary::random_bytes(17, 64 * 1024);
    write(&x, "/x", &payload);
    let mut holder = rig.store.admin();
    holder
        .batch_execute("BEGIN; LOCK TABLE pack IN SHARE MODE")
        .unwrap();
    let mut observer = rig.store.admin();
    let pattern = format!("%INTO {schema}.pack %");
    let first = std::thread::scope(|scope| {
        let proxy = &rig.proxy;
        let cutter = scope.spawn(move || {
            let waiting = wait_until(Duration::from_secs(20), || {
                observer
                    .query_opt(
                        "SELECT 1 FROM pg_stat_activity WHERE pid = $1 \
                          AND wait_event_type = 'Lock' AND query LIKE $2",
                        &[&old, &pattern],
                    )
                    .unwrap()
                    .is_some()
            });
            assert!(proxy.cut(old), "cut");
            waiting
        });
        let first = verdict(&commit(&x));
        assert!(cutter.join().unwrap(), "x waited at the hold");
        first
    });
    assert!(matches!(first, Verdict::Unknown(_)), "{first:?}");
    assert!(alive(&rig.store, old), "x's old backend still waits");
    write(&y, "/y", b"y\n");
    let yid = std::thread::scope(|scope| {
        let releaser = scope.spawn(|| {
            let gone = wait_until(Duration::from_secs(20), || !alive(&rig.store, old));
            holder.batch_execute("ROLLBACK").unwrap();
            gone
        });
        let yid = committed(&y, "y overtakes");
        assert!(releaser.join().unwrap(), "the reopen ended x's old backend");
        yid
    });
    assert_eq!(reopens(&rig.service), 1);
    assert_eq!(rig.store.chain(branch)[0].parent, Some(warm), "y on warm");
    let resolution = x
        .resolve_retained()
        .unwrap_or_else(|unresolved| panic!("{unresolved:?}"))
        .expect("retained");
    assert!(resolution.outcome.is_none(), "x was not committed");
    let xid = committed(&x, "x carries its change");
    let chain = rig.store.chain(branch);
    assert_linear(&chain);
    assert_eq!(
        chain.iter().take(3).map(|c| c.id).collect::<Vec<_>>(),
        vec![xid, yid, warm]
    );
    let tree = rig.store.tree(branch);
    assert_eq!(tree.get("/x"), Some(&Node::File(payload)));
    assert_eq!(tree.get("/y"), Some(&Node::File(b"y\n".to_vec())));
    let twin = 0;
    assert_eq!(commit_rows(&rig.store) as usize, chain.len() + twin);
    rig.service.unmount(x);
    rig.service.unmount(y);
    rig.service.stop();
    assert!(rig.store.no_open_transaction());
}

/// Every backend of the service (writer and readers) ends and the server
/// refuses connections: calls fail definitely, rows still refuse removals;
/// once the server is back, the next Commit reopens once and the keys are
/// taken again.
#[test]
fn the_server_going_away_entirely_and_coming_back() {
    let Some(rig) = rig("wr_down") else {
        return;
    };
    let (a, b) = (rig.store.fork("a"), rig.store.fork("b"));
    let held = rig.service.bind(a);
    let workspace = rig.service.bind(b);
    write(&workspace, "/warm", b"warm\n");
    let warm = committed(&workspace, "warm-up");
    rig.gate.refuse(true);
    for pid in live_proxied(&rig) {
        kill(&rig.store, pid);
    }
    write(&workspace, "/down", b"down\n");
    let attempts = rig.gate.attempts();
    let refused = verdict(&commit(&workspace));
    let made = rig.gate.attempts() - attempts;
    println!("TESTERWR_DOWN commit attempts={made}: {refused:?}");
    assert!(matches!(refused, Verdict::Refused(_)), "{refused:?}");
    let mount = rig.service.try_bind(rig.store.fork("c"));
    assert!(mount.is_err(), "no mount while down");
    assert_eq!(reopens(&rig.service), 0);
    assert_eq!(key_holders(&rig), vec![]);
    assert_eq!(rows(&rig.store, a), (1, 0));
    assert_eq!(remove(&rig.store, "a"), "MountsActive");
    assert_eq!(compact(&rig.store), "MountsActive");
    rig.gate.refuse(false);
    let attempts = rig.gate.attempts();
    let answer = verdict(&commit(&workspace));
    let made = rig.gate.attempts() - attempts;
    println!("TESTERWR_BACK commit attempts={made}: {answer:?}");
    let Verdict::Committed(id) = answer else {
        panic!("the Commit after the server is back: {answer:?}");
    };
    assert_eq!(reopens(&rig.service), 1);
    let chain = rig.store.chain(b);
    assert_eq!((chain[0].id, chain[0].parent), (id, Some(warm)));
    let holders = key_holders(&rig);
    assert_eq!(holders.len(), 1);
    assert_eq!(holders[0].1, 4, "Store, project, a, b again");
    rig.service.unmount(held);
    rig.service.unmount(workspace);
    assert_eq!(all_rows(&rig.store), 0);
    rig.service.stop();
}

/// An unmount whose release cannot reach the server (the reopen it needs
/// is refused): the Workspace's overlay is closed and the call fails. Once
/// the server is back and another call reopens the writer, the closed
/// Workspace must not be resurrected: its keys must not be taken again for
/// the service's lifetime, and its row must not stay a live mount that
/// refuses removals and compaction with nothing mounted.
#[test]
fn an_unmount_whose_reopen_is_refused_leaves_no_live_mount_behind() {
    let Some(rig) = rig("wr_unmount_down") else {
        return;
    };
    let (a, b) = (rig.store.fork("a"), rig.store.fork("b"));
    let leaving = rig.service.bind(a);
    let workspace = rig.service.bind(b);
    rig.gate.refuse(true);
    kill(&rig.store, rig.writer);
    let attempts = rig.gate.attempts();
    let failed = rig.service.try_unmount(leaving);
    let made = rig.gate.attempts() - attempts;
    println!("TESTERWR_UNMOUNT_DOWN attempts={made}: {failed:?}");
    assert!(failed.is_err(), "the release could not be sent");
    rig.gate.refuse(false);
    write(&workspace, "/b", b"b\n");
    committed(&workspace, "a Commit reopens the writer");
    assert_eq!(reopens(&rig.service), 1);
    let holders = key_holders(&rig);
    let a_rows = rows(&rig.store, a);
    let removal = remove(&rig.store, "a");
    rig.service.unmount(workspace);
    let after = key_holders(&rig);
    let compaction = compact(&rig.store);
    println!(
        "TESTERWR_UNMOUNT_DOWN after reopen: holders={holders:?} a_rows={a_rows:?} \
         remove_a={removal} after_all_unmounted: holders={after:?} compact={compaction}"
    );
    // An operator's release of the row from its own session (it cannot
    // unlock the owner session's keys): with nothing mounted and no row
    // left, compaction must be admitted.
    let ops: Vec<Vec<u8>> = (rig.store.admin())
        .query("SELECT op_id FROM gc_intent WHERE state <> 'done'", &[])
        .unwrap()
        .iter()
        .map(|row| row.get(0))
        .collect();
    let handles = rig.store.handles();
    for op in &ops {
        let op: [u8; 16] = op.as_slice().try_into().unwrap();
        adversary::history(&handles).gc_release(&op).unwrap();
    }
    handles.close();
    let repaired = compact(&rig.store);
    println!(
        "TESTERWR_UNMOUNT_DOWN operator released {} rows: rows_left={} compact={repaired} holders={:?}",
        ops.len(),
        all_rows(&rig.store),
        key_holders(&rig)
    );
    assert_eq!(
        after,
        vec![],
        "keys held for a Workspace that was unmounted (a_rows {a_rows:?}, remove {removal}, \
         compaction after an operator release: {repaired})"
    );
    assert_ne!(
        a_rows.0, 1,
        "a closed Workspace's row stays live: remove {removal}"
    );
    rig.service.stop();
}

/// Known limit (documented): an unmount's release whose reply is lost. (1)
/// Lost after the server ran it: the row is gone and the reopen takes none
/// of its keys. (2) Cut while its delete waits on a row lock, so it never
/// runs: the row stays (live or uncertain), is never deleted a second
/// time, none of its keys are taken again, and the service's other mount
/// keeps its keys; another Branch can still be removed.
#[test]
fn a_lost_unmount_reply_is_exactly_the_documented_limit() {
    let Some(rig) = rig("wr_unmount_lost") else {
        return;
    };
    let (a, b, c, d) = (
        rig.store.fork("a"),
        rig.store.fork("b"),
        rig.store.fork("c"),
        rig.store.fork("d"),
    );
    let stays = rig.service.bind(b);
    // (1) The release's message is the writer's whole unmount flight;
    // count it on a twin, then drop its last reply.
    let twin = rig.service.bind(d);
    rig.service.unmount(twin);
    let twin = rig.service.bind(d);
    rig.proxy.arm(None);
    rig.service.unmount(twin);
    let window = rig.proxy.disarm();
    let k = window.seen;
    let lost = rig.service.bind(a);
    rig.proxy.arm(Some((k - 1, Fault::Drop)));
    let answer = rig.service.try_unmount(lost);
    let window = rig.proxy.disarm();
    assert!(window.fired.is_some());
    println!("TESTERWR_UNMOUNT_LOST_AFTER {answer:?}");
    assert_eq!(rows(&rig.store, a), (0, 0), "the server deleted it");
    // The unmount's own mark-uncertain may already have reopened the
    // writer; either way the next call serves on one.
    let probe = rig.service.bind(c);
    rig.service.unmount(probe);
    let holders = key_holders(&rig);
    assert_eq!(holders.len(), 1);
    assert_eq!(holders[0].1, 3, "only b's keys: a's are not taken again");
    assert_eq!(remove(&rig.store, "a"), "Ok");
    // (2) The delete waits on the row, the connection is cut.
    let lost = rig.service.bind(c);
    let old = writer_pid(&rig);
    let (_, schema) = rig.store.postgres.clone().unwrap();
    let mut holder = rig.store.admin();
    holder
        .batch_execute(&format!(
            "BEGIN; SELECT 1 FROM {schema}.gc_intent WHERE scope_kind = 'branch' FOR UPDATE"
        ))
        .unwrap();
    let mut observer = rig.store.admin();
    let answer = std::thread::scope(|scope| {
        let proxy = &rig.proxy;
        let cutter = scope.spawn(move || {
            let waiting = wait_until(Duration::from_secs(20), || {
                observer
                    .query_opt(
                        "SELECT 1 FROM pg_stat_activity WHERE pid = $1 \
                          AND wait_event_type = 'Lock'",
                        &[&old],
                    )
                    .unwrap()
                    .is_some()
            });
            assert!(proxy.cut(old), "cut");
            waiting
        });
        let releaser = scope.spawn(|| {
            // Released once the old backend is gone (a reopen ended it), or
            // after a bound.
            wait_until(Duration::from_secs(20), || !alive(&rig.store, old));
            holder.batch_execute("ROLLBACK").unwrap();
        });
        let answer = rig.service.try_unmount(lost);
        assert!(cutter.join().unwrap(), "the delete waited");
        releaser.join().unwrap();
        answer
    });
    let c_rows = rows(&rig.store, c);
    println!("TESTERWR_UNMOUNT_LOST_BEFORE {answer:?} rows={c_rows:?}");
    assert!(answer.is_err(), "an unknown release");
    assert!(!alive(&rig.store, old), "the old backend was ended");
    assert_eq!(c_rows.0 + c_rows.1, 1, "the row was never deleted");
    let probe = rig.service.bind(d);
    rig.service.unmount(probe);
    assert_eq!(c_rows, rows(&rig.store, c), "not deleted a second time");
    let holders = key_holders(&rig);
    assert_eq!(holders.len(), 1);
    assert_eq!(holders[0].1, 3, "only b's keys: c's are not taken again");
    assert_eq!(remove(&rig.store, "d"), "Ok", "another Branch is removable");
    let removal = remove(&rig.store, "c");
    let compaction = compact(&rig.store);
    println!("TESTERWR_UNMOUNT_LOST_BEFORE remove_c={removal} compact={compaction}");
    assert_ne!(removal, "Ok", "the documented limit: the row stays");
    rig.service.unmount(stays);
    rig.service.stop();
}

/// A compaction from another service moves the generation while the
/// writer (with nothing mounted) is quarantined: the reopen answers a
/// definite refusal each time, one connection per call, nothing half-done.
#[test]
fn a_generation_moved_during_the_quarantine_refuses_cleanly() {
    let Some(rig) = rig("wr_stale") else {
        return;
    };
    let a = rig.store.fork("a");
    // Garbage for the compaction to collect: a Branch with its own content,
    // removed again.
    let garbage = rig.store.fork("g");
    let workspace = rig.service.bind(garbage);
    write(&workspace, "/g", &adversary::random_bytes(5, 256 * 1024));
    committed(&workspace, "garbage");
    rig.service.unmount(workspace);
    assert_eq!(remove(&rig.store, "g"), "Ok");
    let probe = rig.service.bind(a);
    rig.service.unmount(probe);
    kill(&rig.store, rig.writer);
    let before = generation(&rig.store);
    assert_eq!(compact(&rig.store), "Ok", "nothing mounted: admitted");
    let after = generation(&rig.store);
    println!("TESTERWR_STALE generation {before} -> {after}");
    for attempt in 0..2 {
        let attempts = rig.gate.attempts();
        let answer = rig.service.try_bind(a);
        let made = rig.gate.attempts() - attempts;
        println!(
            "TESTERWR_STALE {attempt} attempts={made}: {:?}",
            answer.as_ref().err()
        );
        assert!(answer.is_err(), "a moved generation refuses");
        assert!(made <= 1, "one connection per call");
        assert_eq!(reopens(&rig.service), 0);
        assert_eq!(key_holders(&rig), vec![], "no key left behind");
        assert_eq!(all_rows(&rig.store), 0, "no row left behind");
    }
    rig.service.abandon();
}

/// Other handles keep the quarantine: a persistence handle whose backend
/// died fails its call and never opens a new connection.
#[test]
fn a_persistence_handle_still_refuses_and_never_reconnects() {
    let Some(rig) = rig("wr_handles") else {
        return;
    };
    let (url, _) = rig.store.postgres.clone().unwrap();
    let (gated, _) = fault_proxy::proxied_url(&url, rig.gate.port);
    let selection = rig.store.selection_via(&gated);
    let before = rig.proxy.pids().len();
    let handles = layerfs_persistence::Handles::open_writable(
        &selection,
        &rig.store.binding,
        adversary::CURSOR,
    )
    .unwrap();
    let pids: Vec<i32> = rig.proxy.pids()[before..]
        .iter()
        .flatten()
        .copied()
        .collect();
    for pid in &pids {
        kill(&rig.store, *pid);
    }
    let attempts = rig.gate.attempts();
    let fork = |name: &str| {
        adversary::history(&handles).fork(&layerfs_history::ForkRequest {
            stack: rig.store.stack,
            branch: rig.store.branch(name),
            name: layerfs_history::HistoryName::new(name).unwrap(),
            source: layerfs_history::ForkSource::Layer(rig.store.layer),
        })
    };
    let first = fork("h1");
    let second = fork("h2");
    println!("TESTERWR_HANDLES {first:?} {second:?}");
    assert!(first.is_err() && second.is_err());
    assert_eq!(rig.gate.attempts() - attempts, 0, "no new connection");
    handles.close();
    rig.service.stop();
}

/// SQLite is unchanged: a Store service's mount, Commit and unmount work
/// and its writer never counts a reopen.
#[test]
fn sqlite_counts_no_reopen() {
    let store = adversary::TestStore::sqlite("wr_sqlite", SEED);
    let service = store.open();
    let branch = store.fork("s");
    let workspace = service.bind(branch);
    write(&workspace, "/s", b"s\n");
    let id = committed(&workspace, "SQLite Commit");
    assert_eq!(store.chain(branch)[0].id, id);
    service.unmount(workspace);
    assert_eq!(reopens(&service), 0);
    service.stop();
}
