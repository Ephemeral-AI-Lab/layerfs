//! One ordinary command to its end in a local Sandbox: the command identity,
//! the Workspace directory and a search path (on macOS this process's own);
//! nothing else of this process's environment, no shell of its own and no
//! implicit timeout. Each command leads a process group of its own, named in
//! the Sandbox until it is empty, so what it leaves running ends with the
//! Sandbox.
use super::{groups, process, process::LocalProcess, sandbox::Sandbox, search::search_path};
use crate::{backend::docker::ExecRequest, RuntimeError, WireFailure};
use nix::sys::signal::{killpg, Signal};
use std::{
    io::{self, Read, Write},
    os::unix::process::{CommandExt, ExitStatusExt},
    path::Path,
    process::{Command, Stdio},
};

/// One command's end: its exit code (128 + the signal for one a signal
/// ended, as the Engine reports it), its process id (diagnostic only) and the
/// bytes each stream delivered.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalExit {
    pub code: i32,
    pub pid: u32,
    pub stdout: u64,
    pub stderr: u64,
}

impl Sandbox {
    /// Runs `request` once while this Sandbox's daemon runs: as its uid/gid
    /// with no supplementary groups, in its directory, its stdout and stderr
    /// streamed to the sinks as they arrive, then waits for its exit. A sink
    /// failure closes the command's pipes, still waits for its exit, and
    /// returns the sink's error. Standard input is not offered.
    pub fn run(
        &self,
        request: &ExecRequest,
        stdout: &mut (dyn Write + Send),
        stderr: &mut (dyn Write + Send),
    ) -> Result<LocalExit, Box<WireFailure>> {
        check(request).map_err(|cause| self.failure(cause))?;
        self.endpoint()?;
        let mut command = Command::new(&request.arguments[0]);
        command
            .args(&request.arguments[1..])
            .env_clear()
            .env("PATH", search_path())
            .current_dir(Path::new(&request.directory))
            // std drops supplementary groups (setgroups 0) when it sets a uid as root
            .uid(request.identity.uid)
            .gid(request.identity.gid)
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for entry in &request.environment {
            if let Some((name, value)) = entry.split_once('=') {
                command.env(name, value);
            }
        }
        let mut child = command.spawn().map_err(|error| {
            let mut failure = self.io(error);
            failure.attempted = true;
            failure
        })?;
        let pid = child.id();
        // named before anything is waited for: the leader is not reaped
        // before the wait below, so it is observable even once it exited
        let named = LocalProcess::observe(pid)
            .and_then(|leader| groups::name(self.directory(), leader).map(|()| leader));
        let leader = match named {
            Ok(leader) => leader,
            Err(error) => {
                // an unnamed group would outlive the Sandbox: it ends here, once
                if let Ok(group) = process::pid(pid) {
                    let _ = killpg(group, Signal::SIGKILL);
                }
                let _ = child.wait();
                let mut failure = self.io(error);
                failure.attempted = true;
                return Err(failure);
            }
        };
        let out = child.stdout.take();
        let err = child.stderr.take();
        let copied = std::thread::scope(|scope| {
            let errors = scope.spawn(move || copy(err, stderr));
            let outs = copy(out, stdout);
            let errs = errors
                .join()
                .unwrap_or_else(|_| Err(io::Error::other("stderr copy panicked")));
            Ok::<_, io::Error>((outs?, errs?))
        });
        // the pipes are closed here either way; the exit is still waited for
        let status = child.wait();
        // a name left behind is harmless: its group is checked again when ended
        let _ = groups::forget_if_empty(self.directory(), leader);
        let (out_bytes, err_bytes) = copied.map_err(|error| self.io(error))?;
        let status = status.map_err(|error| self.io(error))?;
        let code = status
            .code()
            .or_else(|| status.signal().map(|signal| 128 + signal))
            .ok_or_else(|| self.invalid("local command exit status"))?;
        Ok(LocalExit {
            code,
            pid,
            stdout: out_bytes,
            stderr: err_bytes,
        })
    }
}

/// The Engine's own checks of an ordinary command, and no stdin.
fn check(request: &ExecRequest) -> Result<(), RuntimeError> {
    request.identity.check()?;
    if request.arguments.is_empty()
        || request.arguments[0].is_empty()
        || !Path::new(&request.directory).is_absolute()
        || (request.arguments.iter())
            .chain(&request.environment)
            .chain(std::iter::once(&request.directory))
            .any(|v| v.as_bytes().contains(&0))
    {
        return Err(RuntimeError::Protocol("ordinary command fields"));
    }
    if (request.environment.iter()).any(|v| !v.contains('=') || v.starts_with('=')) {
        return Err(RuntimeError::Protocol("ordinary environment field"));
    }
    if request.stdin {
        return Err(RuntimeError::Unsupported("local command standard input"));
    }
    Ok(())
}

/// Copies one stream to its sink until end of stream; the bytes delivered.
fn copy(source: Option<impl Read>, sink: &mut (dyn Write + Send)) -> io::Result<u64> {
    let Some(mut source) = source else {
        return Ok(0);
    };
    let mut total = 0u64;
    let mut window = [0u8; 8192];
    loop {
        let n = match source.read(&mut window) {
            Ok(0) => return Ok(total),
            Ok(n) => n,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        sink.write_all(&window[..n])?;
        total = total.saturating_add(n as u64);
    }
}
