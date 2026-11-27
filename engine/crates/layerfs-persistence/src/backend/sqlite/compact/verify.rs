//! §7 step 5: immutable verification of the staged file against the live tuple.

use super::build::Built;
use super::build::{cache_kib, CACHE_KIB};
use super::lengths::PackLengths;
use super::op::stop_at;
use super::op::visited_path;
use super::op::RunOp;
use super::repack::rewritten_row;
use super::repack::REWRITTEN;
use super::staged::PackScan;
use super::staged::StagedLocators;
use super::types::db_error;
use super::types::MarkTuple;
use super::window::staged_windows;
use super::window::ReadAhead;
use crate::backend::error::BackendError;
use crate::backend::sqlite::schema;
use crate::backend::sqlite::session::connection::Session;
use crate::backend::sqlite::session::connection::SqlWork;
use crate::gc::compact::CompactFailpoint;
use crate::gc::compact::CompactRequest;
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

/// The staged file's history row count and logical sum, and the foreign-key
/// failures of every table but `object_location`, whose one key (its pack)
/// [`scan_packs`] checks by merge. The other tables with a foreign key are
/// the history tables, `content_signature` and `pack_unit`: small, or read in
/// the order of the table they name.
const SCAN_SQL: &str = "SELECT \
(SELECT count(*) FROM sqlite_schema s, pragma_foreign_key_check(s.name) \
WHERE s.type='table' AND s.name!='object_location'),\
(SELECT count(*) FROM kept_node),\
(SELECT COALESCE(SUM(logical),0) FROM \"commit\")+\
(SELECT COALESCE(SUM(logical),0) FROM layer)";

/// `object_location`'s foreign keys: how many, and how many of them are its
/// pack. [`scan_packs`] checks exactly that one.
const LOCATOR_KEYS_SQL: &str = "SELECT count(*),\
COALESCE(SUM(\"table\"='pack' AND \"from\"='pack_id' AND \"to\"='pack_id'),0) \
FROM pragma_foreign_key_list('object_location')";

/// A window whose packs average under this many stored bytes is asked for
/// whole before its rows are read. A pack's row is one page beside its
/// payload, so the rows of dense packs lie a few pages apart over the whole
/// file: one sequential request brings them in faster than one device round
/// trip per pack. Larger packs leave too much payload between two rows.
const DENSE_PACK_BYTES: u64 = 512 * 1024;

/// What the pass by pack found: the object rows and the pack rows.
struct Staged {
    objects: u64,
    canonical: i64,
    packs: PackScan,
}

/// Reads the staged file by pack: one scan of `object_location`, then the
/// `pack` rows in pack order with their locators and the packs the build
/// wrote again ([`super::staged`]).
///
/// The `pack` rows are read one window at a time. The windows say where each
/// run of retained packs lies in the fresh file. Read in one pass without
/// them, the rows of a file the kernel no longer holds cost one device round
/// trip per pack (measured on a 4.9 GB Store of 38,666 packs with the file
/// out of memory: 2.3 s, against 0.7 s window by window). The windows only
/// place the read-ahead: the ranges read are consecutive and cover every
/// identity, so the pass is one scan of the table, and a row outside every
/// window is read too. The file is immutable to this reader, so the
/// statements see one state. The windows are cut from the stream of pack
/// lengths and each is read as soon as it is cut. Without windows (the
/// group-row layouts keep `pack` small and in one place) one range reads the
/// table.
fn scan_packs(
    connection: &Connection,
    scratch: &Path,
    built: &Built,
    lengths: &mut PackLengths,
    mark: &MarkResult,
) -> HistoryResult<Staged> {
    let retained = mark.objects().retained_packs();
    let mut locators = StagedLocators::read(connection, mark.sort_space())?;
    let (objects, canonical) = (locators.objects, locators.canonical);
    // The packs the build wrote again are listed in its scratch, which no
    // one writes any more; without one there is no such table.
    let scratch = match built.repacked.rewritten {
        0 => None,
        _ => Some(open_immutable(scratch)?),
    };
    let mut listed = match &scratch {
        Some(scratch) => Some(scratch.prepare(REWRITTEN).map_err(db_error)?),
        None => None,
    };
    let mut rows = match listed.as_mut() {
        Some(listed) => Some(listed.query(params![]).map_err(db_error)?),
        None => None,
    };
    let mut rewritten = std::iter::from_fn(|| {
        let row = rows.as_mut()?.next().map_err(db_error).transpose()?;
        Some(row.and_then(rewritten_row))
    });
    let mut pass = locators.pass(connection, &mut rewritten)?;
    if built.payload_in_pack {
        // What a window's range holds beyond its own payload (see `windows`).
        let slack = built.target_bytes.saturating_sub(built.stored);
        let mut ahead = ReadAhead::open(&built.target)?;
        staged_windows(lengths.packs()?, retained, slack, &mut |window| {
            if window.bytes.saturating_sub(slack) < window.packs.saturating_mul(DENSE_PACK_BYTES) {
                ahead.ask(&window);
            }
            pass.through(window.last)
        })?;
    }
    // Every identity after the last window, or the whole table without one.
    let packs = pass.finish()?;
    if packs.rewritten != built.repacked.rewritten {
        return Err(HistoryError::Integrity("staged Store repacked pack"));
    }
    Ok(Staged {
        objects,
        canonical,
        packs,
    })
}

