//! What this host must provide before a namespace Sandbox may start, and the
//! two host facts its lifecycle reads: whether user namespaces are blocked
//! (host-wide, or by the command filter) and which cgroup a process is in. Every condition is checked before any
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
/// Every command runs under the filter that refuses one
/// (`local::deny_user_namespaces`, D-8), so a kernel that can apply it is
/// accepted; so is a host that blocks user namespaces host-wide
/// (`user.max_user_namespaces = 0`, which a VM guest sets at start). This
/// runtime never sets the limit: on the docker machine it would change the
/// whole Docker host. A filter that cannot be installed later fails its
/// command; no command runs without it.
pub(crate) fn require_user_namespaces_blocked() -> Result<(), RuntimeError> {
    let limit = fs::read_to_string(MAX_USER_NAMESPACES).ok();
    if limit.and_then(|text| text.trim().parse::<u64>().ok()) == Some(0) || filter_available() {
        return Ok(());
    }
    Err(RuntimeError::Unsupported(
        "user namespaces are not blocked: user.max_user_namespaces is not 0 and this kernel cannot \
         apply the seccomp filter that refuses them to every command (its \
         /proc/sys/kernel/seccomp/actions_avail lacks allow, errno or kill_process), so a command \
         could unshare one and keep its own copy of the Workspace mount",
    ))
}

/// The seccomp actions this kernel offers to a filter.
const SECCOMP_ACTIONS: &str = "/proc/sys/kernel/seccomp/actions_avail";

/// Whether this kernel can apply the command filter: it offers every action
/// the filter returns.
fn filter_available() -> bool {
    let Ok(text) = fs::read_to_string(SECCOMP_ACTIONS) else {
        return false;
    };
    let offered: Vec<&str> = text.split_whitespace().collect();
    ["allow", "errno", "kill_process"]
        .iter()
        .all(|action| offered.contains(action))
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

/// The Sandbox's root is an overlay whose writable layer is the Sandbox's own
/// directory under this runtime root, so the root must be on a filesystem that
/// can be an overlay upper. overlayfs refuses another overlay or a network
/// filesystem there, and a host that cannot hold it is refused before any
/// Sandbox is made rather than failing inside the Sandbox's mount.
pub(crate) fn require_overlay_upper(root: &Path) -> Result<(), RuntimeError> {
    let resolved = fs::canonicalize(root)?;
    match crate::local::os_impl::linux::mounts::filesystem(&resolved) {
        Ok(kind) if !UNSUITABLE.contains(&kind.as_str()) && !kind.starts_with("fuse") => Ok(()),
        // an unreadable mount table is no proof either way: the mount itself
        // then reports what is wrong, and nothing is invented here
        Err(_) => Ok(()),
        Ok(_) => Err(RuntimeError::Unsupported(
            "the namespaces runtime root is on a filesystem that cannot hold a Sandbox root's              writable layer (overlayfs or a network filesystem); put LAYERFS_LOCAL_DIR on a local              filesystem such as the project VM's ext4, xfs or btrfs disk",
        )),
    }
}

/// Filesystems an overlay upper layer cannot live on.
const UNSUITABLE: [&str; 7] = ["overlay", "nfs", "nfs4", "cifs", "smb3", "9p", "virtiofs"];

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
