//! The doctor's check of the saved PostgreSQL connection, separate from the
//! audit (`run.rs`).
use super::run::with_cancellation;
use crate::diagnostic::{emit, ConnectionStep, Observation, Options, Outcome};
use crate::{
    backend::{error::BackendError, postgresql::session::driver::Driver},
    PostgresSelection,
};
use layerfs_storage::port::PersistenceError;

/// Checks the saved PostgreSQL connection without modifying the database.
/// The diagnostic async driver owns and bounds DNS/TCP/TLS/authentication.
pub fn connection(
    selection: &PostgresSelection,
    binding: &[u8],
    options: Options,
    resolver: &mut crate::diagnostic::Resolver<'_>,
    observer: &mut dyn FnMut(Observation),
) -> Result<(), PersistenceError> {
    if options.check().is_err() {
        emit(
            observer,
            "store.connection.cancelled",
            Outcome::Unknown,
            "connection diagnosis cancelled or deadline expired before admission",
        );
        return Ok(());
    }
    let mut client = match Driver::diagnostic(
        selection,
        options.deadline,
        resolver,
        options.cancelled,
    ) {
        Ok(client) => client,
        Err(error) => {
            if matches!(
                error,
                crate::backend::postgresql::session::driver::Error::Deadline { .. }
            ) || error.backend() == BackendError::Cancelled
            {
                emit(observer,"store.connection.cancelled",Outcome::Unknown,"connection admission cancelled or deadline expired; owned connect/handshake future dropped at its bounded boundary");
            }
            if let crate::backend::postgresql::session::driver::Error::Unsupported(reason) = &error
            {
                emit(
                    observer,
                    ConnectionStep::Configuration.id(),
                    Outcome::Unknown,
                    *reason,
                );
                return Ok(());
            }
            let code = error.code().map(|c| c.code());
            let step = ConnectionStep::of_refused_connection(code);
            let outcome = match step {
                ConnectionStep::Transport => Outcome::Unknown,
                _ => Outcome::Fail,
            };
            let code = code.unwrap_or("unavailable");
            emit(observer,step.id(),outcome,format!("PostgreSQL connection not established; SQLSTATE {code}; credentials and driver error text omitted"));
            return Ok(());
        }
    };
    let target = client.cancellation();
    let result = with_cancellation(options.cancelled, target, || {
        connection_queries(&mut client, selection, binding, options, observer)
    });
    if let Some(released) = client.released() {
        emit(
            observer,
            "store.connection.cancellation.release",
            if released {
                Outcome::Pass
            } else {
                Outcome::Unknown
            },
            if released {
                "server cancellation and ROLLBACK acknowledged; preflight query/snapshot released"
            } else {
                "preflight query/snapshot release was not acknowledged within bounded cleanup grace"
            },
        );
    }
    if client.close().is_err() {
        emit(
            observer,
            "store.connection.close",
            Outcome::Unknown,
            "database session close was not acknowledged",
        );
    }
    result?
}

fn connection_queries(
    client: &mut Driver,
    selection: &PostgresSelection,
    binding: &[u8],
    options: Options,
    observer: &mut dyn FnMut(Observation),
) -> Result<(), PersistenceError> {
    emit(observer,"store.connection.authentication",Outcome::Pass,"PostgreSQL driver established authenticated database session using configured TLS policy; DNS/TCP/TLS were combined by the driver");
    if options.check().is_err() {
        emit(
            observer,
            "store.connection.cancelled",
            Outcome::Unknown,
            "connection diagnosis cancelled before schema read",
        );
        return Ok(());
    }
    let result=client.query_one("SELECT COALESCE(has_schema_privilege((SELECT oid FROM pg_catalog.pg_namespace WHERE nspname=$1),'USAGE'),false), EXISTS(SELECT 1 FROM pg_catalog.pg_namespace WHERE nspname=$1)",&[&selection.schema]);
    match result {
        Ok(row) => {
            let permitted: bool = row.try_get(0).map_err(|_| PersistenceError::Malformed)?;
            let exists: bool = row.try_get(1).map_err(|_| PersistenceError::Malformed)?;
            emit(
                observer,
                "store.connection.schema",
                if exists && permitted {
                    Outcome::Pass
                } else {
                    Outcome::Fail
                },
                if exists && permitted {
                    "configured schema exists and role has USAGE"
                } else {
                    "configured schema is absent or role lacks USAGE"
                },
            );
        }
        Err(error) => emit(
            observer,
            "store.connection.schema",
            Outcome::Unknown,
            format!(
                "read-only schema query did not complete; SQLSTATE {}",
                error.code().map(|c| c.code()).unwrap_or("unavailable")
            ),
        ),
    }
    if options.check().is_err() {
        emit(
            observer,
            "store.connection.cancelled",
            Outcome::Unknown,
            "connection diagnosis interrupted before identity read",
        );
        return Ok(());
    }
    let table = format!("\"{}\".history_meta", selection.schema.replace('"', "\"\""));
    match client.query_one(
        &format!("SELECT CASE WHEN octet_length(binding_key) BETWEEN 1 AND 128 THEN binding_key END,CASE WHEN pg_typeof(catalog_incarnation)='bigint'::regtype THEN catalog_incarnation::text::bigint END,CASE WHEN octet_length(catalog_id)=32 THEN catalog_id END FROM {table} WHERE id=1"),
        &[],
    ) {
        Ok(row) => {
            let recorded: Option<Vec<u8>> = row.try_get(0).map_err(|_| PersistenceError::Malformed)?;
            let incarnation: Option<i64> = row.try_get(1).map_err(|_| PersistenceError::Malformed)?;
            let catalog: Option<Vec<u8>> = row.try_get(2).map_err(|_| PersistenceError::Malformed)?;
            let matches = recorded.as_deref() == Some(binding)
                && incarnation.is_some_and(|value| value > 0)
                && layerfs_history::CatalogId::derive(recorded.as_deref().unwrap_or_default())
                    .is_ok_and(|id| Some(id.as_slice()) == catalog.as_deref());
            emit(
                observer,
                "store.connection.identity",
                if matches {
                    Outcome::Pass
                } else {
                    Outcome::Fail
                },
                if matches {
                    "configured Store metadata is readable and binding matches the saved Store"
                } else {
                    "database schema authority metadata is malformed or does not match the saved Store"
                },
            );
        }
        Err(error) => emit(
            observer,
            "store.connection.read",
            if matches!(error.code().map(|c| c.code()), Some("42501" | "42P01")) {
                Outcome::Fail
            } else {
                Outcome::Unknown
            },
            format!(
                "configured Store metadata read did not complete; SQLSTATE {}",
                error.code().map(|c| c.code()).unwrap_or("unavailable")
            ),
        ),
    }
    if options.check().is_err() {
        emit(
            observer,
            "store.connection.cancelled",
            Outcome::Unknown,
            "connection diagnosis interrupted before negotiated TLS observation",
        );
        return Ok(());
    }
    // Establish actual negotiated TLS when the server exposes this observation.
    match client.query_one("SELECT ssl FROM pg_catalog.pg_stat_ssl WHERE pid=pg_backend_pid()",&[]){
        Ok(row)=>{let ssl:bool=row.try_get(0).map_err(|_|PersistenceError::Malformed)?;emit(observer,"store.connection.tls",Outcome::Pass,if ssl{"server reports TLS active; verification followed configured driver policy"}else{"server reports no TLS; connection followed configured driver policy"});}
        Err(_)=>emit(observer,"store.connection.tls",Outcome::Unknown,"negotiated TLS observation unavailable; successful connection alone does not establish TLS"),
    }
    Ok(())
}
