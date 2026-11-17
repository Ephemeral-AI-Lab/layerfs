//! §7 step 5: immutable verification of the staged file against the live tuple.

use super::build::Built;
use super::op::stop_at;
use super::op::RunOp;
use super::types::db_error;
use super::types::CompactFailpoint;
use super::types::CompactRequest;
use super::types::MarkTuple;
use crate::backend::records::BackendError;
use crate::backend::sqlite::schema;
use crate::backend::sqlite::session::connection::Session;
use crate::backend::sqlite::session::connection::SqlWork;
use crate::MarkResult;
use layerfs_history::error::HistoryError;
use layerfs_history::error::HistoryResult;
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::OpenFlags;
use std::cell::RefCell;
use std::path::Path;
use std::sync::Arc;

/// The first-statement tuple, in the column order every reader decodes.
pub(crate) const TUPLE_SQL: &str =
    "SELECT h.catalog_id,h.catalog_incarnation,h.next_history_serial,\
p.next_pack_id,p.next_ordinal,t.logical,t.canonical,t.stored,t.metadata,\
(SELECT COALESCE(MAX(stamp),0) FROM content_signature),\
(SELECT COUNT(*) FROM \"commit\")+(SELECT COUNT(*) FROM layer) \
FROM history_meta h, store_policy p, store_totals t \
WHERE h.id=1 AND p.id=1 AND t.id=1";

/// The §6.4 live-heads digest input, in identity order.
pub(crate) const LIVE_STACKS: &str = "SELECT s.layer_stack_id,s.head_layer_id FROM layer_stack s \
WHERE NOT EXISTS (SELECT 1 FROM layer_stack_retired r WHERE r.layer_stack_id=s.layer_stack_id) \
ORDER BY s.layer_stack_id";

/// The marker-free Branches, in identity order.
pub(crate) const LIVE_BRANCHES: &str =
    "SELECT b.branch_id,b.head_commit_id,b.base_layer_id FROM branch b \
WHERE NOT EXISTS (SELECT 1 FROM branch_retired r WHERE r.branch_id=b.branch_id) \
AND NOT EXISTS (SELECT 1 FROM layer_stack_retired s WHERE s.layer_stack_id=b.layer_stack_id) \
ORDER BY b.branch_id";

/// The read-only URI of one database file that must not be changed while it is
/// read. Path bytes are percent-encoded so `?`, `#` and `%` do not misdirect
/// the URI (§7 step 6).
#[cfg(unix)]
pub(crate) fn immutable_uri(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut uri = String::from("file:");
    for byte in path.as_os_str().as_bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                uri.push(char::from(*byte));
            }
            _ => {
                use std::fmt::Write as _;
                let _ = write!(uri, "%{byte:02X}");
            }
        }
    }
    uri.push_str("?immutable=1");
    uri
}

