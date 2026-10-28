//! The hidden `ns-exec` entry: join the daemon's mount namespace, then run one
//! command in it (prototype for #3/#499).
//!
//! `workspace.exec` is an Engine exec in the container's initial mount
//! namespace. When the daemon serves from a private namespace, the Workspace
//! mounts are in that namespace only, so the Engine's entry command is this
//! helper: it reads the daemon's pid from the pid file the daemon wrote, joins
//! `/proc/<pid>/ns/mnt`, drops the root identity the Engine starts it with back
//! to the ordinary command identity, and execs the command it was given. The
//! Engine still sees one process and the same streams and exit code. Nothing
//! is run when any step fails: a helper that cannot join must not silently run
//! the command in the wrong namespace.
use nix::{
    sched::{setns, CloneFlags},
    unistd::{setgid, setgroups, setuid, Gid, Uid},
};
use std::{
    ffi::{OsStr, OsString},
    fs,
    os::unix::process::CommandExt,
    process::Command,
};

/// Exit code of a helper that refused before running the command.
const REFUSED: u8 = 125;
const USAGE: &str = "usage: layerfs-daemon ns-exec --pid-file <path> [--directory <path>] [--uid <uid> --gid <gid>] -- <command> [args...]";

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
    let (pid_file, directory, identity, command) = parse(args)?;
    let pid = read_pid(&pid_file)?;
    join_mount_namespace(pid)?;
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

/// `--pid-file <path> [--directory <path>] [--uid <uid> --gid <gid>] -- <command> [args...]`.
fn parse(
    args: &[OsString],
) -> Result<(String, Option<String>, Option<(u32, u32)>, Vec<OsString>), String> {
    let mut rest = args.iter();
    let pid_file = match rest.next() {
        Some(flag) if flag == OsStr::new("--pid-file") => path(rest.next())?,
        _ => return Err(USAGE.to_owned()),
    };
    let mut directory = None;
    let mut uid = None;
    let mut gid = None;
    loop {
        match rest.next() {
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
    if command.is_empty() {
        return Err(USAGE.to_owned());
    }
    match (uid, gid) {
        (Some(0), _) | (_, Some(0)) => {
            Err("--uid and --gid must name the non-root command identity".to_owned())
        }
        (Some(uid), Some(gid)) => Ok((pid_file, directory, Some((uid, gid)), command)),
        (None, None) => Ok((pid_file, directory, None, command)),
        _ => Err("--uid and --gid must be given together".to_owned()),
    }
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

/// The daemon's pid from its pid file, or a clear refusal.
fn read_pid(pid_file: &str) -> Result<u32, String> {
    let pid = fs::read_to_string(pid_file)
        .map_err(|error| format!("read daemon pid file {pid_file}: {error}"))?;
    match pid.trim().parse::<u32>() {
        Ok(pid) if pid != 0 => Ok(pid),
        _ => Err(format!("daemon pid file {pid_file} does not name a pid")),
    }
}

fn join_mount_namespace(pid: u32) -> Result<(), String> {
    let namespace = format!("/proc/{pid}/ns/mnt");
    let file = fs::File::open(&namespace)
        .map_err(|error| format!("open mount namespace {namespace}: {error}"))?;
    setns(&file, CloneFlags::CLONE_NEWNS)
        .map_err(|error| format!("join mount namespace {namespace}: {error}"))
}

/// The Engine starts this helper as root, because only root holds the
/// capability the `setns` needs; the command must still run as the Sandbox's
/// ordinary command identity, with no supplementary groups and no capability.
fn drop_to(uid: u32, gid: u32) -> Result<(), String> {
    setgroups(&[]).map_err(|error| format!("drop supplementary groups: {error}"))?;
    setgid(Gid::from_raw(gid)).map_err(|error| format!("drop to gid {gid}: {error}"))?;
    setuid(Uid::from_raw(uid)).map_err(|error| format!("drop to uid {uid}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_identity_cannot_be_root() {
        let args: Vec<OsString> = ["--pid-file", "/p", "--uid", "0", "--gid", "0", "--", "true"]
            .into_iter()
            .map(OsString::from)
            .collect();
        assert!(parse(&args).is_err());
    }

    #[test]
    fn a_command_needs_a_separator_and_a_program() {
        assert!(parse(&[OsString::from("--pid-file"), "/p".into()]).is_err());
        assert!(parse(&[OsString::from("--pid-file"), "/p".into(), "--".into()]).is_err());
    }

    #[test]
    fn the_identity_is_all_or_nothing() {
        let args = [
            "--pid-file".into(),
            "/p".into(),
            "--uid".into(),
            "501".into(),
            "--".into(),
            "true".into(),
        ];
        assert!(parse(&args).is_err());
    }

    #[test]
    fn a_parsed_command_keeps_its_arguments() {
        let args: Vec<OsString> = ["--pid-file", "/p", "--uid", "501", "--gid", "20", "--"]
            .into_iter()
            .map(OsString::from)
            .chain(["bash".into(), "-c".into(), "printf hello".into()])
            .collect();
        let (pid_file, directory, identity, command) = parse(&args).unwrap();
        assert_eq!(pid_file, "/p");
        assert_eq!(directory, None);
        assert_eq!(identity, Some((501, 20)));
        assert_eq!(command, ["bash", "-c", "printf hello"]);
    }

    #[test]
    fn a_missing_or_unreadable_pid_file_is_a_refusal() {
        let directory =
            std::env::temp_dir().join(format!("layerfs-ns-exec-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        let missing = directory.join("missing.pid");
        assert!(read_pid(missing.to_str().unwrap()).is_err());
        fs::write(&missing, b"not a pid\n").unwrap();
        assert!(read_pid(missing.to_str().unwrap()).is_err());
        fs::write(&missing, b"0\n").unwrap();
        assert!(read_pid(missing.to_str().unwrap()).is_err());
        fs::write(&missing, b"4321\n").unwrap();
        assert_eq!(read_pid(missing.to_str().unwrap()).unwrap(), 4321);
        fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn a_missing_namespace_is_a_refusal() {
        assert!(join_mount_namespace(u32::MAX - 1).is_err());
    }
}
