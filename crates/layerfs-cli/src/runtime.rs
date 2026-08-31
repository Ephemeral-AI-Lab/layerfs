#![cfg(unix)]

use crate::{invoke_session, CliError, CliResult, CliSession};
use std::ffi::OsString;
use std::io::{ErrorKind, Read, Write};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const REQUEST_MAGIC: [u8; 4] = *b"LFC1";
const RESPONSE_MAGIC: [u8; 4] = *b"LFR1";
const MAX_ARGUMENTS: usize = 1024;
const MAX_ARGUMENT_BYTES: usize = 8 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

pub(crate) fn dispatch(
    context: &Path,
    arguments: Vec<OsString>,
    json: bool,
    output: &mut dyn Write,
) -> CliResult<i32> {
    let family = command_family(&arguments);
    if matches!(family, Family::Local) {
        if owner_active(context) && is_context_change(&arguments) {
            writeln!(
                output,
                "FAILED stop the active container or Workspace before changing context"
            )?;
            return Ok(1);
        }
        return crate::invoke(context, arguments, json, output);
    }
    if owner_active(context) {
        return remote(context, &arguments, json, output);
    }
    if matches!(family, Family::StartOwner) {
        start_owner(context)?;
        return remote(context, &arguments, json, output);
    }
    if matches!(family, Family::RequiresOwner) {
        writeln!(
            output,
            "FAILED no active Workspace context; create a Workspace or start its container first"
        )?;
        return Ok(1);
    }
    crate::invoke(context, arguments, json, output)
}

pub(crate) fn serve(context: PathBuf) -> CliResult<()> {
    let socket = socket_path(&context);
    if socket.exists() {
        if UnixStream::connect(&socket).is_ok() {
            return Err(CliError::Context);
        }
        std::fs::remove_file(&socket)?;
    }
    if let Some(parent) = socket.parent() {
        std::fs::create_dir_all(parent)?;
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
    }
    let listener = UnixListener::bind(&socket)?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    let session = Arc::new(CliSession::open(&context)?);
    let shutdown = Arc::new(AtomicBool::new(false));
    let container_bound = Arc::new(AtomicBool::new(false));
    let mut workers: Vec<std::thread::JoinHandle<()>> = Vec::new();
    while !shutdown.load(Ordering::Acquire) {
        let mut index = 0;
        while index < workers.len() {
            if workers[index].is_finished() {
                let worker = workers.swap_remove(index);
                let _ = worker.join();
            } else {
                index += 1;
            }
        }
        match listener.accept() {
            Ok((stream, _)) => {
                let session = session.clone();
                let shutdown = shutdown.clone();
                let container_bound = container_bound.clone();
                workers.push(std::thread::spawn(move || {
                    let _ = handle(stream, session, shutdown, container_bound);
                }));
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error.into()),
        }
    }
    for worker in workers {
        let _ = worker.join();
    }
    drop(listener);
    if socket.exists() {
        std::fs::remove_file(socket)?;
    }
    Ok(())
}

fn handle(
    mut stream: UnixStream,
    session: Arc<CliSession>,
    shutdown: Arc<AtomicBool>,
    container_bound: Arc<AtomicBool>,
) -> CliResult<()> {
    let (arguments, json) = read_request(&mut stream)?;
    let start_container = matches!(command_family(&arguments), Family::StartOwner)
        && argument(&arguments, 0) == Some("container")
        && argument(&arguments, 1) == Some("start");
    let stop_container =
        argument(&arguments, 0) == Some("container") && argument(&arguments, 1) == Some("stop");
    let end_workspace =
        argument(&arguments, 0) == Some("workspace") && argument(&arguments, 1) == Some("end");
    let create_workspace =
        argument(&arguments, 0) == Some("workspace") && argument(&arguments, 1) == Some("create");
    let mut bytes = Vec::new();
    let code = invoke_session(&session, arguments, json, &mut bytes).unwrap_or_else(|error| {
        let _ = writeln!(bytes, "FAILED {error}");
        2
    });
    write_response(&mut stream, code, &bytes)?;
    if code == 0 && start_container {
        container_bound.store(true, Ordering::Release);
    }
    if code == 0 && stop_container {
        shutdown.store(true, Ordering::Release);
    }
    if code == 0 && end_workspace && !container_bound.load(Ordering::Acquire) && session.idle()? {
        shutdown.store(true, Ordering::Release);
    }
    if create_workspace && !container_bound.load(Ordering::Acquire) && session.idle()? {
        shutdown.store(true, Ordering::Release);
    }
    Ok(())
}

