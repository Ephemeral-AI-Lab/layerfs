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

/// Removes the Sandbox's directory once nothing is in it, with the
/// Workspace and command groups below it. A group that still has processes
/// is refused rather than reported as gone.
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
    remove_tree(directory)
}

/// Removes empty group `directory` and the groups below it, deepest first.
fn remove_tree(directory: &Path) -> io::Result<()> {
    let entries = match fs::read_dir(directory) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        other => other?,
    };
    for entry in entries {
        let entry = entry?;
        // a group's own files are files; its child groups are folders
        if entry.file_type()?.is_dir() {
            remove_tree(&entry.path())?;
        }
    }
    match fs::remove_dir(directory) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// The group of one command (compute-plane 2.0 §5.3), below its Sandbox's
/// group `sandbox`: `ws-<workspace>/exec-<exec>`, mirroring the home's
/// `workspace/<id>/exec/<id>.json`. One `cgroup.kill` on `ws-<workspace>`
/// ends a Workspace's commands.
pub(crate) fn exec_path(sandbox: &Path, workspace: &str, exec: &str) -> PathBuf {
    sandbox
        .join(format!("ws-{workspace}"))
        .join(format!("exec-{exec}"))
}

/// Makes command group `directory` (its Workspace's group with it), new: a
/// group of the same name with a process in it is the same command id
/// running, refused `AlreadyExists`; an empty one a command left is taken.
pub(crate) fn create_exec(directory: &Path) -> io::Result<()> {
    if let Some(workspace) = directory.parent() {
        match fs::DirBuilder::new().mode(0o755).create(workspace) {
            Err(error) if error.kind() != io::ErrorKind::AlreadyExists => return Err(error),
            _ => {}
        }
    }
    match fs::DirBuilder::new().mode(0o755).create(directory) {
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => match populated(directory)? {
            0 => Ok(()),
            _ => Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "a command with this exec id runs in the Workspace",
            )),
        },
        other => other,
    }
}

/// Removes command group `directory` once its command ended and nothing it
/// started runs, then its Workspace's group when that is empty too. A group
/// something still runs in stays, and ends with its Sandbox.
pub(crate) fn remove_exec(directory: &Path) {
    if populated(directory).ok() != Some(0) || fs::remove_dir(directory).is_err() {
        return;
    }
    if let Some(workspace) = directory.parent() {
        // another command of the Workspace may still have its group
        let _ = fs::remove_dir(workspace);
    }
}

/// One command's group as its stop handle names it (`crate::stop`).
pub(crate) struct ExecGroup<'a>(pub(crate) &'a Path);

impl crate::stop::Stoppable for ExecGroup<'_> {
    fn running(&self) -> io::Result<bool> {
        match populated(self.0) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            other => Ok(other? > 0),
        }
    }

    fn signal(&self, signal: nix::sys::signal::Signal) -> io::Result<()> {
        let procs = match fs::read_to_string(self.0.join("cgroup.procs")) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            other => other?,
        };
        for pid in procs
            .lines()
            .filter_map(|line| line.trim().parse::<i32>().ok())
        {
            let pid = nix::unistd::Pid::from_raw(pid);
            match nix::sys::signal::kill(pid, signal) {
                Ok(()) | Err(nix::errno::Errno::ESRCH) => {}
                Err(errno) => return Err(errno.into()),
            }
        }
        Ok(())
    }

    fn kill(&self) -> io::Result<()> {
        kill(self.0)
    }
}
