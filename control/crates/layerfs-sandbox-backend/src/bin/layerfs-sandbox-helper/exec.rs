//! The exec stage: one command inside a running Sandbox. The helper joins the
//! Sandbox's cgroup first (after entering the Sandbox's mount namespace this
//! host path no longer resolves), opens the three namespace handles, then
//! enters them. Its child is created inside the Sandbox's PID namespace, runs
//! as the requested uid:gid and inherits no-new-privs, so file capabilities
//! and setuid bits cannot raise it again. Just before exec it installs the
//! filter that refuses it a user namespace (`local::deny_user_namespaces`,
//! D-8); a command whose filter cannot be installed never starts.
//!
//! With `--command-cgroup <group>` (a command with its own group,
//! layerfs-project#113) the helper opens that group's `cgroup.procs` before
//! entering the namespaces, and the command's child process moves itself
//! into it just before exec (`local::start_in_cgroup`). The helper itself
//! is never in the command's group, so a cancel's signals and `cgroup.kill`
//! reach the command and what it started, never the helper, which then
//! reports how the command ended; a child that cannot join never runs.
//! Once the command runs (its exec succeeded, so it is in its group), the
//! helper writes one byte to its standard input, which the backend gives
//! it as one end of a socket: only then is the command's stop handle
//! recorded, so a cancel never finds the group empty while the command is
//! still to start. The command itself gets no standard input.
//!
//! With `--trace <file>` (a traced call, `LAYERFS_TRACE_STEP`) it writes
//! when each stage ended to that file, which the worker made, once the
//! command ended ([`Stamps`]).
use nix::{
    sched::{setns, CloneFlags},
    sys::prctl::set_no_new_privs,
};
use std::{
    ffi::OsString,
    fs::{self, File},
    io::Write,
    os::fd::AsFd,
    os::unix::process::{CommandExt, ExitStatusExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

/// One parsed exec stage.
struct Run {
    init: u32,
    cgroup: PathBuf,
    command_cgroup: Option<PathBuf>,
    trace: Option<PathBuf>,
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
    // opened before the Sandbox's mount namespace hides this host path
    let mut stamps = Stamps::open(run.trace.as_deref());
    stamps.mark("helper.start");
    // opened before the Sandbox's mount namespace hides this host path
    let command_procs = match &run.command_cgroup {
        Some(group) => Some(procs(group)?),
        None => None,
    };
    join(&run.cgroup)?;
    stamps.mark("helper.cgroup_join");
    let namespaces = Namespaces::open(run.init)?;
    namespaces.enter()?;
    set_no_new_privs().map_err(|error| format!("setting no-new-privs: {error}"))?;
    stamps.mark("helper.setns");
    let (program, rest) = run.arguments.split_first().ok_or("no command")?;
    let mut command = Command::new(program);
    command
        .args(rest)
        .uid(run.uid)
        .gid(run.gid)
        .current_dir(&run.directory)
        .stdin(Stdio::null())
        .process_group(0);
    if let Some(procs) = command_procs {
        layerfs_sandbox_backend::local::start_in_cgroup(&mut command, procs);
    }
    layerfs_sandbox_backend::local::deny_user_namespaces(&mut command);
    let mut child = match command.spawn() {
        Ok(child) => child,
        // the command never started: this stage's own failure, which must
        // never look like the command's status
        Err(error) => fail(&format!("starting {}: {error}", program.to_string_lossy())),
    };
    stamps.mark("helper.bash_spawn");
    // the command is in its group: the backend may record its stop handle
    if run.command_cgroup.is_some() {
        let ready = std::io::stdin().as_fd().try_clone_to_owned();
        if let Err(error) = ready.and_then(|fd| File::from(fd).write_all(b"r")) {
            let _ = child.kill();
            let _ = child.wait();
            fail(&format!("telling the command started: {error}"));
        }
        stamps.mark("helper.ready");
    }
    let status = child
        .wait()
        .map_err(|error| format!("waiting for the command: {error}"))?;
    stamps.mark("helper.bash");
    stamps.write();
    Ok(status
        .code()
        .or_else(|| status.signal().map(|signal| 128 + signal))
        .unwrap_or(super::EXIT_FAILURE))
}

/// The stage times of a traced command: `<step> <microseconds since the
/// epoch>` lines, kept here and written to the worker's file once the
/// command ended, so tracing adds no write before it. Untraced, nothing.
struct Stamps(Option<(File, String)>);

impl Stamps {
    fn open(file: Option<&Path>) -> Self {
        let file = file.and_then(|file| fs::OpenOptions::new().write(true).open(file).ok());
        Self(file.map(|file| (file, String::new())))
    }

    fn mark(&mut self, step: &str) {
        if let Some((_, text)) = &mut self.0 {
            let at = SystemTime::now().duration_since(UNIX_EPOCH);
            let at = at.map_or(0, |at| at.as_micros());
            text.push_str(&format!("{step} {at}\n"));
        }
    }

    fn write(self) {
        if let Some((mut file, text)) = self.0 {
            let _ = file.write_all(text.as_bytes());
        }
    }
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

/// Group `cgroup`'s process list, opened to move a process into it.
fn procs(cgroup: &Path) -> Result<File, String> {
    fs::OpenOptions::new()
        .write(true)
        .open(cgroup.join("cgroup.procs"))
        .map_err(|error| format!("opening {}: {error}", cgroup.display()))
}

/// The Sandbox's cgroup, joined by this helper (and so by a command with no
/// group of its own), so the Sandbox's `cgroup.kill` ends them too.
fn join(cgroup: &Path) -> Result<(), String> {
    let pid = nix::unistd::getpid().as_raw();
    let mut file =
        procs(cgroup).map_err(|error| format!("joining the Sandbox's cgroup: {error}"))?;
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
    let (mut command_cgroup, mut trace) = (None, None);
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
            Some("--command-cgroup") => command_cgroup = Some(PathBuf::from(value)),
            Some("--trace") => trace = Some(PathBuf::from(value)),
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
        command_cgroup,
        trace,
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
