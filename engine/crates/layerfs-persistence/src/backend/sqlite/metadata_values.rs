//! Stored inode values: one row an ordinal, read and written in bounded sets.
use crate::backend::{
    records::{BackendError, Param},
    Transaction,
};
use layerfs_storage::{location::InodeValueRow, port::*};
/// Every stored value among ascending distinct ordinals, in ordinal order, in
/// one statement execution. The ordinals travel as one JSON parameter of
/// inclusive runs, so a leaf whose values were written together costs one
/// range scan and a scattered one a rowid seek for each run.
pub(crate) fn read(
    tx: &Transaction<'_>,
    ordinals: &[u32],
    out: &mut Vec<InodeValueRow>,
) -> Result<(), PersistenceError> {
    let mut runs = String::from("[");
    let mut index = 0;
    while index < ordinals.len() {
        let first = ordinals[index];
        let mut last = first;
        while index + 1 < ordinals.len() && ordinals[index + 1] == last + 1 {
            index += 1;
            last = ordinals[index];
        }
        index += 1;
        if runs.len() > 1 {
            runs.push(',');
        }
        runs.push_str(&format!("[{first},{last}]"));
    }
    runs.push(']');
    let rows = tx.query(
        "SELECT v.ordinal,v.value FROM json_each(?1) r JOIN inode_value v ON v.ordinal BETWEEN json_extract(r.value,'$[0]') AND json_extract(r.value,'$[1]')",
        vec![Param::Text(runs)],
    )?;
    for row in rows {
        let ordinal = u32::try_from(row.get::<i64>(0)?).map_err(|_| BackendError::Integrity)?;
        let value = row
            .get::<Vec<u8>>(1)?
            .try_into()
            .map_err(|_| BackendError::Integrity)?;
        out.push(InodeValueRow { ordinal, value });
    }
    Ok(())
}
/// Inserts new rows in bounded multi-row statements. An ordinal that already
/// has a value fails the transaction: a stored value is never replaced.
pub(crate) fn write(tx: &Transaction<'_>, rows: &[InodeValueRow]) -> Result<(), BackendError> {
    let limit = tx
        .input_limit(2, 64, 8)?
        .min(layerfs_storage::policy::BATCH_OBJECT_LIMIT);
    let mut remaining = rows;
    while !remaining.is_empty() {
        // Stable powers of two reuse prepared statements without padding rows.
        let count = 1usize << remaining.len().min(limit).ilog2();
        let (page, tail) = remaining.split_at(count);
        remaining = tail;
        let sql = format!(
            "INSERT INTO inode_value(ordinal,value) VALUES {}",
            std::iter::repeat_n("(?,?)", page.len())
                .collect::<Vec<_>>()
                .join(",")
        );
        let params = page
            .iter()
            .flat_map(|row| {
                [
                    Param::I64(i64::from(row.ordinal)),
                    Param::Bytes(row.value.to_vec()),
                ]
            })
            .collect();
        tx.query(&sql, params)?;
    }
    Ok(())
}
