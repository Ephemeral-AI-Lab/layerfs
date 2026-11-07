//! One ordinary command in a namespace Sandbox: the helper enters the
//! Sandbox's mount, PID and IPC namespaces, becomes uid:gid with no
//! capabilities and no-new-privs, and runs the command there. The streams are
//! the helper's own pipes; nothing of this process's environment but the
//! search path reaches it, and no stdin is offered.
use super::{cgroup, sandbox::Sandbox};
use crate::local::search::search_path;
use crate::local::streams;
use crate::steps;
use crate::stop::{self, ExecSlot, Stop};
use crate::ExecExit;
use crate::{ExecRequest, RuntimeError, WireFailure};
use std::{
    io::{self, Write},
    os::unix::process::{CommandExt, ExitStatusExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

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
    /// of its own (`ws-<workspace>/exec-<exec>`, its stop handle, told to
    /// the slot before it starts); the helper itself goes back to the
    /// Sandbox's group once the command started, so a cancel ends only the
    /// command and the helper reports how it ended.
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
        steps::mark("exec.endpoint");
        let group = match slot {
            Some(slot) => Some(self.exec_group(slot)?),
            None => None,
        };
        steps::mark("exec.cgroup_create");
        let outcome = self.spawn_in(request, init.pid, group.as_deref(), stdout, stderr);
        if let Some(group) = &group {
            cgroup::remove_exec(group);
        }
        steps::mark("exec.cgroup_remove");
        outcome
    }

    /// The stop handle of command `exec` of Workspace `workspace`: its
    /// group follows from this Sandbox's and the two ids, so whoever holds
    /// the command keeps no table of them.
    pub fn exec_stop(&self, workspace: &str, exec: &str) -> Option<Stop> {
        (stop::component(workspace) && stop::component(exec)).then(|| Stop::Cgroup {
            path: cgroup::exec_path(&self.cgroup, workspace, exec),
        })
    }

    /// The group of the command `slot` names, made new and told to the slot
    /// before anything starts in it.
    fn exec_group(&self, slot: ExecSlot<'_>) -> Result<PathBuf, Box<WireFailure>> {
        if !stop::component(slot.workspace) || !stop::component(slot.exec) {
            return Err(self.failure(RuntimeError::Protocol("exec group names")));
        }
        let group = cgroup::exec_path(&self.cgroup, slot.workspace, slot.exec);
        cgroup::create_exec(&group).map_err(|error| self.io(error))?;
        let told = (slot.spawned)(&Stop::Cgroup {
            path: group.clone(),
        });
        if let Err(error) = told {
            cgroup::remove_exec(&group);
            return Err(self.io(error));
        }
        Ok(group)
    }

    /// Spawns the helper for `request` in group `group` (the Sandbox's own
    /// when `None`) and waits for it. While this thread's steps are timed,
    /// the helper writes its stages' times to a file of its own
    /// (`--trace`, [`steps::helper_file`]).
    fn spawn_in(
        &self,
        request: &ExecRequest,
        init: u32,
        group: Option<&Path>,
        stdout: &mut (dyn Write + Send),
        stderr: &mut (dyn Write + Send),
    ) -> Result<ExecExit, Box<WireFailure>> {
        let mut command = Command::new(&self.helper);
        command
            .arg("exec")
            .arg("--init")
            .arg(init.to_string())
            .arg("--cgroup")
            .arg(group.unwrap_or(&self.cgroup));
        if group.is_some() {
            command.arg("--leave").arg(&self.cgroup);
        }
        let traced = steps::helper_file();
        if let Some(file) = &traced {
            command.arg("--trace").arg(file);
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
            .stdin(Stdio::null())
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
        steps::mark("exec.helper_spawn");
        let mut child = child.map_err(|error| {
            if let Some(file) = &traced {
                let _ = std::fs::remove_file(file);
            }
            let mut failure = self.io(error);
            failure.attempted = true;
            failure
        })?;
        let pid = child.id();
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
        steps::mark("exec.helper_wait");
        if let (Some(file), Some(spawned)) = (&traced, spawned) {
            steps::helper(file, spawned);
        }
        let (out_bytes, err_bytes) = copied.map_err(|error| self.io(error))?;
        let status = status.map_err(|error| self.io(error))?;
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
