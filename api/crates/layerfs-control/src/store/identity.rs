//! Stores that hold many projects (#506 Store-first): their records in the
//! home, a new empty Store, the projects a Store holds, and a project added
//! to a live Store.
//!
//! A Store's record is `<home>/.stores/<store>/store.record` (private):
//! its kind, binding and cursor key, and for a PostgreSQL Store its schema
//! and `synchronous_commit` (the URL, which may hold a secret, is passed per
//! call). Its projects' entries are `<home>/.stores/<store>/projects/<p>/`.
//! A Store made by an Init of its own, before #506 or with no `store`, has
//! no record: it is named after its one project, whose entry
//! `<home>/<project>/` carries the Store's binding. Every Store-level call
//! is one attempt; nothing is retried.
use crate::common::outcome::{failed, refused, Outcome};
use crate::sandbox::files as discovery;
use crate::sandbox::registry::{random, text, Project};
use crate::store::record::{required, StoreKind, StoreRecord, StoreRef};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::provision::{ProviderKind, StoreManifest, StoreProfile};
use layerfs_bridge::sdk_values::hex;
use layerfs_history::{BranchId, HistoryCatalogConfig, HistoryName, LayerStackId};
use layerfs_persistence::{PersistenceConfig, SealedStore, SqlitePersistenceProfile};
use layerfs_project::store::{AddRequest, CreateRequest, InitError, ProjectApi};
use layerfs_project::{InitCounters, ProjectError};
use layerfs_store_service::StoreService;
use std::path::{Path, PathBuf};
use std::sync::Arc;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// What kind of Store a record describes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Kind {
    Sqlite,
    Postgresql {
        schema: String,
        synchronous_commit: String,
    },
}

/// A Store's authority: what any opener of it needs besides its location.
pub(crate) struct Identity {
    pub(crate) kind: Kind,
    pub(crate) binding: Vec<u8>,
    pub(crate) cursor_key: [u8; 32],
}

/// `<home>/.stores/<store>`, the name checked as a project name is.
pub(crate) fn store_dir(home: &Path, store: &str) -> Result<PathBuf> {
    HistoryName::new(store).map_err(|_| {
        refused(
            "InvalidInput",
            format!("a Store name is 1-63 lowercase letters, digits, _ . - (not {store:?})"),
        )
    })?;
    Ok(crate::common::home::paths::store_dir(home, store))
}

fn record_path(home: &Path, store: &str) -> Result<PathBuf> {
    store_dir(home, store)?;
    Ok(crate::common::home::paths::store_record(home, store))
}

/// Store `store`'s record in `home` (`store/<name>/record.json`).
fn load_record(home: &Path, store: &str) -> Result<StoreRecord> {
    let path = record_path(home, store)?;
    StoreRecord::read(home, store)?
        .ok_or_else(|| refused("NoSuchStore", format!("no Store record {}", path.display())))
}

/// Whether `store` has no Store record: a Store made by its project's own Init.
pub(crate) fn legacy(home: &Path, store: &str) -> Result<bool> {
    Ok(!record_path(home, store)?.exists())
}

