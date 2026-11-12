//! `backup.restore`: a backup as a sqlite Store on `native`.
use super::family::{hex, write_link};
use super::progress::Shown;
use super::record::{BucketRecord, StoreLink};
use crate::lease;
use crate::project::record::ProjectRecord;
use crate::record::{StoreKind, StoreRecord};
use crate::Related;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::provision::{ProviderKind, StoreManifest, StoreProfile};
use layerfs_bucket::backup as remote;
use layerfs_bucket::bucket::pieces;
use layerfs_history::{BranchSnapshot, HistoryName, NamedSnapshot};
use layerfs_home::events::Events;
use layerfs_home::home::flock::{try_lock, unlock};
use layerfs_home::home::{self, paths};
use layerfs_home::outcome::{failed, refused, Outcome};
use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// `backup.restore(store, bucket, file?)`.
pub type BackupRestore = super::create::BackupCreate;

/// `backup.restore(store, bucket, file?)`: the backup as a sqlite Store on
/// `native`: a new Store when `store` does not exist, else it replaces an
/// idle Store on `native` unchanged since its last backup or restore
/// (`Conflict` otherwise; `InUse` while a process holds it or a Sandbox of
/// its projects runs). The download, its checks and the read of its
/// projects happen before the home lock; the lock covers the re-checks, the
/// install and the record writes only.
pub fn restore(
    home: &Path,
    events: &mut Events<'_>,
    request: &BackupRestore,
    related: Related,
) -> Outcome {
    let home = home.to_path_buf();
    let store = request.store.as_str();
    layerfs_home::names::StoreName::parse(store)?;
    let bucket = request.bucket.as_str();
    let driver = BucketRecord::of(&home, bucket)?.driver()?;
    let existing = StoreRecord::read(&home, store)?;
    let file = request
        .file
        .clone()
        .or_else(|| {
            let link = existing.as_ref()?.link(bucket)?;
            link.file().map(str::to_owned)
        })
        .unwrap_or_else(|| store.to_owned());
    let mut _lease = None;
    let identity = match &existing {
        None => None,
        Some(record) => {
            if record.kind()? == StoreKind::Postgresql {
                return Err(refused(
                    "InvalidInput",
                    format!("Store {store} is a PostgreSQL database"),
                ));
            }
            let identity = crate::identity::identity_in(record)?;
            // refused before the download too; checked again before the install
            {
                let _lock = home::lock::take(&home)?;
                let _store = crate::record::lock(&home, store)?;
                replaceable(&home, store, bucket, record, related)?;
            }
            _lease = Some(lease::hold(&home, store)?);
            Some(identity)
        }
    };
    let cursor = match &identity {
        Some(identity) => identity.cursor_key,
        None => layerfs_home::helpers::random()?,
    };
    let mut shown = Shown::new();
    shown.phase(events, "check");
    let staging = Staging::make(&home)?;
    let staged = staging.0.join(remote::STORE_FILE);
    // `download` with its bytes and pieces, `verify`, then `install`
    let loaded = shown
        .transfer(events, || {
            remote::load(driver.as_ref(), &file, &staged, cursor)
        })
        // the cause alone: what the load left is in `staging`, removed below
        .map_err(|failure| refused("Failed", failure.cause))?;
    let record = match (&existing, &identity) {
        (Some(record), Some(identity)) => {
            if loaded.binding != identity.binding {
                return Err(refused(
                    "Failed",
                    format!(
                        "the bucket's {file} has another binding ({}), not Store {store}'s",
                        pieces::hex(&loaded.binding)
                    ),
                ));
            }
            record.clone()
        }
        _ => StoreRecord::restored(store, &loaded.binding, &cursor),
    };
    let found = projects_in(&home, store, &record, &staged)?;
    shown.phase(events, "install");
    let _lock = home::lock::take(&home)?;
    // a Store replaced here is checked and replaced under its lock, which
    // the link below takes again
    let store_lock = match &existing {
        Some(_) => Some(crate::record::lock(&home, store)?),
        None => None,
    };
    let destination = paths::store_file(&home, store);
    let now = StoreRecord::read(&home, store)?;
    match (&existing, now) {
        (None, None) if !destination.exists() => {
            home::private_dir(&paths::store_dir(&home, store))?;
            // a link never replaces a Store that appeared meanwhile
            std::fs::hard_link(&staged, &destination).map_err(failed)?;
            record.write(&home)?;
        }
        (None, _) => {
            return Err(refused(
                "Exists",
                format!("a Store named {store} was made while this restore downloaded"),
            ));
        }
        (Some(_), None) => {
            return Err(refused(
                "NotFound",
                format!("Store {store} was removed while this restore downloaded"),
            ));
        }
        (Some(_), Some(now)) => {
            if now.field("binding") != record.field("binding") {
                return Err(refused(
                    "Conflict",
                    format!("Store {store} was replaced while this restore downloaded"),
                ));
            }
            replaceable(&home, store, bucket, &now, related)?;
            std::fs::rename(&staged, &destination).map_err(failed)?;
        }
    }
    drop(store_lock);
    write_link(
        &home,
        store,
        bucket,
        StoreLink::new(
            &file,
            "restore",
            loaded.bytes,
            &loaded.sha256,
            loaded.heads.as_ref(),
        ),
    )?;
    let Listing {
        projects: held,
        sqlite: opened,
    } = found;
    let restored = Restored {
        binding: match &identity {
            Some(identity) => identity.binding.clone(),
            None => loaded.binding.clone(),
        },
        cursor_key: cursor,
        // the Store file this home now holds, as a daemon opens it
        bytes: std::fs::metadata(&destination).map_err(failed)?.len(),
        // the version its push recorded; else the one that opens it here
        sqlite: loaded.sqlite.clone().unwrap_or(opened),
    };
    let projects = write_projects(&home, store, &restored, &held)?;
    Ok(SdkValue::map([
        ("store", SdkValue::text(store)),
        ("bucket", SdkValue::text(bucket)),
        ("file", SdkValue::text(&file)),
        ("bytes", SdkValue::Int(loaded.bytes as i64)),
        ("sha256", hex(&loaded.sha256)),
        ("heads", loaded.heads.as_ref().map(hex).unwrap_or_default()),
        ("projects", projects),
    ]))
}

