//! #818 W4 (lane W): an inode reservation on PostgreSQL as one pipelined
//! message (`backend/postgresql/history/reserve.rs`), its statement
//! prepared by the SDK open's flight (`prepare_sdk_writes`).
//!
//! * The answers are the transaction's: ranges follow each other per
//!   scope; another catalog's scope is refused `Integrity` before a range
//!   that does not fit is refused `Capacity`; a refusal writes nothing; a
//!   count of zero is `InvalidInput`.
//! * Once prepared, a reservation is one round trip and one committed
//!   write, with no prepare.
//! * A dropped connection and a killed backend at every server message of
//!   one reservation: the scope advanced by the whole range or not at all;
//!   a success names exactly the range taken; a dropped reply answers
//!   `UnknownOutcome`; no reconnect or replay; no barrier key left; the
//!   next reservation starts after every range the scope gave out.
//!
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "../support/mod.rs"]
mod support;

use layerfs_history::*;
use layerfs_persistence::{PostgresHandles, PostgresSelection, SqlWork};
use std::time::{Duration, Instant};
use support::fault_proxy::{Fault, FaultProxy};
use support::pg::{url, Schema};
use support::root;

const BINDING: &[u8] = b"reserve-message";
const CURSOR: [u8; 32] = [84; 32];

fn work(handles: &PostgresHandles) -> SqlWork {
    handles.diagnostics().unwrap()
}

fn reserve(handles: &PostgresHandles, tag: u8, count: u64) -> HistoryResult<(u64, u64)> {
    let scope = root(tag);
    (handles
        .history
        .reserve_inodes(&ReserveRequest { scope, count }))
    .map(|reserved| (reserved.start, reserved.count))
}

/// The scope's row: its high-water mark and authority.
fn row(schema: &Schema, tag: u8) -> Option<(i64, Vec<u8>)> {
    let sql = format!(
        "SELECT highwater, authority_id FROM {}.scope_allocator WHERE scope_id = $1",
        schema.name
    );
    let scope = root(tag);
    let found = schema
        .admin()
        .query_opt(&sql, &[&scope.as_bytes().as_slice()]);
    found.unwrap().map(|row| (row.get(0), row.get(1)))
}

fn set_row(schema: &Schema, tag: u8, highwater: i64, authority: &[u8]) {
    let sql = format!(
        "INSERT INTO {}.scope_allocator (scope_id, highwater, authority_id) VALUES ($1, $2, $3)",
        schema.name
    );
    let scope = root(tag);
    let mut admin = schema.admin();
    admin
        .execute(
            &sql,
            &[&scope.as_bytes().as_slice(), &highwater, &authority],
        )
        .unwrap();
}

/// Barrier keys (`objsubid` 2) any backend of this database holds.
fn barrier_keys(schema: &Schema) -> i64 {
    let sql = "SELECT count(*) FROM pg_locks WHERE locktype = 'advisory' AND objsubid = 2";
    schema.admin().query_one(sql, &[]).unwrap().get(0)
}

