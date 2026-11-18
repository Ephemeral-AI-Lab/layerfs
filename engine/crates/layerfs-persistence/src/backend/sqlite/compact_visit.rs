//! §6.3 visited scratch: a SQLite compaction's visited set is a temp-file
//! table the build attaches.
//!
//! The mark hands every reached object and its locating pack to the sink as
//! it discovers them. The sink only keeps them: the path is chosen after the
//! walk, and a `none` decision drops the sink without creating a file. A
//! `rebuild` calls [`VisitSink::finish`], which writes
//! `<store>.compact-visited` (journal off, no sidecars) in one transaction:
//! the `visited` rows in identity order and the retained packs (`live_pack`)
//! in pack order, so both tables are filled by appending and neither needs a
//! sort or a second pass in SQL. The §7 build attaches the file as `scratch`
//! and joins `object_location` against it, and the abort paths delete it
//! with the rest of the artifact set.
use layerfs_content::ObjectId;
use layerfs_history::error::{HistoryError, HistoryResult};
use rusqlite::types::{ToSqlOutput, ValueRef};
use rusqlite::{Connection, OpenFlags};
use std::cell::RefCell;
use std::path::{Path, PathBuf};

/// Rows one `INSERT` writes; two bound values each.
const ROWS_PER_INSERT: usize = 128;

/// The reached objects a mark hands over, and the scratch they are written to
/// when the run rebuilds.
pub(crate) struct VisitSink {
    path: PathBuf,
    /// Every reached object and its locating pack, in discovery order.
    reached: RefCell<Vec<(ObjectId, i64)>>,
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
            reached: RefCell::default(),
            created: false,
            finished: false,
        })
    }

    /// Keeps one reached object and its locating pack as it is discovered.
    pub(crate) fn visit(&self, id: ObjectId, pack_id: i64) -> HistoryResult<()> {
        self.reached.borrow_mut().push((id, pack_id));
        Ok(())
    }

    /// Writes the scratch: the visited rows and the retained packs derived
    /// from them, in one transaction. Closes the connection, refuses a
    /// sidecar and returns the file's size.
    pub(crate) fn finish(mut self) -> HistoryResult<u64> {
        let mut reached = self.reached.take();
        // Key order makes every insert an append to the table's last page.
        // The walk reaches an object once and an object has one locator, so
        // a repeated identity could only repeat its row.
        reached.sort_unstable_by(|left, right| left.0.cmp(&right.0));
        reached.dedup_by(|next, kept| next.0 == kept.0);
        let mut packs: Vec<i64> = reached.iter().map(|(_, pack_id)| *pack_id).collect();
        packs.sort_unstable();
        packs.dedup();
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
 pack_id INTEGER NOT NULL\
) STRICT, WITHOUT ROWID;\
CREATE TABLE live_pack (pack_id INTEGER PRIMARY KEY) STRICT, WITHOUT ROWID;\
BEGIN IMMEDIATE;",
            )
            .map_err(db_error)?;
        insert(
            &connection,
            "INSERT INTO visited(object_id,pack_id) VALUES",
            "(?,?)",
            &reached,
            |(id, pack_id)| [blob(id.as_bytes()), integer(*pack_id)],
        )?;
        drop(reached);
        insert(
            &connection,
            "INSERT INTO live_pack(pack_id) VALUES",
            "(?)",
            &packs,
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
