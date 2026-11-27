//! A release that quarantines the writer before its delete is sent leaves
//! its row an orphan the next reopen deletes (review round 2, V1).
//!
//! The writer here never prepared its release statements (the Store
//! service prepares them at open; a full statement cache clears them the
//! same way), so the release's flight starts with their prepare. The proxy
//! drops that prepare's reply: nothing of the release was sent, the writer
//! is quarantined and its keys go with the connection. The release took the
//! op from the session's held ops before the flight, so the row must be
//! recorded as an orphan right there; otherwise no reopen deletes it and a
//! closed Workspace's row refuses removals and compaction.
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
#[path = "../support/verify/reopen_second_round.rs"]
#[allow(dead_code)]
mod testerwr2;

use adversary::{TestStore, CURSOR};
use fault_proxy::{Fault, FaultProxy};
use layerfs_persistence::internals::register_separately;
use layerfs_persistence::Handles;
use std::time::Duration;
use testerwr::{all_rows, compact, Watchdog, LOCK_TIMEOUT_MS, SEED};
use testerwr2::*;

/// The first prepare reply of the release's flight (see the test).
const PREPARE_REPLY: u64 = 3;

/// [`rig2`] without the mount statements prepared at open: the release's
/// statements are prepared inside its own flight.
fn cold(label: &str) -> Option<Rig2> {
    let store = TestStore::postgres(label, SEED)?;
    let (url, _) = store.postgres.clone().unwrap();
    let (proxy, proxied) = FaultProxy::start(&url);
    let selection = store.with_lock_timeout(&store.selection_via(&proxied), LOCK_TIMEOUT_MS);
    let Handles::Postgres(handles) =
        Handles::open_writable(&selection, &store.binding, CURSOR).unwrap()
    else {
        unreachable!()
    };
    handles.storage.enable_reopen();
    Some(Rig2 {
        store,
        proxy,
        handles,
    })
}

#[test]
fn a_release_whose_prepare_reply_is_lost_leaves_an_orphan_the_reopen_deletes() {
    let Some(rig) = cold("wr_v1_release") else {
        return;
    };
    let _dog = Watchdog::arm("v1", Duration::from_secs(100));
    let branch = rig.store.fork("a");
    let intent = mount(&rig.store, branch, "a", 0xC1);
    register_separately(&rig.handles.history, &intent).unwrap();
    assert_eq!(op_rows(&rig.store, &intent.op_id).len(), 1, "registered");
    let reopens_before = reopens(&rig);
    // The release's flight, unfaulted: its prepares in one read transaction
    // (`BEGIN` and `SET LOCAL`: C C Z; per statement 1 t T Z; `COMMIT`: C Z),
    // then the delete and the unlock (2 D C Z each). Message 3 is the
    // delete's ParseComplete.
    rig.proxy.arm(Some((PREPARE_REPLY, Fault::Drop)));
    let released = rig.history().gc_release(&intent.op_id);
    let window = rig.proxy.disarm();
    let fired = window.fired.clone().expect("the prepare's reply dropped");
    let rows = op_rows(&rig.store, &intent.op_id);
    println!(
        "WR_V1 release {released:?}; fired at {} ({}); window {:?}; rows {rows:?}",
        fired.at, fired.kind, window.log
    );
    // `1` is ParseComplete: the dropped reply was the prepare's, so the
    // delete itself was never sent.
    assert_eq!(fired.kind, '1', "the dropped reply was a prepare's");
    assert!(released.is_err(), "a lost prepare reply: {released:?}");
    assert_eq!(rows.len(), 1, "the delete was never sent: {rows:?}");
    // The next call reopens; its settle deletes the orphaned row.
    let next = read(&rig, branch);
    assert!(next.is_ok(), "the next call reopens: {next:?}");
    assert_eq!(reopens(&rig) - reopens_before, 1, "one reopen");
    let rows = op_rows(&rig.store, &intent.op_id);
    assert!(
        rows.is_empty(),
        "the released op's row survived the reopen: {rows:?}"
    );
    assert_eq!(all_rows(&rig.store), 0, "no row left");
    assert_eq!(writer_keys(&rig).1, Vec::<i64>::new(), "no key held");
    assert_eq!(compact(&rig.store), "Ok", "compaction admitted");
    rig.handles.close();
}
