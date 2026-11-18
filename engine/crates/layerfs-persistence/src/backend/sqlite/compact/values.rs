//! §7 rebuild: only the inode value blocks a kept leaf reads are copied.
//!
//! An inode leaf stores ordinals; the values live in `inode_value_block` rows
//! of up to 255 consecutive ordinals. The mark reads every reached leaf, so it
//! knows each ordinal a read of a kept leaf asks for
//! (`KeptObjects::inode_ordinal_runs`). A block that holds none of them belongs
//! only to leaves the rebuild collects, and used to cross every rebuild: a
//! Store whose large project was deleted kept all of that project's values.
//!
//! A block is one unit: a block with any read ordinal is copied whole, byte
//! for byte, so no value is encoded again and the ordinals a kept block holds
//! beside the read ones stay. Gaps between blocks are already valid in a
//! Store (an ordinal range is reserved before its values are written), and
//! `store_policy.next_ordinal` is copied unchanged, so no ordinal is reused.
//!
//! [`plan`] writes the read ordinals, as runs, to the mark's visited scratch
//! (`<store>.compact-visited`, table `live_ordinal`), where [`LIVE_BLOCK`]
//! looks each block up with one key seek. [`verify`] reads the staged file
//! back and requires every read ordinal to be inside a staged block.

use super::types::db_error;
use crate::Spans;
use layerfs_history::error::{HistoryError, HistoryResult};
use rusqlite::{params, Connection};

/// The read ordinals as inclusive runs. Runs never overlap, so the run that
/// starts last at or before an ordinal is the only one that can hold it.
const TABLE: &str = "CREATE TABLE scratch.live_ordinal (\
run_first INTEGER PRIMARY KEY, run_last INTEGER NOT NULL) STRICT;";

/// The filter that keeps a block holding a read ordinal: the last run starting
/// at or before the block's end must reach the block's start.
pub(crate) const LIVE_BLOCK: &str = "first_ordinal<=(SELECT run_last FROM scratch.live_ordinal \
WHERE run_first<=last_ordinal ORDER BY run_first DESC LIMIT 1)";

/// Writes the mark's read ordinals to the scratch the block copy reads.
/// The runs of `read` ascend, so the rows are appended in key order.
pub(crate) fn plan(connection: &Connection, read: &Spans) -> HistoryResult<()> {
    connection.execute_batch(TABLE).map_err(db_error)?;
    let mut insert = connection
        .prepare("INSERT INTO scratch.live_ordinal(run_first,run_last) VALUES(?1,?2)")
        .map_err(db_error)?;
    read.each(&mut |first, last| {
        insert
            .execute(params![i64::from(first), i64::from(last)])
            .map_err(db_error)?;
        Ok(())
    })
}

/// The body bytes of the blocks the fresh file holds: the §11
/// `keptWaste.inodeBlocksBytes` of the rebuilt Store.
pub(crate) fn kept_bytes(connection: &Connection) -> HistoryResult<u64> {
    let bytes: i64 = connection
        .query_row(
            "SELECT COALESCE(SUM(length(body)),0) FROM main.inode_value_block",
            params![],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    u64::try_from(bytes).map_err(|_| HistoryError::Integrity("staged Store inode value blocks"))
}

/// §7 step 6 for the block copy: every ordinal the mark read is inside a
/// block of the staged file. One ordered pass over the staged block keys and
/// the runs; nothing outside the staged file is read.
pub fn verify(connection: &Connection, read: &Spans) -> HistoryResult<()> {
    const MISSING: HistoryError = HistoryError::Integrity("staged Store inode value blocks");
    let mut statement = connection
        .prepare("SELECT first_ordinal,last_ordinal FROM inode_value_block ORDER BY first_ordinal")
        .map_err(db_error)?;
    let mut rows = statement.query(params![]).map_err(db_error)?;
    let mut next = || -> HistoryResult<Option<(i64, i64)>> {
        match rows.next().map_err(db_error)? {
            Some(row) => Ok(Some((
                row.get(0).map_err(db_error)?,
                row.get(1).map_err(db_error)?,
            ))),
            None => Ok(None),
        }
    };
    let mut block = next()?;
    read.each(&mut |first, last| {
        // The first ordinal of the run no block has covered yet.
        let (mut wanted, last) = (i64::from(first), i64::from(last));
        while wanted <= last {
            match block {
                Some((_, end)) if end < wanted => block = next()?,
                Some((start, end)) if start <= wanted => wanted = end + 1,
                _ => return Err(MISSING),
            }
        }
        Ok(())
    })
}
