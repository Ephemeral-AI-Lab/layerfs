//! A local process named by its id and kernel start time, so a reused id is
//! never taken for the process that was started.
use nix::{sys::signal::Signal, unistd::Pid};
use std::io;

/// One started process: its id and its start time in clock ticks since boot
/// (`/proc/<pid>/stat` field 22). Both must match for it to count as alive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalProcess {
    pub pid: u32,
    pub start: u64,
}

impl LocalProcess {
    /// The process running now under `pid`, read once from `/proc`.
    pub fn observe(pid: u32) -> io::Result<Self> {
        let (_, start) = stat(pid)?;
        Ok(Self { pid, start })
    }
    /// True while this exact process exists and has not exited: a zombie
    /// (or a dead entry) is not alive, and another process under the same id
    /// has another start time.
    pub fn alive(self) -> bool {
        match stat(self.pid) {
            Ok((state, start)) => start == self.start && !matches!(state, b'Z' | b'X' | b'x'),
            Err(_) => false,
        }
    }
    /// Sends `signal` once, only after this exact process was observed alive.
    pub(super) fn signal(self, signal: Signal) -> io::Result<()> {
        if !self.alive() {
            return Ok(());
        }
        let pid = i32::try_from(self.pid)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "process id width"))?;
        match nix::sys::signal::kill(Pid::from_raw(pid), signal) {
            Ok(()) | Err(nix::errno::Errno::ESRCH) => Ok(()),
            Err(errno) => Err(errno.into()),
        }
    }
}

/// State and start time of `pid`. The command name may hold spaces and
/// parentheses, so fields are counted after its last `)`.
fn stat(pid: u32) -> io::Result<(u8, u64)> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/stat"))?;
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "process stat fields");
    let rest = text.rsplit_once(')').ok_or_else(invalid)?.1;
    let mut fields = rest.split_ascii_whitespace();
    let state = fields
        .next()
        .and_then(|s| s.bytes().next())
        .ok_or_else(invalid)?;
    // field 3 was the state; field 22 is 19 fields further on
    let start = fields
        .nth(18)
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or_else(invalid)?;
    Ok((state, start))
}
