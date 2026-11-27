//! #41 T10.5: the PostgreSQL `store.status` compaction projection.
//!
//! `store.status` reads the Store-side `gc_op` row with its compaction
//! `gc_intent` in one status query — no Store open, schema mutation or
//! artifact read — and projects the same shape as the SQLite sidecar: the op
//! `{id, name, phase, started, uncertain}` from the row, `state` from the
//! compaction intent and the owner's liveness — in flight only while the
//! intent is present and the recorded backend still lives — and PostgreSQL's
//! `mark`/`build`/`swap`/`cleanup` phases. An idle Store (no row, or a run that
//! settled and removed it) projects `None`. The record's own fact `uncertain`
//! stays true for every remaining row; `state` is what separates a busy run
//! from a returned run's or a gone owner's leftover.
//!
//! Needs a writable PostgreSQL server: set `LAYERFS_TEST_POSTGRES_URL` (for
//! example `host=127.0.0.1 port=55432 user=postgres dbname=lfs_t105
//! sslmode=disable`). Without it each test reports NOT_RUN.
#[path = "../support/mod.rs"]
mod support;

use layerfs_content::{encode_whole_file_payload, FinalizedObject, ObjectId, ObjectRole};
use layerfs_history::HistoryCatalog as _;
use layerfs_persistence::{
    postgresql_compact_status, CompactFailpoint, CompactRequest, CompactStatus, PostgresHandles,
};
use layerfs_storage::Storage;
use support::pg::Schema;

fn binding(tag: &str) -> Vec<u8> {
    format!("postgresql-status-{tag}").into_bytes()
}

fn whole_file(handles: &PostgresHandles, bytes: &[u8]) -> ObjectId {
    let object = FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(bytes).unwrap(),
    )
    .unwrap();
    save_objects(handles, &[object])
}

fn save_objects(handles: &PostgresHandles, objects: &[FinalizedObject]) -> ObjectId {
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    let mut last = None;
    for object in objects {
        last = Some(object.id());
        save.accept(object.clone()).unwrap();
    }
    save.finish().unwrap();
    last.expect("at least one object")
}

fn request(op: u8, failpoint: Option<CompactFailpoint>) -> CompactRequest {
    CompactRequest {
        op_id: [op; 16],
        failpoint,
        ..CompactRequest::default()
    }
}

/// The live §8 phases, in order, as `store.status` names them.
const PHASES: [&str; 4] = ["mark", "build", "swap", "cleanup"];

