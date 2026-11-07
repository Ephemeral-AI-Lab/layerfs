//! Stores that hold many projects (#506 Store-first): their records in the
//! home, a new empty Store and the projects a Store holds.
//!
//! A Store's record is `<home>/.stores/<store>/store.record` (private):
//! its kind, binding and cursor key, and for a PostgreSQL Store its schema
//! and `synchronous_commit` (the URL, which may hold a secret, is passed per
//! call). Its projects' entries are `<home>/.stores/<store>/projects/<p>/`.
//! A Store made by an Init of its own, before #506 or with no `store`, has
//! no record: it is named after its one project, whose entry
//! `<home>/<project>/` carries the Store's binding. Every Store-level call
//! is one attempt; nothing is retried.
use crate::project::discovery;
use crate::record::{required, StoreKind, StoreRecord, StoreRef};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::sdk_values::hex;
use layerfs_history::{HistoryCatalogConfig, HistoryName};
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_persistence::{PersistenceConfig, SealedStore, SqlitePersistenceProfile};
use layerfs_project::store::{CreateRequest, ProjectApi};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// What kind of Store a record describes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Kind {
    Sqlite,
    Postgresql {
        schema: String,
        synchronous_commit: String,
    },
}

/// A Store's authority: what any opener of it needs besides its location.
pub struct Identity {
    pub kind: Kind,
    pub binding: Vec<u8>,
    pub cursor_key: [u8; 32],
}

/// `<home>/.stores/<store>`, the name checked as a project name is.
pub fn store_dir(home: &Path, store: &str) -> Result<PathBuf> {
    HistoryName::new(store).map_err(|_| {
        refused(
            "InvalidInput",
            format!("a Store name is 1-63 lowercase letters, digits, _ . - (not {store:?})"),
        )
    })?;
    Ok(layerfs_home::home::paths::store_dir(home, store))
}

fn record_path(home: &Path, store: &str) -> Result<PathBuf> {
    store_dir(home, store)?;
    Ok(layerfs_home::home::paths::store_record(home, store))
}

/// Store `store`'s record in `home` (`store/<name>/record.json`).
fn load_record(home: &Path, store: &str) -> Result<StoreRecord> {
    let path = record_path(home, store)?;
    StoreRecord::read(home, store)?
        .ok_or_else(|| refused("NoSuchStore", format!("no Store record {}", path.display())))
}

/// Whether `store` has no Store record: a Store made by its project's own Init.
pub fn legacy(home: &Path, store: &str) -> Result<bool> {
    Ok(!record_path(home, store)?.exists())
}

/// The Store a call's project shares with others, when it names one: the
/// `store` argument, except for the one project of a Store made by its own
/// Init, which keeps its unshared name.
pub fn shared_store(args: &SdkValue) -> Result<Option<String>> {
    let Some(store) = args.get_text("store") else {
        return Ok(None);
    };
    let home = discovery::home(args)?;
    let own = args.get_text("project") == Some(store) && legacy(&home, store)?;
    Ok((!own).then(|| store.to_owned()))
}

fn bytes32(value: &SdkValue, key: &str) -> Result<[u8; 32]> {
    match value.get(key) {
        Some(SdkValue::Bytes(bytes)) => bytes
            .as_slice()
            .try_into()
            .map_err(|_| refused("InvalidInput", format!("Store record {key}"))),
        _ => Err(refused("InvalidInput", format!("Store record {key}"))),
    }
}

/// The identity of the Store a call names: its `store`'s record, the
/// project entry of a Store made by its own Init, or, with no `store`, the
/// project's entry.
pub fn identity(args: &SdkValue) -> Result<Identity> {
    // a worker is sent its Store's record with each call (2.0, #533 step 7)
    if let Some(record @ SdkValue::Map(_)) = args.get("store_record") {
        return identity_of(record);
    }
    let home = discovery::home(args)?;
    let store = args.get_text("store");
    if let Some(store) = store {
        let path = record_path(&home, store)?;
        if path.exists() {
            return identity_in(&load_record(&home, store)?);
        }
    }
    // a Store made by its project's own Init: that project's entry
    let mut named = args.clone();
    if let (Some(store), SdkValue::Map(fields)) = (store, &mut named) {
        fields.retain(|(name, _)| name != "project" && name != "store");
        fields.push(("project".to_owned(), SdkValue::text(store)));
    }
    let project = discovery::project(&named).map_err(|refusal| match store {
        Some(store) if refusal.code == "Failed" || refusal.code == "Unauthorized" => refused(
            "NoSuchStore",
            format!("no LayerFS Store named {store} in this home"),
        ),
        _ => refusal,
    })?;
    Ok(Identity {
        kind: Kind::Sqlite,
        binding: project.manifest.binding,
        cursor_key: project.manifest.cursor_key,
    })
}

/// The identity a Store record sent with a call holds.
pub fn identity_of(value: &SdkValue) -> Result<Identity> {
    let name = value.get_text("name").unwrap_or("");
    identity_in(&StoreRecord::from_record(name, value.clone()))
}

