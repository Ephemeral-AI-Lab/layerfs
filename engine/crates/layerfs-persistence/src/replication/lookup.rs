//! Reads of one attached schema: `main` (the Store) or `b` (a bundle).
use super::{ChangeCursor, ChangeError};
use rusqlite::{Connection, OptionalExtension};

pub(super) fn table_exists(c: &Connection, schema: &str, name: &str) -> Result<bool, ChangeError> {
    Ok(c.query_row(
        &format!("SELECT 1 FROM {schema}.sqlite_schema WHERE type='table' AND name=?1"),
        [name],
        |_| Ok(()),
    )
    .optional()?
    .is_some())
}

pub(super) fn state_of(c: &Connection, schema: &str) -> Result<Option<ChangeCursor>, ChangeError> {
    let row: Option<(Vec<u8>, i64)> = c
        .query_row(
            &format!("SELECT state,seq FROM {schema}.change_state WHERE id=1"),
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    row.map(|(state, seq)| {
        Ok(ChangeCursor {
            state: state
                .try_into()
                .map_err(|_| ChangeError::Integrity("change state length"))?,
            seq: u64::try_from(seq).map_err(|_| ChangeError::Integrity("change state sequence"))?,
        })
    })
    .transpose()
}

pub(super) fn catalog_of(c: &Connection, schema: &str) -> Result<Vec<u8>, ChangeError> {
    Ok(c.query_row(
        &format!("SELECT catalog_id FROM {schema}.history_meta WHERE id=1"),
        [],
        |r| r.get(0),
    )?)
}

pub(super) fn last_seq(c: &Connection, schema: &str) -> Result<Option<u64>, ChangeError> {
    let seq: Option<i64> = c.query_row(
        &format!("SELECT max(seq) FROM {schema}.change_log"),
        [],
        |r| r.get(0),
    )?;
    seq.map(|s| u64::try_from(s).map_err(|_| ChangeError::Integrity("change log sequence")))
        .transpose()
}
