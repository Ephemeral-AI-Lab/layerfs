use base64::Engine;
use blake3::Hasher;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::time::Instant;

type AppResult<T> = Result<T, Box<dyn Error>>;
const DATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../data");
const MIN: usize = 8192;
const TARGET: usize = 16384;
const MAX: usize = 32768;
const SMALL_MASK: u64 = 0x0000_d903_0353_7000;
const LARGE_MASK: u64 = 0x0000_d901_0353_0000;
const SHIFTED_SMALL_MASK: u64 = 0x0001_b206_06a6_e000;
const SHIFTED_LARGE_MASK: u64 = 0x0001_b202_06a6_0000;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS objects(
  object_kind INTEGER NOT NULL,
  object_digest BLOB NOT NULL,
  object_length INTEGER NOT NULL,
  object_bytes BLOB NOT NULL,
  PRIMARY KEY(object_kind, object_digest),
  CHECK(length(object_bytes) = object_length)
) STRICT;
CREATE TABLE IF NOT EXISTS roots(
  root_id BLOB PRIMARY KEY,
  manifest_digest BLOB NOT NULL UNIQUE,
  logical_digest BLOB NOT NULL,
  logical_bytes INTEGER NOT NULL,
  manifest_bytes BLOB NOT NULL,
  object_count INTEGER NOT NULL
) STRICT;
CREATE TABLE IF NOT EXISTS root_members(
  root_id BLOB NOT NULL REFERENCES roots(root_id),
  ordinal INTEGER NOT NULL,
  object_kind INTEGER NOT NULL,
  object_digest BLOB NOT NULL,
  object_length INTEGER NOT NULL,
  PRIMARY KEY(root_id, ordinal),
  FOREIGN KEY(object_kind, object_digest) REFERENCES objects(object_kind, object_digest)
) STRICT;
CREATE TABLE IF NOT EXISTS current_head(
  slot INTEGER PRIMARY KEY CHECK(slot = 1),
  root_id BLOB NOT NULL REFERENCES roots(root_id),
  generation INTEGER NOT NULL
) STRICT;
"#;

const INSERT_OBJECT: &str =
    "INSERT INTO objects(object_kind,object_digest,object_length,object_bytes) VALUES(?1,?2,?3,?4)";
const SELECT_OBJECT: &str =
    "SELECT object_length,object_bytes FROM objects WHERE object_kind=?1 AND object_digest=?2";
const INSERT_ROOT: &str = "INSERT INTO roots(root_id,manifest_digest,logical_digest,logical_bytes,manifest_bytes,object_count) VALUES(?1,?2,?3,?4,?5,?6)";
const INSERT_MEMBER: &str = "INSERT INTO root_members(root_id,ordinal,object_kind,object_digest,object_length) VALUES(?1,?2,?3,?4,?5)";
const SELECT_HEAD: &str = "SELECT root_id,generation FROM current_head WHERE slot=1";
const UPSERT_HEAD: &str = "INSERT INTO current_head(slot,root_id,generation) VALUES(1,?1,?2) ON CONFLICT(slot) DO UPDATE SET root_id=excluded.root_id,generation=excluded.generation";
const SELECT_ROOT: &str = "SELECT manifest_digest,logical_digest,logical_bytes,manifest_bytes,object_count FROM roots WHERE root_id=?1";
const SELECT_MEMBERS: &str = "SELECT ordinal,object_kind,object_digest,object_length FROM root_members WHERE root_id=?1 ORDER BY ordinal";

#[derive(Debug, Deserialize, Clone)]
struct ObjectSpec {
    kind: i64,
    digest: String,
    length: usize,
}

#[derive(Debug, Deserialize, Clone)]
struct Fixture {
    fixture_id: String,
    bytes: usize,
    logical_digest: String,
    manifest_digest: String,
    root_id: String,
    object_count: usize,
    objects: Vec<ObjectSpec>,
    manifest_b64: String,
    source_path: String,
}

#[derive(Default)]
struct Stats {
    statements: u64,
    rows_read: u64,
    rows_inserted: u64,
    new_objects: u64,
    reused_objects: u64,
    source_cdc_ns: Option<u128>,
    root_ns: Option<u128>,
    materialize_ns: Option<u128>,
    materialized_bytes: Option<u64>,
    materialized_digest: Option<String>,
}

