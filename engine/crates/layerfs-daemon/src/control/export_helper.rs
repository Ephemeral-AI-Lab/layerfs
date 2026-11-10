//! One bounded NFS client helper attempt, including post-KILL observation.
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use std::{
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

/// How long one helper run may take before it is killed and reported.
pub const HELPER_WAIT: Duration = Duration::from_secs(60);
pub const REAP_WAIT: Duration = Duration::from_secs(5);
const PAUSE: Duration = Duration::from_millis(5);
const OUTPUT: usize = 2048;

/// One helper command; `{port}` and `{dir}` in its words are replaced.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Helper(pub Vec<String>);
impl Helper {
    /// Splits a configured command line on whitespace.
    pub fn words(line: &str) -> Option<Self> {
        let words: Vec<String> = line.split_whitespace().map(str::to_owned).collect();
        (!words.is_empty()).then_some(Self(words))
    }
    fn command(&self, port: u16, directory: &str) -> Option<Command> {
        let mut words = self.0.iter().map(|word| {
            word.replace("{port}", &port.to_string())
                .replace("{dir}", directory)
        });
        let mut command = Command::new(words.next()?);
        command.args(words);
        Some(command)
    }
}

pub(super) enum Ran {
    Done,
    Busy(String),
    Failed(String),
    /// Timeout or observation failure after spawn: effects are not known.
    Interrupted(String),
}

/// Retained children are never waited on synchronously or signalled twice.
/// A bounded background reaper reaps their exits and records their status.
#[derive(Debug, Default)]
pub(super) struct Helpers {
    held: Mutex<Vec<Child>>,
    reaped: Mutex<Vec<(u32, String)>>,
    reaping: AtomicBool,
    pub helper_wait: Duration,
    pub reap_wait: Duration,
}
impl Helpers {
    pub fn new(helper_wait: Duration, reap_wait: Duration) -> Arc<Self> {
        Arc::new(Self {
            helper_wait,
            reap_wait,
            ..Self::default()
        })
    }
    pub fn reap(&self) -> bool {
        let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
        let mut reaped = self.reaped.lock().unwrap_or_else(|e| e.into_inner());
        held.retain_mut(|child| match child.try_wait() {
            Ok(Some(s)) => {
                reaped.push((child.id(), s.to_string()));
                false
            }
            Ok(None) => true,
            Err(e) => {
                reaped.push((child.id(), format!("wait error: {e}")));
                false
            }
        });
        let excess = reaped.len().saturating_sub(64);
        reaped.drain(..excess);
        held.is_empty()
    }
    pub fn reaped(&self) -> Vec<(u32, String)> {
        self.reap();
        self.reaped
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    fn retain(self: &Arc<Self>, child: Child) {
        self.held
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(child);
        if !self.reaping.swap(true, Ordering::AcqRel) {
            let queue = self.clone();
            if std::thread::Builder::new()
                .name("layerfs-helper-reap".into())
                .spawn(move || loop {
                    if queue.reap() {
                        queue.reaping.store(false, Ordering::Release);
                        if queue.reap() || queue.reaping.swap(true, Ordering::AcqRel) {
                            break;
                        }
                    }
                    std::thread::sleep(Duration::from_millis(20));
                })
                .is_err()
            {
                self.reaping.store(false, Ordering::Release);
            }
        }
    }
    pub fn run(self: &Arc<Self>, helper: &Helper, port: u16, directory: &str) -> Ran {
        self.reap();
        let Some(mut command) = helper.command(port, directory) else {
            return Ran::Failed("empty helper command".into());
        };
        let mut child = match command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => return Ran::Failed(format!("{:?}: {error}", helper.0.first())),
        };
        let mut pipes: Vec<Box<dyn Read>> = Vec::new();
        let mut pipe_error = None;
        // Even an exited helper can leave a descendant holding its pipe open.
        // Never wait for EOF, and drain while it runs so verbose helpers progress.
        for pipe in [
            child.stderr.take().map(|pipe| {
                let result = fcntl(&pipe, FcntlArg::F_SETFL(OFlag::O_NONBLOCK));
                (Box::new(pipe) as Box<dyn Read>, result)
            }),
            child.stdout.take().map(|pipe| {
                let result = fcntl(&pipe, FcntlArg::F_SETFL(OFlag::O_NONBLOCK));
                (Box::new(pipe) as Box<dyn Read>, result)
            }),
        ]
        .into_iter()
        .flatten()
        {
            match pipe.1 {
                Ok(_) => pipes.push(pipe.0),
                Err(error) => pipe_error = Some(format!("helper output: {error}")),
            }
        }
        let mut output = Vec::new();
        let deadline = Instant::now() + self.helper_wait;
        let status = loop {
            drain(&mut pipes, &mut output);
            if let Some(error) = pipe_error.take() {
                break Err(error);
            }
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) if Instant::now() < deadline => std::thread::sleep(PAUSE),
                Ok(None) => break Err(format!("helper did not finish in {:?}", self.helper_wait)),
                Err(error) => break Err(format!("helper wait: {error}")),
            }
        };
        let status = match status {
            Ok(status) => Ok(status),
            Err(detail) => {
                let kill = child.kill();
                let deadline = Instant::now() + self.reap_wait;
                let reaped = if self.reap_wait.is_zero() {
                    None
                } else {
                    loop {
                        match child.try_wait() {
                            Ok(Some(status)) => break Some(status),
                            Ok(None) if Instant::now() < deadline => std::thread::sleep(PAUSE),
                            Ok(None) | Err(_) => break None,
                        }
                    }
                };
                let pid = child.id();
                let disposition = match reaped {
                    Some(status) => {
                        self.reaped
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .push((pid, status.to_string()));
                        format!("exit reaped ({status})")
                    }
                    None => {
                        self.retain(child);
                        format!("exit not observed after {:?}; helper may remain in kernel wait; child custody retained", self.reap_wait)
                    }
                };
                Err(format!("{detail}; pid={pid}; KILL={kill:?}; {disposition}"))
            }
        };
        drain(&mut pipes, &mut output);
        let output = String::from_utf8_lossy(&output);
        match status {
            Ok(status) if status.success() => Ran::Done,
            Ok(status) => {
                let detail = format!("{status}: {}", output.trim());
                if output.to_ascii_lowercase().contains("busy") {
                    Ran::Busy(detail)
                } else {
                    Ran::Failed(detail)
                }
            }
            Err(detail) => Ran::Interrupted(format!("{detail}: {}", output.trim())),
        }
    }
}

