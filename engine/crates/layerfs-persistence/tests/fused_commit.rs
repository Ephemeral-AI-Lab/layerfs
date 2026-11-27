//! #768 stage 2: a Workspace Commit as one write transaction
//! (`layerfs_persistence::Committer`). On PostgreSQL it is one pipelined
//! message: 1 round trip and 1 transaction once its statements are
//! prepared on the connection, one more round trip for the prepares. On
//! SQLite it is one `BEGIN IMMEDIATE` transaction.
//!
//! Every refusal is checked to have written nothing: the Branch head is
//! unchanged, none of the held publication's objects is located and none of
//! its hints is recorded. A race (the head moved, another writer stored one
//! of its objects, and on PostgreSQL the Commit's identity exists) is
//! `Fused::Race`, after which the separate path still publishes; an epoch
//! change is `Conflict`, a generation change `StaleStore`, a lane wait past
//! `lock_timeout` `Busy` and a lost reply an unknown outcome that
//! quarantines the session.
//!
//! Needs `LAYERFS_TEST_POSTGRES_URL` for the PostgreSQL tests; NOT_RUN
//! without it.
mod support;

use layerfs_content::{construct_bytes, ObjectId};
use layerfs_history::records::{
    BindingHint, CommitMeta, CommitOutcome, CommitRequest, CommitVia, ForkRequest, ForkSource,
    StackInitialization,
};
use layerfs_history::{
    BranchId, BranchSnapshot, HistoryCatalog, HistoryCatalogConfig, HistoryError, LayerStackId,
};
use layerfs_persistence::internals::fused_commit_at;
use layerfs_persistence::{
    Committer, Fused, HistoryProvider, PersistenceConfig, PostgresHandles, PostgresSelection,
    SqlWork, SqliteHandles, SqlitePersistenceProfile,
};
use layerfs_storage::{
    port::{PackPersistence, Publication},
    PreparedTail, Storage, StoragePolicy,
};
use layerfs_telemetry::timer::Timing;
use std::sync::Arc;
use std::time::{Duration, Instant};
use support::{branch_id, name, root, stack, Temp};

const BINDING: &[u8] = b"fused-commit";
const CURSOR: [u8; 32] = [68; 32];

fn catalog() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: BINDING.to_vec(),
        cursor_key: CURSOR,
        incarnation: 3,
    }
}

fn bytes(seed: u32, len: usize) -> Vec<u8> {
    let mut state = seed.wrapping_mul(2654435761) | 1;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state >> 24) as u8
        })
        .collect()
}

/// One Save of one file of `seed`, its last publication held.
fn held(storage: &Storage, seed: u32) -> (PreparedTail<'_>, ObjectId) {
    held_bytes(storage, seed, 5000)
}

/// One Save of one file of `len` bytes of `seed`, its last publication held.
fn held_bytes(storage: &Storage, seed: u32, len: usize) -> (PreparedTail<'_>, ObjectId) {
    let save = storage.begin_save().unwrap();
    let root = Timing::disabled("construct", |scope| {
        construct_bytes(
            storage.policy().construction(),
            &storage.policy().construction().capacities(),
            &bytes(seed, len),
            &mut save.sink(),
            scope.child("content"),
        )
    })
    .0
    .unwrap()
    .root;
    (save.finish_prepared().unwrap(), root)
}

/// One Store, its Branch `work` and the session's counters.
struct Fixture<'a> {
    storage: Storage,
    persistence: Arc<dyn PackPersistence>,
    history: &'a HistoryProvider,
    committer: Committer,
    branch: BranchId,
    stack: LayerStackId,
    work: Box<dyn Fn() -> SqlWork + 'a>,
}

impl<'a> Fixture<'a> {
    fn new(
        persistence: Arc<dyn PackPersistence>,
        history: &'a HistoryProvider,
        work: Box<dyn Fn() -> SqlWork + 'a>,
    ) -> Self {
        let storage = Storage::new(persistence.clone()).unwrap();
        let save = storage.begin_save().unwrap();
        let genesis = Timing::disabled("construct", |scope| {
            construct_bytes(
                storage.policy().construction(),
                &storage.policy().construction().capacities(),
                b"genesis tree",
                &mut save.sink(),
                scope.child("content"),
            )
        })
        .0
        .unwrap()
        .root;
        save.finish().unwrap();
        let record = history
            .initialize_layerstack(&StackInitialization {
                logical: 0,
                stack: stack(1),
                name: name("main"),
                scope: root(200),
                profile: root(201),
                genesis_root: genesis,
            })
            .unwrap();
        history
            .fork(&ForkRequest {
                stack: stack(1),
                branch: branch_id(1),
                name: name("work"),
                source: ForkSource::Layer(record.head_layer),
            })
            .unwrap();
        Self {
            storage,
            persistence,
            history,
            committer: history.committer(),
            branch: branch_id(1),
            stack: stack(1),
            work,
        }
    }

    fn snapshot(&self) -> BranchSnapshot {
        self.history.branch_snapshot(self.branch).unwrap().unwrap()
    }

