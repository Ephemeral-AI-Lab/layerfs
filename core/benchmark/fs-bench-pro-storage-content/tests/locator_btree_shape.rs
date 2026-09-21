//! Diagnostic (not a gate): price the locator table's B-tree shape.
//!
//! Registered in
//! `docs/roadmap/0.1/0.1.7/evidence/issue219-ns19r-algogap-20260921T095811Z/pre-registration.md`
//! **before** its first run. It is a diagnostic in the sense of
//! `docs/general/benchmark_rules.md` §3.1: it registers no row, changes no product line, and makes
//! no performance claim. It measures one mechanism - how many B-trees a locator insert maintains -
//! on a replica of the row's own row shape.
//!
//! **Owner ruling: the database page size stays 4 KiB.** This reads `PRAGMA page_size` and refuses a
//! database whose page size is not 4096. It never sets it.
//!
//! Three arms, one difference each, everything else identical:
//!
//! * `reference` - `crates/layerfs-layerstack-store/sql/schema/v7.sql:9-16`: a 32-byte
//!   `object_id` primary key, `WITHOUT ROWID`, and no secondary index anywhere in the schema
//!   (`grep -c "CREATE INDEX" v7.sql` = 0). One B-tree per row.
//! * `store-indexed` - `core/crates/layerfs-storage/sql/schema.sql:66-77`: a 40-byte
//!   `(object_id, save_id)` primary key plus `CREATE INDEX objects_save`. Two B-trees per row.
//! * `store-unindexed` - the same table as `store-indexed` with `objects_save` dropped, so the
//!   wider key and the second B-tree are separable rather than pooled.
//!
//! Run:
//! ```text
//! cargo test --manifest-path core/benchmark/fs-bench-pro-storage-content/Cargo.toml \
//!   --test locator_btree_shape -- --nocapture --test-threads=1
//! ```

use rusqlite::limits::Limit;
use rusqlite::types::Value;
use rusqlite::{params_from_iter, Connection};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// The row's own locator count: `pipeline.inserted` = `space.canonical_objects_total`.
const ROWS: usize = 25_245;
/// The row's own pack count: `pipeline.packs_created`.
const PACKS: i64 = 1_270;
const SAVE_ID: i64 = 1;
/// The owner-ruled page size. Read and asserted, never set.
const PAGE_SIZE: i64 = 4_096;
/// `sqlite/write.rs:184` - most rows this writer puts in one statement.
const CHUNK_CAP: usize = 128;
/// `sqlite/write.rs:177-178` - the SQL text one bound row contributes to the Store's statement.
const STORE_ROW_SQL_BYTES: usize =
    b"(?,?,?,?,?,?,(SELECT save_id FROM temp.layerfs_read_scope)),".len();
/// The reference's statement binds five columns; it has no `save_id` to subselect.
const REFERENCE_ROW_SQL_BYTES: usize = b"(?,?,?,?,?),".len();

#[derive(Clone, Copy, PartialEq, Eq)]
enum Arm {
    Reference,
    StoreIndexed,
    StoreUnindexed,
}