/// Mount points at or under `home`, deepest first, from `table` or the host's
/// mount table (`mount` on macOS, `/proc/self/mountinfo` on Linux).
pub(super) fn mounted_under(home: &Path, table: Option<&Path>) -> io::Result<Vec<PathBuf>> {
    let points: Vec<PathBuf> = if let Some(table) = table {
        let text = std::fs::read_to_string(table).unwrap_or_default();
        (text.lines().map(str::trim).filter(|l| !l.is_empty()))
            .map(PathBuf::from)
            .collect()
    } else {
        #[cfg(target_os = "macos")]
        let points = {
            let out = Command::new("/sbin/mount")
                .env("LC_ALL", "C")
                .stdin(Stdio::null())
                .output()?;
            (String::from_utf8_lossy(&out.stdout).lines())
                .filter_map(|l| Some(PathBuf::from(l.split_once(" on ")?.1.rsplit_once(" (")?.0)))
                .collect()
        };
        #[cfg(not(target_os = "macos"))]
        let points = (std::fs::read_to_string("/proc/self/mountinfo")?.lines())
            .filter_map(|line| line.split(' ').nth(4))
            .map(|point| PathBuf::from(point.replace("\\040", " ").replace("\\011", "\t")))
            .collect();
        points
    };
    let mut found: Vec<PathBuf> = points.into_iter().filter(|p| p.starts_with(home)).collect();
    found.sort_by_key(|point| std::cmp::Reverse(point.components().count()));
    found.dedup();
    Ok(found)
}

fn drain(pipes: &mut [Box<dyn Read>], output: &mut Vec<u8>) {
    for pipe in pipes {
        let mut bytes = [0; OUTPUT];
        // One bounded read per pipe and observation: even continuous output
        // cannot defer the helper's deadline. Excess bytes are discarded.
        match pipe.read(&mut bytes) {
            Ok(count) => output.extend_from_slice(&bytes[..count.min(OUTPUT - output.len())]),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(_) => {}
        }
    }
}