/// A restore's own download folder, `store/.restore-<random>/` (hidden from
/// the Store names, and on the home's filesystem so the install is one
/// rename); removed with whatever it still holds when the restore ends.
/// The restore holds an `flock` on the folder's `lock` file throughout, so
/// a folder whose lock can be taken belongs to a restore that ended without
/// removing it (killed): the next restore removes it.
struct Staging(std::path::PathBuf, File);

impl Staging {
    fn make(home: &Path) -> Result<Self> {
        let random = layerfs_home::helpers::random()?;
        let parent = home.join("store");
        let dir = parent.join(format!(".restore-{}", pieces::hex(&random[..12])));
        // under the home lock no folder is between its creation and its lock
        let _lock = home::lock::take(home)?;
        sweep(&parent);
        home::private_dir(&dir)?;
        let lock = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(dir.join("lock"))
            .map_err(failed)?;
        if !try_lock(&lock, true).map_err(failed)? {
            return Err(failed("a new restore folder's lock is held"));
        }
        Ok(Self(dir, lock))
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
        unlock(&self.1);
    }
}

/// Removes the restore folders under `parent` whose restore has ended: their
/// lock can be taken. A folder without a lock file is left alone.
fn sweep(parent: &Path) {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().starts_with(".restore-") {
            continue;
        }
        let dir = entry.path();
        let Ok(lock) = OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(dir.join("lock"))
        else {
            continue;
        };
        if try_lock(&lock, true).unwrap_or(false) {
            let _ = std::fs::remove_dir_all(&dir);
            unlock(&lock);
        }
    }
}

/// Refuses replacing Store `store` unless it is an idle sqlite Store on
/// `native`, unchanged since its last backup or restore through `bucket`.
/// Under the home lock and the Store's lock.
fn replaceable(
    home: &Path,
    store: &str,
    bucket: &str,
    record: &StoreRecord,
    related: Related,
) -> Result<()> {
    let machine = record.machine()?;
    if machine != "native" {
        return Err(refused("Unsupported", format!("Store {store} is on {machine}; a restore replaces a Store on native: move it there first")));
    }
    record.unblocked()?;
    let destination = paths::store_file(home, store);
    // this process's own open copy closes first
    crate::open::close(&destination)?;
    (related.store_idle)(home, store)?;
    let linked = record.link(bucket).and_then(|link| link.heads());
    let now = remote::heads(&destination).ok().map(|h| pieces::hex(&h));
    if linked.is_none() || now != linked {
        return Err(refused(
            "Conflict",
            format!("Store {store} changed since its last backup or restore through {bucket}; back it up first or restore under a new name"),
        ));
    }
    // every opener (a daemon, a mounted Workspace, an SDK process) holds
    // SQLite's sidecars while the Store is open
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = destination.as_os_str().to_owned();
        sidecar.push(suffix);
        if std::path::Path::new(&sidecar).exists() {
            return Err(refused(
                "InUse",
                format!("Store {store} is open (its {suffix} exists)"),
            ));
        }
    }
    Ok(())
}

/// What the restored Store file holds, read before it is installed: its
/// projects and the SQLite version that opened the file here.
struct Listing {
    projects: Vec<Found>,
    sqlite: String,
}

/// One project a restored Store holds.
struct Found {
    name: String,
    /// Its stack, as a listing gives it.
    stack: SdkValue,
    /// Its `main` Branch, absent for a Store that holds no such Branch.
    main: Option<Box<BranchSnapshot>>,
}

/// What a restored project's record is built from: the restored Store's
/// own authority, file and SQLite version, which `project.init` also
/// records when it writes the same record.
struct Restored {
    binding: Vec<u8>,
    cursor_key: [u8; 32],
    bytes: u64,
    sqlite: String,
}