/// Waits until the recorded owner backend is gone from `pg_stat_activity`.
fn wait_gone(admin: &mut postgres::Client, pid: i32, start: &str) {
    for _ in 0..400 {
        let live: bool = admin
            .query_one(
                "SELECT EXISTS (SELECT 1 FROM pg_stat_activity \
                  WHERE pid=$1 AND backend_start::text=$2)",
                &[&pid, &start],
            )
            .unwrap()
            .get(0);
        if !live {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("the recorded owner backend never ended");
}

#[test]
fn the_projection_reads_gc_op_gc_intent_and_owner_liveness() {
    let Some(url) = support::pg::url() else {
        return;
    };
    let schema = Schema::new(url.clone(), "row");
    let _handles = schema.create(&binding("row"), [71; 32]);
    let name = schema.name.clone();
    let s = &schema.name;

    // Idle: no compaction row, so no op and no phases.
    let idle = postgresql_compact_status(&url, &name).unwrap();
    assert!(idle.is_none(), "an idle Store projects no op");

    // A live owner: an open, non-idle backend is exactly what the §9
    // dead-owner rule treats as "still running the op".
    let mut owner = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    owner.batch_execute("BEGIN; SELECT 1").unwrap();
    let (pid, start): (i32, String) = {
        let row = owner
            .query_one(
                "SELECT pg_backend_pid(),backend_start::text FROM pg_stat_activity \
                  WHERE pid=pg_backend_pid()",
                &[],
            )
            .unwrap();
        (row.get(0), row.get(1))
    };
    let op_id = [0xabu8; 16];
    let artifact = format!("lfs_gc_{}", "ab".repeat(16));
    let digest = [9u8; 32];
    let mut admin = schema.admin();
    admin
        .execute(
            &format!(
                "INSERT INTO {s}.gc_op \
                 (id,generation,phase,scratch_schema,mark_serial,mark_pack_id,mark_digest,\
                  started_ms,artifact,pid,backend_start,op_id) \
                 VALUES (1,$1,'build',$2,$3,$4,$5,$6,$2,$7,$8,$9)"
            ),
            &[
                &7i64,
                &artifact,
                &11i64,
                &12i64,
                &digest.to_vec(),
                &1_700_000_000_000i64,
                &i64::from(pid),
                &start,
                &op_id.to_vec(),
            ],
        )
        .unwrap();
    admin
        .execute(
            &format!(
                "INSERT INTO {s}.gc_intent \
                 (op_id,kind,scope_kind,owner_runtime,owner_boot,state,uncertain,phase,\
                  generation,artifact,updated_ms) \
                 VALUES ($1,'compaction','store',$2,$3,'live',FALSE,'build',7,$4,1700000000000)"
            ),
            &[&op_id.to_vec(), &"runtime-t105", &"boot-t105", &artifact],
        )
        .unwrap();

    let live = postgresql_compact_status(&url, &name)
        .unwrap()
        .expect("a live op is projected");
    assert_eq!(live.op_id, op_id, "the intent carries the op identity");
    assert_eq!(live.generation, 7);
    assert_eq!(live.phase, "build");
    assert_eq!(live.started_ms, 1_700_000_000_000);
    assert_eq!(live.owner_pid, Some(i64::from(pid)));
    assert_eq!(live.owner_backend_start.as_deref(), Some(start.as_str()));
    assert_eq!(live.owner_runtime.as_deref(), Some("runtime-t105"));
    assert_eq!(live.owner_boot.as_deref(), Some("boot-t105"));
    assert_eq!(live.intent_state.as_deref(), Some("live"));
    assert_eq!(live.mark_serial, Some(11));
    assert_eq!(live.mark_pack_id, Some(12));
    assert_eq!(live.mark_digest.as_deref(), Some(&digest[..]));
    assert!(live.alive, "a non-idle backend is a live owner");
    assert!(live.uncertain, "the remaining row is uncertain");
    let busy: CompactStatus = live.status();
    assert_eq!(busy.id, format!("op-{}", "ab".repeat(16)));
    assert_eq!(busy.name, "store.compact");
    assert_eq!(busy.phase, "build");
    assert_eq!(busy.started, 1_700_000_000_000);
    assert!(busy.uncertain);
    assert_eq!(busy.state, "busy");
    assert_eq!(busy.phases, &PHASES[..]);

    // A gone owner leaves the same record as an uncertain leftover.
    owner.close().unwrap();
    wait_gone(&mut admin, pid, &start);
    let leftover = postgresql_compact_status(&url, &name)
        .unwrap()
        .expect("the row remains");
    assert!(!leftover.alive);
    assert!(leftover.uncertain);
    assert_eq!(leftover.op_id, op_id);
    let gone = leftover.status();
    assert_eq!(gone.state, "unresolved");
    assert_eq!(gone.phase, "build");
    assert_eq!(gone.id, build_op(0xab));
    assert!(gone.uncertain);

    // A settled run removes the row: the idle projection is back.
    admin
        .batch_execute(&format!(
            "DELETE FROM {s}.gc_op; DELETE FROM {s}.gc_intent WHERE kind='compaction'"
        ))
        .unwrap();
    assert!(postgresql_compact_status(&url, &name).unwrap().is_none());
}

fn build_op(byte: u8) -> String {
    format!("op-{}", format!("{byte:02x}").repeat(16))
}

/// Lowercase-hex text of `bytes` (the `gc_op` artifact and intent ids).
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(text, "{byte:02x}").expect("writing to a String cannot fail");
    }
    text
}

