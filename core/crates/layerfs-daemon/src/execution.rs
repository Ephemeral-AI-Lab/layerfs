//! Bounded shell execution inside one mounted Workspace.
use layerfs_bridge::contract::{
    Code, Failure, Response, WorkspaceExecWire, WORKSPACE_EXEC_OUTPUT_BYTES,
};
use layerfs_workspace::Workspace;
use nix::{
    errno::Errno,
    fcntl::{fcntl, FcntlArg, OFlag},
    poll::{poll, PollFd, PollFlags},
    unistd::read,
};
use std::{
    os::{fd::AsFd, unix::process::CommandExt},
    process::{ChildStderr, ChildStdout, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn nonblocking(pipe: &impl std::os::fd::AsFd) -> Result<(), Failure> {
    let flags = OFlag::from_bits_truncate(fcntl(pipe, FcntlArg::F_GETFL).map_err(|_| Code::Io)?);
    fcntl(pipe, FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK)).map_err(|_| Code::Io)?;
    Ok(())
}

fn drain(
    pipe: &mut Option<impl std::os::fd::AsFd>,
    output: &mut Vec<u8>,
    truncated: &mut bool,
) -> Result<(), Failure> {
    let Some(current) = pipe.as_ref() else {
        return Ok(());
    };
    let mut buffer = [0; 4096];
    for _ in 0..16 {
        match read(current, &mut buffer) {
            Ok(0) => {
                *pipe = None;
                return Ok(());
            }
            Ok(count) => {
                let keep = count.min(WORKSPACE_EXEC_OUTPUT_BYTES.saturating_sub(output.len()));
                output.extend_from_slice(&buffer[..keep]);
                *truncated |= keep != count;
            }
            Err(Errno::EAGAIN) => return Ok(()),
            Err(_) => return Err(Code::Io.into()),
        }
    }
    Ok(())
}

pub(crate) fn execute(
    workspace: &Workspace,
    incarnation: &[u8; 32],
    command: &[u8],
    deadline: Instant,
) -> Result<Response, Failure> {
    let command = std::str::from_utf8(command).map_err(|_| Code::InvalidInput)?;
    let mut child = Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .current_dir(workspace.mount_path())
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                Code::Unsupported
            } else {
                Code::Io
            }
        })?;
    let pid = child.id() as i32;
    let mut stdout: Option<ChildStdout> = child.stdout.take();
    let mut stderr: Option<ChildStderr> = child.stderr.take();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut out_truncated = false;
    let mut err_truncated = false;
    let result = (|| -> Result<_, Failure> {
        nonblocking(stdout.as_ref().ok_or(Code::Io)?)?;
        nonblocking(stderr.as_ref().ok_or(Code::Io)?)?;
        let mut status = None;
        loop {
            drain(&mut stdout, &mut out, &mut out_truncated)?;
            drain(&mut stderr, &mut err, &mut err_truncated)?;
            if status.is_none() {
                status = child.try_wait()?;
            }
            if status.is_some() && stdout.is_none() && stderr.is_none() {
                return Ok(status);
            }
            if Instant::now() >= deadline {
                return Err(Failure {
                    unknown: true,
                    ..Code::Deadline.into()
                });
            }
            let mut ready = Vec::with_capacity(2);
            if let Some(pipe) = stdout.as_ref() {
                ready.push(PollFd::new(pipe.as_fd(), PollFlags::POLLIN));
            }
            if let Some(pipe) = stderr.as_ref() {
                ready.push(PollFd::new(pipe.as_fd(), PollFlags::POLLIN));
            }
            if ready.is_empty() {
                thread::sleep(Duration::from_millis(5));
            } else {
                match poll(&mut ready, 5u16) {
                    Ok(_) | Err(Errno::EINTR) => {}
                    Err(_) => return Err(Code::Io.into()),
                }
            }
        }
    })();
    let status = match result {
        Ok(status) => status,
        Err(mut failure) => {
            let _ = nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(pid),
                nix::sys::signal::Signal::SIGKILL,
            );
            let _ = child.wait();
            failure.unknown = true;
            return Err(failure);
        }
    };
    let result = WorkspaceExecWire {
        workspace: workspace.id().as_bytes().to_vec(),
        incarnation: *incarnation,
        exit_status: status.and_then(|status| status.code()),
        stdout: out,
        stderr: err,
        stdout_truncated: out_truncated,
        stderr_truncated: err_truncated,
    };
    result.validate()?;
    Ok(Response::WorkspaceExec(Box::new(result)))
}
