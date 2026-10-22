//! The process groups a local Sandbox's processes run in. A container's
//! processes end with it; a local Sandbox's would outlive it on this kernel,
//! so each command starts its own process group, named in the Sandbox's
//! `groups` directory while it may have members, and every named group (and
//! the daemon's own) is killed when the Sandbox stops or is deleted.
//!
//! A group is named by its leader: id, start time and boot. Its id stays the
//! group's while any member lives, so a group whose leader is gone is still
//! known by it; a leader found with another start time means the id was
//! reused and the group is not this Sandbox's. A process that leaves its
//! group (`setsid`) is not followed unless it is named here too
//! ([`super::Sandbox::track`]).
use super::process::{self, LocalProcess};
use nix::{fcntl::OFlag, sys::signal::Signal};
use std::{
    fs::{self, OpenOptions},
    io,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::Path,
    time::{Duration, Instant},
};

/// The directory of a Sandbox that names its groups, one empty file each.
pub(super) const GROUPS: &str = "groups";
/// Pause between two looks at a killed group.
const PAUSE: Duration = Duration::from_millis(20);

/// Names the group `leader` leads (its id is the group's), once.
pub(super) fn name(directory: &Path, leader: LocalProcess) -> io::Result<()> {
    let groups = directory.join(GROUPS);
    // a Sandbox started before groups were named has no directory yet
    match fs::DirBuilder::new().mode(0o700).create(&groups) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    OpenOptions::new()
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags((OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC).bits())
        .open(groups.join(entry(leader)?))
        .map(drop)
}

/// Forgets the group `leader` led once no member of it is left. Called after
/// the leader was reaped; a group that still has members stays named.
pub(super) fn forget_if_empty(directory: &Path, leader: LocalProcess) -> io::Result<()> {
    match nix::sys::signal::killpg(process::pid(leader.pid)?, None) {
        Err(nix::errno::Errno::ESRCH) => {}
        _ => return Ok(()),
    }
    match fs::remove_file(directory.join(GROUPS).join(entry(leader)?)) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// Kills every group the Sandbox at `directory` named, and the group of
/// `daemon` (its leader), once with KILL, as a container's end kills what
/// ran in it, then waits up to `wait` for their members to be gone. Groups
/// of another boot ended with it.
pub(super) fn end(
    directory: &Path,
    daemon: Option<LocalProcess>,
    wait: Duration,
) -> io::Result<()> {
    let mut leaders: Vec<LocalProcess> = daemon.into_iter().collect();
    match fs::read_dir(directory.join(GROUPS)) {
        Ok(items) => {
            for item in items {
                let name = item?.file_name();
                let leader = name.to_str().and_then(parse).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "local Sandbox group name")
                })?;
                leaders.push(leader);
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let this_boot = process::boot()?;
    leaders.retain(|leader| leader.boot == Some(this_boot));
    let table = processes()?;
    let mut killed = Vec::new();
    for leader in leaders {
        if !members(&table, leader).is_empty() {
            match nix::sys::signal::killpg(process::pid(leader.pid)?, Signal::SIGKILL) {
                Ok(()) | Err(nix::errno::Errno::ESRCH) => killed.push(leader),
                Err(errno) => return Err(errno.into()),
            }
        }
    }
    let deadline = Instant::now() + wait;
    loop {
        let table = processes()?;
        if killed
            .iter()
            .all(|leader| members(&table, *leader).is_empty())
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(io::Error::other(
                "a process of the local Sandbox still runs after KILL",
            ));
        }
        std::thread::sleep(PAUSE);
    }
}

/// The running members of `leader`'s group: none when the leader's id now
/// names another process (the id was reused, so the group is another's).
fn members(table: &[(u32, process::Stat)], leader: LocalProcess) -> Vec<u32> {
    let reused = (table.iter()).any(|(pid, stat)| *pid == leader.pid && stat.start != leader.start);
    if reused {
        return Vec::new();
    }
    (table.iter())
        .filter(|(_, stat)| stat.group == leader.pid && !stat.ended())
        .map(|(pid, _)| *pid)
        .collect()
}

/// Every process of this kernel now, with its stat. One that ends while
/// being read is left out.
#[cfg(target_os = "macos")]
fn processes() -> io::Result<Vec<(u32, process::Stat)>> {
    super::darwin::processes()
}
#[cfg(target_os = "linux")]
fn processes() -> io::Result<Vec<(u32, process::Stat)>> {
    let mut table = Vec::new();
    for item in fs::read_dir("/proc")? {
        let Some(pid) = item?.file_name().to_str().and_then(|n| n.parse().ok()) else {
            continue;
        };
        if let Ok(stat) = process::stat(pid) {
            table.push((pid, stat));
        }
    }
    Ok(table)
}

/// A group's file name: `<boot as 32 hex>-<leader id>-<leader start>`.
fn entry(leader: LocalProcess) -> io::Result<String> {
    let boot = leader
        .boot
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "a group leader's boot"))?;
    Ok(format!("{boot:032x}-{}-{}", leader.pid, leader.start))
}

fn parse(name: &str) -> Option<LocalProcess> {
    let mut parts = name.split('-');
    let boot = parts.next().filter(|b| b.len() == 32)?;
    let process = LocalProcess {
        boot: Some(u128::from_str_radix(boot, 16).ok()?),
        pid: parts.next()?.parse().ok()?,
        start: parts.next()?.parse().ok()?,
    };
    parts.next().is_none().then_some(process)
}
