//! #768 steps B and C: a Branch mount's registration on PostgreSQL as one pipelined
//! message (the session keys, the Branch probe, the conflict scan, the
//! insert and `COMMIT`), checked against the statement-by-statement
//! registration it replaces.
//!
//! * Equivalence over seeded random sets of existing intents (every kind,
//!   Store, project and Branch scopes of the mount's project and another,
//!   live, uncertain and done, with and without owners): the one message
//!   admits exactly when the shared checks (`validate_mount`,
//!   `conflicts_for`) find nothing to refuse, and the whole registration
//!   (the message, then the statement-by-statement fallback on a refusal)
//!   answers exactly as the statement-by-statement registration alone, the
//!   refusal and its holders included. A missing and a retired Branch, a
//!   done row of the same op and a live re-registration of the same op are
//!   checked the same way. No refusal leaves a key held.
//! * Races between two sessions of one Store: a mount and a project removal,
//!   and a mount and a compaction, are never both admitted; two mounts of
//!   one Branch are both admitted.
//! * A message whose insert waits on a table lock and whose backend is then
//!   terminated is an unknown outcome: the call fails, no row is written,
//!   the backend ends with every key and the session refuses further calls.
//! * A message whose insert reaches the session's lock timeout is a definite
//!   refusal: its keys are given back and the fallback reports `Busy`.
//! * Step C: an owner release whose delete the server refuses keeps its row
//!   and its keys; the next release frees both in one message.
//!
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
mod support;

use layerfs_history::error::HistoryError;
use layerfs_history::records::{GcIntent, GcIntentKind, GcScopeKind};
use layerfs_history::{BranchId, CatalogId, HistoryCatalog, LayerStackId};
use layerfs_persistence::internals::{
    mount_one_message, mount_separately_refuses, register_separately,
};
use layerfs_persistence::PostgresHandles;
use layerfs_storage::StoragePolicy;
use std::sync::Barrier;
use std::time::{Duration, Instant};
use support::pg::{branch_id, catalog, project, url, Schema};

const CURSOR: [u8; 32] = [77; 32];
const LOCK_TIMEOUT_MS: u32 = 5_000;
/// The mount under test registers with this op tag.
const MOUNT: u8 = 0xA0;

fn binding(tag: &str) -> Vec<u8> {
    format!("postgresql-mount-pipeline-{tag}-{}", std::process::id()).into_bytes()
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
    .placed(Some([tag; 32]), Some("sandbox"), Some("machine"))
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

fn open(schema: &Schema, binding: &[u8], lock_timeout_ms: u32, create: bool) -> PostgresHandles {
    let mut selection = schema.selection();
    selection.lock_timeout_ms = lock_timeout_ms;
    if create {
        PostgresHandles::create(
            &selection,
            StoragePolicy::frozen_default(),
            &catalog(binding, CURSOR, 3),
        )
        .expect("PostgreSQL Store creation")
    } else {
        PostgresHandles::open_writable(&selection, binding, CURSOR).expect("PostgreSQL session")
    }
}

/// The session advisory key of `kind` and `id` (spec 5.2).
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

/// Whether no backend holds any of a Branch mount's three keys.
fn keys_free(
    admin: &mut postgres::Client,
    binding: &[u8],
    stack: LayerStackId,
    branch: BranchId,
) -> bool {
    let catalog = CatalogId::derive(binding).unwrap();
    [
        advisory_key(binding, 1, catalog.as_slice()),
        advisory_key(binding, 2, stack.as_slice()),
        advisory_key(binding, 3, branch.as_slice()),
    ]
    .into_iter()
    .all(|key| {
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
    })
}

fn rows(admin: &mut postgres::Client) -> i64 {
    admin
        .query_one("SELECT count(*) FROM gc_intent", &[])
        .unwrap()
        .get(0)
}

/// One existing intent row, written by the admin connection.
#[derive(Clone, Debug)]
struct Row {
    op: Vec<u8>,
    kind: &'static str,
    scope_kind: &'static str,
    scope_id: Vec<u8>,
    state: &'static str,
    owner_runtime: Option<String>,
    owner_boot: Option<String>,
}

fn insert(admin: &mut postgres::Client, rows: &[Row]) {
    admin.execute("DELETE FROM gc_intent", &[]).unwrap();
    for row in rows {
        admin
            .execute(
                "INSERT INTO gc_intent (op_id, kind, scope_kind, scope_id, project, owner_runtime, \
                 owner_boot, state, uncertain, phase, updated_ms) \
                 VALUES ($1, $2, $3, $4, 'other', $5, $6, $7, $7 = 'uncertain', 'mark', 1)",
                &[
                    &row.op,
                    &row.kind,
                    &row.scope_kind,
                    &row.scope_id,
                    &row.owner_runtime,
                    &row.owner_boot,
                    &row.state,
                ],
            )
            .unwrap();
    }
}

/// A small deterministic generator (xorshift64*), so a failing set is
/// reproduced from its seed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn pick<T: Copy>(&mut self, from: &[T]) -> T {
        from[(self.next() % from.len() as u64) as usize]
    }
}