    /// The request of a Commit of `candidate` over `over`.
    fn request(&self, over: &BranchSnapshot, candidate: ObjectId) -> CommitRequest {
        CommitRequest {
            branch: self.branch,
            expected_head: over.branch.head_commit,
            expected_base: over.branch.base_layer,
            expected_root: over.effective_root,
            candidate_root: candidate,
            profile: root(201),
            scope: root(200),
            second_parent: None,
            meta: CommitMeta::new(CommitVia::Workspace),
            force: false,
            required_head: None,
        }
    }

    fn fused(
        &self,
        tail: Option<&Publication>,
        request: &CommitRequest,
        hints: &[BindingHint],
    ) -> Result<CommitOutcome, Fused> {
        self.committer.commit(tail, self.stack, request, hints)
    }

    /// One Commit on the separate path: publish the tail, then
    /// `commit_branch` in the lane.
    fn separate(&self, tail: PreparedTail<'_>, over: &BranchSnapshot, candidate: ObjectId) {
        tail.publish().unwrap();
        let lane = self.history.publish_lane().unwrap();
        let outcome = self
            .history
            .commit_branch(&self.request(over, candidate))
            .unwrap();
        drop(lane);
        assert!(matches!(outcome, CommitOutcome::Committed { .. }));
    }

    /// Whether any of `ids` is located in the Store.
    fn stored(&self, ids: &[ObjectId]) -> bool {
        let mut rows = Vec::new();
        self.persistence.locate(ids, &mut rows).unwrap();
        !rows.is_empty()
    }

    /// The refused attempt wrote nothing: the head is `head`, the held
    /// objects are not stored and the hint is not recorded.
    fn nothing_written(&self, label: &str, head: &BranchSnapshot, ids: &[ObjectId]) {
        let now = self.snapshot();
        assert_eq!(
            (now.branch.head_commit, now.effective_root),
            (head.branch.head_commit, head.effective_root),
            "{label}: the head did not move"
        );
        assert!(!self.stored(ids), "{label}: no held object is stored");
        assert!(
            self.history.bindings_of(HINT.child).unwrap().is_empty(),
            "{label}: no hint is recorded"
        );
    }
}

/// The hint every attempt carries.
const HINT: BindingHint = BindingHint {
    child: 9_000_001,
    parent: 1,
    name: Vec::new(),
};

fn hint() -> BindingHint {
    BindingHint {
        name: b"fused".to_vec(),
        ..HINT
    }
}

fn ids(tail: &PreparedTail<'_>) -> Vec<ObjectId> {
    tail.publication()
        .map(|batch| batch.objects.iter().map(|row| row.object_id).collect())
        .unwrap_or_default()
}

/// Counter delta of one call: round trips, prepares, transactions, commits.
fn delta(before: &SqlWork, after: &SqlWork) -> [u64; 4] {
    [
        after.round_trips - before.round_trips,
        after.prepares - before.prepares,
        after.transactions - before.transactions,
        after.write_commits - before.write_commits,
    ]
}

/// Three fused Commits in a row; each publishes, records its hint and
/// advances the Branch. Returns the counter deltas of each.
fn three_commits(fixture: &Fixture<'_>) -> Vec<[u64; 4]> {
    let mut deltas = Vec::new();
    for seed in 1..=3 {
        let over = fixture.snapshot();
        let (tail, candidate) = held(&fixture.storage, seed);
        let ids = ids(&tail);
        let hints = [BindingHint {
            child: HINT.child + u64::from(seed),
            ..hint()
        }];
        let before = (fixture.work)();
        let outcome = fixture
            .fused(
                tail.publication(),
                &fixture.request(&over, candidate),
                &hints,
            )
            .unwrap();
        deltas.push(delta(&before, &(fixture.work)()));
        let CommitOutcome::Committed { record, displaced } = outcome else {
            panic!("committed: {outcome:?}");
        };
        assert_eq!(displaced, None);
        assert_eq!(record.parent, over.branch.head_commit);
        tail.acknowledged().unwrap();
        let now = fixture.snapshot();
        assert_eq!(now.branch.head_commit, Some(record.id));
        assert_eq!(now.effective_root, candidate);
        assert_eq!(
            fixture.history.commit(record.id).unwrap().as_ref(),
            Some(&*record)
        );
        assert!(fixture.stored(&ids), "the held objects are published");
        assert_eq!(
            fixture.history.bindings_of(hints[0].child).unwrap(),
            hints.to_vec()
        );
        // The Commit's ancestry reads through the lineage it recorded.
        assert!(fixture
            .history
            .is_ancestor(
                over.branch.head_commit.map_or(
                    layerfs_history::NodeId::Layer(over.branch.base_layer),
                    layerfs_history::NodeId::Commit
                ),
                layerfs_history::NodeId::Commit(record.id)
            )
            .unwrap());
        // The published objects read back through a new handle.
        let fresh = Storage::new(fixture.persistence.clone()).unwrap();
        fresh.reader().unwrap().read_objects(&[candidate]).unwrap();
    }
    deltas
}

