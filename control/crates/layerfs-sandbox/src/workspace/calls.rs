//! The `workspace` family: each call goes to its Sandbox's machine, or,
//! for a Sandbox whose daemon this host reaches (#535 G4), straight to its
//! daemon; an exec always runs through the Sandbox's runtime, in its machine.
use crate::direct;
use crate::family::sandbox_of;
use crate::host::{Call, Message};
use crate::record::SandboxRecord;
use crate::route::{on_machine, reach, Asked};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::names::{WorkspaceAt, WorkspaceName, WorkspaceRef};
use layerfs_home::outcome::Outcome;
use layerfs_machine::worker::Unreached;
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// `workspace.mount(sandbox, branch, { name })`.
#[derive(Clone, Debug, Default)]
pub struct MountRequest {
    /// The Sandbox's name.
    pub sandbox: String,
    /// The Branch it mounts.
    pub branch: Option<String>,
    /// A name, unique in the Sandbox, which `ws` then uses (#536 C3).
    pub name: Option<String>,
}

/// A Workspace just mounted.
pub struct Mounted {
    /// What names it from then on: `<sandbox>:<name>`, or
    /// `<sandbox>:<path>` when it has no name.
    pub ws: String,
    /// Its side's answer.
    pub answer: SdkValue,
}

/// `workspace.exec(ws, command, { message })`.
#[derive(Clone, Debug, Default)]
pub struct ExecRequest {
    /// The Workspace.
    pub ws: String,
    /// The bash command.
    pub command: Option<String>,
    /// Its Commit message.
    pub message: Message,
}

/// `workspace.commit(ws, { message, force })`.
#[derive(Clone, Debug, Default)]
pub struct CommitRequest {
    /// The Workspace.
    pub ws: String,
    /// Its Commit message.
    pub message: Message,
    /// Publish over a moved head instead of merging; `None` when not sent.
    pub force: Option<bool>,
}

/// `workspace.unmount(ws, { discard, force })`.
#[derive(Clone, Debug, Default)]
pub struct UnmountRequest {
    /// The Workspace.
    pub ws: String,
    /// `discard`, when sent.
    pub discard: Option<bool>,
    /// `force`, when sent.
    pub force: Option<bool>,
}

/// `workspace.status(ws)`.
#[derive(Clone, Debug, Default)]
pub struct StatusRequest {
    /// The Workspace.
    pub ws: String,
}

/// A Workspace of the Sandbox, named `ws` from then on.
pub fn mount(home: &Path, events: &mut Events<'_>, request: &MountRequest) -> Result<Mounted> {
    let home = home.to_path_buf();
    let name = request.sandbox.as_str();
    let record = sandbox_of(&home, name)?;
    // refused while a create or close holds the Sandbox, or left it
    // unresolved (#536 §7.1)
    record.free()?;
    // a name, unique in the Sandbox, which `ws` then uses (#536 C3)
    let named = match &request.name {
        Some(text) => Some(WorkspaceName::parse(text)?),
        None => None,
    };
    let call = Call::Mount {
        branch: request.branch.clone(),
        name: named.as_ref().map(|named| named.as_str().to_owned()),
    };
    // a Sandbox whose daemon this host reaches is mounted from here
    // (#535 G4); every other through its machine
    let mounted = match direct::forwarded(&record) {
        true => crate::workspace::direct::call(&home, events, &record, call, Message::Absent)?,
        false => on_machine(
            &home,
            events,
            &record,
            call,
            crate::workspace::dispatch::call,
        )?,
    };
    let path = mounted.get_text("path").unwrap_or("").to_owned();
    let ws = WorkspaceRef {
        sandbox: name.to_owned(),
        at: named.map_or(WorkspaceAt::Path(path), WorkspaceAt::Name),
    };
    Ok(Mounted {
        ws: ws.to_string(),
        answer: mounted,
    })
}

