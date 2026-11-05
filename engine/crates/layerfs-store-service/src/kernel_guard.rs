//! Whether this process may open the global Store directly (#352 F-020).
//!
//! Every opener of one Store must share one Linux kernel and the local
//! filesystem that holds it: SQLite's WAL locks and the writers' admission
//! lane (a `flock` on the Store's directory) coordinate only within one
//! kernel. The guard checks what this process can verify before it opens any
//! file: the platform, and the type of the filesystem holding the Store's
//! directory, from an allowlist of local filesystems. Anything else is
//! `Unsupported` with its reason; there is no other route to fall back to.
use std::path::Path;

/// Filesystems that carry the Store's locks: ext4, xfs and btrfs.
#[cfg(target_os = "linux")]
const LOCAL: [(nix::sys::statfs::FsType, &str); 3] = [
    (nix::sys::statfs::EXT4_SUPER_MAGIC, "ext4"),
    (nix::sys::statfs::XFS_SUPER_MAGIC, "xfs"),
    (nix::sys::statfs::BTRFS_SUPER_MAGIC, "btrfs"),
];

/// Why direct Store access is refused here: the reason of `Unsupported`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Unsupported(pub String);

impl std::fmt::Display for Unsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Ensures `directory` exists (creating it `0700` when missing) and satisfies
/// the daemon's private-directory requirements: a real directory (not a
/// symlink) owned by the current effective user with no group/other access
/// (`mode & 0o077 == 0`). Existing directories' permissions are never modified.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn ensure_private_directory(directory: &Path) -> Result<(), Unsupported> {
    use std::fs;
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    let euid = nix::unistd::geteuid().as_raw();
    let meta = match fs::symlink_metadata(directory) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(directory)
                .map_err(|create_err| {
                    Unsupported(format!(
                        "the Store's directory {} cannot be examined: {error} (create 0700 failed: {create_err})",
                        directory.display()
                    ))
                })?;
            fs::symlink_metadata(directory).map_err(|stat_err| {
                Unsupported(format!(
                    "the Store's directory {} cannot be examined: {stat_err}",
                    directory.display()
                ))
            })?
        }
        Err(error) => {
            return Err(Unsupported(format!(
                "the Store's directory {} cannot be examined: {error}",
                directory.display()
            )));
        }
    };
    let path = directory.display();
    if meta.file_type().is_symlink() {
        return Err(Unsupported(format!(
            "the Store's directory {path} is a symlink; a real private directory (mode 0700) is required"
        )));
    }
    if !meta.is_dir() {
        return Err(Unsupported(format!(
            "the Store's directory {path} is not a directory (mode {:04o}); a private directory (mode 0700) is required",
            meta.mode() & 0o7777
        )));
    }
    if meta.uid() != euid {
        return Err(Unsupported(format!(
            "the Store's directory {path} is owned by uid {}, not the current user (uid {euid}); \
             the daemon requires a private directory owned by uid {euid} (mode 0700)",
            meta.uid()
        )));
    }
    let mode = meta.mode() & 0o7777;
    if mode & 0o077 != 0 {
        return Err(Unsupported(format!(
            "the Store's directory {path} has mode {mode:04o} (group/other bits {:04o} are set); \
             the daemon requires a private directory (mode 0700, no group/other access); \
             fix with: chmod 700 {path}",
            mode & 0o077
        )));
    }
    Ok(())
}

/// The directory Checked by the guard: `store` directly when it is already an
/// existing directory (or has no file component), otherwise `store.parent()`
/// when `store` names the `store.sqlite` file inside its directory.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn store_directory(store: &Path) -> &Path {
    if store.is_dir() {
        return store;
    }
    let is_store_file = store
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| name.starts_with("store.sqlite") || name.ends_with(".sqlite"));
    if is_store_file {
        store
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
    } else {
        store
    }
}

/// Checks the platform, private-directory ownership/mode (creating `store`'s
/// directory `0700` when missing) and the filesystem holding `store`'s
/// directory, before anything is opened. Returns the filesystem's name.
pub fn check(store: &Path) -> Result<&'static str, Unsupported> {
    #[cfg(target_os = "linux")]
    {
        let directory = store_directory(store);
        ensure_private_directory(directory)?;
        let stat = nix::sys::statfs::statfs(directory).map_err(|error| {
            Unsupported(format!(
                "the Store's directory {} cannot be examined: {error}",
                directory.display()
            ))
        })?;
        let kind = stat.filesystem_type();
        LOCAL
            .iter()
            .find(|(magic, _)| *magic == kind)
            .map(|(_, name)| *name)
            .ok_or_else(|| {
                Unsupported(format!(
                    "filesystem 0x{:x} at {} does not carry SQLite locks between kernels; \
                     direct Store access needs ext4, xfs or btrfs",
                    kind.0,
                    directory.display()
                ))
            })
    }
    // macOS native (the `native` machine): the daemon, the SDK and every
    // command share this one macOS kernel, so the Store's locks coordinate
    // exactly as on Linux; only a local filesystem may hold the Store.
    #[cfg(target_os = "macos")]
    {
        let directory = store_directory(store);
        ensure_private_directory(directory)?;
        let stat = nix::sys::statfs::statfs(directory).map_err(|error| {
            Unsupported(format!(
                "the Store's directory {} cannot be examined: {error}",
                directory.display()
            ))
        })?;
        match stat.filesystem_type_name() {
            "apfs" => Ok("apfs"),
            "hfs" => Ok("hfs"),
            other => Err(Unsupported(format!(
                "filesystem {other} at {} does not carry SQLite locks between kernels; \
                 direct Store access needs apfs or hfs",
                directory.display()
            ))),
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = store;
        Err(Unsupported(
            "direct Store access needs the daemons' Linux kernel".to_owned(),
        ))
    }
}
