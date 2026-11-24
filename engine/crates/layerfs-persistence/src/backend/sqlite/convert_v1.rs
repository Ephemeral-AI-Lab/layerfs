//! The offline conversion of a closed schema version 1 SQLite Store (one row
//! per inode value and binding hint, before #91) to schema version 2 (the
//! blocks), in place. It is `tools/store_blocks.py migrate` in the engine, so
//! a restore can load a backup written before #91: the restore converts its
//! own private download before it opens it, and never a live Store.
//!
//! The recorded schema must be exactly one a release before #91 wrote (less
//! the tables and columns later open-time additive migrations add); anything
//! else is refused unchanged. The first writable open adds the rest.
use super::schema::{
    ACQUISITION, APPLICATION_ID, DIGESTS, HISTORY, LINEAGE, METADATA, OBJECTS, SCRATCH, UNITS,
    UNIT_INDEX,
};
use crate::blocks::{hints, values};
use layerfs_history::records::BindingHint;
use layerfs_storage::location::InodeValueRow;
use rusqlite::{params, Connection, OpenFlags};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Schema version 1's inode value table, as release 2.0 wrote it before #91.
const V1_VALUES: &str = "CREATE TABLE inode_value (
 ordinal INTEGER PRIMARY KEY CHECK(ordinal BETWEEN 1 AND 4294967295),
 value BLOB NOT NULL CHECK(length(value)=73)
) STRICT;";
/// Schema version 1's binding hint table, as release 2.0 wrote it before #91.
const V1_HINTS: &str = "CREATE TABLE binding_hint (
    child INTEGER NOT NULL CHECK (child != 0),
    parent INTEGER NOT NULL CHECK (parent != 0),
    name BLOB NOT NULL CHECK (length(name) BETWEEN 1 AND 255),
    PRIMARY KEY (child, parent, name)
) STRICT, WITHOUT ROWID;";
/// The position columns T1.2a/T1.4 appended after the schema converted here.
const HISTORY_SERIAL: &str = "    history_serial INTEGER NOT NULL DEFAULT 0,\n";
const NEXT_SERIAL: &str = "    next_history_serial INTEGER NOT NULL DEFAULT 1,\n";
const TRACE_SALT: &str = ",\n trace_salt BLOB NOT NULL DEFAULT \
(X'0000000000000000000000000000000000000000000000000000000000000000') \
CHECK(length(trace_salt)=32)";
const SIDECARS: [&str; 2] = ["-wal", "-shm"];

/// What a conversion did.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvertedStore {
    /// `PRAGMA user_version` before and after (16-18 → 36-38, 22-24 → 42-44).
    pub user_version: (i64, i64),
    /// Inode value blocks written.
    pub value_blocks: usize,
    /// Binding hint blocks written.
    pub hint_blocks: usize,
}

/// Converts the closed SQLite Store at `path` in place when it is schema
/// version 1; `Ok(None)` (untouched) when it is not. A refusal leaves the
/// file in an unspecified state: call it only on a private copy.
pub fn convert_schema_version_1(path: &Path) -> Result<Option<ConvertedStore>, String> {
    for suffix in SIDECARS {
        let side = sidecar(path, suffix);
        if side.exists() && std::fs::metadata(&side).map_err(text)?.len() > 0 {
            return Err(format!("the Store is not closed (non-empty {suffix})"));
        }
    }
    let old = {
        let con = open(path)?;
        let id: i64 = con
            .query_row("PRAGMA application_id", [], |r| r.get(0))
            .map_err(text)?;
        let old: i64 = con
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(text)?;
        if id != APPLICATION_ID || !matches!(old, 16..=18 | 22..=24) {
            return Ok(None);
        }
        old
    };
    let converted = convert(path, old)?;
    for suffix in SIDECARS {
        let side = sidecar(path, suffix);
        if side.exists() {
            if std::fs::metadata(&side).map_err(text)?.len() > 0 {
                return Err(format!("the converted Store kept a non-empty {suffix}"));
            }
            std::fs::remove_file(&side).map_err(text)?;
        }
    }
    Ok(Some(converted))
}

