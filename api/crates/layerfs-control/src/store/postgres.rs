//! PostgreSQL Stores (#506): opening one for Store calls and attaching a
//! LayerFS schema.
//!
//! A PostgreSQL Store is one schema; its URL, which may hold a password, is
//! passed per call (`postgresql: { url }`) and never stored. Nothing
//! converts a SQLite Store into one (#533 step 10c): a project reaches
//! PostgreSQL only by an Init into it. Built without the `postgresql`
//! feature, every call here is `Unsupported`.
use crate::common::outcome::{refused, Outcome};
use crate::sandbox::registry::text;
use crate::store::identity::{self as stores, Identity, Kind};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_store_service::StoreService;
use std::path::Path;

#[cfg(not(feature = "postgresql"))]
const UNBUILT: &str =
    "this LayerFS build has no PostgreSQL engine; build the binding with --features postgresql";

/// `synchronous_commit` as the call or record names it: off (default),
/// local, remote_write, on or remote_apply.
pub(crate) fn setting(args: &SdkValue) -> Result<String, SdkRefusal> {
    let given = args
        .get("postgresql")
        .and_then(|p| p.get_text("synchronousCommit"))
        .unwrap_or("off");
    match given {
        "off" | "local" | "remote_write" | "on" | "remote_apply" => Ok(given.to_owned()),
        other => Err(refused(
            "InvalidInput",
            format!(
                "synchronousCommit is off, local, remote_write, on or remote_apply, not {other}"
            ),
        )),
    }
}

/// The schema a call names: `[a-z_][a-z0-9_]*`, at most 63 bytes.
pub(crate) fn schema(args: &SdkValue) -> Result<String, SdkRefusal> {
    let schema = args
        .get("postgresql")
        .and_then(|p| p.get_text("schema"))
        .ok_or_else(|| refused("InvalidInput", "postgresql.schema is required"))?;
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

pub(crate) fn url(args: &SdkValue) -> Result<&str, SdkRefusal> {
    args.get("postgresql")
        .and_then(|p| p.get_text("url"))
        .ok_or_else(|| {
            refused(
                "InvalidInput",
                "postgresql.url is required (passed per call, never stored)",
            )
        })
}

#[cfg(feature = "postgresql")]
pub(crate) fn selection(
    url: &str,
    schema: &str,
    sync: &str,
) -> layerfs_persistence::PostgresSelection {
    use layerfs_persistence::SynchronousCommit as S;
    let setting = match sync {
        "local" => S::Local,
        "remote_write" => S::RemoteWrite,
        "on" => S::On,
        "remote_apply" => S::RemoteApply,
        _ => S::Off,
    };
    layerfs_persistence::PostgresSelection::new(url, schema).with_synchronous_commit(setting)
}

/// Opens the PostgreSQL Store `identity` for this process's Store calls.
pub(crate) fn open(
    url: &str,
    schema: &str,
    sync: &str,
    identity: &Identity,
    template: &Path,
) -> Result<StoreService, SdkRefusal> {
    #[cfg(feature = "postgresql")]
    {
        StoreService::open_postgres(
            &selection(url, schema, sync),
            &identity.binding,
            identity.cursor_key,
            template,
        )
        .map_err(|refusal| refused("Failed", format!("{refusal:?}")))
    }
    #[cfg(not(feature = "postgresql"))]
    {
        let _ = (url, schema, sync, identity, template);
        Err(refused("Unsupported", UNBUILT))
    }
}

/// The binding key the schema of a call's `postgresql { url, schema,
/// synchronousCommit }` records, read once.
pub(crate) fn read_binding(args: &SdkValue) -> Result<Vec<u8>, SdkRefusal> {
    binding(url(args)?, &schema(args)?, &setting(args)?)
}

/// The binding key the LayerFS schema `schema` records.
fn binding(url: &str, schema: &str, sync: &str) -> Result<Vec<u8>, SdkRefusal> {
    #[cfg(feature = "postgresql")]
    {
        layerfs_persistence::PostgresHandles::binding(&selection(url, schema, sync))
            .map_err(|error| refused("Failed", format!("reading schema {schema}: {error}")))
    }
    #[cfg(not(feature = "postgresql"))]
    {
        let _ = (url, schema, sync);
        Err(refused("Unsupported", UNBUILT))
    }
}

/// The PostgreSQL Store a `sandbox.create` call's daemon opens (#506), or
/// None when the call's Store is not one. The URL is the call's
/// (`postgresql.url`) and goes only into the daemon's private config. The
/// project must be in the schema; its home entry, which holds its Sandbox
/// records, is made here (owner-only) the first time.
pub(crate) fn sandbox_store(
    args: &SdkValue,
) -> Result<Option<layerfs_bridge::DaemonStore>, SdkRefusal> {
    if !stores::postgresql(args)? {
        return Ok(None);
    }
    let identity = stores::identity(args)?;
    let Kind::Postgresql {
        schema,
        synchronous_commit,
    } = identity.kind
    else {
        return Err(refused("InvalidInput", "not a PostgreSQL Store"));
    };
    // a container may reach the server by another name than this process
    let url = args
        .get("postgresql")
        .and_then(|p| p.get_text("daemon_url"))
        .map_or_else(|| url(args).map(str::to_owned), |u| Ok(u.to_owned()))?;
    let name = text(args, "project")?;
    let listed = stores::list_projects(args)?;
    let found = matches!(&listed, SdkValue::List(items)
        if items.iter().any(|p| p.get_text("name") == Some(name)));
    if !found {
        return Err(refused(
            "NoSuchProject",
            format!("no project {name} in the PostgreSQL Store's schema {schema}"),
        ));
    }
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

/// A PostgreSQL server's status, `{ home, url, schemas }`: one connection, made
/// once: `{ reachable: true, serverVersion, latencyMs, latencyUs, schemas: {
/// name: bytes | null } }` (null for a schema the server lacks), or, when
/// the server is not reached or refuses the login, `{ reachable: false,
/// error: { code: Unavailable | Denied, message } }`. A failed query after
/// connecting is refused `Failed`.
pub(crate) fn status(args: &SdkValue) -> Outcome {
    let url = text(args, "url")?;
    let schemas = match args.get("schemas") {
        None | Some(SdkValue::Null) => Vec::new(),
        Some(SdkValue::List(items)) => items
            .iter()
            .map(|item| match item {
                SdkValue::Text(name) => Ok(name.clone()),
                _ => Err(refused("InvalidInput", "schemas is a list of schema names")),
            })
            .collect::<Result<_, _>>()?,
        Some(_) => return Err(refused("InvalidInput", "schemas is a list of schema names")),
    };
    #[cfg(feature = "postgresql")]
    {
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
        match layerfs_persistence::postgresql_status(url, &schemas) {
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
            Err(error @ E::Unreachable(_)) => Ok(unreached("Unavailable", error.to_string())),
            Err(error @ E::Denied(_)) => Ok(unreached("Denied", error.to_string())),
            Err(error @ E::Failed(_)) => Err(refused("Failed", error.to_string())),
        }
    }
    #[cfg(not(feature = "postgresql"))]
    {
        let _ = (url, schemas);
        Err(refused("Unsupported", UNBUILT))
    }
}
