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
        "SELECT count(*) FROM history_meta WHERE id=1 AND typeof(catalog_id)='blob' AND length(catalog_id)=32 AND typeof(catalog_incarnation)='integer' AND typeof(identity_format)='integer' AND typeof(schema_version)='integer' AND typeof(binding_key)='blob' AND length(binding_key) BETWEEN 1 AND 128 AND typeof(schema_source)='text' AND schema_source=?1 AND typeof(schema_definition)='text' AND schema_definition=?2",
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
    let policy = tx.query("SELECT count(*) FROM store_policy WHERE id=1 AND typeof(small_file_threshold_bytes)='integer'", vec![])?;
    if policy
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

/// Derive storage-class checks from trusted declarations. SQLite affinity does
/// not forbid a corrupt integer cell from holding arbitrarily large text.
/// Scalar predicates reject that state before generic row conversion copies it.
pub(crate) fn type_checks(
    layout: crate::SqlitePackLayout,
    acquisition: crate::SqliteAcquisitionSchema,
) -> Result<Vec<(String, String)>, BackendError> {
    let trusted = rusqlite::Connection::open_in_memory().map_err(rows::error)?;
    for script in schema::scripts((layout, acquisition)) {
        trusted.execute_batch(&script).map_err(rows::error)?;
    }
    let work = RefCell::new(SqlWork::default());
    let tables = query::run(&trusted, "SELECT name FROM sqlite_schema WHERE type='table' AND substr(name,1,7)!='sqlite_' ORDER BY name", vec![], &work)?;
    let quote = |name: &str| format!("\"{}\"", name.replace('"', "\"\""));
    let mut result = Vec::new();
    for table in tables {
        let table: String = table.get(0)?;
        let columns = query::run(
            &trusted,
            &format!("PRAGMA table_info({})", quote(&table)),
            vec![],
            &work,
        )?;
        let mut predicates = Vec::new();
        for column in columns {
            let name = quote(&column.get::<String>(1)?);
            let kind = match column.get::<String>(2)?.as_str() {
                "INTEGER" => "integer",
                "TEXT" => "text",
                "BLOB" => "blob",
                // A STRICT table's `ANY` column (the change log's keys)
                // declares no storage class to check.
                "ANY" => continue,
                _ => return Err(BackendError::Integrity),
            };
            let non_null: i64 = column.get(3)?;
            predicates.push(if non_null != 0 {
                format!("typeof({name})!='{kind}'")
            } else {
                format!("({name} IS NOT NULL AND typeof({name})!='{kind}')")
            });
        }
        result.push((
            table.clone(),
            format!(
                "SELECT EXISTS(SELECT 1 FROM {} WHERE {} LIMIT 1)",
                quote(&table),
                predicates.join(" OR ")
            ),
        ));
    }
    if acquisition == crate::SqliteAcquisitionSchema::Tables {
        result.push(("sqlite_sequence".to_owned(), "SELECT EXISTS(SELECT 1 FROM sqlite_sequence WHERE typeof(name)!='text' OR typeof(seq)!='integer' LIMIT 1)".to_owned()));
    }
    Ok(result)
}