/// A head that moved after the request read it: a race, nothing written;
/// the separate path then publishes over the new head.
fn head_moved(fixture: &Fixture<'_>) {
    let over = fixture.snapshot();
    // Another Commit lands over `over` first, on the separate path.
    let (other, other_root) = held(&fixture.storage, 41);
    fixture.separate(other, &over, other_root);
    let moved = fixture.snapshot();
    let (tail, candidate) = held(&fixture.storage, 40);
    let ids = ids(&tail);
    let attempt = fixture.fused(
        tail.publication(),
        &fixture.request(&over, candidate),
        &[hint()],
    );
    assert!(matches!(attempt, Err(Fused::Race)), "{attempt:?}");
    fixture.nothing_written("head moved", &moved, &ids);
    fixture.separate(tail, &moved, candidate);
}

/// Another handle stored one of the held objects meanwhile: the held
/// publication would lose that identity, a race; nothing written. The
/// separate path compares it with the winner and publishes.
fn object_stored_meanwhile(fixture: &Fixture<'_>) {
    let over = fixture.snapshot();
    let (tail, candidate) = held(&fixture.storage, 50);
    let ids = ids(&tail);
    let other = Storage::new(fixture.persistence.clone()).unwrap();
    let (theirs, same) = held(&other, 50);
    assert_eq!(same, candidate);
    theirs.publish().unwrap();
    let attempt = fixture.fused(
        tail.publication(),
        &fixture.request(&over, candidate),
        &[hint()],
    );
    assert!(matches!(attempt, Err(Fused::Race)), "{attempt:?}");
    let now = fixture.snapshot();
    assert_eq!(now.branch.head_commit, over.branch.head_commit);
    assert!(fixture.history.bindings_of(HINT.child).unwrap().is_empty());
    // The other handle's objects are the stored ones; ours are not.
    assert!(fixture.stored(&ids));
    let outcome = tail.publish().unwrap();
    assert!(outcome.reused > 0, "{outcome:?}");
    let lane = fixture.history.publish_lane().unwrap();
    fixture
        .history
        .commit_branch(&fixture.request(&over, candidate))
        .unwrap();
    drop(lane);
}

#[test]
fn sqlite_fused_commit() {
    let temp = Temp::new("fused-commit");
    let handles = SqliteHandles::create(
        PersistenceConfig::sqlite(temp.join("store"))
            .with_sqlite_profile(SqlitePersistenceProfile::Normal),
        StoragePolicy::frozen_default(),
        &catalog(),
    )
    .unwrap();
    let fixture = Fixture::new(
        handles.storage.clone(),
        &handles.history,
        Box::new(|| handles.diagnostics().unwrap()),
    );
    // One write transaction, one write commit, each Commit.
    for counts in three_commits(&fixture) {
        assert_eq!([counts[2], counts[3]], [1, 1], "{counts:?}");
    }
    head_moved(&fixture);
    object_stored_meanwhile(&fixture);
    // A project without its epoch row is `Conflict`. (An identical Commit
    // identity is not exercised on SQLite: there the fused transaction runs
    // `commit_branch` itself, so it follows that function's own rule on
    // both paths.)
    let over = fixture.snapshot();
    let (tail, candidate) = held(&fixture.storage, 60);
    let ids = ids(&tail);
    let store = rusqlite::Connection::open(temp.join("store")).unwrap();
    store
        .execute(
            "DELETE FROM project_epoch WHERE layer_stack_id = ?1",
            [fixture.stack.as_slice()],
        )
        .unwrap();
    let attempt = fixture.fused(
        tail.publication(),
        &fixture.request(&over, candidate),
        &[hint()],
    );
    assert!(
        matches!(
            attempt,
            Err(Fused::History(HistoryError::Conflict("project epoch")))
        ),
        "{attempt:?}"
    );
    fixture.nothing_written("sqlite epoch row", &over, &ids);
}

/// The PostgreSQL Store of one test, its schema dropped when it ends.
struct Pg {
    url: String,
    schema: String,
}

impl Pg {
    fn new(tag: &str) -> Option<Self> {
        let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
            eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
            return None;
        };
        let pg = Self {
            url,
            schema: format!("lfs768f_{tag}_{}", std::process::id()),
        };
        pg.admin()
            .batch_execute(&format!("DROP SCHEMA IF EXISTS {} CASCADE", pg.schema))
            .unwrap();
        drop(
            PostgresHandles::create(
                &pg.selection(30_000),
                StoragePolicy::frozen_default(),
                &catalog(),
            )
            .unwrap(),
        );
        Some(pg)
    }
    fn selection(&self, lock_timeout_ms: u32) -> PostgresSelection {
        let mut selection = PostgresSelection::new(&self.url, &self.schema);
        selection.lock_timeout_ms = lock_timeout_ms;
        selection
    }
    fn open(&self, lock_timeout_ms: u32) -> PostgresHandles {
        PostgresHandles::open_writable(&self.selection(lock_timeout_ms), BINDING, CURSOR).unwrap()
    }
    fn admin(&self) -> postgres::Client {
        postgres::Client::connect(&self.url, postgres::NoTls).unwrap()
    }
}

