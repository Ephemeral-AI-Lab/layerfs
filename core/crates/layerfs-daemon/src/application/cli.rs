//! Thin actual deployment CLI; no launcher or shell command mode.
use super::{config, Application, ApplicationError};
use std::{
    io::{self, Write},
    net::TcpListener,
    path::Path,
    process::ExitCode,
};

/// Runs the Linux filesystem application from one protected configuration file.
/// Args/env carry no key and no command; ordinary runtime execution is separate.
pub fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(cause) => {
            eprintln!("{cause}");
            ExitCode::FAILURE
        }
    }
}
fn run() -> Result<(), ApplicationError> {
    let mut args = std::env::args_os();
    let _program = args.next();
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--config")) {
        return Err(ApplicationError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: layerfs-daemon --config <protected-file>",
        )));
    }
    let path = args.next().ok_or_else(|| {
        ApplicationError::Io(io::Error::new(io::ErrorKind::InvalidInput, "config path"))
    })?;
    if args.next().is_some() {
        return Err(ApplicationError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unexpected argument",
        )));
    }
    let setup = config::read(Path::new(&path)).map_err(ApplicationError::Io)?;
    let listener = TcpListener::bind(&setup.listen).map_err(ApplicationError::Io)?;
    let application = Application::start(setup)?;
    println!(
        "LAYERFS_DAEMON_LISTEN {}",
        listener.local_addr().map_err(ApplicationError::Io)?
    );
    io::stdout().flush().map_err(ApplicationError::Io)?;
    application.serve(listener)
}
