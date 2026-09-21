//! Diagnostic (not a gate): the commit term's page price, and whether declared pack capacity is
//! flushed.
//!
//! Registered in
//! `docs/roadmap/0.1/0.1.7/evidence/issue219-ns19t-commitprice-20260921T110000Z/pre-registration.md`
//! **before** its first run. It is a diagnostic in the sense of `docs/general/benchmark_rules.md`
//! §3.1: no product line changes, no arm is registered as a shippable change, no gate is claimed.
//!
//! It exists because round 18 took 27,022,997 ns out of `diag_commit_total_ns` by removing
//! 1,101,824 B of index - 24.5 ns per byte - while the same term prices the row's 301,865,004 B of
//! pack payload at 1.43 ns per byte. One term, two page prices, 17x apart. This diagnostic reads the
//! engine's own counters to find out whether the commit's price is per page-write, and whether the
//! 31,057,876 B of `zeroblob` capacity round 17 measured is written to disk at all.
//!
//! **Owner ruling: the database page size stays 4 KiB.** It is read and asserted, never set.
//!
//! Run:
//! ```text
//! cargo test --release --manifest-path core/benchmark/fs-bench-pro-storage-content/Cargo.toml \
//!   --test commit_page_price -- --nocapture --test-threads=1
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
/// `sqlite/write.rs:184`.
const CHUNK_CAP: usize = 128;
/// `sqlite/write.rs:177-178`.
const STORE_ROW_SQL_BYTES: usize =
    b"(?,?,?,?,?,?,(SELECT save_id FROM temp.layerfs_read_scope)),".len();
const REFERENCE_ROW_SQL_BYTES: usize = b"(?,?,?,?,?),".len();
/// `core/crates/layerfs-storage/src/policy.rs:103`.
const PACK_LIMIT: i64 = 256 * 1024;
/// The row's own average content per pack: `pack_bytes_written / packs_created` = 301,865,004 / 1,270.
const PARTIAL_BYTES: usize = 237_689;

// ---------------------------------------------------------------------------------------------
// The engine's own counters, through the raw handle (`rusqlite-0.40.2/src/lib.rs:952`).
// ---------------------------------------------------------------------------------------------

/// One connection-scoped counter, read without resetting it.
fn db_status(connection: &Connection, operation: i32) -> Option<i64> {
    let mut current: std::os::raw::c_int = 0;
    let mut highwater: std::os::raw::c_int = 0;
    // SAFETY: `handle()` is this connection's live `sqlite3*`, and `sqlite3_db_status` only reads
    // counters from it. Both out-parameters are owned locals.
    let code = unsafe {
        libsqlite3_sys::sqlite3_db_status(
            connection.handle(),
            operation,
            &mut current,
            &mut highwater,
            0,
        )
    };
    (code == 0).then(|| i64::from(current))
}

/// One process-global counter, read without resetting it.
fn global_status(operation: i32) -> Option<i64> {
    let mut current: std::os::raw::c_int = 0;
    let mut highwater: std::os::raw::c_int = 0;
    let code =
        unsafe { libsqlite3_sys::sqlite3_status(operation, &mut current, &mut highwater, 0) };
    (code == 0).then(|| i64::from(current))
}

/// The readings the pre-registration names. `None` is `NOT_MEASURED`, never a zero.
#[derive(Default, Clone, Copy)]
struct Counters {
    cache_write: Option<i64>,
    cache_spill: Option<i64>,
    cache_used: Option<i64>,
    pagecache_used: Option<i64>,
    pagecache_overflow: Option<i64>,
}

impl Counters {
    fn read(connection: &Connection) -> Self {
        Self {
            cache_write: db_status(connection, libsqlite3_sys::SQLITE_DBSTATUS_CACHE_WRITE),
            cache_spill: db_status(connection, libsqlite3_sys::SQLITE_DBSTATUS_CACHE_SPILL),
            cache_used: db_status(connection, libsqlite3_sys::SQLITE_DBSTATUS_CACHE_USED),
            pagecache_used: global_status(libsqlite3_sys::SQLITE_STATUS_PAGECACHE_USED),
            pagecache_overflow: global_status(libsqlite3_sys::SQLITE_STATUS_PAGECACHE_OVERFLOW),
        }
    }
}

fn difference(after: Option<i64>, before: Option<i64>) -> Option<i64> {
    match (after, before) {
        (Some(a), Some(b)) => Some(a - b),
        _ => None,
    }
}

fn number(value: Option<i64>) -> String {
    match value {
        Some(value) => value.to_string(),
        None => "null".to_owned(),
    }
}

