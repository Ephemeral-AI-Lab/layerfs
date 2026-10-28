//! Commands in a mounted Workspace, run by the Sandbox runtime owner.
//!
//! `exec` runs `bash -c <command>` as the Sandbox's command identity in the
//! Workspace directory and returns the exit code and output when it ends; the
//! daemon is not involved. A background command is started in its own session
//! with its output in a log file, and this binding keeps its process id. The
//! Engine offers no per-exec cancel, so `cancel` signals the process group
//! from a second command and waits (bounded) until it is gone.
use crate::job::{failed, refused, Outcome};
use crate::output::Spool;
use crate::registry::{lock, naming, sandbox, sandbox_live, text, Process, SandboxState};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::time::Duration;

/// Home of the command identity, which has no account in the image.
const HOME: &str = "HOME=/tmp";
/// Checks that a cancelled process group is gone.
const GONE_CHECKS: u32 = 50;
/// Pause between those checks.
const GONE_PAUSE: Duration = Duration::from_millis(100);

/// One command to its end: exit code, stdout, stderr. `spool` sends every
/// non-empty stream to its file instead of keeping it inline.
fn run(
    state: &SandboxState,
    directory: &str,
    arguments: Vec<String>,
    spool: bool,
) -> Result<(i32, Spool, Spool), SdkRefusal> {
    let created = layerfs_api::internal::exec(
        &state.runtime,
        state.identity,
        arguments,
        vec![HOME.to_owned()],
        directory.to_owned(),
        false,
    )
    .map_err(failed)?;
    let attached = created.start().map_err(failed)?;
    let (_input, output, handle) = attached.into_parts().map_err(failed)?;
    let (mut stdout, mut stderr) = (Spool::new(spool), Spool::new(spool));
    output.copy_to(&mut stdout, &mut stderr).map_err(failed)?;
    let inspected = handle.inspect().map_err(failed)?;
    let code = inspected
        .known_root_exit()
        .ok_or_else(|| refused("Unknown", "the command's exit code is not known"))?;
    Ok((code, stdout, stderr))
}

/// `workspace.exec(ws, command, { background?, spool? })`; with
/// `background`, `backend.daemon.process.start(ws, command)`.
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
    if !args.get_bool("background") {
        let (code, stdout, stderr) = run(
            &state,
            &directory,
            vec!["bash".into(), "-c".into(), command.into()],
            args.get_bool("spool"),
        )?;
        let mut streams = crate::output::fields(stdout, stderr)?.into_iter();
        let mut result = vec![
            ("kind", SdkValue::Text(format!("exit:{code}"))),
            ("exitCode", SdkValue::Int(i64::from(code))),
        ];
        result.extend(streams.by_ref().take(2));
        result.push(("message", SdkValue::opt_text(args.get_text("message"))));
        result.push(("truncated", SdkValue::Bool(false)));
        result.extend(streams);
        return Ok(SdkValue::Map(
            result
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value))
                .collect(),
        ));
    }
    let _guard = crate::discovery::guard(&state.directory)?;
    // the id names its Sandbox, so `cancel` needs nothing else
    let id = format!(
        "proc_{}_{}",
        state.id,
        layerfs_bridge::sdk_values::hex(&crate::registry::random()?[..12])
    );
    let script = format!("setsid bash -c \"$0\" >/tmp/layerfs-{id}.log 2>&1 </dev/null & echo $!");
    let (code, stdout, stderr) = run(
        &state,
        &directory,
        vec!["bash".into(), "-c".into(), script, command.into()],
        false,
    )?;
    let pid = String::from_utf8_lossy(stdout.kept())
        .trim()
        .parse::<u32>()
        .map_err(|_| {
            refused(
                "Failed",
                format!(
                    "background start exited {code}: {}",
                    String::from_utf8_lossy(stderr.kept())
                ),
            )
        })?;
    let start = start_time(&state, pid)?.ok_or_else(|| {
        refused(
            "Unknown",
            "background process exited before its identity was observed",
        )
    })?;
    let process = Process {
        path: path.to_owned(),
        command: command.to_owned(),
        pid,
        stopped: false,
        start,
    };
    crate::discovery::save_process(&state, &id, &process)?;
    lock(&state.processes)?.insert(id.clone(), process);
    Ok(SdkValue::map([
        ("kind", SdkValue::text("running")),
        ("process", SdkValue::Text(id)),
        ("command", SdkValue::text(command)),
    ]))
}