#[test]
fn reservation_answers_as_the_transaction_in_one_message() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "w4_reserve");
    let handles = schema.create(BINDING, CURSOR);
    handles.storage.prepare_sdk_writes().unwrap();
    let authority = handles.history.catalog_id().as_slice().to_vec();

    let before = work(&handles);
    assert_eq!(reserve(&handles, 1, 5), Ok((1, 5)));
    let after = work(&handles);
    assert_eq!(after.round_trips - before.round_trips, 1, "one message");
    assert_eq!(after.prepares, before.prepares, "prepared at the open");
    assert_eq!(after.write_commits - before.write_commits, 1);
    assert_eq!(after.rollbacks, before.rollbacks);
    assert_eq!(reserve(&handles, 1, 3), Ok((6, 3)), "the next range");
    assert_eq!(reserve(&handles, 2, 1), Ok((1, 1)), "a scope of its own");
    assert_eq!(row(&schema, 1), Some((8, authority.clone())));

    // A count of zero keeps the transaction and its refusal.
    let zero = reserve(&handles, 1, 0);
    assert_eq!(zero, Err(HistoryError::InvalidInput("inode reservation")));

    // Another catalog's scope: the authority first, even when the range
    // could not fit either; nothing written.
    set_row(&schema, 3, 7, &[9; 32]);
    let foreign = Err(HistoryError::Integrity("scope authority"));
    assert_eq!(reserve(&handles, 3, 2), foreign);
    assert_eq!(reserve(&handles, 3, u64::MAX), foreign);
    assert_eq!(row(&schema, 3), Some((7, vec![9; 32])));

    // The range must end below 2^63 - 1.
    let capacity = Err(HistoryError::Capacity("inode serials"));
    set_row(&schema, 4, i64::MAX - 3, &authority);
    assert_eq!(reserve(&handles, 4, 3), capacity);
    assert_eq!(reserve(&handles, 4, u64::MAX), capacity);
    assert_eq!(row(&schema, 4), Some((i64::MAX - 3, authority.clone())));
    let last = (i64::MAX - 2) as u64;
    assert_eq!(reserve(&handles, 4, 2), Ok((last, 2)), "the last range");
    assert_eq!(reserve(&handles, 5, i64::MAX as u64), capacity);
    assert_eq!(reserve(&handles, 5, u64::MAX), capacity);
    assert_eq!(row(&schema, 5), None, "a refused new scope has no row");
    let widest = (i64::MAX - 1) as u64;
    assert_eq!(reserve(&handles, 5, widest), Ok((1, widest)), "the widest");

    assert_eq!(barrier_keys(&schema), 0);
    drop(handles);
    schema.drop_schema();
}

