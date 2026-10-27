//! A local process named by its id, kernel start time and boot, so a reused
//! id, or the same id and start time after a reboot or snapshot restore, is
//! never taken for the process that was started.
use nix::{sys::signal::Signal, unistd::Pid};
use std::{io, sync::OnceLock};

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
    /// The process running now under `pid`, read once from `/proc`, in this
    /// boot.
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

/// This boot's id (`/proc/sys/kernel/random/boot_id`, a UUID) as a number,
/// read once per process.
pub(super) fn boot() -> io::Result<u128> {
    static BOOT: OnceLock<u128> = OnceLock::new();
    if let Some(boot) = BOOT.get() {
        return Ok(*boot);
    }
    let text = std::fs::read_to_string("/proc/sys/kernel/random/boot_id")?;
    let hex: String = text.trim().chars().filter(|c| *c != '-').collect();
    let boot = (hex.len() == 32)
        .then(|| u128::from_str_radix(&hex, 16).ok())
        .flatten()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "boot id"))?;
    Ok(*BOOT.get_or_init(|| boot))
}

/// What `/proc/<pid>/stat` says of one process.
#[derive(Clone, Copy, Debug)]
pub(super) struct Stat {
    pub(super) state: u8,
    pub(super) group: u32,
    pub(super) start: u64,
}

impl Stat {
    /// A zombie or a dead entry: it runs nothing any more.
    pub(super) fn ended(self) -> bool {
        matches!(self.state, b'Z' | b'X' | b'x')
    }
}

/// State, process group and start time of `pid`. The command name may hold
/// spaces and parentheses, so fields are counted after its last `)`.
pub(super) fn stat(pid: u32) -> io::Result<Stat> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/stat"))?;
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "process stat fields");
    let rest = text.rsplit_once(')').ok_or_else(invalid)?.1;
    // field 3 (the state) is the first after the name, so field n is at n - 3
    let fields: Vec<&str> = rest.split_ascii_whitespace().collect();
    let field = |n: usize| fields.get(n - 3).copied().ok_or_else(invalid);
    Ok(Stat {
        state: field(3)?.bytes().next().ok_or_else(invalid)?,
        group: field(5)?.parse().map_err(|_| invalid())?,
        start: field(22)?.parse().map_err(|_| invalid())?,
    })
}

/// A process id as the kernel's signed type.
pub(super) fn pid(id: u32) -> io::Result<Pid> {
    i32::try_from(id)
        .map(Pid::from_raw)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "process id width"))
}