fn convert(path: &Path, old: i64) -> Result<ConvertedStore, String> {
    let tables = old >= 22;
    let layout = old - if tables { 6 } else { 0 };
    let new = old + 20;
    let con = open(path)?;
    let (recorded, definition): (String, String) = con
        .query_row(
            "SELECT schema_source,schema_definition FROM history_meta",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(text)?;
    if definition != live_definition(&con)? {
        return Err("the live schema differs from the recorded definition".to_owned());
    }
    let v2_values = statement(METADATA, "CREATE TABLE inode_value_block (");
    let v2_hints = statement(HISTORY, "CREATE TABLE binding_hint_block (");
    // the open-time additive migrations add these, newest first; a Store may lack a suffix of them
    let optional = [DIGESTS, SCRATCH, LINEAGE];
    let hint_table = format!("{v2_hints}\n\n");
    let without = (0..optional.len() + 2)
        .map(|missing| {
            let mut without: Vec<&str> = optional[..missing.min(optional.len())].to_vec();
            if missing == optional.len() + 1 {
                without.push(&hint_table);
            }
            without
        })
        .find(|without| {
            source(old, layout, tables, without)
                .replace(v2_values, V1_VALUES)
                .replace(v2_hints, V1_HINTS)
                == recorded
        })
        .ok_or("the recorded schema is not one a release before #91 wrote")?;

    con.execute_batch("BEGIN IMMEDIATE").map_err(text)?;
    let rows = {
        let mut stmt = con
            .prepare("SELECT ordinal,value FROM inode_value ORDER BY ordinal")
            .map_err(text)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)))
            .map_err(text)?;
        let mut out = Vec::new();
        for row in rows {
            let (ordinal, value) = row.map_err(text)?;
            out.push(InodeValueRow {
                ordinal: u32::try_from(ordinal)
                    .map_err(|_| "an inode value ordinal is out of range")?,
                value: value
                    .try_into()
                    .map_err(|_| "an inode value is not 73 bytes")?,
            });
        }
        out
    };
    let value_blocks = values::encode(&rows).map_err(|e| format!("inode values: {e:?}"))?;
    drop(rows);
    con.execute_batch("DROP TABLE inode_value").map_err(text)?;
    con.execute_batch(v2_values).map_err(text)?;
    {
        let mut insert = con
            .prepare("INSERT INTO inode_value_block VALUES (?1,?2,?3)")
            .map_err(text)?;
        for block in &value_blocks {
            insert
                .execute(params![block.first, block.last, block.body])
                .map_err(text)?;
        }
    }
    let has_hints: bool = con
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name='binding_hint'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map_err(text)?
        > 0;
    let mut hint_blocks = 0;
    if has_hints {
        let rows = {
            let mut stmt = con
                .prepare("SELECT child,parent,name FROM binding_hint")
                .map_err(text)?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(BindingHint {
                        child: r.get::<_, i64>(0)? as u64,
                        parent: r.get::<_, i64>(1)? as u64,
                        name: r.get(2)?,
                    })
                })
                .map_err(text)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(text)?
        };
        let blocks = hints::encode(&rows).map_err(|e| format!("binding hints: {e:?}"))?;
        con.execute_batch("DROP TABLE binding_hint").map_err(text)?;
        con.execute_batch(v2_hints).map_err(text)?;
        let mut insert = con
            .prepare("INSERT INTO binding_hint_block VALUES (?1,?2,?3)")
            .map_err(text)?;
        let mut seq: BTreeMap<i64, i64> = BTreeMap::new();
        for block in &blocks {
            let n = seq.entry(block.group).or_insert(0);
            *n += 1;
            insert
                .execute(params![block.group, *n, block.body])
                .map_err(text)?;
        }
        hint_blocks = blocks.len();
    }
    con.execute_batch("PRAGMA writable_schema=ON")
        .map_err(text)?;
    for table in ["store_policy", "history_meta"] {
        con.execute(
            "UPDATE sqlite_schema SET sql=replace(sql,?1,?2) WHERE type='table' AND name=?3",
            params![
                format!("schema_version={old})"),
                format!("schema_version={new})"),
                table
            ],
        )
        .map_err(text)?;
    }
    con.execute_batch("PRAGMA writable_schema=OFF; COMMIT")
        .map_err(text)?;
    drop(con);

    // a new connection reads the rewritten schema
    let con = open(path)?;
    con.execute_batch("BEGIN IMMEDIATE").map_err(text)?;
    con.execute_batch(&format!("PRAGMA user_version={new}"))
        .map_err(text)?;
    con.execute("UPDATE store_policy SET schema_version=?1", params![new])
        .map_err(text)?;
    let definition = live_definition(&con)?;
    con.execute(
        "UPDATE history_meta SET schema_version=?1,schema_source=?2,schema_definition=?3",
        params![new, source(new, layout, tables, &without), definition],
    )
    .map_err(text)?;
    con.execute_batch("COMMIT").map_err(text)?;
    let check: String = con
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(text)?;
    if check != "ok" {
        return Err(format!(
            "the converted Store fails its integrity check: {check}"
        ));
    }
    con.execute_batch("VACUUM").map_err(text)?;
    con.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
        .map_err(text)?;
    con.close().map_err(|(_, e)| text(e))?;
    Ok(ConvertedStore {
        user_version: (old, new),
        value_blocks: value_blocks.len(),
        hint_blocks,
    })
}

