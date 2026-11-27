//! Host commands: one provider CLI call with a deadline and captured output
//! (the port of `vm/cli.js`), detached helpers that outlive the call, and the
//! host facts setup needs (a free loopback port, a new secret). Nothing here
//! retries: a call runs once and its exit, output or timeout is reported.
use crate::platform::error::{Code, MachineError};
use crate::setup::spec::Tools;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::ffi::OsStr;
use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, TcpListener};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Captured output kept per stream; a provider's answers are far smaller.
const KEEP: usize = 1 << 20;
/// Exit checks start [`FIRST_POLL`] apart and grow with the command's run
/// (an eighth of it) up to [`POLL`]: a command is seen soon after it ends,
/// not at the next fixed 20 ms step, which added about 17 ms to a 26 ms
/// provider call (#66).
const FIRST_POLL: Duration = Duration::from_millis(1);
const POLL: Duration = Duration::from_millis(20);
/// How long past its deadline a command's output may take to reach its end
/// (after its process group was killed there).
const DRAIN: Duration = Duration::from_secs(1);

thread_local! {
    /// This thread's probe deadline, set by [`within`]: every command the
    /// thread runs meanwhile ends by then.
    static CAP: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
}

/// Runs `probe` with every command it starts on this thread ending by
/// `deadline`: one still running then is killed with its whole process
/// group (a `limactl shell`'s `ssh`, say) and reaped, as at [`run`]'s own
/// deadline, and the group of one that ended is killed too, so a probe past
/// its time leaves no process behind.
pub(crate) fn within<T>(deadline: Instant, probe: impl FnOnce() -> T) -> T {
    let before = CAP.replace(Some(deadline));
    let out = probe();
    CAP.set(before);
    out
}

/// A finished command: its exit code (`None` when a signal ended it) and output.
#[derive(Debug)]
pub(crate) struct Output {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    pub fn ok(&self) -> bool {
        self.code == Some(0)
    }

    /// The last lines of what the command said, for an error message.
    pub fn said(&self) -> String {
        let text = if self.stderr.trim().is_empty() {
            &self.stdout
        } else {
            &self.stderr
        };
        let lines: Vec<&str> = text.trim().lines().collect();
        lines[lines.len().saturating_sub(3)..].join("; ")
    }

    /// The trimmed stdout of a command that succeeded, else `Failed` naming `what`.
    pub fn answer(self, what: &str) -> Result<String, MachineError> {
        if self.ok() {
            return Ok(self.stdout.trim().to_owned());
        }
        Err(MachineError::new(
            Code::Failed,
            format!("{what} failed ({}): {}", self.exit(), self.said()),
        ))
    }

    pub fn exit(&self) -> String {
        self.code
            .map_or_else(|| "ended by a signal".into(), |code| format!("exit {code}"))
    }
}

/// The full path of `name` on the tools' search path, else `Unsupported`
/// naming `tool:<name>`.
pub(crate) fn find(tools: &Tools, name: &str) -> Result<PathBuf, MachineError> {
    lookup(tools, name).ok_or_else(|| {
        MachineError::missing(
            format!("{name} is not on the search path"),
            vec![format!("tool:{name}")],
        )
    })
}

/// The full path of tool `name` on the tools' search path (`PATH`, then the
/// fallback folders), if it is there: how every caller outside the adapters
/// finds `docker` too, so both find the same one.
pub fn find_tool(tools: &Tools, name: &str) -> Option<PathBuf> {
    lookup(tools, name)
}