/// The identity Store record `record` holds, from its [`StoreRef`], parsed
/// once (R4).
pub fn identity_in(record: &StoreRecord) -> Result<Identity> {
    let kind = match record.store_ref()? {
        StoreRef::Sqlite { .. } => Kind::Sqlite,
        StoreRef::Postgres { server, .. } => Kind::Postgresql {
            schema: required(server.schema.clone(), "schema")?,
            synchronous_commit: required(server.synchronous_commit.clone(), "synchronous_commit")?,
        },
    };
    let binding = record
        .binding()
        .ok_or_else(|| refused("InvalidInput", "Store record binding"))?
        .to_vec();
    Ok(Identity {
        kind,
        binding,
        cursor_key: bytes32(record.to_record(), "cursor")?,
    })
}

/// Saves a Store record; `store_dir` must exist.
pub fn save_record(home: &Path, store: &str, identity: &Identity) -> Result<()> {
    let mut fields = vec![
        ("name", SdkValue::text(store)),
        ("binding", SdkValue::Bytes(identity.binding.clone())),
        ("cursor", SdkValue::Bytes(identity.cursor_key.to_vec())),
    ];
    match &identity.kind {
        Kind::Sqlite => fields.push(("kind", SdkValue::text("sqlite"))),
        Kind::Postgresql {
            schema,
            synchronous_commit,
        } => fields.extend([
            ("kind", SdkValue::text("postgresql")),
            ("schema", SdkValue::text(schema)),
            ("synchronous_commit", SdkValue::text(synchronous_commit)),
        ]),
    }
    // the identity fields are this module's; the record's other fields
    // (machine, backups) are kept, read and written under the Store's lock
    record_path(home, store)?;
    let _store = crate::record::lock(home, store)?;
    let mut record = StoreRecord::read(home, store)?
        .unwrap_or_else(|| StoreRecord::from_record(store, SdkValue::Map(Vec::new())));
    for (name, value) in fields {
        record = record.with(name, value);
    }
    record.write(home)
}

/// The Store file a call names (`storePath`).
pub fn store_path(args: &SdkValue) -> Result<PathBuf> {
    match args.get_text("storePath") {
        Some(path) if !path.is_empty() => Ok(PathBuf::from(path)),
        _ => Err(refused(
            "InvalidInput",
            "a Store call needs storePath, the Store volume's Store file",
        )),
    }
}

/// Whether the Store a call names is a PostgreSQL Store (#506): its record says so.
pub fn postgresql(args: &SdkValue) -> Result<bool> {
    let Some(store) = args.get_text("store") else {
        return Ok(false);
    };
    let home = discovery::home(args)?;
    let path = record_path(&home, store)?;
    Ok(path.exists() && load_record(&home, store)?.kind()? == StoreKind::Postgresql)
}

/// Refuses a call on a PostgreSQL Store that needs a Store file.
pub fn sqlite_only(identity: &Identity, what: &str) -> Result<()> {
    match identity.kind {
        Kind::Sqlite => Ok(()),
        Kind::Postgresql { .. } => Err(refused(
            "Unsupported",
            format!("{what} needs a SQLite Store; this one is a PostgreSQL schema"),
        )),
    }
}

/// Creates and seals the empty Store at `<destination>.creating`, then
/// renames it into place; a failure removes this call's own file.
pub fn make_empty(
    global: &Path,
    destination: &Path,
    identity: &Identity,
    policy: layerfs_storage::StoragePolicy,
) -> Result<SealedStore> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    if !global.exists() {
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(global)
            .map_err(failed)?;
    }
    let (target, _) = layerfs_store_service::install::paths(destination).map_err(failed)?;
    layerfs_store_service::install::absent(&target)
        .map_err(|error| refused("Exists", format!("{}: {error}", target.display())))?;
    let mut name = target.as_os_str().to_owned();
    name.push(".creating");
    let temporary = PathBuf::from(name);
    let result = (|| {
        let sealed = ProjectApi::new()
            .create_store(CreateRequest {
                store: PersistenceConfig::sqlite(&temporary)
                    .with_sqlite_profile(SqlitePersistenceProfile::Disposable)
                    .into(),
                policy,
                catalog: HistoryCatalogConfig {
                    binding_key: identity.binding.clone(),
                    cursor_key: identity.cursor_key,
                    incarnation: 1,
                },
            })
            .map_err(|error| refused("Failed", format!("Store create: {error:?}")))?;
        // a daemon opens only a Store file only its user can read
        std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o600))
            .map_err(failed)?;
        layerfs_store_service::install::absent(&target).map_err(failed)?;
        std::fs::rename(&temporary, &target).map_err(failed)?;
        Ok(sealed)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

/// The projects of `{ home, store, storePath, postgresql? }`:
/// `[{ name, stack, headLayer }]` by name, read from the Store itself.
pub fn list_projects(args: &SdkValue) -> Outcome {
    crate::service::with_service(args, |service| {
        let stacks = service
            .store
            .published(layerfs_project::store::projects)
            .map_err(failed)?;
        Ok(SdkValue::List(
            stacks
                .into_iter()
                .map(|stack| {
                    SdkValue::map([
                        ("name", SdkValue::text(stack.name.as_str())),
                        ("stack", SdkValue::text(hex(&stack.id.to_bytes()))),
                        (
                            "headLayer",
                            SdkValue::text(hex(stack.head_layer.as_slice())),
                        ),
                    ])
                })
                .collect(),
        ))
    })
}
