//! The `workspace` family: each call goes to its Sandbox's machine, or,
//! for a Sandbox whose daemon this host reaches (#535 G4), straight to its
//! daemon; an exec always runs through the Sandbox's runtime, in its machine.
use crate::common::context::Context;
use crate::common::home;
use crate::common::names::{WorkspaceAt, WorkspaceName, WorkspaceRef};
use crate::common::outcome::{refused, Outcome};
use crate::machine::worker::Unreached;
use crate::sandbox::direct;
use crate::sandbox::family::sandbox_of;
use crate::sandbox::route::{on_machine, reach};
use layerfs_bridge::control::{SdkRefusal, SdkValue};

/// Runs one `workspace` call.
pub fn workspace(context: &mut Context<'_>, call: &str, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    if call == "mount" {
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
            true => direct::call(context, &record, "workspace.mount", more)?,
            false => on_machine(context, &record, "workspace.mount", more)?,
        };
        let path = mounted.get_text("path").unwrap_or("").to_owned();
        let ws = WorkspaceRef {
            sandbox: name.to_owned(),
            at: named.map_or(WorkspaceAt::Path(path), WorkspaceAt::Name),
        };
        return Ok(home::with_field(
            mounted,
            "ws",
            SdkValue::text(ws.to_string()),
        ));
    }
    let ws = WorkspaceRef::parse(args.get_text("ws").unwrap_or(""))?;
    let record = sandbox_of(&home, &ws.sandbox)?;
    // a status only reads; every other call is refused while an intent
    // holds the Sandbox
    if call != "status" {
        record.free()?;
    }
    // a path, or a name the Sandbox's side resolves to its path
    let mut more = vec![("path", SdkValue::text(ws.at.as_str()))];
    let forwarded = direct::forwarded(&record);
    // an exec runs through the Sandbox's runtime, in its machine, even when
    // this host reaches its daemon: its directory is checked here, where
    // its Workspace records are (#535 G4)
    if forwarded && call == "exec" {
        let directory = direct::exec_directory(&home, &record, ws.at.as_str())?;
        more = vec![
            ("path", SdkValue::text(&directory)),
            ("directory", SdkValue::Text(directory)),
        ];
    }
    // A workspace-scoped kill, and the force gate, run where the Sandbox's
    // runtime is: its machine's worker or this process, even when this host
    // reaches only its daemon's forwarded port (#535 G4).
    let ending = match call {
        "unmount" => Some(crate::workspace::serve::ending(args)?),
        _ => None,
    };
    let killing = ending == Some(crate::workspace::serve::Ending::Kill);
    if killing || ending == Some(crate::workspace::serve::Ending::Normal { force: true }) {
        let at = ws.at.as_str();
        // a name is resolved here where the Workspace records are; the
        // machine side resolves one it recorded itself
        let path = match forwarded && !at.starts_with('/') {
            true => direct::exec_directory(&home, &record, at)?,
            false => at.to_owned(),
        };
        match holders(context, &record, &path, killing) {
            Ok(Some((path, ids))) => {
                return Err(crate::sandbox::exec::busy(record.name(), &path, ids))
            }
            Ok(None) => {}
            // a scan that cannot run leaves the recovery path (`force`) as it
            // was, exactly as before this option existed
            Err(_) if !killing => {}
            Err(refusal) => return Err(refusal),
        }
    }
    let op = match call {
        "exec" => {
            more.push(("command", args.get("command").cloned().unwrap_or_default()));
            more.push(("message", args.get("message").cloned().unwrap_or_default()));
            "workspace.exec"
        }
        "commit" => {
            more.push(("message", args.get("message").cloned().unwrap_or_default()));
            // publish over a moved head instead of merging
            if let Some(force) = args.get("force") {
                more.push(("force", force.clone()));
            }
            "workspace.commit"
        }
        "unmount" => {
            for flag in ["discard", "force", "kill"] {
                if let Some(value) = args.get(flag) {
                    more.push((flag, value.clone()));
                }
            }
            "workspace.unmount"
        }
        "status" => "backend.daemon.workspace.status",
        _ => return Err(refused("InvalidInput", format!("no call workspace.{call}"))),
    };
    // a status starts no machine (P4)
    let out = match call {
        _ if forwarded && call != "exec" => direct::call(context, &record, op, more),
        "status" => {
            reach(context, &record, op, more, false).unwrap_or_else(|Unreached(why)| Err(why))
        }
        _ => on_machine(context, &record, op, more),
    };
    // A normal unmount the daemon refused Busy names the executor's foreground
    // execs, so the caller can cancel them instead of forcing.
    let out = match out {
        Err(refusal)
            if call == "unmount"
                && refusal.code == "Busy"
                && refusal.detail.get("processes").is_none() =>
        {
            let at = ws.at.as_str();
            let path = match at.starts_with('/') {
                true => at.to_owned(),
                false => {
                    direct::exec_directory(&home, &record, at).unwrap_or_else(|_| at.to_owned())
                }
            };
            match holders(context, &record, &path, false) {
                Ok(Some((path, ids))) => Err(crate::sandbox::exec::busy(record.name(), &path, ids)),
                _ => Err(refusal),
            }
        }
        other => other,
    };
    let out = out?;
    if call == "exec" {
        // the command's output as events too, for front ends that stream
        for stream in ["stdout", "stderr"] {
            match out.get(stream) {
                Some(SdkValue::Text(text)) if !text.is_empty() => {
                    context.output(stream, text.as_bytes())
                }
                Some(SdkValue::Bytes(bytes)) if !bytes.is_empty() => context.output(stream, bytes),
                _ => {}
            }
        }
    }
    Ok(out)
}

/// The foreground execs still running in the Workspace at `path`, from the
/// side that runs the Sandbox's runtime: `{ processes }` read, or cancelled
/// first (`kill`). `Ok(None)` when none is left.
fn holders(
    context: &mut Context<'_>,
    record: &crate::sandbox::record::SandboxRecord,
    path: &str,
    kill: bool,
) -> Result<Option<(String, Vec<String>)>, SdkRefusal> {
    let more = vec![
        ("path", SdkValue::text(path)),
        ("kill", SdkValue::Bool(kill)),
    ];
    let answer = on_machine(context, record, "workspace.holders", more)?;
    let ids = match answer.get("processes") {
        Some(SdkValue::List(items)) => (items.iter())
            .filter_map(|item| match item {
                SdkValue::Text(id) => Some(id.clone()),
                _ => None,
            })
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    let path = answer.get_text("path").unwrap_or(path).to_owned();
    Ok((!ids.is_empty()).then_some((path, ids)))
}