fn pragma(connection: &Connection, name: &str) -> Result<i64, String> {
    connection
        .query_row(&format!("PRAGMA {name}"), [], |row| row.get(0))
        .map_err(|error| format!("{name}: {error}"))
}

// ---------------------------------------------------------------------------------------------
// The three locator shapes, transcribed from the two schemas as in round 17.
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Reference,
    StoreUnindexed,
    StoreIndexed,
}

impl Shape {
    fn name(self) -> &'static str {
        match self {
            Shape::Reference => "reference",
            Shape::StoreUnindexed => "store-unindexed",
            Shape::StoreIndexed => "store-indexed",
        }
    }

    fn parameters(self) -> usize {
        match self {
            Shape::Reference => 5,
            Shape::StoreIndexed | Shape::StoreUnindexed => 6,
        }
    }

    fn row_sql_bytes(self) -> usize {
        match self {
            Shape::Reference => REFERENCE_ROW_SQL_BYTES,
            Shape::StoreIndexed | Shape::StoreUnindexed => STORE_ROW_SQL_BYTES,
        }
    }

    fn schema(self) -> String {
        let saves = "CREATE TABLE saves (save_id INTEGER PRIMARY KEY, publication INTEGER) STRICT;";
        let objects = match self {
            Shape::Reference => {
                "CREATE TABLE objects (\
                 object_id BLOB NOT NULL PRIMARY KEY CHECK (length(object_id) = 32),\
                 canonical_length INTEGER NOT NULL CHECK (canonical_length > 0 AND canonical_length <= 16777216),\
                 pack_id INTEGER NOT NULL REFERENCES object_packs(pack_id),\
                 group_number INTEGER NOT NULL CHECK (group_number >= 0 AND group_number < 256),\
                 record_number INTEGER NOT NULL CHECK (record_number >= 0 AND record_number < 8191)\
                 ) STRICT, WITHOUT ROWID;"
            }
            Shape::StoreIndexed | Shape::StoreUnindexed => {
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
            Shape::Reference => "CREATE TABLE object_packs (\
                 pack_id INTEGER PRIMARY KEY CHECK (pack_id > 0),\
                 data BLOB NOT NULL\
                 ) STRICT;"
                .to_owned(),
            Shape::StoreIndexed | Shape::StoreUnindexed => "CREATE TABLE object_packs (\
                 pack_id INTEGER PRIMARY KEY CHECK (pack_id > 0),\
                 data BLOB NOT NULL,\
                 save_id INTEGER NOT NULL REFERENCES saves(save_id)\
                 ) STRICT;\
                 CREATE INDEX packs_save ON object_packs(save_id, pack_id);"
                .to_owned(),
        };
        let locator_index = match self {
            Shape::Reference | Shape::StoreUnindexed => "",
            Shape::StoreIndexed => "CREATE INDEX objects_save ON objects(save_id, object_id);",
        };
        format!("{saves}{packs}{objects}{locator_index}")
    }

    fn insert_sql(self, rows: usize) -> String {
        let store = matches!(self, Shape::StoreIndexed | Shape::StoreUnindexed);
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

fn chunk_rows(connection: &Connection, parameters: usize, row_sql_bytes: usize) -> usize {
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

// ---------------------------------------------------------------------------------------------
// The profile, the transaction, and the report.
// ---------------------------------------------------------------------------------------------

fn open(path: &Path) -> Result<Connection, String> {
    let _ = std::fs::remove_file(path);
    let connection = Connection::open(path).map_err(|error| error.to_string())?;
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
    let page_size = pragma(&connection, "page_size")?;
    if page_size != PAGE_SIZE {
        return Err(format!(
            "page size {page_size} is not the owner-ruled {PAGE_SIZE}; the arm is void"
        ));
    }
    Ok(connection)
}

/// One arm's whole report: the transaction is timed in its three parts and the engine's counters are
/// read across the COMMIT.
struct Arm {
    label: &'static str,
    statements: u64,
    payload_bytes: i64,
    insert_ns: u64,
    inplace_ns: u64,
    commit_ns: u64,
    pages_before: i64,
    pages_after: i64,
    freelist_after: i64,
    cache_write: Option<i64>,
    cache_spill: Option<i64>,
    cache_used_before: Option<i64>,
    cache_used_after: Option<i64>,
    pagecache_used: Option<i64>,
    pagecache_overflow: Option<i64>,
    file_bytes: u64,
    allocated_bytes: u64,
}

impl Arm {
    fn report(&self) {
        let written = self.pages_after - self.pages_before;
        let per_page = match self.cache_write {
            Some(pages) if pages > 0 => format!("{:.0}", self.commit_ns as f64 / pages as f64),
            _ => "null".to_owned(),
        };
        let per_byte = if self.payload_bytes > 0 {
            format!("{:.3}", self.commit_ns as f64 / self.payload_bytes as f64)
        } else {
            "null".to_owned()
        };
        println!(
            "{{\"arm\":{:?},\"statements\":{},\"payload_bytes\":{},\"insert_ns\":{},\
             \"inplace_ns\":{},\"commit_ns\":{},\"pages_before\":{},\"pages_after\":{},\
             \"pages_written\":{},\"freelist_after\":{},\"cache_write\":{},\"cache_spill\":{},\
             \"cache_used_before\":{},\"cache_used_after\":{},\"pagecache_used\":{},\
             \"pagecache_overflow\":{},\"ns_per_cache_write\":{},\"ns_per_payload_byte\":{},\
             \"file_bytes\":{},\"allocated_bytes\":{}}}",
            self.label,
            self.statements,
            self.payload_bytes,
            self.insert_ns,
            self.inplace_ns,
            self.commit_ns,
            self.pages_before,
            self.pages_after,
            written,
            self.freelist_after,
            number(self.cache_write),
            number(self.cache_spill),
            number(self.cache_used_before),
            number(self.cache_used_after),
            number(self.pagecache_used),
            number(self.pagecache_overflow),
            per_page,
            per_byte,
            self.file_bytes,
            self.allocated_bytes,
        );
    }
}

/// Q1: the three locator shapes, with the COMMIT timed apart from the insert loop.
fn measure_shape(shape: Shape, directory: &Path) -> Result<(), String> {
    let path: PathBuf = directory.join(format!("{}.sqlite", shape.name()));
    let connection = open(&path)?;
    connection
        .execute_batch(shape.schema().as_str())
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

    let pack_data = vec![0x5Au8; 64];
    for pack_id in 1..=PACKS {
        match shape {
            Shape::Reference => connection
                .execute(
                    "INSERT INTO object_packs (pack_id, data) VALUES (?1, ?2)",
                    rusqlite::params![pack_id, &pack_data],
                )
                .map_err(|error| error.to_string())?,
            Shape::StoreIndexed | Shape::StoreUnindexed => connection
                .execute(
                    "INSERT INTO object_packs (pack_id, data, save_id) VALUES (?1, ?2, ?3)",
                    rusqlite::params![pack_id, &pack_data, SAVE_ID],
                )
                .map_err(|error| error.to_string())?,
        };
    }

    let pages_before = pragma(&connection, "page_count")?;
    let before = Counters::read(&connection);
    let ids = object_ids();
    let chunk = chunk_rows(&connection, shape.parameters(), shape.row_sql_bytes());
    let parameters = shape.parameters();

    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(|error| error.to_string())?;
    let started = Instant::now();
    let mut statements = 0_u64;
    for (page_index, page) in ids.chunks(chunk).enumerate() {
        let sql = shape.insert_sql(page.len());
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
    let insert_ns = started.elapsed().as_nanos() as u64;
    let commit_started = Instant::now();
    connection
        .execute_batch("COMMIT")
        .map_err(|error| error.to_string())?;
    let commit_ns = commit_started.elapsed().as_nanos() as u64;
    let after = Counters::read(&connection);

    let metadata = std::fs::metadata(&path).map_err(|error| error.to_string())?;
    let arm = Arm {
        label: shape.name(),
        statements,
        payload_bytes: 0,
        insert_ns,
        inplace_ns: 0,
        commit_ns,
        pages_before,
        pages_after: pragma(&connection, "page_count")?,
        freelist_after: pragma(&connection, "freelist_count")?,
        cache_write: difference(after.cache_write, before.cache_write),
        cache_spill: difference(after.cache_spill, before.cache_spill),
        cache_used_before: before.cache_used,
        cache_used_after: after.cache_used,
        pagecache_used: after.pagecache_used,
        pagecache_overflow: after.pagecache_overflow,
        file_bytes: metadata.len(),
        allocated_bytes: allocated(&metadata),
    };
    arm.report();
    drop(connection);
    let _ = std::fs::remove_file(&path);
    Ok(())
}

#[cfg(unix)]
fn allocated(metadata: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.blocks() * 512
}

#[cfg(not(unix))]
fn allocated(_metadata: &std::fs::Metadata) -> u64 {
    0
}

/// Q2: 1,270 packs at `PACK_LIMIT`, written three ways.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PackArm {
    Zeroblob,
    Partial,
    Full,
}

impl PackArm {
    fn name(self) -> &'static str {
        match self {
            PackArm::Zeroblob => "packs-zeroblob",
            PackArm::Partial => "packs-partial",
            PackArm::Full => "packs-full",
        }
    }
}

fn measure_packs(arm: PackArm, directory: &Path) -> Result<(), String> {
    let path: PathBuf = directory.join(format!("{}.sqlite", arm.name()));
    let connection = open(&path)?;
    connection
        .execute_batch(
            "CREATE TABLE object_packs (\
             pack_id INTEGER PRIMARY KEY CHECK (pack_id > 0),\
             data BLOB NOT NULL\
             ) STRICT;",
        )
        .map_err(|error| error.to_string())?;

    let content = vec![0x5Au8; PARTIAL_BYTES];
    let full = vec![0x5Au8; PACK_LIMIT as usize];
    let pages_before = pragma(&connection, "page_count")?;
    let before = Counters::read(&connection);

    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(|error| error.to_string())?;
    let mut payload_bytes = 0_i64;
    let mut statements = 0_u64;
    let started = Instant::now();
    for pack_id in 1..=PACKS {
        match arm {
            PackArm::Zeroblob | PackArm::Partial => {
                connection
                    .execute(
                        "INSERT INTO object_packs (pack_id, data) VALUES (?1, zeroblob(?2))",
                        rusqlite::params![pack_id, PACK_LIMIT],
                    )
                    .map_err(|error| error.to_string())?;
            }
            PackArm::Full => {
                connection
                    .execute(
                        "INSERT INTO object_packs (pack_id, data) VALUES (?1, ?2)",
                        rusqlite::params![pack_id, &full],
                    )
                    .map_err(|error| error.to_string())?;
                payload_bytes += PACK_LIMIT;
            }
        }
        statements += 1;
    }
    let insert_ns = started.elapsed().as_nanos() as u64;

    // The product's real shape: a zero-filled row written in place through a BLOB handle
    // (`sqlite/write.rs:80-105`).
    let inplace_started = Instant::now();
    if arm == PackArm::Partial {
        for pack_id in 1..=PACKS {
            let mut blob = connection
                .blob_open("main", "object_packs", "data", pack_id, false)
                .map_err(|error| error.to_string())?;
            blob.write_at(&content, 0)
                .map_err(|error| error.to_string())?;
            drop(blob);
            payload_bytes += PARTIAL_BYTES as i64;
        }
    }
    let inplace_ns = inplace_started.elapsed().as_nanos() as u64;

    let commit_started = Instant::now();
    connection
        .execute_batch("COMMIT")
        .map_err(|error| error.to_string())?;
    let commit_ns = commit_started.elapsed().as_nanos() as u64;
    let after = Counters::read(&connection);

    let metadata = std::fs::metadata(&path).map_err(|error| error.to_string())?;
    let report = Arm {
        label: arm.name(),
        statements,
        payload_bytes,
        insert_ns,
        inplace_ns,
        commit_ns,
        pages_before,
        pages_after: pragma(&connection, "page_count")?,
        freelist_after: pragma(&connection, "freelist_count")?,
        cache_write: difference(after.cache_write, before.cache_write),
        cache_spill: difference(after.cache_spill, before.cache_spill),
        cache_used_before: before.cache_used,
        cache_used_after: after.cache_used,
        pagecache_used: after.pagecache_used,
        pagecache_overflow: after.pagecache_overflow,
        file_bytes: metadata.len(),
        allocated_bytes: allocated(&metadata),
    };
    report.report();
    drop(connection);
    let _ = std::fs::remove_file(&path);
    Ok(())
}

#[test]
fn commit_page_price_diagnostic() {
    let directory = std::env::temp_dir().join("layerfs-219-commit-page-price");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("diagnostic directory");
    eprintln!(
        "DIAGNOSTIC, not a gate: Q1 {} locator rows in three shapes with the COMMIT timed apart; \
         Q2 {} packs at PACK_LIMIT {PACK_LIMIT} written three ways; page size read and asserted",
        ROWS, PACKS
    );
    for shape in [Shape::Reference, Shape::StoreUnindexed, Shape::StoreIndexed] {
        if let Err(error) = measure_shape(shape, &directory) {
            panic!("shape {} failed: {error}", shape.name());
        }
    }
    for arm in [PackArm::Zeroblob, PackArm::Partial, PackArm::Full] {
        if let Err(error) = measure_packs(arm, &directory) {
            panic!("pack arm {} failed: {error}", arm.name());
        }
    }
}
