//! The exec stage: one command inside a running Sandbox. The helper opens
//! the group `--cgroup` names, the leaf of the Sandbox's cgroup, and the
//! command's, below, then forks itself into the first (`clone3` with
//! `CLONE_INTO_CGROUP`, `local::OneThread`): the child does the work, inside the group, so the
//! Sandbox's `cgroup.kill` ends it and its stop finds the group empty only
//! once it is gone; the parent, outside it, only waits and ends as the child
//! did. No process ever moves between groups: a move (a pid written to
//! `cgroup.procs`) waits for an RCU grace period, 5–28 ms after an idle
//! spell, which every command used to pay. The child opens the three
//! namespace handles, then enters them (after entering the Sandbox's mount
//! namespace the host's cgroup and `/proc` paths no longer resolve). Its own
//! child, the command, is created inside the Sandbox's PID namespace, runs
//! as the requested uid:gid and inherits no-new-privs, so file capabilities
//! and setuid bits cannot raise it again. Just before exec it installs the
//! filter that refuses it a user namespace (`local::deny_user_namespaces`,
//! D-8); a command whose filter cannot be installed never starts.
//!
//! With `--command-cgroup <group>` (a command with its own group,
//! layerfs-project#113) the command is created inside that group, opened
//! before entering the namespaces; with none it is created in the
//! helper's. The helper itself is never in the command's group, so a
//! cancel's signals and `cgroup.kill` reach the command and what it
//! started, never the helper, which then reports how the command ended; a
//! command that cannot be created there never runs. Once the command runs
//! (its exec succeeded), the helper writes one byte to its standard input,
//! which the backend gives it as one end of a socket: only then is the
//! command's stop handle recorded, so a cancel never finds the group empty
//! while the command is still to start. The command gets no standard
//! input unless `--stdin inherit` gives it the helper's own: the backend
//! then writes the caller's bytes on its end of that socket once the
//! command runs, and shuts its writing down for the end of the input.
//!
//! With `--trace <file>` (a traced call, `LAYERFS_TRACE_STEP`) it writes
//! when each stage ended to that file, which the worker made, once the
//! command ended ([`Stamps`]). `helper.cgroup_join` is the fork into the
//! Sandbox's group.
//!
//! With `--runtime <dir>` (a Sandbox with the bundle's runtime layer, R5,
//! bound at `<dir>` inside it) the command's `PATH` gets `<dir>/bin` at its
//! end (`sandbox_runtime::search_path`). The program is looked up on that
//! `PATH` inside the Sandbox, so an image's own `bash` wins, and an image with
//! none (alpine, distroless) runs the runtime's static `bash` instead of
//! failing.
use layerfs_sandbox_backend::local::OneThread;
use nix::{
    sched::{setns, CloneFlags},
    sys::{
        prctl::set_no_new_privs,
        signal::{kill, Signal},
        wait::{waitpid, WaitStatus},
    },
    unistd::Pid,
};
use std::{
    ffi::OsString,
    fs::{self, File},
    io::{self, Read, Write},
    os::fd::AsFd,
    os::unix::fs::OpenOptionsExt,
    os::unix::process::CommandExt,
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
    runtime: Option<PathBuf>,
    stdin: bool,
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
    // opened before the Sandbox's mount namespace hides these host paths
    let mut stamps = Stamps::open(run.trace.as_deref());
    stamps.mark("helper.start");
    let process = OneThread::open().map_err(|error| format!("this helper's threads: {error}"))?;
    let sandbox = group(&run.cgroup)?;
    let command_group = match &run.command_cgroup {
        Some(path) => Some(group(path)?),
        None => None,
    };
    let fork = process.fork_into(Some(sandbox.as_fd()));
    match fork.map_err(|error| format!("starting in the Sandbox's cgroup: {error}"))? {
        Some(child) => return Ok(mirror(pid(child))),
        None => drop(sandbox),
    }
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
        .stdin(match run.stdin {
            true => Stdio::inherit(),
            false => Stdio::null(),
        })
        .process_group(0);
    if let Some(runtime) = &run.runtime {
        // set on the command, so its lookup of a bare program name (`bash`)
        // searches this PATH, in the Sandbox's root
        let path = (std::env::var_os("PATH").filter(|path| !path.is_empty()))
            .unwrap_or_else(|| CONTAINER_PATH.into());
        let path = layerfs_bridge::sandbox_runtime::search_path(&path, runtime);
        command.env("PATH", path);
    }
    layerfs_sandbox_backend::local::deny_user_namespaces(&mut command);
    let child = match start(&process, &mut command, command_group.as_ref()) {
        Ok(child) => child,
        // the command never started: this stage's own failure, which must
        // never look like the command's status
        Err(error) => fail(&format!("starting {}: {error}", program.to_string_lossy())),
    };
    stamps.mark("helper.bash_spawn");
    // the command is in its group: the backend may record its stop handle
    if command_group.is_some() {
        let ready = std::io::stdin().as_fd().try_clone_to_owned();
        if let Err(error) = ready.and_then(|fd| File::from(fd).write_all(b"r")) {
            let _ = kill(child, Signal::SIGKILL);
            let _ = wait(child);
            fail(&format!("telling the command started: {error}"));
        }
        stamps.mark("helper.ready");
    }
    let code = wait(child).map_err(|error| format!("waiting for the command: {error}"))?;
    stamps.mark("helper.bash");
    stamps.write();
    Ok(code)
}

