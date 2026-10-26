//! Indexed root updates over a fixed SQL input, inside the caller's transaction.
use super::{
    accounting::{charge, ROOT_BYTES},
    statements::{signed, Failure},
};
use crate::backend::{sqlite::rows, Transaction};
use layerfs_content::ObjectId;
use layerfs_storage::port::acquisition::Owner;
use std::collections::BTreeSet;

/// Statement input slots, not a limit on the caller's public write window.
const INPUT_ROWS: usize = 32;

pub(crate) fn record(
    tx: &Transaction<'_>,
    owner: Owner,
    sql: &str,
    roots: &[(u64, ObjectId)],
) -> Result<(), Failure> {
    let operation = signed(owner.operation)?;
    let mut statement = tx.prepare(sql)?;
    let mut offset = 0;
    while offset < roots.len() {
        let mut positions = [None; INPUT_ROWS];
        let mut objects = [None; INPUT_ROWS];
        let mut distinct = BTreeSet::new();
        let mut count = 0;
        for (position, object) in roots[offset..].iter().take(INPUT_ROWS) {
            let position = match signed(*position) {
                Ok(position) => position,
                Err(_) if count != 0 => break,
                Err(error) => return Err(error),
            };
            // Stop before a repeated position. The next execution then sees
            // its already-filled row and returns the same first failure as
            // sequential updates. An earlier missing row still wins.
            if !distinct.insert(position) {
                break;
            }
            positions[count] = Some(position);
            objects[count] = Some(object.as_bytes().as_slice());
            count += 1;
        }
        let mut values: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(1 + 2 * INPUT_ROWS);
        values.push(&operation);
        for index in 0..INPUT_ROWS {
            values.push(&positions[index]);
            values.push(&objects[index]);
        }
        let mut recorded = statement.mapped(&values, 8 + 40 * count as u64, count, |row| {
            row.get::<_, i64>(0).map_err(rows::error)
        })?;
        // RETURNING order is not an input-order guarantee. Verify identities
        // against this fixed input; do not infer success from its row count.
        recorded.sort_unstable();
        for (position, _) in &roots[offset..offset + count] {
            if recorded.binary_search(&signed(*position)?).is_err() {
                return Err(Failure::Changed(*position));
            }
        }
        offset += count;
    }
    charge(tx, owner, 0, roots.len() as i64 * ROOT_BYTES)?;
    Ok(())
}
