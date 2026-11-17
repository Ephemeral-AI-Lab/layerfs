//! What a role may do in a Store's schema, for `store.attach`'s check of a
//! machine's connection and for a PostgreSQL compaction's preflight: the
//! server's version, one round trip's latency, the role, and each privilege
//! the Store's writes and compaction need that it lacks. One connection
//! opened for the call and closed after it, read-only queries only, no retry.
use crate::backend::postgresql::session::connection::{Connector, Session};
use crate::backend::postgresql::status::PostgresStatusError;
use crate::backend::records::{BackendError, Param};
use crate::PostgresSelection;
use layerfs_history::error::HistoryResult;
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
    /// fixed order: the schema's `USAGE` and `CREATE`, the database's
    /// `CREATE` and `TEMP`, `pg_read_all_stats`, then each table's `SELECT`,
    /// `INSERT`, `UPDATE`, `DELETE` and each sequence's `USAGE`. Empty when
    /// the role has them all (or there is no schema).
    pub missing: Vec<String>,
}

/// The role, the schema's `USAGE` and `CREATE` (NULL without the schema), the
/// database's `CREATE` (a compaction creates the `lfs_gc_<hex>` scratch
/// schema) and `TEMP` (a merge stages in temporary tables, and a compaction
/// reads its visited set from them), and `pg_read_all_stats` (a compaction
/// reads another backend's `pg_stat_activity` row).
const ROLE: &str = "SELECT current_user::text, \
     CASE WHEN EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = $1::text) \
     THEN has_schema_privilege($1::text, 'USAGE') END, \
     CASE WHEN EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = $1::text) \
     THEN has_schema_privilege($1::text, 'CREATE') END, \
     has_database_privilege(current_database(), 'CREATE'), \
     has_database_privilege(current_database(), 'TEMP'), \
     pg_has_role('pg_read_all_stats', 'MEMBER')";

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

/// The Store-wide privileges a compaction needs before it names or creates
/// its scratch schema, in the order [`compaction_missing`] reports a missing
/// one: the schema's `USAGE` and `CREATE`, the database's `CREATE` and `TEMP`,
/// `pg_read_all_stats`, and ownership of every Store table. `CREATE` on the
/// schema is the `ALTER TABLE ... SET SCHEMA` of Phase B; `CREATE` on the
/// database is the scratch schema itself; `pg_read_all_stats` is the recorded
/// owner's `pg_stat_activity` row and its `pg_terminate_backend` proof; the
/// tables' owner is what lets Phase B `DROP TABLE` and `ALTER TABLE ... SET
/// SCHEMA` them, and an owner has every privilege with grant option, so the
/// column checks both `pg_class.relowner` membership and the ACL form.
const COMPACTION: &str = "SELECT \
     has_schema_privilege($1::text, 'USAGE')::int, \
     has_schema_privilege($1::text, 'CREATE')::int, \
     has_database_privilege(current_database(), 'CREATE')::int, \
     has_database_privilege(current_database(), 'TEMP')::int, \
     pg_has_role('pg_read_all_stats', 'MEMBER')::int, \
     (SELECT COALESCE(bool_and( \
          pg_has_role(c.relowner, 'MEMBER') \
          AND has_table_privilege(c.oid, 'SELECT WITH GRANT OPTION') \
          AND has_table_privilege(c.oid, 'INSERT WITH GRANT OPTION') \
          AND has_table_privilege(c.oid, 'UPDATE WITH GRANT OPTION') \
          AND has_table_privilege(c.oid, 'DELETE WITH GRANT OPTION') \
          AND has_table_privilege(c.oid, 'TRUNCATE WITH GRANT OPTION') \
          AND has_table_privilege(c.oid, 'REFERENCES WITH GRANT OPTION') \
          AND has_table_privilege(c.oid, 'TRIGGER WITH GRANT OPTION')), TRUE)::int \
        FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
        WHERE n.nspname = $1::text AND c.relkind IN ('r', 'p'))";

/// The privilege named for each [`COMPACTION`] column, in its order.
const COMPACTION_NAMES: [&str; 6] = [
    "USAGE on the Store schema",
    "CREATE on the Store schema",
    "CREATE on the database",
    "TEMP on the database",
    "pg_read_all_stats",
    "ownership of the Store's tables",
];

/// The first Store-wide privilege `session`'s role lacks for a PostgreSQL
/// compaction, or `None` when it has them all: one read-only round trip,
/// taken before the mark and before the `lfs_gc_<hex>` scratch schema is
/// named or created. The caller reports the returned name as `Unsupported`
/// (AC-44).
pub(crate) fn compaction_missing(session: &Session) -> HistoryResult<Option<&'static str>> {
    session.run(false, |tx| {
        let rows = tx.query(COMPACTION, vec![Param::Text(tx.schema.to_owned())])?;
        let Some(row) = rows.first() else {
            return Err(BackendError::Integrity.into());
        };
        for (at, privilege) in COMPACTION_NAMES.iter().enumerate() {
            if row.get::<i64>(at)? == 0 {
                return Ok(Some(*privilege));
            }
        }
        Ok(None)
    })
}

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
    let create_schema: Option<bool> = row.try_get(2).map_err(failed)?;
    let create_database: bool = row.try_get(3).map_err(failed)?;
    let temp: bool = row.try_get(4).map_err(failed)?;
    let read_all_stats: bool = row.try_get(5).map_err(failed)?;
    let mut missing = Vec::new();
    if usage == Some(false) {
        missing.push(format!("USAGE on schema {schema}"));
    }
    if create_schema == Some(false) {
        missing.push(format!("CREATE on schema {schema}"));
    }
    if !create_database {
        missing.push("CREATE on the database".to_owned());
    }
    if !temp {
        missing.push("TEMP on the database".to_owned());
    }
    if !read_all_stats {
        missing.push("pg_read_all_stats".to_owned());
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
