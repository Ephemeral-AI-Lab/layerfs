//! Recorded name bindings of saved trees: hints for finding an inode's paths.
use super::params;
use super::rows::{cell, many, sql};
use super::transaction::Transaction;
use layerfs_history::error::HistoryResult;
use layerfs_history::records::BindingHint;

const INSERT_BINDING: &str = "hint_insert_binding";

const BINDINGS_OF: &str = "hint_bindings_of";

/// Records each binding once; a known row is left as it is.
pub(crate) fn record(tx: &Transaction<'_>, rows: &[BindingHint]) -> HistoryResult<()> {
    for row in rows {
        tx.execute(
            INSERT_BINDING,
            params![row.child as i64, row.parent as i64, row.name.clone()],
        )
        .map_err(sql)?;
    }
    Ok(())
}

/// Every recorded binding of one inode serial.
pub(crate) fn of(tx: &Transaction<'_>, child: u64) -> HistoryResult<Vec<BindingHint>> {
    many(tx, BINDINGS_OF, params![child as i64], |row| {
        Ok(BindingHint {
            child: cell::<i64>(row, 0)? as u64,
            parent: cell::<i64>(row, 1)? as u64,
            name: cell(row, 2)?,
        })
    })
}
