//! The namespaces backend's helper: one single-threaded binary with two
//! stages, so the calls that act on the calling process (`unshare`, `setns`,
//! `pivot_root`, `PR_SET_NO_NEW_PRIVS`) run in a process of their own and the
//! product crate stays free of `unsafe`.
//!
//! - `init --spec <file>`: joins the Sandbox's cgroup, creates its mount, PID
//!   and IPC namespaces, wires its own network namespace (which it holds)
//!   and starts the inner stage, which is PID 1 of them, on the machine's
//!   network.
//! - `init-inner --spec <file>`: builds the private root, pivots to it, starts
//!   `layerfs-daemon` as root and reaps, as an init process does. When the
//!   spec names a written resolver file, it first shares its mounts (so the
//!   daemon's later Workspace mounts reach the commands' copy) and holds a
//!   gate: on its standard input, a socket to the outer stage, it says the
//!   root is built and starts the daemon only once the outer stage answers
//!   that the commands' own mount view, with that file, is made.
//! - `exec …`: enters a running Sandbox's namespaces (its network one
//!   included, else the command never runs), becomes the requested
//!   nonroot identity with no-new-privs and runs one command under the
//!   filter that refuses it a user namespace (the library's audited
//!   `local/os/linux/seccomp.rs`).
#![deny(unsafe_code)]

#[cfg(target_os = "linux")]
mod exec;
#[cfg(target_os = "linux")]
mod init;
#[cfg(target_os = "linux")]
mod mounts;
#[cfg(target_os = "linux")]
mod network;

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
                     <pid> --cgroup <dir> [--leave <dir>] [--trace <file>] --uid <uid> --gid <gid> \
                     --cwd <dir> -- <command> …";
