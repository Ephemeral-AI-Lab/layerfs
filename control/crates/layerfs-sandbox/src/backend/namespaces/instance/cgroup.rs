//! One cgroup v2 directory per Sandbox: the whole Sandbox ends with one
//! write to `cgroup.kill`, and nothing it started can leave that group, so a
//! process that left its process group still ends with it. The hierarchy is
//! the host's unified one; there is no second, nested controller setup.
//!
//! The `layerfs` directory gives each Sandbox's group the memory and pids
//! controllers its parent lets it have ([`give_controllers`],
//! layerfs-project#182 L4), so a Sandbox's memory and processes can be read.
//! No limit is ever set. Its parent (the hierarchy's root) is enabled by
//! whoever owns it before anything runs there: the real root is exempt from
//! the no-internal-process rule; a container's cgroup namespace root is
//! enabled by its launcher (the docker machine's entry script).
//!
//! A Sandbox's own group holds no process ([`create`]): its init stages,
//! daemon, command helpers and the commands with no exec id run in its leaf
//! group `init` ([`leaf`]), and the commands with an exec id in their
//! Workspace's `ws-<workspace>/exec-<exec>` group. So cgroup v2's
//! no-internal-process rule lets it give the leaf and its Workspace groups
//! the memory and pids controllers it has, and a Workspace's memory and
//! processes can be read; its own files still count all of them.
use crate::ContainerId;
use std::{
    fs,
    io::{self, Write},
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

/// This host's unified cgroup v2 hierarchy.
pub(crate) const ROOT: &str = "/sys/fs/cgroup";
/// The directory this runtime owns under it, one child per Sandbox.
const GROUP: &str = "layerfs";

/// Sandbox `id`'s cgroup directory.
pub(crate) fn path(id: &ContainerId) -> PathBuf {
    Path::new(ROOT).join(GROUP).join(id.as_str())
}

/// The controllers a Sandbox's group is given, when its parent has them.
const CONTROLLERS: [&str; 2] = ["memory", "pids"];

/// Whether this process already said that the controllers were not given.
static TOLD: AtomicBool = AtomicBool::new(false);
/// Whether it already said so of a Sandbox's Workspace groups.
static TOLD_WORKSPACES: AtomicBool = AtomicBool::new(false);

/// The name of a Sandbox group's leaf group ([`leaf`]).
const LEAF: &str = "init";

/// The leaf group below Sandbox group `sandbox` that holds every process of
/// the Sandbox outside a command group: its two init stages and daemon
/// (the helper spec's `cgroup`), each command's helper, and the commands
/// with no exec id. One `cgroup.kill` on `sandbox` still ends them all.
pub fn leaf(sandbox: &Path) -> PathBuf {
    sandbox.join(LEAF)
}

/// Makes new Sandbox group `sandbox`'s [`leaf`], then, while no process is
/// in either, gives the groups below `sandbox` the controllers it has
/// ([`give_controllers`]). A failed leaf is an error; a failed give is not:
/// the Workspaces' memory and process values stay unread, and the
/// process's log says why once.
pub fn split(sandbox: &Path) -> io::Result<()> {
    fs::DirBuilder::new().mode(0o755).create(leaf(sandbox))?;
    if let Err(error) = give_controllers(sandbox) {
        if !TOLD_WORKSPACES.swap(true, Ordering::Relaxed) {
            eprintln!(
                "layerfs: Workspace groups get no memory or pids controller \
                 (writing {} failed: {error}); their memory and process values stay null",
                sandbox.join("cgroup.subtree_control").display()
            );
        }
    }
    Ok(())
}

/// Gives the groups under `group` the controllers of [`CONTROLLERS`] that
/// `group` has (listed in its `cgroup.controllers`) and does not give yet
/// (not in its `cgroup.subtree_control`): one write of `+memory +pids`, or
/// of the one listed, to its `cgroup.subtree_control`. Nothing is written
/// when there is nothing to give. It never writes a limit and never writes
/// above `group`.
pub fn give_controllers(group: &Path) -> io::Result<()> {
    let listed = |file: &str| -> io::Result<Vec<String>> {
        let text = fs::read_to_string(group.join(file))?;
        Ok(text.split_whitespace().map(str::to_owned).collect())
    };
    let (has, given) = (
        listed("cgroup.controllers")?,
        listed("cgroup.subtree_control")?,
    );
    let wanted: Vec<String> = (CONTROLLERS.iter())
        .filter(|name| has.iter().any(|c| c == *name) && !given.iter().any(|c| c == *name))
        .map(|name| format!("+{name}"))
        .collect();
    if wanted.is_empty() {
        return Ok(());
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .open(group.join("cgroup.subtree_control"))?;
    file.write_all(wanted.join(" ").as_bytes())
}

/// Makes this runtime's own directory once (idempotent) and returns Sandbox
/// `id`'s directory, made here with its [`leaf`] and no process, with the
/// files this runtime needs ([`split`]). The directory gives Sandbox groups
/// the controllers it can ([`give_controllers`]); a failed write does not
/// fail the Sandbox: its memory and process values stay unread, and the
/// process's log (the worker's) says why once.
pub(crate) fn create(id: &ContainerId) -> io::Result<PathBuf> {
    let parent = Path::new(ROOT).join(GROUP);
    match fs::DirBuilder::new().mode(0o755).create(&parent) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    if let Err(error) = give_controllers(&parent) {
        if !TOLD.swap(true, Ordering::Relaxed) {
            eprintln!(
                "layerfs: Sandbox groups get no memory or pids controller \
                 (writing {} failed: {error}); their memory and process values stay null",
                parent.join("cgroup.subtree_control").display()
            );
        }
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
    if let Err(error) = split(&directory) {
        let _ = fs::remove_dir(&directory);
        return Err(error);
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

/// Removes the Sandbox's directory once nothing is in it, with its leaf and
/// the Workspace and command groups below it. A group that still has processes
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

/// Removes command group `directory` if nothing of it runs, then its
/// Workspace's group when that is empty too; one look, no wait. A group's
/// `populated` drops when its last process has exited, not when it is
/// reaped, but not in order with its parent being told: since Linux 6.19
/// (d245698d727a) an exiting task leaves its cgroup after `exit_notify`, at
/// its final context switch (later still on PREEMPT_RT), so the run that
/// waited for the command can find its group still counted (before 6.19 it
/// left before `exit_notify`, as 650828a686 assumed). A child signalled with
/// the command may also exit after the command did. The cancel removes the
/// group once it reports it empty (`crate::runtime::cancel`), which needs no ordering.
/// A group something still runs in (nothing cancelled it) stays, and ends
/// with its Sandbox.
///
/// KNOWN GAP (owner decision): on Linux 6.19 or later an uncancelled
/// command's one look here can leave its group, empty, until the Sandbox
/// ends; a bounded wait on `cgroup.events` in the exec return path would
/// close it at a cost in exec latency. KNOWN GAP (owner decision): removing
/// the Workspace's group can race another command of the same Workspace
/// between the two `mkdir`s of [`create_exec`] (its `ws-<id>` found, then
/// removed here, then its `exec-<id>` refused `ENOENT`): that command fails,
/// reported, never retried. Fixes: stop removing `ws-<id>` per command (the
/// Sandbox's end removes it), or a cross-process lock around a Workspace
/// group's create and remove.
pub(crate) fn remove_exec(directory: &Path) {
    if populated(directory).ok() != Some(0) || fs::remove_dir(directory).is_err() {
        return;
    }
    if let Some(workspace) = directory.parent() {
        // another command of the Workspace may still have its group
        let _ = fs::remove_dir(workspace);
    }
}

/// One command's group as its stop handle names it (`crate::runtime::cancel`).
pub(crate) struct ExecGroup<'a>(pub(crate) &'a Path);

impl crate::runtime::stop::Stoppable for ExecGroup<'_> {
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

    fn release(&self) {
        remove_exec(self.0)
    }
}
