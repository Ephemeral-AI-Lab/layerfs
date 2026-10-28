//! The hidden `ns-exec` entry: join the daemon's mount namespace, then run one
//! command in it (prototype for #3/#499).
//!
//! `workspace.exec` is an Engine exec in the container's initial mount
//! namespace. When the daemon serves from a private namespace, the Workspace
//! mounts are in that namespace only, so the Engine's entry command is this
//! helper: it receives the daemon's mount-namespace descriptor over the
//! daemon's socket, joins it, drops the root identity the Engine starts it with
//! back to the ordinary command identity, and execs the command it was given.
//! The Engine still sees one process and the same streams and exit code.
//! Nothing is run when any step fails: a helper that cannot join must not
//! silently run the command in the wrong namespace. Every refusal exits 125
//! with a `layerfs-daemon ns-exec:` reason on stderr, which the host sees as
//! the command's exit code and stderr (a mixed old/new pair therefore refuses
//! deliberately instead of running unwrapped).
use nix::{
    sched::{setns, CloneFlags},
    unistd::{setgid, setgroups, setuid, Gid, Uid},
};
use std::{
    ffi::{OsStr, OsString},
    os::unix::process::CommandExt,
    path::Path,
    process::Command,
};

/// Exit code of a helper that refused before running the command.
const REFUSED: u8 = 125;
const USAGE: &str = "usage: layerfs-daemon ns-exec --socket <path> [--directory <path>] [--uid <uid> --gid <gid>] -- <command> [args...]";

pub(super) fn main(args: &[OsString]) -> std::process::ExitCode {
    match run(args) {
        // `run` only returns when it refused; a successful one execs.
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("layerfs-daemon ns-exec: {message}");
            std::process::ExitCode::from(REFUSED)
        }
    }
}

fn run(args: &[OsString]) -> Result<(), String> {
    let Parsed {
        socket,
        directory,
        identity,
        command,
    } = parse(args)?;
    // The daemon is not dumpable, so a Sandbox root (CAP_SYS_ADMIN without
    // CAP_SYS_PTRACE) cannot open its `/proc/<pid>/ns/mnt`; the daemon sends
    // the descriptor over this socket instead.
    let namespace = super::ns_fd::receive(Path::new(&socket))
        .map_err(|error| format!("receive mount namespace from {socket}: {error}"))?;
    setns(&namespace, CloneFlags::CLONE_NEWNS)
        .map_err(|error| format!("join mount namespace from {socket}: {error}"))?;
    // The Engine starts this process in the container's initial namespace,
    // where the mount directory is only the empty directory the daemon
    // mounted over. Enter the joined namespace's view of it before running.
    if let Some(directory) = directory {
        std::env::set_current_dir(&directory)
            .map_err(|error| format!("enter directory {directory}: {error}"))?;
    }
    if let Some((uid, gid)) = identity {
        drop_to(uid, gid)?;
    }
    let (program, rest) = command.split_first().ok_or_else(|| USAGE.to_owned())?;
    let error = Command::new(program).args(rest).exec();
    Err(format!("exec {}: {error}", program.to_string_lossy()))
}

/// One parsed helper invocation: the daemon socket, the optional directory to
/// enter after joining, the ordinary identity to drop to and the command.
struct Parsed {
    socket: String,
    directory: Option<String>,
    identity: Option<(u32, u32)>,
    command: Vec<OsString>,
}

/// `--socket <path> [--directory <path>] [--uid <uid> --gid <gid>] --
/// <command> [args...]`.
fn parse(args: &[OsString]) -> Result<Parsed, String> {
    let mut rest = args.iter();
    let mut socket = None;
    let mut directory = None;
    let mut uid = None;
    let mut gid = None;
    loop {
        match rest.next() {
            Some(flag) if flag == OsStr::new("--socket") && socket.is_none() => {
                socket = Some(path(rest.next())?)
            }
            Some(flag) if flag == OsStr::new("--directory") && directory.is_none() => {
                directory = Some(path(rest.next())?)
            }
            Some(flag) if flag == OsStr::new("--uid") && uid.is_none() => {
                uid = Some(number(rest.next(), "uid")?)
            }
            Some(flag) if flag == OsStr::new("--gid") && gid.is_none() => {
                gid = Some(number(rest.next(), "gid")?)
            }
            Some(flag) if flag == OsStr::new("--") => break,
            _ => return Err(USAGE.to_owned()),
        }
    }
    let command: Vec<OsString> = rest.cloned().collect();
    let Some(socket) = socket else {
        return Err(USAGE.to_owned());
    };
    if command.is_empty() {
        return Err(USAGE.to_owned());
    }
    let identity = match (uid, gid) {
        (Some(0), _) | (_, Some(0)) => {
            return Err("--uid and --gid must name the non-root command identity".to_owned())
        }
        (Some(uid), Some(gid)) => Some((uid, gid)),
        (None, None) => None,
        _ => return Err("--uid and --gid must be given together".to_owned()),
    };
    Ok(Parsed {
        socket,
        directory,
        identity,
        command,
    })
}

fn path(value: Option<&OsString>) -> Result<String, String> {
    value
        .ok_or_else(|| USAGE.to_owned())?
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| "a path is not UTF-8".to_owned())
}

fn number(value: Option<&OsString>, name: &str) -> Result<u32, String> {
    value
        .and_then(|value| value.to_str())
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| format!("--{name} is not a number"))
}

/// The Engine starts this helper as root, because only root holds the
/// capability the `setns` needs; the command must still run as the Sandbox's
/// ordinary command identity, with no supplementary groups and no capability.
fn drop_to(uid: u32, gid: u32) -> Result<(), String> {
    setgroups(&[]).map_err(|error| format!("drop supplementary groups: {error}"))?;
    setgid(Gid::from_raw(gid)).map_err(|error| format!("drop to gid {gid}: {error}"))?;
    setuid(Uid::from_raw(uid)).map_err(|error| format!("drop to uid {uid}: {error}"))
}
