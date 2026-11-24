//! The 2.0 conversion of a phase-3 schema version 2 Store
//! (`tools/store_gc_convert.py`): the old machine authority and Sandbox
//! tombstones move to the home, `gc_audit` is exported to JSONL, and the
//! converted Store opens as schema version 3 only after every known machine
//! is copied or declared gone. The tool is the offline operator step the
//! version 2 refusal names; the source file stays the backup.
#![cfg(any(target_os = "macos", target_os = "linux"))]
mod support;

use layerfs_history::{
    GcIntent, GcIntentKind, GcScopeKind, HistoryCatalog, HistoryCatalogConfig, PlaceAuthority,
};
use layerfs_persistence::{PersistenceConfig, SqliteHandles, SqlitePersistenceProfile};
use layerfs_storage::StoragePolicy;
use support::{Temp, BINDING};

/// The lean `gc.sql`; the phase-3 source was this text plus [`AUTHORITY`].
const GC: &str = include_str!("../sql/sqlite/gc.sql");

/// The three tables the phase-3 branch's `gc.sql` appended.
const AUTHORITY: &str = "CREATE TABLE gc_machine_authority (\n    machine TEXT PRIMARY KEY,\n    incarnation TEXT NOT NULL,\n    updated_ms INTEGER NOT NULL CHECK (updated_ms >= 0)\n) STRICT;\n\nCREATE TABLE gc_sandbox_tombstone (\n    sandbox TEXT NOT NULL,\n    machine TEXT NOT NULL,\n    generation TEXT NOT NULL,\n    retired_ms INTEGER NOT NULL CHECK (retired_ms >= 0),\n    PRIMARY KEY (sandbox, machine)\n) STRICT, WITHOUT ROWID;\n\nCREATE TABLE gc_audit (\n    seq INTEGER PRIMARY KEY AUTOINCREMENT,\n    at_ms INTEGER NOT NULL CHECK (at_ms >= 0),\n    action TEXT NOT NULL,\n    op_id BLOB,\n    detail TEXT\n) STRICT;\n";

fn config(path: &std::path::Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Normal)
}

fn create(path: &std::path::Path) -> SqliteHandles {
    SqliteHandles::create(
        config(path),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: [71; 32],
            incarnation: 7,
        },
    )
    .expect("Store creation")
}

fn writable(
    path: &std::path::Path,
) -> Result<SqliteHandles, layerfs_storage::port::PersistenceError> {
    SqliteHandles::open_writable(config(path), BINDING, [71; 32])
}

fn definition(sql: &rusqlite::Connection) -> String {
    let mut statement = sql
        .prepare("SELECT type,name,sql FROM sqlite_schema WHERE substr(name,1,7)!='sqlite_' ORDER BY type,name")
        .unwrap();
    let rows = statement
        .query_map([], |r| Ok([r.get::<_, String>(0)?, r.get(1)?, r.get(2)?]))
        .unwrap();
    rows.map(|r| r.unwrap().map(|v| v + "\n").concat())
        .collect()
}