impl Found {
    /// The record `project.init` writes for this project, from the restored
    /// Store's own history: its `manifest` and the `installed` manifest a
    /// Sandbox needs. `None` when the project has no `main` Branch to name.
    fn rebuilt(&self, store: &str, restored: &Restored) -> Option<ProjectRecord> {
        let main = self.main.as_ref()?;
        let manifest = StoreManifest {
            provider: ProviderKind::Sqlite,
            locator: crate::project::init::LOCATOR.to_owned(),
            profile: StoreProfile::Disposable,
            binding: restored.binding.clone(),
            cursor_key: restored.cursor_key,
            stack: main.branch.stack.to_bytes(),
            branch: main.branch.id.to_bytes(),
            root: main.base_root.to_bytes(),
            bytes: restored.bytes,
            host_sqlite: restored.sqlite.clone(),
            daemon_sqlite: None,
        };
        // as Init installs in place: the same file, so the same SQLite
        let mut installed = manifest.clone();
        installed.daemon_sqlite = Some(restored.sqlite.clone());
        let fields = SdkValue::map([
            ("manifest", SdkValue::Bytes(manifest.encode().ok()?)),
            ("installed", SdkValue::Bytes(installed.encode().ok()?)),
        ]);
        Some(ProjectRecord::from_record(store, &self.name, fields))
    }
}

/// [`Listing`] of the Store file `file`: read on a connection of this
/// call's own that is closed before this returns.
fn projects_in(home: &Path, store: &str, record: &StoreRecord, file: &Path) -> Result<Listing> {
    let open = crate::open_args::OpenArgs {
        home: Some(home.to_path_buf()),
        store: Some(store.to_owned()),
        store_record: Some(record.to_record().clone()),
        store_path: Some(file.display().to_string()),
        ..Default::default()
    };
    crate::service::with_service(&open, |service| {
        // the project's own line of Layers: `main`, through its name index
        let main =
            HistoryName::new(layerfs_store_service::history::branch::MAIN).map_err(failed)?;
        let projects = service
            .store
            .published(|history| {
                let mut found = Vec::new();
                for stack in layerfs_project::store::projects(history)? {
                    let snapshot = match history.branch_snapshot_named(&stack.name, &main)? {
                        NamedSnapshot::Found(snapshot) => Some(snapshot),
                        NamedSnapshot::NoStack | NamedSnapshot::NoBranch => None,
                    };
                    found.push(Found {
                        name: stack.name.as_str().to_owned(),
                        stack: SdkValue::text(pieces::hex(&stack.id.to_bytes())),
                        main: snapshot,
                    });
                }
                Ok(found)
            })
            .map_err(failed)?;
        Ok(Listing {
            projects,
            sqlite: service.opened.sqlite_version.clone(),
        })
    })
}

/// True when `record` holds what `discovery::project` reads: a `manifest`
/// and an `installed` manifest, each of which decodes.
fn complete(record: &ProjectRecord) -> bool {
    let decodes = |name: &str| match record.to_record().get(name) {
        Some(SdkValue::Bytes(bytes)) => StoreManifest::decode(bytes).is_ok(),
        _ => false,
    };
    decodes("manifest") && decodes("installed")
}

/// The project records of a restored Store, rebuilt from what it holds:
/// one complete record per project (as `project.init` writes it), a record
/// whose project the Store lacks removed, and an existing complete record
/// kept as it is. Under the home lock, after [`replaceable`] found no
/// Sandbox of the Store.
fn write_projects(
    home: &Path,
    store: &str,
    restored: &Restored,
    found: &[Found],
) -> Result<SdkValue> {
    let mut names = Vec::new();
    for project in found {
        let existing = ProjectRecord::read(home, store, &project.name)?;
        if !existing.as_ref().is_some_and(complete) {
            write_project(home, store, restored, project, existing)?;
        }
        names.push(SdkValue::text(&project.name));
    }
    for old in paths::names(&paths::store_dir(home, store).join("project")).map_err(failed)? {
        if !names.contains(&SdkValue::text(&old)) {
            std::fs::remove_dir_all(paths::project_dir(home, store, &old)).map_err(failed)?;
        }
    }
    Ok(SdkValue::List(names))
}

/// Writes one project's record: the complete one [`Found::rebuilt`] makes,
/// keeping what an existing record already holds (`created`, the name and
/// stack a listing gave it), or the listing's own when the Store's history
/// gives no `main` Branch to name.
fn write_project(
    home: &Path,
    store: &str,
    restored: &Restored,
    project: &Found,
    existing: Option<ProjectRecord>,
) -> Result<()> {
    let Some(rebuilt) = project.rebuilt(store, restored) else {
        return match existing {
            Some(_) => Ok(()),
            None => ProjectRecord::found(store, &project.name, project.stack.clone()).write(home),
        };
    };
    let mut record = match existing {
        Some(old) => old.with_fields_of(&rebuilt),
        None => rebuilt,
    };
    // the record's own time: a restored project's is when it was restored
    if record.created().is_none() {
        record = record.created_now();
    }
    record.write(home)
}
