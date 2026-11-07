//! Commands in a mounted Workspace, run by the Sandbox runtime owner.
//!
//! `exec` runs `bash -c <command>` as the Sandbox's command identity in the
//! Workspace directory and returns the exit code and output when it ends; the
//! daemon is not involved. A local Sandbox
//! (`layerfs_sandbox_backend::Sandbox::HostProcess`) runs the same commands as
//! processes of this kernel. Each command has an exec id, a stop handle and,
//! on the side that keeps its Workspace's record, a record while it runs
//! (`super::record`), through which another process cancels it.
use super::id::ExecId;
use super::record::{ExecRecord, Running};
use crate::host::Host;
use crate::output::Spool;
use crate::registry::{sandbox, SandboxState};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::helpers::required;
use layerfs_home::outcome::Outcome;
use layerfs_sandbox_backend::{ExecSlot, Stop};

/// One exec as a host operation carries it.
#[derive(Clone, Debug, Default)]
pub struct Exec<'a> {
    /// The Workspace's path (or name).
    pub path: Option<&'a str>,
    /// The directory a caller that keeps the Workspace's record checked
    /// (#535 G4).
    pub directory: Option<&'a str>,
    /// The bash command.
    pub command: Option<&'a str>,
    /// Send every non-empty stream to its file.
    pub spool: bool,
    /// Its exec id (control makes one when a caller sends none).
    pub exec_id: Option<&'a str>,
    /// The Workspace's id, from a caller that keeps its record and the
    /// command's (#535 G4): no record is kept on this side.
    pub workspace: Option<&'a str>,
}

/// One command to its end: exit code, stdout, stderr and its stop handle's
/// form. `spool` sends every non-empty stream to its file instead of
/// keeping it inline. Its home is the Sandbox's scratch directory, since
/// the command identity has no account there. While this thread's steps
/// are timed, so are the runtime's (`layerfs_sandbox_backend::steps`), and
/// `command` is the whole run.
fn run(
    state: &SandboxState,
    directory: &str,
    arguments: Vec<String>,
    spool: bool,
    slot: ExecSlot<'_>,
) -> Result<(i32, Spool, Spool), SdkRefusal> {
    let runtime = state.runtime()?;
    let stepped = layerfs_home::trace::stepping();
    if stepped {
        layerfs_sandbox_backend::steps::begin();
    }
    let ran = crate::runtime::run(
        runtime,
        state.identity,
        arguments,
        vec![format!("HOME={}", crate::runtime::scratch(runtime))],
        (directory, spool),
        slot,
    );
    if stepped {
        for (name, took) in layerfs_sandbox_backend::steps::take() {
            layerfs_home::trace::step_of(name, took);
        }
    }
    layerfs_home::trace::step("command");
    ran
}

/// `workspace.exec(ws, command, { spool?, exec_id? })` on the Workspace at
/// `path` (a `directory` a caller checked, and the `command`).
///
/// A foreground result is `kind`, `exitCode`, `stdout`, `stderr`, `message`
/// and `truncated` (always false: no output is cut), then `execId` and
/// `stop` (`cgroup` or `group`, how a cancel ends it). A command a cancel
/// ended is `kind: "cancelled"` with exit code 128 + the signal. A stream
/// over the inline window (1 MiB), or any non-empty stream with `spool`, is
/// null here and spooled; the result then also names its `output`, read
/// with `workspace.exec_output` and closed with `sandbox.exec_release`, and
/// the `stdoutBytes` and `stderrBytes` sizes. A `directory` from a caller
/// that keeps the Workspace's record (#535 G4) is the one the command runs
/// in; that caller keeps the command's record too.
pub fn exec(host: &Host, exec: &Exec<'_>) -> Outcome {
    let state = sandbox(host)?;
    layerfs_home::trace::step("registry");
    let path = required(exec.path, "path")?;
    let command = required(exec.command, "command")?;
    let id = match exec.exec_id {
        Some(id) => ExecId::parse(id)?,
        None => ExecId::mint()?,
    };
    // a caller that reaches the Sandbox's daemon directly keeps its
    // Workspace records and sends the directory it checked there (#535 G4)
    let (directory, workspace) = match (exec.directory, exec.workspace) {
        (Some(directory), Some(workspace)) => (directory.to_owned(), workspace.to_owned()),
        _ => (
            state.directory(path)?,
            layerfs_bridge::sdk_values::hex(&state.token(path)?.workspace.to_bytes()),
        ),
    };
    // a Sandbox with no runtime is refused before any record is made
    state.runtime()?;
    let running = match exec.workspace {
        Some(_) => None,
        None => {
            let named = crate::files::workspaces(&state.directory)?
                .into_iter()
                .find(|(_, record)| record.path() == Some(directory.as_str()))
                .and_then(|(_, record)| record.name().map(str::to_owned));
            // its stop handle is recorded once the command runs in it
            let record = ExecRecord::new(&id, &workspace, (&directory, named.as_deref()));
            let unreached = &mut crate::exec::cancel::unreached;
            Some(Running::start(
                &state.directory,
                &state.id,
                &record,
                unreached,
            )?)
        }
    };
    let mut form = None;
    let started = host.started.clone();
    let mut spawned = |stop: &Stop| {
        form = Some(stop.form());
        // a caller that keeps the record records the handle now
        if let Some(started) = &started {
            let stop = super::record::stop_value(stop);
            let _ = started.send(SdkValue::map([(
                "exec_started",
                SdkValue::map([("stop", stop)]),
            )]));
        }
        running
            .as_ref()
            .map_or(Ok(()), |running| running.spawned(stop))
    };
    let slot = ExecSlot {
        workspace: &workspace,
        exec: id.as_str(),
        spawned: &mut spawned,
    };
    let arguments = vec!["bash".into(), "-c".into(), command.into()];
    layerfs_home::trace::step("exec_record");
    let ran = run(&state, &directory, arguments, exec.spool, slot);
    let cancelled = match running {
        Some(running) => running.end()?,
        None => false,
    };
    layerfs_home::trace::step("exec_record_end");
    let (code, stdout, stderr) = ran?;
    let kind = match cancelled && code > 128 {
        true => "cancelled".to_owned(),
        false => format!("exit:{code}"),
    };
    let mut streams = crate::output::fields(stdout, stderr)?.into_iter();
    let mut result = vec![
        ("kind", SdkValue::Text(kind)),
        ("exitCode", SdkValue::Int(i64::from(code))),
    ];
    result.extend(streams.by_ref().take(2));
    result.push((
        "message",
        SdkValue::opt_text(host.message.text().map(str::to_owned)),
    ));
    result.push(("truncated", SdkValue::Bool(false)));
    result.extend(streams);
    result.push(("execId", SdkValue::text(id.as_str())));
    result.push(("stop", SdkValue::opt_text(form.map(str::to_owned))));
    Ok(SdkValue::Map(
        result
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    ))
}
