//! #818 I3b (lane W): a write unit on the PostgreSQL writer session, one
//! transaction over several flights of one writer turn, each sent after its
//! predecessor's replies are read (`DecidedTurn::prepare_unit` and
//! `send_in_unit`).
//!
//! The unit used throughout prepares its statements, then sends `BEGIN`
//! (lane row and barrier), one insert and a count, leaving the transaction
//! open. A second flight inserts a row whose value is that count and
//! commits. Checked:
//!
//! * a clean unit commits both rows, counted as one prepare flight, two
//!   flights and one committed write; a second unit sends no prepare;
//! * a refusal in the second flight rolls back the first flight's row and
//!   releases the barrier key (`ROLLBACK` and the release in one more
//!   round trip), and the session stays usable;
//! * while the transaction is open, an ordinary flight, a prepare and a
//!   statement that was not prepared are refused with nothing sent; a turn
//!   that ends with the transaction open rolls it back, never commits;
//! * a dropped connection and a killed backend at every server message of
//!   the unit: atomic (both rows or neither), a lost reply answered
//!   `Unknown`, no reconnect or replay inside the unit.
//!
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
mod support;

use layerfs_persistence::internals::{
    postgresql_unit, BackendError, PipelineRefusal, PipelineReply, PipelineRequest as Request,
    UnitTurn,
};
use layerfs_persistence::{PostgresHandles, PostgresSelection, SqlWork};
use std::time::{Duration, Instant};
use support::fault_proxy::{Fault, FaultProxy};
use support::pg::{url, Schema};

const BINDING: &[u8] = b"session-unit";
const CURSOR: [u8; 32] = [83; 32];

struct Texts {
    insert: String,
    count: String,
}

fn texts(schema: &Schema) -> Texts {
    Texts {
        insert: format!(
            "INSERT INTO {}_unit.rows (k, v) VALUES ($1, $2)",
            schema.name
        ),
        count: format!(
            "SELECT count(*)::int8 FROM {}_unit.rows WHERE k < $1",
            schema.name
        ),
    }
}

/// A new Store, and the unit's table, empty, in a schema of its own (the
/// Store checks its own schema's tables when it opens).
fn setup(schema: &Schema) {
    schema.drop_schema();
    drop(schema.create(BINDING, CURSOR));
    let name = &schema.name;
    schema
        .admin()
        .batch_execute(&format!(
            "DROP SCHEMA IF EXISTS {name}_unit CASCADE; CREATE SCHEMA {name}_unit; \
             CREATE TABLE {name}_unit.rows (k int8 PRIMARY KEY, v int8 NOT NULL)"
        ))
        .unwrap();
}

/// Drops the unit's schema.
fn teardown(schema: &Schema) {
    let name = &schema.name;
    let dropped = format!("DROP SCHEMA IF EXISTS {name}_unit CASCADE");
    schema.admin().batch_execute(&dropped).unwrap();
}

fn rows(schema: &Schema) -> Vec<(i64, i64)> {
    let found = schema
        .admin()
        .query(
            &format!("SELECT k, v FROM {}_unit.rows ORDER BY k", schema.name),
            &[],
        )
        .unwrap();
    found.iter().map(|row| (row.get(0), row.get(1))).collect()
}

/// The first refused request's error.
fn first_error(replies: &[Result<PipelineReply, PipelineRefusal>]) -> BackendError {
    (replies.iter().find_map(|reply| reply.as_ref().err()))
        .map(|refusal| refusal.error)
        .unwrap_or(BackendError::Unknown)
}

