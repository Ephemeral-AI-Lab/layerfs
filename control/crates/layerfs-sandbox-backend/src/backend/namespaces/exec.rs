//! One ordinary command in a namespace Sandbox: the helper enters the
//! Sandbox's mount, PID and IPC namespaces, becomes uid:gid with no
//! capabilities and no-new-privs, and runs the command there. The streams are
//! the helper's own pipes; nothing of this process's environment but the
//! search path reaches it. Its standard input is the request's bytes, given
//! whole once it runs, or `/dev/null` with none.
use super::{cgroup, sandbox::Sandbox};
use crate::runtime::cancel::{self, ExecSlot, Stop};
use crate::runtime::trace;
use crate::shared::search_path::search_path;
use crate::shared::streams;
use crate::ExecExit;
use crate::{ExecRequest, RuntimeError, WireFailure};
use std::{
    io::{self, Read, Write},
    os::fd::OwnedFd,
    os::unix::net::UnixStream,
    os::unix::process::{CommandExt, ExitStatusExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

/// Why a command cannot run in a namespaces Sandbox whose folder has no
/// helper copy of its own (`bin/layerfs-sandbox-helper`): an earlier build
/// made it, and this one runs no other helper in its place. The caller
/// names the records-only `sandbox remove` as the fix.
pub const NO_HELPER_COPY: &str = "this namespaces Sandbox has no helper copy in its own folder \
     (an earlier build made it), and this build runs no other helper in its place";

impl Sandbox {
    /// [`Sandbox::run_in`] with no exec id: the command joins the Sandbox's
    /// own group.
    pub fn run(
        &self,
        request: &ExecRequest,
        stdout: &mut (dyn Write + Send),
        stderr: &mut (dyn Write + Send),
    ) -> Result<ExecExit, Box<WireFailure>> {
        self.run_in(request, None, stdout, stderr)
    }

    /// Runs `request` once while this Sandbox's init stage runs, as its
    /// uid:gid, in its directory inside the Sandbox, its stdout and stderr
    /// streamed to the sinks as they arrive, then waits for its exit. A sink
    /// failure closes the command's pipes, still waits for its exit, and
    /// returns the sink's error. With a `slot`, the command runs in a group
    /// of its own (`ws-<workspace>/exec-<exec>`, its stop handle), which the
    /// helper never joins: a cancel ends only the command, and the helper
    /// reports how it ended. The slot is told the stop handle only once the
    /// command is in its group (the helper says so on a socket); a slot
    /// that refuses it (a cancel came first) has the command ended at once.
    pub fn run_in(
        &self,
        request: &ExecRequest,
        slot: Option<ExecSlot<'_>>,
        stdout: &mut (dyn Write + Send),
        stderr: &mut (dyn Write + Send),
    ) -> Result<ExecExit, Box<WireFailure>> {
        check(request).map_err(|cause| self.failure(cause))?;
        let init = self
            .init
            .ok_or_else(|| self.invalid("running command admission"))?;
        self.endpoint()?;
        trace::mark("exec.endpoint");
        let group = match &slot {
            Some(slot) => Some(self.exec_group(slot)?),
            None => None,
        };
        trace::mark("exec.cgroup_create");
        let told = group.as_deref().zip(slot);
        let outcome = self.spawn_in(request, init.pid, told, stdout, stderr);
        if let Some(group) = &group {
            cgroup::remove_exec(group);
        }
        trace::mark("exec.cgroup_remove");
        outcome
    }

    /// The stop handle of command `exec` of Workspace `workspace`: its
    /// group follows from this Sandbox's and the two ids, so whoever holds
    /// the command keeps no table of them.
    pub fn exec_stop(&self, workspace: &str, exec: &str) -> Option<Stop> {
        (cancel::component(workspace) && cancel::component(exec)).then(|| Stop::Cgroup {
            path: cgroup::exec_path(&self.cgroup, workspace, exec),
        })
    }

    /// The group of the command `slot` names, made new. The slot is told
    /// it only once the command is in it ([`Sandbox::spawn_in`]).
    fn exec_group(&self, slot: &ExecSlot<'_>) -> Result<PathBuf, Box<WireFailure>> {
        if !cancel::component(slot.workspace) || !cancel::component(slot.exec) {
            return Err(self.failure(RuntimeError::Protocol("exec group names")));
        }
        let group = cgroup::exec_path(&self.cgroup, slot.workspace, slot.exec);
        cgroup::create_exec(&group).map_err(|error| self.io(error))?;
        Ok(group)
    }

    /// Spawns the helper for `request` and waits for it; with `group`, the
    /// command's own group and the slot to tell once the command is in it.
    /// While this thread's steps are timed, the helper writes its stages'
    /// times to a file of its own (`--trace`, [`trace::helper_file`]).
    fn spawn_in(
        &self,
        request: &ExecRequest,
        init: u32,
        group: Option<(&Path, ExecSlot<'_>)>,
        stdout: &mut (dyn Write + Send),
        stderr: &mut (dyn Write + Send),
    ) -> Result<ExecExit, Box<WireFailure>> {
        // a reopened Sandbox runs the helper copy in its own folder; one
        // made before that copy existed has none, and nothing else is used
        // in its place
        match std::fs::metadata(&self.helper) {
            Ok(metadata) if metadata.is_file() && metadata.len() > 0 => {}
            _ => {
                return Err(WireFailure::local(RuntimeError::Unsupported(
                    NO_HELPER_COPY,
                )))
            }
        }
        let mut command = Command::new(&self.helper);
        command
            .arg("exec")
            .arg("--init")
            .arg(init.to_string())
            .arg("--cgroup")
            .arg(cgroup::leaf(&self.cgroup));
        // the helper's stdin is one end of a socket: it writes one byte
        // there once the command is in its group, and its exit closes it
        // (and, with `--stdin inherit`, the command's standard input: the
        // request's bytes are written on this end once it runs)
        let mut ready = None;
        if let Some((path, _)) = &group {
            let (ours, theirs) = UnixStream::pair().map_err(|error| self.io(error))?;
            command
                .arg("--command-cgroup")
                .arg(path)
                .stdin(Stdio::from(OwnedFd::from(theirs)));
            ready = Some(ours);
        } else if request.stdin.is_some() {
            command.stdin(Stdio::piped());
        } else {
            command.stdin(Stdio::null());
        }
        if request.stdin.is_some() {
            command.arg("--stdin").arg("inherit");
        }
        let traced = trace::helper_file();
        if let Some(file) = &traced {
            command.arg("--trace").arg(file);
        }
        // a Sandbox started with the bundle's runtime layer (R5): its `bin`
        // ends the command's PATH, so an image with no `bash` still runs one
        if self.paths.runtime.is_dir() {
            command
                .arg("--runtime")
                .arg(layerfs_bridge::sandbox_runtime::GUEST);
        }
        command
            .arg("--uid")
            .arg(request.identity.uid.to_string())
            .arg("--gid")
            .arg(request.identity.gid.to_string())
            .arg("--cwd")
            .arg(&request.directory)
            .arg("--")
            .args(&request.arguments)
            .env_clear()
            .env("PATH", search_path())
            .current_dir(self.directory())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        for entry in &request.environment {
            if let Some((name, value)) = entry.split_once('=') {
                command.env(name, value);
            }
        }
        let spawned = traced.is_some().then(std::time::SystemTime::now);
        let child = command.spawn();
        trace::mark("exec.helper_spawn");
        let mut child = child.map_err(|error| {
            if let Some(file) = &traced {
                let _ = std::fs::remove_file(file);
            }
            let mut failure = self.io(error);
            failure.attempted = true;
            failure
        })?;
        let pid = child.id();
        // this process's copy of the helper's end goes, so the helper's
        // exit is an end of file here
        drop(command);
        let (refused, input) = match (ready, group) {
            (Some(mut ready), Some((path, slot))) => {
                let (refused, started) = told(&mut ready, path, slot);
                (refused, started.then_some(Input::Socket(ready)))
            }
            _ => (None, child.stdin.take().map(Input::Pipe)),
        };
        let out = child.stdout.take();
        let err = child.stderr.take();
        let copied = std::thread::scope(|scope| {
            scope.spawn(move || match input {
                Some(Input::Socket(socket)) => {
                    if let Some(bytes) = request.stdin.as_deref() {
                        let _ = (&socket).write_all(bytes);
                    }
                    let _ = socket.shutdown(std::net::Shutdown::Write);
                }
                Some(Input::Pipe(pipe)) => streams::feed(Some(pipe), request.stdin.as_deref()),
                None => {}
            });
            let errors = scope.spawn(move || streams::copy(err, stderr));
            let outs = streams::copy(out, stdout);
            let errs = errors
                .join()
                .unwrap_or_else(|_| Err(io::Error::other("stderr copy panicked")));
            Ok::<_, io::Error>((outs?, errs?))
        });
        // the pipes are closed here either way; the exit is still waited for
        let status = child.wait();
        trace::mark("exec.helper_wait");
        if let (Some(file), Some(spawned)) = (&traced, spawned) {
            trace::helper(file, spawned);
        }
        let (out_bytes, err_bytes) = copied.map_err(|error| self.io(error))?;
        let status = status.map_err(|error| self.io(error))?;
        if let Some(error) = refused {
            let mut failure = self.io(error);
            failure.attempted = true;
            return Err(failure);
        }
        let code = (status.code())
            .or_else(|| status.signal().map(|signal| 128 + signal))
            .ok_or_else(|| self.invalid("namespaces command exit status"))?;
        if status.code().is_none() {
            // the helper ends itself with a signal when it cannot set the
            // command up (or the Sandbox ended under it), so this is a
            // reported failure, never a command's exit code
            return Err(self.invalid("namespaces exec: the helper ended without a status"));
        }
        Ok(ExecExit {
            code,
            pid: Some(pid),
            stdout_bytes: out_bytes,
            stderr_bytes: err_bytes,
        })
    }
}

/// Where a command's standard input is written: the helper's socket (a
/// command in its own group), or the helper's stdin pipe.
enum Input {
    Socket(UnixStream),
    Pipe(std::process::ChildStdin),
}

/// Waits for the helper to say the command is in its group `path` (one
/// byte on `ready`), then tells `slot` its stop handle; with it, whether
/// the command started. Bounded by the helper: its exit ends the wait,
/// with nothing told (the command did not start). A slot that refuses it
/// as interrupted (a cancel came first) has the command ended at once with
/// the kill and runs on to its exit, which then reports the signal; any
/// other refusal ends it too and is the error returned.
fn told(ready: &mut UnixStream, path: &Path, slot: ExecSlot<'_>) -> (Option<io::Error>, bool) {
    let mut byte = [0u8; 1];
    if !matches!(ready.read(&mut byte), Ok(1)) {
        return (None, false);
    }
    let handle = Stop::Cgroup {
        path: path.to_path_buf(),
    };
    let Err(error) = (slot.spawned)(&handle) else {
        return (None, true);
    };
    // the cancel was answered as SIGKILL: no SIGTERM first, which a
    // default disposition makes fatal at once and reports as 143
    let _ = cancel::kill(&handle);
    (
        (error.kind() != io::ErrorKind::Interrupted).then_some(error),
        true,
    )
}

/// The Engine's own checks of an ordinary command.
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
    Ok(())
}
