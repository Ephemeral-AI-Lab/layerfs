//! #768: the idle Workspace reader a mount takes is checked locally, and
//! with one empty query once its connection has been silent (no reply from
//! the server) for `reader_idle_check_ms` (30 s by default,
//! `READER_IDLE_CHECK_MS`), so a network path that ended the idle
//! connection without telling the client costs the mount a fresh reader,
//! not a failed or stalled first read.
//!
//! * A reader idle under the threshold is handed over with nothing sent.
//! * An aged reader whose path resets at the next packet (the proxy drops
//!   the connection at the check's reply) is replaced by a fresh reader.
//! * A reader that went silent past the threshold while a Workspace held
//!   it is checked at its next checkout, though it spent almost no time in
//!   the pool: silence counts from the server's last reply.
//! * An aged reader whose backend was terminated while the path hid it (the
//!   proxy holds every later byte, as a silent drop does) is replaced within
//!   the check's 5 s bound (`READER_CHECK_TIMEOUT`).
//!
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
use crate::adversary;
use crate::fault_proxy;

use adversary::TestStore;
use fault_proxy::{Fault, FaultProxy};
use layerfs_history::WorkspaceId;
use layerfs_persistence::{StoreSelection, READER_CHECK_TIMEOUT, READER_IDLE_CHECK_MS};
use layerfs_storage::ReservationBlocks;
use layerfs_store_service::store::open::{open_store_observed, OpenedStore};
use std::time::{Duration, Instant};

/// The threshold of the aged cases: short, so a test ages a reader quickly.
const SHORT_MS: u32 = 200;

fn ws(n: u8) -> WorkspaceId {
    WorkspaceId::from_authority([n; 32]).unwrap()
}

/// A Store service dialing through `proxy`, with readers checked by query
/// after `idle_check_ms`; its idle reader prefilled. Returns the service and
/// the proxy's connection index of that reader.
fn opened(store: &TestStore, proxy: &FaultProxy, idle_check_ms: u32) -> (OpenedStore, usize) {
    let (url, _) = store.postgres.as_ref().unwrap();
    let proxied = fault_proxy::proxied_url(url, proxy.port).0;
    let StoreSelection::Postgres(mut selection) = store.selection_via(&proxied) else {
        unreachable!("a PostgreSQL Store");
    };
    selection.reader_idle_check_ms = idle_check_ms;
    let opened = open_store_observed(
        StoreSelection::Postgres(selection),
        &store.binding,
        [7; 32],
        4,
        64 * 1024 * 1024,
        ReservationBlocks::default(),
    )
    .unwrap();
    let index = proxy.pids().len();
    assert!(matches!(opened.store.prefill_reader(), Some(Ok(()))));
    assert_eq!(
        proxy.pids().len(),
        index + 1,
        "the idle reader's connection"
    );
    (opened, index)
}

#[test]
fn a_reader_idle_under_the_threshold_is_handed_over_with_nothing_sent() {
    let Some(store) = TestStore::postgres("reader_fresh", &[("/seed", b"seed\n")]) else {
        return;
    };
    let (url, _) = store.postgres.clone().unwrap();
    let (proxy, _) = FaultProxy::start(&url);
    let (opened, _) = opened(&store, &proxy, READER_IDLE_CHECK_MS);
    proxy.arm(None);
    opened.store.mount_reader(ws(1)).unwrap();
    let window = proxy.disarm();
    assert_eq!(window.seen, 0, "nothing sent: {:?}", window.log);
    let reuse = opened.store.reuse_work();
    assert_eq!((reuse.readers_opened, reuse.readers_reused), (0, 1));
    opened.store.close_reader(ws(1)).unwrap();
}