/// The unit: `BEGIN`, insert `(first, 10)`, count rows below 1,000; then
/// insert `(second, count)` and `COMMIT`. The committed count, or the
/// refusal (`Unknown` when the session is not trusted).
fn unit(
    turn: &mut UnitTurn<'_, '_>,
    texts: &Texts,
    first: i64,
    second: i64,
) -> Result<i64, BackendError> {
    let prepared = turn.prepare(&[&texts.insert, &texts.count])?;
    let begin = turn.begin.clone();
    let (replies, trusted) = turn.send(
        &prepared,
        &[
            Request::Simple(&begin),
            Request::Statement {
                sql: &texts.insert,
                params: &[&first, &10_i64],
                bytes: 16,
            },
            Request::Statement {
                sql: &texts.count,
                params: &[&1_000_i64],
                bytes: 8,
            },
        ],
        true,
    )?;
    if !trusted {
        return Err(BackendError::Unknown);
    }
    if replies.iter().any(Result::is_err) {
        turn.count(false);
        return Err(first_error(&replies));
    }
    let Ok(PipelineReply::Rows(found)) = &replies[2] else {
        panic!("the count's rows");
    };
    let count: i64 = found[0].get(0);
    let (replies, trusted) = turn.send(
        &prepared,
        &[
            Request::Statement {
                sql: &texts.insert,
                params: &[&second, &count],
                bytes: 16,
            },
            Request::Simple("COMMIT"),
        ],
        false,
    )?;
    if !trusted {
        return Err(BackendError::Unknown);
    }
    let committed = replies.iter().all(Result::is_ok);
    turn.count(committed);
    match committed {
        true => Ok(count),
        false => Err(first_error(&replies)),
    }
}

fn work(handles: &PostgresHandles) -> SqlWork {
    handles.diagnostics().unwrap()
}

/// Barrier keys (`objsubid` 2) any backend of this database holds.
fn barrier_keys(schema: &Schema) -> i64 {
    schema
        .admin()
        .query_one(
            "SELECT count(*) FROM pg_locks WHERE locktype = 'advisory' AND objsubid = 2 \
             AND granted AND database = (SELECT oid FROM pg_database \
             WHERE datname = current_database())",
            &[],
        )
        .unwrap()
        .get(0)
}

#[test]
fn unit_commits_rolls_back_and_guards() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "i3b_unit");
    setup(&schema);
    let texts = texts(&schema);
    let handles = schema.open(BINDING, CURSOR).unwrap();

    // A clean unit: one prepare flight, two flights, one committed write.
    let before = work(&handles);
    let answer = postgresql_unit(&handles, |turn| unit(turn, &texts, 1, 2));
    let after = work(&handles);
    assert_eq!(answer, Ok(1));
    assert_eq!(rows(&schema), vec![(1, 10), (2, 1)]);
    assert_eq!(
        after.round_trips - before.round_trips,
        3,
        "prepare + 2 flights"
    );
    assert_eq!(after.prepares - before.prepares, 2);
    assert_eq!(after.statements - before.statements, 5);
    assert_eq!(after.write_transactions - before.write_transactions, 1);
    assert_eq!(after.write_commits - before.write_commits, 1);

    // A second unit sends no prepare.
    let before = work(&handles);
    let answer = postgresql_unit(&handles, |turn| unit(turn, &texts, 3, 4));
    let after = work(&handles);
    assert_eq!(answer, Ok(3));
    assert_eq!(after.round_trips - before.round_trips, 2, "2 flights");
    assert_eq!(after.prepares, before.prepares);

    // A refusal in the second flight rolls back the first flight's row
    // and releases the barrier key, in one more round trip.
    let before = work(&handles);
    let answer = postgresql_unit(&handles, |turn| unit(turn, &texts, 5, 5));
    let after = work(&handles);
    assert!(
        matches!(answer, Err(ref error) if *error != BackendError::Unknown),
        "{answer:?}"
    );
    assert_eq!(rows(&schema), vec![(1, 10), (2, 1), (3, 10), (4, 3)]);
    assert_eq!(
        after.round_trips - before.round_trips,
        3,
        "2 flights + release"
    );
    assert_eq!(after.rollbacks - before.rollbacks, 1);
    assert_eq!(barrier_keys(&schema), 0);

    // While the transaction is open: an ordinary flight, a prepare and a
    // statement not prepared are refused with nothing sent; the turn ends
    // with the transaction open, so it is rolled back (never committed).
    let before = work(&handles);
    let guarded = postgresql_unit(&handles, |turn| {
        let prepared = turn.prepare(&[&texts.insert])?;
        let begin = turn.begin.clone();
        let (replies, trusted) = turn.send(
            &prepared,
            &[
                Request::Simple(&begin),
                Request::Statement {
                    sql: &texts.insert,
                    params: &[&6_i64, &10_i64],
                    bytes: 16,
                },
            ],
            true,
        )?;
        assert!(trusted && replies.iter().all(Result::is_ok));
        assert_eq!(
            turn.send_plain(&[Request::Simple("SELECT 1")]).err(),
            Some(BackendError::Integrity)
        );
        assert_eq!(
            turn.prepare(&[&texts.count]).err(),
            Some(BackendError::Integrity)
        );
        let unprepared = turn.send(
            &prepared,
            &[Request::Statement {
                sql: &texts.count,
                params: &[&1_i64],
                bytes: 8,
            }],
            true,
        );
        assert_eq!(unprepared.err(), Some(BackendError::Integrity));
        Ok(())
    });
    let after = work(&handles);
    assert_eq!(guarded, Ok(()));
    assert_eq!(rows(&schema).len(), 4, "the open unit was rolled back");
    // Only the open flight and the turn's rollback: the refused calls sent
    // nothing (and the insert's statement was cached).
    assert_eq!(
        after.round_trips - before.round_trips,
        2,
        "flight + rollback"
    );
    assert_eq!(after.statements - before.statements, 2);
    assert_eq!(after.rollbacks - before.rollbacks, 1);
    assert_eq!(after.write_commits, before.write_commits);
    assert_eq!(barrier_keys(&schema), 0);

    // The session is usable after both.
    let answer = postgresql_unit(&handles, |turn| unit(turn, &texts, 7, 8));
    assert_eq!(answer, Ok(5));
    drop(handles);
    teardown(&schema);
}