/// Waits until a backend waits for a lock `blocker` holds; its pid.
fn waiting_on(observer: &mut postgres::Client, blocker: i32) -> i32 {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let row = observer
            .query_opt(
                "SELECT pid FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid)) LIMIT 1",
                &[&blocker],
            )
            .unwrap();
        if let Some(row) = row {
            return row.get(0);
        }
        assert!(Instant::now() < deadline, "no backend waits for the lock");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The admin connection's backend.
fn pid(client: &mut postgres::Client) -> i32 {
    client
        .query_one("SELECT pg_backend_pid()", &[])
        .unwrap()
        .get(0)
}

impl Drop for Pg {
    fn drop(&mut self) {
        if let Ok(mut client) = postgres::Client::connect(&self.url, postgres::NoTls) {
            let _ = client.batch_execute(&format!("DROP SCHEMA IF EXISTS {} CASCADE", self.schema));
        }
    }
}

#[test]
fn postgresql_fused_commit_is_one_round_trip() {
    let Some(pg) = Pg::new("rt") else { return };
    let handles = pg.open(30_000);
    let fixture = Fixture::new(
        handles.storage.clone(),
        &handles.history,
        Box::new(|| handles.diagnostics().unwrap()),
    );
    let deltas = three_commits(&fixture);
    // [round trips, prepares, transactions, write commits]. The first Commit
    // prepares every statement of the message in one flight, the second the
    // lineage statement of a Commit with a parent; the third is warm: one
    // flight, one transaction.
    assert_eq!(
        deltas[0][0], 2,
        "cold: the prepares, then the message: {deltas:?}"
    );
    assert!(deltas[0][1] >= 6, "{deltas:?}");
    assert_eq!(deltas[1], [2, 1, 1, 1], "{deltas:?}");
    assert_eq!(deltas[2], [1, 0, 1, 1], "warm: {deltas:?}");
    head_moved(&fixture);
    object_stored_meanwhile(&fixture);

    // The same Commit identity again (the same request made at the same
    // time over the same head, after the Branch was put back): a race.
    let over = fixture.snapshot();
    let (tail, candidate) = held(&fixture.storage, 70);
    tail.publish().unwrap();
    let request = fixture.request(&over, candidate);
    let made = fused_commit_at(&fixture.committer, None, fixture.stack, &request, 77).unwrap();
    let CommitOutcome::Committed { record, .. } = made else {
        panic!("{made:?}");
    };
    pg.admin()
        .execute(
            &format!(
                "UPDATE {}.branch SET head_commit_id = $1 WHERE branch_id = $2",
                pg.schema
            ),
            &[
                &over.branch.head_commit.map(|id| id.to_bytes().to_vec()),
                &fixture.branch.as_slice(),
            ],
        )
        .unwrap();
    let again = fused_commit_at(&fixture.committer, None, fixture.stack, &request, 77);
    assert!(matches!(again, Err(Fused::Race)), "{again:?}");
    assert_eq!(
        fixture.snapshot().branch.head_commit,
        over.branch.head_commit
    );
    assert!(fixture.history.commit(record.id).unwrap().is_some());
}

/// The pack statements a held tail's packs make: contiguous runs of at most
/// one ordinary pack's bytes, each holding at least one pack.
fn pack_runs(batch: &Publication) -> usize {
    let (mut runs, mut bytes) = (0, 0);
    for pack in &batch.packs {
        let length = pack.body().len();
        if runs == 0 || bytes + length > layerfs_storage::policy::PACK_LIMIT {
            (runs, bytes) = (runs + 1, 0);
        }
        bytes += length;
    }
    runs
}

/// A tail wider than one ordinary pack is sent as several pack statements
/// that stay in the Commit's one message: one round trip, one transaction,
/// every held object published and the Branch advanced.
#[test]
fn postgresql_fused_commit_splits_a_wide_tail() {
    let Some(pg) = Pg::new("wide") else { return };
    let handles = pg.open(30_000);
    let fixture = Fixture::new(
        handles.storage.clone(),
        &handles.history,
        Box::new(|| handles.diagnostics().unwrap()),
    );
    // Prepares every statement of a Commit with a parent.
    three_commits(&fixture);
    let commit = |seed: u32, len: usize| {
        let over = fixture.snapshot();
        let (tail, candidate) = held_bytes(&fixture.storage, seed, len);
        let batch = tail.publication().unwrap();
        let (runs, ids) = (pack_runs(batch), ids(&tail));
        let before = (fixture.work)();
        let outcome = fixture
            .fused(Some(batch), &fixture.request(&over, candidate), &[hint()])
            .unwrap();
        let after = (fixture.work)();
        assert!(
            matches!(outcome, CommitOutcome::Committed { .. }),
            "{outcome:?}"
        );
        tail.acknowledged().unwrap();
        assert_eq!(fixture.snapshot().effective_root, candidate);
        assert!(fixture.stored(&ids), "the held objects are published");
        let fresh = Storage::new(fixture.persistence.clone()).unwrap();
        fresh.reader().unwrap().read_objects(&[candidate]).unwrap();
        (
            runs,
            delta(&before, &after),
            after.statements - before.statements,
        )
    };
    let (narrow_runs, narrow, narrow_statements) = commit(80, 5000);
    let (wide_runs, wide, wide_statements) = commit(81, 1024 * 1024);
    assert_eq!(narrow_runs, 1);
    assert!(wide_runs >= 3, "a 1 MiB tail needs several statements");
    assert_eq!(narrow, [1, 0, 1, 1], "warm narrow: {narrow:?}");
    assert_eq!(wide, [1, 0, 1, 1], "warm wide: {wide:?}");
    assert_eq!(
        wide_statements - narrow_statements,
        (wide_runs - narrow_runs) as u64,
        "one more statement per extra run"
    );
}

/// A refused message on a fresh Store: `refuse` changes the Store (or holds
/// a lock) through the admin connection, then the attempt is checked.
fn refused(
    tag: &str,
    lock_timeout_ms: u32,
    refuse: impl FnOnce(&Pg, &mut postgres::Client, &Fixture<'_>) -> Option<std::thread::JoinHandle<()>>,
    check: impl FnOnce(&Result<CommitOutcome, Fused>),
) {
    let Some(pg) = Pg::new(tag) else { return };
    let handles = pg.open(lock_timeout_ms);
    let fixture = Fixture::new(
        handles.storage.clone(),
        &handles.history,
        Box::new(|| handles.diagnostics().unwrap()),
    );
    // Warm the message's statements with one Commit.
    let over = fixture.snapshot();
    let (tail, candidate) = held(&fixture.storage, 1);
    fixture
        .fused(tail.publication(), &fixture.request(&over, candidate), &[])
        .unwrap();
    tail.acknowledged().unwrap();
    let over = fixture.snapshot();
    let (tail, candidate) = held(&fixture.storage, 80);
    let ids = ids(&tail);
    let packs: Vec<i64> = tail
        .publication()
        .map(|batch| batch.packs.iter().map(|pack| pack.info().pack_id).collect())
        .unwrap_or_default();
    let mut admin = pg.admin();
    let helper = refuse(&pg, &mut admin, &fixture);
    let attempt = fixture.fused(
        tail.publication(),
        &fixture.request(&over, candidate),
        &[hint()],
    );
    if let Some(helper) = helper {
        helper.join().unwrap();
    }
    drop(admin);
    check(&attempt);
    if matches!(attempt, Err(Fused::History(HistoryError::UnknownOutcome))) {
        // Quarantined: every later call is refused before it is sent.
        assert!(matches!(
            fixture.history.branch_snapshot(fixture.branch),
            Err(HistoryError::UnknownOutcome)
        ));
    }
    // Nothing written, read on a connection of its own: the head, the
    // held packs and objects, the hint.
    let mut admin = pg.admin();
    let s = &pg.schema;
    let head: Option<Vec<u8>> = admin
        .query_one(
            &format!("SELECT head_commit_id FROM {s}.branch WHERE branch_id = $1"),
            &[&fixture.branch.as_slice()],
        )
        .unwrap()
        .get(0);
    assert_eq!(
        head,
        over.branch.head_commit.map(|id| id.to_bytes().to_vec())
    );
    let ids: Vec<Vec<u8>> = ids.iter().map(|id| id.as_bytes().to_vec()).collect();
    let count = |sql: String, admin: &mut postgres::Client| -> i64 {
        admin.query_one(&sql, &[]).unwrap().get(0)
    };
    let located: i64 = admin
        .query_one(
            &format!("SELECT count(*) FROM {s}.object_location WHERE object_id = ANY($1)"),
            &[&ids],
        )
        .unwrap()
        .get(0);
    assert_eq!(located, 0, "no held object is stored");
    let stored: i64 = admin
        .query_one(
            &format!("SELECT count(*) FROM {s}.pack WHERE pack_id = ANY($1)"),
            &[&packs],
        )
        .unwrap()
        .get(0);
    assert_eq!(stored, 0, "no held pack is stored");
    assert_eq!(
        count(
            format!(
                "SELECT count(*) FROM {s}.binding_hint_block WHERE grp = {}",
                HINT.child / 64
            ),
            &mut admin
        ),
        0,
        "no hint is recorded"
    );
}

#[test]
fn postgresql_generation_change_is_stale() {
    refused(
        "stale",
        30_000,
        |pg, admin, _| {
            admin
                .batch_execute(&format!(
                    "UPDATE {}.history_meta SET catalog_incarnation = catalog_incarnation + 1",
                    pg.schema
                ))
                .unwrap();
            None
        },
        |attempt| {
            assert!(
                matches!(attempt, Err(Fused::History(HistoryError::StaleStore))),
                "{attempt:?}"
            )
        },
    );
}

#[test]
fn postgresql_lane_wait_is_busy() {
    refused(
        "busy",
        300,
        |pg, admin, _| {
            admin
                .batch_execute(&format!(
                    "BEGIN; SELECT 1 FROM {}.store_lane WHERE id = 1 FOR UPDATE",
                    pg.schema
                ))
                .unwrap();
            None
        },
        |attempt| {
            assert!(
                matches!(attempt, Err(Fused::History(HistoryError::Busy))),
                "{attempt:?}"
            )
        },
    );
}

#[test]
fn postgresql_missing_epoch_row_is_a_conflict() {
    refused(
        "noepoch",
        30_000,
        |pg, admin, fixture| {
            admin
                .execute(
                    &format!(
                        "DELETE FROM {}.project_epoch WHERE layer_stack_id = $1",
                        pg.schema
                    ),
                    &[&fixture.stack.as_slice()],
                )
                .unwrap();
            None
        },
        |attempt| {
            assert!(
                matches!(
                    attempt,
                    Err(Fused::History(HistoryError::Conflict("project epoch")))
                ),
                "{attempt:?}"
            )
        },
    );
}

/// A Branch removal racing the message: its marker transaction retires the
/// Branch and bumps the project epoch in one transaction, here held open
/// until the message waits for the epoch row (`FOR SHARE`), then committed.
/// The message read the Branch and the epoch before it: `Conflict`.
#[test]
fn postgresql_removal_during_the_message_is_a_conflict() {
    refused(
        "removal",
        30_000,
        |pg, admin, fixture| {
            let stack = fixture.stack.as_slice().to_vec();
            let branch = fixture.branch.as_slice().to_vec();
            admin
                .batch_execute(&format!("BEGIN; SET LOCAL search_path TO {}", pg.schema))
                .unwrap();
            admin
                .execute(
                    "INSERT INTO branch_retired (branch_id, removed_ms) VALUES ($1, 1)",
                    &[&branch],
                )
                .unwrap();
            admin
                .execute(
                    "UPDATE project_epoch SET epoch = epoch + 1 WHERE layer_stack_id = $1",
                    &[&stack],
                )
                .unwrap();
            let blocker = pid(admin);
            let mut observer = pg.admin();
            let mut client = std::mem::replace(admin, pg.admin());
            Some(std::thread::spawn(move || {
                waiting_on(&mut observer, blocker);
                client.batch_execute("COMMIT").unwrap();
            }))
        },
        |attempt| {
            assert!(
                matches!(
                    attempt,
                    Err(Fused::History(HistoryError::Conflict("project epoch")))
                ),
                "{attempt:?}"
            )
        },
    );
}

/// The connection is ended while the message waits for the lane row: its
/// replies are lost, the outcome is unknown and the session is quarantined;
/// nothing runs on the separate path.
#[test]
fn postgresql_lost_reply_is_unknown() {
    refused(
        "lost",
        30_000,
        |pg, admin, _| {
            admin
                .batch_execute(&format!(
                    "BEGIN; SELECT 1 FROM {}.store_lane WHERE id = 1 FOR UPDATE",
                    pg.schema
                ))
                .unwrap();
            let blocker = pid(admin);
            let mut observer = pg.admin();
            let mut holder = std::mem::replace(admin, pg.admin());
            Some(std::thread::spawn(move || {
                let writer = waiting_on(&mut observer, blocker);
                observer
                    .execute("SELECT pg_terminate_backend($1)", &[&writer])
                    .unwrap();
                holder.batch_execute("ROLLBACK").unwrap();
            }))
        },
        |attempt| {
            assert!(
                matches!(attempt, Err(Fused::History(HistoryError::UnknownOutcome))),
                "{attempt:?}"
            )
        },
    );
}

/// A TCP proxy to the test server that, once armed, rewrites the next
/// fused lane request (simple query text with the attempt check and the
/// barrier) so its barrier statement sleeps between the lock and the
/// unlock: `pg_advisory_lock(..), pg_sleep(30)` inside the fenced
/// subquery. Server bytes pass through unchanged. [`Self::cut`] ends every
/// connection's client side, leaving the server side open.
struct BarrierPause {
    url: String,
    armed: Arc<std::sync::atomic::AtomicBool>,
    clients: Arc<std::sync::Mutex<Vec<std::net::TcpStream>>>,
}

impl BarrierPause {
    fn start(url: &str) -> Self {
        use std::net::{TcpListener, TcpStream, ToSocketAddrs};
        let config: layerfs_persistence::PostgresConfig = url.parse().unwrap();
        let layerfs_persistence::PostgresHost::Tcp(host) = &config.get_hosts()[0] else {
            panic!("the test server must be reached over TCP");
        };
        let port = config.get_ports().first().copied().unwrap_or(5432);
        let upstream = (host.as_str(), port)
            .to_socket_addrs()
            .unwrap()
            .next()
            .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut proxied = format!(
            "host=127.0.0.1 port={} user={} dbname={} sslmode=disable",
            listener.local_addr().unwrap().port(),
            config.get_user().unwrap(),
            config.get_dbname().unwrap_or(config.get_user().unwrap()),
        );
        if let Some(password) = config.get_password() {
            proxied.push_str(&format!(
                " password={}",
                String::from_utf8(password.to_vec()).unwrap()
            ));
        }
        let armed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = armed.clone();
        let clients: Arc<std::sync::Mutex<Vec<TcpStream>>> = Arc::default();
        let kept = clients.clone();
        std::thread::spawn(move || {
            for client in listener.incoming() {
                let Ok(client) = client else { return };
                kept.lock().unwrap().push(client.try_clone().unwrap());
                let Ok(server) = TcpStream::connect(upstream) else {
                    return;
                };
                let (mut back_from, mut back_to) =
                    (server.try_clone().unwrap(), client.try_clone().unwrap());
                std::thread::spawn(move || {
                    let _ = std::io::copy(&mut back_from, &mut back_to);
                    let _ = back_to.shutdown(std::net::Shutdown::Write);
                });
                let flag = flag.clone();
                std::thread::spawn(move || {
                    let _ = rewrite(client, server, &flag);
                });
            }
        });
        Self {
            url: proxied,
            armed,
            clients,
        }
    }

    /// Ends the client side of every connection so far: each client sees
    /// its connection end without replies (a lost reply), while the server
    /// side stays open and its backend keeps running what it was sent.
    fn cut(&self) {
        for client in self.clients.lock().unwrap().iter() {
            let _ = client.shutdown(std::net::Shutdown::Both);
        }
    }
}

/// Client to server, message by message: the untyped startup (after any
/// SSL or GSS request), then typed messages; the armed lane query is
/// rewritten once.
fn rewrite(
    mut client: std::net::TcpStream,
    mut server: std::net::TcpStream,
    armed: &std::sync::atomic::AtomicBool,
) -> std::io::Result<()> {
    use std::io::{Read, Write};
    use std::sync::atomic::Ordering;
    let mut untyped = true;
    loop {
        let mut kind = [0u8; 1];
        if !untyped {
            client.read_exact(&mut kind)?;
        }
        let mut length = [0u8; 4];
        client.read_exact(&mut length)?;
        let mut body = vec![0u8; u32::from_be_bytes(length) as usize - 4];
        client.read_exact(&mut body)?;
        if untyped {
            let code = body
                .get(..4)
                .map(|c| u32::from_be_bytes(c.try_into().unwrap()));
            // SSLRequest and GSSENCRequest are followed by another startup.
            untyped = matches!(code, Some(80_877_103 | 80_877_104));
            server.write_all(&length)?;
            server.write_all(&body)?;
            continue;
        }
        let text = String::from_utf8_lossy(&body);
        if kind[0] == b'Q'
            && text.contains("layerfs.attempt")
            && text.contains("pg_advisory_lock(")
            && armed.swap(false, Ordering::SeqCst)
        {
            let paused = text.replace(
                " OFFSET 0) AS barrier",
                ", pg_sleep(30) OFFSET 0) AS barrier",
            );
            assert_ne!(paused, text, "the lane request carries the barrier");
            body = paused.into_bytes();
        }
        server.write_all(&kind)?;
        server.write_all(&(body.len() as u32 + 4).to_be_bytes())?;
        server.write_all(&body)?;
    }
}

/// #776 P6 on the fused path: the message's barrier statement is cancelled
/// after its lock and before its unlock (`pg_cancel_backend` while it
/// sleeps between them). The refusal is definite, so the session stays
/// open; the barrier must not stay held on it: `pg_locks` shows no
/// advisory lock on that backend, and another writer, whose barrier takes
/// the key exclusively, commits afterwards instead of waiting until its
/// `lock_timeout`.
#[test]
fn postgresql_cancelled_barrier_is_released() {
    let Some(pg) = Pg::new("cancel") else { return };
    let proxy = BarrierPause::start(&pg.url);
    let mut selection = PostgresSelection::new(&proxy.url, &pg.schema);
    selection.lock_timeout_ms = 30_000;
    let handles = PostgresHandles::open_writable(&selection, BINDING, CURSOR).unwrap();
    let fixture = Fixture::new(
        handles.storage.clone(),
        &handles.history,
        Box::new(|| handles.diagnostics().unwrap()),
    );
    // Warm the message's statements with two Commits (the second has a
    // parent line), so the cancelled attempt sends no prepares.
    for seed in [1, 2] {
        let over = fixture.snapshot();
        let (tail, candidate) = held(&fixture.storage, seed);
        fixture
            .fused(tail.publication(), &fixture.request(&over, candidate), &[])
            .unwrap();
        tail.acknowledged().unwrap();
    }

    let over = fixture.snapshot();
    let (tail, candidate) = held(&fixture.storage, 81);
    let held_ids = ids(&tail);
    proxy.armed.store(true, std::sync::atomic::Ordering::SeqCst);
    let mut observer = pg.admin();
    let canceller = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let row = observer
                .query_opt(
                    "SELECT pid FROM pg_stat_activity \
                      WHERE wait_event = 'PgSleep' AND query LIKE '%pg_advisory_lock(%' LIMIT 1",
                    &[],
                )
                .unwrap();
            if let Some(row) = row {
                let writer: i32 = row.get(0);
                observer
                    .execute("SELECT pg_cancel_backend($1)", &[&writer])
                    .unwrap();
                return (writer, observer);
            }
            assert!(Instant::now() < deadline, "the barrier never paused");
            std::thread::sleep(Duration::from_millis(20));
        }
    });
    let before = handles.diagnostics().unwrap();
    let attempt = fixture.fused(tail.publication(), &fixture.request(&over, candidate), &[]);
    let after = handles.diagnostics().unwrap();
    let (writer, mut observer) = canceller.join().unwrap();
    assert!(
        matches!(
            attempt,
            Err(Fused::History(HistoryError::OwnershipUnavailable))
        ),
        "a cancelled statement is a definite refusal: {attempt:?}"
    );
    fixture.nothing_written("cancelled", &over, &held_ids);
    tail.refused();
    let barriers: i64 = observer
        .query_one(
            "SELECT count(*) FROM pg_locks \
              WHERE locktype = 'advisory' AND pid = $1 AND granted AND objsubid = 2",
            &[&writer],
        )
        .unwrap()
        .get(0);
    assert_eq!(
        barriers, 0,
        "no barrier is left held on the writer's backend"
    );
    // The message, then the leftover release on its failure path.
    assert_eq!(
        delta(&before, &after)[..2],
        [2, 0],
        "{before:?} -> {after:?}"
    );

    // Another writer takes the barrier exclusively; the leftover would make
    // it wait until its lock_timeout (`Busy`).
    let other = pg.open(2_000);
    let storage = Storage::new(other.storage.clone()).unwrap();
    let over = fixture.snapshot();
    let (tail, candidate) = held(&storage, 82);
    let outcome = other.history.committer().commit(
        tail.publication(),
        fixture.stack,
        &fixture.request(&over, candidate),
        &[],
    );
    assert!(
        matches!(outcome, Ok(CommitOutcome::Committed { .. })),
        "{outcome:?}"
    );
    tail.acknowledged().unwrap();
    // The cancelled writer's session is still usable.
    let over = fixture.snapshot();
    let (tail, candidate) = held(&fixture.storage, 83);
    let again = fixture.fused(tail.publication(), &fixture.request(&over, candidate), &[]);
    assert!(
        matches!(again, Ok(CommitOutcome::Committed { .. })),
        "{again:?}"
    );
    tail.acknowledged().unwrap();
}