fn wait_gone(schema: &Schema, pid: i32) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut admin = schema.admin();
    loop {
        let gone: bool = admin
            .query_one(
                "SELECT NOT EXISTS (SELECT 1 FROM pg_stat_activity WHERE pid=$1)",
                &[&pid],
            )
            .unwrap()
            .get(0);
        if gone {
            return;
        }
        assert!(Instant::now() < deadline, "backend {pid} still runs");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// One reservation of 4 through the proxy with `damage`: its answer, the
/// high-water before and after, and the messages it caused.
fn sample(
    schema: &Schema,
    proxy: &FaultProxy,
    proxied: &str,
    damage: Option<(u64, Fault)>,
) -> (HistoryResult<(u64, u64)>, i64, i64, u64) {
    let selection = PostgresSelection::new(proxied, &schema.name);
    let handles = PostgresHandles::open_writable(&selection, BINDING, CURSOR).unwrap();
    handles.storage.prepare_sdk_writes().unwrap();
    let before = row(schema, 1).map_or(0, |(highwater, _)| highwater);
    let connections = proxy.pids().len();
    let opened = work(&handles).connects;
    proxy.arm(damage);
    let answer = reserve(&handles, 1, 4);
    let window = proxy.disarm();
    assert_eq!(proxy.pids().len(), connections, "no reconnect or replay");
    assert_eq!(work(&handles).connects, opened);
    if let Some((at, fault)) = damage {
        let fired = window.fired.as_ref().expect("the armed fault fired");
        assert_eq!(fired.at, at);
        if fault == Fault::Drop {
            assert_eq!(answer, Err(HistoryError::UnknownOutcome), "message {at}");
        }
        if let Some(pid) = proxy.pids()[fired.connection] {
            wait_gone(schema, pid);
        }
    }
    drop(handles);
    let after = row(schema, 1).map_or(0, |(highwater, _)| highwater);
    (answer, before, after, window.seen)
}

fn sweep(tag: &str, fault: Fault) {
    let Some(url) = url() else { return };
    let (proxy, proxied) = FaultProxy::start(&url);
    let schema = Schema::new(url, tag);
    drop(schema.create(BINDING, CURSOR));
    let (answer, before, after, seen) = sample(&schema, &proxy, &proxied, None);
    assert_eq!((answer, before, after), (Ok((1, 4)), 0, 4));
    eprintln!("reserve_clean_messages {seen}");
    for at in 0..seen {
        let (answer, before, after, _) = sample(&schema, &proxy, &proxied, Some((at, fault)));
        eprintln!("reserve {fault:?} at {at}: {answer:?}, highwater {before} -> {after}");
        match answer {
            Ok(range) => assert_eq!((range, after), (((before + 1) as u64, 4), before + 4)),
            Err(HistoryError::UnknownOutcome) => {
                assert!(after == before || after == before + 4, "whole or nothing")
            }
            Err(other) => assert_eq!(after, before, "a refusal writes nothing: {other:?}"),
        }
        assert_eq!(
            barrier_keys(&schema),
            0,
            "message {at}: no barrier key left"
        );
    }
    // The next reservation starts after every range the scope gave out.
    let handles = PostgresHandles::open_writable(&schema.selection(), BINDING, CURSOR).unwrap();
    let highwater = row(&schema, 1).unwrap().0;
    assert_eq!(reserve(&handles, 1, 1), Ok(((highwater + 1) as u64, 1)));
    drop(handles);
    schema.drop_schema();
}

/// The open's prepare flight with `fault` at every server message, on a
/// writer that reopens (as the Store service's): the flight answers
/// success or `Unknown` (the session quarantined, nothing resent), and the
/// next reservation on the same handles is answered, after one reopen
/// when the flight was lost, with the next range.
fn prepare_sweep(tag: &str, fault: Fault) {
    let Some(url) = url() else { return };
    let (proxy, proxied) = FaultProxy::start(&url);
    let schema = Schema::new(url, tag);
    drop(schema.create(BINDING, CURSOR));
    let selection = PostgresSelection::new(&proxied, &schema.name);
    let open = || {
        let handles = PostgresHandles::open_writable(&selection, BINDING, CURSOR).unwrap();
        handles.storage.enable_reopen();
        handles
    };
    let handles = open();
    proxy.arm(None);
    handles.storage.prepare_sdk_writes().unwrap();
    let seen = proxy.disarm().seen;
    drop(handles);
    eprintln!("prepare_clean_messages {seen}");
    let mut next = 1;
    for at in 0..seen {
        let handles = open();
        let connections = proxy.pids().len();
        proxy.arm(Some((at, fault)));
        let prepared = handles.storage.prepare_sdk_writes();
        let window = proxy.disarm();
        assert_eq!(proxy.pids().len(), connections, "no reconnect or replay");
        let fired = window.fired.as_ref().expect("the armed fault fired");
        if fault == Fault::Drop {
            assert!(prepared.is_err(), "message {at}: a lost reply");
        }
        if let Some(pid) = proxy.pids()[fired.connection] {
            wait_gone(&schema, pid);
        }
        let answer = reserve(&handles, 1, 1);
        eprintln!("prepare {fault:?} at {at}: {prepared:?}, then {answer:?}");
        assert_eq!(answer, Ok((next, 1)), "message {at}: the next range");
        next += 1;
        drop(handles);
    }
    assert_eq!(barrier_keys(&schema), 0);
    schema.drop_schema();
}

#[test]
fn prepare_drop_at_every_message() {
    prepare_sweep("w4_prepare_drop", Fault::Drop);
}

#[test]
fn prepare_kill_at_every_message() {
    prepare_sweep("w4_prepare_kill", Fault::HoldKill);
}

#[test]
fn reservation_drop_at_every_message() {
    sweep("w4_reserve_drop", Fault::Drop);
}

#[test]
fn reservation_kill_at_every_message() {
    sweep("w4_reserve_kill", Fault::HoldKill);
}
