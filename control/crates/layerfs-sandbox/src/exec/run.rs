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
use crate::output::{Place, Spool};
use crate::registry::{sandbox, SandboxState};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::helpers::required;
use layerfs_home::outcome::{refused, Outcome};
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
    /// The folder it starts in, as a path from the Workspace's root.
    pub cwd: Option<&'a str>,
    /// Its environment variables, `K=V` each.
    pub env: &'a [String],
    /// Its whole standard input; `None`: it reads `/dev/null`.
    pub stdin: Option<&'a [u8]>,
    /// Its exec id (control makes one when a caller sends none).
    pub exec_id: Option<&'a str>,
    /// The Workspace's id, from a caller that keeps its record and the
    /// command's (#535 G4): no record is kept on this side.
    pub workspace: Option<&'a str>,
    /// The log tag of the call that asked for it, for the command's record.
    pub tag: Option<&'a str>,
}

/// One command to its end: exit code, stdout, stderr and its stop
/// handle's form. A stream past the inline window spills to its file at
/// `place`. Its home is the Sandbox's scratch directory, since the command
/// identity has no account there; the caller's variables come after it,
/// so a `HOME` of theirs wins. While this thread's steps are timed, so are
/// the runtime's (`layerfs_sandbox_backend::steps`), and `command` is the
/// whole run.
fn run(
    state: &SandboxState,
    (directory, place): (&str, Option<&Place>),
    (arguments, env, stdin): (Vec<String>, &[String], Option<&[u8]>),
    slot: ExecSlot<'_>,
) -> Result<(i32, Spool, Spool), SdkRefusal> {
    let runtime = state.runtime()?;
    let stepped = layerfs_home::trace::stepping();
    if stepped {
        layerfs_sandbox_backend::steps::begin();
    }
    let mut environment = vec![format!("HOME={}", crate::runtime::scratch(runtime))];
    environment.extend(env.iter().cloned());
    let ran = crate::runtime::run(
        runtime,
        state.identity,
        (arguments, environment, stdin.map(<[u8]>::to_vec)),
        (directory, place),
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

/// The folder a command starts in: Workspace directory `root`, or its
/// folder `cwd` (a path from the root; a leading `/` is the root's).
/// Refused `InvalidInput` for a `..`, `.` or empty component, or a NUL.
pub fn start_folder(root: &str, cwd: Option<&str>) -> Result<String, SdkRefusal> {
    let Some(cwd) = cwd else {
        return Ok(root.to_owned());
    };
    let inside = cwd.trim_start_matches('/').trim_end_matches('/');
    if inside.is_empty() {
        return Ok(root.to_owned());
    }
    if cwd.contains('\0')
        || inside
            .split('/')
            .any(|part| matches!(part, "" | "." | ".."))
    {
        return Err(refused(
            "InvalidInput",
            format!("cwd is a folder of the Workspace as a path from its root, not {cwd:?}"),
        ));
    }
    Ok(format!("{}/{inside}", root.trim_end_matches('/')))
}

/// Refused `InvalidInput` unless every entry of `env` is `K=V`: a name of
/// letters, digits and `_`, not starting with a digit, and no NUL.
pub fn checked_env(env: &[String]) -> Result<(), SdkRefusal> {
    for entry in env {
        let good = entry.split_once('=').is_some_and(|(name, _)| {
            !name.is_empty()
                && !name.starts_with(|c: char| c.is_ascii_digit())
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        }) && !entry.contains('\0');
        if !good {
            return Err(refused(
                "InvalidInput",
                format!("env is K=V with K a variable name, not {entry:?}"),
            ));
        }
    }
    Ok(())
}

/// `workspace.exec(ws, command, { cwd, env, stdin })` on the Workspace at
/// `path` (a `directory` a caller checked, and the `command`).
///
/// Its result is `exit_code`, `cancelled` (a cancel ended it: exit code
/// 128 + the signal), the streams (`crate::output::fields`: each inline,
/// or null with its tail and file once past 1 MiB, both sizes), then
/// `exec`, `stop` (`cgroup` or `group`, how a cancel ends it) and
/// `duration_ms`. A `directory` from a caller that keeps the Workspace's
/// record (#535 G4) is the one the command runs in; that caller keeps the
/// command's record too.
pub fn exec(host: &Host, exec: &Exec<'_>) -> Outcome {
    let begun = std::time::Instant::now();
    let state = sandbox(host)?;
    layerfs_home::trace::step("registry");
    let path = required(exec.path, "path")?;
    let command = required(exec.command, "command")?;
    checked_env(exec.env)?;
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
    let start = start_folder(&directory, exec.cwd)?;
    // a Sandbox with no runtime is refused before any record is made
    let runtime = state.runtime()?;
    let place =
        crate::output::folder(&crate::runtime::scratch(runtime), &workspace).map(|folder| Place {
            folder,
            exec: id.to_string(),
        });
    let running = match exec.workspace {
        Some(_) => None,
        None => {
            let named = crate::files::workspaces(&state.directory)?
                .into_iter()
                .find(|(_, record)| record.path() == Some(directory.as_str()))
                .and_then(|(_, record)| record.name().map(str::to_owned));
            // its stop handle is recorded once the command runs in it
            let record = ExecRecord::new(&id, &workspace, (&directory, named.as_deref()))
                .started_by(exec.tag, command);
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
    let ran = run(
        &state,
        (&start, place.as_ref()),
        (arguments, exec.env, exec.stdin),
        slot,
    );
    let cancelled = match running {
        Some(running) => running.end()?,
        None => false,
    };
    layerfs_home::trace::step("exec_record_end");
    let (code, stdout, stderr) = ran?;
    let mut result = vec![
        ("exit_code", SdkValue::Int(i64::from(code))),
        ("cancelled", SdkValue::Bool(cancelled && code > 128)),
    ];
    result.extend(crate::output::fields(stdout, stderr)?);
    result.push(("exec", SdkValue::text(id.as_str())));
    result.push(("stop", SdkValue::opt_text(form.map(str::to_owned))));
    let took = i64::try_from(begun.elapsed().as_millis()).unwrap_or(i64::MAX);
    result.push(("duration_ms", SdkValue::Int(took)));
    Ok(SdkValue::Map(
        result
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    ))
}