fn start_owner(context: &Path) -> CliResult<()> {
    if owner_active(context) {
        return Ok(());
    }
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("__layerfs_context_owner")
        .arg(context)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    let mut child = command.spawn()?;
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if owner_active(context) {
            return Ok(());
        }
        if child.try_wait()?.is_some() {
            return Err(CliError::Context);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Err(CliError::Context)
}

fn owner_active(context: &Path) -> bool {
    UnixStream::connect(socket_path(context)).is_ok()
}

fn remote(
    context: &Path,
    arguments: &[OsString],
    json: bool,
    output: &mut dyn Write,
) -> CliResult<i32> {
    let mut stream = UnixStream::connect(socket_path(context))?;
    write_request(&mut stream, arguments, json)?;
    let (code, bytes) = read_response(&mut stream)?;
    output.write_all(&bytes)?;
    Ok(code)
}

fn write_request(stream: &mut UnixStream, arguments: &[OsString], json: bool) -> CliResult<()> {
    if arguments.len() > MAX_ARGUMENTS {
        return Err(CliError::Parse("argument count"));
    }
    stream.write_all(&REQUEST_MAGIC)?;
    stream.write_all(&[u8::from(json)])?;
    stream.write_all(&(arguments.len() as u16).to_be_bytes())?;
    let mut total = 0_usize;
    for argument in arguments {
        let bytes = argument.as_os_str().as_bytes();
        total = total.saturating_add(bytes.len());
        if total > MAX_ARGUMENT_BYTES || bytes.len() > u32::MAX as usize {
            return Err(CliError::Parse("argument bytes"));
        }
        stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
        stream.write_all(bytes)?;
    }
    Ok(())
}

fn read_request(stream: &mut UnixStream) -> CliResult<(Vec<OsString>, bool)> {
    let mut magic = [0; 4];
    stream.read_exact(&mut magic)?;
    if magic != REQUEST_MAGIC {
        return Err(CliError::Parse("control protocol"));
    }
    let mut flag = [0];
    stream.read_exact(&mut flag)?;
    if flag[0] > 1 {
        return Err(CliError::Parse("control flags"));
    }
    let count = read_u16(stream)? as usize;
    if count > MAX_ARGUMENTS {
        return Err(CliError::Parse("argument count"));
    }
    let mut total = 0_usize;
    let mut arguments = Vec::with_capacity(count);
    for _ in 0..count {
        let length = read_u32(stream)? as usize;
        total = total.saturating_add(length);
        if total > MAX_ARGUMENT_BYTES {
            return Err(CliError::Parse("argument bytes"));
        }
        let mut bytes = vec![0; length];
        stream.read_exact(&mut bytes)?;
        arguments.push(OsString::from_vec(bytes));
    }
    Ok((arguments, flag[0] == 1))
}

fn write_response(stream: &mut UnixStream, code: i32, bytes: &[u8]) -> CliResult<()> {
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(CliError::Parse("response bytes"));
    }
    stream.write_all(&RESPONSE_MAGIC)?;
    stream.write_all(&code.to_be_bytes())?;
    stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
    stream.write_all(bytes)?;
    Ok(())
}

fn read_response(stream: &mut UnixStream) -> CliResult<(i32, Vec<u8>)> {
    let mut magic = [0; 4];
    stream.read_exact(&mut magic)?;
    if magic != RESPONSE_MAGIC {
        return Err(CliError::Parse("control protocol"));
    }
    let mut status = [0; 4];
    stream.read_exact(&mut status)?;
    let length = read_u32(stream)? as usize;
    if length > MAX_RESPONSE_BYTES {
        return Err(CliError::Parse("response bytes"));
    }
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes)?;
    Ok((i32::from_be_bytes(status), bytes))
}

fn read_u16(stream: &mut UnixStream) -> CliResult<u16> {
    let mut bytes = [0; 2];
    stream.read_exact(&mut bytes)?;
    Ok(u16::from_be_bytes(bytes))
}

fn read_u32(stream: &mut UnixStream) -> CliResult<u32> {
    let mut bytes = [0; 4];
    stream.read_exact(&mut bytes)?;
    Ok(u32::from_be_bytes(bytes))
}

fn socket_path(context: &Path) -> PathBuf {
    let absolute = if context.is_absolute() {
        context.to_owned()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(context)
    };
    let uid = std::fs::metadata(context)
        .map(|metadata| metadata.uid())
        .unwrap_or_else(|_| {
            std::fs::metadata(".")
                .map(|metadata| metadata.uid())
                .unwrap_or(0)
        });
    let root = Path::new("/tmp").join(format!("layerfs-cli-{uid}"));
    let digest = blake3::hash(absolute.as_os_str().as_bytes()).to_hex();
    root.join(format!("{digest}.socket"))
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Family {
    Local,
    StartOwner,
    RequiresOwner,
    Either,
}

fn command_family(arguments: &[OsString]) -> Family {
    match (argument(arguments, 0), argument(arguments, 1)) {
        (Some("db" | "context"), _) => Family::Local,
        (Some("container"), Some("create" | "status" | "remove")) => Family::Local,
        (Some("container"), Some("start")) | (Some("workspace"), Some("create")) => {
            Family::StartOwner
        }
        (Some("workspace"), _) => Family::RequiresOwner,
        _ => Family::Either,
    }
}

fn argument(arguments: &[OsString], index: usize) -> Option<&str> {
    arguments.get(index).and_then(|value| value.to_str())
}

fn is_context_change(arguments: &[OsString]) -> bool {
    argument(arguments, 0) == Some("context") && argument(arguments, 1) == Some("use")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_protocol_round_trips_arguments() {
        let (mut left, mut right) = UnixStream::pair().unwrap();
        let arguments = vec![OsString::from("workspace"), OsString::from("create")];
        write_request(&mut left, &arguments, true).unwrap();
        let (actual, json) = read_request(&mut right).unwrap();
        assert_eq!(actual, arguments);
        assert!(json);
    }
}
