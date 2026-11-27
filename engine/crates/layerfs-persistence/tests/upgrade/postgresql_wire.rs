//! G and J for PostgreSQL `store upgrade` (#804 §9, §11), on the wire.
//!
//! J: a pass-through tap (#768's) shows the upgrade's connection sends
//! exactly two simple `Query` frames and waits for two `ReadyForQuery`
//! replies when it upgrades (one when the Store is current, two when
//! message 2 is refused), with no extended-protocol message at all.
//!
//! G: #768's fault proxy cuts the connection at every server message of a
//! clean run, startup to the last `ReadyForQuery`. Whatever the cut, the
//! Store is wholly schema 3 (every row unchanged) or wholly schema 4, the
//! answer never claims what did not happen (`Ok` only when upgraded; a
//! definite refusal only when unchanged; `Unknown` otherwise), and a re-run
//! converges. NOT_RUN without `LAYERFS_TEST_POSTGRES_URL`.
#[path = "../support/mod.rs"]
mod support;

use layerfs_persistence::upgrade::{
    upgrade_postgresql, UpgradeError, UpgradeOutcome, UpgradeReport, UpgradeRequest, UpgradeWork,
};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};
use support::fault_proxy::{proxied_url, Fault, FaultProxy};
use support::pg_fixture::{FixtureDb, SCHEMA};
use support::wire_tap::Tap;

fn run(url: &str) -> Result<UpgradeReport, UpgradeError> {
    let request = UpgradeRequest {
        cancel: &AtomicBool::new(false),
        failpoint: None,
    };
    upgrade_postgresql(url, SCHEMA, &request, &mut |_| {})
}

fn version(db: &FixtureDb) -> i64 {
    db.client()
        .query_one("SELECT schema_version FROM history_meta", &[])
        .unwrap()
        .get(0)
}

/// `(connections, simple queries, extended-protocol replies)` the tap saw.
fn wire(tap: &Tap) -> (usize, usize, usize) {
    let conns = tap.disarm();
    let used: Vec<_> = conns.iter().filter(|c| !c.is_empty()).collect();
    let queries = used
        .iter()
        .flat_map(|c| c.iter())
        .filter(|r| r.kind == 'Z' && !r.sql.is_empty())
        .count();
    let extended = used
        .iter()
        .flat_map(|c| c.iter())
        .filter(|r| matches!(r.kind, '1' | '2' | '3' | 's' | 'n'))
        .count();
    (used.len(), queries, extended)
}

#[test]
fn message_two_is_one_query_frame_and_one_wait() {
    let Some(admin) = support::pg::url() else {
        return;
    };
    let db = FixtureDb::restore(&admin, "wire_counts");
    let (upstream_url, upstream) = proxied_url(&db.url, 0);
    drop(upstream_url);
    let tap = Tap::start(upstream);
    let (url, _) = proxied_url(&db.url, tap.port);

    tap.arm();
    let report = run(&url).unwrap();
    assert_eq!(report.outcome, UpgradeOutcome::Upgraded);
    assert_eq!(
        report.work,
        UpgradeWork {
            connects: 1,
            round_trips: 2,
            write_transactions: 1
        }
    );
    assert_eq!(
        wire(&tap),
        (1, 2, 0),
        "upgraded: 1 connection, 2 Query frames, no extended protocol"
    );

    tap.arm();
    let report = run(&url).unwrap();
    assert_eq!(report.outcome, UpgradeOutcome::Current);
    assert_eq!(
        report.work,
        UpgradeWork {
            connects: 1,
            round_trips: 1,
            write_transactions: 0
        }
    );
    assert_eq!(
        wire(&tap),
        (1, 1, 0),
        "current: 1 connection, 1 Query frame"
    );
    drop(db);

    // Refused inside message 2 (the lane row held): 1 connection, 2 frames,
    // nothing written.
    let db = FixtureDb::restore(&admin, "wire_refused");
    let (url, _) = proxied_url(&db.url, tap.port);
    let mut holder = db.client();
    holder
        .batch_execute("BEGIN; SELECT 1 FROM store_lane WHERE id=1 FOR UPDATE")
        .unwrap();
    tap.arm();
    let error = run(&url).unwrap_err();
    assert!(matches!(error, UpgradeError::Busy(_)), "{error:?}");
    assert_eq!(
        wire(&tap),
        (1, 2, 0),
        "refused in message 2: 1 connection, 2 Query frames"
    );
    holder.batch_execute("ROLLBACK").unwrap();
    assert_eq!(version(&db), 3);
}

/// Waits (bounded) until no other backend is connected to the fixture
/// database: a cut client's server session has ended its transaction.
fn quiet(db: &FixtureDb) {
    let mut client = postgres::Client::connect(&db.admin, postgres::NoTls).unwrap();
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        let others: i64 = client
            .query_one(
                "SELECT count(*) FROM pg_stat_activity WHERE datname = $1",
                &[&db.name],
            )
            .unwrap()
            .get(0);
        if others == 0 {
            return;
        }
        assert!(Instant::now() < until, "the cut session never ended");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_connection_cut_at_every_reply_leaves_the_store_wholly_old_or_new() {
    let Some(admin) = support::pg::url() else {
        return;
    };
    // The clean run's server messages, counted through the proxy.
    let total = {
        let db = FixtureDb::restore(&admin, "cut_count");
        let (proxy, url) = FaultProxy::start(&db.url);
        proxy.arm(None);
        run(&url).unwrap();
        proxy.disarm().seen
    };
    assert!(total > 20, "{total} messages");
    let (mut old, mut new) = (0, 0);
    for at in 0..total {
        let db = FixtureDb::restore(&admin, &format!("cut{at}"));
        let digest = db.digest();
        let (proxy, url) = FaultProxy::start(&db.url);
        proxy.arm(Some((at, Fault::Drop)));
        let answer = run(&url);
        let fired = proxy.disarm().fired;
        assert!(
            fired.is_some(),
            "cut {at}: the fault never fired: {answer:?}"
        );
        quiet(&db);
        match version(&db) {
            3 => {
                assert_eq!(db.digest(), digest, "cut {at}: schema 3 with rows changed");
                assert!(answer.is_err(), "cut {at}: Ok over an unchanged Store");
                old += 1;
                assert_eq!(
                    run(&db.url).unwrap().outcome,
                    UpgradeOutcome::Upgraded,
                    "cut {at}"
                );
            }
            4 => {
                assert!(
                    matches!(answer, Ok(_) | Err(UpgradeError::Unknown(_))),
                    "cut {at}: {answer:?} over an upgraded Store"
                );
                new += 1;
                assert_eq!(
                    run(&db.url).unwrap().outcome,
                    UpgradeOutcome::Current,
                    "cut {at}"
                );
            }
            other => panic!("cut {at}: schema version {other}"),
        }
    }
    eprintln!("{total} cuts: {old} wholly old, {new} wholly new");
}