/// Rewrites a fresh version 3 Store into the phase-3 version 2 format: the
/// three authority tables, the version 36 checks and values, and the source
/// and definition that build recorded.
fn downgrade(path: &std::path::Path, machines: &[(&str, &str)]) {
    let sql = rusqlite::Connection::open(path).unwrap();
    let source: String = sql
        .query_row(
            "SELECT schema_source FROM history_meta WHERE id=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let old = source
        .replace("schema_version=39", "schema_version=36")
        .replace(GC, &format!("{GC}{AUTHORITY}"));
    assert_ne!(old, source, "the lean source became the phase-3 source");
    sql.execute_batch(AUTHORITY).unwrap();
    for (machine, incarnation) in machines {
        sql.execute(
            "INSERT INTO gc_machine_authority(machine,incarnation,updated_ms) VALUES(?1,?2,5)",
            rusqlite::params![machine, incarnation],
        )
        .unwrap();
    }
    sql.execute(
        "INSERT INTO gc_sandbox_tombstone(sandbox,machine,generation,retired_ms) \
         VALUES('box','native','old-generation',7)",
        rusqlite::params![],
    )
    .unwrap();
    sql.execute(
        "INSERT INTO gc_audit(at_ms,action,op_id,detail) VALUES(8,'settle',?1,'conversion')",
        rusqlite::params![vec![0u8, 1, 2, 3]],
    )
    .unwrap();
    sql.execute_batch(
        "PRAGMA writable_schema=ON; \
         UPDATE sqlite_schema SET sql=replace(sql,'schema_version=39)','schema_version=36)') \
           WHERE type='table' AND name IN ('store_policy','history_meta'); \
         PRAGMA writable_schema=OFF;",
    )
    .unwrap();
    sql.execute_batch("PRAGMA user_version=36").unwrap();
    drop(sql);
    // Reopen: SQLite reads the patched checks before the version columns change.
    let sql = rusqlite::Connection::open(path).unwrap();
    sql.execute(
        "UPDATE store_policy SET schema_version=36",
        rusqlite::params![],
    )
    .unwrap();
    sql.execute(
        "UPDATE history_meta SET schema_version=36,schema_source=?1,schema_definition=?2",
        rusqlite::params![old, definition(&sql)],
    )
    .unwrap();
    let _ = sql.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)");
    drop(sql);
    // The tool refuses a non-empty -wal: a closed Store leaves none.
    for suffix in ["-wal", "-shm"] {
        let side = std::path::PathBuf::from(format!("{}{suffix}", path.display()));
        if side.metadata().map(|meta| meta.len() == 0).unwrap_or(false) {
            let _ = std::fs::remove_file(side);
        }
    }
}

/// A layout 3 home with one machine record per name.
fn home(temp: &Temp, machines: &[&str]) -> std::path::PathBuf {
    let path = temp.join("home");
    std::fs::create_dir_all(path.join("machine")).unwrap();
    std::fs::write(
        path.join("layerfs.json"),
        "{\"layout\":3,\"id\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"}\n",
    )
    .unwrap();
    for machine in machines {
        let folder = path.join("machine").join(machine);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("record.json"),
            format!("{{\"v\":1,\"name\":\"{machine}\",\"platform\":\"native\",\"created\":1}}\n"),
        )
        .unwrap();
    }
    path
}

/// The tool's path, or None when python3 is not on this machine (NOT_RUN).
fn tool() -> Option<std::path::PathBuf> {
    let tool =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tools/store_gc_convert.py");
    if !tool.is_file()
        || std::process::Command::new("python3")
            .arg("--version")
            .output()
            .is_err()
    {
        return None;
    }
    Some(tool)
}

fn run(tool: &std::path::Path, args: &[&str]) -> std::process::Output {
    std::process::Command::new("python3")
        .arg(tool)
        .args(args)
        .output()
        .expect("the conversion tool runs")
}

#[test]
fn the_converted_store_opens_and_the_home_holds_the_old_authority() {
    let Some(tool) = tool() else {
        eprintln!("NOT_RUN: python3 or tools/store_gc_convert.py is absent");
        return;
    };
    let temp = Temp::new("gc-conversion");
    let path = temp.join("db");
    drop(create(&path));
    downgrade(&path, &[("native", "old-incarnation")]);
    let home = home(&temp, &["native"]);
    let target = temp.join("converted");
    let output = run(
        &tool,
        &[
            "convert",
            path.to_str().unwrap(),
            target.to_str().unwrap(),
            home.to_str().unwrap(),
        ],
    );
    assert!(
        output.status.success(),
        "the conversion succeeds: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let record = std::fs::read_to_string(home.join("machine/native/record.json")).unwrap();
    assert!(
        record.contains("\"incarnation\":\"old-incarnation\""),
        "the machine record took the old authority: {record}"
    );
    let tombstone =
        std::fs::read_to_string(home.join("machine/native/sandbox-retired/box.json")).unwrap();
    assert!(
        tombstone.contains("\"generation\":\"old-generation\""),
        "the home holds the old Sandbox tombstone: {tombstone}"
    );
    let audit = std::fs::read_to_string(home.join("log/gc-audit.jsonl")).unwrap();
    assert!(
        audit.contains("\"action\":\"settle\""),
        "the audit is exported: {audit}"
    );
    let handles = writable(&target).expect("the converted Store opens as schema version 3");
    drop(handles);
    let sql = rusqlite::Connection::open(&target).unwrap();
    assert_eq!(
        sql.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        39
    );
    for table in ["gc_audit", "gc_machine_authority", "gc_sandbox_tombstone"] {
        let count: i64 = sql
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name=?1",
                rusqlite::params![table],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "{table} is dropped in the target");
    }
    let sequence: i64 = sql
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name='sqlite_sequence'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(sequence, 0, "gc_audit's sequence table is gone");
}

