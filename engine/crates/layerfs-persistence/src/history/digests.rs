//! Saved file roots by the digest and length of their bytes (#364).
use super::params;
use super::rows::{cell, many, sql};
use super::transaction::Transaction;
use layerfs_history::error::{HistoryError, HistoryResult};
use layerfs_history::records::{FileDigest, FILE_DIGEST_WINDOW};

const INSERT: &str = "digest_insert";
const OF: &str = "digest_of";

fn length(value: u64) -> HistoryResult<i64> {
    i64::try_from(value).map_err(|_| HistoryError::Integrity("file digest length"))
}

/// Records the root built for each content; a recorded content takes the
/// newer root, so a row whose root the Store lost is replaced.
pub(crate) fn record(tx: &Transaction<'_>, rows: &[FileDigest]) -> HistoryResult<()> {
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
