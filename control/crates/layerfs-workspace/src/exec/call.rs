//! `workspace.exec` and `workspace.cancel` as a caller makes them
//! (compute-plane 2.0 §5.1, call surface 2.0): the command always runs
//! through its Sandbox's runtime, in its machine. Control makes the exec id
//! (`exec-` and 16 hex digits) and sends it as the call's progress event,
//! `{ phase: "start", done: 0, total: 1, exec }`, before the command
//! starts. The side that keeps the Workspace's record keeps the command's
//! (`crate::exec::record`): that side, here or in a worker, for every
//! Sandbox but one this host reaches directly (#535 G4), whose Workspace
//! records, and so its commands' records (route `machine:<name>`), are in
//! this home. A cancel goes where the records are; on a machine it rides
//! `sandbox.host` (D-11), and a worker that does not answer gives
//! `unknown`, nothing changed. Nothing here times a command: there is no
//! implicit timeout, Commit or unmount.
use layerfs_sandbox::exec::cancel::{self, Outcome};
use layerfs_sandbox::exec::id::ExecId;
use layerfs_sandbox::exec::record::{self, ExecRecord, Route, Running};
use layerfs_sandbox::host::{Call, Message};
use layerfs_sandbox::route::{on_machine, reach, Asked};
use crate::route::send::at;
use crate::route::dispatch::call as dispatch;
use layerfs_protocol::provision::install::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::outcome::shell_word;
use layerfs_home::outcome::{with_fix, with_next, Outcome as Answer};
use layerfs_machine::worker::Unreached;
use layerfs_sandbox::Stop;
use std::io::ErrorKind;
use std::path::Path;
use std::time::Duration;

/// `workspace.exec(workspace, command, { cwd, env, stdin })`.
#[derive(Clone, Debug, Default)]
pub struct ExecRequest {
    /// The Workspace, `<sandbox>:<name>`.
    pub ws: String,
    /// The bash command.
    pub command: Option<String>,
    /// The folder it starts in, as a path from the Workspace's root.
    pub cwd: Option<String>,
    /// Its environment variables, `K=V` each.
    pub env: Vec<String>,
    /// Its whole standard input; `None`: it reads `/dev/null`.
    pub stdin: Option<Vec<u8>>,
    /// The call's log tag, kept in the command's record while it runs
    /// (`sandbox.get` shows it under `running`).
    pub tag: Option<String>,
}

/// `workspace.cancel(workspace, { exec })`.
#[derive(Clone, Debug, Default)]
pub struct CancelRequest {
    /// The Workspace, `<sandbox>:<name>`.
    pub ws: String,
    /// The command's exec id; with none, every command running in it.
    pub exec: Option<String>,
}