/// Inserts one `gc_op` row owned by `(pid, backend_start)` with the given
/// generation and phase, naming `op_id`.
fn insert_op(
    schema: &Schema,
    op_id: [u8; 16],
    generation: i64,
    phase: &str,
    pid: i32,
    start: &str,
) {
    let s = &schema.name;
    let artifact = format!("lfs_gc_{}", hex(&op_id));
    schema
        .admin()
        .execute(
            &format!(
                "INSERT INTO {s}.gc_op \
                 (id,generation,phase,scratch_schema,started_ms,artifact,pid,backend_start,op_id) \
                 VALUES (1,$1,$2,$3,1700000000000,$3,$4,$5,$6)"
            ),
            &[
                &generation,
                &phase,
                &artifact,
                &i64::from(pid),
                &start,
                &op_id.to_vec(),
            ],
        )
        .unwrap();
}

/// Inserts the live compaction intent one in-flight run registers.
fn insert_intent(schema: &Schema, op_id: [u8; 16], generation: i64, phase: &str) {
    let s = &schema.name;
    let artifact = format!("lfs_gc_{}", hex(&op_id));
    schema
        .admin()
        .execute(
            &format!(
                "INSERT INTO {s}.gc_intent \
                 (op_id,kind,scope_kind,owner_runtime,owner_boot,state,uncertain,phase,\
                  generation,artifact,updated_ms) \
                 VALUES ($1,'compaction','store',$2,$3,'live',FALSE,$4,$5,$6,1700000000000)"
            ),
            &[
                &op_id.to_vec(),
                &"runtime-f50r4-1",
                &"boot-f50r4-1",
                &phase,
                &generation,
                &artifact,
            ],
        )
        .unwrap();
}

/// The `(owner_runtime, owner_boot)` tokens a process records: its start
/// time and this boot's id, plus this PID namespace. `pid` may be a process
/// that never existed, which is how a provably dead owner is written.
fn owner_tokens(pid: u32) -> (String, String) {
    let start = std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|text| {
            let rest = text.rsplit_once(')')?.1.trim_start();
            rest.split_whitespace().nth(19)?.parse::<u64>().ok()
        })
        .unwrap_or(0);
    let namespace = std::fs::read_link("/proc/self/ns/pid")
        .ok()
        .and_then(|target| {
            let text = target.to_str()?;
            text.strip_prefix("pid:[")?
                .strip_suffix(']')?
                .parse::<u64>()
                .ok()
        })
        .map(|namespace| format!("@ns:{namespace}"))
        .unwrap_or_default();
    let boot = std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .ok()
        .and_then(|text| u128::from_str_radix(text.trim().replace('-', "").as_str(), 16).ok())
        .unwrap_or(0);
    (
        format!("pid:{pid}@{start}{namespace}"),
        format!("boot:{boot:032x}:f50r6"),
    )
}

/// Inserts only the compaction `gc_intent`: the Phase M window before
/// `persist_mark` writes `gc_op`, with the recorded state and owner tokens.
fn insert_intent_only(
    schema: &Schema,
    op_id: [u8; 16],
    phase: &str,
    state: &str,
    owner_runtime: &str,
    owner_boot: &str,
) {
    let s = &schema.name;
    let artifact = format!("lfs_gc_{}", hex(&op_id));
    let uncertain = state == "uncertain";
    schema
        .admin()
        .execute(
            &format!(
                "INSERT INTO {s}.gc_intent \
                 (op_id,kind,scope_kind,owner_runtime,owner_boot,state,uncertain,phase,\
                  generation,artifact,updated_ms) \
                 VALUES ($1,'compaction','store',$2,$3,$4,$5,$6,7,$7,1700000000000)"
            ),
            &[
                &op_id.to_vec(),
                &owner_runtime,
                &owner_boot,
                &state,
                &uncertain,
                &phase,
                &artifact,
            ],
        )
        .unwrap();
}

fn owner_backend(owner: &mut postgres::Client) -> (i32, String) {
    let row = owner
        .query_one(
            "SELECT pg_backend_pid(),backend_start::text FROM pg_stat_activity \
              WHERE pid=pg_backend_pid()",
            &[],
        )
        .unwrap();
    (row.get(0), row.get(1))
}

