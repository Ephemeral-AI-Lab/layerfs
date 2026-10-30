//! Recorded name bindings of saved trees: hints for finding an inode's paths.
use super::params;
use super::rows::{cell, many, sql};
use super::transaction::Transaction;
use crate::backend::Column;
use layerfs_history::error::HistoryResult;
use layerfs_history::records::BindingHint;

const INSERT_BINDING: &str = "hint_insert_binding";
const INSERT_BINDINGS: &str = "hint_insert_bindings";
/// Rows bound in one array statement.
pub(crate) const ARRAY_ROWS: usize = 1024;

const BINDINGS_OF: &str = "hint_bindings_of";

/// Records each binding once; a known row is left as it is.
/// An engine with an array form takes up to [`ARRAY_ROWS`] per statement.
pub(crate) fn record(tx: &Transaction<'_>, rows: &[BindingHint]) -> HistoryResult<()> {
    for part in rows.chunks(ARRAY_ROWS) {
        let columns = vec![
            Column::I64(part.iter().map(|row| row.child as i64).collect()),
            Column::I64(part.iter().map(|row| row.parent as i64).collect()),
            Column::Bytes(part.iter().map(|row| row.name.clone()).collect()),
        ];
        if let Some(done) = tx.arrays(INSERT_BINDINGS, columns) {
            done?;
            continue;
        }
        for row in part {
            tx.execute(
                INSERT_BINDING,
                params![row.child as i64, row.parent as i64, row.name.clone()],
            )
            .map_err(sql)?;
        }
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