/// The answer a registration gave, compared whole (variant and holders).
fn outcome(result: &Result<(), HistoryError>) -> String {
    match result {
        Ok(()) => "admitted".to_owned(),
        Err(error) => format!("{error:?}"),
    }
}

/// The full registration and the statement-by-statement one, each from the
/// same rows; each admitted mount is released again.
fn final_decisions(
    handles: &PostgresHandles,
    admin: &mut postgres::Client,
    set: &[Row],
    intent: &GcIntent,
) -> (String, String) {
    insert(admin, set);
    let full = handles.history.gc_register(intent);
    if full.is_ok() {
        handles.history.gc_release(&intent.op_id).unwrap();
    }
    insert(admin, set);
    let separate = register_separately(&handles.history, intent);
    if separate.is_ok() {
        handles.history.gc_release(&intent.op_id).unwrap();
    }
    (outcome(&full), outcome(&separate))
}

#[test]
fn the_one_message_admits_exactly_what_the_shared_checks_admit() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "mount_equivalence");
    let binding = binding("equivalence");
    let handles = open(&schema, &binding, LOCK_TIMEOUT_MS, true);
    let (stack, branch, _) = project(&handles.history, 1);
    let (other_stack, other_branch, _) = project(&handles.history, 2);
    let catalog = CatalogId::derive(&binding).unwrap().as_slice().to_vec();
    let intent = mount(stack, branch, MOUNT);
    let mut admin = schema.admin();
    let scopes: [(&str, Vec<u8>); 5] = [
        ("store", catalog),
        ("project", stack.as_slice().to_vec()),
        ("project", other_stack.as_slice().to_vec()),
        ("branch", branch.as_slice().to_vec()),
        ("branch", other_branch.as_slice().to_vec()),
    ];
    let seed = 0x0768_B0B0_u64 ^ u64::from(std::process::id());
    println!("MOUNT_EQUIVALENCE_SEED {seed:#x}");
    let mut rng = Rng(seed | 1);
    let (mut admitted, mut refused) = (0, 0);
    for set_index in 0..48u32 {
        let size = (rng.next() % 4) as usize;
        let set: Vec<Row> = (0..size)
            .map(|row| {
                let scope = &scopes[(rng.next() % scopes.len() as u64) as usize];
                let mut op_id = rng.next().to_be_bytes().to_vec();
                op_id.extend_from_slice(&(set_index * 16 + row as u32).to_be_bytes());
                op_id.extend_from_slice(&[0, 0, 0, 1]);
                Row {
                    op: op_id,
                    kind: rng.pick(&["mount", "init", "merge", "removal", "compaction"]),
                    scope_kind: scope.0,
                    scope_id: scope.1.clone(),
                    state: rng.pick(&["live", "live", "uncertain", "done"]),
                    owner_runtime: rng.pick(&[None, Some("runtime-x")]).map(str::to_owned),
                    owner_boot: rng
                        .pick(&[None, Some(())])
                        .map(|()| format!("boot-{set_index}-{row}")),
                }
            })
            .collect();
        let label = format!("set {set_index} (seed {seed:#x}): {set:?}");

        insert(&mut admin, &set);
        let shared = mount_separately_refuses(&handles.history, &intent).unwrap();
        let fast = mount_one_message(&handles.history, &intent)
            .unwrap_or_else(|error| panic!("{label}: the one message failed: {error:?}"));
        assert_eq!(
            fast, !shared,
            "{label}: the one message against the shared checks"
        );
        if fast {
            admitted += 1;
            handles.history.gc_release(&intent.op_id).unwrap();
        } else {
            refused += 1;
            assert_eq!(
                rows(&mut admin),
                set.len() as i64,
                "{label}: a refusal writes nothing"
            );
        }
        assert!(
            keys_free(&mut admin, &binding, stack, branch),
            "{label}: a key leaked"
        );

        let (full, separate) = final_decisions(&handles, &mut admin, &set, &intent);
        assert_eq!(full, separate, "{label}: the final decision");
        assert!(
            keys_free(&mut admin, &binding, stack, branch),
            "{label}: a key leaked"
        );
    }
    println!("MOUNT_EQUIVALENCE {{\"admitted\":{admitted},\"refused\":{refused}}}");
    assert!(admitted > 0 && refused > 0, "the sets cover both answers");
}

