//! Compare untrusted schema claims inside SQLite before extracting their bytes.
use super::super::{
    schema,
    session::{connection::SqlWork, query, rows, transaction::Transaction},
};
use crate::backend::error::BackendError;
use crate::backend::seam::{Param, Record};
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
    let shipped = schema_rows(&trusted, &work)?;
    let shipped_definition = definition(&shipped)?;
    // A Store the additive position-column migrations brought forward records
    // the definition SQLite rewrote: the appended columns end the column list
    // instead of sitting where the shipped schema declares them. The doctor
    // admits that rewritten schema as well as the shipped one (REBUILD.md kind
    // 2), and rejects every other difference.
    for (table, column, _) in schema::POSITION_COLUMNS {
        trusted
            .execute_batch(&format!("ALTER TABLE {table} DROP COLUMN {column}"))
            .map_err(rows::error)?;
    }
    for (_, _, alter) in schema::POSITION_COLUMNS {
        trusted.execute_batch(alter).map_err(rows::error)?;
    }
    let migrated = schema_rows(&trusted, &work)?;
    let migrated_definition = definition(&migrated)?;
    // A Store `store upgrade` brought from schema 2 (#804) has `attempt`
    // appended after every other `commit` column, after the position columns
    // too when the Store had been position-migrated: admitted the same way.
    let upgraded = with_attempt_appended(&shipped_scripts(selection)?, &work)?;
    let upgraded_definition = definition(&upgraded)?;
    reappend_attempt(&trusted)?;
    let migrated_upgraded = schema_rows(&trusted, &work)?;
    let migrated_upgraded_definition = definition(&migrated_upgraded)?;
    let candidates = [&shipped, &migrated, &upgraded, &migrated_upgraded];
    let claims = tx.query(
        "SELECT count(*) FROM history_meta WHERE id=1 AND typeof(catalog_id)='blob' AND length(catalog_id)=32 AND typeof(catalog_incarnation)='integer' AND typeof(identity_format)='integer' AND typeof(schema_version)='integer' AND typeof(binding_key)='blob' AND length(binding_key) BETWEEN 1 AND 128 AND typeof(schema_source)='text' AND schema_source=?1 AND typeof(schema_definition)='text'",
        vec![Param::Text(schema::source(selection))],
    )?;
    if claims
        .first()
        .ok_or(BackendError::Integrity)?
        .get::<i64>(0)?
        != 1
    {
        return Err(BackendError::Integrity);
    }
    let recorded = tx.query(
        "SELECT schema_source,schema_definition FROM history_meta WHERE id=1",
        vec![],
    )?;
    let recorded = recorded
        .first()
        .filter(|_| recorded.len() == 1)
        .ok_or(BackendError::Integrity)?;
    let recorded_definition: String = recorded.get(1)?;
    if ![
        &shipped_definition,
        &migrated_definition,
        &upgraded_definition,
        &migrated_upgraded_definition,
    ]
    .iter()
    .any(|candidate| same_schema(&recorded_definition, candidate))
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
        != shipped.len() as i64
    {
        return Err(BackendError::Integrity);
    }
    for (index, row) in shipped.iter().enumerate() {
        let kind: String = row.get(0)?;
        let name: String = row.get(1)?;
        let live = tx.query(
            "SELECT sql FROM sqlite_schema WHERE type=?1 AND name=?2",
            vec![Param::Text(kind), Param::Text(name)],
        )?;
        let live: String = live.first().ok_or(BackendError::Integrity)?.get(0)?;
        let mut admitted = false;
        for candidate in candidates {
            let sql: String = candidate[index].get(2)?;
            admitted |= same_schema(&live, &sql);
        }
        if !admitted {
            return Err(BackendError::Integrity);
        }
    }
    Ok(())
}

/// A scratch database holding the shipped scripts.
fn shipped_scripts(
    selection: (crate::SqlitePackLayout, crate::SqliteAcquisitionSchema),
) -> Result<rusqlite::Connection, BackendError> {
    let scratch = rusqlite::Connection::open_in_memory().map_err(rows::error)?;
    for script in schema::scripts(selection) {
        scratch.execute_batch(&script).map_err(rows::error)?;
    }
    Ok(scratch)
}

/// `scratch`'s rows once `attempt` is appended as `store upgrade` appends it.
fn with_attempt_appended(
    scratch: &rusqlite::Connection,
    work: &RefCell<SqlWork>,
) -> Result<Vec<Record>, BackendError> {
    reappend_attempt(scratch)?;
    schema_rows(scratch, work)
}

/// Drops `commit.attempt` (and its index) and adds both back, so the column
/// ends the column list as `ALTER TABLE ... ADD COLUMN` writes it.
fn reappend_attempt(scratch: &rusqlite::Connection) -> Result<(), BackendError> {
    let index =
        "CREATE UNIQUE INDEX commit_attempt ON \"commit\"(attempt) WHERE attempt IS NOT NULL";
    scratch
        .execute_batch(&format!(
            "DROP INDEX commit_attempt; ALTER TABLE \"commit\" DROP COLUMN attempt; \
             ALTER TABLE \"commit\" ADD COLUMN attempt BLOB CHECK(attempt IS NULL OR length(attempt)=16); \
             {index}"
        ))
        .map_err(rows::error)
}

/// The schema rows the shipped scripts create, ordered as recorded.
fn schema_rows(
    connection: &rusqlite::Connection,
    work: &RefCell<SqlWork>,
) -> Result<Vec<Record>, BackendError> {
    query::run(
        connection,
        "SELECT type,name,sql FROM sqlite_schema WHERE substr(name,1,7)!='sqlite_' ORDER BY type,name",
        vec![],
        work,
    )
}

/// The recorded definition text for one row set.
fn definition(rows: &[Record]) -> Result<String, BackendError> {
    let mut definition = String::new();
    for row in rows {
        for index in 0..3 {
            definition.push_str(&row.get::<String>(index)?);
            definition.push('\n');
        }
    }
    Ok(definition)
}

/// Whether two schema texts carry the same SQL. SQLite rewrites a table's
/// stored definition when a column is appended, moving only whitespace and a
/// separator; every token must still match, so no other schema is admitted.
pub(crate) fn same_schema(left: &str, right: &str) -> bool {
    tokens(left) == tokens(right)
}

/// The SQL tokens of a schema text, with whitespace excluded. A quoted
/// identifier, string or blob stays one token, as does a run of identifier
/// characters; every other byte is one token.
fn tokens(sql: &str) -> Vec<&str> {
    let bytes = sql.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let start = index;
        match bytes[index] {
            byte if byte.is_ascii_whitespace() => {
                index += 1;
                continue;
            }
            quote @ (b'\'' | b'"' | b'`') => {
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == quote {
                        index += 1;
                        if index < bytes.len() && bytes[index] == quote {
                            index += 1;
                            continue;
                        }
                        break;
                    }
                    index += 1;
                }
            }
            b'[' => {
                index += 1;
                while index < bytes.len() && bytes[index] != b']' {
                    index += 1;
                }
                index = (index + 1).min(bytes.len());
            }
            byte if byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$' => {
                index += 1;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric()
                        || bytes[index] == b'_'
                        || bytes[index] == b'$')
                {
                    index += 1;
                }
            }
            _ => index += 1,
        }
        tokens.push(&sql[start..index]);
    }
    tokens
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
