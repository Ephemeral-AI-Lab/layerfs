//! `backup restore` converts a closed schema version 1 SQLite Store (before
//! #91) to schema version 3 (#776) before it opens it: every inode value and binding
//! hint comes back exactly from the blocks, the recorded schema is the one the
//! engine recognizes, and a schema no release wrote is refused unchanged.
//!
//! The fixture is a v1 Store's schema and rows, made the way
//! `tools/test_store_blocks.py` makes one. It has no history and is not in
//! WAL mode; the open test switches it to WAL, as the engine wrote every
//! Store, before converting and opening it.
use layerfs_history::records::BindingHint;
use layerfs_persistence::convert_schema_version_1;
use layerfs_persistence::internals::{
    converted_source, decode_hint_block, decode_value_block, schema_version_1_source,
    sqlite_definition, SCHEMA_VERSION_1_SOURCES,
};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

const APPLICATION_ID: i64 = 1279677264;
const BINDING: &[u8] = b"binding";

fn value(ordinal: u32) -> [u8; 73] {
    let mut v = [ordinal as u8; 73];
    if ordinal % 3 != 0 {
        // an inline file value: mode, mtime and nanos, then 16 zero bytes
        v[0] = 0x81;
        v[1..9].copy_from_slice(&1u64.to_be_bytes());
        v[41..45].copy_from_slice(&0o100644u32.to_be_bytes());
        v[45..53].copy_from_slice(&(1_700_000_000i64 + ordinal as i64).to_be_bytes());
        v[57..].fill(0);
    } else {
        v[0] = 1;
    }
    v
}

/// A closed v1 Store of `version` whose recorded source is the `missing`-th
/// candidate; its values and (when its schema has the table) hints.
fn old_store(
    path: &Path,
    version: i64,
    missing: usize,
) -> (Vec<(u32, [u8; 73])>, Vec<BindingHint>) {
    let source = schema_version_1_source(version, missing);
    let con = Connection::open(path).unwrap();
    con.execute_batch(&source).unwrap();
    con.execute_batch(&format!(
        "PRAGMA application_id={APPLICATION_ID}; PRAGMA user_version={version}"
    ))
    .unwrap();
    con.execute(
        "INSERT INTO store_policy(id,schema_version,format_profile,small_file_threshold_bytes,\
         whole_file_delta_max_depth,chunk_delta_max_depth,metadata_delta_max_depth) \
         VALUES(1,?1,1,262144,3,3,3)",
        params![version],
    )
    .unwrap();
    // gaps every 97 ordinals start new blocks
    let values: Vec<(u32, [u8; 73])> = (1..=600u32)
        .filter(|o| o % 97 != 0)
        .map(|o| (o, value(o)))
        .collect();
    for (ordinal, v) in &values {
        con.execute(
            "INSERT INTO inode_value VALUES (?1,?2)",
            params![ordinal, v.to_vec()],
        )
        .unwrap();
    }
    let has_hints: bool = con
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name='binding_hint'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
        > 0;
    let hints: Vec<BindingHint> = if has_hints {
        (1..200u64)
            .map(|c| BindingHint {
                child: c * 7,
                parent: c % 5 + 1,
                name: format!("file-{c}.txt").into_bytes(),
            })
            .collect()
    } else {
        vec![]
    };
    for h in &hints {
        con.execute(
            "INSERT INTO binding_hint VALUES (?1,?2,?3)",
            params![h.child as i64, h.parent as i64, h.name],
        )
        .unwrap();
    }
    con.execute(
        "INSERT INTO history_meta(id,catalog_id,catalog_incarnation,binding_key,identity_format,\
         schema_version,schema_source,schema_definition) VALUES(1,?1,1,?2,3,?3,?4,?5)",
        params![
            layerfs_history::CatalogId::derive(BINDING)
                .unwrap()
                .as_slice()
                .to_vec(),
            BINDING.to_vec(),
            version,
            source,
            sqlite_definition(&con).unwrap()
        ],
    )
    .unwrap();
    con.close().unwrap();
    (values, hints)
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-convert-v1-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("store.sqlite")
}

