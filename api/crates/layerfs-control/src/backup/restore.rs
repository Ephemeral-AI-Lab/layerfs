//! `backup.restore`: a backup as a sqlite Store on `native`.
use super::family::{hex, progress, write_link};
use super::record::{BucketRecord, StoreLink};
use crate::common::context::Context;
use crate::common::home::lease;
use crate::common::home::{self, paths};
use crate::common::outcome::{failed, refused, Outcome};
use crate::project::record::ProjectRecord;
use crate::store::record::{StoreKind, StoreRecord};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_store_backup::backup as remote;
use layerfs_store_backup::bucket::pieces;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// `backup.restore(store, bucket, file?)`: the backup as a sqlite Store on
/// `native`: a new Store when `store` does not exist, else it replaces an
/// idle Store on `native` unchanged since its last backup or restore
/// (`Conflict` otherwise; `InUse` while a process holds it or a Sandbox of
/// its projects runs). The download, its checks and the read of its
/// projects happen before the home lock; the lock covers the re-checks, the
/// install and the record writes only.
pub(super) fn restore(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    let store = args.get_text("store").unwrap_or("");
    crate::common::names::StoreName::parse(store)?;
    let bucket = args.get_text("bucket").unwrap_or("");
    let driver = BucketRecord::of(&home, bucket)?.driver()?;
    let existing = StoreRecord::read(&home, store)?;
    let file = args
        .get_text("file")
        .map(str::to_owned)
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
            let identity = crate::store::identity::identity_in(record)?;
            // refused before the download too; checked again before the install
            {
                let _lock = home::lock::take(&home)?;
                let _store = crate::store::record::lock(&home, store)?;
                replaceable(&home, store, bucket, record)?;
            }
            _lease = Some(lease::hold(&home, store)?);
            Some(identity)
        }
    };
    let cursor = match &identity {
        Some(identity) => identity.cursor_key,
        None => crate::sandbox::registry::random()?,
    };
    progress(context, "download");
    let staging = Staging::make(&home)?;
    let staged = staging.0.join(remote::STORE_FILE);
    let loaded = remote::load(driver.as_ref(), &file, &staged, cursor)
        .map_err(|failure| refused("Failed", failure.to_string()))?;
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
    progress(context, "install");
    let _lock = home::lock::take(&home)?;
    // a Store replaced here is checked and replaced under its lock, which
    // the link below takes again
    let store_lock = match &existing {
        Some(_) => Some(crate::store::record::lock(&home, store)?),
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
            replaceable(&home, store, bucket, &now)?;
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
struct Staging(std::path::PathBuf);

impl Staging {
    fn make(home: &Path) -> Result<Self> {
        let random = crate::sandbox::registry::random()?;
        let dir = home
            .join("store")
            .join(format!(".restore-{}", pieces::hex(&random[..12])));
        home::private_dir(&dir)?;
        Ok(Self(dir))
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Refuses replacing Store `store` unless it is an idle sqlite Store on
/// `native`, unchanged since its last backup or restore through `bucket`.
/// Under the home lock and the Store's lock.
fn replaceable(home: &Path, store: &str, bucket: &str, record: &StoreRecord) -> Result<()> {
    let machine = record.machine()?;
    if machine != "native" {
        return Err(refused("Unsupported", format!("Store {store} is on {machine}; a restore replaces a Store on native: move it there first")));
    }
    record.unblocked()?;
    let destination = paths::store_file(home, store);
    // this process's own open copy closes first
    crate::store::open::close(&destination)?;
    crate::inventory::store_idle(home, store)?;
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
    let args = SdkValue::map([
        ("home", SdkValue::text(home.display().to_string())),
        ("store", SdkValue::text(store)),
        ("store_record", record.to_record().clone()),
        ("storePath", SdkValue::text(file.display().to_string())),
    ]);
    let SdkValue::List(items) = crate::store::identity::list_projects(&args)? else {
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
