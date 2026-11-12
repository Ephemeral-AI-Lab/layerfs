//! `backup.restore`: a backup as a sqlite Store on `native`.
use super::family::{hex, write_link};
use super::progress::Shown;
use super::record::{BucketRecord, StoreLink};
use crate::lease;
use crate::project::record::ProjectRecord;
use crate::record::{StoreKind, StoreRecord};
use crate::Related;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bucket::backup as remote;
use layerfs_bucket::bucket::pieces;
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
    let projects = write_projects(&home, store, &found)?;
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

/// `(name, stack)` of each project the Store file `file` holds, read on a
/// connection of this call's own that is closed before this returns.
fn projects_in(
    home: &Path,
    store: &str,
    record: &StoreRecord,
    file: &Path,
) -> Result<Vec<(String, SdkValue)>> {
    let open = crate::open_args::OpenArgs {
        home: Some(home.to_path_buf()),
        store: Some(store.to_owned()),
        store_record: Some(record.to_record().clone()),
        store_path: Some(file.display().to_string()),
        ..Default::default()
    };
    let SdkValue::List(items) = crate::identity::list_projects(&open)? else {
        return Err(refused("Failed", "the Store's project list is not a list"));
    };
    Ok(items
        .iter()
        .filter_map(|item| {
            let name = item.get_text("name")?.to_owned();
            Some((name, item.get("stack").cloned().unwrap_or_default()))
        })
        .collect())
}

/// The project records of a restored Store, rebuilt from what it holds:
/// one per project; a record whose project it lacks is removed. Under the
/// home lock, after [`replaceable`] found no Sandbox of the Store.
fn write_projects(home: &Path, store: &str, found: &[(String, SdkValue)]) -> Result<SdkValue> {
    let mut names = Vec::new();
    for (name, stack) in found {
        if ProjectRecord::read(home, store, name)?.is_none() {
            ProjectRecord::found(store, name, stack.clone()).write(home)?;
        }
        names.push(SdkValue::text(name));
    }
    for old in paths::names(&paths::store_dir(home, store).join("project")).map_err(failed)? {
        if !names.contains(&SdkValue::text(&old)) {
            std::fs::remove_dir_all(paths::project_dir(home, store, &old)).map_err(failed)?;
        }
    }
    Ok(SdkValue::List(names))
}
