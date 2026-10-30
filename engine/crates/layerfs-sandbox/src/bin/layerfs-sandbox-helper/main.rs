//! The namespaces backend's helper: one single-threaded binary with two
//! stages, so the calls that act on the calling process (`unshare`, `setns`,
//! `pivot_root`, `PR_SET_NO_NEW_PRIVS`) run in a process of their own and the
//! product crate stays free of `unsafe`.
//!
//! - `init --spec <file>`: joins the Sandbox's cgroup, creates its mount, PID
//!   and IPC namespaces and starts the inner stage, which is PID 1 of them.
//! - `init-inner --spec <file>`: builds the private root, pivots to it, starts
//!   `layerfs-daemon` as root and reaps, as an init process does.
//! - `exec …`: enters a running Sandbox's namespaces, becomes the requested
//!   nonroot identity with no-new-privs and runs one command.
#![deny(unsafe_code)]

#[cfg(target_os = "linux")]
mod exec;
#[cfg(target_os = "linux")]
mod init;
#[cfg(target_os = "linux")]
mod mounts;

#[cfg(target_os = "linux")]
fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("layerfs-sandbox-helper: {error}");
            std::process::exit(EXIT_FAILURE);
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("layerfs-sandbox-helper: the namespaces Sandbox runtime is Linux only");
    std::process::exit(125);
}

/// A stage's own failure; a command's exit code is never this.
#[cfg(target_os = "linux")]
const EXIT_FAILURE: i32 = 125;

#[cfg(target_os = "linux")]
fn run() -> Result<i32, String> {
    let arguments: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let (stage, rest) = arguments.split_first().ok_or(USAGE)?;
    match stage.to_str() {
        Some("init") => init::outer(spec(rest)?),
        Some("init-inner") => init::inner(spec(rest)?),
        Some("exec") => exec::run(rest),
        _ => Err(USAGE.to_owned()),
    }
}

/// `--spec <file>`, the only argument of either init stage.
#[cfg(target_os = "linux")]
fn spec(arguments: &[std::ffi::OsString]) -> Result<&std::path::Path, String> {
    match arguments {
        [flag, path] if flag == "--spec" => Ok(std::path::Path::new(path)),
        _ => Err(USAGE.to_owned()),
    }
}

#[cfg(target_os = "linux")]
const USAGE: &str = "usage: layerfs-sandbox-helper init|init-inner --spec <file> | exec --init \
                     <pid> --cgroup <dir> --uid <uid> --gid <gid> --cwd <dir> -- <command> …";
