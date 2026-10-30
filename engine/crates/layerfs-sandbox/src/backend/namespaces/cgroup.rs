//! One cgroup v2 directory per Sandbox: the whole Sandbox ends with one
//! write to `cgroup.kill`, and nothing it started can leave that group, so a
//! process that left its process group still ends with it. The hierarchy is
//! the host's unified one; there is no second, nested controller setup.
use crate::ContainerId;
use std::{
    fs,
    io::{self, Write},
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
};

/// This host's unified cgroup v2 hierarchy.
pub(crate) const ROOT: &str = "/sys/fs/cgroup";
/// The directory this runtime owns under it, one child per Sandbox.
const GROUP: &str = "layerfs";

/// Sandbox `id`'s cgroup directory.
pub(crate) fn path(id: &ContainerId) -> PathBuf {
    Path::new(ROOT).join(GROUP).join(id.as_str())
}

/// Makes this runtime's own directory once (idempotent) and returns Sandbox
/// `id`'s directory, made here and empty, with the files this runtime needs.
pub(crate) fn create(id: &ContainerId) -> io::Result<PathBuf> {
    let parent = Path::new(ROOT).join(GROUP);
    match fs::DirBuilder::new().mode(0o755).create(&parent) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    let directory = path(id);
    fs::DirBuilder::new().mode(0o755).create(&directory)?;
    // a kernel without cgroup.kill (before 5.14) cannot end a Sandbox as a whole
    if !directory.join("cgroup.kill").exists() {
        let _ = fs::remove_dir(&directory);
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "this kernel's cgroup v2 has no cgroup.kill",
        ));
    }
    Ok(directory)
}

/// Ends every process of the Sandbox with one write: the kernel sends
/// `SIGKILL` to all of them, descendants included, however they left their
/// process groups.
pub(crate) fn kill(directory: &Path) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .open(directory.join("cgroup.kill"))?;
    file.write_all(b"1")
}

/// The number of processes the Sandbox's group still has.
pub(crate) fn populated(directory: &Path) -> io::Result<u64> {
    let events = fs::read_to_string(directory.join("cgroup.events"))?;
    let mut count = None;
    for line in events.lines() {
        if let Some(value) = line.strip_prefix("populated ") {
            count = value.trim().parse().ok();
        }
    }
    count.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "cgroup.events populated"))
}

/// Removes the Sandbox's directory once nothing is in it. A group that still
/// has processes is refused rather than reported as gone.
pub(crate) fn remove(directory: &Path) -> io::Result<()> {
    match populated(directory) {
        Ok(0) => {}
        Ok(_) => {
            return Err(io::Error::other(
                "a process of the namespaces Sandbox still runs after SIGKILL",
            ))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    }
    match fs::remove_dir(directory) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}
