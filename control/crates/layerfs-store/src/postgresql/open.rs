//! PostgreSQL Stores (#506): opening one for Store calls and attaching a
//! LayerFS schema.
//!
//! A PostgreSQL Store is one schema; its URL, which may hold a password, is
//! passed per call (`postgresql: { url }`) and never stored. Nothing
//! converts a SQLite Store into one (#533 step 10c): a project reaches
//! PostgreSQL only by an Init into it. The PostgreSQL engine is always
//! built beside SQLite (#726).
use crate::identity as stores;
use crate::identity::{Identity, Kind};
use crate::open_args::{OpenArgs, PgSelection};
use layerfs_bridge::control::SdkRefusal;
use layerfs_home::outcome::{refused, Outcome};
use layerfs_store_service::{OpenRefusal, StoreService};
use std::path::Path;

/// The `synchronous_commit` a Store connected without one uses (the
/// default since 2026-11-17, #732; `store.connect`'s row).
pub const DEFAULT_SYNCHRONOUS_COMMIT: &str = "local";

/// `synchronous_commit` as the call or record names it: off, local (the
/// default), remote_write, on or remote_apply
/// ([`crate::postgresql::connect::synchronous_commit`]). A record keeps the
/// value it was connected with.
pub fn setting_of(given: Option<&str>) -> Result<String, SdkRefusal> {
    let given = given.unwrap_or(DEFAULT_SYNCHRONOUS_COMMIT);
    crate::postgresql::connect::synchronous_commit(given)?;
    Ok(given.to_owned())
}

/// The schema a call names: `[a-z_][a-z0-9_]*`, at most 63 bytes.
pub fn schema_of(schema: Option<&str>) -> Result<String, SdkRefusal> {
    let schema = schema.ok_or_else(|| refused("InvalidInput", "postgresql.schema is required"))?;
    let valid = !schema.is_empty()
        && schema.len() <= 63
        && schema
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_lowercase() || b == b'_')
        && schema
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    if !valid {
        return Err(refused(
            "InvalidInput",
            format!("a PostgreSQL schema is [a-z_][a-z0-9_]*, at most 63 bytes, not {schema:?}"),
        ));
    }
    Ok(schema.to_owned())
}

/// The URL a call names, required.
pub fn url_of(url: Option<&str>) -> Result<&str, SdkRefusal> {
    url.ok_or_else(|| {
        refused(
            "InvalidInput",
            "postgresql.url is required (passed per call, never stored)",
        )
    })
}

pub fn selection(url: &str, schema: &str, sync: &str) -> layerfs_persistence::PostgresSelection {
    use layerfs_persistence::SynchronousCommit as S;
    let setting = match sync {
        "off" => S::Off,
        "remote_write" => S::RemoteWrite,
        "on" => S::On,
        "remote_apply" => S::RemoteApply,
        _ => S::Local,
    };
    layerfs_persistence::PostgresSelection::new(url, schema).with_synchronous_commit(setting)
}

/// Opens the PostgreSQL Store `identity` for this process's Store calls.
pub fn open(
    url: &str,
    schema: &str,
    sync: &str,
    identity: &Identity,
    template: &Path,
) -> Result<StoreService, SdkRefusal> {
    StoreService::open_postgres(
        &selection(url, schema, sync),
        &identity.binding,
        identity.cursor_key,
        template,
    )
    .map_err(|refusal| {
        let reason = match refusal {
            OpenRefusal::Unsupported(reason) => reason.0,
            OpenRefusal::Missing(reason)
            | OpenRefusal::Busy(reason)
            | OpenRefusal::Failed(reason) => reason,
        };
        refused("Failed", reason)
    })
}

/// The binding key the schema of a call's `postgresql { url, schema,
/// synchronousCommit }` records, read once.
pub fn read_binding(postgresql: &PgSelection) -> Result<Vec<u8>, SdkRefusal> {
    binding(
        postgresql.url()?,
        &postgresql.schema()?,
        &postgresql.setting()?,
    )
}

/// The refusal for a PostgreSQL call that got no session, `None` for any
/// other failure: no connection was made, or the server answered it with
/// an error (no such database, failed authentication).
pub fn no_session(error: &layerfs_storage::port::PersistenceError) -> Option<SdkRefusal> {
    let layerfs_storage::port::PersistenceError::Refused { status, .. } = error else {
        return None;
    };
    let message = match status.as_str() {
        "Closed" => NO_CONNECTION,
        "Rejected" => "the server refused the session: the database does not exist, or the user or password is wrong",
        _ => return None,
    };
    Some(refused("Unavailable", message))
}

/// How [`no_session`] says that no connection was made.
const NO_CONNECTION: &str = "no connection to the server was made (it refused the connection, did not answer, or closed it)";

