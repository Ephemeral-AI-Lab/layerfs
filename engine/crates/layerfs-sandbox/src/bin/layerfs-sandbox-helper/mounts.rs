//! The Sandbox's private root: a tmpfs tree holding only what its daemon and
//! its commands need, with the Sandbox's own directory and the Store's
//! directory bound at their own paths so every path keeps one meaning on both
//! sides of the pivot. Nothing the Sandbox mounts reaches the VM's own mount
//! tree: the init stage made this mount namespace private first.
//!
//! The userland (`/usr`, `/bin`, `/sbin`, `/lib`, `/lib64`, `/etc`) is the
//! VM's own, bound read-only. There is no image: deploying a namespaces
//! Sandbox means the VM that already holds the daemon and its libraries.
use layerfs_sandbox::backend::HelperSpec;
use nix::{
    mount::{mount, umount2, MntFlags, MsFlags},
    unistd::chdir,
};
use std::{
    fs, io,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::{Path, PathBuf},
};

/// The VM userland bound read-only inside the Sandbox; each is skipped when
/// this host has no such path.
const USERLAND: [&str; 6] = ["/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc"];

/// The devices a daemon and ordinary commands need, bound from this host's
/// `/dev`; nothing else of `/dev` is visible.
const DEVICES: [&str; 7] = ["fuse", "null", "zero", "full", "random", "urandom", "tty"];

/// Builds the private root `spec.root` (a tmpfs) and everything inside it.
pub(super) fn build(spec: &HelperSpec) -> Result<(), String> {
    fs::set_permissions(&spec.root, fs::Permissions::from_mode(0o755))
        .map_err(|error| format!("{}: {error}", spec.root.display()))?;
    tmpfs(&spec.root, MsFlags::empty(), "mode=0755")?;
    for source in USERLAND {
        let source = Path::new(source);
        if fs::symlink_metadata(source).is_err() {
            continue;
        }
        let target = inside(&spec.root, source)?;
        bind(source, &target, true)?;
    }
    let proc_directory = inside(&spec.root, Path::new("/proc"))?;
    mount(
        None::<&Path>,
        &proc_directory,
        Some("proc"),
        MsFlags::MS_NOSUID | MsFlags::MS_NODEV | MsFlags::MS_NOEXEC,
        None::<&str>,
    )
    .map_err(|error| format!("mounting {}: {error}", proc_directory.display()))?;
    devices(&inside(&spec.root, Path::new("/dev"))?)?;
    let scratch = inside(&spec.root, Path::new("/tmp"))?;
    tmpfs(
        &scratch,
        MsFlags::MS_NOSUID | MsFlags::MS_NODEV,
        "mode=1777",
    )?;
    // the daemon's setup, overlay, Workspace mounts and `tmp`
    bind(&spec.sandbox, &inside(&spec.root, &spec.sandbox)?, false)?;
    // the shared Store's directory: SQLite's WAL and the writers' directory
    // lock see the same inode as every other opener on this kernel
    bind(&spec.store, &inside(&spec.root, &spec.store)?, false)?;
    Ok(())
}

/// Pivots to `root`, so the VM's own tree is no longer reachable from the
/// Sandbox, and drops the old root.
pub(super) fn pivot(root: &Path) -> Result<(), String> {
    // the private root must be a mount point of its own, which the tmpfs is
    chdir(root).map_err(|error| format!("{}: {error}", root.display()))?;
    nix::unistd::pivot_root(Path::new("."), Path::new("."))
        .map_err(|error| format!("pivoting to the Sandbox's root: {error}"))?;
    umount2(Path::new("."), MntFlags::MNT_DETACH)
        .map_err(|error| format!("dropping the VM's root: {error}"))?;
    chdir("/").map_err(|error| format!("entering the Sandbox's root: {error}"))
}

/// A private `/dev`: the basic devices, a `devpts` of its own for pseudo
/// terminals, and a `shm` directory for what expects one.
fn devices(dev: &Path) -> Result<(), String> {
    tmpfs(dev, MsFlags::MS_NOSUID | MsFlags::MS_NOEXEC, "mode=0755")?;
    let host = Path::new("/dev");
    for name in DEVICES {
        let source = host.join(name);
        if fs::symlink_metadata(&source).is_err() {
            continue;
        }
        // a bind needs its target to exist; the device then replaces this
        // placeholder in the private tmpfs
        let target = dev.join(name);
        fs::write(&target, b"").map_err(|error| format!("{}: {error}", target.display()))?;
        bind(&source, &target, false)?;
    }
    let pts = dev.join("pts");
    directory(&pts, 0o755)?;
    mount(
        None::<&Path>,
        &pts,
        Some("devpts"),
        MsFlags::MS_NOSUID | MsFlags::MS_NOEXEC,
        Some("newinstance,ptmxmode=0666,mode=0620"),
    )
    .map_err(|error| format!("mounting {}: {error}", pts.display()))?;
    let shm = dev.join("shm");
    directory(&shm, 0o755)?;
    tmpfs(&shm, MsFlags::MS_NOSUID | MsFlags::MS_NODEV, "mode=1777")?;
    for (name, target) in [
        ("ptmx", "pts/ptmx"),
        ("fd", "/proc/self/fd"),
        ("stdin", "/proc/self/fd/0"),
        ("stdout", "/proc/self/fd/1"),
        ("stderr", "/proc/self/fd/2"),
    ] {
        let link = dev.join(name);
        let _ = fs::remove_file(&link);
        std::os::unix::fs::symlink(target, &link)
            .map_err(|error| format!("{}: {error}", link.display()))?;
    }
    Ok(())
}

/// `path`'s place inside `root`, made: `/a/b` becomes `<root>/a/b`.
fn inside(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let relative = path
        .strip_prefix("/")
        .map_err(|_| format!("{} is not an absolute runtime path", path.display()))?;
    let target = root.join(relative);
    fs::create_dir_all(&target).map_err(|error| format!("{}: {error}", target.display()))?;
    Ok(target)
}

/// A tmpfs at `target`, so nothing it holds outlives the namespaces.
fn tmpfs(target: &Path, flags: MsFlags, data: &str) -> Result<(), String> {
    mount(
        Some("tmpfs"),
        target,
        Some("tmpfs"),
        flags | MsFlags::MS_NOSUID,
        Some(data),
    )
    .map_err(|error| format!("mounting a tmpfs at {}: {error}", target.display()))
}

/// `source` at `target`, read-only when `read_only`. A read-only bind makes
/// the top of that mount read-only; a nested mount inside it (this VM has
/// none under the userland) is not remounted by this step.
fn bind(source: &Path, target: &Path, read_only: bool) -> Result<(), String> {
    mount(
        Some(source),
        target,
        None::<&str>,
        MsFlags::MS_BIND | MsFlags::MS_REC,
        None::<&str>,
    )
    .map_err(|error| {
        format!(
            "binding {} at {}: {error}",
            source.display(),
            target.display()
        )
    })?;
    if !read_only {
        return Ok(());
    }
    mount(
        None::<&Path>,
        target,
        None::<&str>,
        MsFlags::MS_BIND | MsFlags::MS_REMOUNT | MsFlags::MS_RDONLY,
        None::<&str>,
    )
    .map_err(|error| format!("making {} read-only: {error}", target.display()))
}

fn directory(path: &Path, mode: u32) -> Result<(), String> {
    if let Err(error) = fs::DirBuilder::new().mode(mode).create(path) {
        if error.kind() != io::ErrorKind::AlreadyExists {
            return Err(format!("{}: {error}", path.display()));
        }
    }
    Ok(())
}
