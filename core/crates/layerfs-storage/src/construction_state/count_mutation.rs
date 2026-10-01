//! Exact prevalidated count/seed changes and one known/Unknown commit.
use super::{count_index, count_state::Counts, profile};
use crate::{StorageError, StorageResult};
use rusqlite::Connection;
pub(crate) fn commit(
    connection: &Connection,
    engine: Option<&'static crate::engine::EngineGuard>,
    state: &Counts,
) -> StorageResult<()> {
    if let Some(guard) = engine {
        guard.validate()?;
    }
    crate::sqlite::write::begin_immediate(connection)?;
    let result = (|| {
        count_index::verify(connection, state)?;
        let attempt = state.attempt.as_ref().unwrap();
        for (old, new) in &attempt.rows {
            let affected = match (old, new) {
                (None, Some(new)) => connection.execute(
                    "INSERT INTO canonical_counts VALUES(?1,?2)",
                    rusqlite::params![
                        state.scope.key(new.serial())?.as_slice(),
                        new.encode_value()?.as_slice()
                    ],
                )?,
                (Some(old), Some(new)) => connection.execute(
                    "UPDATE canonical_counts SET value=?1 WHERE key=?2 AND value=?3",
                    rusqlite::params![
                        new.encode_value()?.as_slice(),
                        state.scope.key(old.serial())?.as_slice(),
                        old.encode_value()?.as_slice()
                    ],
                )?,
                (Some(old), None) => connection.execute(
                    "DELETE FROM canonical_counts WHERE key=?1 AND value=?2",
                    rusqlite::params![
                        state.scope.key(old.serial())?.as_slice(),
                        old.encode_value()?.as_slice()
                    ],
                )?,
                _ => return Err(StorageError::Integrity("count attempt empty change")),
            };
            if affected != 1 {
                return Err(StorageError::Integrity("count CAS acknowledgement"));
            }
        }
        let zeros = state.scope.zeros()?;
        for (old, new) in &attempt.seeds {
            let affected = match (old, new) {
                (None, Some(new)) => connection.execute(
                    "INSERT INTO zero_seeds VALUES(?1,?2)",
                    rusqlite::params![
                        zeros.key(new.serial)?.as_slice(),
                        new.encode_value()?.as_slice()
                    ],
                )?,
                (Some(old), None) => connection.execute(
                    "DELETE FROM zero_seeds WHERE key=?1 AND value=?2",
                    rusqlite::params![
                        zeros.key(old.serial)?.as_slice(),
                        old.encode_value()?.as_slice()
                    ],
                )?,
                _ => return Err(StorageError::Integrity("zero attempt change")),
            };
            if affected != 1 {
                return Err(StorageError::Integrity("zero acknowledgement"));
            }
        }
        for old in &attempt.deleted_rows {
            if connection.execute(
                "DELETE FROM canonical_counts WHERE key=?1 AND value=?2",
                rusqlite::params![
                    state.scope.key(old.serial())?.as_slice(),
                    old.encode_value()?.as_slice()
                ],
            )? != 1
            {
                return Err(StorageError::Integrity("count retirement acknowledgement"));
            }
        }
        for old in &attempt.deleted_seeds {
            if connection.execute(
                "DELETE FROM zero_seeds WHERE key=?1 AND value=?2",
                rusqlite::params![
                    zeros.key(old.serial)?.as_slice(),
                    old.encode_value()?.as_slice()
                ],
            )? != 1
            {
                return Err(StorageError::Integrity("zero retirement acknowledgement"));
            }
        }
        count_index::write_owner(connection, &state.scope, &attempt.after)?;
        if attempt.after.stage == 6
            && (!count_index::empty(connection, "canonical_counts")?
                || !count_index::empty(connection, "zero_seeds")?)
        {
            return Err(StorageError::Integrity("count retirement final EOF"));
        }
        Ok(())
    })();
    profile::finish_write_guarded(connection, result, engine)
}
