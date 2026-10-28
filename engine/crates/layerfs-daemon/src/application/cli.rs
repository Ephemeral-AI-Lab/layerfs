//! Thin actual deployment CLI; no launcher or shell command mode beyond the
//! hidden `ns-exec` helper that joins this daemon's mount namespace.
use super::{config, Application, ApplicationError};
#[cfg(target_os = "linux")]
use std::fs;
use std::{
    ffi::{OsStr, OsString},
    io::{self, Write},
    net::TcpListener,
    path::Path,
    process::ExitCode,
};

/// Runs the Linux filesystem application from one protected configuration file.
/// Args/env carry no key and no command; ordinary runtime execution is separate.
/// The one exception is `ns-exec`, a helper the Sandbox runtime starts through
/// the Engine so a command can run in this daemon's private mount namespace.
pub fn main() -> ExitCode {
    let mut args = std::env::args_os();
    let _program = args.next();
    let args: Vec<OsString> = args.collect();
    if args
        .first()
        .is_some_and(|flag| flag == OsStr::new("ns-exec"))
    {
        return ns_exec(&args[1..]);
    }
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(cause) => {
            eprintln!("{cause}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(target_os = "linux")]
fn ns_exec(args: &[OsString]) -> ExitCode {
    super::ns_exec::main(args)
}

#[cfg(not(target_os = "linux"))]
fn ns_exec(_args: &[OsString]) -> ExitCode {
    eprintln!("layerfs-daemon ns-exec: requires Linux");
    ExitCode::FAILURE
}

fn run(args: &[OsString]) -> Result<(), ApplicationError> {
    if args.first().map(OsString::as_os_str) != Some(OsStr::new("--config")) {
        return Err(ApplicationError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: layerfs-daemon --config <protected-file>",
        )));
    }
    let path = args.get(1).ok_or_else(|| {
        ApplicationError::Io(io::Error::new(io::ErrorKind::InvalidInput, "config path"))
    })?;
    if args.len() != 2 {
        return Err(ApplicationError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unexpected argument",
        )));
    }
    let setup = config::read(Path::new(path)).map_err(ApplicationError::Io)?;
    // Before the listener and any worker thread: the daemon takes its own
    // mount namespace so a Workspace FUSE mount is absent from the namespace
    // an Engine exec or a `docker exec` enters (prototype for #3/#499). A
    // failure refuses startup rather than serving with the old topology.
    #[cfg(target_os = "linux")]
    let _ns_fd = if setup.private_mount_ns {
        layerfs_fuse::mount::private_namespace().map_err(ApplicationError::Io)?;
        if let Some(pid_file) = setup.mount_ns_pid_file() {
            write_pid_file(&pid_file).map_err(ApplicationError::Io)?;
        }
        let socket = setup.mount_ns_socket().ok_or_else(|| {
            ApplicationError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "private mount namespace without a socket path",
            ))
        })?;
        Some(super::ns_fd::NsFd::start(&socket).map_err(ApplicationError::Io)?)
    } else {
        None
    };
    let listener = TcpListener::bind(&setup.listen).map_err(ApplicationError::Io)?;
    let application = Application::start(setup)?;
    println!(
        "LAYERFS_DAEMON_LISTEN {}",
        listener.local_addr().map_err(ApplicationError::Io)?
    );
    io::stdout().flush().map_err(ApplicationError::Io)?;
    application.watch_idle().map_err(ApplicationError::Io)?;
    application.serve(listener)
}

/// One atomic publish of this process's pid: a temporary file beside the
/// named one, then a rename. The directory exists because the protected
/// configuration already created the Overlay's own directory there.
#[cfg(target_os = "linux")]
fn write_pid_file(path: &Path) -> io::Result<()> {
    let temporary = path.with_extension(format!("tmp{}", std::process::id()));
    fs::write(&temporary, format!("{}\n", std::process::id()))?;
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(())
}