#[derive(Serialize)]
struct ResultRecord {
    schema: &'static str,
    candidate: &'static str,
    lane: String,
    operation: String,
    fixture_id: String,
    sqlite_version: String,
    pragma: serde_json::Value,
    duration_ns: u128,
    phase_source_cdc_ns: serde_json::Value,
    phase_root_ns: serde_json::Value,
    phase_materialize_ns: serde_json::Value,
    materialized_bytes: serde_json::Value,
    materialization_mib_s: serde_json::Value,
    materialized_digest: serde_json::Value,
    destination_sync: bool,
    phase_sql_commit_ns: u128,
    phase_close_reopen_verify_ns: u128,
    checkpoint_ns: u128,
    setup_ns: u128,
    sqlite_prepare_calls: &'static str,
    sqlite_statement_calls: u64,
    sqlite_rows_read: u64,
    sqlite_rows_inserted: u64,
    new_objects: u64,
    reused_objects: u64,
    db_bytes_before: u64,
    db_bytes_after: u64,
    wal_bytes_before: u64,
    wal_bytes_after: u64,
    user_cpu_ns: &'static str,
    system_cpu_ns: &'static str,
    peak_rss_bytes: &'static str,
    retained: bool,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn bytes(hex_value: &str) -> Vec<u8> {
    (0..hex_value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex_value[i..i + 2], 16).unwrap())
        .collect()
}
fn domain_digest(domain: &str, data: &[u8]) -> Vec<u8> {
    let mut hasher = Hasher::new();
    hasher.update(domain.as_bytes());
    hasher.update(&[0]);
    hasher.update(data);
    hasher.finalize().as_bytes().to_vec()
}
fn load_fixture(id: &str) -> AppResult<Fixture> {
    Ok(serde_json::from_slice(&fs::read(
        Path::new(DATA).join("fixtures").join(format!("{id}.json")),
    )?)?)
}
fn edit_base_fixture_id(id: &str) -> String {
    id.strip_prefix("edit-small-")
        .map(|suffix| format!("edit-base-{suffix}"))
        .unwrap_or_else(|| "edit-base-10m".to_string())
}
fn gear() -> AppResult<Vec<u64>> {
    Ok(fs::read_to_string(Path::new(DATA).join("gear.txt"))?
        .lines()
        .map(|line| u64::from_str_radix(line.trim_start_matches("0x"), 16))
        .collect::<Result<_, _>>()?)
}
fn fastcdc_cut(bytes: &[u8], gear: &[u64]) -> usize {
    if bytes.len() <= MIN {
        return bytes.len();
    }
    let retained = bytes.len().min(MAX);
    let mut index = MIN;
    let mut hash = 0u64;
    while index < MAX && index + 1 < bytes.len() {
        hash = hash
            .wrapping_shl(2)
            .wrapping_add(gear[bytes[index] as usize].wrapping_shl(1));
        let shifted_mask = if index < TARGET {
            SHIFTED_SMALL_MASK
        } else {
            SHIFTED_LARGE_MASK
        };
        if hash & shifted_mask == 0 {
            return index;
        }
        hash = hash.wrapping_add(gear[bytes[index + 1] as usize]);
        let mask = if index < TARGET {
            SMALL_MASK
        } else {
            LARGE_MASK
        };
        if hash & mask == 0 {
            return index + 1;
        }
        index += 2;
    }
    retained
}
fn stream_chunks(
    path: &str,
    gear: &[u64],
    mut f: impl FnMut(Vec<u8>) -> AppResult<()>,
) -> AppResult<()> {
    let mut file = File::open(path)?;
    let mut buffer = vec![0u8; MAX];
    let mut filled = 0usize;
    let mut eof = false;
    while !eof || filled > 0 {
        if !eof && filled < MAX {
            let count = file.read(&mut buffer[filled..])?;
            if count == 0 {
                eof = true;
            } else {
                filled += count;
            }
        }
        if filled == 0 && eof {
            break;
        }
        let cut = fastcdc_cut(&buffer[..filled], gear);
        let chunk = buffer[..cut].to_vec();
        buffer.copy_within(cut..filled, 0);
        filled -= cut;
        f(chunk)?;
    }
    Ok(())
}

