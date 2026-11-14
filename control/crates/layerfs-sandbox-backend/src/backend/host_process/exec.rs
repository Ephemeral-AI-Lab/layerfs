//! One ordinary command to its end in a local Sandbox: the command identity,
//! the Workspace directory, a search path (on macOS this process's own), and
//! (on macOS) the caller's Rust and Git locations (`RUSTUP_HOME`,
//! `CARGO_HOME`, `GIT_CONFIG_GLOBAL`) when present; nothing else of this
//! process's environment, no shell of its own and no implicit timeout. Each
//! command leads a process group of its own, named in the Sandbox until it is
//! empty, so what it leaves running ends with the Sandbox.
use super::sandbox::Sandbox;
use crate::local::streams;
use crate::local::{groups, search::search_path};
use crate::stop::{ExecSlot, Stop};
use crate::{ExecExit, ExecRequest, RuntimeError, WireFailure};
use layerfs_bridge::process::{self, LocalProcess};
use nix::sys::signal::{killpg, Signal};
use std::{
    ffi::OsString,
    io::{self, Write},
    os::unix::process::{CommandExt, ExitStatusExt},
    path::Path,
    process::{Command, Stdio},
};

impl Sandbox {
    /// [`Sandbox::run_in`] with no slot to tell its stop handle to.
    pub fn run(
        &self,
        request: &ExecRequest,
        stdout: &mut (dyn Write + Send),
        stderr: &mut (dyn Write + Send),
    ) -> Result<ExecExit, Box<WireFailure>> {
        self.run_in(request, None, stdout, stderr)
    }

    /// Runs `request` once while this Sandbox's daemon runs: as its uid/gid
    /// with no supplementary groups, in its directory, its stdout and stderr
    /// streamed to the sinks as they arrive, then waits for its exit. A sink
    /// failure closes the command's pipes, still waits for its exit, and
    /// returns the sink's error. Standard input is not offered. The exit
    /// code is 128 + the signal for a command a signal ended, as the Engine
    /// reports it; the process id is diagnostic only. A `slot` is told the
    /// command's stop handle, its process group and leader, before its
    /// output is read; a slot that fails ends the group at once. One that
    /// refuses it as interrupted (a cancel came first) lets the command run
    /// on to that end, which its exit code then reports.
    pub fn run_in(
        &self,
        request: &ExecRequest,
        mut slot: Option<ExecSlot<'_>>,
        stdout: &mut (dyn Write + Send),
        stderr: &mut (dyn Write + Send),
    ) -> Result<ExecExit, Box<WireFailure>> {
        check(request).map_err(|cause| self.failure(cause))?;
        self.endpoint()?;
        let mut command = Command::new(&request.arguments[0]);
        command
            .args(&request.arguments[1..])
            .env_clear()
            .env("PATH", search_path())
            .envs(user_environment())
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
        let told = match (&named, slot.as_mut()) {
            (Ok(leader), Some(slot)) => (slot.spawned)(&Stop::Group {
                pgid: pid,
                leader: *leader,
            }),
            _ => Ok(()),
        };
        let leader = match (named, told) {
            (Ok(leader), Ok(())) => leader,
            // a cancel came before the handle was told: the command ends
            // here and runs on to that end, which its exit reports
            (Ok(leader), Err(error)) if error.kind() == io::ErrorKind::Interrupted => {
                if let Ok(group) = process::pid(pid) {
                    let _ = killpg(group, Signal::SIGKILL);
                }
                leader
            }
            (Err(error), _) | (_, Err(error)) => {
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
            let errors = scope.spawn(move || streams::copy(err, stderr));
            let outs = streams::copy(out, stdout);
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
        Ok(ExecExit {
            code,
            pid: Some(pid),
            stdout_bytes: out_bytes,
            stderr_bytes: err_bytes,
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

/// The caller's Rust and Git locations on macOS, when present, so a command
/// whose `HOME` is the Sandbox's scratch directory still sees them unless the
/// caller's environment or the request sets its own.
#[cfg(target_os = "macos")]
fn user_environment() -> Vec<(&'static str, OsString)> {
    use std::path::PathBuf;
    let home = non_empty_env("HOME").map(PathBuf::from);
    let existing = |path: PathBuf| path.exists().then(|| path.into_os_string());
    let mut vars = Vec::new();
    for (name, subdir) in [("RUSTUP_HOME", ".rustup"), ("CARGO_HOME", ".cargo")] {
        let value =
            non_empty_env(name).or_else(|| home.as_ref().and_then(|h| existing(h.join(subdir))));
        if let Some(value) = value {
            vars.push((name, value));
        }
    }
    let git_config = non_empty_env("GIT_CONFIG_GLOBAL")
        .or_else(|| home.as_ref().and_then(|h| existing(h.join(".gitconfig"))))
        .or_else(|| {
            let xdg = non_empty_env("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| home.as_ref().map(|h| h.join(".config")))?;
            existing(xdg.join("git").join("config"))
        });
    if let Some(value) = git_config {
        vars.push(("GIT_CONFIG_GLOBAL", value));
    }
    vars
}

#[cfg(not(target_os = "macos"))]
fn user_environment() -> Vec<(&'static str, OsString)> {
    Vec::new()
}

#[cfg(target_os = "macos")]
fn non_empty_env(name: &str) -> Option<OsString> {
    std::env::var_os(name).filter(|value| !value.is_empty())
}
