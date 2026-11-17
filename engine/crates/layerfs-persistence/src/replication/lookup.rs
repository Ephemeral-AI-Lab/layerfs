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

/// The recorded change state; `None` when the Store is not an export base,
/// so it has no change log.
pub(super) fn state_of(c: &Connection, schema: &str) -> Result<Option<ChangeCursor>, ChangeError> {
    if !table_exists(c, schema, "change_state")? {
        return Ok(None);
    }
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

/// Checks the cursor `since` against the Store in `main` and returns its
/// recorded base and last change sequence. The Store keeps no record of the
/// states it exported, so it checks the cursor's state only at the base, and
/// its sequence above it.
pub(super) fn range(
    c: &Connection,
    since: ChangeCursor,
) -> Result<(ChangeCursor, u64), ChangeError> {
    let base = state_of(c, "main")?.ok_or(ChangeError::NotBase)?;
    if since.seq < base.seq || (since.seq == base.seq && since.state != base.state) {
        return Err(ChangeError::Cursor(
            "the cursor precedes or differs from the Store's recorded state",
        ));
    }
    let last = last_seq(c, "main")?.unwrap_or(0).max(base.seq);
    if since.seq > last {
        return Err(ChangeError::Cursor("the cursor is ahead of the Store"));
    }
    Ok((base, last))
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
