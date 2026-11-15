//! The `project_epoch` writer fence (spec §5).
//!
//! Every operation that creates a Commit or Layer reads the project's epoch at
//! the start of the write transaction that will insert it and re-checks that
//! same row before the transaction commits. Each removal marker bumps the epoch
//! in its own marker transaction (§4), so a value that changed between the two
//! reads means a removal raced this write: the writer refuses
//! `Conflict("project epoch")` and the whole write transaction rolls back, so
//! no Commit or Layer is left behind. A missing row (a project deleted and
//! compacted away, §4.1/§6) is the same `Conflict`, never "unchanged".
//!
//! The refusal code is `Conflict`: §10's table names it for the sealed tuple
//! and for a missing `project_epoch` row, and §12's marker-epoch case says a
//! one-shot write racing `branch.remove`/`project.delete` gets `Conflict` and
//! leaves no Commit. It carries no detail (§10; gc-spec-gaps G1 fixes the
//! empty detail for every `Conflict`).
//!
//! On SQLite both reads run inside the one `BEGIN IMMEDIATE` transaction that
//! also writes the Commit/Layer, so a marker can never interleave between them
//! (its `BEGIN IMMEDIATE` is `Busy` until this writer finishes). PostgreSQL
//! makes the re-check the writer's last statement before `COMMIT` with
//! `SELECT ... FOR SHARE` (`gc_epoch_share`), which also blocks the marker's
//! `UPDATE project_epoch SET epoch = epoch + 1` until this transaction resolves:
//! a removal that committed first is seen as a changed value, and one still in
//! flight is serialized after this writer.

use super::rows::{cell, one};
use super::transaction::Transaction;
use layerfs_history::error::{HistoryError, HistoryResult};
use layerfs_history::identity::LayerStackId;

/// Unlocked read at the start of the write transaction.
const READ: &str = "gc_epoch_read";
/// Re-read before `COMMIT`; `FOR SHARE` on PostgreSQL, a plain read on SQLite.
const SHARE: &str = "gc_epoch_share";

/// The epoch one writer observed when its write transaction opened.
#[derive(Clone, Copy)]
pub(crate) struct Fence {
    stack: LayerStackId,
    epoch: i64,
}

/// Reads the project epoch at the start of the write transaction.
///
/// A missing row is `Conflict`: after `project.delete` and compaction there is
/// no fence left to honour, so the write is refused rather than treated as
/// unchanged.
pub(crate) fn open(tx: &Transaction<'_>, stack: LayerStackId) -> HistoryResult<Fence> {
    let epoch = read(tx, READ, stack)?.ok_or(HistoryError::Conflict("project epoch"))?;
    Ok(Fence { stack, epoch })
}

impl Fence {
    /// Re-checks the epoch as the last statement before `COMMIT`; a changed or
    /// removed row is `Conflict` and the caller's transaction writes nothing.
    pub(crate) fn confirm(&self, tx: &Transaction<'_>) -> HistoryResult<()> {
        let current =
            read(tx, SHARE, self.stack)?.ok_or(HistoryError::Conflict("project epoch"))?;
        if current != self.epoch {
            return Err(HistoryError::Conflict("project epoch"));
        }
        Ok(())
    }
}

fn read(tx: &Transaction<'_>, statement: &str, stack: LayerStackId) -> HistoryResult<Option<i64>> {
    one(tx, statement, [stack.as_slice()], |row| cell::<i64>(row, 0))
}