/// Whether a PostgreSQL refusal's `message` says that no connection to
/// the server was made at all: the name did not resolve, nothing listened
/// or nothing answered in time (the client's `error connecting to server`
/// and `timeout waiting for server`, a silent dial's limit, or
/// [`no_session`]'s words). A server that answered (a refused login, a
/// missing database, any other error it sent) was reached: `false`. The
/// one case where a machine's loopback hint can help.
pub fn not_connected(message: &str) -> bool {
    [
        NO_CONNECTION,
        "error connecting to server",
        "timeout waiting for server",
        "the server did not answer within",
        "the dial could not be started",
    ]
    .iter()
    .any(|said| message.contains(said))
}

/// The binding key the LayerFS schema `schema` records.
fn binding(url: &str, schema: &str, sync: &str) -> Result<Vec<u8>, SdkRefusal> {
    (layerfs_persistence::PostgresHandles::binding(&selection(url, schema, sync))).map_err(
        |error| {
            no_session(&error).unwrap_or_else(|| {
                let said = crate::words::persistence(&error);
                refused("Failed", format!("reading schema {schema}: {said}"))
            })
        },
    )
}

/// The PostgreSQL Store a `sandbox.create` call's daemon opens (#506), or
/// None when the call's Store is not one. The URL is the call's
/// (`postgresql.url`) and goes only into the daemon's private config. The
/// caller's reach check found the project in the schema on its one session
/// (#65; every route to `sandbox.create` runs it), so nothing is read here.
pub fn sandbox_store(open: &OpenArgs) -> Result<Option<layerfs_bridge::DaemonStore>, SdkRefusal> {
    if !stores::postgresql(open)? {
        return Ok(None);
    }
    let identity = stores::identity(open)?;
    let Kind::Postgresql {
        schema,
        synchronous_commit,
    } = identity.kind
    else {
        return Err(refused("InvalidInput", "not a PostgreSQL Store"));
    };
    // the caller resolved the URL and rewrote it for the Sandbox's machine
    // (#535 §7.4); the daemon dials the same server as this side
    let url = url_of(open.postgresql.as_ref().and_then(|p| p.url.as_deref()))?.to_owned();
    Ok(Some(layerfs_bridge::DaemonStore::Postgres {
        url,
        schema,
        synchronous_commit,
        authority: layerfs_bridge::StoreAuthority {
            binding: identity.binding,
            cursor_key: identity.cursor_key,
        },
    }))
}

/// A PostgreSQL server's status at `url`, for `schemas`: one connection, made
/// once: `{ reachable: true, serverVersion, latencyMs, latencyUs, schemas: {
/// name: bytes | null } }` (null for a schema the server lacks), or, when
/// the server is not reached or refuses the login, `{ reachable: false,
/// error: { code: Unavailable | Denied, message } }`. A failed query after
/// connecting is refused `Failed`.
pub fn status(url: &str, schemas: &[String]) -> Outcome {
    use layerfs_bridge::control::SdkValue;
    use layerfs_persistence::PostgresStatusError as E;
    let unreached = |code: &str, message: String| {
        SdkValue::map([
            ("reachable", SdkValue::Bool(false)),
            (
                "error",
                SdkValue::map([
                    ("code", SdkValue::text(code)),
                    ("message", SdkValue::text(message)),
                ]),
            ),
        ])
    };
    match layerfs_persistence::postgresql_status(url, schemas) {
        Ok(status) => Ok(SdkValue::map([
            ("reachable", SdkValue::Bool(true)),
            ("serverVersion", SdkValue::text(status.server_version)),
            (
                "latencyMs",
                SdkValue::Int(status.latency.as_millis() as i64),
            ),
            (
                "latencyUs",
                SdkValue::Int(status.latency.as_micros() as i64),
            ),
            (
                "schemas",
                SdkValue::Map(
                    status
                        .schemas
                        .into_iter()
                        .map(|(name, bytes)| {
                            (
                                name,
                                bytes.map_or(SdkValue::Null, |b| SdkValue::Int(b as i64)),
                            )
                        })
                        .collect(),
                ),
            ),
        ])),
        Err(error @ E::Unreachable(..)) => Ok(unreached("Unavailable", error.to_string())),
        Err(error @ E::Denied(..)) => Ok(unreached("Denied", error.to_string())),
        Err(error @ E::Failed(_)) => Err(refused("Failed", error.to_string())),
    }
}

/// The open service of a PostgreSQL Store, from the call's
/// `postgresql: { url }` and the Store's record (#506).
pub fn open_postgres(
    postgresql: Option<&PgSelection>,
    identity: &Identity,
    template: &Path,
) -> Result<StoreService, SdkRefusal> {
    let Kind::Postgresql {
        schema,
        synchronous_commit,
    } = &identity.kind
    else {
        return Err(refused("InvalidInput", "not a PostgreSQL Store"));
    };
    let url = postgresql
        .and_then(|given| given.url.as_deref())
        .ok_or_else(|| {
            refused(
                "InvalidInput",
                "a PostgreSQL Store call needs postgresql.url (passed per call, never stored)",
            )
        })?;
    crate::postgresql::open::open(url, schema, synchronous_commit, identity, template)
}