/// F50R4-1: a returned run released its compaction `gc_intent`, but its owner
/// backend is still alive and non-idle. The remaining `gc_op` row is a
/// leftover, so `store.status` must project `unresolved`, not `busy`.
#[test]
fn a_returned_run_with_a_live_owner_is_unresolved() {
    let Some(url) = support::pg::url() else {
        return;
    };
    let schema = Schema::new(url.clone(), "ret");
    let _handles = schema.create(&binding("ret"), [71; 32]);

    // A live, non-idle owner: the §9 liveness test the leftover rule applies.
    let mut owner = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    owner.batch_execute("BEGIN; SELECT 1").unwrap();
    let (pid, start) = owner_backend(&mut owner);

    let op_id = [0xc1u8; 16];
    insert_op(&schema, op_id, 5, "build", pid, &start);
    // No `gc_intent`: the run returned and released it.

    let recorded = postgresql_compact_status(&url, &schema.name)
        .unwrap()
        .expect("the row remains");
    assert!(recorded.alive, "the owner backend is alive");
    assert!(
        recorded.intent_state.is_none(),
        "the released intent is what marks the run returned"
    );
    assert!(recorded.uncertain);
    let projected = recorded.status();
    assert_eq!(
        projected.state, "unresolved",
        "a returned run is a leftover even while its owner lives"
    );
    assert_eq!(projected.phase, "build");
    owner.close().unwrap();
}

/// F50R4-1: an in-flight run between two of its transactions has an `idle`
/// owner backend, but its compaction `gc_intent` is still registered. The old
/// non-idle-only liveness test read that as gone; the intent keeps it `busy`.
#[test]
fn an_in_flight_run_between_transactions_is_busy() {
    let Some(url) = support::pg::url() else {
        return;
    };
    let schema = Schema::new(url.clone(), "idleop");
    let _handles = schema.create(&binding("idleop"), [71; 32]);

    let mut owner = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    let (pid, start) = owner_backend(&mut owner);
    // No transaction open: PostgreSQL reports this backend `idle`.
    let state: String = schema
        .admin()
        .query_one(
            "SELECT state::text FROM pg_stat_activity \
              WHERE pid=$1 AND backend_start::text=$2",
            &[&pid, &start],
        )
        .unwrap()
        .get(0);
    assert_eq!(state, "idle");

    let op_id = [0xc2u8; 16];
    insert_op(&schema, op_id, 6, "build", pid, &start);
    insert_intent(&schema, op_id, 6, "build");

    let recorded = postgresql_compact_status(&url, &schema.name)
        .unwrap()
        .expect("a live op is projected");
    assert!(recorded.alive, "an idle backend is still alive");
    assert_eq!(recorded.intent_state.as_deref(), Some("live"));
    assert!(recorded.uncertain);
    let projected = recorded.status();
    assert_eq!(
        projected.state, "busy",
        "the intent keeps the between-transactions run in flight"
    );
    assert_eq!(projected.phase, "build");
    assert_eq!(projected.id, build_op(0xc2));
    owner.close().unwrap();
}

