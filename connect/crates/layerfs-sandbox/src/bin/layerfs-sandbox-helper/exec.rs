//! The exec stage: one command inside a running Sandbox. The helper joins the
//! Sandbox's cgroup first (after entering the Sandbox's mount namespace this
//! host path no longer resolves), opens the three namespace handles, then
//! enters them. Its child is created inside the Sandbox's PID namespace, runs
//! as the requested uid:gid and inherits no-new-privs, so file capabilities
//! and setuid bits cannot raise it again.
use nix::{
    sched::{setns, CloneFlags},
    sys::prctl::set_no_new_privs,
};
use std::{
    ffi::OsString,
    fs::{self, File},
    io::Write,
    os::unix::process::{CommandExt, ExitStatusExt},
    path::{Path, PathBuf},
    process::Command,
};

/// One parsed exec stage.
struct Run {
    init: u32,
    cgroup: PathBuf,
    uid: u32,
    gid: u32,
    directory: PathBuf,
    arguments: Vec<OsString>,
}

/// Runs one command and exits with its code (128 + signal for one a signal
/// ended). A setup failure ends this process with a signal, so the caller
/// never reads it as a command's exit code.
pub(super) fn run(arguments: &[OsString]) -> Result<i32, String> {
    let run = parse(arguments)?;
    join(&run.cgroup)?;
    let namespaces = Namespaces::open(run.init)?;
    namespaces.enter()?;
    set_no_new_privs().map_err(|error| format!("setting no-new-privs: {error}"))?;
    let (program, rest) = run.arguments.split_first().ok_or("no command")?;
    let mut command = Command::new(program);
    command
        .args(rest)
        .uid(run.uid)
        .gid(run.gid)
        .current_dir(&run.directory)
        .process_group(0);
    let mut child = match command.spawn() {
        Ok(child) => child,
        // the command never started: this stage's own failure, which must
        // never look like the command's status
        Err(error) => fail(&format!("starting {}: {error}", program.to_string_lossy())),
    };
    let status = child
        .wait()
        .map_err(|error| format!("waiting for the command: {error}"))?;
    Ok(status
        .code()
        .or_else(|| status.signal().map(|signal| 128 + signal))
        .unwrap_or(super::EXIT_FAILURE))
}

/// The three namespace handles of a Sandbox's init stage, all opened before
/// any is entered: after entering the Sandbox's mount namespace this host's
/// `/proc/<pid>/ns` no longer resolves.
struct Namespaces {
    mount: File,
    pid: File,
    ipc: File,
}

impl Namespaces {
    fn open(init: u32) -> Result<Self, String> {
        let directory = PathBuf::from(format!("/proc/{init}/ns"));
        let open = |name: &str| {
            File::open(directory.join(name))
                .map_err(|error| format!("{}: {error}", directory.join(name).display()))
        };
        Ok(Self {
            mount: open("mnt")?,
            pid: open("pid_for_children").or_else(|_| open("pid"))?,
            ipc: open("ipc")?,
        })
    }

    fn enter(self) -> Result<(), String> {
        let Self { mount, pid, ipc } = self;
        setns(mount, CloneFlags::CLONE_NEWNS)
            .map_err(|error| format!("entering the Sandbox's mount namespace: {error}"))?;
        setns(pid, CloneFlags::CLONE_NEWPID)
            .map_err(|error| format!("entering the Sandbox's PID namespace: {error}"))?;
        setns(ipc, CloneFlags::CLONE_NEWIPC)
            .map_err(|error| format!("entering the Sandbox's IPC namespace: {error}"))
    }
}

/// The Sandbox's own cgroup, so `cgroup.kill` ends this command too.
fn join(cgroup: &Path) -> Result<(), String> {
    let pid = nix::unistd::getpid().as_raw();
    let mut file = fs::OpenOptions::new()
        .write(true)
        .open(cgroup.join("cgroup.procs"))
        .map_err(|error| format!("joining the Sandbox's cgroup: {error}"))?;
    file.write_all(pid.to_string().as_bytes())
        .map_err(|error| format!("joining the Sandbox's cgroup: {error}"))
}

/// Ends this stage the way only a failure can: with a signal, never an exit
/// code, so the caller cannot read it as the command's status.
fn fail(message: &str) -> ! {
    eprintln!("layerfs-sandbox-helper: {message}");
    let _ = nix::sys::signal::raise(nix::sys::signal::Signal::SIGKILL);
    std::process::abort()
}

fn parse(arguments: &[OsString]) -> Result<Run, String> {
    let (mut init, mut uid, mut gid) = (0, 0, 0);
    let (mut cgroup, mut directory) = (PathBuf::new(), PathBuf::new());
    let mut command = Vec::new();
    let mut index = 0;
    while index < arguments.len() {
        let flag = arguments[index].to_str();
        if flag == Some("--") {
            command.extend(arguments[index + 1..].iter().cloned());
            break;
        }
        let value = arguments
            .get(index + 1)
            .ok_or_else(|| format!("{} needs a value", arguments[index].to_string_lossy()))?;
        match flag {
            Some("--init") => init = number(value)?,
            Some("--uid") => uid = number(value)?,
            Some("--gid") => gid = number(value)?,
            Some("--cgroup") => cgroup = PathBuf::from(value),
            Some("--cwd") => directory = PathBuf::from(value),
            _ => {
                return Err(format!(
                    "unknown argument {}",
                    arguments[index].to_string_lossy()
                ))
            }
        }
        index += 2;
    }
    if init == 0
        || uid == 0
        || gid == 0
        || cgroup.as_os_str().is_empty()
        || !directory.is_absolute()
        || command.is_empty()
    {
        return Err("missing or invalid exec argument".to_owned());
    }
    Ok(Run {
        init,
        cgroup,
        uid,
        gid,
        directory,
        arguments: command,
    })
}

fn number(value: &OsString) -> Result<u32, String> {
    value
        .to_str()
        .and_then(|text| text.parse().ok())
        .ok_or_else(|| format!("{} is not a number", value.to_string_lossy()))
}
