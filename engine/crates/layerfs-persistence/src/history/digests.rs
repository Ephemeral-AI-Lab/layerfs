//! Saved file roots by the digest and length of their bytes (#364), kept
//! only for chunked files (#91): a file below the small-file threshold is one
//! whole-file object, so building it again finds the stored object by its
//! identity, and its digest row would cost more than it saves.
use super::params;
use super::rows::{cell, many, sql};
use super::{provider::HistoryProvider, transaction::Transaction};
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

/// Records the root built for each content of at least `floor` bytes; a
/// recorded content takes the newer root, so a row whose root the Store lost
/// is replaced. A smaller content is not recorded.
/// A repeated content keeps only its last root, as it would one row at a
/// time; an engine with an array form then takes one statement per window,
/// any other one statement per distinct content (a window of 1,000 empty
/// files is one row, not 1,000).
pub(crate) fn record(tx: &Transaction<'_>, rows: &[FileDigest], floor: u64) -> HistoryResult<()> {
    let rows: Vec<FileDigest> = rows
        .iter()
        .filter(|row| row.length >= floor)
        .copied()
        .collect();
    for part in rows.chunks(super::hints::ARRAY_ROWS) {
        let mut last = BTreeMap::new();
        for row in part {
            last.insert((row.digest, row.length), row.root);
        }
        let mut columns = (Vec::new(), Vec::new(), Vec::new());
        for ((digest, bytes), root) in &last {
            columns.0.push(digest.to_vec());
            columns.1.push(length(*bytes)?);
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
            None => {
                for ((digest, bytes), root) in last {
                    tx.execute(
                        INSERT,
                        params![digest.to_vec(), length(bytes)?, root.to_vec()],
                    )
                    .map_err(sql)?;
                }
            }
        }
    }
    Ok(())
}

/// The recorded root of each (digest, length), in the order asked, when the
/// Store holds that root's object: a row left by a Commit that never
/// finished names none. A content below the floor has no row and is not
/// asked for; a window of only such contents costs no statement.
pub(crate) fn roots(
    provider: &HistoryProvider,
    contents: &[([u8; 32], u64)],
) -> HistoryResult<Vec<Option<[u8; 32]>>> {
    if contents.len() > FILE_DIGEST_WINDOW {
        return Err(HistoryError::Integrity("file digest window"));
    }
    let asked: Vec<([u8; 32], u64)> = contents
        .iter()
        .filter(|(_, length)| *length >= provider.digest_floor)
        .copied()
        .collect();
    let mut found = match asked.is_empty() {
        true => Vec::new(),
        false => provider.read_statement(|tx| stored(tx, &asked))?,
    }
    .into_iter();
    contents
        .iter()
        .map(|(_, length)| match *length >= provider.digest_floor {
            true => found
                .next()
                .ok_or(HistoryError::Integrity("file digest roots")),
            false => Ok(None),
        })
        .collect()
}

fn stored(
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
    // One statement per distinct content; a repeated one reuses the answer.
    let mut known = BTreeMap::new();
    contents
        .iter()
        .map(|(digest, bytes)| {
            if let Some(root) = known.get(&(*digest, *bytes)) {
                return Ok(*root);
            }
            let rows = many(tx, OF, params![digest.to_vec(), length(*bytes)?], |row| {
                cell::<Vec<u8>>(row, 0)
            })?;
            let root = rows
                .into_iter()
                .next()
                .map(|root| {
                    <[u8; 32]>::try_from(root)
                        .map_err(|_| HistoryError::Integrity("file digest root"))
                })
                .transpose()?;
            known.insert((*digest, *bytes), root);
            Ok(root)
        })
        .collect()
}