#[test]
fn an_aged_reader_whose_path_reset_is_replaced() {
    let Some(store) = TestStore::postgres("reader_reset", &[("/seed", b"seed\n")]) else {
        return;
    };
    let (url, _) = store.postgres.clone().unwrap();
    let (proxy, _) = FaultProxy::start(&url);
    let (opened, reader) = opened(&store, &proxy, SHORT_MS);
    std::thread::sleep(Duration::from_millis(u64::from(SHORT_MS) + 100));
    proxy.arm(Some((0, Fault::Drop)));
    opened.store.mount_reader(ws(1)).unwrap();
    let window = proxy.disarm();
    let fired = window.fired.expect("the check's reply was dropped");
    assert_eq!(fired.connection, reader, "the check ran on the idle reader");
    let reuse = opened.store.reuse_work();
    assert_eq!(
        (reuse.readers_opened, reuse.readers_reused),
        (1, 0),
        "a fresh reader serves the mount"
    );
    // The fresh reader reads.
    opened.store.close_reader(ws(1)).unwrap();
    opened.store.mount_reader(ws(2)).unwrap();
    opened.store.close_reader(ws(2)).unwrap();
}

#[test]
fn a_reader_silent_while_checked_out_is_checked_at_its_next_checkout() {
    let Some(store) = TestStore::postgres("reader_held", &[("/seed", b"seed\n")]) else {
        return;
    };
    let (url, _) = store.postgres.clone().unwrap();
    let (proxy, _) = FaultProxy::start(&url);
    let (opened, reader) = opened(&store, &proxy, SHORT_MS);
    opened.store.mount_reader(ws(1)).unwrap();
    let reuse = opened.store.reuse_work();
    assert_eq!((reuse.readers_opened, reuse.readers_reused), (0, 1));
    // The Workspace holds the reader past the threshold without reading,
    // then hands it back; the next mount takes it at once.
    std::thread::sleep(Duration::from_millis(u64::from(SHORT_MS) + 100));
    opened.store.close_reader(ws(1)).unwrap();
    proxy.arm(Some((0, Fault::Drop)));
    opened.store.mount_reader(ws(2)).unwrap();
    let window = proxy.disarm();
    let fired = window.fired.expect("the check's reply was dropped");
    assert_eq!(fired.connection, reader, "the check ran on the held reader");
    let reuse = opened.store.reuse_work();
    assert_eq!(
        (reuse.readers_opened, reuse.readers_reused),
        (1, 1),
        "a fresh reader serves the second mount"
    );
    opened.store.close_reader(ws(2)).unwrap();
}

#[test]
fn an_aged_reader_whose_backend_died_silently_is_replaced_within_the_bound() {
    let Some(store) = TestStore::postgres("reader_silent", &[("/seed", b"seed\n")]) else {
        return;
    };
    let (url, _) = store.postgres.clone().unwrap();
    let (proxy, _) = FaultProxy::start(&url);
    let (opened, reader) = opened(&store, &proxy, SHORT_MS);
    let pid = proxy.pids()[reader].expect("the reader's backend");
    // From the backend's next message on, nothing reaches the client: its
    // termination notice, its close and the check's reply are all held.
    proxy.arm(Some((0, Fault::Pause)));
    let mut admin = store.admin();
    admin
        .query_one("SELECT pg_terminate_backend($1)", &[&pid])
        .unwrap();
    assert!(adversary::wait_until(Duration::from_secs(10), || {
        admin
            .query_one(
                "SELECT count(*) FROM pg_stat_activity WHERE pid = $1",
                &[&pid],
            )
            .unwrap()
            .get::<_, i64>(0)
            == 0
    }));
    std::thread::sleep(Duration::from_millis(u64::from(SHORT_MS) + 100));
    let start = Instant::now();
    opened.store.mount_reader(ws(1)).unwrap();
    let took = start.elapsed();
    proxy.release();
    let window = proxy.disarm();
    let fired = window
        .fired
        .expect("the terminated backend's notice was held");
    assert_eq!(fired.connection, reader);
    assert!(
        took >= READER_CHECK_TIMEOUT && took < READER_CHECK_TIMEOUT + Duration::from_secs(5),
        "the check waited its bound, then a fresh reader opened: {took:?}"
    );
    let reuse = opened.store.reuse_work();
    assert_eq!((reuse.readers_opened, reuse.readers_reused), (1, 0));
    opened.store.close_reader(ws(1)).unwrap();
}
