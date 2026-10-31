//! One ordinary command in a namespace Sandbox: the helper enters the
//! Sandbox's mount, PID and IPC namespaces, becomes uid:gid with no
//! capabilities and no-new-privs, and runs the command there. The streams are
//! the helper's own pipes; nothing of this process's environment but the
//! search path reaches it, and no stdin is offered.
use super::sandbox::Sandbox;
use crate::backend::native::search::search_path;
use crate::backend::streams;
use crate::provider::ExecExit;
use crate::{ExecRequest, RuntimeError, WireFailure};
use std::{
    io::{self, Write},
    os::unix::process::{CommandExt, ExitStatusExt},
    path::Path,
    process::{Command, Stdio},
};

impl Sandbox {
    /// Runs `request` once while this Sandbox's init stage runs, as its
    /// uid:gid, in its directory inside the Sandbox, its stdout and stderr
    /// streamed to the sinks as they arrive, then waits for its exit. A sink
    /// failure closes the command's pipes, still waits for its exit, and
    /// returns the sink's error.
    pub fn run(
        &self,
        request: &ExecRequest,
        stdout: &mut (dyn Write + Send),
        stderr: &mut (dyn Write + Send),
    ) -> Result<ExecExit, Box<WireFailure>> {
        check(request).map_err(|cause| self.failure(cause))?;
        let init = self
            .init
            .ok_or_else(|| self.invalid("running command admission"))?;
        self.endpoint()?;
        let mut command = Command::new(&self.helper);
        command
            .arg("exec")
            .arg("--init")
            .arg(init.pid.to_string())
            .arg("--cgroup")
            .arg(&self.cgroup)
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
        let mut child = command.spawn().map_err(|error| {
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