pub(crate) fn lookup(tools: &Tools, name: &str) -> Option<PathBuf> {
    std::env::split_paths(&tools.path)
        .filter(|dir| !dir.as_os_str().is_empty())
        .map(|dir| dir.join(name))
        .find(|path| is_executable(path))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Runs `command` once with `input` on stdin, killing it at `timeout`. A
/// command that does not end in time is `Unknown`: what it did is not known.
///
/// It runs in its own process group, and at the deadline the whole group is
/// killed, not only the command: a descendant that still holds its output
/// (`limactl`'s `ssh` or host agent, say) would otherwise keep the readers,
/// and the call, waiting past the deadline. The output is read until the
/// deadline and [`DRAIN`] more, never longer: output still held open then
/// has the group killed and is `Unknown` too, even when the command itself
/// ended (its call did not). A command that ends and leaves a descendant
/// running without its pipes keeps it: only [`within`]'s probe deadline
/// ends the group of a command that ended.
pub(crate) fn run(
    mut command: Command,
    input: Option<&[u8]>,
    timeout: Duration,
    what: &str,
) -> Result<Output, MachineError> {
    command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let cap = CAP.get();
    let own = Instant::now() + timeout;
    let deadline = cap.map_or(own, |cap| cap.min(own));
    if Instant::now() >= deadline {
        return Err(MachineError::new(
            Code::Unknown,
            format!("{what}: probe deadline already exceeded; no command was started"),
        ));
    }
    let mut child = command.spawn().map_err(|e| spawn_failed(what, &e))?;
    if let (Some(bytes), Some(mut stdin)) = (input, child.stdin.take()) {
        let bytes = bytes.to_vec();
        // a command that ends without reading its input is judged by its
        // exit; the writer is not joined, so a descendant that keeps the
        // pipe open unread holds only this thread, never the call
        thread::spawn(move || drop(stdin.write_all(&bytes)));
    }
    let out = collect(child.stdout.take());
    let err = collect(child.stderr.take());
    let waited = wait_for(&mut child, deadline, true);
    if waited.is_err() {
        kill_group(&child);
        let _ = child.kill();
        let _ = child.wait();
    }
    // A version/list command may exit while a descendant still holds its pipes.
    // Under an observational deadline, end that private process group before
    // reading to the end; otherwise pipe EOF could outlive the budget.
    if cap.is_some() {
        kill_group(&child);
    }
    // the output may take [`DRAIN`] past the deadline to reach its end; one
    // still held open then is not waited for: its holders are killed
    let ended = out.ended_by(deadline + DRAIN) && err.ended_by(deadline + DRAIN);
    if !ended {
        kill_group(&child);
    }
    let status = waited.map_err(|error| {
        MachineError::new(Code::Unknown, format!("{what}: waiting failed: {error}"))
    })?;
    let Some(status) = status else {
        return Err(MachineError::new(
            Code::Unknown,
            if deadline < own {
                format!("{what} did not end by its probe's deadline and was stopped")
            } else {
                format!(
                    "{what} did not end within {} s and was stopped; its outcome is not known",
                    timeout.as_secs()
                )
            },
        ));
    };
    if !ended {
        return Err(MachineError::new(
            Code::Unknown,
            format!(
                "{what} ended, but its output was still held open when its time ran out; its outcome is not known"
            ),
        ));
    }
    Ok(Output {
        code: status.code(),
        stdout: out.text(),
        stderr: err.text(),
    })
}

pub(crate) fn spawn_failed(what: &str, error: &io::Error) -> MachineError {
    MachineError::new(Code::Failed, format!("{what}: could not start: {error}"))
}

/// One pipe read to its end on a thread of its own: what it held so far
/// (up to [`KEEP`]) and whether it ended.
struct Reader {
    kept: Arc<Mutex<Vec<u8>>>,
    ended: mpsc::Receiver<()>,
}

impl Reader {
    /// Whether the pipe ended by `deadline`; asked again, it answers at once.
    fn ended_by(&self, deadline: Instant) -> bool {
        let left = deadline.saturating_duration_since(Instant::now());
        !matches!(
            self.ended.recv_timeout(left),
            Err(mpsc::RecvTimeoutError::Timeout)
        )
    }

    fn text(&self) -> String {
        let kept = self.kept.lock().unwrap_or_else(|e| e.into_inner());
        String::from_utf8_lossy(&kept).into_owned()
    }
}

fn collect<R: Read + Send + 'static>(stream: Option<R>) -> Reader {
    let kept = Arc::new(Mutex::new(Vec::new()));
    let (done, ended) = mpsc::channel();
    let into = Arc::clone(&kept);
    thread::spawn(move || {
        if let Some(mut stream) = stream {
            let mut buffer = [0u8; 8192];
            while let Ok(n) = stream.read(&mut buffer) {
                if n == 0 {
                    break;
                }
                let mut kept = into.lock().unwrap_or_else(|e| e.into_inner());
                let room = KEEP.saturating_sub(kept.len());
                kept.extend_from_slice(&buffer[..n.min(room)]);
            }
        }
        let _ = done.send(());
    });
    Reader { kept, ended }
}

/// SIGKILL to the process group `child` leads (it was started with its own).
fn kill_group(child: &Child) {
    if let Ok(pid) = i32::try_from(child.id()) {
        let _ = kill(Pid::from_raw(-pid), Signal::SIGKILL);
    }
}

/// Waits for `child` until `deadline`; past it the child is killed and `None`
/// is returned.
pub(crate) fn wait(child: &mut Child, deadline: Instant) -> io::Result<Option<ExitStatus>> {
    wait_for(child, deadline, false)
}

/// [`wait`]; `group` kills the child's whole process group (it leads one).
fn wait_for(child: &mut Child, deadline: Instant, group: bool) -> io::Result<Option<ExitStatus>> {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            match group {
                true => kill_group(child),
                false => drop(child.kill()),
            }
            child.wait()?;
            return Ok(None);
        }
        thread::sleep((started.elapsed() / 8).clamp(FIRST_POLL, POLL));
    }
}

