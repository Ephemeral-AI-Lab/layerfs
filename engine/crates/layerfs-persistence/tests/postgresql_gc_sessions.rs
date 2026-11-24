//! #768: GC admission for a Workspace mount (register) and unmount (release)
//! on PostgreSQL, across two sessions of one Store.
//!
//! * The pinned counts of one mount register and one owner release, on both
//!   engines; SQLite's are unchanged by #768.
//! * A session's advisory keys exclude the other session's GC calls: a mount
//!   on one session refuses a compaction and a Branch removal on another, and
//!   a compaction's exclusive key refuses a mount. (A backend never conflicts
//!   with its own advisory locks, so the one-handle tests in
//!   `postgresql_gc_fence.rs` see only the intent rows.)
//! * A mount's register and release do not wait for another session's held
//!   publication lane.
//! * A release whose row was already removed elsewhere still unlocks its
//!   session keys, and a lock refused for a stale catalog leaves no key held.
//! * A session with a registered mount still meets the generation fence.
//!
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
mod support;

use layerfs_history::error::HistoryError;
use layerfs_history::records::{GcIntent, GcIntentKind, GcScopeKind};
use layerfs_history::{BranchId, CatalogId, HistoryCatalog, LayerStackId};
use layerfs_persistence::{
    PersistenceConfig, PostgresHandles, SqlWork, SqliteHandles, SqlitePersistenceProfile,
};
use layerfs_storage::StoragePolicy;
use std::sync::mpsc;
use std::time::Duration;
use support::pg::{catalog, project, url, Schema};
use support::Temp;

const CURSOR: [u8; 32] = [76; 32];
/// The lane wait of the sessions under test: long enough that a register or
/// release that still queued behind a held lane misses [`ANSWER`].
const LOCK_TIMEOUT_MS: u32 = 3_000;
/// How long a register or release may take while another session holds the
/// publication lane.
const ANSWER: Duration = Duration::from_millis(1_500);

/// Every test opens its own catalog identity: the advisory keys are
/// database-wide, and a second run of this file on the same database must
/// not share them either.
fn binding(tag: &str) -> Vec<u8> {
    format!("postgresql-gc-sessions-{tag}-{}", std::process::id()).into_bytes()
}

fn op(tag: u8) -> [u8; 16] {
    [tag; 16]
}

fn mount(stack: LayerStackId, branch: BranchId, tag: u8) -> GcIntent {
    GcIntent::shared(
        op(tag),
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.as_slice(),
    )
    .in_project(stack.as_slice())
    .named(Some("proj"), Some("work"))
    .placed(
        Some([tag; 32]),
        Some(&format!("sandbox-{tag}")),
        Some(&format!("machine-{tag}")),
    )
    .owned(Some("runtime-a"), Some(&format!("boot-{tag}")))
}

fn compaction(binding: &[u8], tag: u8) -> GcIntent {
    GcIntent::operation(
        op(tag),
        GcIntentKind::Compaction,
        GcScopeKind::Store,
        CatalogId::derive(binding).unwrap().as_slice(),
    )
    .owned(Some("runtime-b"), Some(&format!("boot-{tag}")))
    .with_phase(Some("mark"))
}

fn removal(stack: LayerStackId, tag: u8) -> GcIntent {
    GcIntent::operation(
        op(tag),
        GcIntentKind::Removal,
        GcScopeKind::Project,
        stack.as_slice(),
    )
    .named(Some("proj"), None)
}

/// Two writable sessions of one new Store, each with its own connection and
/// in-process admission.
fn sessions(schema: &Schema, binding: &[u8]) -> (PostgresHandles, PostgresHandles) {
    let mut selection = schema.selection();
    selection.lock_timeout_ms = LOCK_TIMEOUT_MS;
    let first = PostgresHandles::create(
        &selection,
        StoragePolicy::frozen_default(),
        &catalog(binding, CURSOR, 3),
    )
    .expect("PostgreSQL Store creation");
    let second = PostgresHandles::open_writable(&selection, binding, CURSOR)
        .expect("second PostgreSQL session");
    (first, second)
}

/// The session advisory key of `kind` and `id` in the catalog `binding`
/// derives (spec 5.2).
fn advisory_key(binding: &[u8], kind: u8, id: &[u8]) -> i64 {
    let catalog = CatalogId::derive(binding).unwrap();
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"layerfs/advisory/v1\0");
    hasher.update(catalog.as_slice());
    hasher.update(&[kind]);
    hasher.update(id);
    let mut head = [0u8; 8];
    head.copy_from_slice(&hasher.finalize().as_bytes()[..8]);
    i64::from_be_bytes(head)
}

