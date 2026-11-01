//! Host commands: one provider CLI call with a deadline and captured output
//! (the port of `vm/cli.js`), detached helpers that outlive the call, and the
//! host facts setup needs (a free loopback port, a new secret). Nothing here
//! retries: a call runs once and its exit, output or timeout is reported.
use super::error::{Code, MachineError};
use super::spec::Tools;
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
use std::thread;
use std::time::{Duration, Instant};

/// Captured output kept per stream; a provider's answers are far smaller.
const KEEP: usize = 1 << 20;
const POLL: Duration = Duration::from_millis(20);

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
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|e| spawn_failed(what, &e))?;
    let feeder = match (input, child.stdin.take()) {
        (Some(bytes), Some(mut stdin)) => {
            let bytes = bytes.to_vec();
            Some(thread::spawn(move || {
                // a command that ends without reading its input is judged by its exit
                let _ = stdin.write_all(&bytes);
            }))
        }
        _ => None,
    };
    let out = collect(child.stdout.take());
    let err = collect(child.stderr.take());
    let status = wait(&mut child, Instant::now() + timeout)
        .map_err(|e| MachineError::new(Code::Unknown, format!("{what}: waiting failed: {e}")))?;
    if let Some(feeder) = feeder {
        let _ = feeder.join();
    }
    let stdout = out.join().unwrap_or_default();
    let stderr = err.join().unwrap_or_default();
    match status {
        Some(status) => Ok(Output {
            code: status.code(),
            stdout,
            stderr,
        }),
        None => Err(MachineError::new(
            Code::Unknown,
            format!(
                "{what} did not end within {} s and was stopped; its outcome is not known",
                timeout.as_secs()
            ),
        )),
    }
}

pub(crate) fn spawn_failed(what: &str, error: &io::Error) -> MachineError {
    MachineError::new(Code::Failed, format!("{what}: could not start: {error}"))
}

fn collect<R: Read + Send + 'static>(stream: Option<R>) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut kept = Vec::new();
        if let Some(mut stream) = stream {
            let mut buffer = [0u8; 8192];
            while let Ok(n) = stream.read(&mut buffer) {
                if n == 0 {
                    break;
                }
                let room = KEEP.saturating_sub(kept.len());
                kept.extend_from_slice(&buffer[..n.min(room)]);
            }
        }
        String::from_utf8_lossy(&kept).into_owned()
    })
}

/// Waits for `child` until `deadline`; past it the child is killed and `None`
/// is returned.
pub(crate) fn wait(child: &mut Child, deadline: Instant) -> io::Result<Option<ExitStatus>> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            child.wait()?;
            return Ok(None);
        }
        thread::sleep(POLL);
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
    let mut command = Command::new(sh);
    command
        .arg("-c")
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
    let mut command = Command::new(ps);
    command.args(["-o", "command=", "-p"]).arg(pid.to_string());
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

/// `program args...` as a command.
pub(crate) fn command<I, S>(program: &Path, args: I) -> Command
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(program);
    command.args(args);
    command
}
