//! The real launcher adapter (#555): one command in a detached process
//! group, with its pid, output and exit receipt written under
//! `step/launch/<exec>/` outside the launcher. A launcher that did not
//! start the command reads the same files, so a restart resumes the wait.
//!
//! Process detail stays here: the runtime sees only [`Launcher`], and this
//! adapter changes no daemon protocol and adds no core queue or actor.
use crate::common::home::{self, paths};
use crate::common::outcome::{failed, refused};
use crate::step::contract::{ExecId, ExecRequest};
use crate::step::launcher::{Launcher, Observation};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use nix::errno::Errno;
use nix::sys::signal::{kill, killpg, Signal};
use nix::unistd::Pid;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The wrapper written beside one exec: run the command, then write its
/// exit code, so the outcome survives a launcher restart.
const WRAPPER: &str = r#"#!/bin/bash
set -u
trap 'printf "%d" 143 > "$2"; exit 143' TERM
bash -c "$1"
code=$?
printf '%d' "$code" > "$2"
exit "$code"
"#;

/// A launcher that runs each command as its own detached process.
///
/// `start` lays out `root/<exec>/` (stdout, stderr, exit, pid, run), starts
/// `bash run <command> <exit>` in a new process group and answers a handle
/// naming those files. `observe` reads the exit receipt first and the
/// process's liveness second, so any launcher instance over the same `root`
/// observes the command. `cancel` sends its process group `SIGTERM` once; the
/// exit receipt is written by the wrapper.
#[derive(Clone, Debug)]
pub struct ProcessLauncher {
    root: PathBuf,
}

impl ProcessLauncher {
    /// A launcher whose receipts live under `root`.
    pub fn open(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The launcher of the home's own `step/launch/` folder.
    pub fn in_home(home: &Path) -> Self {
        Self::open(paths::step_launch_dir(home))
    }

    /// The receipt folder of one exec.
    fn directory(&self, id: &ExecId) -> PathBuf {
        self.root.join(id.as_str())
    }
}

impl Launcher for ProcessLauncher {
    fn start(&self, id: &ExecId, request: &ExecRequest) -> Result<SdkValue, SdkRefusal> {
        let directory = self.directory(id);
        home::private_dir(&directory)?;
        let stdout = directory.join("stdout");
        let stderr = directory.join("stderr");
        let exit = directory.join("exit");
        let script = directory.join("run");
        write_receipt(&script, WRAPPER.as_bytes())?;
        let out = create(&stdout)?;
        let err = create(&stderr)?;
        let mut command = Command::new("bash");
        command
            .arg(&script)
            .arg(&request.command)
            .arg(&exit)
            .stdin(Stdio::null())
            .stdout(Stdio::from(out))
            .stderr(Stdio::from(err))
            .process_group(0);
        let child = command.spawn().map_err(failed)?;
        let pid = child.id();
        // The outcome is the wrapper's file; waiting here only reaps the
        // child while this launcher lives, so it never becomes a zombie.
        std::thread::spawn(move || {
            let mut child = child;
            let _ = child.wait();
        });
        if let Err(error) = write_receipt(&directory.join("pid"), pid.to_string().as_bytes()) {
            if let Ok(raw) = i32::try_from(pid) {
                let _ = killpg(Pid::from_raw(raw), Signal::SIGKILL);
            }
            return Err(error);
        }
        let path = |path: &Path| SdkValue::text(path.to_string_lossy().into_owned());
        Ok(SdkValue::map([
            ("kind", SdkValue::text("process")),
            ("pid", SdkValue::Int(i64::from(pid))),
            ("stdout", path(&stdout)),
            ("stderr", path(&stderr)),
            ("exit", path(&exit)),
        ]))
    }

    fn observe(&self, handle: &SdkValue) -> Observation {
        let Some(exit) = handle.get_text("exit") else {
            return Observation::Gone;
        };
        if let Some(code) = exit_code(Path::new(exit)) {
            return Observation::Ended(code);
        }
        match handle
            .get_int("pid")
            .and_then(|pid| i32::try_from(pid).ok())
        {
            Some(pid) if alive(pid) => Observation::Running,
            _ => Observation::Gone,
        }
    }

    fn cancel(&self, handle: &SdkValue) -> Result<(), SdkRefusal> {
        let Some(pid) = handle
            .get_int("pid")
            .and_then(|pid| i32::try_from(pid).ok())
        else {
            return Ok(());
        };
        match killpg(Pid::from_raw(pid), Signal::SIGTERM) {
            Ok(()) | Err(Errno::ESRCH) => Ok(()),
            Err(error) => Err(failed(error)),
        }
    }
}

/// A private output file of one exec.
fn create(path: &Path) -> Result<fs::File, SdkRefusal> {
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(failed)
}

/// Writes `bytes` to `path` as a private temporary file, then one rename.
fn write_receipt(path: &Path, bytes: &[u8]) -> Result<(), SdkRefusal> {
    let parent = path
        .parent()
        .ok_or_else(|| refused("Failed", "a receipt needs a folder"))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("receipt");
    let temporary = parent.join(format!(".{name}.{}.tmp", std::process::id()));
    let mut file = create(&temporary)?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    match written.and_then(|()| fs::rename(&temporary, path)) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(failed(error))
        }
    }
}

/// The exit code the wrapper recorded, or `None` while it is not there.
fn exit_code(path: &Path) -> Option<i32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// Whether `pid` names a live process. `EPERM` means it exists, owned by
/// another identity; any other error means it does not.
fn alive(pid: i32) -> bool {
    match kill(Pid::from_raw(pid), None::<Signal>) {
        Ok(()) | Err(Errno::EPERM) => true,
        Err(_) => false,
    }
}
