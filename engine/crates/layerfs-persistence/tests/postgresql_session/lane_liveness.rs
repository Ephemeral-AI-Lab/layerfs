//! PostgreSQL lane liveness after an uncertain outcome or a lost lane
//! connection.
//!
//! * Finding #3: a session quarantined by an uncertain write closes its
//!   connection. Before the fix its backend stayed connected, idle in its
//!   aborted transaction, until the process exited: a transaction the server
//!   aborted with an error has already released its row locks, but the
//!   backend keeps its session advisory keys (mount and compaction presence),
//!   and an uncertain outcome that leaves the transaction live keeps every
//!   row lock, `store_lane` included.
//! * Finding #10: a write nested in a held lane commits on the session
//!   connection while the row lock lives on the lane connection. When the lane
//!   connection dies, the nested write now fails definitely and writes
//!   nothing; before the fix it committed without the lock.
//!
//! The only injected conditions are a test-only trigger that raises an
//! unlisted SQLSTATE (classed as an unknown outcome) and an administrator's
//! `pg_terminate_backend` of the lane connection.
//!
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "../support/mod.rs"]
mod support;

use layerfs_history::HistoryCatalog;
use layerfs_persistence::PostgresHandles;
use layerfs_storage::{
    port::{PackPersistence, PersistenceError, Reserve},
    StoragePolicy,
};
use std::time::{Duration, Instant};
use support::pg::{catalog, url, Schema};

const BINDING: &[u8] = b"postgresql-lane-liveness";
const CURSOR: [u8; 32] = [91; 32];
/// The lane wait of the second session: a row still held after the first
/// session's failure shows up as `Busy` within it.
const LOCK_TIMEOUT_MS: u32 = 3_000;

fn reserve() -> Reserve {
    Reserve {
        packs: 1,
        ordinals: 0,
    }
}

fn handles(schema: &Schema) -> (PostgresHandles, PostgresHandles) {
    let mut selection = schema.selection();
    selection.lock_timeout_ms = LOCK_TIMEOUT_MS;
    let first = PostgresHandles::create(
        &selection,
        StoragePolicy::frozen_default(),
        &catalog(BINDING, CURSOR, 3),
    )
    .expect("PostgreSQL Store creation");
    let second = PostgresHandles::open_writable(&selection, BINDING, CURSOR)
        .expect("second PostgreSQL session");
    (first, second)
}

fn next_pack(schema: &Schema) -> i64 {
    schema
        .admin()
        .query_one("SELECT next_pack_id FROM store_policy WHERE id=1", &[])
        .unwrap()
        .get(0)
}

/// Backends idle in an aborted transaction whose last statement was the
/// Store's allocation `UPDATE`: the quarantined session's, before the fix.
fn aborted_allocations(admin: &mut postgres::Client) -> i64 {
    admin
        .query_one(
            "SELECT count(*) FROM pg_stat_activity WHERE datname = current_database() \
             AND pid <> pg_backend_pid() AND state = 'idle in transaction (aborted)' \
             AND query LIKE 'UPDATE store_policy SET next_pack_id=next_pack_id+%'",
            &[],
        )
        .unwrap()
        .get(0)
}