/// True while the process group of `pid` has a member that is not a zombie.
/// The container's first process is the daemon, which does not reap orphans,
/// so a finished member can stay a zombie that `kill -0` still finds.
fn start_time(state: &SandboxState, pid: u32) -> Result<Option<String>, SdkRefusal> {
    let (code, out, _) = run(state, "/tmp", vec!["bash".into(), "-c".into(), format!("s=$(cat /proc/{pid}/stat 2>/dev/null) || exit 1; set -- ${{s##*) }}; shift 19; printf '%s' \"$1\"")], false)?;
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
        "/tmp",
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

/// `backend.daemon.process.list()`: the background processes started in the
/// project's Sandboxes that still run (a gone Sandbox's went with it), each
/// with its `sandbox`.
pub(crate) fn processes(args: &SdkValue) -> Outcome {
    let mut items = Vec::new();
    for id in crate::discovery::sandbox_ids(args)? {
        let named = naming(args, &id);
        if !crate::discovery::sandbox_dir(&named, &id)?
            .join("sandbox.record")
            .exists()
        {
            continue;
        }
        match sandbox_live(&named) {
            Ok(state) => {
                crate::discovery::refresh(&state)?;
                items.extend(listed(&state)?);
            }
            Err(refusal) if crate::discovery::gone_container(&refusal).is_some() => {}
            Err(refusal) => return Err(refusal),
        }
    }
    Ok(SdkValue::map([
        ("kind", SdkValue::text("ok")),
        ("items", SdkValue::List(items)),
    ]))
}

/// One Sandbox's processes, as `processes` lists them.
fn listed(state: &SandboxState) -> Result<Vec<SdkValue>, SdkRefusal> {
    let listed = lock(&state.processes)?
        .iter()
        .map(|(id, process)| {
            (
                id.clone(),
                process.path.clone(),
                process.command.clone(),
                process.pid,
                process.stopped,
                process.start.clone(),
            )
        })
        .collect::<Vec<_>>();
    let mut items = Vec::new();
    for (id, path, command, pid, stopped, start) in listed {
        let running = !stopped && owned_alive(state, pid, &start)?;
        items.push(SdkValue::map([
            ("id", SdkValue::Text(id)),
            ("sandbox", SdkValue::text(&state.id)),
            ("path", SdkValue::Text(path)),
            ("command", SdkValue::Text(command)),
            (
                "status",
                SdkValue::text(if running {
                    "running"
                } else if stopped {
                    "stopped"
                } else {
                    "exited"
                }),
            ),
        ]));
    }
    Ok(items)
}

/// `backend.daemon.process.cancel(process)`: TERM to the process group, then a
/// bounded wait until it is gone. The process id names its Sandbox.
pub(crate) fn cancel(args: &SdkValue) -> Outcome {
    let id = text(args, "process")?;
    let sandbox = id
        .strip_prefix("proc_")
        .and_then(|rest| rest.split_once('_'))
        .map(|(sandbox, _)| sandbox)
        .ok_or_else(|| refused("NoSuchProcess", format!("{id} is not a process id")))?;
    let state = sandbox_live(&naming(args, sandbox))?;
    crate::discovery::refresh(&state)?;
    let _guard = crate::discovery::guard(&state.directory)?;
    let (pid, start, stopped) = lock(&state.processes)?
        .get(id)
        .map(|process| (process.pid, process.start.clone(), process.stopped))
        .ok_or_else(|| {
            refused(
                "NoSuchProcess",
                format!("{id} is not owned by {}", state.id),
            )
        })?;
    if stopped || !owned_alive(&state, pid, &start)? {
        return Ok(SdkValue::map([
            ("kind", SdkValue::text("Stopped")),
            ("process", SdkValue::text(id)),
            ("status", SdkValue::text("stopped")),
        ]));
    }
    run(
        &state,
        "/tmp",
        vec![
            "bash".into(),
            "-c".into(),
            format!("kill -TERM -- -{pid} 2>/dev/null; true"),
        ],
        false,
    )?;
    let mut gone = false;
    for _ in 0..GONE_CHECKS {
        if !alive(&state, pid)? {
            gone = true;
            break;
        }
        std::thread::sleep(GONE_PAUSE);
    }
    if let Some(process) = lock(&state.processes)?.get_mut(id) {
        process.stopped = gone;
        crate::discovery::save_process(&state, id, process)?;
    }
    Ok(SdkValue::map([
        ("kind", SdkValue::text("Stopped")),
        ("process", SdkValue::text(id)),
        (
            "status",
            SdkValue::text(if gone { "stopped" } else { "stopping" }),
        ),
    ]))
}