/// One argument for a remote POSIX shell: never left to its word splitting.
pub(crate) fn shell_escape(arg: &str) -> String {
    let plain = !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-./=:@%+,".contains(c));
    if plain {
        arg.to_owned()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

/// Starts `program args` detached: in its own process group, its stdin and
/// stdout closed, its stderr appended to `log`, and reparented to init once
/// the starting shell exits, so it outlives this process and leaves no child
/// here. Returns its pid.
pub(crate) fn detach(
    tools: &Tools,
    program: &Path,
    args: &[String],
    log: &Path,
) -> Result<u32, MachineError> {
    let sh = find(tools, "sh")?;
    let mut command = self::command(tools, &sh, ["-c"]);
    command
        .arg("log=$1; shift; \"$@\" </dev/null >/dev/null 2>>\"$log\" & echo $!")
        .arg("sh")
        .arg(log)
        .arg(program)
        .args(args)
        .process_group(0);
    let out = run(
        command,
        None,
        Duration::from_secs(10),
        "starting a detached helper",
    )?
    .answer("starting a detached helper")?;
    out.parse()
        .map_err(|_| MachineError::new(Code::Failed, format!("a detached helper's pid {out:?}")))
}

/// How much of a log's end [`tail`] reads.
const TAIL: u64 = 16 << 10;

/// The last `lines` lines of the host file `log` as one line, read from
/// its last 16 KiB only; "no reason given" for an empty or unreadable one.
pub(crate) fn tail(log: &Path, lines: usize) -> String {
    use std::io::{Read, Seek, SeekFrom};
    let mut bytes = Vec::new();
    if let Ok(mut file) = std::fs::File::open(log) {
        let size = file.metadata().map_or(0, |found| found.len());
        if file
            .seek(SeekFrom::Start(size.saturating_sub(TAIL)))
            .is_ok()
        {
            let _ = file.take(TAIL).read_to_end(&mut bytes);
        }
    }
    let text = String::from_utf8_lossy(&bytes);
    let all: Vec<&str> = text.trim().lines().collect();
    let said = all[all.len().saturating_sub(lines)..].join("; ");
    if said.is_empty() {
        "no reason given".into()
    } else {
        said
    }
}

/// Whether `pid` runs and its command line holds `marker` (a recorded pid
/// may since have been reused by another process).
pub(crate) fn alive(tools: &Tools, pid: u32, marker: &str) -> bool {
    let Ok(raw) = i32::try_from(pid) else {
        return false;
    };
    if kill(Pid::from_raw(raw), None).is_err() {
        return false;
    }
    let Ok(ps) = find(tools, "ps") else {
        return true;
    };
    let mut command = self::command(tools, &ps, ["-o", "command=", "-p"]);
    command.arg(pid.to_string());
    run(command, None, Duration::from_secs(10), "ps")
        .is_ok_and(|out| out.ok() && out.stdout.contains(marker))
}

/// Ends a detached helper, bounded: SIGTERM, up to 3 s, then SIGKILL.
pub(crate) fn end(tools: &Tools, pid: u32, marker: &str) {
    if !alive(tools, pid, marker) {
        return;
    }
    let target = Pid::from_raw(pid as i32);
    let _ = kill(target, Signal::SIGTERM);
    let until = Instant::now() + Duration::from_secs(3);
    while Instant::now() < until && kill(target, None).is_ok() {
        thread::sleep(Duration::from_millis(50));
    }
    if kill(target, None).is_ok() {
        let _ = kill(target, Signal::SIGKILL);
    }
}

/// A host loopback port that was free a moment ago.
pub(crate) fn free_port() -> Result<u16, MachineError> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .map_err(|e| MachineError::new(Code::Failed, format!("no free loopback port: {e}")))?;
    listener
        .local_addr()
        .map(|addr| addr.port())
        .map_err(|e| MachineError::new(Code::Failed, format!("no free loopback port: {e}")))
}

/// A new 256-bit secret, as 64 hex digits.
pub(crate) fn secret() -> Result<String, MachineError> {
    let mut bytes = [0u8; 32];
    File::open("/dev/urandom")
        .and_then(|mut random| random.read_exact(&mut bytes))
        .map_err(|e| MachineError::new(Code::Failed, format!("no random bytes: {e}")))?;
    Ok(bytes.iter().fold(String::with_capacity(64), |mut hex, b| {
        let _ = write!(hex, "{b:02x}");
        hex
    }))
}

/// `program args...` as a command, run with the tools' search path as its
/// `PATH`, so a provider CLI finds its own helpers where LayerFS found it.
pub(crate) fn command<I, S>(tools: &Tools, program: &Path, args: I) -> Command
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(program);
    command.args(args);
    // an empty search path leaves the command the system's default one
    if !tools.path.is_empty() {
        command.env("PATH", &tools.path);
    }
    command
}
