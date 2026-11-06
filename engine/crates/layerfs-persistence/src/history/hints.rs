//! Recorded name bindings of saved trees: hints for finding an inode's
//! paths, kept in append-only blocks by group of 64 serials (#91).
use super::params;
use super::rows::{cell, many, sql};
use super::transaction::Transaction;
use crate::backend::Column;
use crate::blocks::hints::{decode, encode, group_of};
use layerfs_history::error::{HistoryError, HistoryResult};
use layerfs_history::records::BindingHint;

const INSERT_BLOCK: &str = "hint_insert_block";
const INSERT_BLOCKS: &str = "hint_insert_blocks";
/// Rows bound in one array statement.
pub(crate) const ARRAY_ROWS: usize = 1024;

const BLOCKS_OF: &str = "hint_blocks_of";

/// Appends the bindings as new blocks, each the next of its group. A
/// binding recorded before may be recorded again: readers drop repeats.
/// The blocks are encoded before any statement; an engine with an array
/// form takes up to [`ARRAY_ROWS`] blocks per statement.
pub(crate) fn record(tx: &Transaction<'_>, rows: &[BindingHint]) -> HistoryResult<()> {
    let blocks =
        encode(rows).map_err(|_| HistoryError::Integrity("binding hint block encoding"))?;
    for part in blocks.chunks(ARRAY_ROWS) {
        let columns = vec![
            Column::I64(part.iter().map(|block| block.group).collect()),
            Column::Bytes(part.iter().map(|block| block.body.clone()).collect()),
        ];
        if let Some(done) = tx.arrays(INSERT_BLOCKS, columns) {
            done?;
            continue;
        }
        for block in part {
            tx.execute(INSERT_BLOCK, params![block.group, block.body.clone()])
                .map_err(sql)?;
        }
    }
    Ok(())
}

/// Every recorded binding of one inode serial, in `(parent, name)` order
/// and without repeats, from its group's blocks.
pub(crate) fn of(tx: &Transaction<'_>, child: u64) -> HistoryResult<Vec<BindingHint>> {
    let group = group_of(child);
    let bodies = many(tx, BLOCKS_OF, params![group], |row| cell::<Vec<u8>>(row, 0))?;
    let mut found = Vec::new();
    for body in bodies {
        let hints =
            decode(group, &body).map_err(|_| HistoryError::Integrity("binding hint block"))?;
        found.extend(hints.into_iter().filter(|hint| hint.child == child));
    }
    found.sort_unstable();
    found.dedup();
    Ok(found)
}