#[test]
fn a_quarantined_session_closes_its_connection_and_frees_the_lane_row() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "quarantine_lane");
    let (first, second) = handles(&schema);
    first.storage.reserve(reserve()).unwrap();
    let mut admin = schema.admin();
    admin
        .batch_execute(
            "CREATE FUNCTION quarantine_probe() RETURNS trigger LANGUAGE plpgsql AS \
             $$ BEGIN RAISE EXCEPTION 'probe' USING ERRCODE = 'XX000'; END $$; \
             CREATE TRIGGER quarantine_probe BEFORE UPDATE ON store_policy \
             FOR EACH ROW EXECUTE FUNCTION quarantine_probe()",
        )
        .unwrap();
    let before = next_pack(&schema);
    // The write takes the lane row in its BEGIN, then meets the trigger's
    // internal error: an unknown outcome, so the session is quarantined.
    assert!(matches!(
        first.storage.reserve(reserve()),
        Err(PersistenceError::Uncertain)
    ));
    // The quarantined session's backend is gone; the server ends it as soon
    // as it reads the closed connection (bounded wait).
    let until = Instant::now() + Duration::from_secs(10);
    while aborted_allocations(&mut admin) != 0 {
        assert!(
            Instant::now() < until,
            "the quarantined session's backend is still connected"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    // Removing the trigger needs the table lock the failed UPDATE took; no
    // transaction of the quarantined session holds it.
    admin
        .batch_execute(
            "SET lock_timeout TO 3000; DROP TRIGGER quarantine_probe ON store_policy; \
             DROP FUNCTION quarantine_probe()",
        )
        .expect("the quarantined session left no transaction holding store_policy");
    let started = Instant::now();
    let reserved = second
        .storage
        .reserve(reserve())
        .expect("the second session takes the lane row");
    assert!(started.elapsed() < Duration::from_millis(LOCK_TIMEOUT_MS as u64));
    assert_eq!(reserved.first_pack_id, before);
    // The first session stays quarantined.
    assert!(matches!(
        first.storage.reserve(reserve()),
        Err(PersistenceError::Uncertain)
    ));
}

#[test]
fn a_nested_write_after_the_lane_connection_died_fails_and_writes_nothing() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "lost_lane");
    let (first, second) = handles(&schema);
    first.storage.reserve(reserve()).unwrap();
    let before = next_pack(&schema);
    let lane = first.history.publish_lane().expect("publication lane");
    // The lane connection is the backend of this schema's Store sitting idle
    // in a transaction that holds a row lock on store_lane.
    let mut admin = schema.admin();
    let terminated: i64 = admin
        .query_one(
            "SELECT count(*) FILTER (WHERE pg_terminate_backend(a.pid)) FROM pg_stat_activity a \
             WHERE a.state = 'idle in transaction' AND a.pid <> pg_backend_pid() \
             AND EXISTS (SELECT 1 FROM pg_locks l \
             WHERE l.pid = a.pid AND l.locktype = 'relation' \
             AND l.relation = 'store_lane'::regclass)",
            &[],
        )
        .unwrap()
        .get(0);
    assert_eq!(terminated, 1, "exactly the lane connection is terminated");
    // Wait, bounded, for the server to end it.
    let until = Instant::now() + Duration::from_secs(10);
    while admin
        .query_one(
            "SELECT count(*) FROM pg_locks l WHERE l.locktype = 'relation' \
             AND l.relation = 'store_lane'::regclass AND l.pid <> pg_backend_pid()",
            &[],
        )
        .unwrap()
        .get::<_, i64>(0)
        != 0
    {
        assert!(Instant::now() < until, "the lane backend ends");
        std::thread::sleep(Duration::from_millis(20));
    }
    // The nested write runs in the still-held turn and must not commit.
    let nested = first.storage.reserve(reserve());
    assert!(
        matches!(&nested, Err(PersistenceError::Refused { status }) if status == "Closed"),
        "{nested:?}"
    );
    assert_eq!(next_pack(&schema), before, "the nested write wrote nothing");
    drop(lane);
    // Another session takes the lane and writes at once.
    let reserved = second.storage.reserve(reserve()).unwrap();
    assert_eq!(reserved.first_pack_id, before);
}

/// The lane barrier key of `schema`, as `session/profile.rs` derives it.
fn barrier_key(schema: &str) -> (i32, i32) {
    let hash = schema.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    });
    (0x4c46_4c4e, hash as i32)
}

/// Holds `lock` (SQL run after `BEGIN`) on an administrator connection for
/// `hold`, then commits. Returns once the lock is held.
fn hold_for(schema: &Schema, lock: String, hold: Duration) -> std::thread::JoinHandle<()> {
    let (held, wait) = std::sync::mpsc::channel();
    let mut admin = schema.admin();
    let holder = std::thread::spawn(move || {
        admin.batch_execute(&format!("BEGIN; {lock}")).unwrap();
        held.send(()).unwrap();
        std::thread::sleep(hold);
        admin.batch_execute("COMMIT").unwrap();
    });
    wait.recv_timeout(Duration::from_secs(10))
        .expect("the holder took its lock");
    holder
}

/// #776 C2: before a lost COMMIT's outcome is judged, passing the lane waits
/// for a publication still holding the lane row (a non-nested one) or the
/// barrier key shared (a nested one), and refuses `Busy` at the lock timeout.
#[test]
fn passing_the_lane_waits_for_a_publication_still_running() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "pass_lane");
    let (first, _second) = handles(&schema);
    let hold = Duration::from_millis(400);

    let holder = hold_for(
        &schema,
        "SELECT 1 FROM store_lane WHERE id = 1 FOR UPDATE".into(),
        hold,
    );
    let started = Instant::now();
    first.history.pass_publication_lane().unwrap();
    assert!(started.elapsed() >= hold / 2, "{:?}", started.elapsed());
    holder.join().unwrap();

    let (class, key) = barrier_key(&schema.name);
    let holder = hold_for(
        &schema,
        format!("SELECT pg_advisory_xact_lock_shared({class}, {key})"),
        hold,
    );
    let started = Instant::now();
    first.history.pass_publication_lane().unwrap();
    assert!(started.elapsed() >= hold / 2, "{:?}", started.elapsed());
    holder.join().unwrap();

    let mut selection = schema.selection();
    selection.lock_timeout_ms = 200;
    let short = PostgresHandles::open_writable(&selection, BINDING, CURSOR).unwrap();
    let holder = hold_for(
        &schema,
        "SELECT 1 FROM store_lane WHERE id = 1 FOR UPDATE".into(),
        Duration::from_millis(1_500),
    );
    let refused = short.history.pass_publication_lane().unwrap_err();
    assert!(
        matches!(refused, layerfs_history::HistoryError::Busy),
        "{refused:?}"
    );
    holder.join().unwrap();
    // Nothing is left held: the lane is free at once.
    first.history.pass_publication_lane().unwrap();
}
