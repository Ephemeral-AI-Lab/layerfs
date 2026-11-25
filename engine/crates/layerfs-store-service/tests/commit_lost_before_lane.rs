//! #803 review: a fused Workspace Commit whose reply is lost while the
//! server is still on the message's second step, the pack insert, which
//! comes before the lane row. The test holds `pack` in `SHARE` mode, so the
//! insert waits; then the writer's connection is cut at the proxy (the
//! client sees no reply: an unknown outcome, the Commit retained), the
//! retained Commit is resolved while the insert still waits, and only then
//! is the hold released.
//!
//! Resolution must answer exactly: either committed (installed, and the
//! Branch holds that Commit) or not committed (folded back, and nothing
//! lands afterwards). A lane pass alone answered "not committed" here and
//! the server then committed: the lane row was not yet taken. The writer's
//! backend is now ended first (`pg_terminate_backend`, waited for), so the
//! insert is aborted and nothing lands.
//!
//! PostgreSQL only: a SQLite publication runs in this process, so once its
//! call has answered nothing of it applies later and there is no such
//! window. Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "support/fault_proxy.rs"]
#[allow(dead_code)]
mod fault_proxy;

use adversary::{commit, verdict, wait_until, write, Service, TestStore, Verdict};
use fault_proxy::FaultProxy;
use layerfs_history::CommitOutcome;
use std::time::Duration;

const SEED: &[(&str, &[u8])] = &[("/seed", b"seed\n")];

#[test]
fn a_reply_lost_before_the_lane_resolves_exactly() {
    let Some(store) = TestStore::postgres("lostlane", SEED) else {
        return;
    };
    let (url, schema) = store.postgres.clone().unwrap();
    let (proxy, proxied) = FaultProxy::start(&url);
    // The default lane wait (30 s) also bounds the held insert's lock
    // wait: the hold below ends well before it.
    let selection = store.with_lock_timeout(&store.selection_via(&proxied), 30_000);
    let branch = store.fork("lost");
    let service = Service::open(&store, selection);
    let workspace = service.bind(branch);
    write(&workspace, "/warm", b"warm\n");
    let Verdict::Committed(_) = verdict(&commit(&workspace)) else {
        panic!("warm-up Commit");
    };
    let before: Vec<_> = store.chain(branch).iter().map(|c| c.id).collect();
    write(&workspace, "/fault", &adversary::random_bytes(7, 64 * 1024));

    let mut holder = store.admin();
    holder
        .batch_execute("BEGIN; LOCK TABLE pack IN SHARE MODE")
        .unwrap();
    let mut observer = store.admin();
    let pattern = format!("%INTO {schema}.pack %");
    let (first, writer) = std::thread::scope(|scope| {
        let proxy = &proxy;
        let cutter = scope.spawn(move || {
            let mut writer = None;
            let waiting = wait_until(Duration::from_secs(20), || {
                writer = observer
                    .query_opt(
                        "SELECT pid FROM pg_stat_activity \
                          WHERE wait_event_type = 'Lock' AND query LIKE $1 LIMIT 1",
                        &[&pattern],
                    )
                    .unwrap()
                    .map(|row| row.get::<_, i32>(0));
                writer.is_some()
            });
            // Cut even when the wait was not seen, so the Commit cannot
            // wait for the hold.
            let pid = writer.unwrap_or_default();
            let cut = proxy.cut(pid);
            (waiting && cut, pid, observer)
        });
        let first = verdict(&commit(&workspace));
        (first, cutter.join().unwrap())
    });
    let (cut, writer, mut observer) = writer;
    assert!(
        cut,
        "the fused message waited at its pack insert and was cut"
    );
    assert!(
        matches!(first, Verdict::Unknown(_)),
        "a lost reply: {first:?}"
    );
    assert!(workspace.has_retained(), "the unknown Commit is retained");

    // Resolved while the insert still waits for the hold.
    let resolution = workspace.resolve_retained();
    holder.batch_execute("ROLLBACK").unwrap();
    let gone = wait_until(Duration::from_secs(20), || {
        observer
            .query_opt("SELECT 1 FROM pg_stat_activity WHERE pid = $1", &[&writer])
            .unwrap()
            .is_none()
    });
    assert!(gone, "the writer's backend ended");
    let after: Vec<_> = store.chain(branch).iter().map(|c| c.id).collect();
    let resolution = resolution.unwrap_or_else(|unresolved| panic!("{unresolved:?}"));
    let resolution = resolution.expect("a retained Commit was resolved");
    match &resolution.outcome {
        None => assert_eq!(
            after, before,
            "resolved not committed: nothing may land afterwards"
        ),
        Some(CommitOutcome::Committed { record, .. }) => {
            assert_eq!(after.first(), Some(&record.id), "resolved committed");
            assert_eq!(after.len(), before.len() + 1);
        }
        Some(other) => panic!("resolved {other:?}"),
    }
    // The quarantined writer session serves nothing more; the Branch's tree
    // read by a fresh service agrees with the resolution.
    let _ = service.try_unmount(workspace);
    service.abandon();
    let landed = store.tree(branch).contains_key("/fault");
    assert_eq!(landed, resolution.outcome.is_some(), "the Branch's tree");
    assert!(store.no_open_transaction());
}