/// The three keys a Branch mount takes: Store, project, Branch.
fn mount_keys(binding: &[u8], stack: LayerStackId, branch: BranchId) -> [i64; 3] {
    let catalog = CatalogId::derive(binding).unwrap();
    [
        advisory_key(binding, 1, catalog.as_slice()),
        advisory_key(binding, 2, stack.as_slice()),
        advisory_key(binding, 3, branch.as_slice()),
    ]
}

/// Whether another session could take `key` exclusively now (taken and
/// given back at once).
fn key_free(admin: &mut postgres::Client, key: i64) -> bool {
    let free: bool = admin
        .query_one("SELECT pg_try_advisory_lock($1::bigint)", &[&key])
        .unwrap()
        .get(0);
    if free {
        admin
            .query_one("SELECT pg_advisory_unlock($1::bigint)", &[&key])
            .unwrap();
    }
    free
}

fn bump_generation(schema: &Schema) {
    schema
        .admin()
        .execute(
            "UPDATE history_meta SET catalog_incarnation=catalog_incarnation+1 WHERE id=1",
            &[],
        )
        .unwrap();
}

/// Counter delta of one call.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Delta {
    statements: u64,
    transactions: u64,
    commits: u64,
    round_trips: u64,
    prepares: u64,
}

impl Delta {
    fn of(before: &SqlWork, after: &SqlWork) -> Self {
        Self {
            statements: after.statements - before.statements,
            transactions: after.transactions - before.transactions,
            commits: after.commits - before.commits,
            round_trips: after.round_trips - before.round_trips,
            prepares: after.prepares - before.prepares,
        }
    }
}

/// One warm mount register and its owner release, as counter deltas. A first
/// register and release prepare every statement, so prepares do not count.
fn measure(
    history: &dyn HistoryCatalog,
    work: &dyn Fn() -> SqlWork,
    label: &str,
) -> (Delta, Delta) {
    let (stack, branch, _) = project(history, 1);
    history.gc_register(&mount(stack, branch, 1)).unwrap();
    history.gc_release(&op(1)).unwrap();
    let before = work();
    history.gc_register(&mount(stack, branch, 2)).unwrap();
    let registered = work();
    history.gc_release(&op(2)).unwrap();
    let released = work();
    let register = Delta::of(&before, &registered);
    let release = Delta::of(&registered, &released);
    eprintln!("COUNT {label} mount register: {register:?}");
    eprintln!("COUNT {label} owner release: {release:?}");
    (register, release)
}

/// The pinned counts. On PostgreSQL `statements` counts the statements a
/// call sends on its own (each one round trip); a transaction's `BEGIN`
/// batch (its settings and generation fence) and its `COMMIT` are counted
/// in `round_trips` and `transactions`/`commits`, not in `statements`.
///
/// Before #768 a mount register was 8 round trips (one fenced lock
/// statement per key, then a lane write: `BEGIN` with the lane row and the
/// barrier, the Branch probe, the conflict scan, the insert, `COMMIT`) and
/// a release 8 (a lane write reading the row, deleting it and reading the
/// Branch's project to rebuild its keys, then one unlock per key).
#[test]
fn a_mount_register_and_release_are_pinned_on_both_engines() {
    let Some(url) = url() else { return };
    let temp = Temp::new("gc-sessions-counts");
    let sqlite = SqliteHandles::create(
        PersistenceConfig::sqlite(temp.join("store"))
            .with_sqlite_profile(SqlitePersistenceProfile::Normal),
        StoragePolicy::frozen_default(),
        &catalog(b"gc-sessions-sqlite", CURSOR, 3),
    )
    .unwrap();
    let (register, release) = measure(&sqlite.history, &|| sqlite.diagnostics().unwrap(), "sqlite");
    // SQLite is unchanged: one BEGIN IMMEDIATE write each.
    assert_eq!(
        (register.statements, register.transactions, register.commits),
        (5, 1, 1),
        "{register:?}"
    );
    assert_eq!(
        (release.statements, release.transactions, release.commits),
        (3, 1, 1),
        "{release:?}"
    );

    let schema = Schema::new(url, "counts");
    let binding = binding("counts");
    let handles = schema.create(&binding, CURSOR);
    let (register, release) = measure(
        &handles.history,
        &|| handles.diagnostics().unwrap(),
        "postgresql",
    );
    assert_eq!(register.prepares, 0, "{register:?}");
    assert_eq!(release.prepares, 0, "{release:?}");
    // One lock statement for all three keys; one write outside the lane:
    // BEGIN (settings, fence), the Branch probe, the conflict scan, the
    // insert, COMMIT.
    assert_eq!(
        (
            register.round_trips,
            register.statements,
            register.transactions
        ),
        (6, 4, 1),
        "{register:?}"
    );
    // One fenced DELETE … RETURNING, then one unlock statement for all keys.
    assert_eq!(
        (
            release.round_trips,
            release.statements,
            release.transactions
        ),
        (2, 2, 0),
        "{release:?}"
    );
}

