//! Bounded prevalidated fact/parent changes and exact selected owner acknowledgements.
use super::fact_state::Facts;
use super::{fact_index, profile};
use crate::{StorageError, StorageResult};
use rusqlite::Connection;

pub(crate) fn transaction<T>(
    connection: &Connection,
    engine: Option<&'static crate::engine::EngineGuard>,
    state: &mut Facts,
    body: impl FnOnce(&Connection, &mut Facts) -> StorageResult<T>,
) -> StorageResult<T> {
    if let Some(guard) = engine {
        guard.validate()?;
    }
    crate::sqlite::write::begin_immediate(connection)?;
    let result = (|| {
        fact_index::verify(connection, state)?;
        let value = body(connection, state)?;
        let attempt = state.attempt.as_ref().unwrap();
        if attempt.table == 17 {
            for fact in &attempt.bases {
                let scope = attempt.after.scope.as_ref().unwrap();
                let sql = if attempt.kind == "base_retire" {
                    "DELETE FROM base_facts WHERE key=?1 AND value=?2"
                } else {
                    "INSERT INTO base_facts VALUES(?1,?2)"
                };
                if connection.execute(
                    sql,
                    rusqlite::params![
                        scope.key(fact.serial)?.as_slice(),
                        fact.encode_value()?.as_slice()
                    ],
                )? != 1
                {
                    return Err(StorageError::Integrity(
                        "base fact insertion acknowledgement",
                    ));
                }
            }
        } else {
            let scope = attempt.after.scope.as_ref().unwrap();
            for (old, new) in &attempt.parents {
                let affected = match (old, new) {
                    (None, Some(new)) => connection.execute(
                        "INSERT INTO parent_eligibility VALUES(?1,?2)",
                        rusqlite::params![scope.key(new.serial)?.as_slice(), u8::from(new.bound)],
                    )?,
                    (Some(old), Some(new)) => connection.execute(
                        "UPDATE parent_eligibility SET bound=?1 WHERE key=?2 AND bound=?3",
                        rusqlite::params![
                            u8::from(new.bound),
                            scope.key(old.serial)?.as_slice(),
                            u8::from(old.bound)
                        ],
                    )?,
                    (Some(old), None) => connection.execute(
                        "DELETE FROM parent_eligibility WHERE key=?1 AND bound=?2",
                        rusqlite::params![scope.key(old.serial)?.as_slice(), u8::from(old.bound)],
                    )?,
                    (None, None) => {
                        return Err(StorageError::Integrity("empty parent fact transition"))
                    }
                };
                if affected != 1 {
                    return Err(StorageError::Integrity("parent fact acknowledgement"));
                }
            }
        }
        fact_index::write_owner(connection, attempt.table, &attempt.after)?;
        Ok(value)
    })();
    profile::finish_transaction_guarded(connection, result, engine)
}
