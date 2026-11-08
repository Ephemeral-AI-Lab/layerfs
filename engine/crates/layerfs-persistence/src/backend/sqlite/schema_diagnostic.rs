//! Compare untrusted schema claims inside SQLite before extracting their bytes.
use super::{
    schema,
    session::{connection::SqlWork, query, rows, transaction::Transaction},
};
use crate::backend::records::{BackendError, Param};
use std::cell::RefCell;

/// The scratch database contains only the finite shipped schema, never Store
/// rows. Its schema output therefore has a format-derived bound. The original
/// Store is queried only for scalar comparisons within its pinned transaction.
pub(crate) fn check(
    tx: &Transaction<'_>,
    acquisition: crate::SqliteAcquisitionSchema,
) -> Result<(), BackendError> {
    let selection = (tx.layout(), acquisition);
    let trusted = rusqlite::Connection::open_in_memory().map_err(rows::error)?;
    for script in schema::scripts(selection) {
        trusted.execute_batch(&script).map_err(rows::error)?;
    }
    let work = RefCell::new(SqlWork::default());
    let expected = query::run(&trusted,
        "SELECT type,name,sql FROM sqlite_schema WHERE substr(name,1,7)!='sqlite_' ORDER BY type,name",
        vec![], &work)?;
    let mut definition = String::new();
    for row in &expected {
        for index in 0..3 {
            definition.push_str(&row.get::<String>(index)?);
            definition.push('\n');
        }
    }
    let claims = tx.query(
        "SELECT count(*) FROM history_meta WHERE id=1 AND typeof(catalog_id)='blob' AND length(catalog_id)=32 AND typeof(binding_key)='blob' AND length(binding_key) BETWEEN 1 AND 128 AND typeof(schema_source)='text' AND schema_source=?1 AND typeof(schema_definition)='text' AND schema_definition=?2",
        vec![Param::Text(schema::source(selection)), Param::Text(definition)],
    )?;
    if claims
        .first()
        .ok_or(BackendError::Integrity)?
        .get::<i64>(0)?
        != 1
    {
        return Err(BackendError::Integrity);
    }
    let count = tx.query(
        "SELECT count(*) FROM sqlite_schema WHERE substr(name,1,7)!='sqlite_'",
        vec![],
    )?;
    if count
        .first()
        .ok_or(BackendError::Integrity)?
        .get::<i64>(0)?
        != expected.len() as i64
    {
        return Err(BackendError::Integrity);
    }
    for row in expected {
        let matched = tx.query(
            "SELECT count(*) FROM sqlite_schema WHERE type=?1 AND name=?2 AND sql=?3",
            (0..3)
                .map(|index| row.get::<String>(index).map(Param::Text))
                .collect::<Result<_, _>>()?,
        )?;
        if matched
            .first()
            .ok_or(BackendError::Integrity)?
            .get::<i64>(0)?
            != 1
        {
            return Err(BackendError::Integrity);
        }
    }
    Ok(())
}