/// A mount on one session takes its keys shared: a compaction (exclusive
/// Store key) and a Branch removal (exclusive Branch key) on another
/// session are refused and name the mount; once it is released both admit.
#[test]
fn a_mount_on_one_session_refuses_compaction_and_removal_on_another() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "cross_mount");
    let binding = binding("cross_mount");
    let (a, b) = sessions(&schema, &binding);
    let (stack, branch, _) = project(&a.history, 1);
    a.history.gc_register(&mount(stack, branch, 1)).unwrap();

    let refusal = b.history.gc_register(&compaction(&binding, 2)).unwrap_err();
    let HistoryError::MountsActive(state) = &refusal else {
        panic!("a compaction beside a mount is MountsActive: {refusal:?}");
    };
    assert_eq!(state.holders.len(), 1, "{state:?}");
    assert_eq!(state.holders[0].op, "01".repeat(16));
    let refusal = b
        .history
        .gc_remove_branch(branch, 1_000, &removal(stack, 3))
        .unwrap_err();
    assert!(
        matches!(refusal, HistoryError::MountsActive(_)),
        "{refusal:?}"
    );

    a.history.gc_release(&op(1)).unwrap();
    let mut admin = schema.admin();
    for key in mount_keys(&binding, stack, branch) {
        assert!(key_free(&mut admin, key), "the release unlocked {key}");
    }
    b.history.gc_register(&compaction(&binding, 2)).unwrap();
    b.history.gc_release(&op(2)).unwrap();
    b.history
        .gc_remove_branch(branch, 2_000, &removal(stack, 3))
        .unwrap();
}

/// A compaction on one session holds the Store key exclusively: a mount on
/// another session is refused with the compaction named, and holds no key
/// afterwards.
#[test]
fn a_compaction_on_one_session_refuses_a_mount_on_another() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "cross_compaction");
    let binding = binding("cross_compaction");
    let (a, b) = sessions(&schema, &binding);
    let (stack, branch, _) = project(&a.history, 1);
    b.history.gc_register(&compaction(&binding, 2)).unwrap();

    let refusal = a.history.gc_register(&mount(stack, branch, 1)).unwrap_err();
    let HistoryError::BusyOp(state) = &refusal else {
        panic!("a mount beside a compaction is Busy: {refusal:?}");
    };
    assert_eq!(state.kind, GcIntentKind::Compaction, "{state:?}");
    assert_eq!(state.op, "02".repeat(16));
    assert!(a
        .history
        .gc_intents(GcScopeKind::Branch, branch.as_slice())
        .unwrap()
        .iter()
        .all(|intent| intent.kind != GcIntentKind::Mount));

    b.history.gc_release(&op(2)).unwrap();
    let mut admin = schema.admin();
    for key in mount_keys(&binding, stack, branch) {
        assert!(key_free(&mut admin, key), "the refused mount holds {key}");
    }
    a.history.gc_register(&mount(stack, branch, 1)).unwrap();
    a.history.gc_release(&op(1)).unwrap();
}

/// Runs `call` on its own thread while this thread holds `holder`'s
/// publication lane. Returns whether it answered within [`ANSWER`], then
/// drops the lane and returns its result. The call's thread ends once the
/// lane is dropped (or its own lock wait ends), whatever this thread does.
fn beside_held_lane<T: Send>(
    holder: &PostgresHandles,
    call: impl FnOnce() -> T + Send,
) -> (bool, T) {
    std::thread::scope(|scope| {
        let lane = holder.history.publish_lane().expect("the lane");
        let (sent, answer) = mpsc::channel();
        let worker = scope.spawn(move || {
            let value = call();
            let _ = sent.send(());
            value
        });
        let answered = answer.recv_timeout(ANSWER).is_ok();
        drop(lane);
        (answered, worker.join().expect("the call's thread"))
    })
}

