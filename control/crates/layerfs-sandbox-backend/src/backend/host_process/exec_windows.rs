//! The Windows exec: one command, spawned straight into the Sandbox
//! directory with its environment, its streams counted. One exact
//! attempt; a stop kills the child (A1; the WinFsp mount is A2).
use crate::types::{ExecExit, ExecRequest};
use crate::{RuntimeError, WireFailure};
use std::{
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
};

/// Runs `request` in `directory` (the Sandbox directory, or its join with
/// the request own directory), answering its exit and stream sizes. The
/// identity is this user: Windows has no identity switch (in-process).
pub fn run(directory: &PathBuf, request: &ExecRequest) -> Result<ExecExit, Box<WireFailure>> {
    let fail = |e: std::io::Error| WireFailure::local(RuntimeError::Io(e));
    let Some(program) = request.arguments.first() else {
        return Err(WireFailure::local(RuntimeError::Protocol("exec arguments")));
    };
    let mut cwd = directory.clone();
    if !request.directory.is_empty() {
        let rel = request.directory.trim_start_matches('/');
        let rel = rel.trim_start_matches('\\');
        cwd = directory.join(rel);
    }
    let mut command = Command::new(program);
    command
        .args(&request.arguments[1..])
        .env_clear()
        .envs(request.environment.iter().filter_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            Some((key, value))
        }))
        .current_dir(&cwd)
        .stdin(if request.stdin { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(fail)?;
    let pid = child.id();
    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();
    let mut stdout_bytes: u64 = 0;
    let mut stderr_bytes: u64 = 0;
    let mut buffer = [0u8; 8192];
    loop {
        let mut progressed = false;
        if let Some(pipe) = stdout_pipe.as_mut() {
            match pipe.read(&mut buffer) {
                Ok(0) => { stdout_pipe = None; progressed = true; }
                Ok(n) => { stdout_bytes += n as u64; progressed = true; }
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Err(fail(e)),
            }
        }
        if let Some(pipe) = stderr_pipe.as_mut() {
            match pipe.read(&mut buffer) {
                Ok(0) => { stderr_pipe = None; progressed = true; }
                Ok(n) => { stderr_bytes += n as u64; progressed = true; }
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Err(fail(e)),
            }
        }
        if stdout_pipe.is_none() && stderr_pipe.is_none() {
            break;
        }
        if !progressed {
            if let Ok(Some(status)) = child.try_wait() {
                                if let Some(p) = stdout_pipe.as_mut() {
                    loop { match p.read(&mut buffer) { Ok(0) | Err(_) => break, Ok(n) => stdout_bytes += n as u64 } }
                }
                if let Some(p) = stderr_pipe.as_mut() {
                    loop { match p.read(&mut buffer) { Ok(0) | Err(_) => break, Ok(n) => stderr_bytes += n as u64 } }
                }
                return Ok(ExecExit { code: status.code().unwrap_or(-1), pid: Some(pid), stdout_bytes, stderr_bytes });
            }
        }
    }
    let status = child.wait().map_err(fail)?;
    Ok(ExecExit { code: status.code().unwrap_or(-1), pid: Some(pid), stdout_bytes, stderr_bytes })
}
