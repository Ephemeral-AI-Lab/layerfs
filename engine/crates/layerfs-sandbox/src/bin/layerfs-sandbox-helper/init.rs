//! The two init stages. The outer one creates the namespaces and joins the
//! cgroup; its first child, the inner one, is PID 1 of the new PID namespace,
//! so it builds the private root, starts the daemon and reaps. Neither stage
//! retries anything: a failure ends the stage and is written to the Sandbox's
//! daemon log, which the start's error keeps.
use super::mounts;
use layerfs_sandbox::backend::HelperSpec;
use nix::{
    sched::{unshare, CloneFlags},
    sys::wait::{waitpid, WaitStatus},
    unistd::Pid,
};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

/// The outer init stage: joins the Sandbox's cgroup, becomes the first
/// process of its mount, PID and IPC namespaces, and starts the inner stage.
/// Its exit code is the daemon's, so the Sandbox ends when the daemon does.
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
    let mut inner = Command::new(executable)
        .arg("init-inner")
        .arg("--spec")
        .arg(spec_path)
        .spawn()
        .map_err(|error| format!("starting the Sandbox's init stage: {error}"))?;
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
    let daemon = Command::new(&spec.daemon)
        .arg("--config")
        .arg(&spec.config)
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

/// The Sandbox's own cgroup, written by the Sandbox's own init stage: every
/// process it creates afterwards inherits it, so one write ends them all.
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
