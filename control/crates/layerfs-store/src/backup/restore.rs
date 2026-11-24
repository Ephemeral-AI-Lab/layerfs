//! `backup.restore`: a backup as a sqlite Store on the machine whose calls
//! run in this process (`native` on macOS and Linux).
use super::family::write_link;
use super::progress::Shown;
use super::record::{BucketRecord, StoreLink};
use super::restored::{records, Found, Restored};
use crate::get::hex_text;
use crate::identity::{Identity, Kind};
use crate::intent;
use crate::lease;
use crate::record::{StoreKind, StoreRecord};
use crate::Related;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::sdk_values::hex;
use layerfs_bucket::backup as remote;
use layerfs_home::events::Events;
use layerfs_home::home::flock::{try_lock, unlock};
use layerfs_home::home::{self, paths};
use layerfs_home::op::Op;
use layerfs_home::outcome::{failed, refused, Outcome};
use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// `backup.restore(bucket, file, store)`: the same three names as a
/// create's (`file` is always given by the call).
pub type BackupRestore = super::create::BackupCreate;

/// `backup.restore(bucket, file, store)`: the backup as a sqlite Store on
/// the machine whose calls run in this process ([`here`]): a new Store when
/// `store` does not exist (outcome `created`), else it replaces an idle
/// Store on that machine unchanged since its last
/// backup or restore (outcome `replaced`; `Conflict` otherwise; `InUse`
/// while a process holds it or a Sandbox of its projects runs). A replace
/// is the Store's `op` (`restore`) from its check to its end, uncertain
/// from just before the install's one rename. The download, its checks and
/// the read of its projects happen before the home lock; the lock covers
/// the re-checks, the planning of each project's record, the install and
/// the record writes only. Phases `check`, `download` (bytes), `verify`,
/// `install`.
pub fn restore(
    home: &Path,
    events: &mut Events<'_>,
    request: &BackupRestore,
    related: Related,
) -> Outcome {
    let home = home.to_path_buf();
    let store = request.store.as_str();
    layerfs_home::names::StoreName::parse(store)?;
    here()?;
    // the Store's own directory, before this call writes anything (its
    // staging folder included): a restore installs into the folder a daemon
    // opens, so an existing one must be the same private directory
    // `store.create` and every opener require, refused with its own code and
    // remedy (#570 P1). Absent: the install makes it `0700`, as before.
    if paths::store_dir(&home, store).exists() {
        layerfs_store_service::kernel_guard::check(&paths::store_file(&home, store))
            .map_err(|reason| refused("Unsupported", reason.0))?;
    }
    let bucket = request.bucket.as_str();
    let driver = BucketRecord::of(&home, bucket)?.driver(&home)?;
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
    let mut op = None;
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
            // what a killed `backup create` of it left goes (spec v20)
            super::create::stale_here(&home, store);
            let args = SdkValue::map([
                ("bucket", SdkValue::text(bucket)),
                ("file", SdkValue::text(&file)),
            ]);
            op = Some(intent::begin(&home, store, intent::RESTORE, "check", args)?);
            Some(identity)
        }
    };
    let ran = run(
        &home,
        events,
        store,
        bucket,
        &file,
        &existing,
        identity,
        driver.as_ref(),
        &mut op,
        related,
    );
    let (loaded, projects) = match ran {
        Ok(done) => done,
        Err(refusal) => {
            if let Some(op) = &op {
                super::ended::failed(&home, store, op, (bucket, &file, "restore"), &refusal);
            }
            return Err(refusal);
        }
    };
    // the first 20; `projects_count` only when there were more
    let (projects, count) = layerfs_home::helpers::shown_names(&projects);
    Ok(SdkValue::map([
        ("ok", SdkValue::Bool(true)),
        ("projects", projects),
        ("projects_count", count),
        (
            "outcome",
            SdkValue::text(match existing {
                Some(_) => "replaced",
                None => "created",
            }),
        ),
        ("store", SdkValue::text(store)),
        ("bucket", SdkValue::text(bucket)),
        ("file", SdkValue::text(&file)),
        ("size", SdkValue::Int(loaded.bytes as i64)),
        ("sha256", hex_text(&loaded.sha256)),
        (
            "heads_sha256",
            loaded
                .heads
                .as_ref()
                .map(|heads| hex_text(heads))
                .unwrap_or_default(),
        ),
    ]))
}

