//! Recovery across real transaction boundaries, observed outside the Store.
#[allow(dead_code)]
#[path = "pg_doctor/conninfo.rs"]
mod conninfo;
#[path = "pg_recovery/relay.rs"]
mod relay;
mod support;
use layerfs_history::{HistoryCatalog, HistoryError};
use layerfs_persistence::{PostgresHandles, PostgresSelection};
use layerfs_storage::port::{PackPersistence, PersistenceError, Reserve};
use postgres::Client;
use relay::{Cut, Relay};
use std::time::{Duration, Instant};
use support::pg::Schema;

struct Fixture {
    db: Schema,
    relay: Relay,
    selection: PostgresSelection,
}
impl Fixture {
    fn new(tag: &str) -> Self {
        let url = std::env::var("LAYERFS_TEST_POSTGRES_URL").expect("requires PostgreSQL");
        let db = Schema::new(url.clone(), tag);
        drop(db.create(b"session-recovery", [2; 32]));
        let relay = Relay::new(&url);
        let proxied = conninfo::with_local_proxy(&url, relay.port);
        let url = conninfo::with_option(&proxied, "application_name", &db.name);
        let mut selection = PostgresSelection::new(url, &db.name);
        selection.lock_timeout_ms = 100;
        Self {
            db,
            relay,
            selection,
        }
    }
    fn open(&self) -> PostgresHandles {
        PostgresHandles::open_writable(&self.selection, b"session-recovery", [2; 32]).unwrap()
    }
    fn pid(&self, except: i32) -> i32 {
        self.db
            .admin()
            .query_one(
                "SELECT pid FROM pg_stat_activity WHERE application_name=$1 AND pid<>$2",
                &[&self.db.name, &except],
            )
            .unwrap()
            .get(0)
    }
}
fn terminate(admin: &mut Client, pid: i32) {
    assert!(admin
        .query_one("SELECT pg_terminate_backend($1,5000)", &[&pid])
        .unwrap()
        .get::<_, bool>(0));
}
fn read(handles: &PostgresHandles) {
    handles.history.layer_stack(support::stack(3)).unwrap();
}
fn reserve(handles: &PostgresHandles) -> Result<(), PersistenceError> {
    handles
        .storage
        .reserve(Reserve {
            packs: 1,
            ordinals: 0,
        })
        .map(|_| ())
}
fn connects(h: &PostgresHandles) -> u64 {
    h.storage.diagnostics().unwrap().connects
}
// A sequence counts attempted bodies even on rollback. The table records only
// committed effects. Install after Store open to keep schema validation real.
fn markers(admin: &mut Client, table: &str, operation: &str) {
    admin.batch_execute(&format!("CREATE SEQUENCE attempts; CREATE TABLE marker(i int); CREATE FUNCTION mark_attempt() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('attempts'); INSERT INTO marker VALUES(747); RETURN NULL; END $$; CREATE TRIGGER mark AFTER {operation} ON {table} FOR EACH STATEMENT EXECUTE FUNCTION mark_attempt()" )).unwrap();
}
fn check_markers(admin: &mut Client, expected: &[i32]) {
    let r = admin
        .query_one("SELECT last_value,is_called FROM attempts", &[])
        .unwrap();
    assert_eq!((r.get::<_, i64>(0), r.get::<_, bool>(1)), (1, true));
    let rows: Vec<i32> = admin
        .query("SELECT i FROM marker ORDER BY i", &[])
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect();
    assert_eq!(rows, expected);
}
#[test]
#[ignore = "requires LAYERFS_TEST_POSTGRES_URL"]
fn disconnect_before_begin_recovers_in_same_call() {
    let f = Fixture::new("before");
    let h = f.open();
    read(&h);
    let old = f.pid(0);
    terminate(&mut f.db.admin(), old);
    read(&h);
    let new = f.pid(0);
    assert_ne!(old, new);
    read(&h);
    assert_eq!(f.pid(0), new);
    assert_eq!(connects(&h), 2);
}
#[test]
#[ignore = "requires LAYERFS_TEST_POSTGRES_URL"]
fn disconnect_at_commit_preserves_unknown_then_recovers() {
    let f = Fixture::new("atcommit");
    let h = f.open();
    let mut admin = f.db.admin();
    markers(&mut admin, "kept_node", "DELETE");
    f.relay.arm(Cut::BeforeCommit);
    assert_eq!(
        h.history.mark_history_closure().err(),
        Some(HistoryError::UnknownOutcome)
    );
    f.relay.fired();
    check_markers(&mut admin, &[]);
    assert_eq!(connects(&h), 1);
    read(&h);
    assert_eq!(connects(&h), 2);
}
#[test]
#[ignore = "requires LAYERFS_TEST_POSTGRES_URL"]
fn disconnected_body_refuses_without_replay() {
    let f = Fixture::new("body");
    let h = f.open();
    let mut admin = f.db.admin();
    markers(&mut admin, "kept_node", "DELETE");
    f.relay.arm(Cut::AfterDelete);
    assert_eq!(
        h.history.mark_history_closure().err(),
        Some(HistoryError::OwnershipUnavailable)
    );
    f.relay.fired();
    check_markers(&mut admin, &[]);
    read(&h);
}
#[test]
#[ignore = "requires LAYERFS_TEST_POSTGRES_URL"]
fn read_commit_disconnect_is_definite() {
    let f = Fixture::new("readcommit");
    let h = f.open();
    f.relay.arm(Cut::BeforeCommit);
    assert_eq!(
        h.history.layer_stack(support::stack(3)).err(),
        Some(HistoryError::OwnershipUnavailable)
    );
    f.relay.fired();
    h.history
        .layer_stack_named(&support::name("missing"))
        .unwrap();
}
#[test]
#[ignore = "requires LAYERFS_TEST_POSTGRES_URL"]
fn lost_idle_lane_refuses_then_fresh_hold_recovers() {
    let f = Fixture::new("idle");
    let h = f.open();
    let main = f.pid(0);
    let hold = h.history.publish_lane().unwrap();
    let lane = f.pid(main);
    assert_ne!(lane, main);
    drop(hold);
    terminate(&mut f.db.admin(), lane);
    assert_eq!(
        h.history.publish_lane().err(),
        Some(HistoryError::OwnershipUnavailable)
    );
    drop(h.history.publish_lane().unwrap());
    read(&h);
    let fresh = f.pid(main);
    assert_ne!(fresh, lane);
    assert_eq!(f.pid(fresh), main);
}
#[test]
#[ignore = "requires LAYERFS_TEST_POSTGRES_URL"]
fn held_lane_loss_during_write_rolls_back_and_excludes_competitors() {
    let f = Fixture::new("held");
    let h = f.open();
    let main = f.pid(0);
    let mut other = f.db.selection();
    other.lock_timeout_ms = 100;
    let rival = PostgresHandles::open_writable(&other, b"session-recovery", [2; 32]).unwrap();
    let hold = h.history.publish_lane().unwrap();
    let lane = f.pid(main);
    let mut admin = f.db.admin();
    markers(&mut admin, "store_policy", "UPDATE");
    let mut killer = f.db.admin();
    let (send, recv) = std::sync::mpsc::channel();
    f.relay.arm(Cut::AfterUpdate(Box::new(move || {
        terminate(&mut killer, lane);
        send.send(rival.history.publish_lane().err()).unwrap();
    })));
    assert_eq!(
        reserve(&h),
        Err(PersistenceError::Refused {
            status: "Closed".into()
        })
    );
    assert_eq!(
        recv.recv_timeout(Duration::from_secs(10)).unwrap(),
        Some(HistoryError::Busy)
    );
    f.relay.fired();
    drop(hold);
    check_markers(&mut admin, &[]);
    read(&h);
}
#[test]
#[ignore = "requires LAYERFS_TEST_POSTGRES_URL"]
fn diagnostic_commit_disconnect_preserves_observational_unknown() {
    let f = Fixture::new("diag");
    let h = PostgresHandles::open_diagnostic(
        &f.selection,
        b"session-recovery",
        [2; 32],
        Some(Instant::now() + Duration::from_secs(15)),
        &mut |host, port, _| {
            std::net::ToSocketAddrs::to_socket_addrs(&(host, port)).map(Iterator::collect)
        },
    )
    .unwrap();
    f.relay.arm(Cut::BeforeCommit);
    for _ in 0..2 {
        assert_eq!(
            h.diagnostic_mounts(None).err(),
            Some(PersistenceError::Uncertain)
        );
    }
    f.relay.fired();
    assert_eq!(connects(&h), 1);
}
#[test]
#[ignore = "requires LAYERFS_TEST_POSTGRES_URL (plaintext TCP fixture)"]
fn committed_write_with_truncated_command_complete_is_unknown_once() {
    let f = Fixture::new("partial");
    let h = f.open();
    let mut admin = f.db.admin();
    markers(&mut admin, "store_policy", "UPDATE");
    f.relay.arm(Cut::PartialCommit);
    assert_eq!(reserve(&h), Err(PersistenceError::Uncertain));
    f.relay.fired();
    check_markers(&mut admin, &[747]);
}
