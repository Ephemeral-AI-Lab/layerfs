//! One bounded NFS client helper attempt, including post-KILL observation.
use layerfs_bridge::process::Retained;
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use std::{
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Arc,
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
    /// Whether this helper can detach lazily: `-l`/`--lazy` (also inside a
    /// combined word such as `-fl`) asks the kernel to drop the mount from the
    /// host's table before its superblock goes. The table then cannot witness
    /// that the client's mount ended, so a vanished entry proves nothing.
    pub fn lazy(&self) -> bool {
        self.0.iter().any(|word| {
            word == "--lazy"
                || (word.starts_with('-') && !word.starts_with("--") && word.contains('l'))
        })
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
/// The retained queue's one background thread reaps their exits and records
/// their status; its bound is that thread, not time.
#[derive(Debug, Default)]
pub(super) struct Helpers {
    retained: Retained,
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
        self.retained.reap()
    }
    pub fn reaped(&self) -> Vec<(u32, String)> {
        self.retained.exits()
    }
    fn retain(self: &Arc<Self>, child: Child) {
        self.retained.retain(child)
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
                        self.retained.record(pid, status);
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

/// Whether the host's table still lists `directory`: the path as configured,
/// or the same name below the canonical mount home (`/var` and `/tmp` are
/// `/private/var` and `/private/tmp` on macOS, which is what the table
/// prints). Only the home is resolved. Resolving the mount directory itself
/// would look the path up through the mount: against a wedged NFS client that
/// lookup can block with no bound, and it fails (EIO, ETIMEDOUT) exactly while
/// the mount is in trouble, which would read as "not mounted".
pub(super) fn listed(points: &[PathBuf], home: &Path, directory: &Path) -> bool {
    if points.iter().any(|point| point == directory) {
        return true;
    }
    let (Some(name), Ok(real)) = (directory.file_name(), home.canonicalize()) else {
        return false;
    };
    points.iter().any(|point| point == &real.join(name))
}

/// Mount points at or under `home`, deepest first, from `table` or the host's
/// mount table (`mount` on macOS, `/proc/self/mountinfo` on Linux).
pub(super) fn mounted_under(home: &Path, table: Option<&Path>) -> io::Result<Vec<PathBuf>> {
    let points: Vec<PathBuf> = if let Some(table) = table {
        let text = match std::fs::read_to_string(table) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error),
        };
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
            if !out.status.success() {
                return Err(io::Error::other(format!(
                    "mount listing failed: {}",
                    out.status
                )));
            }
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
    let real = home.canonicalize().ok();
    let mut found: Vec<PathBuf> = (points.into_iter())
        .filter(|p| p.starts_with(home) || real.as_deref().is_some_and(|r| p.starts_with(r)))
        .collect();
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
