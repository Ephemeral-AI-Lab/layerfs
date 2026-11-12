//! Adding and dropping a Store's change log: the `change_state` and
//! `change_log` tables and one logging trigger per logged table and write
//! kind. They exist only while the Store is an export base, and the Store
//! records its schema with them, so it opens and passes Doctor either way. A
//! Store that was never marked keeps the schema of a Store without this module.
use super::ChangeError;
use crate::backend::sqlite::schema;
use rusqlite::{params, Connection};

/// Adds the change log to the Store open in `c`'s write transaction and
/// records the schema with it. Refuses a Store whose recorded schema is not
/// this build's: an older Store gains its missing tables when it is first
/// opened writable.
pub(super) fn add(c: &Connection) -> Result<(), ChangeError> {
    let selection = recorded(c, false)?;
    c.execute_batch(&schema::changes(selection.0))?;
    record(c, &schema::recorded_source(selection, true))
}

/// Drops every logging trigger and the change log from the Store open in
/// `c`'s write transaction and records the schema without them.
pub(super) fn remove(c: &Connection) -> Result<(), ChangeError> {
    let selection = recorded(c, true)?;
    // The triggers go first: each one reads `change_state`, and `record`
    // below updates `history_meta`, whose trigger would meet a dropped table.
    for name in schema::changes(selection.0)
        .lines()
        .filter_map(|line| line.strip_prefix("CREATE TRIGGER "))
        .filter_map(|rest| rest.split_whitespace().next())
    {
        c.execute_batch(&format!("DROP TRIGGER {name}"))?;
    }
    c.execute_batch("DROP TABLE change_log; DROP TABLE change_state;")?;
    record(c, &schema::source(selection))
}

/// The Store's layout and acquisition schema, when its recorded schema is
/// this build's with (`logged`) or without the change log and matches the
/// Store's own definition.
fn recorded(c: &Connection, logged: bool) -> Result<schema::Selection, ChangeError> {
    let version: i64 = c.query_row("PRAGMA main.user_version", [], |r| r.get(0))?;
    let selection =
        schema::selection(version).ok_or(ChangeError::Integrity("unknown Store schema version"))?;
    let (source, recorded): (String, String) = c.query_row(
        "SELECT schema_source,schema_definition FROM main.history_meta WHERE id=1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if source != schema::recorded_source(selection, logged) || recorded != definition(c)? {
        return Err(ChangeError::Integrity(
            "the Store's recorded schema is not this build's; open it writable once to migrate it",
        ));
    }
    Ok(selection)
}

/// Records `source` and the Store's current definition as its schema.
fn record(c: &Connection, source: &str) -> Result<(), ChangeError> {
    let definition = definition(c)?;
    c.execute(
        "UPDATE main.history_meta SET schema_source=?1,schema_definition=?2 WHERE id=1",
        params![source, definition],
    )?;
    Ok(())
}

/// The Store's schema definition, as open validates it.
fn definition(c: &Connection) -> Result<String, ChangeError> {
    let work = std::cell::RefCell::new(crate::SqlWork::default());
    schema::definition(c, &work).map_err(|error| ChangeError::Store(error.into()))
}
