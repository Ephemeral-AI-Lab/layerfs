//! A PostgreSQL server's status for a home's connection page (#506 E5/E7):
//! its version, one round trip's latency and the size of each named schema
//! (tables and materialized views with their indexes and TOAST), on one
//! connection opened for the call and closed after it, with no retry.
use crate::backend::postgresql::session::connection::Connector;
use crate::PostgresSelection;
use std::time::{Duration, Instant};

/// What a server answered.
#[derive(Debug)]
pub struct PostgresStatus {
    /// `SHOW server_version`.
    pub server_version: String,
    /// The `SHOW server_version` round trip.
    pub latency: Duration,
    /// Each schema asked for, with its size in bytes; `None` when the
    /// server has no such schema.
    pub schemas: Vec<(String, Option<u64>)>,
}

/// Why a status read did not complete.
#[derive(Debug)]
pub enum PostgresStatusError {
    /// The server was not reached, or the connection failed before a query.
    Unreachable(String),
    /// The server refused the credentials or the database.
    Denied(String),
    /// A query failed after the connection was made.
    Failed(String),
}

impl std::fmt::Display for PostgresStatusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreachable(why) => write!(f, "PostgreSQL server unreachable: {why}"),
            Self::Denied(why) => write!(f, "PostgreSQL server refused the connection: {why}"),
            Self::Failed(why) => write!(f, "PostgreSQL status query failed: {why}"),
        }
    }
}

const SIZES: &str = "SELECT s.name, CASE WHEN n.oid IS NULL THEN NULL ELSE \
     COALESCE(sum(pg_total_relation_size(c.oid)), 0)::bigint END \
     FROM unnest($1::text[]) AS s(name) \
     LEFT JOIN pg_namespace n ON n.nspname = s.name \
     LEFT JOIN pg_class c ON c.relnamespace = n.oid AND c.relkind IN ('r', 'm') \
     GROUP BY s.name, n.oid ORDER BY s.name";

/// Reads the status of the server at `url` and the sizes of `schemas`.
pub fn status(url: &str, schemas: &[String]) -> Result<PostgresStatus, PostgresStatusError> {
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
    let names: Vec<String> = schemas.to_vec();
    let mut sizes = Vec::new();
    for row in client.query(SIZES, &[&names]).map_err(failed)? {
        let name: String = row.try_get(0).map_err(failed)?;
        let bytes: Option<i64> = row.try_get(1).map_err(failed)?;
        sizes.push((name, bytes.map(|b| u64::try_from(b).unwrap_or(0))));
    }
    let _ = client.close();
    Ok(PostgresStatus {
        server_version,
        latency,
        schemas: sizes,
    })
}
