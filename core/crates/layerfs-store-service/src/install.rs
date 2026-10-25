//! A sealed Store's one-time installation into a project volume (#373).
//!
//! Every installer claims the destination's exclusive temporary name, writes
//! the sealed bytes, renames it into place, opens the Store and checks its
//! initial Branch. The daemon does this for a Store streamed to it; Import
//! does it in the SDK's process ([`install_local`]), so no Sandbox is started
//! to install a Store. One attempt: a failure keeps whatever it left, named
//! in its report, and nothing is deleted or tried again.
use crate::bootstrap::{open_store_observed, OpenedStore};
use crate::store::{checked_base, PortError};
use layerfs_bridge::provision::{StoreManifest, StoreProfile};
use layerfs_content::ContentError;
use layerfs_history::{BranchId, HistoryError, LayerStackId};
use layerfs_persistence::{PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::ReservationBlocks;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

/// The daemon's user inside its Sandbox (the runtime's `User` is `0:0`). A
/// daemon opens only a Store file and directory it owns, so an installer
/// must run as this user too.
#[cfg(not(target_os = "windows"))]
const DAEMON_UID: u32 = 0;

/// The destination and its exclusive temporary name.
pub fn paths(destination: &Path) -> io::Result<(PathBuf, PathBuf)> {
    if !destination.is_absolute()
        || destination
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "absolute normalized Store destination required",
        ));
    }
    let name = destination
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Store file name"))?;
    let parent = destination
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Store parent"))?
        .canonicalize()?;
    let mut temporary = name.to_os_string();
    temporary.push(".installing");
    Ok((parent.join(name), parent.join(temporary)))
}

/// Refuses a destination that exists.
pub fn absent(destination: &Path) -> io::Result<()> {
    match fs::symlink_metadata(destination) {
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Store destination exists",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Creates the temporary file once (owner-only), while the destination is absent.
pub fn claim(destination: &Path, temporary: &Path) -> io::Result<File> {
    absent(destination)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(temporary)
}

/// Why an installed Store's initial Branch is not the sealed one.
#[derive(Debug)]
pub enum InitialCheck {
    History(HistoryError),
    Mismatch(&'static str),
    Content {
        error: ContentError,
        provider: Option<Arc<PortError>>,
    },
}

/// Checks that `opened` holds the manifest's initial Branch: its stack, root
/// and no Commit, with a readable base.
pub fn check_initial(opened: &OpenedStore, manifest: &StoreManifest) -> Result<(), InitialCheck> {
    let branch = BranchId::from_bytes(manifest.branch).map_err(InitialCheck::History)?;
    let stack = LayerStackId::from_bytes(manifest.stack).map_err(InitialCheck::History)?;
    let snapshot = opened
        .store
        .history()
        .branch_snapshot(branch)
        .map_err(InitialCheck::History)?
        .ok_or(InitialCheck::Mismatch("installed Branch missing"))?;
    if snapshot.branch.stack != stack
        || snapshot.effective_root.to_bytes() != manifest.root
        || snapshot.branch.head_commit.is_some()
    {
        return Err(InitialCheck::Mismatch("installed initial Branch"));
    }
    let ports = opened.store.ports(snapshot.scope);
    checked_base(ports.client(), &snapshot).map_err(|error| InitialCheck::Content {
        error,
        provider: ports
            .failure()
            .unwrap_or_else(|error| Some(Arc::new(error))),
    })?;
    Ok(())
}

/// A failed local install: its cause and what it left.
#[derive(Debug)]
pub struct LocalInstallFailure {
    /// The temporary file this attempt created, if it did.
    pub temporary: Option<PathBuf>,
    /// Whether the Store file was renamed into place.
    pub published: bool,
    pub cause: String,
}

impl std::fmt::Display for LocalInstallFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Store install: {}", self.cause)?;
        if let Some(temporary) = &self.temporary {
            let state = if self.published {
                "installed at"
            } else {
                "left"
            };
            write!(f, " ({state} {})", temporary.display())?;
        }
        Ok(())
    }
}

/// Installs the sealed Store `source` (described by `manifest`) at
/// `destination` in this process: the project volume's
/// `global/store.sqlite`, its `global/` directory made owner-only. The
/// destination's filesystem must pass the kernel guard, as every Store opener
/// must, and this process must run as the daemons' user. Returns the
/// installed manifest, which records this process's SQLite version.
pub fn install_local(
    source: &Path,
    manifest: &StoreManifest,
    destination: &Path,
) -> Result<StoreManifest, LocalInstallFailure> {
    let mut left = LocalInstallFailure {
        temporary: None,
        published: false,
        cause: String::new(),
    };
    let result = (|| {
        if manifest.profile != StoreProfile::Disposable || manifest.daemon_sqlite.is_some() {
            return Err("a sealed, not yet installed, Disposable Store is required".to_owned());
        }
        let directory = destination
            .parent()
            .ok_or("the Store destination has no directory")?;
        crate::kernel_guard::check(directory).map_err(|reason| reason.0)?;
        installer_user()?;
        let text = |error: io::Error| error.to_string();
        if !directory.exists() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                fs::DirBuilder::new()
                    .mode(0o700)
                    .create(directory)
                    .map_err(text)?;
            }
            #[cfg(not(unix))]
            {
                fs::DirBuilder::new().create(directory).map_err(text)?;
            }
        }
        let (target, temporary) = paths(destination).map_err(text)?;
        let mut file = claim(&target, &temporary).map_err(text)?;
        left.temporary = Some(temporary.clone());
        let mut sealed = File::open(source).map_err(text)?;
        if sealed.metadata().map_err(text)?.len() != manifest.bytes {
            return Err("the sealed file's length is not the manifest's".to_owned());
        }
        let copied = io::copy(&mut (&mut sealed).take(manifest.bytes), &mut file).map_err(text)?;
        if copied != manifest.bytes || sealed.read(&mut [0]).map_err(text)? != 0 {
            return Err("the sealed file changed while it was copied".to_owned());
        }
        file.flush().map_err(text)?;
        drop(file);
        fs::rename(&temporary, &target).map_err(text)?;
        left.published = true;
        left.temporary = Some(target.clone());
        let opened = open_store_observed(
            PersistenceConfig::sqlite(&target)
                .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
            &manifest.binding,
            manifest.cursor_key,
            1,
            64 * 1024,
            ReservationBlocks::default(),
        )
        .map_err(|error| format!("open: {error}"))?;
        check_initial(&opened, manifest).map_err(|check| format!("{check:?}"))?;
        let mut installed = manifest.clone();
        installed.daemon_sqlite = Some(opened.sqlite_version.clone());
        Ok(installed)
    })();
    result.map_err(|cause: String| LocalInstallFailure { cause, ..left })
}

/// Refuses an installer that is not the daemons' user.
fn installer_user() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        // Windows has no daemons' uid to check here; the destination's
        // local-volume discipline is the kernel guard's, which still applies.
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::MetadataExt;
        // the owner of /proc/self is this process's effective user
        let uid = fs::metadata("/proc/self")
            .map_err(|error| format!("this process's user is unknown: {error}"))?
            .uid();
        if uid != DAEMON_UID {
            return Err(format!(
                "the Store install runs as user {uid}; it must run as user {DAEMON_UID}, the                  daemons' user, which opens only a Store it owns"
            ));
        }
        Ok(())
    }
}
