//! PostgreSQL Stores (#506): opening one for Store calls, importing a
//! SQLite Store into a new schema, and attaching an imported schema again.
//!
//! A PostgreSQL Store is one schema; its URL, which may hold a password, is
//! passed per call (`postgresql: { url }`) and never stored. Init stays
//! SQLite only, so a PostgreSQL Store gets its data from an import. Built
//! without the `postgresql` feature, every call here is `Unsupported`.
use crate::discovery;
use crate::job::{refused, Outcome};
use crate::registry::text;
use crate::stores::{self, Identity, Kind};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_store_service::StoreService;
use std::path::Path;

#[cfg(not(feature = "postgresql"))]
const UNBUILT: &str =
    "this LayerFS build has no PostgreSQL engine; build the binding with --features postgresql";

/// `synchronous_commit` as the call or record names it: off (default),
/// local, remote_write, on or remote_apply.
fn setting(args: &SdkValue) -> Result<String, SdkRefusal> {
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
fn schema(args: &SdkValue) -> Result<String, SdkRefusal> {
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

fn url(args: &SdkValue) -> Result<&str, SdkRefusal> {
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
fn selection(url: &str, schema: &str, sync: &str) -> layerfs_persistence::PostgresSelection {
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

/// `runtime.store.import_postgresql { home, store, storePath, target,
/// postgresql: { url, schema, synchronousCommit } }`: copies the SQLite Store
/// `store` (every project, the same ids) into the new schema, then records
/// the PostgreSQL Store `target` with the source's binding.
pub(crate) fn import(args: &SdkValue) -> Outcome {
    let home = discovery::home(args)?;
    let target = text(args, "target")?;
    let (url, schema, sync) = (url(args)?, schema(args)?, setting(args)?);
    let source = stores::identity(args)?;
    stores::sqlite_only(&source, "an import into PostgreSQL")?;
    let store_file = stores::store_path(args)?;
    let directory = stores::claim(&home, target)?;
    let _guard = discovery::guard(&directory)?;
    let imported = copy(&store_file, url, &schema, &sync, &source);
    let receipt = match imported {
        Ok(receipt) => receipt,
        Err(refusal) => {
            let _ = std::fs::remove_dir_all(&directory);
            return Err(refusal);
        }
    };
    let identity = Identity {
        kind: Kind::Postgresql {
            schema: schema.clone(),
            synchronous_commit: sync,
        },
        binding: source.binding,
        cursor_key: source.cursor_key,
    };
    stores::save_record(&home, target, &identity)?;
    Ok(SdkValue::map([
        ("kind", SdkValue::text("Imported")),
        ("store", SdkValue::text(target)),
        ("schema", SdkValue::text(schema)),
        ("tables", receipt.0),
        ("packBytes", SdkValue::Int(receipt.1 as i64)),
    ]))
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

/// One import: `(tables, pack bytes)`.
fn copy(
    store: &Path,
    url: &str,
    schema: &str,
    sync: &str,
    source: &Identity,
) -> Result<(SdkValue, u64), SdkRefusal> {
    #[cfg(feature = "postgresql")]
    {
        use layerfs_persistence::{PersistenceConfig, PostgresHandles, SqlitePersistenceProfile};
        let receipt = PostgresHandles::import(
            PersistenceConfig::sqlite(store)
                .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
            &selection(url, schema, sync),
            &source.binding,
            source.cursor_key,
        )
        .map_err(|error| refused("Failed", format!("import into PostgreSQL: {error}")))?;
        let tables = receipt
            .tables
            .iter()
            .map(|(table, rows)| {
                SdkValue::map([
                    ("table", SdkValue::text(*table)),
                    ("rows", SdkValue::Int(*rows as i64)),
                ])
            })
            .collect();
        Ok((SdkValue::List(tables), receipt.pack_bytes))
    }
    #[cfg(not(feature = "postgresql"))]
    {
        let _ = (store, url, schema, sync, source);
        Err(refused("Unsupported", UNBUILT))
    }
}

/// `runtime.store.attach_postgresql { home, store, postgresql: { url,
/// schema, synchronousCommit } }`: records the PostgreSQL Store `store`
/// and checks that it opens with that URL. A schema this home imported
/// keeps its recorded binding and cursor key; any other LayerFS schema
/// gets the binding it records (read once, read-only) and a fresh cursor
/// key, as a clone does. The write setting is the call's.
pub(crate) fn attach(args: &SdkValue) -> Outcome {
    let home = discovery::home(args)?;
    let store = text(args, "store")?;
    let (schema, sync) = (schema(args)?, setting(args)?);
    let url = url(args)?;
    let fresh = stores::legacy(&home, store)?;
    let (binding, cursor_key) = if fresh {
        if home.join(store).join("project.record").exists() {
            return Err(refused(
                "Exists",
                format!("Store {store} is a SQLite Store"),
            ));
        }
        (binding(url, &schema, &sync)?, crate::registry::random()?)
    } else {
        let recorded = stores::identity(args)?;
        match &recorded.kind {
            Kind::Postgresql {
                schema: recorded, ..
            } if *recorded != schema => {
                return Err(refused(
                    "InvalidInput",
                    format!("Store {store} is schema {recorded}, not {schema}"),
                ))
            }
            Kind::Postgresql { .. } => {}
            Kind::Sqlite => {
                return Err(refused(
                    "Exists",
                    format!("Store {store} is a SQLite Store"),
                ))
            }
        }
        (recorded.binding, recorded.cursor_key)
    };
    let identity = Identity {
        kind: Kind::Postgresql {
            schema: schema.clone(),
            synchronous_commit: sync.clone(),
        },
        binding,
        cursor_key,
    };
    let directory = stores::store_dir(&home, store)?;
    discovery::directory(&home.join(".stores"))?;
    discovery::directory(&directory)?;
    let _guard = discovery::guard(&directory)?;
    stores::save_record(&home, store, &identity)?;
    let projects = match stores::list_projects(args) {
        Ok(projects) => projects,
        // a schema that does not open is not attached
        Err(refusal) if fresh => {
            let _ = std::fs::remove_dir_all(&directory);
            return Err(refusal);
        }
        Err(refusal) => return Err(refusal),
    };
    Ok(SdkValue::map([
        ("kind", SdkValue::text("Attached")),
        ("store", SdkValue::text(store)),
        ("schema", SdkValue::text(schema)),
        ("synchronousCommit", SdkValue::text(sync)),
        ("projects", projects),
    ]))
}