/// The finding #10 barrier guards publication's head check and existence
/// probe, which never read `gc_intent`: a mount's register and its owner's
/// release no longer wait for another session's held publication lane.
#[test]
fn mount_register_and_release_do_not_wait_for_a_held_lane() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "lane_free");
    let binding = binding("lane_free");
    let (a, b) = sessions(&schema, &binding);
    let (stack, branch, _) = project(&a.history, 1);

    let (answered, registered) =
        beside_held_lane(&b, || a.history.gc_register(&mount(stack, branch, 1)));
    registered.expect("the register");
    assert!(
        answered,
        "a mount register waited for another session's lane"
    );

    let (answered, released) = beside_held_lane(&b, || a.history.gc_release(&op(1)));
    released.expect("the release");
    assert!(answered, "a release waited for another session's lane");
    assert!(a
        .history
        .gc_intents(GcScopeKind::Branch, branch.as_slice())
        .unwrap()
        .is_empty());
}

/// B1: a release whose row another session already removed (an operator, or
/// a reconcile that settled it) still unlocks the keys this session took at
/// registration; a compaction is then admitted.
#[test]
fn a_release_after_the_row_was_removed_elsewhere_unlocks_its_keys() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "gone_row");
    let binding = binding("gone_row");
    let (a, b) = sessions(&schema, &binding);
    let (stack, branch, _) = project(&a.history, 1);
    a.history.gc_register(&mount(stack, branch, 1)).unwrap();
    // Another session deletes the row; it holds none of the row's keys, so it
    // unlocks nothing.
    b.history.gc_release(&op(1)).unwrap();
    let mut admin = schema.admin();
    assert!(!key_free(
        &mut admin,
        mount_keys(&binding, stack, branch)[0]
    ));

    a.history.gc_release(&op(1)).unwrap();
    for key in mount_keys(&binding, stack, branch) {
        assert!(
            key_free(&mut admin, key),
            "the owner's release leaked {key}"
        );
    }
    b.history.gc_register(&compaction(&binding, 2)).unwrap();
    b.history.gc_release(&op(2)).unwrap();
}

/// B2: a mount whose session is behind the Store's generation is refused
/// `StaleStore` and holds none of the keys it asked for.
#[test]
fn a_stale_register_leaves_no_key_held() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "stale_lock");
    let binding = binding("stale_lock");
    let handles = schema.create(&binding, CURSOR);
    let (stack, branch, _) = project(&handles.history, 1);
    bump_generation(&schema);

    let refusal = handles
        .history
        .gc_register(&mount(stack, branch, 1))
        .unwrap_err();
    assert!(matches!(refusal, HistoryError::StaleStore), "{refusal:?}");
    let mut admin = schema.admin();
    for key in mount_keys(&binding, stack, branch) {
        assert!(key_free(&mut admin, key), "a stale register holds {key}");
    }
    let rows: i64 = admin
        .query_one("SELECT count(*) FROM gc_intent", &[])
        .unwrap()
        .get(0);
    assert_eq!(rows, 0, "a stale register writes no row");
}

/// The fence still guards a session that holds a registered mount: after an
/// out-of-band generation bump its next register and its release are
/// `StaleStore` and change nothing; the release that follows on a session at
/// the installed generation removes the row.
#[test]
fn a_registered_mount_still_meets_the_generation_fence() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "stale_mounted");
    let binding = binding("stale_mounted");
    let handles = schema.create(&binding, CURSOR);
    let (stack, branch, _) = project(&handles.history, 1);
    handles
        .history
        .gc_register(&mount(stack, branch, 1))
        .unwrap();
    bump_generation(&schema);

    let refusal = handles
        .history
        .gc_register(&mount(stack, branch, 2))
        .unwrap_err();
    assert!(matches!(refusal, HistoryError::StaleStore), "{refusal:?}");
    let refusal = handles.history.gc_release(&op(1)).unwrap_err();
    assert!(matches!(refusal, HistoryError::StaleStore), "{refusal:?}");
    let mut admin = schema.admin();
    let rows: i64 = admin
        .query_one("SELECT count(*) FROM gc_intent", &[])
        .unwrap()
        .get(0);
    assert_eq!(rows, 1, "the stale release deleted nothing");
    assert!(!key_free(
        &mut admin,
        mount_keys(&binding, stack, branch)[2]
    ));

    let fresh = schema.open(&binding, CURSOR).unwrap();
    fresh.history.gc_release(&op(1)).unwrap();
    assert!(fresh
        .history
        .gc_intents(GcScopeKind::Branch, branch.as_slice())
        .unwrap()
        .is_empty());
    drop(handles);
}
