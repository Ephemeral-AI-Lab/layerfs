//! What a role may do in a Store's schema, for `store.attach`'s check of a
//! machine's connection: the server's version, one round trip's latency,
//! the role, and each privilege a Sandbox's writes need that it lacks. One
//! connection opened for the call and closed after it, read-only queries
//! only, no retry.
use crate::backend::postgresql::session::connection::Connector;
use crate::backend::postgresql::status::PostgresStatusError;
use crate::PostgresSelection;
use std::time::{Duration, Instant};

/// What the server answered for a role and a schema.
#[derive(Debug)]
pub struct PostgresAccess {
    /// `SHOW server_version`.
    pub server_version: String,
    /// The `SHOW server_version` round trip.
    pub latency: Duration,
    /// `current_user`.
    pub role: String,
    /// Whether the database has the schema.
    pub schema: bool,
    /// Each privilege the role lacks, as `<privilege> on <object>`, in a
    /// fixed order: the schema's `USAGE`, the database's `TEMP`, then each
    /// table's `SELECT`, `INSERT`, `UPDATE`, `DELETE` and each sequence's
    /// `USAGE`. Empty when the role has them all (or there is no schema).
    pub missing: Vec<String>,
}

/// The role, the schema's `USAGE` (NULL without the schema) and the
/// database's `TEMP` (a merge stages in temporary tables).
const ROLE: &str = "SELECT current_user::text, \
     CASE WHEN EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = $1::text) \
     THEN has_schema_privilege($1::text, 'USAGE') END, \
     has_database_privilege(current_database(), 'TEMP')";

/// Each table and sequence privilege of the schema the role lacks.
const MISSING: &str = "SELECT p.privilege || ' on ' || $1::text || '.' || c.relname \
     FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
     CROSS JOIN LATERAL unnest(CASE WHEN c.relkind = 'S' THEN ARRAY['USAGE'] \
     ELSE ARRAY['SELECT', 'INSERT', 'UPDATE', 'DELETE'] END) \
     WITH ORDINALITY AS p(privilege, at) \
     WHERE n.nspname = $1::text AND c.relkind IN ('r', 'p', 'S') \
     AND NOT CASE WHEN c.relkind = 'S' THEN has_sequence_privilege(c.oid, p.privilege) \
     ELSE has_table_privilege(c.oid, p.privilege) END \
     ORDER BY c.relkind DESC, c.relname, p.at";

/// Reads what the role `url` logs in as may do in `schema`.
pub fn access(url: &str, schema: &str) -> Result<PostgresAccess, PostgresStatusError> {
    let connector = Connector::new(&PostgresSelection::new(url, "public"))
        .map_err(|_| PostgresStatusError::Unreachable("the URL does not parse".to_owned()))?;
    let mut client = connector
        .connect_raw()
        .map_err(|error| match error.code() {
            Some(code) if code.code().starts_with("28") || code.code() == "3D000" => {
                PostgresStatusError::Denied(error.to_string())
            }
            _ => PostgresStatusError::Unreachable(error.to_string()),
        })?;
    let failed = |error: postgres::Error| PostgresStatusError::Failed(error.to_string());
    let started = Instant::now();
    let server_version: String = client
        .query_one("SHOW server_version", &[])
        .map_err(failed)?
        .try_get(0)
        .map_err(failed)?;
    let latency = started.elapsed();
    let row = client.query_one(ROLE, &[&schema]).map_err(failed)?;
    let role: String = row.try_get(0).map_err(failed)?;
    let usage: Option<bool> = row.try_get(1).map_err(failed)?;
    let temp: bool = row.try_get(2).map_err(failed)?;
    let mut missing = Vec::new();
    if usage == Some(false) {
        missing.push(format!("USAGE on schema {schema}"));
    }
    if !temp {
        missing.push("TEMP on the database".to_owned());
    }
    if usage == Some(true) {
        for row in client.query(MISSING, &[&schema]).map_err(failed)? {
            missing.push(row.try_get(0).map_err(failed)?);
        }
    }
    let _ = client.close();
    Ok(PostgresAccess {
        server_version,
        latency,
        role,
        schema: usage.is_some(),
        missing,
    })
}
