//! One bounded NFS client helper attempt, including post-KILL observation.
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use std::{
    io::{self, Read},
    process::{Child, Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};

/// How long one helper run may take before it is killed and reported.
pub const HELPER_WAIT: Duration = Duration::from_secs(60);
const REAP_WAIT: Duration = Duration::from_secs(5);
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
/// A later helper attempt may only reap their observed exits, not replay them.
#[derive(Debug, Default)]
pub(super) struct Helpers(Mutex<Vec<Child>>);
impl Helpers {
    pub fn run(&self, helper: &Helper, port: u16, directory: &str) -> Ran {
        {
            let mut held = self.0.lock().unwrap_or_else(|error| error.into_inner());
            held.retain_mut(|child| !matches!(child.try_wait(), Ok(Some(_))));
        }
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
        let deadline = Instant::now() + HELPER_WAIT;
        let status = loop {
            drain(&mut pipes, &mut output);
            if let Some(error) = pipe_error.take() {
                break Err(error);
            }
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) if Instant::now() < deadline => std::thread::sleep(PAUSE),
                Ok(None) => break Err(format!("helper did not finish in {HELPER_WAIT:?}")),
                Err(error) => break Err(format!("helper wait: {error}")),
            }
        };
        let status = match status {
            Ok(status) => Ok(status),
            Err(detail) => {
                let kill = child.kill();
                let deadline = Instant::now() + REAP_WAIT;
                let reaped = loop {
                    match child.try_wait() {
                        Ok(Some(_)) => break true,
                        Ok(None) if Instant::now() < deadline => std::thread::sleep(PAUSE),
                        Ok(None) | Err(_) => break false,
                    }
                };
                let pid = child.id();
                let disposition = if reaped {
                    "exit reaped".to_owned()
                } else {
                    self.0
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .push(child);
                    "exit not observed after 5s; helper may remain in kernel wait; child custody retained".into()
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