pub(crate) fn open_immutable(path: &Path) -> HistoryResult<Connection> {
    #[cfg(unix)]
    {
        let connection = Connection::open_with_flags(
            immutable_uri(path),
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_URI
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(db_error)?;
        connection
            .busy_timeout(std::time::Duration::ZERO)
            .map_err(db_error)?;
        Ok(connection)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err(HistoryError::Unsupported("SQLite Store compaction"))
    }
}

pub(crate) fn live_tuple(session: &Arc<Session>) -> HistoryResult<MarkTuple> {
    session
        .run::<_, BackendError>(false, |tx| {
            let rows = tx.query(TUPLE_SQL, vec![])?;
            let row = rows.first().ok_or(BackendError::Integrity)?;
            Ok(MarkTuple {
                catalog_id: row.get(0)?,
                generation: row.get(1)?,
                next_history_serial: row.get(2)?,
                next_pack_id: row.get(3)?,
                next_ordinal: row.get(4)?,
                logical: row.get(5)?,
                canonical: row.get(6)?,
                stored: row.get(7)?,
                metadata: row.get(8)?,
                stamp: row.get(9)?,
                history_rows: row.get(10)?,
            })
        })
        .map_err(Into::into)
}

pub(crate) fn file_tuple(connection: &Connection) -> HistoryResult<MarkTuple> {
    connection
        .query_row(TUPLE_SQL, params![], |row| {
            Ok(MarkTuple {
                catalog_id: row.get(0)?,
                generation: row.get(1)?,
                next_history_serial: row.get(2)?,
                next_pack_id: row.get(3)?,
                next_ordinal: row.get(4)?,
                logical: row.get(5)?,
                canonical: row.get(6)?,
                stored: row.get(7)?,
                metadata: row.get(8)?,
                stamp: row.get(9)?,
                history_rows: row.get(10)?,
            })
        })
        .map_err(db_error)
}

pub(crate) fn file_heads_digest(connection: &Connection) -> HistoryResult<[u8; 32]> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"layerfs/live-heads/v1\0");
    let mut stacks = connection.prepare(LIVE_STACKS).map_err(db_error)?;
    let mut rows = stacks.query(params![]).map_err(db_error)?;
    while let Some(row) = rows.next().map_err(db_error)? {
        let id: Vec<u8> = row.get(0).map_err(db_error)?;
        let head: Vec<u8> = row.get(1).map_err(db_error)?;
        if id.len() != 17 || head.len() != 33 {
            return Err(HistoryError::Integrity("live-heads stack width"));
        }
        hasher.update(&id);
        hasher.update(&head);
    }
    drop(rows);
    drop(stacks);
    let mut branches = connection.prepare(LIVE_BRANCHES).map_err(db_error)?;
    let mut rows = branches.query(params![]).map_err(db_error)?;
    while let Some(row) = rows.next().map_err(db_error)? {
        let id: Vec<u8> = row.get(0).map_err(db_error)?;
        let head: Option<Vec<u8>> = row.get(1).map_err(db_error)?;
        let base: Vec<u8> = row.get(2).map_err(db_error)?;
        if id.len() != 17 || base.len() != 33 {
            return Err(HistoryError::Integrity("live-heads branch width"));
        }
        hasher.update(&id);
        match head {
            Some(head) if head.len() == 33 => hasher.update(&head),
            Some(_) => return Err(HistoryError::Integrity("live-heads commit width")),
            None => hasher.update(&[0u8; 33]),
        };
        hasher.update(&base);
    }
    Ok(*hasher.finalize().as_bytes())
}

/// The staged file's row counts and recomputed sums, one pass per table.
const SCAN_SQL: &str = "SELECT \
(SELECT count(*) FROM pragma_foreign_key_check),\
o.n,o.canonical,p.n,p.stored,p.metadata,\
(SELECT count(*) FROM kept_node),\
(SELECT COALESCE(SUM(logical),0) FROM \"commit\")+\
(SELECT COALESCE(SUM(logical),0) FROM layer) \
FROM (SELECT count(*) AS n,COALESCE(SUM(canonical_length),0) AS canonical \
FROM object_location) o,\
(SELECT count(*) AS n,COALESCE(SUM(length),0) AS stored,\
COALESCE(SUM(CASE WHEN domain=0 THEN length ELSE 0 END),0) AS metadata FROM pack) p";

/// What [`SCAN_SQL`] returns, in column order.
struct Scan {
    foreign_keys: i64,
    objects: i64,
    canonical: i64,
    packs: i64,
    stored: i64,
    metadata: i64,
    history: i64,
    logical: i64,
}

/// The staged Store and the mark a verification reads it against.
pub(crate) struct VerifyInput<'a> {
    pub(crate) store: &'a Path,
    pub(crate) built: &'a Built,
    pub(crate) mark: &'a MarkResult,
    pub(crate) tuple: &'a MarkTuple,
    pub(crate) layout: crate::SqlitePackLayout,
    pub(crate) acquisition: crate::SqliteAcquisitionSchema,
}