#[test]
fn every_schema_version_1_layout_converts_with_every_value_and_hint() {
    for version in [16, 17, 18, 22, 23, 24] {
        for missing in 0..SCHEMA_VERSION_1_SOURCES {
            let at = format!("{version}/{missing}");
            let path = temp(&format!("{version}-{missing}"));
            let (values, hints) = old_store(&path, version, missing);
            let done = convert_schema_version_1(&path).unwrap().unwrap();
            assert_eq!(done.user_version, (version, version + 40), "{at}");
            let con = Connection::open(&path).unwrap();
            let user_version: i64 = con
                .query_row("PRAGMA user_version", [], |r| r.get(0))
                .unwrap();
            assert_eq!(user_version, version + 40, "{at}");
            let (source, definition, recorded): (String, String, i64) = con
                .query_row(
                    "SELECT schema_source,schema_definition,schema_version FROM history_meta",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .unwrap();
            assert_eq!(recorded, version + 40, "{at}");
            assert_eq!(source, converted_source(version, missing), "{at}");
            assert_eq!(definition, sqlite_definition(&con).unwrap(), "{at}");
            let tables: Vec<String> = con
                .prepare("SELECT name FROM sqlite_schema WHERE type='table'")
                .unwrap()
                .query_map([], |r| r.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            assert!(
                !tables
                    .iter()
                    .any(|t| t == "inode_value" || t == "binding_hint"),
                "{at}"
            );

            let mut back = Vec::new();
            let mut blocks = con
                .prepare("SELECT first_ordinal,last_ordinal,body FROM inode_value_block ORDER BY first_ordinal")
                .unwrap();
            for row in blocks
                .query_map([], |r| {
                    Ok((
                        r.get::<_, u32>(0)?,
                        r.get::<_, u32>(1)?,
                        r.get::<_, Vec<u8>>(2)?,
                    ))
                })
                .unwrap()
            {
                let (first, last, body) = row.unwrap();
                for (i, v) in decode_value_block(first, last, &body)
                    .unwrap()
                    .into_iter()
                    .enumerate()
                {
                    back.push((first + i as u32, v));
                }
            }
            drop(blocks);
            assert!(back == values, "{at}: values differ");

            // a Store written before the hint table converts without it; its first open adds it
            assert_eq!(
                tables.iter().any(|t| t == "binding_hint_block"),
                !hints.is_empty(),
                "{at}"
            );
            if !hints.is_empty() {
                let mut got = Vec::new();
                let mut blocks = con
                    .prepare("SELECT grp,body FROM binding_hint_block ORDER BY grp,seq")
                    .unwrap();
                for row in blocks
                    .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)))
                    .unwrap()
                {
                    let (group, body) = row.unwrap();
                    got.extend(decode_hint_block(group, &body).unwrap());
                }
                let mut want = hints.clone();
                want.sort_by(|a, b| {
                    (a.child, a.parent, &a.name).cmp(&(b.child, b.parent, &b.name))
                });
                assert!(got == want, "{at}: hints differ");
            }
            drop(con);
            for suffix in ["-wal", "-shm"] {
                let mut side = path.as_os_str().to_owned();
                side.push(suffix);
                assert!(!PathBuf::from(side).exists(), "{at}: {suffix} left");
            }
            // a converted Store is not schema version 1: a second call leaves it alone
            assert_eq!(convert_schema_version_1(&path).unwrap(), None, "{at}");
        }
    }
}

#[test]
fn a_schema_no_release_wrote_is_refused_unchanged() {
    let path = temp("foreign");
    old_store(&path, 16, 0);
    let con = Connection::open(&path).unwrap();
    con.execute(
        "UPDATE history_meta SET schema_source=schema_source||'-- edited'",
        [],
    )
    .unwrap();
    drop(con);
    let before = std::fs::read(&path).unwrap();
    let err = convert_schema_version_1(&path).unwrap_err();
    assert!(err.contains("not one a release before #91 wrote"), "{err}");
    assert!(
        std::fs::read(&path).unwrap() == before,
        "a refused Store is changed"
    );
}

#[test]
fn a_store_that_is_not_closed_is_refused() {
    let path = temp("open");
    old_store(&path, 16, 0);
    let mut wal = path.as_os_str().to_owned();
    wal.push("-wal");
    std::fs::write(PathBuf::from(wal), b"not empty").unwrap();
    let err = convert_schema_version_1(&path).unwrap_err();
    assert!(err.contains("not closed"), "{err}");
}

/// What `backup restore` does next: the first writable open of a converted
/// Store runs the additive migrations its older recorded source still needs
/// (#776 gates them on that source), and a later open runs none of them.
#[test]
fn a_converted_store_completes_on_its_first_writable_open() {
    use layerfs_persistence::{
        thread_work, PersistenceConfig, SqliteHandles, SqlitePersistenceProfile,
    };
    let open = |path: &Path| {
        let before = thread_work();
        let config =
            PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Normal);
        let handles = SqliteHandles::open_writable(config, BINDING, [9; 32]);
        (
            handles.map(drop),
            thread_work().since(&before).write_transactions,
        )
    };
    for version in [16, 17, 18, 22, 23, 24] {
        for missing in 0..SCHEMA_VERSION_1_SOURCES {
            let at = format!("{version}/{missing}");
            let path = temp(&format!("open-{version}-{missing}"));
            old_store(&path, version, missing);
            // the engine wrote every Store in WAL mode; the fixture does not
            let con = Connection::open(&path).unwrap();
            let mode: String = con
                .query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))
                .unwrap();
            assert_eq!(mode, "wal", "{at}");
            con.close().unwrap();
            convert_schema_version_1(&path).unwrap().unwrap();
            let (first, writes) = open(&path);
            first.unwrap_or_else(|e| panic!("{at}: first open: {e:?}"));
            assert!(writes > 1, "{at}: the first open migrated nothing");
            let (second, writes) = open(&path);
            second.unwrap_or_else(|e| panic!("{at}: second open: {e:?}"));
            // the merge record sweep alone
            assert_eq!(writes, 1, "{at}");
        }
    }
}