/// The Workspace `ws` names, as every call but mount starts: the home, the
/// Sandbox's record, the Workspace's path (or name), the directory an exec
/// on a Sandbox this host reaches runs in, and whether it reaches the
/// Sandbox's daemon.
struct At {
    home: PathBuf,
    record: SandboxRecord,
    path: String,
    directory: Option<String>,
    forwarded: bool,
}

/// Starts a call on `ws`: a status (`reads`) is not refused while an
/// intent holds the Sandbox; an exec (`exec`) on a Sandbox whose daemon
/// this host reaches has its directory checked here.
fn at(home: &Path, ws: &str, reads: bool, exec: bool) -> Result<At> {
    let home = home.to_path_buf();
    let ws = WorkspaceRef::parse(ws)?;
    let record = sandbox_of(&home, &ws.sandbox)?;
    // a status only reads; every other call is refused while an intent
    // holds the Sandbox
    if !reads {
        record.free()?;
    }
    // a path, or a name the Sandbox's side resolves to its path
    let mut path = ws.at.as_str().to_owned();
    let mut directory = None;
    let forwarded = direct::forwarded(&record);
    // an exec runs through the Sandbox's runtime, in its machine, even when
    // this host reaches its daemon: its directory is checked here, where
    // its Workspace records are (#535 G4)
    if forwarded && exec {
        path = crate::workspace::direct::exec_directory(&home, &record, ws.at.as_str())?;
        directory = Some(path.clone());
    }
    Ok(At {
        home,
        record,
        path,
        directory,
        forwarded,
    })
}

/// Runs `call` with `message` on the Workspace: straight to its daemon
/// when this host reaches it, else through its machine.
fn changing(events: &mut Events<'_>, at: At, call: Call, message: Message) -> Outcome {
    match at.forwarded {
        true => crate::workspace::direct::call(&at.home, events, &at.record, call, message),
        false => on_machine(
            &at.home,
            events,
            &at.record,
            Asked {
                call,
                message,
                image: None,
            },
            crate::workspace::dispatch::call,
        ),
    }
}

/// `workspace.exec`: a command in the Workspace, always through the
/// Sandbox's runtime in its machine; its output comes as events too.
pub fn exec(home: &Path, events: &mut Events<'_>, request: &ExecRequest) -> Outcome {
    let at = at(home, &request.ws, false, true)?;
    let call = Call::Exec {
        path: Some(at.path),
        directory: at.directory,
        command: request.command.clone(),
        spool: false,
    };
    let asked = Asked {
        call,
        message: request.message.clone(),
        image: None,
    };
    let out = on_machine(
        &at.home,
        events,
        &at.record,
        asked,
        crate::workspace::dispatch::call,
    )?;
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

/// `workspace.commit`: the Workspace's changes as one Commit.
pub fn commit(home: &Path, events: &mut Events<'_>, request: &CommitRequest) -> Outcome {
    let at = at(home, &request.ws, false, false)?;
    let call = Call::Commit {
        path: Some(at.path.clone()),
        force: request.force,
    };
    changing(events, at, call, request.message.clone())
}

/// `workspace.unmount`: the Workspace's terminal unmount.
pub fn unmount(home: &Path, events: &mut Events<'_>, request: &UnmountRequest) -> Outcome {
    let at = at(home, &request.ws, false, false)?;
    let call = Call::Unmount {
        path: Some(at.path.clone()),
        discard: request.discard,
        force: request.force,
    };
    changing(events, at, call, Message::Absent)
}

/// `workspace.status`: the Workspace's state; it starts no machine (P4).
pub fn status(home: &Path, events: &mut Events<'_>, request: &StatusRequest) -> Outcome {
    let at = at(home, &request.ws, true, false)?;
    let call = Call::Status {
        path: Some(at.path.clone()),
    };
    if at.forwarded {
        return crate::workspace::direct::call(&at.home, events, &at.record, call, Message::Absent);
    }
    reach(
        &at.home,
        events,
        &at.record,
        call,
        crate::workspace::dispatch::call,
    )
    .unwrap_or_else(|Unreached(why)| Err(why))
}
