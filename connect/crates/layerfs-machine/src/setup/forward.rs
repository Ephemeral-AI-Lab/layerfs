//! The detached `ssh -N -L 127.0.0.1:<port>:<target>` that carries a worker's
//! port to the host (`ssh`, and `firecracker` through its Lima host). It is
//! started detached so it outlives one CLI call; its pid goes into the
//! endpoint. `ExitOnForwardFailure` makes a busy port fail at once.
use crate::error::{Code, MachineError};
use crate::setup::tool;
use crate::spec::Spec;
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
pub(crate) fn open(
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

/// Ends the forward recorded in the machine's last endpoint, if it still runs.
pub(crate) fn close(spec: &Spec) {
    if let Some(endpoint) = &spec.worker {
        if let Some(pid) = endpoint.forward_pid {
            end(spec, pid, endpoint.address.port());
        }
    }
}

/// Ends the forward `pid` on `port`, if it is still that forward.
pub(crate) fn end(spec: &Spec, pid: u32, port: u16) {
    tool::end(&spec.tools, pid, &marker(port));
}

/// Whether the recorded forward still runs.
pub(crate) fn running(spec: &Spec) -> bool {
    spec.worker.as_ref().is_some_and(|endpoint| {
        endpoint
            .forward_pid
            .is_some_and(|pid| tool::alive(&spec.tools, pid, &marker(endpoint.address.port())))
    })
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