/// A lost reply's resolution never terminates a backend it cannot name
/// exactly. The writer runs under a role that may not see its own login's
/// activity row (`-c role=`), so the open's check reads no backend start.
/// The fused message is held in its barrier's sleep and the connection's
/// client side is cut: an unknown outcome, the session quarantined, its
/// backend still running. The lane pass (`pass_publication_lane`) must
/// answer unknown and send no `pg_terminate_backend`: the backend is still
/// listed, with the same start, until the test ends it.
#[test]
fn postgresql_lost_backend_without_start_is_not_terminated() {
    let Some(pg) = Pg::new("nostart") else { return };
    let role = format!("lfs768_nostart_{}", std::process::id());
    let s = &pg.schema;
    let mut admin = pg.admin();
    admin
        .batch_execute(&format!(
            "DROP ROLE IF EXISTS {role}; CREATE ROLE {role}; \
             GRANT USAGE ON SCHEMA {s} TO {role}; \
             GRANT ALL ON ALL TABLES IN SCHEMA {s} TO {role}; \
             GRANT ALL ON ALL SEQUENCES IN SCHEMA {s} TO {role}"
        ))
        .unwrap();
    let proxy = BarrierPause::start(&pg.url);
    let url = format!("{} options='-c role={role}'", proxy.url);
    let mut selection = PostgresSelection::new(&url, s);
    selection.lock_timeout_ms = 30_000;
    let handles = PostgresHandles::open_writable(&selection, BINDING, CURSOR).unwrap();
    let fixture = Fixture::new(
        handles.storage.clone(),
        &handles.history,
        Box::new(|| handles.diagnostics().unwrap()),
    );
    let over = fixture.snapshot();
    let (tail, candidate) = held(&fixture.storage, 91);
    proxy.armed.store(true, std::sync::atomic::Ordering::SeqCst);
    let mut observer = pg.admin();
    let (attempt, (writer, start)) = std::thread::scope(|scope| {
        let proxy = &proxy;
        let cutter = scope.spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let row = observer
                    .query_opt(
                        "SELECT pid, backend_start::text FROM pg_stat_activity \
                          WHERE wait_event = 'PgSleep' AND query LIKE '%pg_advisory_lock(%' \
                          LIMIT 1",
                        &[],
                    )
                    .unwrap();
                if let Some(row) = row {
                    proxy.cut();
                    return (row.get::<_, i32>(0), row.get::<_, String>(1));
                }
                assert!(Instant::now() < deadline, "the barrier never paused");
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        let attempt = fixture.fused(tail.publication(), &fixture.request(&over, candidate), &[]);
        (attempt, cutter.join().unwrap())
    });
    assert!(
        matches!(attempt, Err(Fused::History(HistoryError::UnknownOutcome))),
        "a lost reply: {attempt:?}"
    );
    let passed = fixture.history.pass_publication_lane();
    assert!(
        matches!(passed, Err(HistoryError::UnknownOutcome)),
        "no backend start, no termination: {passed:?}"
    );
    let listed: Option<String> = admin
        .query_opt(
            "SELECT backend_start::text FROM pg_stat_activity WHERE pid = $1",
            &[&writer],
        )
        .unwrap()
        .map(|row| row.get(0));
    assert_eq!(
        listed.as_deref(),
        Some(start.as_str()),
        "the writer's backend was not terminated"
    );
    admin
        .execute("SELECT pg_terminate_backend($1, 10000)", &[&writer])
        .unwrap();
    drop(tail);
    drop(fixture);
    drop(handles);
    admin
        .batch_execute(&format!("DROP OWNED BY {role}; DROP ROLE {role}"))
        .unwrap();
}
