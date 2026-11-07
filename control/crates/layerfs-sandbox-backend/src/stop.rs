//! How one command is stopped (compute-plane 2.0 §5.3, layerfs-project#113):
//! the backend fills its stop handle in at spawn ([`Stop`], reported through
//! [`ExecSlot`]) and acts on it at cancel ([`cancel`]). There are two forms.
//!
//! - `Cgroup { path }` (`namespaces`): the command's own child cgroup,
//!   `<sandbox cgroup>/ws-<workspace id>/exec-<exec id>`. Cancel sends
//!   `SIGTERM` to its processes, waits up to the grace, then writes
//!   `cgroup.kill` and checks it is empty; a child that left its process
//!   group ends too.
//! - `Group { pgid, leader }` (`host_process`): the command's own process
//!   group, known by its leader (id, start time, boot) against id reuse.
//!   Cancel sends `SIGTERM` to the group, waits up to the grace, then
//!   `SIGKILL`. A child that calls `setsid` itself escapes it.
//!
//! Cancel is one explicit attempt: nothing here times a command out,
//! retries or replays.
use layerfs_bridge::process::LocalProcess;
use nix::sys::signal::Signal;
use std::{
    io,
    path::PathBuf,
    time::{Duration, Instant},
};

/// One command's stop handle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stop {
    /// Its own cgroup (`namespaces`).
    Cgroup {
        /// The cgroup's directory on this kernel.
        path: PathBuf,
    },
    /// Its own process group (`host_process`).
    Group {
        /// The group's id: its leader's process id.
        pgid: u32,
        /// The group's leader, against id reuse.
        leader: LocalProcess,
    },
}

impl Stop {
    /// Which form it is, as an exec result names it: `cgroup` or `group`.
    pub fn form(&self) -> &'static str {
        match self {
            Self::Cgroup { .. } => "cgroup",
            Self::Group { .. } => "group",
        }
    }
}

/// What one command's spawn names: the Workspace and exec ids its own
/// cgroup is named by (`namespaces`), and `spawned`, told the command's
/// stop handle before it can be waited for. A `spawned` that fails ends
/// the command at once (or never starts it), and the run answers that
/// failure.
pub struct ExecSlot<'a> {
    /// The Workspace's id (64 hex digits).
    pub workspace: &'a str,
    /// The command's exec id, unique in its Workspace.
    pub exec: &'a str,
    /// Told the stop handle once it is known.
    pub spawned: &'a mut dyn FnMut(&Stop) -> io::Result<()>,
}

/// How a cancel ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cancelled {
    /// Nothing of the command ran: it had ended already.
    Ended,
    /// Its processes were ended with this signal: `SIGTERM` within the
    /// grace, else `SIGKILL`.
    Signalled(Signal),
}

/// Pause between two looks at a signalled command.
const PAUSE: Duration = Duration::from_millis(20);
/// Wait for its processes to be gone after the kill.
const KILL_WAIT: Duration = Duration::from_secs(5);

/// A name usable as one path component of a stop handle: 1 to 128 ASCII
/// letters, digits, `.`, `_` or `-`, not starting with `.` or `-`.
pub fn component(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with(['.', '-'])
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// Ends the command `stop` names once: `SIGTERM` to everything it started,
/// up to `grace` for it to end, then the kill. A command with nothing left
/// running is [`Cancelled::Ended`] and is not signalled.
pub fn cancel(stop: &Stop, grace: Duration) -> io::Result<Cancelled> {
    match stop {
        #[cfg(target_os = "linux")]
        Stop::Cgroup { path } => {
            let group = crate::backend::namespaces::ExecGroup(path);
            ended(&group, grace)
        }
        #[cfg(not(target_os = "linux"))]
        Stop::Cgroup { .. } => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "a cgroup stop handle is acted on by the Linux kernel that holds it",
        )),
        Stop::Group { pgid, leader } => ended(&crate::local::groups::Led(*pgid, *leader), grace),
    }
}

/// What a stop handle acts on: its running processes, a signal to all of
/// them, and the kill that cannot be refused.
pub(crate) trait Stoppable {
    /// Whether any of its processes still runs.
    fn running(&self) -> io::Result<bool>;
    /// Sends `signal` to each of its processes once.
    fn signal(&self, signal: Signal) -> io::Result<()>;
    /// Ends every one of its processes with `SIGKILL`.
    fn kill(&self) -> io::Result<()>;
}

fn ended(group: &dyn Stoppable, grace: Duration) -> io::Result<Cancelled> {
    if !group.running()? {
        return Ok(Cancelled::Ended);
    }
    group.signal(Signal::SIGTERM)?;
    if gone(group, grace)? {
        return Ok(Cancelled::Signalled(Signal::SIGTERM));
    }
    group.kill()?;
    if gone(group, KILL_WAIT)? {
        return Ok(Cancelled::Signalled(Signal::SIGKILL));
    }
    Err(io::Error::other(
        "a process of the command still runs after SIGKILL",
    ))
}

/// Waits up to `wait` for `group` to have no process left.
fn gone(group: &dyn Stoppable, wait: Duration) -> io::Result<bool> {
    let deadline = Instant::now() + wait;
    loop {
        if !group.running()? {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        std::thread::sleep(PAUSE.min(deadline.saturating_duration_since(Instant::now())));
    }
}
