//! The two init stages. The outer one creates the namespaces, joins the
//! cgroup and wires the Sandbox's own network, whose namespace it then
//! holds; its first child, the inner one, is PID 1 of the new PID
//! namespace, so it builds the private root, starts the daemon and reaps.
//! The inner stage and the daemon stay on the machine's network: the
//! daemon's loopback listener, the worker's dial and its Store connection
//! are unchanged. Only commands join the Sandbox's network (`exec`). Where
//! the launcher wrote the Sandbox a resolver file (the spec's `resolver`),
//! the daemon still resolves through the machine's own, and the outer stage
//! also holds the commands' own mount view with the written one
//! (`mounts::commands_view`): the inner stage, once its root is built and
//! shared, says so on its standard input, a socket to the outer one, and
//! starts the daemon only once the outer one answers that the view is made.
//! Neither stage retries anything: a failure ends the stage and is written
//! to the Sandbox's daemon log, which the start's error keeps.
use super::{mounts, network};
use layerfs_sandbox_backend::HelperSpec;
use nix::{
    sched::{setns, unshare, CloneFlags},
    sys::wait::{waitpid, WaitStatus},
    unistd::Pid,
};
use std::{
    fs::{self, File},
    io::{Read, Write},
    os::fd::{AsFd, OwnedFd},
    os::unix::net::UnixStream,
    path::Path,
    process::{Command, Stdio},
};

/// What the inner stage says once the root is built, pivoted and shared.
const BUILT: u8 = b'b';
/// What the outer stage answers once the commands' view is made.
const VIEW: u8 = b'v';

/// The outer init stage: joins the Sandbox's cgroup, becomes the first
/// process of its mount, PID and IPC namespaces, wires the Sandbox's network
/// (`network::wire`), starts the inner stage on the machine's network and
/// then holds the Sandbox's. Its exit code is the daemon's, so the Sandbox
/// ends when the daemon does, and its network with it.
pub(super) fn outer(spec_path: &Path) -> Result<i32, String> {
    let spec = read(spec_path)?;
    join(&spec.cgroup)?;
    unshare(CloneFlags::CLONE_NEWNS | CloneFlags::CLONE_NEWPID | CloneFlags::CLONE_NEWIPC)
        .map_err(|error| format!("creating the Sandbox's namespaces: {error}"))?;
    // this mount namespace is the Sandbox's alone: nothing it mounts
    // propagates back to the VM's own mount tree
    nix::mount::mount(
        None::<&Path>,
        Path::new("/"),
        None::<&str>,
        nix::mount::MsFlags::MS_REC | nix::mount::MsFlags::MS_PRIVATE,
        None::<&str>,
    )
    .map_err(|error| format!("making the Sandbox's mount tree private: {error}"))?;
    let executable =
        std::env::current_exe().map_err(|error| format!("this helper's own path: {error}"))?;
    let (machine, sandbox) = network::wire(&spec)?;
    // the inner stage, and the daemon it starts, keep the machine's network
    setns(&machine, CloneFlags::CLONE_NEWNET)
        .map_err(|error| format!("returning to the machine's network: {error}"))?;
    let mut command = Command::new(executable);
    command.arg("init-inner").arg("--spec").arg(spec_path);
    // with a written resolver file, the two stages' gate: one socket pair,
    // the inner stage's end as its standard input
    let gate = match &spec.resolver {
        Some(_) => {
            let (ours, theirs) =
                UnixStream::pair().map_err(|error| format!("the init stages' gate: {error}"))?;
            command.stdin(Stdio::from(OwnedFd::from(theirs)));
            Some(ours)
        }
        None => None,
    };
    let spawned = command.spawn();
    // the command holds the inner stage's end of the gate: closed here, so
    // the inner stage's exit reads as an end
    drop(command);
    let mut inner =
        spawned.map_err(|error| format!("starting the Sandbox's init stage: {error}"))?;
    // this process holds the Sandbox's network: every command joins it
    // through `/proc/<this pid>/ns/net`
    if let Err(error) = setns(&sandbox, CloneFlags::CLONE_NEWNET) {
        let _ = inner.kill();
        let _ = inner.wait();
        return Err(format!("holding the Sandbox's network: {error}"));
    }
    drop((machine, sandbox));
    if let (Some(ours), Some(written)) = (gate, &spec.resolver) {
        // and its commands' mount view: every command joins it through
        // `/proc/<this pid>/ns/mnt`; a failure ends the Sandbox before its
        // daemon starts, so no command ever runs without it
        if let Err(error) = view(ours, written) {
            let _ = inner.kill();
            let _ = inner.wait();
            return Err(error);
        }
    }
    let status = inner
        .wait()
        .map_err(|error| format!("waiting for the Sandbox's init stage: {error}"))?;
    Ok(code(status))
}

