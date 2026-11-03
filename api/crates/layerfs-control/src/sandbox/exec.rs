//! Commands in a mounted Workspace, run by the Sandbox runtime owner.
//!
//! `exec` runs `bash -c <command>` as the Sandbox's command identity in the
//! Workspace directory and returns the exit code and output when it ends; the
//! daemon is not involved. Background processes recorded by an earlier
//! bundle (the 1.1 `backend.daemon.process.*` calls, deleted in #536 step 2)
//! are still probed before an unmount. A local Sandbox
//! (`layerfs_sandbox::Sandbox::HostProcess`) runs the same commands as
//! processes of this kernel, so the same probes apply.
use crate::common::outcome::{refused, Outcome};
use crate::sandbox::output::Spool;
use crate::sandbox::registry::{lock, sandbox, text, SandboxState};
use layerfs_bridge::control::{SdkRefusal, SdkValue};

/// One command to its end: exit code, stdout, stderr. `spool` sends every
/// non-empty stream to its file instead of keeping it inline. Its home is
/// the Sandbox's scratch directory (`/tmp` in a container), since the
/// command identity has no account there.
fn run(
    state: &SandboxState,
    directory: &str,
    arguments: Vec<String>,
    spool: bool,
) -> Result<(i32, Spool, Spool), SdkRefusal> {
    crate::sandbox::runtime::run(
        &state.runtime,
        state.identity,
        arguments,
        vec![format!(
            "HOME={}",
            crate::sandbox::runtime::scratch(&state.runtime)
        )],
        directory,
        spool,
    )
}

/// `workspace.exec(ws, command, { spool? })`.
///
/// A foreground result is `kind`, `exitCode`, `stdout`, `stderr`, `message`
/// and `truncated` (always false: no output is cut). A stream over the
/// inline window (1 MiB), or any non-empty stream with `spool`, is null
/// here and spooled; the result then also names its `output`, read with
/// `workspace.exec_output` and closed with `sandbox.exec_release`, and the
/// `stdoutBytes` and `stderrBytes` sizes.
pub(crate) fn exec(args: &SdkValue) -> Outcome {
    let state = sandbox(args)?;
    let path = text(args, "path")?;
    let command = text(args, "command")?;
    let directory = state.directory(path)?;
    let (code, stdout, stderr) = run(
        &state,
        &directory,
        vec!["bash".into(), "-c".into(), command.into()],
        args.get_bool("spool"),
    )?;
    let mut streams = crate::sandbox::output::fields(stdout, stderr)?.into_iter();
    let mut result = vec![
        ("kind", SdkValue::Text(format!("exit:{code}"))),
        ("exitCode", SdkValue::Int(i64::from(code))),
    ];
    result.extend(streams.by_ref().take(2));
    result.push(("message", SdkValue::opt_text(args.get_text("message"))));
    result.push(("truncated", SdkValue::Bool(false)));
    result.extend(streams);
    Ok(SdkValue::Map(
        result
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    ))
}

/// True while the process group of `pid` has a member that is not a zombie.
/// The container's first process is the daemon, which does not reap orphans,
/// so a finished member can stay a zombie that `kill -0` still finds.
fn start_time(state: &SandboxState, pid: u32) -> Result<Option<String>, SdkRefusal> {
    let (code, out, _) = run(state, &crate::sandbox::runtime::scratch(&state.runtime), vec!["bash".into(), "-c".into(), format!("s=$(cat /proc/{pid}/stat 2>/dev/null) || exit 1; set -- ${{s##*) }}; shift 19; printf '%s' \"$1\"")], false)?;
    if code == 0 {
        Ok(Some(String::from_utf8_lossy(out.kept()).into_owned()))
    } else {
        Ok(None)
    }
}
fn alive(state: &SandboxState, pid: u32) -> Result<bool, SdkRefusal> {
    // After the `(comm) ` field: state, parent, process group.
    let probe = format!(
        "for f in /proc/[0-9]*/stat; do s=$(cat \"$f\" 2>/dev/null) || continue; \
         set -- ${{s##*) }}; [ \"$1\" != Z ] && [ \"$3\" = {pid} ] && exit 0; done; exit 1"
    );
    let (code, _, _) = run(
        state,
        &crate::sandbox::runtime::scratch(&state.runtime),
        vec!["bash".into(), "-c".into(), probe],
        false,
    )?;
    Ok(code == 0)
}

fn owned_alive(state: &SandboxState, pid: u32, start: &str) -> Result<bool, SdkRefusal> {
    if !alive(state, pid)? {
        return Ok(false);
    }
    if start_time(state, pid)?.as_deref() != Some(start) {
        return Err(refused(
            "Unknown",
            "process group identity is no longer provable; no signal sent",
        ));
    }
    Ok(true)
}

/// Background processes still running in the Workspace at `path`.
pub(crate) fn running_in(state: &SandboxState, path: &str) -> Result<Vec<String>, SdkRefusal> {
    let started = lock(&state.processes)?
        .iter()
        .filter(|(_, process)| process.path == path && !process.stopped)
        .map(|(id, process)| (id.clone(), process.pid, process.start.clone()))
        .collect::<Vec<_>>();
    let mut running = Vec::new();
    for (id, pid, start) in started {
        if owned_alive(state, pid, &start)? {
            running.push(id);
        }
    }
    Ok(running)
}
