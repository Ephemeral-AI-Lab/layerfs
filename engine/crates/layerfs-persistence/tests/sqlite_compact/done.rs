//! Design gaps 7/18: the durable operator audit is a `gc_intent` row retained
//! in state `done`. The SQLite rebuild maps `gc_intent` to `Copy::Done`, so it
//! carries those rows into the installed Store after the run's own admission
//! intent; the settle then clears only the latter and the audit survives the
//! compact. The suite is separate from `sqlite_compact.rs` because the T12.3a
//! matrix report pins that suite's test count, and this copy changes no report.
#![cfg(any(target_os = "macos", target_os = "linux"))]
use crate::support;

use layerfs_content::{encode_whole_file_payload, FinalizedObject, ObjectId, ObjectRole};
use layerfs_history::{
    BranchId, GcIntent, GcIntentKind, GcScopeKind, HistoryCatalog, HistoryCatalogConfig,
    PlaceAuthority,
};
use layerfs_persistence::{
    CompactPath, CompactRequest, PersistenceConfig, SqliteHandles, SqlitePersistenceProfile,
};
use layerfs_storage::{port::PackPersistence, Storage, StoragePolicy};
use std::path::{Path, PathBuf};
use support::Temp;

const BINDING: &[u8] = b"sqlite-compact-done-test-binding";

fn create_store() -> (Temp, SqliteHandles, PathBuf) {
    let temp = Temp::new("sqlite-compact-done");
    let path = temp.join("store.sqlite");
    let handles = SqliteHandles::create(
        PersistenceConfig::sqlite(path.clone())
            .with_sqlite_profile(SqlitePersistenceProfile::Normal),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: [72; 32],
            incarnation: 7,
        },
    )
    .unwrap();
    (temp, handles, path)
}

fn reopen(path: &Path) -> SqliteHandles {
    SqliteHandles::open_writable(
        PersistenceConfig::sqlite(path.to_path_buf())
            .with_sqlite_profile(SqlitePersistenceProfile::Normal),
        BINDING,
        [72; 32],
    )
    .unwrap()
}

fn request(op: u8) -> CompactRequest {
    CompactRequest {
        op_id: [op; 16],
        ..CompactRequest::default()
    }
}

fn whole_file(handles: &SqliteHandles, bytes: &[u8]) -> ObjectId {
    let object = FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(bytes).unwrap(),
    )
    .unwrap();
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    save.accept(object.clone()).unwrap();
    save.finish().unwrap();
    object.id()
}

fn located(handles: &SqliteHandles, id: ObjectId) -> bool {
    let mut found = Vec::new();
    handles.storage.locate(&[id], &mut found).unwrap();
    !found.is_empty()
}

fn clean(handles: &SqliteHandles) {
    let mut out = Vec::new();
    handles
        .diagnose(
            layerfs_persistence::diagnostic::Options::default(),
            &mut |observation| out.push(observation),
        )
        .unwrap();
    assert!(
        !out.iter().any(|observation| matches!(
            observation.outcome,
            layerfs_persistence::diagnostic::Outcome::Fail
                | layerfs_persistence::diagnostic::Outcome::Unknown
        )),
        "{out:?}"
    );
}

/// The retained `done` audit rows a rebuild must carry: oldest first, in the
/// order the bounded production read uses.
fn done_audit(path: &Path) -> Vec<(Vec<u8>, String, i64)> {
    let connection =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let mut statement = connection
        .prepare(
            "SELECT op_id,COALESCE(evidence,''),updated_ms FROM gc_intent \
WHERE state='done' ORDER BY updated_ms, op_id",
        )
        .unwrap();
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get(1)?, row.get(2)?))
        })
        .unwrap();
    rows.map(|row| row.unwrap()).collect()
}

/// The `gc_intent` rows one operation left in the Store at `path`.
fn intent_count(path: &Path, op_id: &[u8; 16]) -> i64 {
    let hex: String = op_id.iter().fold(String::new(), |mut text, byte| {
        use std::fmt::Write as _;
        let _ = write!(text, "{byte:02x}");
        text
    });
    let connection = rusqlite::Connection::open(path).unwrap();
    connection
        .query_row(
            &format!("SELECT count(*) FROM gc_intent WHERE op_id = x'{hex}'"),
            [],
            |row| row.get(0),
        )
        .unwrap()
}