/// The converted Store is usable by the phase-4 GC surface: a mount intent
/// registers on the lean schema, the operator release retains its `done` row,
/// and the bounded audit reads it back. No authority table is involved, so the
/// conversion's drop cannot take the audit writer with it.
#[test]
fn the_converted_store_serves_the_operator_audit() {
    let Some(tool) = tool() else {
        eprintln!("NOT_RUN: python3 or tools/store_gc_convert.py is absent");
        return;
    };
    let temp = Temp::new("gc-conversion-audit");
    let path = temp.join("db");
    drop(create(&path));
    downgrade(&path, &[("native", "old-incarnation")]);
    let home = home(&temp, &["native"]);
    let target = temp.join("converted");
    let output = run(
        &tool,
        &[
            "convert",
            path.to_str().unwrap(),
            target.to_str().unwrap(),
            home.to_str().unwrap(),
        ],
    );
    assert!(
        output.status.success(),
        "the conversion succeeds: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let handles = writable(&target).expect("the converted Store opens as schema version 3");
    let scope = [0x52u8; 32];
    let intent = GcIntent::shared([0x51; 16], GcIntentKind::Mount, GcScopeKind::Store, scope)
        .placed(Some([0x53; 32]), Some("box"), Some("native"))
        .owned(Some("pid:1@0"), Some("boot:1"))
        .with_phase(Some("bind"))
        .authorized(PlaceAuthority::default());
    handles
        .history
        .gc_register(&intent)
        .expect("the lean schema admits a mount intent");
    handles
        .history
        .gc_mark_uncertain(&[0x51; 16])
        .expect("the operator settle marks the row uncertain");
    let evidence = "action=release;operator=alice;op=51515151515151515151515151515151";
    assert!(
        handles
            .history
            .gc_release_done(&[0x51; 16], evidence)
            .expect("the converted Store retains the released row"),
        "the registered mount is released"
    );
    assert!(
        handles
            .history
            .gc_intents(GcScopeKind::Store, &scope)
            .expect("the live read runs on the converted Store")
            .is_empty(),
        "the retained done row is not a live holder"
    );
    let page = handles
        .history
        .gc_done_intents(0, 10)
        .expect("the bounded audit reads the converted Store");
    assert_eq!(page.items.len(), 1, "{page:?}");
    assert_eq!(page.items[0].op_id, [0x51; 16]);
    assert_eq!(page.items[0].evidence.as_deref(), Some(evidence));
    assert_eq!(page.items[0].phase.as_deref(), Some("bind"));
}

#[test]
fn an_unconverted_machine_refuses_until_the_operator_declares_it_gone() {
    let Some(tool) = tool() else {
        eprintln!("NOT_RUN: python3 or tools/store_gc_convert.py is absent");
        return;
    };
    let temp = Temp::new("gc-conversion-gone");
    let path = temp.join("db");
    drop(create(&path));
    downgrade(&path, &[("native", "a"), ("elsewhere", "b")]);
    let home = home(&temp, &["native"]);
    let target = temp.join("converted");
    let refused = run(
        &tool,
        &[
            "convert",
            path.to_str().unwrap(),
            target.to_str().unwrap(),
            home.to_str().unwrap(),
        ],
    );
    assert!(!refused.status.success(), "an unconverted machine refuses");
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("elsewhere"),
        "the refusal names the machine: {}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert!(!target.exists(), "a refused conversion writes no target");
    let gone = run(
        &tool,
        &[
            "convert",
            path.to_str().unwrap(),
            target.to_str().unwrap(),
            home.to_str().unwrap(),
            "--gone",
            "elsewhere",
        ],
    );
    assert!(
        gone.status.success(),
        "--gone is the missing-machine policy: {}",
        String::from_utf8_lossy(&gone.stderr)
    );
    drop(writable(&target).expect("the converted Store opens after the gone declaration"));
}
