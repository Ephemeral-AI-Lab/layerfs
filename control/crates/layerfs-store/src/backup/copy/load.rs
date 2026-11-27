//! Load (#509): download the remote Store file `FILE` to a temporary
//! sibling of the new Store (on its filesystem), verify its sha256 (and
//! each piece's), bring it to the latest format with the chain `store
//! upgrade` runs (#804; a format the chain does not take, such as schema
//! version 1, is refused in the engine's words), check it opens with the
//! binding the remote file records,
//! then link it into place, so a Store that appeared meanwhile is never
//! replaced.
//!
//! Like the sealed-Store install (`install.rs`), the destination must pass
//! the kernel guard and the installer must be the daemons' user. One
//! attempt: a failure keeps the temporary file it left, named in its report.
//!
//! The downloaded Store's history heads digest is read before it is
//! installed (#506 E2). Progress (#506 E4) reports `download` (bytes
//! received), `verify` and `install`.
use crate::backup::bucket::pieces;
use crate::backup::bucket::registry::store_object;
use crate::backup::bucket::{progress, Driver};
use crate::backup::copy::heads::heads;
use crate::backup::copy::object::BINDING;
use layerfs_persistence::{PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::ReservationBlocks;
use layerfs_store_service::store::install::{paths, prepare};
use layerfs_store_service::store::open::open_store_observed;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// SQLite's sidecars of a Store in WAL mode. On Linux, where a pull runs,
/// each exists while the Store is open and the last opener to close removes
/// both. (macOS's system SQLite keeps empty ones; the kernel guard refuses
/// a pull there before this check.)
const SIDECARS: [&str; 2] = ["-wal", "-shm"];

/// What a load installed: a new Store and the binding it opens with.
#[derive(Debug)]
pub struct Loaded {
    /// The Store's binding key, read from the remote copy.
    pub binding: Vec<u8>,
    pub bytes: u64,
    pub sha256: [u8; 32],
    pub pieces: u64,
    /// The installed Store's history heads digest, when it could be read.
    pub heads: Option<[u8; 32]>,
    /// The SQLite version that wrote the remote copy, as its push recorded
    /// it (`layerfs-sqlite`); `None` when the copy carries none.
    pub sqlite: Option<String>,
    /// What the format chain did to the copy before it was opened (#804):
    /// `upgraded` with its steps, or `current`.
    pub upgrade: layerfs_persistence::upgrade::UpgradeReport,
}

/// A failed load: its cause and the temporary file it left, if any.
#[derive(Debug)]
pub struct PullFailure {
    pub cause: String,
    pub left: Option<PathBuf>,
}

impl std::fmt::Display for PullFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Store load: {}", self.cause)?;
        if let Some(left) = &self.left {
            write!(f, " (left {})", left.display())?;
        }
        Ok(())
    }
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn exists(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// Refuses when the local Store `target` is open. Every opener (a daemon,
/// so every mounted Workspace, and an SDK process or worker) holds SQLite's
/// sidecars while it has the Store open.
fn refuse_open(target: &Path) -> Result<(), String> {
    for suffix in SIDECARS {
        if exists(&sidecar(target, suffix)).map_err(|e| e.to_string())? {
            return Err(format!(
                "the local Store {} is open (its {suffix} exists): a daemon, a mounted \
                 Workspace or an SDK or worker process holds it; stop them first (the macOS \
                 worker closes a Store after a quiet minute) and pull again",
                target.display()
            ));
        }
    }
    Ok(())
}

/// Loads the remote Store file `file` from `driver` into the new Store
/// `destination`, which must not exist. The binding comes from the remote
/// file (recorded by push); `cursor_key` is the new Store's own cursor
/// authority, held by its caller as for any Store. A Store that appears at
/// `destination` meanwhile is never replaced.
pub fn load(
    driver: &dyn Driver,
    file: &str,
    destination: &Path,
    cursor_key: [u8; 32],
) -> Result<Loaded, PullFailure> {
    fetch(driver, file, destination, cursor_key)
}

/// Runs the format chain on the private download at `path`, which no one
/// else has open: a current copy is left as it is; a refusal is the
/// engine's text. Nothing cancels it: a restore has no cancel.
fn upgrade_download(path: &Path) -> Result<layerfs_persistence::upgrade::UpgradeReport, String> {
    use layerfs_persistence::upgrade::{upgrade_sqlite, UpgradeError, UpgradeRequest};
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let request = UpgradeRequest {
        cancel: &cancel,
        failpoint: None,
    };
    upgrade_sqlite(path, &request, &mut |_| {}).map_err(|error| match error {
        UpgradeError::Unsupported(why) => format!("the pulled Store cannot be restored: {why}"),
        UpgradeError::InUse(why)
        | UpgradeError::Busy(why)
        | UpgradeError::Failed(why)
        | UpgradeError::Unknown(why) => {
            format!("the pulled Store could not be brought to the latest format: {why}")
        }
        UpgradeError::Cancelled => "the pulled Store's upgrade was cancelled".to_owned(),
    })
}

/// The remote copy's recorded SQLite version, if its upload recorded one.
fn remote_sqlite(metadata: &crate::backup::bucket::Metadata) -> Option<String> {
    metadata
        .get(crate::backup::copy::object::SQLITE_VERSION)
        .filter(|version| !version.is_empty())
        .cloned()
}

/// The remote copy's recorded binding, if its upload recorded one.
fn remote_binding(metadata: &crate::backup::bucket::Metadata) -> Result<Option<Vec<u8>>, String> {
    let Some(text) = metadata.get(BINDING) else {
        return Ok(None);
    };
    let digits = text.as_bytes();
    let value = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
    if digits.len() % 2 != 0 {
        return Err(format!("the remote Store's {BINDING} is malformed"));
    }
    digits
        .chunks(2)
        .map(|pair| Some((value(pair[0])? << 4) | value(pair[1])?))
        .collect::<Option<Vec<u8>>>()
        .map(Some)
        .ok_or_else(|| format!("the remote Store's {BINDING} is malformed"))
}

/// Downloads, verifies, opens and installs a new Store.
fn fetch(
    driver: &dyn Driver,
    name: &str,
    destination: &Path,
    cursor_key: [u8; 32],
) -> Result<Loaded, PullFailure> {
    let mut left = None;
    let result = (|| {
        let text = |error: io::Error| error.to_string();
        let object = store_object(name)?;
        // as install_local: the Store's own directory (`0700` when absent),
        // the Store file guarded; a refusal reclaims only an empty directory
        // this call made
        prepare(destination)?;
        let (target, _) = paths(destination).map_err(text)?;
        if exists(&target).map_err(text)? {
            return Err(format!(
                "a Store exists at {}; a load makes a new Store",
                target.display()
            ));
        }
        refuse_open(&target)?;
        // the file's recorded binding
        let stat = driver
            .stat(&object)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("no Store {name} on the remote: remote file missing"))?;
        let binding = remote_binding(&stat.metadata)?.ok_or_else(|| {
            format!("the remote Store {name} records no {BINDING}; back it up again to load it")
        })?;
        let temporary = sidecar(&target, ".pulling");
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary).map_err(text)?;
        left = Some(temporary.clone());
        let moved = pieces::download(driver, &object, &mut file).map_err(|e| e.to_string())?;
        file.flush().map_err(text)?;
        drop(file);
        // The verified bytes must open as this Store before they are
        // installed; closing it removes the sidecars it created.
        progress::begin("verify", None);
        // a copy of an older format is brought to the latest on this
        // private download first, by the chain `store upgrade` runs (#804);
        // one the chain does not take (schema version 1, a newer build's)
        // is refused in the engine's words
        let upgrade = upgrade_download(&temporary)?;
        let opened = open_store_observed(
            PersistenceConfig::sqlite(&temporary)
                .with_sqlite_profile(SqlitePersistenceProfile::Normal),
            &binding,
            cursor_key,
            1,
            64 * 1024,
            ReservationBlocks::default(),
        )
        .map_err(|error| format!("the pulled Store does not open: {error}"))?;
        drop(opened);
        let digest = heads(&temporary).ok();
        for suffix in SIDECARS {
            if exists(&sidecar(&temporary, suffix)).map_err(text)? {
                return Err(format!("the pulled Store kept its {suffix} after closing"));
            }
        }
        progress::begin("install", None);
        // a link fails if anything appeared at the target: never replaced
        fs::hard_link(&temporary, &target).map_err(|error| {
            format!(
                "the new Store {} could not be placed: {error}",
                target.display()
            )
        })?;
        left = None;
        fs::remove_file(&temporary).map_err(|error| {
            format!(
                "the Store is installed at {}, but its temporary name {} remains: {error}",
                target.display(),
                temporary.display()
            )
        })?;
        Ok(Loaded {
            binding,
            bytes: moved.bytes,
            sha256: moved.sha256,
            pieces: moved.pieces,
            heads: digest,
            sqlite: remote_sqlite(&stat.metadata),
            upgrade,
        })
    })();
    result.map_err(|cause: String| PullFailure {
        cause,
        left: left.clone(),
    })
}
