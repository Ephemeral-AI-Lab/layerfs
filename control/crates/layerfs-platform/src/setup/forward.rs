//! The detached `ssh -N -L 127.0.0.1:<port>:<target>` that carries a Sandbox
//! daemon's port to the host (`lima`; #535 G4). It is started detached so it
//! outlives one CLI call; its pid goes into the [`Forward`].
//! `ExitOnForwardFailure` makes a busy port fail at once.
use crate::error::{Code, MachineError};
use crate::setup::{tool, worker};
use crate::spec::{Forward, Spec};
use std::path::Path;
use std::thread;
use std::time::Duration;

/// How long a new forward must stay up before it counts as open.
const SETTLE: Duration = Duration::from_millis(600);

/// The marker a forward's command line holds, so a reused pid is never ended.
fn marker(port: u16) -> String {
    format!("127.0.0.1:{port}:")
}

/// Opens the forward with `ssh <options...> -N -L 127.0.0.1:port:target dest`.
fn open(
    spec: &Spec,
    options: &[String],
    dest: &str,
    port: u16,
    target: &str,
) -> Result<u32, MachineError> {
    let ssh = tool::find(&spec.tools, "ssh")?;
    let mut args = vec![
        "-N".to_owned(),
        "-o".into(),
        "ExitOnForwardFailure=yes".into(),
    ];
    args.extend(options.iter().cloned());
    args.extend([
        "-L".to_owned(),
        format!("{}{target}", marker(port)),
        dest.to_owned(),
    ]);
    std::fs::create_dir_all(&spec.work_dir).map_err(|e| {
        MachineError::new(Code::Failed, format!("{}: {e}", spec.work_dir.display()))
    })?;
    let log = spec.work_dir.join("forward.log");
    let pid = tool::detach(&spec.tools, &ssh, &args, &log)?;
    thread::sleep(SETTLE);
    if !tool::alive(&spec.tools, pid, &marker(port)) {
        return Err(MachineError::new(
            Code::Failed,
            format!("ssh -L 127.0.0.1:{port} ended at once: {}", tail(&log)),
        ));
    }
    Ok(pid)
}

/// A new forward from a free host loopback port to the daemon listening on
/// `127.0.0.1:<guest_port>` where `dest` is (`ssh <options...> dest`).
pub(crate) fn daemon(
    spec: &Spec,
    options: &[String],
    dest: &str,
    guest_port: u16,
) -> Result<Forward, MachineError> {
    let port = tool::free_port()?;
    let target = format!("127.0.0.1:{guest_port}");
    let pid = open(spec, options, dest, port, &target)?;
    Ok(Forward {
        address: worker::loopback(port),
        pid: Some(pid),
    })
}

/// A daemon's port this host reaches as it is: nothing carries it.
pub(crate) fn shared(guest_port: u16) -> Forward {
    Forward {
        address: worker::loopback(guest_port),
        pid: None,
    }
}

/// Why a machine forwards no Sandbox daemon port.
pub(crate) fn refuse(spec: &Spec, code: Code, why: &str) -> MachineError {
    MachineError::new(
        code,
        format!(
            "the {} machine {} forwards no Sandbox daemon port: {why}",
            spec.kind.name(),
            spec.name
        ),
    )
}

/// Ends the forward `pid` on `port`, if it is still that forward.
pub(crate) fn end(spec: &Spec, pid: u32, port: u16) {
    tool::end(&spec.tools, pid, &marker(port));
}

fn tail(log: &Path) -> String {
    let text = std::fs::read_to_string(log).unwrap_or_default();
    let lines: Vec<&str> = text.trim().lines().collect();
    let said = lines[lines.len().saturating_sub(3)..].join("; ");
    if said.is_empty() {
        "no reason given".into()
    } else {
        said
    }
}
