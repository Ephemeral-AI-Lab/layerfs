//! Pull: download the Store named `NAME` to a temporary sibling of the local
//! Store (on its filesystem, so the swap is one rename), verify its sha256
//! (and each piece's), check it opens with the Store's binding, then rename
//! it over the local Store, which happens only when no opener holds it. The
//! whole Store is replaced, with every project in it.
//!
//! A clone (`clone_store`) is the same download into a new Store: the
//! destination must be absent, the binding comes from the remote copy, and
//! the file is linked into place so a Store that appeared meanwhile is never
//! replaced.
//!
//! Like the sealed-Store install (`install.rs`), the destination must pass
//! the kernel guard and the installer must be the daemons' user. One
//! attempt: a failure keeps the temporary file it left, named in its report.
//!
//! The downloaded Store's history heads digest is read before it is
//! installed (#506 E2). Progress (#506 E4) reports `check` (pull only),
//! `download` (bytes received), `verify` and `install`.
use crate::bootstrap::open_store_observed;
use crate::install::{installer_user, paths};
use crate::kernel_guard;
use crate::remote::heads::heads;
use crate::remote::push::BINDING;
use crate::remote::settings::store_object;
use layerfs_persistence::{PersistenceConfig, SqlitePersistenceProfile};
use layerfs_remote_storage::pieces::{self, hex};
use layerfs_remote_storage::{progress, Driver};
use layerfs_storage::ReservationBlocks;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// SQLite's sidecars of a Store in WAL mode. On Linux, where a pull runs,
/// each exists while the Store is open and the last opener to close removes
/// both. (macOS's system SQLite keeps empty ones; the kernel guard refuses
/// a pull there before this check.)
const SIDECARS: [&str; 2] = ["-wal", "-shm"];

/// What a pull installed.
#[derive(Debug)]
pub struct Pulled {
    pub bytes: u64,
    pub sha256: [u8; 32],
    pub pieces: u64,
    /// Whether a local Store was replaced (else the destination was absent).
    pub replaced: bool,
    /// The installed Store's history heads digest, when it could be read.
    pub heads: Option<[u8; 32]>,
}

/// What a clone installed: a new Store and the binding it opens with.
#[derive(Debug)]
pub struct Cloned {
    /// The Store's binding key, read from the remote copy.
    pub binding: Vec<u8>,
    pub bytes: u64,
    pub sha256: [u8; 32],
    pub pieces: u64,
    /// The installed Store's history heads digest, when it could be read.
    pub heads: Option<[u8; 32]>,
}

/// A failed pull or clone: its cause and the temporary file it left, if any.
#[derive(Debug)]
pub struct PullFailure {
    pub cause: String,
    pub left: Option<PathBuf>,
}

impl std::fmt::Display for PullFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Store pull: {}", self.cause)?;
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

/// Pulls the Store named `name` from `driver` into `destination` (a Store
/// volume's `global/store.sqlite`), which opens with `binding` and
/// `cursor_key`. An existing local Store is replaced only when no opener
/// holds it, checked before the download and again before the rename;
/// nothing here prevents a new opener in between.
pub fn pull(
    driver: &dyn Driver,
    name: &str,
    destination: &Path,
    binding: &[u8],
    cursor_key: [u8; 32],
) -> Result<Pulled, PullFailure> {
    fetch(driver, name, destination, Some(binding), cursor_key).map(|(fetched, replaced)| Pulled {
        bytes: fetched.bytes,
        sha256: fetched.sha256,
        pieces: fetched.pieces,
        replaced,
        heads: fetched.heads,
    })
}

/// Clones the Store named `name` from `driver` into the new Store
/// `destination`, which must not exist. The binding comes from the remote
/// copy (recorded by push); `cursor_key` is the new Store's own cursor
/// authority, held by its caller as for any Store. A Store that appears at
/// `destination` meanwhile is never replaced.
pub fn clone_store(
    driver: &dyn Driver,
    name: &str,
    destination: &Path,
    cursor_key: [u8; 32],
) -> Result<Cloned, PullFailure> {
    fetch(driver, name, destination, None, cursor_key).map(|(fetched, _)| fetched)
}

/// The remote copy's recorded binding, if push recorded one.
fn remote_binding(metadata: &layerfs_remote_storage::Metadata) -> Result<Option<Vec<u8>>, String> {
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

/// Downloads, verifies, opens and installs. With `binding`, replaces an
/// unopened local Store (pull); without it, installs a new Store (clone).
/// Returns what was installed and whether a Store was replaced.
fn fetch(
    driver: &dyn Driver,
    name: &str,
    destination: &Path,
    binding: Option<&[u8]>,
    cursor_key: [u8; 32],
) -> Result<(Cloned, bool), PullFailure> {
    let mut left = None;
    let result = (|| {
        let text = |error: io::Error| error.to_string();
        let object = store_object(name)?;
        let directory = destination
            .parent()
            .ok_or("the Store destination has no directory")?;
        // as install_local: the volume holding the Store's directory
        kernel_guard::check(directory).map_err(|reason| reason.0)?;
        installer_user()?;
        if !directory.exists() {
            use std::os::unix::fs::DirBuilderExt;
            fs::DirBuilder::new()
                .mode(0o700)
                .create(directory)
                .map_err(text)?;
        }
        let (target, _) = paths(destination).map_err(text)?;
        let cloning = binding.is_none();
        if cloning && exists(&target).map_err(text)? {
            return Err(format!(
                "a Store exists at {}; a clone makes a new Store (pull replaces one)",
                target.display()
            ));
        }
        refuse_open(&target)?;
        if !cloning {
            progress::begin("check", None);
        }
        // the copy's recorded binding: a clone's, and a pull's early refusal
        let stat = driver
            .stat(&object)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("no Store {name} on the remote: remote file missing"))?;
        let recorded = remote_binding(&stat.metadata)?;
        let binding = match (binding, recorded) {
            (Some(expected), Some(recorded)) if recorded != expected => {
                return Err(format!(
                    "the remote Store {name} has another binding ({}), not this Store's",
                    hex(&recorded)
                ));
            }
            (Some(expected), _) => expected.to_vec(),
            (None, Some(recorded)) => recorded,
            (None, None) => {
                return Err(format!(
                    "the remote Store {name} records no {BINDING}; push it again to clone it"
                ));
            }
        };
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
        let opened = open_store_observed(
            PersistenceConfig::sqlite(&temporary)
                .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
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
        let replaced = if cloning {
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
            false
        } else {
            let replaced = exists(&target).map_err(text)?;
            refuse_open(&target)?;
            fs::rename(&temporary, &target).map_err(text)?;
            left = None;
            replaced
        };
        Ok((
            Cloned {
                binding,
                bytes: moved.bytes,
                sha256: moved.sha256,
                pieces: moved.pieces,
                heads: digest,
            },
            replaced,
        ))
    })();
    result.map_err(|cause: String| PullFailure {
        cause,
        left: left.clone(),
    })
}
