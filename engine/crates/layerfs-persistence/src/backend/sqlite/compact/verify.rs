//! §7 step 5: immutable verification of the staged file against the live tuple.

use super::build::Built;
use super::build::CACHE_KIB;
use super::op::stop_at;
use super::op::RunOp;
use super::types::db_error;
use super::types::CompactFailpoint;
use super::types::CompactRequest;
use super::types::MarkTuple;
use super::window::ReadAhead;
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
/// `pack` is read before this statement, by [`scan_packs`].
const SCAN_SQL: &str = "SELECT \
(SELECT count(*) FROM pragma_foreign_key_check),\
o.n,o.canonical,\
(SELECT count(*) FROM kept_node),\
(SELECT COALESCE(SUM(logical),0) FROM \"commit\")+\
(SELECT COALESCE(SUM(logical),0) FROM layer) \
FROM (SELECT count(*) AS n,COALESCE(SUM(canonical_length),0) AS canonical \
FROM object_location) o";

/// The staged file's pack rows between two identities: how many, their stored
/// bytes and the metadata-domain part of those.
const PACK_SCAN_SQL: &str = "SELECT count(*),COALESCE(SUM(length),0),\
COALESCE(SUM(CASE WHEN domain=0 THEN length ELSE 0 END),0) \
FROM pack WHERE pack_id BETWEEN ?1 AND ?2";

/// A window whose packs average under this many stored bytes is asked for
/// whole before its rows are read. A pack's row is one page beside its
/// payload, so the rows of dense packs lie a few pages apart over the whole
/// file: one sequential request brings them in faster than one device round
/// trip per pack. Larger packs leave too much payload between two rows.
const DENSE_PACK_BYTES: u64 = 512 * 1024;

/// What [`scan_packs`] counted.
#[derive(Default)]
struct PackScan {
    packs: i64,
    stored: i64,
    metadata: i64,
}

/// Counts and sums the staged file's `pack` rows, one window at a time.
///
/// `built.windows` says where each run of retained packs lies in the fresh
/// file. Read in one pass without it, the rows of a file the kernel no longer
/// holds cost one device round trip per pack (measured on a 4.9 GB Store of
/// 38,666 packs with the file out of memory: 2.3 s, against 0.7 s window by
/// window). The windows only place the read-ahead: the ranges read are
/// consecutive and cover every identity, so the sums are those of one pass
/// over the table, and a row outside every window is counted too. The file is
/// immutable to this reader, so the statements see one state. Without windows
/// (the group-row layouts keep `pack` small and in one place) one range reads
/// the table.
fn scan_packs(connection: &Connection, built: &Built, retained: &[i64]) -> HistoryResult<PackScan> {
    let plan = &built.windows;
    // What a window's range holds beyond its own payload (see `windows`).
    let slack = built.target_bytes.saturating_sub(built.stored);
    let mut ahead = ReadAhead::open(&built.target)?;
    let mut statement = connection.prepare(PACK_SCAN_SQL).map_err(db_error)?;
    let mut scan = PackScan::default();
    let mut read = |first: i64, last: i64| -> HistoryResult<()> {
        let (packs, stored, metadata): (i64, i64, i64) = statement
            .query_row(params![first, last], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map_err(db_error)?;
        scan.packs = scan.packs.saturating_add(packs);
        scan.stored = scan.stored.saturating_add(stored);
        scan.metadata = scan.metadata.saturating_add(metadata);
        Ok(())
    };
    let mut first = i64::MIN;
    let mut before = 0usize;
    for (index, window) in plan.iter().enumerate() {
        let through = retained.partition_point(|pack| *pack <= window.last);
        let packs = (through - before) as u64;
        before = through;
        if window.bytes.saturating_sub(slack) < packs.saturating_mul(DENSE_PACK_BYTES) {
            ahead.ask(window);
        }
        let last = if index + 1 == plan.len() {
            i64::MAX
        } else {
            window.last
        };
        read(first, last)?;
        first = last.saturating_add(1);
    }
    if plan.is_empty() {
        read(i64::MIN, i64::MAX)?;
    }
    Ok(scan)
}

/// What [`SCAN_SQL`] returns, in column order.
struct Scan {
    foreign_keys: i64,
    objects: i64,
    canonical: i64,
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
/// doctor stay separate commands (§7 step 6), so this pass reads no payload
/// except the packs the build wrote again, which [`super::repack::verify`]
/// reads back and hashes.
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
    // The foreign-key pass looks up the pack row of every object, in object
    // identity order, so the lookups land on the pack table's leaf pages at
    // random. Through the default 2 MiB cache nearly every lookup reads its
    // page from the file again (measured on a 4.9 GB Store of 38,666 packs:
    // 1.71 million page reads, against 47 thousand with a cache that holds
    // every such page, 256 MiB there). The build's cache size keeps as many
    // of the pages this pass has read as a compaction may hold.
    connection
        .execute_batch(&format!("PRAGMA cache_size=-{CACHE_KIB}"))
        .map_err(db_error)?;
    schema::check(
        &connection,
        &RefCell::new(SqlWork::default()),
        (layout, acquisition),
    )
    .map_err(HistoryError::from)?;
    // `pack` first: its rows are then in this reader's cache for the
    // foreign-key pass below.
    let packs = scan_packs(&connection, built, mark.objects().packs())?;
    // One statement and one pass over each other table: the row counts the
    // mark fixes and the sums the totals row must equal come from the same
    // scan.
    let scanned = connection
        .query_row(SCAN_SQL, params![], |row| {
            Ok(Scan {
                foreign_keys: row.get(0)?,
                objects: row.get(1)?,
                canonical: row.get(2)?,
                history: row.get(3)?,
                logical: row.get(4)?,
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
    if counted(packs.packs)? != mark.objects().packs().len() as u64 {
        return Err(HistoryError::Integrity("staged Store pack count"));
    }
    super::repack::verify(&connection, &built.repacked.packs)?;
    super::values::verify(&connection, mark.objects().inode_ordinals())?;
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
        || checked.stored != packs.stored
        || checked.metadata != packs.metadata
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
