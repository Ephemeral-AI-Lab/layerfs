use layerfs_bridge::{adapters::native::payload::Output, contract::*};
use std::{
    io::Read,
    os::{fd::AsRawFd, unix::process::CommandExt},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
fn nonblocking(pipe: &impl AsRawFd) -> Result<(), Failure> {
    let fd = pipe.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(Code::Io.into());
    }
    Ok(())
}
fn drain(
    pipe: &mut Option<impl Read>,
    out: &mut Vec<u8>,
    truncated: &mut bool,
) -> Result<(), Failure> {
    let Some(p) = pipe.as_mut() else {
        return Ok(());
    };
    let mut buffer = [0; 4096];
    for _ in 0..16 {
        match p.read(&mut buffer) {
            Ok(0) => {
                *pipe = None;
                return Ok(());
            }
            Ok(n) => {
                let keep = n.min(WORKSPACE_EXEC_OUTPUT_BYTES.saturating_sub(out.len()));
                out.extend_from_slice(&buffer[..keep]);
                *truncated |= keep != n;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
            Err(_) => return Err(Code::Io.into()),
        }
    }
    Ok(())
}
pub fn execute(
    path: &Path,
    workspace: &[u8],
    incarnation: [u8; 32],
    command: &[u8],
    output: &mut Output<'_>,
    deadline: Instant,
) -> Result<Response, Failure> {
    let command = std::str::from_utf8(command).map_err(|_| Code::InvalidInput)?;
    let mut child = Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .current_dir(path)
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let group = child.id() as i32;
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    nonblocking(stdout.as_ref().ok_or(Code::Io)?)?;
    nonblocking(stderr.as_ref().ok_or(Code::Io)?)?;
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut out_truncated = false;
    let mut err_truncated = false;
    let result = (|| {
        let mut exit = None;
        while stdout.is_some() || stderr.is_some() || exit.is_none() {
            if Instant::now() >= deadline {
                return Err(Code::Deadline.into());
            }
            drain(&mut stdout, &mut out, &mut out_truncated)?;
            drain(&mut stderr, &mut err, &mut err_truncated)?;
            if exit.is_none() {
                exit = child.try_wait()?;
            }
            output.progress()?;
            if stdout.is_some() || stderr.is_some() || exit.is_none() {
                std::thread::park_timeout(Duration::from_millis(1));
            }
        }
        Ok(exit.unwrap())
    })();
    match result {
        Ok(status) => Ok(Response::WorkspaceExec(Box::new(WorkspaceExecWire {
            workspace: workspace.to_vec(),
            incarnation,
            exit_status: status.code(),
            stdout: out,
            stderr: err,
            stdout_truncated: out_truncated,
            stderr_truncated: err_truncated,
        }))),
        Err(e) => {
            unsafe { libc::kill(-group, libc::SIGKILL) };
            let _ = child.wait();
            Err(e)
        }
    }
}