/// F50R5-3 (§11): `persist_mark` writes `gc_op`, but the run registers its
/// compaction `gc_intent` before Phase M. A status read in that window must
/// project the registered intent as a busy `mark` op, never idle. The test
/// parks the run inside Phase M by holding `gc_dead`, which `persist_mark`
/// deletes before it inserts the op row.
#[test]
fn a_phase_m_run_without_a_gc_op_row_is_busy_marking() {
    let Some(url) = support::pg::url() else {
        return;
    };
    let schema = Schema::new(url.clone(), "phasem");
    let handles = schema.create(&binding("phasem"), [71; 32]);
    let _ = whole_file(&handles, b"content a rebuild would collect");

    let mut blocker = schema.admin();
    blocker
        .batch_execute(&format!(
            "BEGIN; LOCK TABLE {}.gc_dead IN ACCESS EXCLUSIVE MODE",
            schema.name
        ))
        .unwrap();

    let selection = schema.selection();
    let catalog = binding("phasem");
    let child = std::thread::spawn(move || {
        let handles =
            PostgresHandles::open_writable(&selection, &catalog, [71; 32]).expect("child open");
        handles.compact(&request(0x74, None))
    });

    // Wait for the run to reach `persist_mark` and block on `gc_dead`: it has
    // registered its intent and has not written `gc_op` yet.
    let pattern = format!("%{}.gc_dead%", schema.name);
    let mut poller = schema.admin();
    let mut reached = false;
    for _ in 0..2400 {
        let blocked: i64 = poller
            .query_one(
                "SELECT count(*) FROM pg_stat_activity \
                  WHERE datname=current_database() AND state='active' \
                    AND wait_event_type='Lock' AND query LIKE $1 \
                    AND pid<>pg_backend_pid()",
                &[&pattern],
            )
            .unwrap()
            .get(0);
        if blocked > 0 {
            reached = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert!(reached, "the run never reached Phase M's gc_dead write");
    let gc_ops: i64 = poller
        .query_one(&format!("SELECT count(*) FROM {}.gc_op", schema.name), &[])
        .unwrap()
        .get(0);
    assert_eq!(gc_ops, 0, "Phase M has not written the op row yet");

    let op = postgresql_compact_status(&url, &schema.name)
        .unwrap()
        .expect("the registered Phase M intent is not idle");
    assert!(op.intent_only, "no gc_op row names an owner yet");
    assert!(!op.alive, "an intent-only projection records no owner");
    assert_eq!(op.op_id, [0x74u8; 16]);
    assert_eq!(op.phase, "mark");
    assert!(op.started_ms > 0);
    assert_eq!(op.intent_state.as_deref(), Some("live"));
    let projected = op.status();
    assert_eq!(projected.id, build_op(0x74));
    assert_eq!(projected.name, "store.compact");
    assert_eq!(projected.phase, "mark");
    assert_eq!(projected.state, "busy", "Phase M is in flight, not idle");
    assert_eq!(projected.started, op.started_ms);
    assert_eq!(projected.phases, &PHASES[..]);
    assert!(projected.uncertain);

    blocker.batch_execute("ROLLBACK").unwrap();
    let receipt = child
        .join()
        .expect("the Phase M thread")
        .expect("the released run completes");
    assert!(
        receipt.generation_after > receipt.generation_before,
        "the released run rebuilt"
    );
    assert!(
        postgresql_compact_status(&url, &schema.name)
            .unwrap()
            .is_none(),
        "a settled run projects no op"
    );
}

/// F50R6-1 (§11): an intent-only Phase-M run whose recorded process owner is
/// provably gone cannot still be in flight. `alive` is always false without a
/// `gc_op` row, so `busy` here would have ignored the intent's owner tokens.
#[test]
fn a_phase_m_intent_only_run_with_a_dead_owner_is_unresolved() {
    let Some(url) = support::pg::url() else {
        return;
    };
    if !cfg!(target_os = "linux") {
        eprintln!("NOT_RUN: /proc owner evidence needs Linux");
        return;
    }
    let schema = Schema::new(url.clone(), "deadm");
    let _handles = schema.create(&binding("deadm"), [71; 32]);

    // A pid no process can hold, on this boot and in this PID namespace, is
    // provably gone: a shared Store must not treat it as conservatively live.
    let (runtime, boot) = owner_tokens(999_999_999);
    let op_id = [0xd1u8; 16];
    insert_intent_only(&schema, op_id, "mark", "live", &runtime, &boot);

    let recorded = postgresql_compact_status(&url, &schema.name)
        .unwrap()
        .expect("the registered Phase M intent is not idle");
    assert!(
        recorded.intent_only,
        "no gc_op row names a backend owner yet"
    );
    assert!(
        !recorded.alive,
        "an intent-only projection records no backend"
    );
    assert_eq!(recorded.intent_state.as_deref(), Some("live"));
    assert!(recorded.uncertain);
    let projected = recorded.status();
    assert_eq!(
        projected.state, "unresolved",
        "a provably dead process owner is not in flight"
    );
    assert_eq!(projected.phase, "mark");
    assert_eq!(projected.id, build_op(0xd1));
}

/// F50R6-1 (§11): an `uncertain` intent records that the outcome is unknown;
/// it is not a witness that the run continues. Even with a live owner and no
/// `gc_op` row, the projection is `unresolved`, not `busy`.
#[test]
fn a_phase_m_intent_only_run_marked_uncertain_is_unresolved() {
    let Some(url) = support::pg::url() else {
        return;
    };
    if !cfg!(target_os = "linux") {
        eprintln!("NOT_RUN: /proc owner evidence needs Linux");
        return;
    }
    let schema = Schema::new(url.clone(), "uncm");
    let _handles = schema.create(&binding("uncm"), [71; 32]);

    let (runtime, boot) = owner_tokens(std::process::id());
    let op_id = [0xd2u8; 16];
    insert_intent_only(&schema, op_id, "mark", "uncertain", &runtime, &boot);

    let recorded = postgresql_compact_status(&url, &schema.name)
        .unwrap()
        .expect("the registered Phase M intent is not idle");
    assert!(
        recorded.intent_only,
        "no gc_op row names a backend owner yet"
    );
    assert_eq!(recorded.intent_state.as_deref(), Some("uncertain"));
    assert!(recorded.uncertain);
    let projected = recorded.status();
    assert_eq!(
        projected.state, "unresolved",
        "an uncertain intent says the outcome is unknown, not in flight"
    );
    assert_eq!(projected.phase, "mark");
    assert_eq!(projected.id, build_op(0xd2));
}

#[test]
fn every_postgresql_failpoint_projects_its_phase() {
    let Some(url) = support::pg::url() else {
        return;
    };
    for (tag, point, phase, op) in [
        ("cmark", CompactFailpoint::PostgresAfterMark, "mark", 0x61u8),
        (
            "cbuild",
            CompactFailpoint::PostgresAfterBuild,
            "build",
            0x62,
        ),
        ("cswap", CompactFailpoint::PostgresAfterSwap, "swap", 0x63),
    ] {
        let schema = Schema::new(url.clone(), tag);
        let handles = schema.create(&binding(tag), [71; 32]);
        let _ = whole_file(&handles, b"t10.5 failpoint dead content");
        let error = handles.compact(&request(op, Some(point))).unwrap_err();
        assert!(
            matches!(error, layerfs_history::HistoryError::UnknownOutcome),
            "{tag}: {error:?}"
        );
        let status = postgresql_compact_status(&url, &schema.name)
            .unwrap()
            .unwrap_or_else(|| panic!("{tag}: the op row remains"));
        assert_eq!(status.phase, phase, "{tag}: the row records the phase");
        // The released `gc_intent` is gone, so the `gc_op` row's own
        // `op_id` carries the identity the projection shows.
        assert_eq!(status.op_id, [op; 16], "{tag}: the op row names the op");
        assert!(
            status.alive,
            "{tag}: the owner backend is alive while its idle session runs the test"
        );
        assert!(
            status.intent_state.is_none(),
            "{tag}: the returned run released its compaction intent"
        );
        assert!(status.uncertain, "{tag}: a failed run stays uncertain");
        let projected = status.status();
        assert_eq!(projected.name, "store.compact");
        assert_eq!(projected.id, build_op(op));
        assert_eq!(projected.state, "unresolved", "{tag}: the leftover");
        assert_eq!(projected.phase, phase);
        assert_eq!(projected.phases, &PHASES[..]);
        // The next call settles the leftover and removes the row.
        let receipt = handles.compact(&request(op.wrapping_add(1), None)).unwrap();
        assert!(
            receipt.generation_after >= receipt.generation_before,
            "{tag}: settled"
        );
        assert!(
            postgresql_compact_status(&url, &schema.name)
                .unwrap()
                .is_none(),
            "{tag}: a settled run projects no op"
        );
    }
}

#[test]
fn a_completed_run_leaves_no_projection() {
    let Some(url) = support::pg::url() else {
        return;
    };
    let schema = Schema::new(url.clone(), "done");
    let handles = schema.create(&binding("done"), [71; 32]);
    let _ = whole_file(&handles, b"t10.5 completed dead content");
    let before = handles.history.incarnation();
    let receipt = handles.compact(&request(0x71, None)).unwrap();
    assert_eq!(
        receipt.generation_after,
        before + 1,
        "the rebuild installed"
    );
    // `store.status` after a completed run: idle, with the intent released.
    assert!(postgresql_compact_status(&url, &schema.name)
        .unwrap()
        .is_none());
    let intents: i64 = schema
        .admin()
        .query_one(
            &format!(
                "SELECT count(*) FROM {}.gc_intent WHERE kind='compaction'",
                schema.name
            ),
            &[],
        )
        .unwrap()
        .get(0);
    assert_eq!(intents, 0, "the compaction intent was released");
}
