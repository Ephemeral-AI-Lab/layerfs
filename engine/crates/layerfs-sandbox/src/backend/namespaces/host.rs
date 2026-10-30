//! What this host must provide before a namespace Sandbox may start, and the
//! two host facts its lifecycle reads: whether user namespaces are blocked
//! and which cgroup a process is in. Every condition is checked before any
//! effect; a host that cannot meet them is refused, never quietly given a
//! weaker Sandbox.
use crate::RuntimeError;
use std::{fs, os::unix::fs::FileTypeExt, path::Path};

/// The kernel's limit on user namespaces; `0` blocks them host-wide.
const MAX_USER_NAMESPACES: &str = "/proc/sys/user/max_user_namespaces";
/// This host's unified cgroup v2 hierarchy.
const CGROUP_ROOT: &str = "/sys/fs/cgroup";

/// The init stage must create mount, PID and IPC namespaces and mount the
/// daemon's private root, which needs the real root.
pub(crate) fn require_root() -> Result<(), RuntimeError> {
    if nix::unistd::Uid::effective().is_root() {
        Ok(())
    } else {
        Err(RuntimeError::Unsupported(
            "the namespaces Sandbox runtime runs its daemon as root (mount, PID and IPC namespaces, \
             /dev/fuse); start it as root inside the project's VM",
        ))
    }
}

/// Refuses to start when a command could create a user namespace: it would
/// hold its own copy of the Workspace mount, outside the daemon's unmount.
/// The block is host-wide (`user.max_user_namespaces = 0`), set by whoever
/// owns the VM; this runtime never sets it.
pub(crate) fn require_user_namespaces_blocked() -> Result<(), RuntimeError> {
    let text = fs::read_to_string(MAX_USER_NAMESPACES).map_err(|_| {
        RuntimeError::Unsupported(
            "the namespaces Sandbox runtime cannot read /proc/sys/user/max_user_namespaces, so it \
             cannot prove that user namespaces are blocked",
        )
    })?;
    match text.trim().parse::<u64>() {
        Ok(0) => Ok(()),
        Ok(_) => Err(RuntimeError::Unsupported(
            "user namespaces are not blocked; a command could unshare one and keep its own copy of \
             the Workspace mount. Set user.max_user_namespaces=0 in the project's VM before a \
             namespaces Sandbox starts",
        )),
        Err(_) => Err(RuntimeError::Unsupported(
            "the namespaces Sandbox runtime cannot parse /proc/sys/user/max_user_namespaces, so it \
             cannot prove that user namespaces are blocked",
        )),
    }
}

/// The unified cgroup v2 hierarchy, which ends a Sandbox as a whole.
pub(crate) fn require_cgroup_v2() -> Result<(), RuntimeError> {
    let controllers = Path::new(CGROUP_ROOT).join("cgroup.controllers");
    match fs::read_to_string(&controllers) {
        Ok(_) => Ok(()),
        Err(_) => Err(RuntimeError::Unsupported(
            "the namespaces Sandbox runtime needs the unified cgroup v2 hierarchy at /sys/fs/cgroup",
        )),
    }
}

/// The daemon serves FUSE from inside the Sandbox.
pub(crate) fn require_fuse() -> Result<(), RuntimeError> {
    match fs::metadata("/dev/fuse") {
        Ok(metadata) if metadata.file_type().is_char_device() => Ok(()),
        _ => Err(RuntimeError::Unsupported(
            "the namespaces Sandbox runtime needs a character device /dev/fuse in the project's VM",
        )),
    }
}

/// The cgroup of `pid`, as the kernel reports it, e.g. `0::/layerfs/<id>`.
pub(crate) fn cgroup_of(pid: u32) -> std::io::Result<String> {
    fs::read_to_string(format!("/proc/{pid}/cgroup"))
}

/// Whether the live process `pid` is in `directory`'s cgroup: its recorded
/// start time can outlive a reboot-free id reuse, but its cgroup membership
/// cannot be inherited from a sandbox that never made it.
pub(crate) fn in_cgroup(pid: u32, directory: &Path) -> bool {
    let Ok(table) = cgroup_of(pid) else {
        return false;
    };
    let relative = directory
        .strip_prefix(CGROUP_ROOT)
        .ok()
        .and_then(|r| r.to_str());
    let Some(relative) = relative else {
        return false;
    };
    let suffix = format!("/{relative}");
    (table.lines()).any(|line| line.ends_with(&suffix))
}