/// Inserts one retained audit row as an operator release leaves it: state
/// `done`, the owner columns cleared and the action/op in `evidence`, so the
/// unique owner key can never block a later remount. The production writer
/// that leaves such a row is a separate change; the compact copy is this one.
fn insert_done_audit(path: &Path, branch: BranchId, op: u8, evidence: &str, updated_ms: i64) {
    let connection = rusqlite::Connection::open(path).unwrap();
    connection
        .execute(
            "INSERT INTO gc_intent (
                op_id, kind, scope_kind, scope_id, workspace, sandbox, machine,
                project, branch, owner_runtime, owner_boot, state, uncertain,
                phase, evidence, updated_ms
             ) VALUES (?1, 'mount', 'branch', ?2, NULL, 'box', 'box',
                'proj', 'work', NULL, NULL, 'done', 0, 'release', ?3, ?4)",
            rusqlite::params![[op; 16].as_slice(), branch.as_slice(), evidence, updated_ms],
        )
        .unwrap();
}

/// Design gaps 7/18: the durable operator audit is a `gc_intent` row retained
/// in state `done`. A rebuild must carry those rows into the installed Store
/// after the run's own admission intent; the settle then clears only the
/// latter, so a compact never loses the audit.
#[test]
fn a_rebuild_keeps_the_retained_done_audit_rows() {
    let (_temp, handles, path) = create_store();
    let dead = whole_file(&handles, b"a pack the audit test collects");
    insert_done_audit(&path, support::branch_id(1), 1, "release one", 10);
    insert_done_audit(&path, support::branch_id(1), 2, "release two", 20);
    let receipt = handles.compact(&request(0x5d)).unwrap();
    assert_eq!(
        receipt.path,
        CompactPath::Rebuild,
        "the only pack is wholly dead, so the run rebuilds"
    );
    assert!(receipt.residue.artifacts.is_empty());
    // The run's own admission intent is settled away; the audit stays.
    assert_eq!(intent_count(&path, &[0x5d; 16]), 0);
    assert_eq!(
        done_audit(&path),
        vec![
            (vec![1; 16], "release one".to_owned(), 10),
            (vec![2; 16], "release two".to_owned(), 20),
        ],
        "the rebuild carries the retained done audit rows"
    );
    // The bounded production read still sees them through the installed Store.
    let reopened = reopen(&path);
    let page = reopened.history.gc_done_intents(0, 10).unwrap();
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].evidence.as_deref(), Some("release one"));
    assert_eq!(page.items[1].evidence.as_deref(), Some("release two"));
    assert!(
        !located(&reopened, dead),
        "the rebuild collected the dead pack"
    );
    clean(&reopened);
}

/// Design gaps 1/7/19: the operator release is the production writer of the
/// retained `done` row. The rebuild must settle the run's own admission row
/// and keep the released row, and the bounded audit must still answer from
/// the installed Store.
#[test]
fn a_release_survives_the_rebuild_that_settles_its_run() {
    let (_temp, handles, path) = create_store();
    let dead = whole_file(&handles, b"a pack the release test collects");
    let audit = GcIntent::shared(
        [0x61; 16],
        GcIntentKind::Mount,
        GcScopeKind::Store,
        [0x62; 32],
    )
    .placed(Some([0x63; 32]), Some("box"), Some("box"))
    .owned(Some("pid:1@0"), Some("boot:1"))
    .with_phase(Some("bind"))
    .authorized(PlaceAuthority::default());
    handles.history.gc_register(&audit).unwrap();
    handles.history.gc_mark_uncertain(&[0x61; 16]).unwrap();
    let evidence = "action=release;operator=alice;op=61616161616161616161616161616161";
    assert!(
        handles
            .history
            .gc_release_done(&[0x61; 16], evidence)
            .expect("the operator release retains its row"),
        "the live mount was released"
    );

    let receipt = handles.compact(&request(0x64)).unwrap();
    assert_eq!(
        receipt.path,
        CompactPath::Rebuild,
        "the only pack is wholly dead, so the run rebuilds"
    );
    assert_eq!(
        intent_count(&path, &[0x64; 16]),
        0,
        "the run's own admission row settles"
    );

    let reopened = reopen(&path);
    assert!(
        !located(&reopened, dead),
        "the rebuild collected the dead pack"
    );
    let page = reopened.history.gc_done_intents(0, 10).unwrap();
    assert_eq!(page.items.len(), 1, "{page:?}");
    assert_eq!(page.items[0].op_id, [0x61; 16]);
    assert_eq!(page.items[0].evidence.as_deref(), Some(evidence));
    assert_eq!(page.items[0].phase.as_deref(), Some("bind"));
    clean(&reopened);
}