/// The §7 step 6 verification through an `immutable=1` reader: schema identity,
/// foreign keys, row counts against the mark, both live-heads digests, the
/// allocator tuple and the recomputed totals. `quick_check` and the full
/// doctor stay separate commands (§7 step 6), so this pass reads no payload.
pub(crate) fn verify(
    input: VerifyInput<'_>,
    request: &CompactRequest,
    op: &RunOp,
) -> HistoryResult<()> {
    let VerifyInput {
        store,
        built,
        mark,
        tuple,
        layout,
        acquisition,
    } = input;
    let connection = open_immutable(&built.target)?;
    schema::check(
        &connection,
        &RefCell::new(SqlWork::default()),
        (layout, acquisition),
    )
    .map_err(HistoryError::from)?;
    // One statement and one pass over each table: the row counts the mark
    // fixes and the sums the totals row must equal come from the same scan.
    let scanned = connection
        .query_row(SCAN_SQL, params![], |row| {
            Ok(Scan {
                foreign_keys: row.get(0)?,
                objects: row.get(1)?,
                canonical: row.get(2)?,
                packs: row.get(3)?,
                stored: row.get(4)?,
                metadata: row.get(5)?,
                history: row.get(6)?,
                logical: row.get(7)?,
            })
        })
        .map_err(db_error)?;
    let counted = |value: i64| -> HistoryResult<u64> {
        u64::try_from(value).map_err(|_| HistoryError::Integrity("verification count"))
    };
    if counted(scanned.foreign_keys)? != 0 {
        return Err(HistoryError::Integrity("staged Store foreign key"));
    }
    if counted(scanned.objects)? != mark.objects().len() as u64 {
        return Err(HistoryError::Integrity("staged Store object count"));
    }
    if counted(scanned.packs)? != mark.objects().packs().len() as u64 {
        return Err(HistoryError::Integrity("staged Store pack count"));
    }
    super::repack::verify(&connection, &built.repacked)?;
    if counted(scanned.history)? != mark.history().len() as u64 {
        return Err(HistoryError::Integrity("staged Store history count"));
    }
    stop_at(request, CompactFailpoint::DuringVerify, op, store)?;
    if file_heads_digest(&connection)? != mark.live_heads_digest() {
        return Err(HistoryError::Conflict(
            "staged Store live heads differ from the mark",
        ));
    }
    let checked = file_tuple(&connection)?;
    if checked.catalog_id != tuple.catalog_id
        || checked.generation != tuple.generation + 1
        || checked.next_history_serial != tuple.next_history_serial
        || checked.next_pack_id != tuple.next_pack_id
        || checked.next_ordinal != tuple.next_ordinal
        || checked.stamp > tuple.stamp
    {
        return Err(HistoryError::Integrity("staged Store allocator tuple"));
    }
    // The totals row the tuple read, against the sums the scan recomputed.
    if checked.logical != scanned.logical
        || checked.canonical != scanned.canonical
        || checked.stored != scanned.stored
        || checked.metadata != scanned.metadata
    {
        return Err(HistoryError::Integrity("staged Store totals"));
    }
    let built_logical = u64::try_from(checked.logical).unwrap_or(u64::MAX);
    let built_canonical = u64::try_from(checked.canonical).unwrap_or(u64::MAX);
    let built_stored = u64::try_from(checked.stored).unwrap_or(u64::MAX);
    let built_metadata = u64::try_from(checked.metadata).unwrap_or(u64::MAX);
    if built_logical != built.logical
        || built_canonical != built.canonical
        || built_stored != built.stored
        || built_metadata != built.metadata
    {
        return Err(HistoryError::Integrity("staged Store build totals"));
    }
    connection.close().map_err(|(_, error)| db_error(error))?;
    Ok(())
}

/// The sealed file's tuple and live-heads digest through one `immutable=1`
/// reader (§7 step 1.5).
pub(crate) fn sealed_state(path: &Path) -> HistoryResult<(MarkTuple, [u8; 32])> {
    let connection = open_immutable(path)?;
    let tuple = file_tuple(&connection)?;
    let digest = file_heads_digest(&connection)?;
    connection.close().map_err(|(_, error)| db_error(error))?;
    Ok((tuple, digest))
}
