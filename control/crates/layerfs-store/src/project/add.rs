//! A project added to a live SQLite Store (`project.init` into a shared
//! Store): its entry in the home and the add through the Store's writer lane.
use crate::identity::{identity, legacy, sqlite_only, store_dir, Identity};
use crate::open_args::OpenArgs;
use crate::project::discovery;
use crate::project::discovery::Project;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::provision::{ProviderKind, StoreManifest, StoreProfile};
use layerfs_bridge::sdk_values::hex;
use layerfs_history::{BranchId, HistoryName, LayerStackId};
use layerfs_home::helpers::random;
use layerfs_home::home::paths;
use layerfs_home::names::StoreName;
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_persistence::{PersistenceConfig, SealedStore, SqlitePersistenceProfile};
use layerfs_project::store::{AddRequest, InitError, ProjectApi};
use layerfs_project::{InitCounters, ProjectError};
use std::path::{Path, PathBuf};
use std::sync::Arc;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// A project entry's manifest for project `stack` of the SQLite Store
/// `identity` at `store`, as if installed by `sqlite` there.
pub fn manifest(
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
pub fn save_entry(
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
pub fn make_entry(home: &Path, store: &str, name: &str) -> Result<PathBuf> {
    let directory = store_dir(home, store)?;
    layerfs_home::home::private_dir(&directory.join("project"))?;
    let entry = layerfs_home::home::paths::project_dir(home, store, name);
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

/// `project.init`'s add of folder `source` as the new project `name` of
/// Store `store` in `home` (the caller's, or a worker's root), whose file is
/// `<home>/store/<store>/store.sqlite`.
#[derive(Clone, Debug)]
pub struct ProjectAdd {
    /// The home, or a worker's root.
    pub home: PathBuf,
    /// The Store.
    pub store: StoreName,
    /// The new project's name.
    pub name: String,
    /// The folder.
    pub source: PathBuf,
    /// Init's time limit in milliseconds ([`deadline`](super::init::deadline)).
    pub timeout_ms: Option<i64>,
    /// The Store's record, sent to a worker with the call.
    pub store_record: Option<SdkValue>,
}

impl ProjectAdd {
    /// What opens the Store: its home, name, file and record.
    pub fn open(&self) -> OpenArgs {
        OpenArgs {
            home: Some(self.home.clone()),
            store: Some(self.store.to_string()),
            store_path: Some(self.store_file().display().to_string()),
            store_record: self.store_record.clone(),
            ..OpenArgs::default()
        }
    }

    /// The Store's file.
    pub fn store_file(&self) -> PathBuf {
        paths::store_file(&self.home, &self.store)
    }
}

/// Inits the folder as the new project of the live SQLite Store, through
/// its admission and writer lane, then records its entry. A Store policy is
/// the Store's own; a project added to it has none.
pub fn add_project(request: &ProjectAdd) -> Outcome {
    let (store, name) = (request.store.as_str(), request.name.as_str());
    let source = request.source.clone();
    let deadline = crate::project::init::deadline(request.timeout_ms)?;
    let open = request.open();
    let store_file = request.store_file();
    let global = store_file.parent().unwrap_or(&store_file);
    layerfs_store_service::kernel_guard::check(global)
        .map_err(|reason| refused("Unsupported", reason.0))?;
    let identity = identity(&open)?;
    sqlite_only(&identity, "adding a project")?;
    if !store_file.is_file() {
        return Err(refused(
            "NoStore",
            format!("no Store file at {}", store_file.display()),
        ));
    }
    let home = discovery::home_of(Some(&request.home))?;
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
pub fn add_failure(error: InitError, entry: Option<&Path>, name: &str, store: &str) -> SdkRefusal {
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