#[test]
fn edge_cases_reach_the_same_final_decision() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "mount_edges");
    let binding = binding("edges");
    let handles = open(&schema, &binding, LOCK_TIMEOUT_MS, true);
    let (stack, branch, _) = project(&handles.history, 1);
    let (retired_stack, retired, _) = project(&handles.history, 3);
    let mut admin = schema.admin();
    admin
        .execute(
            "INSERT INTO branch_retired (branch_id, removed_ms) VALUES ($1, 1)",
            &[&retired.as_slice()],
        )
        .unwrap();
    let same_op = |state: &'static str, boot: &str| Row {
        op: op(MOUNT).to_vec(),
        kind: "mount",
        scope_kind: "branch",
        scope_id: branch.as_slice().to_vec(),
        state,
        owner_runtime: None,
        owner_boot: Some(boot.to_owned()),
    };
    let cases: [(&str, GcIntent, Vec<Row>, bool, &str); 4] = [
        (
            "missing Branch",
            mount(stack, branch_id(99), MOUNT),
            vec![],
            true,
            "Missing",
        ),
        (
            "retired Branch",
            mount(retired_stack, retired, MOUNT),
            vec![],
            true,
            "Missing",
        ),
        // The one message never revives or replaces a row of its op: the
        // fallback decides, refusing a retained done row and replacing a
        // live one.
        (
            "done row of the op",
            mount(stack, branch, MOUNT),
            vec![same_op("done", "boot-old")],
            false,
            "Conflict",
        ),
        (
            "live row of the op",
            mount(stack, branch, MOUNT),
            vec![same_op("live", "boot-old")],
            false,
            "admitted",
        ),
    ];
    for (name, intent, set, shared_refuses, expected) in cases {
        insert(&mut admin, &set);
        let shared = mount_separately_refuses(&handles.history, &intent).unwrap();
        assert_eq!(shared, shared_refuses, "{name}: the shared checks");
        let fast = mount_one_message(&handles.history, &intent).unwrap();
        assert!(!fast, "{name}: the one message refuses");
        assert_eq!(
            rows(&mut admin),
            set.len() as i64,
            "{name}: nothing written"
        );
        let stack = LayerStackId::from_slice(intent.project_id.as_deref().unwrap()).unwrap();
        let scope = BranchId::from_slice(&intent.scope_id).unwrap();
        assert!(
            keys_free(&mut admin, &binding, stack, scope),
            "{name}: a key leaked"
        );
        let (full, separate) = final_decisions(&handles, &mut admin, &set, &intent);
        assert_eq!(full, separate, "{name}: the final decision");
        assert!(full.starts_with(expected), "{name}: {full}");
        assert!(
            keys_free(&mut admin, &binding, stack, scope),
            "{name}: a key leaked"
        );
    }
}

/// Runs `first` and `second` at once, released together by a barrier.
fn race<A: Send, B: Send>(
    first: impl FnOnce() -> A + Send,
    second: impl FnOnce() -> B + Send,
) -> (A, B) {
    let barrier = Barrier::new(2);
    std::thread::scope(|scope| {
        let a = scope.spawn(|| {
            barrier.wait();
            first()
        });
        let b = scope.spawn(|| {
            barrier.wait();
            second()
        });
        (a.join().expect("first"), b.join().expect("second"))
    })
}

fn excluded(error: &HistoryError) -> bool {
    matches!(
        error,
        HistoryError::MountsActive(_) | HistoryError::BusyOp(_) | HistoryError::Busy
    )
}

