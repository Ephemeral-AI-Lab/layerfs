//! Pull: download the remote Store to a temporary sibling of the local one,
//! verify its sha256 (and each piece's), check it opens as this project's
//! Store, then rename it over the local Store, which happens only when no
//! opener holds it.
//!
//! Like the sealed-Store install (`install.rs`), the destination must pass
//! the kernel guard and the installer must be the daemons' user. One
//! attempt: a failure keeps the temporary file it left, named in its report.
use crate::bootstrap::open_store_observed;
use crate::install::{installer_user, paths};
use crate::kernel_guard;
use crate::remote::settings::STORE_OBJECT;
use layerfs_persistence::{PersistenceConfig, SqlitePersistenceProfile};
use layerfs_remote_storage::{pieces, Driver};
use layerfs_storage::ReservationBlocks;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// SQLite's sidecars of a Store in WAL mode. Each exists while the Store is
/// open; the last opener to close removes both.
const SIDECARS: [&str; 2] = ["-wal", "-shm"];

/// What a pull installed.
#[derive(Debug)]
pub struct Pulled {
    pub bytes: u64,
    pub sha256: [u8; 32],
    pub pieces: u64,
    /// Whether a local Store was replaced (else the destination was absent).
    pub replaced: bool,
}

/// A failed pull: its cause and the temporary file it left, if any.
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

/// Pulls the Store from `driver` into `destination` (a project volume's
/// `global/store.sqlite`), which opens with `binding` and `cursor_key`.
/// An existing local Store is replaced only when no opener holds it: its
/// `-wal` and `-shm` sidecars are absent. Stop the project's daemons and
/// SDK processes first; nothing here prevents a new opener meanwhile.
pub fn pull(
    driver: &dyn Driver,
    destination: &Path,
    binding: &[u8],
    cursor_key: [u8; 32],
) -> Result<Pulled, PullFailure> {
    let mut left = None;
    let result = (|| {
        let text = |error: io::Error| error.to_string();
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
        let temporary = sidecar(&target, ".pulling");
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary).map_err(text)?;
        left = Some(temporary.clone());
        let moved = pieces::download(driver, STORE_OBJECT, &mut file).map_err(|e| e.to_string())?;
        file.flush().map_err(text)?;
        drop(file);
        // The verified bytes must open as this project's Store before they
        // replace anything; closing it removes the sidecars it created.
        let opened = open_store_observed(
            PersistenceConfig::sqlite(&temporary)
                .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
            binding,
            cursor_key,
            1,
            64 * 1024,
            ReservationBlocks::default(),
        )
        .map_err(|error| format!("the pulled Store does not open: {error}"))?;
        drop(opened);
        for suffix in SIDECARS {
            if exists(&sidecar(&temporary, suffix)).map_err(text)? {
                return Err(format!("the pulled Store kept its {suffix} after closing"));
            }
        }
        let replaced = exists(&target).map_err(text)?;
        for suffix in SIDECARS {
            if replaced && exists(&sidecar(&target, suffix)).map_err(text)? {
                return Err(format!(
                    "the local Store {} is open (its {suffix} exists); stop its openers first",
                    target.display()
                ));
            }
        }
        fs::rename(&temporary, &target).map_err(text)?;
        left = None;
        Ok(Pulled {
            bytes: moved.bytes,
            sha256: moved.sha256,
            pieces: moved.pieces,
            replaced,
        })
    })();
    result.map_err(|cause: String| PullFailure {
        cause,
        left: left.clone(),
    })
}