/// The restore from its download to its record writes: the Store
/// downloaded, installed and linked, and the names of its projects. With
/// `op` (a replace), the link is written as `op` ends.
#[allow(clippy::too_many_arguments)]
fn run(
    home: &Path,
    events: &mut Events<'_>,
    store: &str,
    bucket: &str,
    file: &str,
    existing: &Option<StoreRecord>,
    identity: Option<Identity>,
    driver: &dyn layerfs_bucket::bucket::Driver,
    op: &mut Option<Op>,
    related: Related,
) -> Result<(remote::Loaded, Vec<String>)> {
    let cursor = match &identity {
        Some(identity) => identity.cursor_key,
        None => layerfs_home::helpers::random()?,
    };
    let mut shown = Shown::new();
    shown.phase(events, "check");
    let staging = Staging::make(home)?;
    let staged = staging.0.join(remote::STORE_FILE);
    if let Some(op) = op.as_mut() {
        intent::advance(home, store, op, "download")?;
    }
    // `download` with its bytes, `verify`, then `install`
    let loaded = shown
        .transfer(events, || remote::load(driver, file, &staged, cursor))
        // the cause alone: what the load left is in `staging`, removed below
        .map_err(|failure| refused("Failed", failure.cause))?;
    let record = match (existing, &identity) {
        (Some(record), Some(identity)) => {
            if loaded.binding != identity.binding {
                return Err(refused(
                    "Failed",
                    format!(
                        "the bucket's {file} has another binding ({}), not Store {store}'s",
                        hex(&loaded.binding)
                    ),
                ));
            }
            record.clone()
        }
        _ => StoreRecord::restored(store, here()?, &loaded.binding, &cursor),
    };
    let found = projects_in(home, store, &record, &staged)?;
    // the restored Store's own authority and the file it is installed as, as
    // `project.init` records them; the staged file becomes the installed one
    let restored = Restored {
        identity: Identity {
            kind: Kind::Sqlite,
            binding: match &identity {
                Some(identity) => identity.binding.clone(),
                None => loaded.binding.clone(),
            },
            cursor_key: cursor,
        },
        file: staged.clone(),
        sqlite: loaded
            .sqlite
            .clone()
            .unwrap_or_else(|| found.sqlite.clone()),
    };
    // what each project's record becomes is decided under the home lock the
    // record writes take, and before the install: a record that cannot be
    // rebuilt refuses the restore with the Store not yet installed, rather
    // than installing one and leaving a record short
    shown.phase(events, "install");
    let _lock = home::lock::take(home)?;
    let projects = records(home, store, &restored, &found.projects)?;
    let link = StoreLink::new(
        file,
        "restore",
        loaded.bytes,
        &loaded.sha256,
        loaded.heads.as_ref(),
    );
    if let Some(op) = op.as_mut() {
        // from here a killed restore may or may not have replaced the Store:
        // its `op` is uncertain, so a gone owner leaves it unresolved
        intent::advance(home, store, op, "install")?;
        *op = op.clone().as_uncertain();
        intent::replace(home, store, op.id(), Some(op))?;
    }
    install(
        home,
        store,
        bucket,
        existing.as_ref(),
        &record,
        &staged,
        related,
    )?;
    match op.take() {
        Some(op) => super::ended::finished(home, store, &op, bucket, &link)?,
        None => write_link(home, store, bucket, link)?,
    }
    let projects = projects.write(home)?;
    let names = match projects {
        SdkValue::List(names) => names
            .iter()
            .filter_map(|name| match name {
                SdkValue::Text(name) => Some(name.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    Ok((loaded, names))
}

/// Installs the downloaded Store `staged` as `store` (under the home lock):
/// a new Store where `existing` is `None`, else it replaces the Store that
/// record names.
///
/// The target's folder is checked again first: the download is a long
/// window, in which `store/<name>` can appear or lose its privacy after the
/// restore's own first check, and `home::private_dir` keeps an existing
/// folder's mode. This is `store.create`'s guard, its code and its remedy
/// (#570 P1).
pub fn install(
    home: &Path,
    store: &str,
    bucket: &str,
    existing: Option<&StoreRecord>,
    record: &StoreRecord,
    staged: &Path,
    related: Related,
) -> Result<()> {
    if paths::store_dir(home, store).exists() {
        layerfs_store_service::kernel_guard::check(&paths::store_file(home, store))
            .map_err(|reason| refused("Unsupported", reason.0))?;
    }
    // a Store replaced here is checked and replaced under its lock, which
    // the link below takes again
    let store_lock = match &existing {
        Some(_) => Some(crate::record::lock(home, store)?),
        None => None,
    };
    let destination = paths::store_file(home, store);
    let now = StoreRecord::read(home, store)?;
    match (&existing, now) {
        (None, None) if !destination.exists() => {
            home::private_dir(&paths::store_dir(home, store))?;
            // a link never replaces a Store that appeared meanwhile
            std::fs::hard_link(staged, &destination).map_err(failed)?;
            record.write(home)?;
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
            replaceable(home, store, bucket, &now, related)?;
            std::fs::rename(staged, &destination).map_err(failed)?;
        }
    }
    drop(store_lock);
    Ok(())
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
        let dir = parent.join(format!(".restore-{}", hex(&random[..12])));
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

/// The machine a restore's Store is on: the one whose calls run in this
/// process, which downloads and installs it; `Unsupported` on a host with
/// none, and no other machine is chosen.
fn here() -> Result<&'static str> {
    layerfs_machine::in_process().ok_or_else(|| layerfs_machine::none_in_process("backup.restore"))
}

/// Refuses replacing Store `store` unless it is an idle sqlite Store on
/// the machine whose calls run in this process ([`here`]), unchanged since
/// its last backup or restore through `bucket`. Under the home lock and
/// the Store's lock.
fn replaceable(
    home: &Path,
    store: &str,
    bucket: &str,
    record: &StoreRecord,
    related: Related,
) -> Result<()> {
    let (machine, here) = (record.machine()?, here()?);
    if machine != here {
        return Err(refused("Unsupported", format!("Store {store} is on machine {machine}; a restore replaces a Store on machine {here}, whose calls run in this process: move it there first")));
    }
    // a replace's own `op` (this process's restore) holds nothing here
    if !record.op().is_some_and(|op| op.mine()) {
        record.unblocked()?;
    }
    let destination = paths::store_file(home, store);
    // this process's own open copy closes first
    crate::open::close(&destination)?;
    (related.store_idle)(home, store)?;
    let linked = record.link(bucket).and_then(|link| link.heads());
    let now = remote::heads(&destination).ok().map(|h| hex(&h));
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
        // each project's own line of Layers: `main`, through its name index
        let projects = service
            .store
            .published(super::restored::found_in)
            .map_err(failed)?;
        Ok(Listing {
            projects,
            sqlite: service.opened.sqlite_version.clone(),
        })
    })
}