impl Arm {
    fn name(self) -> &'static str {
        match self {
            Arm::Reference => "reference",
            Arm::StoreIndexed => "store-indexed",
            Arm::StoreUnindexed => "store-unindexed",
        }
    }

    fn parameters(self) -> usize {
        match self {
            Arm::Reference => 5,
            Arm::StoreIndexed | Arm::StoreUnindexed => 6,
        }
    }

    fn row_sql_bytes(self) -> usize {
        match self {
            Arm::Reference => REFERENCE_ROW_SQL_BYTES,
            Arm::StoreIndexed | Arm::StoreUnindexed => STORE_ROW_SQL_BYTES,
        }
    }

    /// The arm's real DDL, transcribed from the two schemas. `saves` is reduced to the columns the
    /// locator's foreign key needs - it holds one row and is identical in every arm.
    fn schema(self) -> String {
        let saves = "CREATE TABLE saves (save_id INTEGER PRIMARY KEY, publication INTEGER) STRICT;";
        let objects = match self {
            Arm::Reference => {
                "CREATE TABLE objects (\
                 object_id BLOB NOT NULL PRIMARY KEY CHECK (length(object_id) = 32),\
                 canonical_length INTEGER NOT NULL CHECK (canonical_length > 0 AND canonical_length <= 16777216),\
                 pack_id INTEGER NOT NULL REFERENCES object_packs(pack_id),\
                 group_number INTEGER NOT NULL CHECK (group_number >= 0 AND group_number < 256),\
                 record_number INTEGER NOT NULL CHECK (record_number >= 0 AND record_number < 8191)\
                 ) STRICT, WITHOUT ROWID;"
            }
            Arm::StoreIndexed | Arm::StoreUnindexed => {
                "CREATE TABLE objects (\
                 object_id BLOB NOT NULL CHECK (length(object_id) = 32),\
                 save_id INTEGER NOT NULL REFERENCES saves(save_id),\
                 object_role INTEGER NOT NULL CHECK (object_role BETWEEN 1 AND 13),\
                 canonical_length INTEGER NOT NULL CHECK (canonical_length > 0 AND canonical_length <= 16777216),\
                 pack_id INTEGER NOT NULL REFERENCES object_packs(pack_id),\
                 group_number INTEGER NOT NULL CHECK (group_number >= 0 AND group_number < 256),\
                 record_number INTEGER NOT NULL CHECK (record_number >= 0 AND record_number < 8191),\
                 PRIMARY KEY (object_id, save_id)\
                 ) STRICT, WITHOUT ROWID;"
            }
        };
        let packs = match self {
            Arm::Reference => "CREATE TABLE object_packs (\
                 pack_id INTEGER PRIMARY KEY CHECK (pack_id > 0),\
                 data BLOB NOT NULL\
                 ) STRICT;"
                .to_owned(),
            Arm::StoreIndexed | Arm::StoreUnindexed => "CREATE TABLE object_packs (\
                 pack_id INTEGER PRIMARY KEY CHECK (pack_id > 0),\
                 data BLOB NOT NULL,\
                 save_id INTEGER NOT NULL REFERENCES saves(save_id)\
                 ) STRICT;\
                 CREATE INDEX packs_save ON object_packs(save_id, pack_id);"
                .to_owned(),
        };
        // The one difference between the two Store arms.
        let locator_index = match self {
            Arm::Reference | Arm::StoreUnindexed => "",
            Arm::StoreIndexed => "CREATE INDEX objects_save ON objects(save_id, object_id);",
        };
        format!("{saves}{packs}{objects}{locator_index}")
    }

    /// The product's own statement shape (`sqlite/write.rs:217-269`), at this arm's columns.
    fn insert_sql(self, rows: usize) -> String {
        let store = matches!(self, Arm::StoreIndexed | Arm::StoreUnindexed);
        let head = if store {
            "INSERT INTO objects (object_id, object_role, canonical_length, pack_id, group_number, record_number, save_id) VALUES "
        } else {
            "INSERT INTO objects (object_id, canonical_length, pack_id, group_number, record_number) VALUES "
        };
        let mut sql = String::from(head);
        for index in 0..rows {
            if index > 0 {
                sql.push(',');
            }
            sql.push('(');
            for column in 0..self.parameters() {
                if column > 0 {
                    sql.push(',');
                }
                sql.push('?');
            }
            if store {
                sql.push_str(",(SELECT save_id FROM temp.layerfs_read_scope))");
            } else {
                sql.push(')');
            }
        }
        sql
    }
}

/// Rows one multi-row `INSERT` may carry, derived from the engine's own limits exactly as
/// `sqlite/write.rs:194-208` derives it.
fn chunk_rows(connection: &Connection, parameters: usize, row_sql_bytes: usize) -> usize {
    // `rusqlite::Connection::limit` answers in `i32`; the product converts to `usize` at
    // `sqlite/write.rs:197-206` and this mirrors it.
    let variables = usize::try_from(
        connection
            .limit(Limit::SQLITE_LIMIT_VARIABLE_NUMBER)
            .unwrap_or(1),
    )
    .unwrap_or(1);
    let sql_length = usize::try_from(
        connection
            .limit(Limit::SQLITE_LIMIT_SQL_LENGTH)
            .unwrap_or(1),
    )
    .unwrap_or(1);
    let by_variables = variables.checked_div(parameters).unwrap_or(0).max(1);
    let by_sql = sql_length.checked_div(row_sql_bytes).unwrap_or(0).max(1);
    by_variables.min(by_sql).min(CHUNK_CAP)
}

