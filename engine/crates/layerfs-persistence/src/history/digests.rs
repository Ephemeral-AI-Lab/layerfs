//! Saved file roots by the digest and length of their bytes (#364).
use super::params;
use super::rows::{cell, many, sql};
use super::transaction::Transaction;
use crate::backend::Column;
use layerfs_history::error::{HistoryError, HistoryResult};
use layerfs_history::records::{FileDigest, FILE_DIGEST_WINDOW};
use std::collections::BTreeMap;

const INSERT: &str = "digest_insert";
const INSERT_ROWS: &str = "digest_insert_rows";
const OF: &str = "digest_of";
const ROOTS: &str = "digest_roots";

fn length(value: u64) -> HistoryResult<i64> {
    i64::try_from(value).map_err(|_| HistoryError::Integrity("file digest length"))
}

/// Records the root built for each content; a recorded content takes the
/// newer root, so a row whose root the Store lost is replaced.
/// An engine with an array form takes one statement per window, the last
/// row of a repeated content winning as it would one row at a time.
pub(crate) fn record(tx: &Transaction<'_>, rows: &[FileDigest]) -> HistoryResult<()> {
    for part in rows.chunks(super::hints::ARRAY_ROWS) {
        let mut last = BTreeMap::new();
        for row in part {
            last.insert((row.digest, row.length), row.root);
        }
        let mut columns = (Vec::new(), Vec::new(), Vec::new());
        for ((digest, bytes), root) in last {
            columns.0.push(digest.to_vec());
            columns.1.push(length(bytes)?);
            columns.2.push(root.to_vec());
        }
        let columns = vec![
            Column::Bytes(columns.0),
            Column::I64(columns.1),
            Column::Bytes(columns.2),
        ];
        match tx.arrays(INSERT_ROWS, columns) {
            Some(done) => {
                done?;
            }
            None => one_by_one(tx, part)?,
        }
    }
    Ok(())
}

fn one_by_one(tx: &Transaction<'_>, rows: &[FileDigest]) -> HistoryResult<()> {
    for row in rows {
        tx.execute(
            INSERT,
            params![row.digest.to_vec(), length(row.length)?, row.root.to_vec()],
        )
        .map_err(sql)?;
    }
    Ok(())
}

/// The recorded root of each (digest, length), in the order asked, when the
/// Store holds that root's object: a row left by a Commit that never
/// finished names none.
pub(crate) fn roots(
    tx: &Transaction<'_>,
    contents: &[([u8; 32], u64)],
) -> HistoryResult<Vec<Option<[u8; 32]>>> {
    if contents.len() > FILE_DIGEST_WINDOW {
        return Err(HistoryError::Integrity("file digest window"));
    }
    let columns = vec![
        Column::Bytes(contents.iter().map(|(digest, _)| digest.to_vec()).collect()),
        Column::I64(
            contents
                .iter()
                .map(|(_, bytes)| length(*bytes))
                .collect::<HistoryResult<_>>()?,
        ),
    ];
    if let Some(found) = tx.arrays(ROOTS, columns) {
        let found = found?;
        if found.len() != contents.len() {
            return Err(HistoryError::Integrity("file digest roots"));
        }
        return found
            .iter()
            .map(|row| {
                cell::<Option<Vec<u8>>>(row, 1)?
                    .map(|root| {
                        <[u8; 32]>::try_from(root)
                            .map_err(|_| HistoryError::Integrity("file digest root"))
                    })
                    .transpose()
            })
            .collect();
    }
    contents
        .iter()
        .map(|(digest, bytes)| {
            let rows = many(tx, OF, params![digest.to_vec(), length(*bytes)?], |row| {
                cell::<Vec<u8>>(row, 0)
            })?;
            rows.into_iter()
                .next()
                .map(|root| {
                    <[u8; 32]>::try_from(root)
                        .map_err(|_| HistoryError::Integrity("file digest root"))
                })
                .transpose()
        })
        .collect()
}
