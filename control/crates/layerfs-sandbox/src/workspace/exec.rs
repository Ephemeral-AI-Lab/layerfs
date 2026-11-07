//! `workspace.exec` and `workspace.exec_cancel` as a caller makes them
//! (compute-plane 2.0 §5.1): the command always runs through its Sandbox's
//! runtime, in its machine. The exec id is the caller's, or one control
//! makes here; either way it is the call's first progress event,
//! `{ exec: { id } }`. The side that keeps the Workspace's record keeps the
//! command's (`crate::exec::record`): that side, here or in a worker, for
//! every Sandbox but one this host reaches directly (#535 G4), whose
//! Workspace records, and so its commands' records (route
//! `machine:<name>`), are in this home. A cancel goes where the record is;
//! on a machine it rides `sandbox.host` (D-11), and a worker that does not
//! answer gives `unknown`, nothing changed.
use crate::exec::cancel::{self, Outcome};
use crate::exec::id::ExecId;
use crate::exec::record::{self, ExecRecord, Route, Running};
use crate::host::{Call, Message};
use crate::route::{on_machine, reach, Asked};
use crate::workspace::calls::at;
use crate::workspace::dispatch::call as dispatch;
use layerfs_bridge::control::SdkValue;
use layerfs_home::events::Events;
use layerfs_home::home::with_field;
use layerfs_home::outcome::Outcome as Answer;
use layerfs_machine::worker::Unreached;
use layerfs_sandbox_backend::Stop;
use std::io::ErrorKind;
use std::path::Path;
use std::time::Duration;

/// `workspace.exec(ws, command, { message, exec_id })`.
#[derive(Clone, Debug, Default)]
pub struct ExecRequest {
    /// The Workspace.
    pub ws: String,
    /// The bash command.
    pub command: Option<String>,
    /// Its Commit message.
    pub message: Message,
    /// The caller's exec id; control makes one when it is `None`.
    pub exec_id: Option<String>,
}

/// `workspace.exec_cancel(ws, exec_id, { grace_ms })`.
#[derive(Clone, Debug, Default)]
pub struct ExecCancelRequest {
    /// The Workspace.
    pub ws: String,
    /// The command's exec id.
    pub exec_id: String,
    /// How long it has to end after SIGTERM, in ms (5000 when `None`).
    pub grace_ms: Option<i64>,
}

/// `workspace.exec`: a command in the Workspace, always through the
/// Sandbox's runtime in its machine; its output comes as events too.
pub fn exec(home: &Path, events: &mut Events<'_>, request: &ExecRequest) -> Answer {
    let at = at(home, &request.ws, false, true)?;
    let id = match &request.exec_id {
        Some(text) => ExecId::parse(text)?,
        None => ExecId::mint()?,
    };
    let shown = SdkValue::map([("id", SdkValue::text(id.as_str()))]);
    events.progress(SdkValue::map([("exec", shown)]));
    // a Sandbox this host reaches directly: its worker keeps no record
    let running = match &at.workspace {
        Some(workspace) => {
            let wsid = workspace.id().unwrap_or_default();
            let mut kept = ExecRecord::new(&id, wsid, (&at.path, workspace.name()));
            kept.route = Route::Machine(at.record.machine().to_owned());
            let mut remote = |found: &ExecRecord, grace| {
                crate::exec::remote::remote(events, (&at.home, &at.record), found, grace)
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
        spool: false,
        exec_id: Some(id.to_string()),
        workspace: (at.workspace.as_ref()).and_then(|w| w.id().map(str::to_owned)),
    };
    let asked = Asked {
        call,
        message: request.message.clone(),
        image: None,
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
                        crate::exec::remote::remote(&mut quiet, place, kept, Duration::ZERO);
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
    let mut out = out?;
    if cancelled && out.get_int("exitCode").is_some_and(|code| code > 128) {
        out = with_field(out, "kind", SdkValue::text("cancelled"));
    }
    // the command's output as events too, for front ends that stream
    for stream in ["stdout", "stderr"] {
        match out.get(stream) {
            Some(SdkValue::Text(text)) if !text.is_empty() => {
                events.output(stream, text.as_bytes())
            }
            Some(SdkValue::Bytes(bytes)) if !bytes.is_empty() => events.output(stream, bytes),
            _ => {}
        }
    }
    Ok(out)
}

/// The stop handle a worker's `{ exec_started: { stop } }` event names.
fn started(event: &SdkValue) -> Option<Stop> {
    record::stop_of(event.get("exec_started")?.get("stop")?)
}

/// `workspace.exec_cancel`: `{ outcome, signal? }`, `outcome` being
/// `cancelled`, `ended` (it had finished) or `unknown` (not confirmed).
pub fn exec_cancel(home: &Path, events: &mut Events<'_>, request: &ExecCancelRequest) -> Answer {
    let id = ExecId::parse(&request.exec_id)?;
    let grace = cancel::grace(request.grace_ms)?;
    let at = at(home, &request.ws, false, true)?;
    if let Some(workspace) = &at.workspace {
        let dir = at.record.dir(&at.home);
        let file = record::file(&dir, workspace.id().unwrap_or_default(), id.as_str());
        let mut remote = |found: &ExecRecord, grace| {
            crate::exec::remote::remote(events, (&at.home, &at.record), found, grace)
        };
        let outcome = cancel::cancel(&dir, at.record.name(), &file, grace, &mut remote)?;
        return Ok(outcome.answer());
    }
    let call = Call::ExecCancel {
        path: Some(at.path.clone()),
        exec_id: Some(id.to_string()),
        grace_ms: request.grace_ms,
        workspace: None,
    };
    match reach(&at.home, events, &at.record, call, dispatch) {
        // not sent, or sent with no answer: nothing is known to have changed
        Err(Unreached(_)) => Ok(Outcome::Unknown.answer()),
        Ok(Err(refusal)) if refusal.code == "Unknown" => Ok(Outcome::Unknown.answer()),
        Ok(answer) => answer,
    }
}