fn splitmix64(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// 32 deterministic, distinct ids in an order arbitrary with respect to `object_id` - the order this
/// Store inserts in (`cas/placement.rs:166-182` builds rows in placement order and never sorts).
/// The same bytes, in the same order, in every arm.
fn object_ids() -> Vec<[u8; 32]> {
    (0..ROWS as u64)
        .map(|index| {
            let mut id = [0u8; 32];
            let mut word = splitmix64(index);
            for slot in 0..4 {
                id[slot * 8..slot * 8 + 8].copy_from_slice(&word.to_le_bytes());
                word = splitmix64(word);
            }
            id
        })
        .collect()
}

fn page_count(connection: &Connection) -> Result<i64, String> {
    connection
        .query_row("PRAGMA page_count", [], |row| row.get(0))
        .map_err(|error| error.to_string())
}

fn freelist_count(connection: &Connection) -> Result<i64, String> {
    connection
        .query_row("PRAGMA freelist_count", [], |row| row.get(0))
        .map_err(|error| error.to_string())
}

/// Pages per B-tree, read off the engine. `None` when the engine was built without `dbstat`.
fn dbstat(connection: &Connection) -> Option<Vec<(String, i64, i64)>> {
    let mut statement = connection
        .prepare("SELECT name, SUM(pgsize), COUNT(*) FROM dbstat GROUP BY name ORDER BY name")
        .ok()?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .ok()?
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    Some(rows)
}

fn json_objects(rows: &[(String, i64, i64)]) -> String {
    let parts = rows
        .iter()
        .map(|(name, bytes, pages)| {
            format!("{{\"name\":{name:?},\"bytes\":{bytes},\"pages\":{pages}}}")
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("[{parts}]")
}

fn measure(arm: Arm, directory: &Path) -> Result<(), String> {
    let path: PathBuf = directory.join(format!("{}.sqlite", arm.name()));
    let _ = std::fs::remove_file(&path);
    let connection = Connection::open(&path).map_err(|error| error.to_string())?;

    // The product's profile, `core/crates/layerfs-storage/src/sqlite/connection.rs:33-47`.
    let journal: String = connection
        .query_row("PRAGMA journal_mode = MEMORY", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if !journal.eq_ignore_ascii_case("memory") {
        return Err(format!("journal mode is {journal}, not MEMORY"));
    }
    connection
        .execute_batch("PRAGMA synchronous = OFF; PRAGMA temp_store = MEMORY;")
        .map_err(|error| error.to_string())?;
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|error| error.to_string())?;
    let page_size: i64 = connection
        .query_row("PRAGMA page_size", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if page_size != PAGE_SIZE {
        return Err(format!(
            "page size {page_size} is not the owner-ruled {PAGE_SIZE}; the run is void"
        ));
    }

    connection
        .execute_batch(arm.schema().as_str())
        .map_err(|error| error.to_string())?;
    connection
        .execute_batch(
            "CREATE TEMP TABLE layerfs_read_scope (save_id INTEGER NOT NULL, publication INTEGER NOT NULL);",
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "INSERT INTO saves (save_id, publication) VALUES (?1, NULL)",
            [SAVE_ID],
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "INSERT INTO temp.layerfs_read_scope (save_id, publication) VALUES (?1, 0)",
            [SAVE_ID],
        )
        .map_err(|error| error.to_string())?;

    // Seed the same packs in every arm, before the first reading, so pack pages never enter the
    // locator delta.
    let pack_data = vec![0x5Au8; 64];
    for pack_id in 1..=PACKS {
        match arm {
            Arm::Reference => connection
                .execute(
                    "INSERT INTO object_packs (pack_id, data) VALUES (?1, ?2)",
                    rusqlite::params![pack_id, &pack_data],
                )
                .map_err(|error| error.to_string())?,
            Arm::StoreIndexed | Arm::StoreUnindexed => connection
                .execute(
                    "INSERT INTO object_packs (pack_id, data, save_id) VALUES (?1, ?2, ?3)",
                    rusqlite::params![pack_id, &pack_data, SAVE_ID],
                )
                .map_err(|error| error.to_string())?,
        };
    }

    let pages_before = page_count(&connection)?;
    let freelist_before = freelist_count(&connection)?;
    let dbstat_before = dbstat(&connection);

    let ids = object_ids();
    let chunk = chunk_rows(&connection, arm.parameters(), arm.row_sql_bytes());
    let parameters = arm.parameters();
    let started = Instant::now();
    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(|error| error.to_string())?;
    let mut statements = 0_u64;
    for (page_index, page) in ids.chunks(chunk).enumerate() {
        let sql = arm.insert_sql(page.len());
        let mut values: Vec<Value> = Vec::with_capacity(page.len() * parameters);
        for (offset, id) in page.iter().enumerate() {
            let index = page_index * chunk + offset;
            values.push(Value::Blob(id.to_vec()));
            if parameters == 6 {
                values.push(Value::Integer(1 + (index as i64 % 13)));
            }
            values.push(Value::Integer(1 + (index as i64 % 20_000)));
            values.push(Value::Integer(1 + (index as i64 / 20)));
            values.push(Value::Integer((index as i64 / 2) % 256));
            values.push(Value::Integer(index as i64 % 2));
        }
        let affected = connection
            .prepare_cached(&sql)
            .map_err(|error| error.to_string())?
            .execute(params_from_iter(values))
            .map_err(|error| error.to_string())?;
        if affected != page.len() {
            return Err(format!("insert cardinality {affected} != {}", page.len()));
        }
        statements += 1;
    }
    connection
        .execute_batch("COMMIT")
        .map_err(|error| error.to_string())?;
    let elapsed_ns = started.elapsed().as_nanos() as u64;

    let pages_after = page_count(&connection)?;
    let freelist_after = freelist_count(&connection)?;
    let dbstat_after = dbstat(&connection);
    let file_bytes = std::fs::metadata(&path)
        .map_err(|error| error.to_string())?
        .len();

    let pages_dirtied = pages_after - pages_before;
    let ns_per_row = elapsed_ns as f64 / ROWS as f64;
    let bytes_per_row = (pages_dirtied as f64) * (page_size as f64) / (ROWS as f64);
    let stat = match &dbstat_after {
        Some(rows) => json_objects(rows),
        None => "null".to_owned(),
    };
    println!(
        "{{\"arm\":{:?},\"rows\":{ROWS},\"statements\":{statements},\"chunk_rows\":{chunk},\
         \"parameters_per_row\":{parameters},\"page_size\":{page_size},\
         \"pages_before\":{pages_before},\"pages_after\":{pages_after},\
         \"pages_dirtied\":{pages_dirtied},\"bytes_per_row\":{bytes_per_row:.1},\
         \"freelist_before\":{freelist_before},\"freelist_after\":{freelist_after},\
         \"elapsed_ns\":{elapsed_ns},\"ns_per_row\":{ns_per_row:.0},\
         \"file_bytes\":{file_bytes},\"dbstat_after\":{stat}}}",
        arm.name()
    );
    let _ = dbstat_before;
    Ok(())
}

#[test]
fn locator_btree_shape_diagnostic() {
    let directory = std::env::temp_dir().join("layerfs-219-locator-btree-shape");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("diagnostic directory");
    eprintln!(
        "DIAGNOSTIC, not a gate: {} rows, {} packs, one transaction per arm, page size read and asserted",
        ROWS, PACKS
    );
    for arm in [Arm::Reference, Arm::StoreUnindexed, Arm::StoreIndexed] {
        if let Err(error) = measure(arm, &directory) {
            panic!("arm {} failed: {error}", arm.name());
        }
    }
}