#[test]
fn a_mount_and_an_exclusive_operation_racing_are_never_both_admitted() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "mount_races");
    let binding = binding("races");
    let a = open(&schema, &binding, LOCK_TIMEOUT_MS, true);
    let b = open(&schema, &binding, LOCK_TIMEOUT_MS, false);
    let (stack, branch, _) = project(&a.history, 1);
    let mut admin = schema.admin();
    let mut counts = std::collections::BTreeMap::<String, u32>::new();
    for round in 0..24u8 {
        let other = if round % 2 == 0 {
            compaction(&binding, 0x10 + round)
        } else {
            removal(stack, 0x10 + round)
        };
        let intent = mount(stack, branch, 0x40 + round);
        // Staggered by up to a few milliseconds either way, so each side
        // sometimes reaches the keys first.
        let (mount_delay, other_delay) =
            [(0, 0), (0, 2), (0, 6), (2, 0)][usize::from(round / 2 % 4)];
        let (mounted, operation) = race(
            || {
                std::thread::sleep(Duration::from_millis(mount_delay));
                a.history.gc_register(&intent)
            },
            || {
                std::thread::sleep(Duration::from_millis(other_delay));
                b.history.gc_register(&other)
            },
        );
        let label = format!(
            "round {round}: mount {mounted:?}, {:?} {operation:?}",
            other.kind
        );
        assert!(
            !(mounted.is_ok() && operation.is_ok()),
            "{label}: both admitted"
        );
        if let Err(error) = &mounted {
            assert!(excluded(error), "{label}");
        }
        if let Err(error) = &operation {
            assert!(excluded(error), "{label}");
        }
        let key = format!(
            "{:?}: mount {}, operation {}",
            other.kind,
            mounted.is_ok(),
            operation.is_ok()
        );
        *counts.entry(key).or_default() += 1;
        if mounted.is_ok() {
            a.history.gc_release(&intent.op_id).unwrap();
        }
        if operation.is_ok() {
            b.history.gc_release(&other.op_id).unwrap();
        }
        assert_eq!(rows(&mut admin), 0, "{label}");
        assert!(
            keys_free(&mut admin, &binding, stack, branch),
            "{label}: a key leaked"
        );
    }
    println!("MOUNT_RACES {counts:?}");
}

#[test]
fn two_sessions_mounting_one_branch_are_both_admitted() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "mount_twice");
    let binding = binding("twice");
    let a = open(&schema, &binding, LOCK_TIMEOUT_MS, true);
    let b = open(&schema, &binding, LOCK_TIMEOUT_MS, false);
    let (stack, branch, _) = project(&a.history, 1);
    let mut admin = schema.admin();
    for round in 0..8u8 {
        let (first, second) = (
            mount(stack, branch, 0x20 + round),
            mount(stack, branch, 0x60 + round),
        );
        let (one, two) = race(
            || a.history.gc_register(&first),
            || b.history.gc_register(&second),
        );
        one.unwrap();
        two.unwrap();
        assert_eq!(rows(&mut admin), 2, "round {round}");
        a.history.gc_release(&first.op_id).unwrap();
        b.history.gc_release(&second.op_id).unwrap();
        assert!(
            keys_free(&mut admin, &binding, stack, branch),
            "round {round}"
        );
    }
}

/// The backend of a session whose statement waits on `gc_intent`'s lock.
fn waiting_backend(admin: &mut postgres::Client) -> Option<i32> {
    admin
        .query_opt(
            "SELECT pid FROM pg_locks WHERE relation = 'gc_intent'::regclass AND NOT granted \
             LIMIT 1",
            &[],
        )
        .unwrap()
        .map(|row| row.get(0))
}

