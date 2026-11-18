//! §6.3 visited scratch: a SQLite compaction's visited set is a temp-file
//! table the build attaches.
//!
//! The path is chosen after the walk, and a `none` decision drops the sink
//! without creating a file. A `rebuild` calls [`VisitSink::finish`], which
//! writes `<store>.compact-visited` (journal off, no sidecars) in one
//! transaction: the `visited` rows as the mark lists them, in identity
//! order, and the retained packs (`live_pack`) in pack order, so both tables
//! are filled by appending and neither needs a sort or a second pass in SQL.
//! A `visited` row is the object's whole locator, as the mark read it from
//! the Store while it held the writer lane, so the §7 build copies the kept
//! locators from this table in one forward scan and never looks one up in
//! the sealed Store. A row also says whether its object was reached only as
//! a delta base, so the rebuild can restate the mark's
//! `keptWaste.deltaBases.bytes` for the packs it writes again. The rows are
//! written as they are listed; the sink holds one statement's worth of them.
//! The build attaches the file as `scratch`, and the abort paths delete it
//! with the rest of the artifact set.
use layerfs_history::error::{HistoryError, HistoryResult};
use layerfs_storage::location::ObjectLocation;
use rusqlite::types::{ToSqlOutput, ValueRef};
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};

/// Rows one `INSERT` writes; seven bound values each.
const ROWS_PER_INSERT: usize = 128;

/// The scratch a rebuilding mark writes its reached objects to.
pub(crate) struct VisitSink {
    path: PathBuf,
    /// Whether [`Self::finish`] started the scratch file.
    created: bool,
    finished: bool,
}

fn sidecars(path: &Path) -> [PathBuf; 3] {
    let with = |suffix: &str| {
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        PathBuf::from(name)
    };
    [with("-journal"), with("-wal"), with("-shm")]
}

fn db_error(_error: rusqlite::Error) -> HistoryError {
    HistoryError::Integrity("visited scratch")
}

fn io_error(error: std::io::Error) -> HistoryError {
    match error.raw_os_error() {
        Some(28) | Some(122) | Some(27) | Some(69) => HistoryError::Capacity("disk space"),
        _ => HistoryError::Integrity("visited scratch I/O"),
    }
}

impl VisitSink {
    /// Starts a sink for the scratch beside the Store. The caller has already
    /// refused a recorded artifact; an existing name here is an unrecorded
    /// leftover. No file is created until [`Self::finish`].
    pub(crate) fn create(path: &Path) -> HistoryResult<Self> {
        if std::fs::symlink_metadata(path).is_ok() {
            return Err(HistoryError::Exists("staged compaction artifact"));
        }
        Ok(Self {
            path: path.to_path_buf(),
            created: false,
            finished: false,
        })
    }