/// One unit through the proxy with `damage`; the rows it left and its
/// answer.
fn sample(
    schema: &Schema,
    proxy: &FaultProxy,
    proxied: &str,
    damage: Option<(u64, Fault)>,
) -> (Vec<(i64, i64)>, Result<i64, BackendError>, u64) {
    setup(schema);
    let texts = texts(schema);
    let handles = PostgresHandles::open_writable(
        &PostgresSelection::new(proxied, &schema.name),
        BINDING,
        CURSOR,
    )
    .unwrap();
    let connections = proxy.pids().len();
    let opened = work(&handles).connects;
    proxy.arm(damage);
    let answer = postgresql_unit(&handles, |turn| unit(turn, &texts, 1, 2));
    let window = proxy.disarm();
    assert_eq!(proxy.pids().len(), connections, "no reconnect or replay");
    assert_eq!(work(&handles).connects, opened);
    if let Some((at, fault)) = damage {
        let fired = window.fired.as_ref().expect("the armed fault fired");
        assert_eq!(fired.at, at);
        if fault == Fault::Drop {
            assert_eq!(answer, Err(BackendError::Unknown), "message {at}");
        }
        if let Some(pid) = proxy.pids()[fired.connection] {
            wait_gone(schema, pid);
        }
    }
    drop(handles);
    (rows(schema), answer, window.seen)
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

fn sweep(tag: &str, fault: Fault) {
    let Some(url) = url() else { return };
    let (proxy, proxied) = FaultProxy::start(&url);
    let schema = Schema::new(url, tag);
    let (clean, answer, seen) = sample(&schema, &proxy, &proxied, None);
    assert_eq!(answer, Ok(1));
    assert_eq!(clean, vec![(1, 10), (2, 1)]);
    eprintln!("unit_clean_messages {seen}");
    for at in 0..seen {
        let (left, answer, _) = sample(&schema, &proxy, &proxied, Some((at, fault)));
        eprintln!("unit {fault:?} at {at}: {answer:?}, rows {left:?}");
        if left == clean {
            assert!(
                answer == Ok(1) || answer == Err(BackendError::Unknown),
                "a committed unit is answered committed or unknown: {answer:?}"
            );
        } else {
            assert!(left.is_empty(), "atomic: {left:?}");
            assert!(answer.is_err(), "a rolled-back unit never succeeds");
        }
    }
    teardown(&schema);
}

#[test]
fn unit_drop_at_every_message() {
    sweep("i3b_unit_drop", Fault::Drop);
}

#[test]
fn unit_kill_at_every_message() {
    sweep("i3b_unit_kill", Fault::HoldKill);
}