fn wait_for<T>(limit: Duration, mut probe: impl FnMut() -> Option<T>) -> Option<T> {
    let start = Instant::now();
    loop {
        if let Some(found) = probe() {
            return Some(found);
        }
        if start.elapsed() > limit {
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_backend_terminated_while_the_insert_waits_is_an_unknown_outcome() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "mount_killed");
    let binding = binding("killed");
    let handles = open(&schema, &binding, LOCK_TIMEOUT_MS, true);
    let (stack, branch, _) = project(&handles.history, 1);
    // Warm: every statement of the message is prepared, so only the insert
    // touches `gc_intent`.
    let warm = mount(stack, branch, 1);
    assert!(mount_one_message(&handles.history, &warm).unwrap());
    handles.history.gc_release(&warm.op_id).unwrap();

    let mut locker = schema.admin();
    let mut admin = schema.admin();
    locker
        .batch_execute("BEGIN; LOCK TABLE gc_intent IN ACCESS EXCLUSIVE MODE")
        .unwrap();
    let intent = mount(stack, branch, MOUNT);
    let (answer, pid) = std::thread::scope(|scope| {
        let call = scope.spawn(|| mount_one_message(&handles.history, &intent));
        let pid = wait_for(Duration::from_secs(4), || waiting_backend(&mut admin))
            .expect("the insert waits on the table lock");
        let held: i64 = admin
            .query_one(
                "SELECT count(*) FROM pg_locks WHERE locktype = 'advisory' AND objsubid = 1 \
                 AND granted AND pid = $1",
                &[&pid],
            )
            .unwrap()
            .get(0);
        assert!(held >= 3, "the keys were granted before the insert: {held}");
        admin
            .query_one("SELECT pg_terminate_backend($1)", &[&pid])
            .unwrap();
        (call.join().expect("the call"), pid)
    });
    locker.batch_execute("ROLLBACK").unwrap();
    assert!(answer.is_err(), "a lost reply is not an answer: {answer:?}");
    let gone = wait_for(Duration::from_secs(5), || {
        let alive: i64 = admin
            .query_one(
                "SELECT count(*) FROM pg_stat_activity WHERE pid = $1",
                &[&pid],
            )
            .unwrap()
            .get(0);
        (alive == 0).then_some(())
    });
    assert!(gone.is_some(), "the terminated backend ended");
    assert_eq!(rows(&mut admin), 0, "the uncommitted insert wrote nothing");
    assert!(keys_free(&mut admin, &binding, stack, branch));
    // The session was quarantined: it sends nothing further.
    assert!(handles
        .history
        .gc_register(&mount(stack, branch, 2))
        .is_err());
    assert_eq!(rows(&mut admin), 0);
}

#[test]
fn an_insert_reaching_the_lock_timeout_gives_its_keys_back() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "mount_lock_timeout");
    let binding = binding("lock_timeout");
    drop(open(&schema, &binding, LOCK_TIMEOUT_MS, true));
    let handles = open(&schema, &binding, 300, false);
    let (stack, branch, _) = project(&handles.history, 1);
    // Warm both registrations' statements.
    let warm = mount(stack, branch, 1);
    handles.history.gc_register(&warm).unwrap();
    handles.history.gc_release(&warm.op_id).unwrap();
    register_separately(&handles.history, &warm).unwrap();
    handles.history.gc_release(&warm.op_id).unwrap();

    let mut locker = schema.admin();
    let mut admin = schema.admin();
    locker
        .batch_execute("BEGIN; LOCK TABLE gc_intent IN ACCESS EXCLUSIVE MODE")
        .unwrap();
    let intent = mount(stack, branch, MOUNT);
    assert!(
        !mount_one_message(&handles.history, &intent).unwrap(),
        "a lock timeout at the insert is a refusal"
    );
    assert!(
        keys_free(&mut admin, &binding, stack, branch),
        "the one message gave its keys back"
    );
    let refusal = handles.history.gc_register(&intent).unwrap_err();
    assert!(matches!(refusal, HistoryError::Busy), "{refusal:?}");
    assert!(
        keys_free(&mut admin, &binding, stack, branch),
        "the fallback gave its keys back"
    );
    locker.batch_execute("ROLLBACK").unwrap();
    assert_eq!(rows(&mut admin), 0);
    handles.history.gc_register(&intent).unwrap();
    handles.history.gc_release(&intent.op_id).unwrap();
}

/// #768 step C: the owner release's delete and unlock are one message, and
/// the unlock gives the keys back only when no live row of the op remains.
/// A delete the server refuses (here a trigger raises) keeps the row and the
/// keys, held with the op; the next release, once the delete can run, frees
/// both in one round trip.
#[test]
fn a_refused_release_keeps_its_row_and_its_keys() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "release_refused");
    let binding = binding("release_refused");
    let handles = open(&schema, &binding, LOCK_TIMEOUT_MS, true);
    let (stack, branch, _) = project(&handles.history, 1);
    let intent = mount(stack, branch, MOUNT);
    handles.history.gc_register(&intent).unwrap();
    let mut admin = schema.admin();
    admin
        .batch_execute(
            "CREATE FUNCTION refuse_delete() RETURNS trigger LANGUAGE plpgsql AS \
             $$ BEGIN RAISE EXCEPTION 'release refused'; END $$; \
             CREATE TRIGGER refuse_delete BEFORE DELETE ON gc_intent \
             FOR EACH ROW EXECUTE FUNCTION refuse_delete();",
        )
        .unwrap();
    let refusal = handles.history.gc_release(&intent.op_id).unwrap_err();
    assert!(
        !matches!(refusal, HistoryError::UnknownOutcome),
        "{refusal:?}"
    );
    assert_eq!(rows(&mut admin), 1, "the refused delete kept the row");
    assert!(
        !keys_free(&mut admin, &binding, stack, branch),
        "the refused release kept the keys"
    );

    admin
        .batch_execute("DROP TRIGGER refuse_delete ON gc_intent")
        .unwrap();
    let before = handles.diagnostics().unwrap();
    handles.history.gc_release(&intent.op_id).unwrap();
    let after = handles.diagnostics().unwrap();
    assert_eq!(
        (
            after.round_trips - before.round_trips,
            after.statements - before.statements
        ),
        (1, 2),
        "one message: the delete and the unlock"
    );
    assert_eq!(rows(&mut admin), 0);
    assert!(
        keys_free(&mut admin, &binding, stack, branch),
        "the keys were given back"
    );
}