/// Starts `command` in a child created inside `group` (this process's own
/// group with none), and returns once it runs: its exec succeeded, which
/// closes the close-on-exec pipe its child would otherwise report why it
/// could not on.
fn start(process: &OneThread, command: &mut Command, group: Option<&File>) -> io::Result<Pid> {
    let (mut said, told) = nix::unistd::pipe2(nix::fcntl::OFlag::O_CLOEXEC)
        .map(|(read, write)| (File::from(read), File::from(write)))?;
    let Some(child) = process.fork_into(group.map(|group| group.as_fd()))? else {
        drop(said);
        let error = command.exec();
        let code = error.raw_os_error().unwrap_or(nix::libc::EIO);
        let _ = (&told).write_all(&code.to_ne_bytes());
        std::process::exit(super::EXIT_FAILURE)
    };
    drop(told);
    let child = pid(child);
    let mut reason = Vec::new();
    said.read_to_end(&mut reason)?;
    let Ok(code) = <[u8; 4]>::try_from(reason.get(..4).unwrap_or_default()) else {
        return Ok(child);
    };
    let _ = wait(child);
    Err(io::Error::from_raw_os_error(i32::from_ne_bytes(code)))
}

/// The exit code of child `child`, 128 + signal for one a signal ended.
fn wait(child: Pid) -> io::Result<i32> {
    loop {
        match waitpid(child, None) {
            Ok(WaitStatus::Exited(_, code)) => return Ok(code),
            Ok(WaitStatus::Signaled(_, signal, _)) => return Ok(128 + signal as i32),
            Ok(_) | Err(nix::errno::Errno::EINTR) => continue,
            Err(errno) => return Err(errno.into()),
        }
    }
}

/// The fork's parent ends as its child, which does the work, ended: with
/// its exit code, or with a signal when a signal ended it (the Sandbox
/// stopped under it, or it failed), so the backend reads the same either way
/// as from a helper that did the work itself.
fn mirror(child: Pid) -> i32 {
    loop {
        match waitpid(child, None) {
            Ok(WaitStatus::Exited(_, code)) => return code,
            Ok(WaitStatus::Signaled(..)) => end(),
            Ok(_) | Err(nix::errno::Errno::EINTR) => continue,
            Err(_) => end(),
        }
    }
}

fn pid(child: u32) -> Pid {
    Pid::from_raw(child as i32)
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

/// A container's search path, for a command started with none.
const CONTAINER_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

/// Group `cgroup`'s directory, opened to create a process inside it.
fn group(cgroup: &Path) -> Result<File, String> {
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_DIRECTORY)
        .open(cgroup)
        .map_err(|error| format!("opening {}: {error}", cgroup.display()))
}

/// Ends this stage the way only a failure can: with a signal, never an exit
/// code, so the caller cannot read it as the command's status.
fn fail(message: &str) -> ! {
    eprintln!("layerfs-sandbox-helper: {message}");
    end()
}

/// `SIGKILL` to this process by its pid, not `raise`: a forked child's C
/// library may still hold its parent's thread id.
fn end() -> ! {
    let _ = kill(nix::unistd::getpid(), Signal::SIGKILL);
    std::process::abort()
}

fn parse(arguments: &[OsString]) -> Result<Run, String> {
    let (mut init, mut uid, mut gid) = (0, 0, 0);
    let (mut cgroup, mut directory) = (PathBuf::new(), PathBuf::new());
    let (mut command_cgroup, mut trace, mut runtime) = (None, None, None);
    let mut stdin = false;
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
            Some("--runtime") => runtime = Some(PathBuf::from(value)),
            Some("--cwd") => directory = PathBuf::from(value),
            Some("--stdin") if value == "inherit" => stdin = true,
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
        || runtime
            .as_deref()
            .is_some_and(|runtime| !runtime.is_absolute())
        || command.is_empty()
    {
        return Err("missing or invalid exec argument".to_owned());
    }
    Ok(Run {
        init,
        cgroup,
        command_cgroup,
        trace,
        runtime,
        stdin,
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