fn configure(conn: &Connection, schema: bool) -> AppResult<()> {
    conn.execute_batch("PRAGMA page_size=4096; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA mmap_size=0; PRAGMA cache_size=-65536; PRAGMA wal_autocheckpoint=0; PRAGMA foreign_keys=ON;")?;
    if schema {
        conn.execute_batch(SCHEMA)?;
    }
    Ok(())
}
fn open_db(path: &Path, schema: bool) -> AppResult<Connection> {
    let conn = Connection::open(path)?;
    configure(&conn, schema)?;
    Ok(conn)
}
fn seed(path: &Path, fixture: &Fixture, lane: &str, gear: &[u64]) -> AppResult<()> {
    let conn = open_db(path, true)?;
    conn.execute_batch("BEGIN IMMEDIATE")?;
    create(&conn, fixture, lane, gear, &mut Stats::default())?;
    conn.execute_batch("COMMIT")?;
    drop(conn);
    checkpoint(path)?;
    Ok(())
}
fn db_size(path: &Path, suffix: &str) -> u64 {
    fs::metadata(format!("{}{}", path.display(), suffix))
        .map(|m| m.len())
        .unwrap_or(0)
}

fn remove_db_files(path: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let _ = fs::remove_file(format!("{}{}", path.display(), suffix));
    }
}

fn effective_pragmas(conn: &Connection) -> AppResult<serde_json::Value> {
    let journal_mode: String = conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
    let synchronous: i64 = conn.query_row("PRAGMA synchronous", [], |row| row.get(0))?;
    let page_size: i64 = conn.query_row("PRAGMA page_size", [], |row| row.get(0))?;
    let mmap_size: i64 = conn.query_row("PRAGMA mmap_size", [], |row| row.get(0))?;
    let cache_size: i64 = conn.query_row("PRAGMA cache_size", [], |row| row.get(0))?;
    let wal_autocheckpoint: i64 =
        conn.query_row("PRAGMA wal_autocheckpoint", [], |row| row.get(0))?;
    let foreign_keys: i64 = conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
    Ok(serde_json::json!({
        "journal_mode": journal_mode.to_lowercase(),
        "synchronous": synchronous,
        "page_size": page_size,
        "mmap_size": mmap_size,
        "cache_size": cache_size,
        "wal_autocheckpoint": wal_autocheckpoint,
        "foreign_keys": foreign_keys,
    }))
}

fn table_counts(conn: &Connection) -> AppResult<(u64, u64, u64, u64)> {
    conn.query_row(
        "SELECT (SELECT COUNT(*) FROM objects), (SELECT COUNT(*) FROM roots), (SELECT COUNT(*) FROM root_members), (SELECT COUNT(*) FROM current_head)",
        [],
        |row| Ok((row.get::<_, i64>(0)? as u64, row.get::<_, i64>(1)? as u64, row.get::<_, i64>(2)? as u64, row.get::<_, i64>(3)? as u64)),
    ).map_err(Into::into)
}

fn assert_counts(
    actual: (u64, u64, u64, u64),
    expected: (u64, u64, u64, u64),
    label: &str,
) -> AppResult<()> {
    if actual != expected {
        return Err(format!("{label} count mismatch: {actual:?} != {expected:?}").into());
    }
    Ok(())
}