/// `workspace.exec`: a command in the Workspace, always through the
/// Sandbox's runtime in its machine; its output comes as events too. The
/// answer is `{ exit_code, stdout, stderr, stdout_file, stderr_file,
/// cancelled, exec, stop, stdout_bytes, stderr_bytes, duration_ms }`: a
/// stream past 1 MiB is its last 1 MiB, with the file that holds all of it.
/// A lost reply is the refusal `Unknown` with `exec` and a `fix` naming
/// `workspace cancel --exec`; nothing is run again.
pub fn exec(home: &Path, events: &mut Events<'_>, request: &ExecRequest) -> Answer {
    let at = at(home, &request.ws, false, true)?;
    crate::exec::run::checked_env(&request.env)?;
    // the folder it starts in is checked before anything is sent
    crate::exec::run::start_folder("/", request.cwd.as_deref())?;
    let id = ExecId::mint()?;
    events.phase(
        "start",
        Some(0),
        Some(1),
        vec![("exec", SdkValue::text(id.as_str()))],
    );
    // a Sandbox this host reaches directly: its worker keeps no record
    let running = match &at.workspace {
        Some(workspace) => {
            let wsid = workspace.id().unwrap_or_default();
            let command = request.command.as_deref().unwrap_or("");
            let mut kept = ExecRecord::new(&id, wsid, (&at.path, workspace.name()))
                .started_by(request.tag.as_deref(), command);
            kept.route = Route::Machine(at.record.machine().to_owned());
            let mut remote = |found: &ExecRecord, grace| {
                layerfs_sandbox::exec::remote::remote(events, (&at.home, &at.record), found, grace)
            };
            let dir = at.record.dir(&at.home);
            let running = Running::start(&dir, at.record.name(), &kept, &mut remote)?;
            Some((running, kept))
        }
        None => None,
    };
    let call = Call::Exec {
        path: Some(at.path.clone()),
        directory: at.directory.clone(),
        command: request.command.clone(),
        cwd: request.cwd.clone(),
        env: request.env.clone(),
        stdin: request.stdin.clone(),
        exec_id: Some(id.to_string()),
        workspace: (at.workspace.as_ref()).and_then(|w| w.id().map(str::to_owned)),
        tag: request.tag.clone(),
    };
    let asked = Asked {
        call,
        message: Message::Absent,
        image: None,
        start: None,
    };
    let out = match &running {
        // the worker tells when the command started: its stop handle is
        // recorded then, and a cancel that came first ends it there
        Some((running, kept)) => {
            let mut passed = |event: SdkValue| match started(&event) {
                Some(stop) => {
                    let refused = running.spawned(&stop).err();
                    if refused.is_some_and(|error| error.kind() == ErrorKind::Interrupted) {
                        let mut none = |_| {};
                        let mut quiet = Events::new(&mut none);
                        let place = (at.home.as_path(), &at.record);
                        layerfs_sandbox::exec::remote::remote(&mut quiet, place, kept, Duration::ZERO);
                    }
                }
                None => events.event(event),
            };
            let mut relayed = Events::new(&mut passed);
            on_machine(&at.home, &mut relayed, &at.record, asked, dispatch)
        }
        None => on_machine(&at.home, events, &at.record, asked, dispatch),
    };
    let cancelled = match running {
        Some((running, _)) => running.end()?,
        None => false,
    };
    let out = out.map_err(|refusal| lost(refusal, &request.ws, &id))?;
    // the command's inline output as events too, for front ends that
    // stream; a spilled stream was streamed whole by the side that ran it
    for stream in ["stdout", "stderr"] {
        match out.get(stream) {
            Some(SdkValue::Text(text)) if !text.is_empty() => {
                events.output(stream, text.as_bytes())
            }
            Some(SdkValue::Bytes(bytes)) if !bytes.is_empty() => events.output(stream, bytes),
            _ => {}
        }
    }
    Ok(answer(&out, cancelled))
}

/// A refusal of the exec: a lost reply (`Unknown`) carries the exec id and
/// the cancel that ends the command if it still runs.
fn lost(refusal: SdkRefusal, ws: &str, id: &ExecId) -> SdkRefusal {
    if refusal.code != "Unknown" {
        return refusal;
    }
    let refusal = with_next(refusal, "exec", SdkValue::text(id.as_str()));
    with_fix(
        refusal,
        format!(
            "layerfs workspace cancel {ws} --exec {id}",
            ws = shell_word(ws),
            id = shell_word(id.as_str())
        ),
    )
}

/// The call's answer from the side's result `out` (`exec::run::exec`'s).
fn answer(out: &SdkValue, cancelled: bool) -> SdkValue {
    let take = |name: &str| out.get(name).cloned().unwrap_or_default();
    let stream = |name: &str, tail: &str| match out.get(name) {
        Some(SdkValue::Null) | None => take(tail),
        Some(value) => value.clone(),
    };
    let exit = out.get_int("exit_code");
    let cancelled = out.get_bool("cancelled") || (cancelled && exit.is_some_and(|code| code > 128));
    SdkValue::map([
        ("exit_code", take("exit_code")),
        ("stdout", stream("stdout", "stdoutTail")),
        ("stderr", stream("stderr", "stderrTail")),
        ("stdout_file", take("stdoutFile")),
        ("stderr_file", take("stderrFile")),
        ("cancelled", SdkValue::Bool(cancelled)),
        ("exec", take("exec")),
        ("stop", take("stop")),
        ("stdout_bytes", take("stdoutBytes")),
        ("stderr_bytes", take("stderrBytes")),
        ("duration_ms", take("duration_ms")),
    ])
}

