//! `ssh` host_key: see `mod.rs`.
use super::observe::ssh_of;
use crate::error::{Code, MachineError};
use crate::setup::tool;
use crate::spec::{Spec, SshSpec};
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::time::Duration;

/// The accepted host key lines, in the machine's work folder.
pub(super) fn accepted(spec: &Spec) -> PathBuf {
    spec.work_dir.join("known_hosts")
}

/// How known_hosts names the host: `host`, or `[host]:port` off port 22.
fn host_pattern(ssh: &SshSpec) -> String {
    if ssh.port == 22 {
        ssh.host.clone()
    } else {
        format!("[{}]:{}", ssh.host, ssh.port)
    }
}

/// The host's lines of the caller's known_hosts, from `ssh-keygen -F`.
pub(super) fn known_lines(spec: &Spec, fingerprints: bool) -> Result<Vec<String>, MachineError> {
    let ssh = ssh_of(spec)?;
    let keygen = tool::find(&spec.tools, "ssh-keygen")?;
    let flags: &[&str] = if fingerprints { &["-l", "-F"] } else { &["-F"] };
    let mut command = tool::command(&keygen, flags);
    command
        .arg(host_pattern(ssh))
        .arg("-f")
        .arg(&ssh.known_hosts);
    let out = tool::run(command, None, Duration::from_secs(10), "ssh-keygen -F")?;
    Ok(out
        .stdout
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect())
}

/// Copies the host's accepted key lines to the work folder, or refuses a host
/// whose key is not known (`Denied`).
pub(super) fn accept_key(spec: &Spec) -> Result<PathBuf, MachineError> {
    let ssh = ssh_of(spec)?;
    let lines = known_lines(spec, false)?;
    if lines.is_empty() {
        return Err(MachineError::new(
            Code::Denied,
            format!(
                "the host key of {} is not in {}; add it there first",
                host_pattern(ssh),
                ssh.known_hosts.display()
            ),
        ));
    }
    let file = accepted(spec);
    let fail =
        |e: std::io::Error| MachineError::new(Code::Failed, format!("{}: {e}", file.display()));
    std::fs::create_dir_all(&spec.work_dir).map_err(fail)?;
    let mut out = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&file)
        .map_err(fail)?;
    out.write_all(format!("{}\n", lines.join("\n")).as_bytes())
        .map_err(fail)?;
    Ok(file)
}
