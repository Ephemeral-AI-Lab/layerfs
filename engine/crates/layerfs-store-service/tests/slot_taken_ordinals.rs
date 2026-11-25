//! #768: two Workspaces on one service, each mounting, committing one small
//! file and unmounting per cycle, lose no ordinal in steady state, whichever
//! unmounts first, on SQLite and on PostgreSQL: over five cycles and the
//! service's close at most one kept block (`KEPT_ORDINALS`) is lost.
//!
//! The second unmount finds the Store's idle slot taken. Of the two idle
//! handles, only the one whose kept ordinals end later can still end the
//! Store's counter, so that one is released (its conditional rewind
//! applies) and the other takes the slot. Before, the closing Workspace's
//! handle was always released: when it was the older one (the Workspaces
//! unmounted in reverse order) its 128 kept ordinals were not adjacent to
//! the counter and every cycle lost them.
//!
//! The PostgreSQL half needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;

use adversary::{commit, verdict, write, Kind, Service, TestStore, Verdict};
use layerfs_history::BranchId;
use layerfs_persistence::StoreSelection;

const SEED: &[(&str, &[u8])] = &[("/seed", b"seed\n")];

/// `(store_policy.next_ordinal, ordinals stored values took)`.
fn counts(store: &TestStore) -> (i64, i64) {
    const NEXT: &str = "SELECT next_ordinal FROM store_policy WHERE id = 1";
    const USED: &str = "SELECT COALESCE(SUM(last_ordinal - first_ordinal + 1), 0)::bigint \
                        FROM inode_value_block";
    match (&store.selection, store.kind) {
        (StoreSelection::Sqlite(config), Kind::Sqlite) => {
            let sql = rusqlite::Connection::open(&config.path).unwrap();
            let used = USED.replace("::bigint", "");
            (
                sql.query_row(NEXT, [], |row| row.get(0)).unwrap(),
                sql.query_row(&used, [], |row| row.get(0)).unwrap(),
            )
        }
        _ => {
            let mut admin = store.admin();
            (
                admin.query_one(NEXT, &[]).unwrap().get(0),
                admin.query_one(USED, &[]).unwrap().get(0),
            )
        }
    }
}

/// Mount both, one small Commit each, unmount in reverse order.
fn cycle(service: &Service, branches: &[BranchId; 2], n: usize) {
    let workspaces: Vec<_> = branches.iter().map(|b| service.bind(*b)).collect();
    for (i, workspace) in workspaces.iter().enumerate() {
        write(
            workspace,
            &format!("/f{n}"),
            format!("{i} {n}\n").as_bytes(),
        );
        let answer = verdict(&commit(workspace));
        assert!(matches!(answer, Verdict::Committed(_)), "{answer:?}");
    }
    for workspace in workspaces.into_iter().rev() {
        service.unmount(workspace);
    }
}

#[test]
fn two_workspaces_per_call_lose_no_ordinals_in_steady_state() {
    adversary::tmp();
    for store in TestStore::both("slot-taken", SEED) {
        let kind = store.kind;
        let service = store.open();
        let branches = [store.fork("w0"), store.fork("w1")];
        for n in 0..5 {
            cycle(&service, &branches, n);
        }
        service.service.close().unwrap();
        service.owner.stop().unwrap();
        // Every ordinal below the counter that no stored value took is lost
        // (Init leaves none). One block of at most `KEPT_ORDINALS` may go
        // once per service (a handle released when another reserved after
        // it); before, 128 more went every cycle.
        let (next, used) = counts(&store);
        let lost = next - 1 - used;
        assert!(
            lost <= layerfs_storage::policy::KEPT_ORDINALS as i64,
            "{kind:?}: {lost} ordinals lost over five cycles (next {next}, used {used})"
        );
    }
}