/// The stop handle a worker's `{ exec_started: { stop } }` event names.
fn started(event: &SdkValue) -> Option<Stop> {
    record::stop_of(event.get("exec_started")?.get("stop")?)
}

/// `workspace.cancel`: `{ signal, already_ended, unknown, outcome,
/// commands }`. Each command gets SIGTERM, then SIGKILL after 5 s.
/// `outcome` is the worst of `commands[].outcome` (unknown, then cancelled,
/// then already_ended); nothing running is `already_ended`; a worker that
/// was not reached or did not answer is `unknown`, nothing known to have
/// changed.
pub fn cancel_call(home: &Path, events: &mut Events<'_>, request: &CancelRequest) -> Answer {
    let id = request.exec.as_deref().map(ExecId::parse).transpose()?;
    let at = at(home, &request.ws, false, true)?;
    if let Some(workspace) = &at.workspace {
        let dir = at.record.dir(&at.home);
        let wsid = workspace.id().unwrap_or_default();
        let mut remote = |found: &ExecRecord, grace| {
            layerfs_sandbox::exec::remote::remote(events, (&at.home, &at.record), found, grace)
        };
        let commands = cancel::every(&dir, at.record.name(), wsid, id.as_ref(), &mut remote)?;
        return Ok(cancelled(&commands, false));
    }
    let call = Call::ExecCancel {
        path: Some(at.path.clone()),
        exec_id: id.as_ref().map(ExecId::to_string),
        grace_ms: None,
        workspace: None,
    };
    let named = |outcome| -> Vec<(String, Outcome)> {
        id.iter().map(|id| (id.to_string(), outcome)).collect()
    };
    match reach(&at.home, events, &at.record, call, dispatch) {
        // not sent, or sent with no answer: nothing is known to have changed
        Err(Unreached(_)) => Ok(cancelled(&named(Outcome::Unknown), true)),
        Ok(Err(refusal)) if refusal.code == "Unknown" => {
            Ok(cancelled(&named(Outcome::Unknown), true))
        }
        Ok(Err(refusal)) => Err(refusal),
        Ok(Ok(answer)) => Ok(cancelled(&commands_of(&answer), false)),
    }
}

/// The commands a side's `{ commands: [{ exec, outcome, signal }] }` names.
fn commands_of(answer: &SdkValue) -> Vec<(String, Outcome)> {
    let Some(SdkValue::List(rows)) = answer.get("commands") else {
        return Vec::new();
    };
    (rows.iter())
        .map(|row| {
            let id = row.get_text("exec").unwrap_or("").to_owned();
            (id, Outcome::read(row))
        })
        .collect()
}

/// The answer of a cancel of `commands`; `unreached` when its side was not
/// reached or did not answer.
fn cancelled(commands: &[(String, Outcome)], unreached: bool) -> SdkValue {
    let unknown = unreached || commands.iter().any(|(_, o)| *o == Outcome::Unknown);
    let signals: Vec<&str> = commands.iter().filter_map(|(_, o)| o.signal()).collect();
    let outcome = match (unknown, signals.is_empty()) {
        (true, _) => "unknown",
        (false, false) => "cancelled",
        (false, true) => "already_ended",
    };
    // the last signal sent: SIGKILL once any command outlived its grace
    let signal = match signals.contains(&"SIGKILL") {
        true => Some("SIGKILL"),
        false => signals.first().copied(),
    };
    let listed = match cancel::listed_answer(commands) {
        SdkValue::Map(mut fields) => fields.pop().map(|(_, rows)| rows).unwrap_or_default(),
        _ => SdkValue::Null,
    };
    SdkValue::map([
        ("signal", SdkValue::opt_text(signal)),
        ("already_ended", SdkValue::Bool(outcome == "already_ended")),
        ("unknown", SdkValue::Bool(unknown)),
        ("outcome", SdkValue::text(outcome)),
        ("commands", listed),
    ])
}
