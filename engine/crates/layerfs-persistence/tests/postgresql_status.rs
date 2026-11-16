//! #41 T10.5: the PostgreSQL `store.status` compaction projection.
//!
//! `store.status` reads the Store-side `gc_op` row with its compaction
//! `gc_intent` in one status query — no Store open, schema mutation or
//! artifact read — and projects the same shape as the SQLite sidecar: the op
//! `{id, name, phase, started, uncertain}` from the row, `state` from the
//! owner's liveness, and PostgreSQL's `mark`/`build`/`swap`/`cleanup` phases.
//! An idle Store (no row, or a run that settled and removed it) projects
//! `None`. The record's own fact `uncertain` stays true for every remaining
//! row; `state` is what separates a busy run from a leftover.
//!
//! Needs a writable PostgreSQL server: set `LAYERFS_TEST_POSTGRES_URL` (for
//! example `host=127.0.0.1 port=55432 user=postgres dbname=lfs_t105
//! sslmode=disable`). Without it each test reports NOT_RUN.
#![cfg(feature = "postgresql")]
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
                  started_ms,artifact,pid,backend_start) \
                 VALUES (1,$1,'build',$2,$3,$4,$5,$6,$2,$7,$8)"
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
        // The released `gc_intent` is gone, so the scratch name carries the
        // op identity the projection shows.
        assert_eq!(status.op_id, [op; 16], "{tag}: the artifact names the op");
        assert!(
            !status.alive,
            "{tag}: the failed owner is not running the op"
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