/// What [`SCAN_SQL`] returns, in column order.
struct Scan {
    foreign_keys: i64,
    history: i64,
    logical: i64,
}

/// The staged Store and the mark a verification reads it against.
pub(crate) struct VerifyInput<'a> {
    pub(crate) store: &'a Path,
    pub(crate) built: &'a Built,
    /// Every pack of the sealed Store and its stored length.
    pub(crate) lengths: &'a mut PackLengths,
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
        lengths,
        mark,
        tuple,
        layout,
        acquisition,
    } = input;
    let connection = open_immutable(&built.target)?;
    // The passes over the objects and the packs read their tables forward
    // and need no cache. The foreign keys of the history tables are still
    // SQLite's check, one lookup per key at random over those tables: through
    // the default 2 MiB cache a Store of 426 thousand Commits read 453
    // thousand pages for it. The build's cache size, which the build's
    // connection has given back, keeps as many of those pages as a
    // compaction may hold.
    connection
        .execute_batch(&format!(
            "PRAGMA cache_size=-{}",
            cache_kib(mark.sort_space(), CACHE_KIB)
        ))
        .map_err(db_error)?;
    schema::check(
        &connection,
        &RefCell::new(SqlWork::default()),
        (layout, acquisition),
    )
    .map_err(HistoryError::from)?;
    // `object_location` has one foreign key, its pack, and the pass by pack
    // is what checks it. A schema that gave it another would not be covered.
    let keys: (i64, i64) = connection
        .query_row(LOCATOR_KEYS_SQL, params![], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .map_err(db_error)?;
    if keys != (1, 1) {
        return Err(HistoryError::Integrity("staged Store locator keys"));
    }
    // The objects, the packs and the packs written again: the row counts the
    // mark fixes, the sums the totals row must equal and every locator's
    // pack, in two forward scans.
    let staged = scan_packs(&connection, &visited_path(store), built, lengths, mark)?;
    let packs = staged.packs;
    let scanned = connection
        .query_row(SCAN_SQL, params![], |row| {
            Ok(Scan {
                foreign_keys: row.get(0)?,
                history: row.get(1)?,
                logical: row.get(2)?,
            })
        })
        .map_err(db_error)?;
    let counted = |value: i64| -> HistoryResult<u64> {
        u64::try_from(value).map_err(|_| HistoryError::Integrity("verification count"))
    };
    if counted(scanned.foreign_keys)? != 0 {
        return Err(HistoryError::Integrity("staged Store foreign key"));
    }
    if staged.objects != mark.objects().len() as u64 {
        return Err(HistoryError::Integrity("staged Store object count"));
    }
    if counted(packs.packs)? != mark.objects().retained_packs().len() {
        return Err(HistoryError::Integrity("staged Store pack count"));
    }
    super::values::verify(&connection, mark.objects().inode_ordinal_runs())?;
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
        || checked.canonical != staged.canonical
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