/// The inner init stage, PID 1 of the Sandbox's PID namespace: builds and
/// pivots to the private root, starts the daemon as root and reaps, so no
/// process of the Sandbox is left a zombie.
pub(super) fn inner(spec_path: &Path) -> Result<i32, String> {
    let spec = read(spec_path)?;
    mounts::build(&spec)?;
    mounts::pivot(&spec.root)?;
    if spec.resolver.is_some() {
        mounts::share()?;
        built()?;
    }
    let daemon = daemon(&spec)?
        .current_dir("/")
        .stdin(Stdio::null())
        .spawn()
        .map_err(|error| format!("starting the daemon: {error}"))?;
    let daemon_pid = Pid::from_raw(
        i32::try_from(daemon.id()).map_err(|_| "daemon process id width".to_owned())?,
    );
    loop {
        match waitpid(None, None) {
            Ok(WaitStatus::Exited(pid, code)) if pid == daemon_pid => return Ok(code),
            Ok(WaitStatus::Signaled(pid, signal, _)) if pid == daemon_pid => {
                return Ok(128 + signal as i32)
            }
            // an orphan of a command: reaped here, as PID 1 does
            Ok(_) => continue,
            Err(nix::errno::Errno::ECHILD) => {
                return Err("the daemon ended without a waitable status".to_owned())
            }
            Err(error) => return Err(format!("reaping the Sandbox's processes: {error}")),
        }
    }
}

/// The outer stage's half of the gate: once the inner stage says its root
/// is built (end of file instead: it failed, and its own error is in the
/// log), the commands' view with the `written` resolver file, then the
/// answer that lets the inner stage start the daemon.
fn view(mut gate: UnixStream, written: &Path) -> Result<(), String> {
    let mut said = [0u8; 1];
    match gate.read(&mut said) {
        Ok(1) if said[0] == BUILT => {}
        Ok(_) => return Ok(()),
        Err(error) => return Err(format!("reading the init stages' gate: {error}")),
    }
    mounts::commands_view(written)?;
    gate.write_all(&[VIEW])
        .map_err(|error| format!("answering the init stages' gate: {error}"))
}

/// The inner stage's half of the gate, on its standard input: says the root
/// is built and waits for the commands' view; anything else ends the stage
/// before the daemon starts.
fn built() -> Result<(), String> {
    let fd: OwnedFd = std::io::stdin()
        .as_fd()
        .try_clone_to_owned()
        .map_err(|error| format!("the init stages' gate: {error}"))?;
    let mut gate = File::from(fd);
    gate.write_all(&[BUILT])
        .map_err(|error| format!("telling the outer stage the root is built: {error}"))?;
    let mut said = [0u8; 1];
    match gate.read(&mut said) {
        Ok(1) if said[0] == VIEW => Ok(()),
        Ok(_) => Err("the outer stage made no commands' view".to_owned()),
        Err(error) => Err(format!("waiting for the commands' view: {error}")),
    }
}

/// The daemon's command line (`HelperSpec::daemon_command`: through the
/// runtime layer's loader when the spec names one). Its environment is this
/// stage's, which the launcher cleared (only `PATH` and LayerFS's own
/// settings), with no loader variable, so nothing reaches the loader but the
/// bundle's libraries.
fn daemon(spec: &HelperSpec) -> Result<Command, String> {
    let mut daemon = spec
        .daemon_command()
        .map_err(|error| format!("starting the daemon: {error}"))?;
    for (name, _) in std::env::vars_os() {
        if name.to_str().is_some_and(|name| name.starts_with("LD_")) {
            daemon.env_remove(name);
        }
    }
    Ok(daemon)
}

/// The leaf of the Sandbox's cgroup (the spec's `cgroup`), written by the
/// Sandbox's own init stage: every process it creates afterwards inherits
/// it, so one write to the Sandbox group's `cgroup.kill` ends them all.
fn join(cgroup: &Path) -> Result<(), String> {
    let pid = nix::unistd::getpid().as_raw();
    let mut file = fs::OpenOptions::new()
        .write(true)
        .open(cgroup.join("cgroup.procs"))
        .map_err(|error| format!("joining the Sandbox's cgroup: {error}"))?;
    file.write_all(pid.to_string().as_bytes())
        .map_err(|error| format!("joining the Sandbox's cgroup: {error}"))
}

fn read(spec_path: &Path) -> Result<HelperSpec, String> {
    let bytes =
        fs::read(spec_path).map_err(|error| format!("reading {}: {error}", spec_path.display()))?;
    HelperSpec::parse(&bytes).map_err(|error| format!("reading {}: {error}", spec_path.display()))
}

fn code(status: std::process::ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    status
        .code()
        .or_else(|| status.signal().map(|signal| 128 + signal))
        .unwrap_or(super::EXIT_FAILURE)
}