fn admit(conn: &Connection, object: &ObjectSpec, data: &[u8], stats: &mut Stats) -> AppResult<()> {
    let found: Option<(i64, Vec<u8>)> = conn
        .query_row(
            SELECT_OBJECT,
            params![object.kind, bytes(&object.digest)],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    stats.statements += 1;
    stats.rows_read += 1;
    if let Some((length, incumbent)) = found {
        if length as usize != data.len() || incumbent != data {
            return Err(format!("same typed ID has unequal bytes: {}", object.digest).into());
        }
        stats.reused_objects += 1;
        return Ok(());
    }
    conn.execute(
        INSERT_OBJECT,
        params![
            object.kind,
            bytes(&object.digest),
            object.length as i64,
            data
        ],
    )?;
    stats.statements += 1;
    stats.rows_inserted += 1;
    stats.new_objects += 1;
    Ok(())
}
fn write_root(conn: &Connection, fixture: &Fixture, stats: &mut Stats) -> AppResult<()> {
    let root = bytes(&fixture.root_id);
    let manifest = base64::engine::general_purpose::STANDARD.decode(&fixture.manifest_b64)?;
    conn.execute(
        INSERT_ROOT,
        params![
            &root,
            bytes(&fixture.manifest_digest),
            bytes(&fixture.logical_digest),
            fixture.bytes as i64,
            manifest,
            fixture.object_count as i64
        ],
    )?;
    stats.statements += 1;
    stats.rows_inserted += 1;
    for (ordinal, object) in fixture.objects.iter().enumerate() {
        conn.execute(
            INSERT_MEMBER,
            params![
                &root,
                ordinal as i64,
                object.kind,
                bytes(&object.digest),
                object.length as i64
            ],
        )?;
        stats.statements += 1;
        stats.rows_inserted += 1;
    }
    let generation: i64 = conn
        .query_row(SELECT_HEAD, [], |row| row.get::<_, i64>(1))
        .optional()?
        .map_or(1, |value| value + 1);
    stats.statements += 1;
    stats.rows_read += 1;
    conn.execute(UPSERT_HEAD, params![root, generation])?;
    stats.statements += 1;
    stats.rows_inserted += 1;
    Ok(())
}
fn verify_root(conn: &Connection, fixture: &Fixture, stats: &mut Stats) -> AppResult<()> {
    let head: Option<Vec<u8>> = conn
        .query_row(SELECT_HEAD, [], |row| row.get(0))
        .optional()?;
    stats.statements += 1;
    stats.rows_read += 1;
    if head.as_deref().map(hex).as_deref() != Some(fixture.root_id.as_str()) {
        return Err("current head mismatch".into());
    }
    let root: (Vec<u8>, Vec<u8>, i64, Vec<u8>, i64) =
        conn.query_row(SELECT_ROOT, params![bytes(&fixture.root_id)], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })?;
    stats.statements += 1;
    stats.rows_read += 1;
    if hex(&root.1) != fixture.logical_digest || root.2 as usize != fixture.bytes {
        return Err("root metadata mismatch".into());
    }
    let mut members = conn.prepare(SELECT_MEMBERS)?;
    let rows: Vec<(i64, i64, Vec<u8>, i64)> = members
        .query_map(params![bytes(&fixture.root_id)], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<Result<_, _>>()?;
    stats.statements += 1;
    stats.rows_read += rows.len() as u64;
    if rows.len() != fixture.objects.len() {
        return Err("member count mismatch".into());
    }
    let mut logical = Hasher::new();
    logical.update(b"logical-v1\0");
    for (_, kind, digest, _) in rows {
        let object: Vec<u8> =
            conn.query_row(SELECT_OBJECT, params![kind, digest], |row| row.get(1))?;
        stats.statements += 1;
        stats.rows_read += 1;
        logical.update(&object);
    }
    if hex(logical.finalize().as_bytes()) != fixture.logical_digest {
        return Err("reconstructed digest mismatch".into());
    }
    Ok(())
}

fn materialize(
    conn: &Connection,
    fixture: &Fixture,
    output_path: &Path,
    stats: &mut Stats,
) -> AppResult<()> {
    let start = Instant::now();
    let head: Option<Vec<u8>> = conn
        .query_row(SELECT_HEAD, [], |row| row.get(0))
        .optional()?;
    stats.statements += 1;
    stats.rows_read += 1;
    if head.as_deref().map(hex).as_deref() != Some(fixture.root_id.as_str()) {
        return Err("current head mismatch".into());
    }
    let root: (Vec<u8>, Vec<u8>, i64, Vec<u8>, i64) =
        conn.query_row(SELECT_ROOT, params![bytes(&fixture.root_id)], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })?;
    stats.statements += 1;
    stats.rows_read += 1;
    if hex(&root.1) != fixture.logical_digest || root.2 as usize != fixture.bytes {
        return Err("root metadata mismatch".into());
    }
    let mut members_statement = conn.prepare(SELECT_MEMBERS)?;
    let members: Vec<(i64, i64, Vec<u8>, i64)> = members_statement
        .query_map(params![bytes(&fixture.root_id)], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<Result<_, _>>()?;
    stats.statements += 1;
    stats.rows_read += members.len() as u64;
    if members.len() != fixture.objects.len() {
        return Err("member count mismatch".into());
    }

    let mut logical = Hasher::new();
    logical.update(b"logical-v1\0");
    let mut output = File::create(output_path)?;
    let mut written = 0u64;
    for (_, kind, digest, length) in members {
        let object: (i64, Vec<u8>) =
            conn.query_row(SELECT_OBJECT, params![kind, digest], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?;
        stats.statements += 1;
        stats.rows_read += 1;
        if object.0 != length {
            return Err("member object length mismatch".into());
        }
        logical.update(&object.1);
        output.write_all(&object.1)?;
        written += object.1.len() as u64;
    }
    output.sync_all()?;
    drop(output);
    let digest = hex(logical.finalize().as_bytes());
    if digest != fixture.logical_digest {
        return Err("materialized digest mismatch".into());
    }
    if written != fixture.bytes as u64 || fs::metadata(output_path)?.len() != fixture.bytes as u64 {
        return Err("materialized byte count mismatch".into());
    }
    stats.materialize_ns = Some(start.elapsed().as_nanos());
    stats.materialized_bytes = Some(written);
    stats.materialized_digest = Some(digest);
    Ok(())
}

fn create(
    conn: &Connection,
    fixture: &Fixture,
    lane: &str,
    gear: &[u64],
    stats: &mut Stats,
) -> AppResult<()> {
    let source_start = Instant::now();
    let mut logical = Hasher::new();
    logical.update(b"logical-v1\0");
    if lane == "end_to_end" {
        stream_chunks(&fixture.source_path, gear, |chunk| {
            logical.update(&chunk);
            let object = ObjectSpec {
                kind: 1,
                digest: hex(&domain_digest("object-v1", &chunk)),
                length: chunk.len(),
            };
            admit(conn, &object, &chunk, stats)
        })?;
        if hex(logical.finalize().as_bytes()) != fixture.logical_digest {
            return Err("fixture logical digest mismatch".into());
        }
        stats.source_cdc_ns = Some(source_start.elapsed().as_nanos());
    } else {
        let mut unique = HashMap::new();
        for object in &fixture.objects {
            unique
                .entry(object.digest.clone())
                .or_insert(object.clone());
        }
        for object in unique.values() {
            let path = Path::new(DATA)
                .join("objects")
                .join(format!("{}-{}.bin", object.kind, object.digest));
            admit(conn, object, &fs::read(path)?, stats)?;
        }
    }
    let root_start = Instant::now();
    write_root(conn, fixture, stats)?;
    stats.root_ns = Some(root_start.elapsed().as_nanos());
    Ok(())
}
fn checkpoint(path: &Path) -> AppResult<u128> {
    let conn = open_db(path, false)?;
    let start = Instant::now();
    let mut statement = conn.prepare("PRAGMA wal_checkpoint(TRUNCATE)")?;
    let _: Vec<(i64, i64, i64)> = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .collect::<Result<_, _>>()?;
    Ok(start.elapsed().as_nanos())
}

fn one(
    path: &Path,
    fixture: &Fixture,
    operation: &str,
    lane: &str,
    gear: &[u64],
    retain: bool,
) -> AppResult<ResultRecord> {
    let before_db = db_size(path, "");
    let before_wal = db_size(path, "-wal");
    let setup_start = Instant::now();
    let conn = open_db(path, true)?;
    let pragma = effective_pragmas(&conn)?;
    let version: String = conn.query_row("SELECT sqlite_version()", [], |row| row.get(0))?;
    let setup_ns = setup_start.elapsed().as_nanos();
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let start = Instant::now();
    let mut stats = Stats::default();
    let output_path = PathBuf::from(format!("{}.materialized", path.display()));
    if operation == "materialize" {
        let _ = fs::remove_file(&output_path);
        materialize(&conn, fixture, &output_path, &mut stats)?;
    } else if operation == "create" || operation == "edit" || operation == "storage_only" {
        create(&conn, fixture, lane, gear, &mut stats)?;
    } else {
        verify_root(&conn, fixture, &mut stats)?;
    }
    conn.execute_batch("COMMIT")?;
    let commit_ns = start.elapsed().as_nanos();
    drop(conn);
    let reopen_start = Instant::now();
    let reopened = open_db(path, false)?;
    verify_root(&reopened, fixture, &mut stats)?;
    drop(reopened);
    let reopen_ns = reopen_start.elapsed().as_nanos();
    let checkpoint_ns = checkpoint(path)?;
    let after_db = db_size(path, "");
    let after_wal = db_size(path, "-wal");
    if operation == "materialize" {
        let _ = fs::remove_file(&output_path);
    }
    Ok(ResultRecord {
        schema: "sqlite-techstack-result-v1",
        candidate: "R-SQL",
        lane: lane.into(),
        operation: operation.into(),
        fixture_id: fixture.fixture_id.clone(),
        sqlite_version: version,
        pragma,
        duration_ns: commit_ns + reopen_ns,
        phase_source_cdc_ns: stats.source_cdc_ns.map_or_else(
            || serde_json::json!("not_applicable"),
            |v| serde_json::json!(v),
        ),
        phase_root_ns: stats.root_ns.map_or_else(
            || serde_json::json!("not_observed"),
            |v| serde_json::json!(v),
        ),
        phase_materialize_ns: stats.materialize_ns.map_or_else(
            || serde_json::json!("not_observed"),
            |v| serde_json::json!(v),
        ),
        materialized_bytes: stats.materialized_bytes.map_or_else(
            || serde_json::json!("not_observed"),
            |v| serde_json::json!(v),
        ),
        materialization_mib_s: stats
            .materialize_ns
            .zip(stats.materialized_bytes)
            .map_or_else(
                || serde_json::json!("not_observed"),
                |(duration, bytes)| {
                    serde_json::json!((bytes as f64 / 1_048_576.0) / (duration as f64 / 1e9))
                },
            ),
        materialized_digest: stats.materialized_digest.map_or_else(
            || serde_json::json!("not_observed"),
            |v| serde_json::json!(v),
        ),
        destination_sync: operation == "materialize",
        phase_sql_commit_ns: commit_ns,
        phase_close_reopen_verify_ns: reopen_ns,
        checkpoint_ns,
        setup_ns,
        sqlite_prepare_calls: "unavailable",
        sqlite_statement_calls: stats.statements,
        sqlite_rows_read: stats.rows_read,
        sqlite_rows_inserted: stats.rows_inserted,
        new_objects: stats.new_objects,
        reused_objects: stats.reused_objects,
        db_bytes_before: before_db,
        db_bytes_after: after_db,
        wal_bytes_before: before_wal,
        wal_bytes_after: after_wal,
        user_cpu_ns: "unavailable",
        system_cpu_ns: "unavailable",
        peak_rss_bytes: "unavailable",
        retained: retain,
    })
}

fn crash(path: &Path, fixture: &Fixture, gear: &[u64], before_commit: bool) -> AppResult<()> {
    let conn = open_db(path, false)?;
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let mut stats = Stats::default();
    create(&conn, fixture, "end_to_end", gear, &mut stats)?;
    if before_commit {
        process::exit(37);
    }
    conn.execute_batch("COMMIT")?;
    process::exit(38)
}
fn self_check(exe: &Path) -> AppResult<()> {
    let fixture = load_fixture("random-10m")?;
    let path = Path::new(DATA).join("self-check-rsql.sqlite");
    remove_db_files(&path);
    let gear_table = gear()?;
    let first = one(&path, &fixture, "create", "end_to_end", &gear_table, false)?;
    if first.sqlite_version != "3.51.0" {
        return Err(format!("wrong SQLite: {}", first.sqlite_version).into());
    }
    let child = Command::new(exe)
        .arg("crash")
        .arg(&path)
        .arg("duplicate-10m")
        .arg("before-commit")
        .status()?;
    if child.code() != Some(37) {
        return Err(format!("crash injection status {:?}", child.code()).into());
    }
    let reopened = open_db(&path, false)?;
    let mut stats = Stats::default();
    verify_root(&reopened, &fixture, &mut stats)?;
    let baseline = table_counts(&reopened)?;
    if baseline.1 != 1 || baseline.2 != fixture.objects.len() as u64 || baseline.3 != 1 {
        return Err(format!("baseline database count mismatch: {baseline:?}").into());
    }
    drop(reopened);
    let before_recovered = open_db(&path, false)?;
    verify_root(&before_recovered, &fixture, &mut stats)?;
    assert_counts(
        table_counts(&before_recovered)?,
        baseline,
        "before-commit recovery",
    )?;
    drop(before_recovered);
    let reuse = open_db(&path, false)?;
    reuse.execute_batch("BEGIN IMMEDIATE")?;
    let object = &fixture.objects[0];
    let object_path = Path::new(DATA)
        .join("objects")
        .join(format!("{}-{}.bin", object.kind, object.digest));
    let mut reuse_stats = Stats::default();
    admit(&reuse, object, &fs::read(object_path)?, &mut reuse_stats)?;
    if reuse_stats.reused_objects != 1 {
        return Err("equal object was not reused".into());
    }
    reuse.execute_batch("ROLLBACK")?;
    drop(reuse);
    let child = Command::new(exe)
        .arg("crash")
        .arg(&path)
        .arg("duplicate-10m")
        .arg("after-commit")
        .status()?;
    if child.code() != Some(38) {
        return Err(format!("after-commit injection status {:?}", child.code()).into());
    }
    let duplicate = load_fixture("duplicate-10m")?;
    let committed = open_db(&path, false)?;
    let mut committed_stats = Stats::default();
    verify_root(&committed, &duplicate, &mut committed_stats)?;
    let existing: HashSet<String> = fixture
        .objects
        .iter()
        .map(|object| format!("{}:{}", object.kind, object.digest))
        .collect();
    let new_objects = duplicate
        .objects
        .iter()
        .filter(|object| !existing.contains(&format!("{}:{}", object.kind, object.digest)))
        .count() as u64;
    assert_counts(
        table_counts(&committed)?,
        (
            baseline.0 + new_objects,
            2,
            baseline.2 + duplicate.objects.len() as u64,
            1,
        ),
        "after-commit recovery",
    )?;
    drop(committed);
    let corrupt = open_db(&path, false)?;
    corrupt.execute_batch("BEGIN IMMEDIATE")?;
    let result = admit(&corrupt, object, b"wrong", &mut stats);
    if result.is_ok() || !result.unwrap_err().to_string().contains("unequal") {
        return Err("unequal same-ID accepted".into());
    }
    corrupt.execute_batch("ROLLBACK")?;
    drop(corrupt);
    println!(
        "{}",
        serde_json::json!({"self_check":"pass","fixture":fixture.fixture_id,"root_id":fixture.root_id,"sqlite_version":first.sqlite_version,"bounded_buffer_bytes":MAX})
    );
    Ok(())
}

fn main() -> AppResult<()> {
    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map(String::as_str).unwrap_or("campaign");
    let gear_table = gear()?;
    if command == "self-check" {
        return self_check(Path::new(args.first().unwrap()));
    }
    if command == "crash" {
        let path = PathBuf::from(&args[2]);
        let fixture = load_fixture(&args[3])?;
        return crash(
            &path,
            &fixture,
            &gear_table,
            args.get(4).map(String::as_str) == Some("before-commit"),
        );
    }
    let fixture_id = args.get(2).map(String::as_str).unwrap_or("random-10m");
    let operation = args.get(3).map(String::as_str).unwrap_or("create");
    let fixture = load_fixture(fixture_id)?;
    for i in 0..6 {
        let path = Path::new(DATA).join(format!("rsql-{fixture_id}-{operation}-{i}.sqlite"));
        remove_db_files(&path);
        if operation == "read" || operation == "materialize" {
            seed(&path, &fixture, "end_to_end", &gear_table)?;
        } else if operation == "edit" {
            seed(
                &path,
                &load_fixture(&edit_base_fixture_id(fixture_id))?,
                "end_to_end",
                &gear_table,
            )?;
        }
        let sample = one(
            &path,
            &fixture,
            operation,
            if operation == "storage_only" {
                "storage_only"
            } else {
                "end_to_end"
            },
            &gear_table,
            i > 0,
        )?;
        if i > 0 {
            println!("{}", serde_json::to_string(&sample)?);
        }
    }
    Ok(())
}
