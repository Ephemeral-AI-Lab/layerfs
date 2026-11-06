//! Stored inode values in append-only blocks (#91), read and written in
//! bounded sets.
use crate::backend::{
    records::{BackendError, Param},
    Transaction,
};
use crate::blocks::values::{encode, pick};
use layerfs_storage::{location::InodeValueRow, port::*};

/// Ascending distinct ordinals as one JSON array of inclusive runs.
fn runs(ordinals: &[u32]) -> String {
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
    runs
}

/// Every stored value among ascending distinct ordinals, in ordinal order,
/// in one statement execution. The ordinals travel as one JSON parameter of
/// inclusive runs; each run seeks the last block starting at or before it
/// and reads the blocks from there to its end, so a leaf whose values were
/// written together costs one or two blocks.
pub(crate) fn read(
    tx: &Transaction<'_>,
    ordinals: &[u32],
    out: &mut Vec<InodeValueRow>,
) -> Result<(), PersistenceError> {
    let rows = tx.query(
        "SELECT b.first_ordinal,b.last_ordinal,b.body FROM inode_value_block b WHERE b.first_ordinal IN (\
         SELECT x.first_ordinal FROM json_each(?1) r JOIN inode_value_block x \
         ON x.first_ordinal BETWEEN coalesce((SELECT max(y.first_ordinal) FROM inode_value_block y \
         WHERE y.first_ordinal<=json_extract(r.value,'$[0]')),0) AND json_extract(r.value,'$[1]')) \
         ORDER BY b.first_ordinal",
        vec![Param::Text(runs(ordinals))],
    )?;
    let ordinal = |v: i64| u32::try_from(v).map_err(|_| BackendError::Integrity);
    pick(
        ordinals,
        rows.into_iter().map(|mut row| {
            Ok((
                ordinal(row.get::<i64>(0)?)?,
                ordinal(row.get::<i64>(1)?)?,
                row.take_bytes(2)?,
            ))
        }),
        out,
    )?;
    Ok(())
}

/// Inserts the values as new blocks in bounded multi-row statements. A
/// block whose ordinals a stored block already holds fails the
/// transaction: a stored value is never replaced.
pub(crate) fn write(tx: &Transaction<'_>, rows: &[InodeValueRow]) -> Result<(), BackendError> {
    let blocks = encode(rows)?;
    let limit = tx
        .input_limit(3, 512, 16)?
        .min(layerfs_storage::policy::BATCH_OBJECT_LIMIT);
    let mut remaining = blocks.as_slice();
    while !remaining.is_empty() {
        // Stable powers of two reuse prepared statements without padding rows.
        let count = 1usize << remaining.len().min(limit).ilog2();
        let (page, tail) = remaining.split_at(count);
        remaining = tail;
        // The last stored block starting at or before a new block's last
        // ordinal is the only stored block that can overlap it.
        let sql = format!(
            "INSERT INTO inode_value_block(first_ordinal,last_ordinal,body) \
             SELECT column1,column2,column3 FROM (VALUES {}) n WHERE NOT EXISTS (\
             SELECT 1 FROM inode_value_block b WHERE b.first_ordinal=(SELECT max(c.first_ordinal) \
             FROM inode_value_block c WHERE c.first_ordinal<=n.column2) AND b.last_ordinal>=n.column1) \
             RETURNING first_ordinal",
            std::iter::repeat_n("(?,?,?)", page.len())
                .collect::<Vec<_>>()
                .join(",")
        );
        let params = page
            .iter()
            .flat_map(|block| {
                [
                    Param::I64(i64::from(block.first)),
                    Param::I64(i64::from(block.last)),
                    Param::Bytes(block.body.clone()),
                ]
            })
            .collect();
        if tx.query(&sql, params)?.len() != page.len() {
            return Err(BackendError::Integrity);
        }
    }
    Ok(())
}
