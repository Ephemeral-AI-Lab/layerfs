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

pub(crate) fn file_page_metrics(connection: &Connection) -> HistoryResult<(u64, u64)> {
    let free: i64 = connection
        .query_row("PRAGMA freelist_count", params![], |row| row.get(0))
        .map_err(db_error)?;
    let page: i64 = connection
        .query_row("PRAGMA page_size", params![], |row| row.get(0))
        .map_err(db_error)?;
    Ok((
        u64::try_from(free).map_err(|_| HistoryError::Integrity("freelist count"))?,
        u64::try_from(page).map_err(|_| HistoryError::Integrity("page size"))?,
    ))
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
/// allocator tuple and the recomputed totals. `quick_check` and the full doctor
/// stay separate commands.
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
    let count = |sql: &str| -> HistoryResult<u64> {
        let value: i64 = connection
            .query_row(sql, params![], |row| row.get(0))
            .map_err(db_error)?;
        u64::try_from(value).map_err(|_| HistoryError::Integrity("verification count"))
    };
    if count("SELECT count(*) FROM pragma_foreign_key_check")? != 0 {
        return Err(HistoryError::Integrity("staged Store foreign key"));
    }
    if count("SELECT count(*) FROM object_location")? != mark.objects().len() as u64 {
        return Err(HistoryError::Integrity("staged Store object count"));
    }
    if count("SELECT count(*) FROM pack")? != mark.objects().packs().len() as u64 {
        return Err(HistoryError::Integrity("staged Store pack count"));
    }
    if count("SELECT count(*) FROM kept_node")? != mark.history().len() as u64 {
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
    let totals = connection
        .query_row(
            "SELECT \
(SELECT logical FROM store_totals WHERE id=1),\
(SELECT COALESCE(SUM(logical),0) FROM \"commit\")+\
(SELECT COALESCE(SUM(logical),0) FROM layer),\
(SELECT canonical FROM store_totals WHERE id=1),\
(SELECT COALESCE(SUM(canonical_length),0) FROM object_location),\
(SELECT stored FROM store_totals WHERE id=1),\
(SELECT COALESCE(SUM(length),0) FROM pack),\
(SELECT metadata FROM store_totals WHERE id=1),\
(SELECT COALESCE(SUM(length),0) FROM pack WHERE domain=0)",
            params![],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                ))
            },
        )
        .map_err(db_error)?;
    if totals.0 != totals.1 || totals.2 != totals.3 || totals.4 != totals.5 || totals.6 != totals.7
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

pub(crate) fn file_free_bytes(target: &Path) -> HistoryResult<u64> {
    let measure = open_immutable(target)?;
    let (free, page) = file_page_metrics(&measure)?;
    measure.close().map_err(|(_, error)| db_error(error))?;
    Ok(free.saturating_mul(page))
}

pub(crate) fn immutable_tuple(path: &Path) -> HistoryResult<MarkTuple> {
    let connection = open_immutable(path)?;
    let tuple = file_tuple(&connection)?;
    connection.close().map_err(|(_, error)| db_error(error))?;
    Ok(tuple)
}

pub(crate) fn immutable_digest(path: &Path) -> HistoryResult<[u8; 32]> {
    let connection = open_immutable(path)?;
    let digest = file_heads_digest(&connection)?;
    connection.close().map_err(|(_, error)| db_error(error))?;
    Ok(digest)
}
