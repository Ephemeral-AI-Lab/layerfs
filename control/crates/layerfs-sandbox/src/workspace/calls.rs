//! The `workspace` family: each call goes to its Sandbox's machine, or,
//! for a Sandbox whose daemon this host reaches (#535 G4), straight to its
//! daemon; an exec always runs through the Sandbox's runtime, in its machine.
use crate::direct;
use crate::family::sandbox_of;
use crate::record::SandboxRecord;
use crate::route::{on_machine, reach};
use layerfs_bridge::control::SdkRefusal;
use layerfs_bridge::control::SdkValue;
use layerfs_home::events::Events;
use layerfs_home::home;
use layerfs_home::names::{WorkspaceAt, WorkspaceName, WorkspaceRef};
use layerfs_home::outcome::Outcome;
use layerfs_machine::worker::Unreached;
use std::path::{Path, PathBuf};

/// `workspace.mount`: a Workspace of the Sandbox, named `ws` from then on.
pub fn mount(home: &Path, events: &mut Events<'_>, args: &SdkValue) -> Outcome {
    let home = home.to_path_buf();
    let name = args.get_text("sandbox").unwrap_or("");
    let record = sandbox_of(&home, name)?;
    // refused while a create or close holds the Sandbox, or left it
    // unresolved (#536 §7.1)
    record.free()?;
    let branch = args.get("branch").cloned().unwrap_or_default();
    let mut more = vec![("branch", branch)];
    // a name, unique in the Sandbox, which `ws` then uses (#536 C3)
    let named = match args.get_text("name") {
        Some(text) => Some(WorkspaceName::parse(text)?),
        None => None,
    };
    if let Some(named) = &named {
        more.push(("name", SdkValue::text(named.as_str())));
    }
    // a Sandbox whose daemon this host reaches is mounted from here
    // (#535 G4); every other through its machine
    let mounted = match direct::forwarded(&record) {
        true => crate::workspace::direct::call(&home, events, &record, "workspace.mount", more)?,
        false => on_machine(
            &home,
            events,
            &record,
            "workspace.mount",
            more,
            crate::workspace::dispatch::call,
        )?,
    };
    let path = mounted.get_text("path").unwrap_or("").to_owned();
    let ws = WorkspaceRef {
        sandbox: name.to_owned(),
        at: named.map_or(WorkspaceAt::Path(path), WorkspaceAt::Name),
    };
    Ok(home::with_field(
        mounted,
        "ws",
        SdkValue::text(ws.to_string()),
    ))
}

/// The Workspace `ws` names, as every call but mount starts: the home, the
/// Sandbox's record, the request's fields so far and whether this host
/// reaches the Sandbox's daemon.
struct At {
    home: PathBuf,
    record: SandboxRecord,
    more: Vec<(&'static str, SdkValue)>,
    forwarded: bool,
}

/// Starts a call on `ws`: a status (`reads`) is not refused while an
/// intent holds the Sandbox; an exec (`exec`) on a Sandbox whose daemon
/// this host reaches has its directory checked here.
fn at(home: &Path, args: &SdkValue, reads: bool, exec: bool) -> Result<At, SdkRefusal> {
    let home = home.to_path_buf();
    let ws = WorkspaceRef::parse(args.get_text("ws").unwrap_or(""))?;
    let record = sandbox_of(&home, &ws.sandbox)?;
    // a status only reads; every other call is refused while an intent
    // holds the Sandbox
    if !reads {
        record.free()?;
    }
    // a path, or a name the Sandbox's side resolves to its path
    let mut more = vec![("path", SdkValue::text(ws.at.as_str()))];
    let forwarded = direct::forwarded(&record);
    // an exec runs through the Sandbox's runtime, in its machine, even when
    // this host reaches its daemon: its directory is checked here, where
    // its Workspace records are (#535 G4)
    if forwarded && exec {
        let directory = crate::workspace::direct::exec_directory(&home, &record, ws.at.as_str())?;
        more = vec![
            ("path", SdkValue::text(&directory)),
            ("directory", SdkValue::Text(directory)),
        ];
    }
    Ok(At {
        home,
        record,
        more,
        forwarded,
    })
}

/// Runs `op` on the Workspace: straight to its daemon when this host
/// reaches it, else through its machine.
fn changing(events: &mut Events<'_>, at: At, op: &str) -> Outcome {
    match at.forwarded {
        true => crate::workspace::direct::call(&at.home, events, &at.record, op, at.more),
        false => on_machine(
            &at.home,
            events,
            &at.record,
            op,
            at.more,
            crate::workspace::dispatch::call,
        ),
    }
}

/// `workspace.exec`: a command in the Workspace, always through the
/// Sandbox's runtime in its machine; its output comes as events too.
pub fn exec(home: &Path, events: &mut Events<'_>, args: &SdkValue) -> Outcome {
    let mut at = at(home, args, false, true)?;
    at.more
        .push(("command", args.get("command").cloned().unwrap_or_default()));
    at.more
        .push(("message", args.get("message").cloned().unwrap_or_default()));
    let out = on_machine(
        &at.home,
        events,
        &at.record,
        "workspace.exec",
        at.more,
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
pub fn commit(home: &Path, events: &mut Events<'_>, args: &SdkValue) -> Outcome {
    let mut at = at(home, args, false, false)?;
    at.more
        .push(("message", args.get("message").cloned().unwrap_or_default()));
    // publish over a moved head instead of merging
    if let Some(force) = args.get("force") {
        at.more.push(("force", force.clone()));
    }
    changing(events, at, "workspace.commit")
}

/// `workspace.unmount`: the Workspace's terminal unmount.
pub fn unmount(home: &Path, events: &mut Events<'_>, args: &SdkValue) -> Outcome {
    let mut at = at(home, args, false, false)?;
    for flag in ["discard", "force"] {
        if let Some(value) = args.get(flag) {
            at.more.push((flag, value.clone()));
        }
    }
    changing(events, at, "workspace.unmount")
}

/// `workspace.status`: the Workspace's state; it starts no machine (P4).
pub fn status(home: &Path, events: &mut Events<'_>, args: &SdkValue) -> Outcome {
    let at = at(home, args, true, false)?;
    let op = "backend.daemon.workspace.status";
    if at.forwarded {
        return crate::workspace::direct::call(&at.home, events, &at.record, op, at.more);
    }
    reach(
        &at.home,
        events,
        &at.record,
        op,
        at.more,
        false,
        crate::workspace::dispatch::call,
    )
    .unwrap_or_else(|Unreached(why)| Err(why))
}
