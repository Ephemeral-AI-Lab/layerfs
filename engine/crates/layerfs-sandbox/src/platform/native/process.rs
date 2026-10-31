//! A local process named by its id, kernel start time and boot, so a reused
//! id, or the same id and start time after a reboot or snapshot restore, is
//! never taken for the process that was started. The three facts come from
//! `/proc` on Linux and from `proc_pidinfo`/`sysctl` on macOS (see
//! [`super::macos`]).
pub(crate) use super::platform::{boot, stat};
use nix::{sys::signal::Signal, unistd::Pid};
use std::io;

/// One started process: its id, its start time in clock ticks since boot
/// (`/proc/<pid>/stat` field 22) and the boot it started in
/// (`/proc/sys/kernel/random/boot_id`). All must match for it to count as
/// alive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalProcess {
    pub pid: u32,
    pub start: u64,
    /// The boot id as a number. `None` only for a record written before boot
    /// ids were kept: it is matched by id and start time alone, as it was
    /// then, and never signalled.
    pub boot: Option<u128>,
}

impl LocalProcess {
    /// The process running now under `pid`, read once from this kernel, in
    /// this boot.
    pub fn observe(pid: u32) -> io::Result<Self> {
        let stat = stat(pid)?;
        Ok(Self {
            pid,
            start: stat.start,
            boot: Some(boot()?),
        })
    }
    /// True while this exact process exists and has not exited: a zombie
    /// (or a dead entry) is not alive, and another process under the same id
    /// has another start time or another boot.
    pub fn alive(self) -> bool {
        let this_boot = self
            .boot
            .is_none_or(|recorded| boot().ok() == Some(recorded));
        match stat(self.pid) {
            Ok(stat) => this_boot && stat.start == self.start && !stat.ended(),
            Err(_) => false,
        }
    }
    /// Sends `signal` once, only after this exact process was observed alive.
    pub(super) fn signal(self, signal: Signal) -> io::Result<()> {
        if self.boot.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "a local process recorded without its boot is never signalled",
            ));
        }
        if !self.alive() {
            return Ok(());
        }
        match nix::sys::signal::kill(pid(self.pid)?, signal) {
            Ok(()) | Err(nix::errno::Errno::ESRCH) => Ok(()),
            Err(errno) => Err(errno.into()),
        }
    }
}

/// What this kernel says of one process.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Stat {
    pub(crate) state: u8,
    pub(crate) group: u32,
    pub(crate) start: u64,
}

impl Stat {
    /// A zombie or a dead entry: it runs nothing any more.
    pub(crate) fn ended(self) -> bool {
        matches!(self.state, b'Z' | b'X' | b'x')
    }
}

/// A process id as the kernel's signed type.
pub(super) fn pid(id: u32) -> io::Result<Pid> {
    i32::try_from(id)
        .map(Pid::from_raw)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "process id width"))
}