/// The schema source the engine records for `version` (its scripts in their
/// order, before the GC tables and the position columns), less `without`.
fn source(version: i64, layout: i64, tables: bool, without: &[&str]) -> String {
    let objects = match layout {
        16 => OBJECTS.to_owned(),
        17 => UNITS.to_owned(),
        _ => format!("{UNITS}{UNIT_INDEX}"),
    };
    let stored = format!("schema_version={version}");
    let mut s = [
        objects,
        METADATA.replace("schema_version=1", &stored),
        HISTORY.replace("schema_version=1", &stored),
        LINEAGE.to_owned(),
        SCRATCH.to_owned(),
        DIGESTS.to_owned(),
    ]
    .concat();
    if tables {
        s.push_str(ACQUISITION);
    }
    s = s
        .replace(HISTORY_SERIAL, "")
        .replacen(TRACE_SALT, "", 1)
        .replace(NEXT_SERIAL, "");
    for text in without {
        s = s.replacen(text, "", 1);
    }
    s
}

/// The statement of `script` that starts with `head`, through its `;`.
fn statement(script: &'static str, head: &str) -> &'static str {
    let start = script.find(head).expect("the schema holds the statement");
    &script[start..start + script[start..].find(";\n").expect("the statement ends") + 1]
}

/// The schema definition as the engine records it.
fn live_definition(con: &Connection) -> Result<String, String> {
    let mut stmt = con
        .prepare("SELECT type,name,sql FROM sqlite_schema WHERE substr(name,1,7)!='sqlite_' ORDER BY type,name")
        .map_err(text)?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(text)?;
    let mut s = String::new();
    for row in rows {
        let (t, n, q) = row.map_err(text)?;
        s.push_str(&format!("{t}\n{n}\n{q}\n"));
    }
    Ok(s)
}

fn open(path: &Path) -> Result<Connection, String> {
    let con = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(text)?;
    con.set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_DEFENSIVE, false)
        .map_err(text)?;
    Ok(con)
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

fn text(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(seed: u8, inline: bool) -> [u8; 73] {
        let mut v = [seed; 73];
        if inline {
            // an inline file value: mode, mtime, nanos, then 16 zero bytes
            v[0] = 0x81;
            v[1..9].copy_from_slice(&1u64.to_be_bytes());
            v[41..45].copy_from_slice(&0o100644u32.to_be_bytes());
            v[45..53].copy_from_slice(&(1_700_000_000i64 + seed as i64).to_be_bytes());
            v[57..].fill(0);
        } else {
            v[0] = 1;
        }
        v
    }

    /// A closed schema version 1 Store in the source a release before #91
    /// recorded, as `tools/test_store_blocks.py` makes one: its schema and
    /// rows only, not a whole Store the engine would open (no history).
    fn old_store(
        path: &Path,
        version: i64,
        without: &[&str],
    ) -> (Vec<InodeValueRow>, Vec<BindingHint>) {
        let layout = version - if version >= 22 { 6 } else { 0 };
        let v2_values = statement(METADATA, "CREATE TABLE inode_value_block (");
        let v2_hints = statement(HISTORY, "CREATE TABLE binding_hint_block (");
        let src = source(version, layout, version >= 22, without)
            .replace(v2_values, V1_VALUES)
            .replace(v2_hints, V1_HINTS);
        let con = Connection::open(path).unwrap();
        con.execute_batch(&src).unwrap();
        con.execute_batch(&format!(
            "PRAGMA application_id={APPLICATION_ID}; PRAGMA user_version={version}"
        ))
        .unwrap();
        con.execute(
            "INSERT INTO store_policy(id,schema_version,format_profile,small_file_threshold_bytes,\
             whole_file_delta_max_depth,chunk_delta_max_depth,metadata_delta_max_depth) VALUES(1,?1,1,262144,3,3,3)",
            params![version],
        )
        .unwrap();
        let rows: Vec<InodeValueRow> = (1..=600u32)
            .filter(|o| o % 97 != 0)
            .map(|o| InodeValueRow {
                ordinal: o,
                value: value(o as u8, o % 3 != 0),
            })
            .collect();
        for row in &rows {
            con.execute(
                "INSERT INTO inode_value VALUES (?1,?2)",
                params![row.ordinal, row.value.to_vec()],
            )
            .unwrap();
        }
        let hints: Vec<BindingHint> = (1..200u64)
            .map(|c| BindingHint {
                child: c * 7,
                parent: c % 5 + 1,
                name: format!("file-{c}.txt").into_bytes(),
            })
            .collect();
        let has_hints = !without.iter().any(|w| w.contains("binding_hint_block"));
        if has_hints {
            for h in &hints {
                con.execute(
                    "INSERT INTO binding_hint VALUES (?1,?2,?3)",
                    params![h.child as i64, h.parent as i64, h.name],
                )
                .unwrap();
            }
        }
        con.execute(
            "INSERT INTO history_meta(id,catalog_id,catalog_incarnation,binding_key,identity_format,\
             schema_version,schema_source,schema_definition) VALUES(1,?1,1,?2,3,?3,?4,?5)",
            params![
                layerfs_history::CatalogId::derive(b"binding").unwrap().as_slice().to_vec(),
                b"binding".to_vec(),
                version,
                src,
                live_definition(&con).unwrap()
            ],
        )
        .unwrap();
        con.close().unwrap();
        (rows, if has_hints { hints } else { vec![] })
    }

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("lfs-convert-v1-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("store.sqlite")
    }

    #[test]
    fn every_schema_version_1_layout_converts_with_every_value_and_hint() {
        let hint_table = format!(
            "{}\n\n",
            statement(HISTORY, "CREATE TABLE binding_hint_block (")
        );
        for version in [16, 17, 18, 22, 23, 24] {
            for without in [
                vec![],
                vec![DIGESTS],
                vec![DIGESTS, SCRATCH, LINEAGE, hint_table.as_str()],
            ] {
                let path = temp(&format!("{version}-{}", without.len()));
                let (rows, hints) = old_store(&path, version, &without);
                let done = convert_schema_version_1(&path).unwrap().unwrap();
                assert_eq!(done.user_version, (version, version + 20));
                let con = Connection::open(&path).unwrap();
                let uv: i64 = con
                    .query_row("PRAGMA user_version", [], |r| r.get(0))
                    .unwrap();
                assert_eq!(uv, version + 20);
                let (src, def, sv): (String, String, i64) = con
                    .query_row(
                        "SELECT schema_source,schema_definition,schema_version FROM history_meta",
                        [],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                    )
                    .unwrap();
                assert_eq!(sv, version + 20);
                assert_eq!(
                    src,
                    source(
                        version + 20,
                        version - if version >= 22 { 6 } else { 0 },
                        version >= 22,
                        &without
                    )
                );
                assert_eq!(def, live_definition(&con).unwrap());
                let mut back = Vec::new();
                let mut stmt = con.prepare("SELECT first_ordinal,last_ordinal,body FROM inode_value_block ORDER BY first_ordinal").unwrap();
                for r in stmt
                    .query_map([], |r| {
                        Ok((
                            r.get::<_, u32>(0)?,
                            r.get::<_, u32>(1)?,
                            r.get::<_, Vec<u8>>(2)?,
                        ))
                    })
                    .unwrap()
                {
                    let (first, last, body) = r.unwrap();
                    for (i, v) in values::decode(first, last, &body)
                        .unwrap()
                        .into_iter()
                        .enumerate()
                    {
                        back.push(InodeValueRow {
                            ordinal: first + i as u32,
                            value: v,
                        });
                    }
                }
                drop(stmt);
                assert!(back == rows, "{version}: values differ");
                let hint_table_kept: i64 = con
                    .query_row(
                        "SELECT count(*) FROM sqlite_schema WHERE name='binding_hint_block'",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap();
                // a Store written before the hint table converts without it; its first open adds it
                assert_eq!(hint_table_kept == 1, !hints.is_empty(), "{version}");
                if !hints.is_empty() {
                    let mut got = Vec::new();
                    let mut stmt = con
                        .prepare("SELECT grp,body FROM binding_hint_block ORDER BY grp,seq")
                        .unwrap();
                    for r in stmt
                        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)))
                        .unwrap()
                    {
                        let (g, body) = r.unwrap();
                        got.extend(hints::decode(g, &body).unwrap());
                    }
                    let mut want = hints.clone();
                    want.sort_by(|a, b| {
                        (a.child, a.parent, &a.name).cmp(&(b.child, b.parent, &b.name))
                    });
                    assert!(got == want, "{version}: hints differ");
                }
                drop(con);
                // and a second conversion leaves it alone
                assert_eq!(convert_schema_version_1(&path).unwrap(), None);
            }
        }
    }

    #[test]
    fn a_schema_this_release_never_wrote_is_refused() {
        let path = temp("foreign");
        old_store(&path, 16, &[]);
        let con = Connection::open(&path).unwrap();
        con.execute_batch("CREATE TABLE extra (x INTEGER) STRICT")
            .unwrap();
        let def = live_definition(&con).unwrap();
        con.execute("UPDATE history_meta SET schema_definition=?1", params![def])
            .unwrap();
        con.execute(
            "UPDATE history_meta SET schema_source=schema_source||'-- edited'",
            [],
        )
        .unwrap();
        drop(con);
        let err = convert_schema_version_1(&path).unwrap_err();
        assert!(err.contains("not one a release before #91 wrote"), "{err}");
    }
}