/// The Store a call's project shares with others, when it names one: the
/// `store` argument, except for the one project of a Store made by its own
/// Init, which keeps its unshared name.
pub(crate) fn shared_store(args: &SdkValue) -> Result<Option<String>> {
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
pub(crate) fn identity(args: &SdkValue) -> Result<Identity> {
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
pub(crate) fn identity_of(value: &SdkValue) -> Result<Identity> {
    let name = value.get_text("name").unwrap_or("");
    identity_in(&StoreRecord::from_record(name, value.clone()))
}

/// The identity Store record `record` holds, from its [`StoreRef`], parsed
/// once (R4).
pub(crate) fn identity_in(record: &StoreRecord) -> Result<Identity> {
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
pub(crate) fn save_record(home: &Path, store: &str, identity: &Identity) -> Result<()> {
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
    let _store = crate::store::record::lock(home, store)?;
    let mut record = StoreRecord::read(home, store)?
        .unwrap_or_else(|| StoreRecord::from_record(store, SdkValue::Map(Vec::new())));
    for (name, value) in fields {
        record = record.with(name, value);
    }
    record.write(home)
}

/// The Store file a call names (`storePath`).
pub(crate) fn store_path(args: &SdkValue) -> Result<PathBuf> {
    match args.get_text("storePath") {
        Some(path) if !path.is_empty() => Ok(PathBuf::from(path)),
        _ => Err(refused(
            "InvalidInput",
            "a Store call needs storePath, the Store volume's Store file",
        )),
    }
}

/// Whether the Store a call names is a PostgreSQL Store (#506): its record says so.
pub(crate) fn postgresql(args: &SdkValue) -> Result<bool> {
    let Some(store) = args.get_text("store") else {
        return Ok(false);
    };
    let home = discovery::home(args)?;
    let path = record_path(&home, store)?;
    Ok(path.exists() && load_record(&home, store)?.kind()? == StoreKind::Postgresql)
}

/// Refuses a call on a PostgreSQL Store that needs a Store file.
pub(crate) fn sqlite_only(identity: &Identity, what: &str) -> Result<()> {
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
pub(crate) fn make_empty(
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
    layerfs_store_service::kernel_guard::check(destination)
        .map_err(|reason| refused("Unsupported", reason.0))?;
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
pub(crate) fn list_projects(args: &SdkValue) -> Outcome {
    crate::store::service::with_service(args, |service| {
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

/// A project entry's manifest for project `stack` of the SQLite Store
/// `identity` at `store`, as if installed by `sqlite` there.
pub(crate) fn manifest(
    identity: &Identity,
    store: &Path,
    stack: LayerStackId,
    branch: BranchId,
    root: [u8; 32],
    sqlite: &str,
) -> Result<(StoreManifest, StoreManifest)> {
    let bytes = std::fs::metadata(store).map_err(failed)?.len();
    let manifest = StoreManifest {
        provider: ProviderKind::Sqlite,
        locator: crate::project::init::LOCATOR.into(),
        profile: StoreProfile::Disposable,
        binding: identity.binding.clone(),
        cursor_key: identity.cursor_key,
        stack: stack.to_bytes(),
        branch: branch.to_bytes(),
        root,
        bytes,
        host_sqlite: sqlite.to_owned(),
        daemon_sqlite: None,
    };
    let mut installed = manifest.clone();
    installed.daemon_sqlite = Some(sqlite.to_owned());
    Ok((manifest, installed))
}

/// Saves a project entry `entry` (made by the caller) for an installed project.
pub(crate) fn save_entry(
    entry: &Path,
    (manifest, installed): (StoreManifest, StoreManifest),
) -> Result<()> {
    let project = Project {
        store: SealedStore {
            path: entry.join("sealed.sqlite"),
            profile: SqlitePersistenceProfile::Disposable,
            sqlite_version: manifest.host_sqlite.clone(),
            bytes: manifest.bytes,
        },
        manifest,
        installed: Some(installed),
    };
    discovery::save_project(entry, &project)
}

/// Makes the entry of project `name` of the shared Store `store` once,
/// owner-only, with its parents: `<home>/store/<store>/project/<name>/`.
pub(crate) fn make_entry(home: &Path, store: &str, name: &str) -> Result<PathBuf> {
    let directory = store_dir(home, store)?;
    crate::common::home::private_dir(&directory.join("project"))?;
    let entry = crate::common::home::paths::project_dir(home, store, name);
    std::fs::create_dir(&entry).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            refused("Exists", format!("project {name} exists in Store {store}"))
        } else {
            failed(error)
        }
    })?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&entry, std::fs::Permissions::from_mode(0o700)).map_err(failed)?;
    Ok(entry)
}

/// `project.init`'s add into a sqlite Store, `{ home, store, storePath, name, path, timeoutMs }`:
/// Inits the folder `path` as the new project `name` of the live SQLite
/// Store, through its admission and writer lane, then records its entry.
/// A Store policy is the Store's own, so `policy` is refused here.
pub(crate) fn add_project(args: &SdkValue) -> Outcome {
    let store = text(args, "store")?;
    let name = text(args, "name")?;
    let source = PathBuf::from(text(args, "path")?);
    if args.get("policy").is_some() {
        return Err(refused(
            "InvalidInput",
            "a project added to a Store uses the Store's policy; policy is refused",
        ));
    }
    let deadline = crate::project::init::deadline(args)?;
    let store_file = store_path(args)?;
    layerfs_store_service::kernel_guard::check(&store_file)
        .map_err(|reason| refused("Unsupported", reason.0))?;
    let identity = identity(args)?;
    sqlite_only(&identity, "adding a project")?;
    if !store_file.is_file() {
        return Err(refused(
            "NoStore",
            format!("no Store file at {}", store_file.display()),
        ));
    }
    let home = discovery::home(args)?;
    if name == store && legacy(&home, store)? {
        return Err(refused(
            "Exists",
            format!("Store {store} already has a project named {name}"),
        ));
    }
    let entry = make_entry(&home, store, name)?;
    let _guard = discovery::guard(&entry)?;
    let history = |text: &str| HistoryName::new(text).map_err(failed);
    let stack = LayerStackId::from_authority(crate::project::init::sixteen(random()?));
    let branch = BranchId::from_authority(crate::project::init::sixteen(random()?));
    let request = AddRequest {
        source,
        store: PersistenceConfig::sqlite(&store_file)
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable)
            .into(),
        binding: identity.binding.clone(),
        cursor_key: identity.cursor_key,
        stack,
        stack_name: history(name)?,
        branch,
        branch_name: history("main")?,
        scope_seed: random()?,
        deadline,
    };
    let counters = Arc::new(InitCounters::new());
    let (added, _) = layerfs_telemetry::timer::Timing::disabled("store.add_project", |timer| {
        ProjectApi::new().add_project(request, timer, counters)
    });
    let added = match added {
        Ok(added) => added,
        Err(failure) => return Err(add_failure(failure.error, Some(&entry), name, store)),
    };
    let root = added.initialized.root.to_bytes();
    save_entry(
        &entry,
        manifest(
            &identity,
            &store_file,
            stack,
            branch,
            root,
            &added.sqlite_version,
        )?,
    )?;
    Ok(SdkValue::map([
        ("kind", SdkValue::text("Initialized")),
        ("project", SdkValue::text(name)),
        ("store", SdkValue::text(store)),
        ("stack", SdkValue::text(hex(&stack.to_bytes()))),
        (
            "layer",
            SdkValue::text(hex(added.branch.branch.base_layer.as_slice())),
        ),
        (
            "root",
            SdkValue::text(hex(added.branch.effective_root.as_bytes())),
        ),
        (
            "storePath",
            SdkValue::text(store_file.display().to_string()),
        ),
    ]))
}

/// An add's refusal. One that wrote nothing to the Store (the name taken,
/// no acquisition tables, another add working, or a refused source entry)
/// removes the entry this call made, if any; any other keeps it for
/// diagnosis.
pub(crate) fn add_failure(
    error: InitError,
    entry: Option<&Path>,
    name: &str,
    store: &str,
) -> SdkRefusal {
    let refusal = match &error {
        InitError::Exists => refused(
            "Exists",
            format!("Store {store} already has a project named {name}"),
        ),
        InitError::NoAcquisition => refused(
            "Unsupported",
            format!(
                "Store {store} was made without the acquisition tables a project is added \
                 through; LayerFS adds no tables to an existing Store"
            ),
        ),
        InitError::Busy => refused(
            "Busy",
            format!("another project is being added to Store {store}; add {name} after it ends"),
        ),
        InitError::Project(ProjectError::InvalidInput { reason, path }) => refused(
            "InvalidInput",
            format!("adding {name} refused {}: {reason}", path.display()),
        ),
        _ => return failed(format!("adding project {name} to Store {store}: {error:?}")),
    };
    if let Some(entry) = entry {
        let _ = std::fs::remove_dir_all(entry);
    }
    refusal
}

/// `project.remove`'s work on the Store's machine (here, or in its worker
/// as the worker op `project.remove`), `{ home, store, storePath, project }`:
/// drops the project's stack (its Layers, Branches and Commits) from the
/// Store in one write, then its entry from the home. The other projects
/// stay. The entry of the project a Store was made by (its own Init) holds
/// the Store's identity, so a Store record is saved before that entry goes.
/// Its packs and objects stay in the Store; reclaiming them is A8 (#411).
/// The caller stops the project's Sandboxes and clients first.
pub(crate) fn remove_project(args: &SdkValue) -> Outcome {
    let home = discovery::home(args)?;
    let store = text(args, "store")?;
    let name = text(args, "project")?;
    let identity = identity(args)?;
    let own = name == store && legacy(&home, store)?;
    let entry = crate::common::home::paths::project_dir(&home, store, name);
    let stack = crate::store::service::with_service(args, |service| {
        let stacks = service
            .store
            .published(layerfs_project::store::projects)
            .map_err(failed)?;
        let Some(stack) = stacks.into_iter().find(|stack| stack.name.as_str() == name) else {
            return Err(refused(
                "NoSuchProject",
                format!("Store {store} holds no project named {name}"),
            ));
        };
        service
            .store
            .history()
            .remove_layerstack(stack.id)
            .map_err(|error| {
                failed(format!(
                    "removing project {name} from Store {store}: {error:?}"
                ))
            })?;
        Ok(stack.id)
    })?;
    // the own project's entry carried the Store's identity until now
    if own {
        crate::common::home::private_dir(&store_dir(&home, store)?)?;
        save_record(&home, store, &identity)?;
    }
    if entry.exists() {
        std::fs::remove_dir_all(&entry).map_err(failed)?;
    }
    Ok(SdkValue::map([
        ("stack", SdkValue::text(hex(&stack.to_bytes()))),
        ("packsKept", SdkValue::Bool(true)),
    ]))
}

/// The open service of a PostgreSQL Store, from the call's
/// `postgresql: { url }` and the Store's record (#506). Built without the
/// `postgresql` feature, it is `Unsupported`.
pub(crate) fn open_postgres(
    args: &SdkValue,
    identity: &Identity,
    template: &Path,
) -> Result<StoreService> {
    let Kind::Postgresql {
        schema,
        synchronous_commit,
    } = &identity.kind
    else {
        return Err(refused("InvalidInput", "not a PostgreSQL Store"));
    };
    let url = args
        .get("postgresql")
        .and_then(|given| given.get_text("url"))
        .ok_or_else(|| {
            refused(
                "InvalidInput",
                "a PostgreSQL Store call needs postgresql.url (passed per call, never stored)",
            )
        })?;
    crate::store::postgres::open(url, schema, synchronous_commit, identity, template)
}