    /// Writes the scratch in one transaction: every reached locator `list`
    /// hands over with its delta-base-only flag, which must come in ascending
    /// identity order with no identity twice, and the retained `packs`,
    /// ascending. Closes the connection, refuses a sidecar and returns the
    /// file's size.
    pub(crate) fn finish(
        mut self,
        packs: &[i64],
        list: impl FnOnce(
            &mut dyn FnMut(&ObjectLocation, bool) -> HistoryResult<()>,
        ) -> HistoryResult<()>,
    ) -> HistoryResult<u64> {
        if std::fs::symlink_metadata(&self.path).is_ok() {
            return Err(HistoryError::Exists("staged compaction artifact"));
        }
        self.created = true;
        let connection = Connection::open_with_flags(
            &self.path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(db_error)?;
        connection
            .busy_timeout(std::time::Duration::ZERO)
            .map_err(db_error)?;
        // The scratch is disposable: `journal_mode=OFF` leaves no journal of
        // its own, so the artifact set has no sidecars to judge.
        connection
            .execute_batch(
                "PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF;\
CREATE TABLE visited (\
 object_id BLOB PRIMARY KEY CHECK(length(object_id)=32),\
 role INTEGER NOT NULL,\
 canonical_length INTEGER NOT NULL,\
 pack_id INTEGER NOT NULL,\
 group_number INTEGER NOT NULL,\
 record_number INTEGER NOT NULL,\
 base_only INTEGER NOT NULL\
) STRICT, WITHOUT ROWID;\
CREATE TABLE live_pack (pack_id INTEGER PRIMARY KEY) STRICT, WITHOUT ROWID;\
BEGIN IMMEDIATE;",
            )
            .map_err(db_error)?;
        // Key order makes every insert an append to the table's last page.
        // The mark lists an object once and an object has one locator, so
        // the key refuses nothing; a row out of order would still be stored.
        let mut rows: Vec<(ObjectLocation, bool)> = Vec::with_capacity(ROWS_PER_INSERT);
        let write = |rows: &[(ObjectLocation, bool)]| {
            insert(
                &connection,
                "INSERT INTO visited(object_id,role,canonical_length,pack_id,\
group_number,record_number,base_only) VALUES",
                "(?,?,?,?,?,?,?)",
                rows,
                |(row, base_only)| {
                    [
                        blob(row.object_id.as_bytes()),
                        integer(i64::from(row.role.code())),
                        integer(row.canonical_length as i64),
                        integer(row.pack_id),
                        integer(row.group_number as i64),
                        integer(row.record_number as i64),
                        integer(i64::from(*base_only)),
                    ]
                },
            )
        };
        list(&mut |row, base_only| {
            rows.push((*row, base_only));
            if rows.len() == ROWS_PER_INSERT {
                write(&rows)?;
                rows.clear();
            }
            Ok(())
        })?;
        write(&rows)?;
        drop(rows);
        insert(
            &connection,
            "INSERT INTO live_pack(pack_id) VALUES",
            "(?)",
            packs,
            |pack_id| [integer(*pack_id)],
        )?;
        connection.execute_batch("COMMIT").map_err(db_error)?;
        connection.close().map_err(|(_, error)| db_error(error))?;
        for sidecar in sidecars(&self.path) {
            if std::fs::symlink_metadata(&sidecar).is_ok() {
                return Err(HistoryError::Integrity("visited scratch sidecar"));
            }
        }
        self.finished = true;
        let bytes = std::fs::metadata(&self.path).map_err(io_error)?.len();
        Ok(bytes)
    }
}

fn blob(bytes: &[u8]) -> ToSqlOutput<'_> {
    ToSqlOutput::Borrowed(ValueRef::Blob(bytes))
}

fn integer(value: i64) -> ToSqlOutput<'static> {
    ToSqlOutput::Borrowed(ValueRef::Integer(value))
}

/// Inserts `rows` in order, [`ROWS_PER_INSERT`] rows a statement and the
/// remainder in one shorter statement.
fn insert<'row, T, const N: usize>(
    connection: &Connection,
    head: &str,
    row: &str,
    rows: &'row [T],
    values: impl Fn(&'row T) -> [ToSqlOutput<'row>; N],
) -> HistoryResult<()> {
    let statement = |count: usize| format!("{head}{}", vec![row; count].join(","));
    let full = statement(ROWS_PER_INSERT);
    for batch in rows.chunks(ROWS_PER_INSERT) {
        let short;
        let sql = if batch.len() == ROWS_PER_INSERT {
            &full
        } else {
            short = statement(batch.len());
            &short
        };
        connection
            .prepare_cached(sql)
            .map_err(db_error)?
            .execute(rusqlite::params_from_iter(batch.iter().flat_map(&values)))
            .map_err(db_error)?;
    }
    Ok(())
}

impl Drop for VisitSink {
    fn drop(&mut self) {
        // A sink that never started its file has nothing to remove, and a
        // finished one leaves its scratch to the build.
        if self.finished || !self.created {
            return;
        }
        let _ = std::fs::remove_file(&self.path);
        for sidecar in sidecars(&self.path) {
            let _ = std::fs::remove_file(sidecar);
        }
    }
}
