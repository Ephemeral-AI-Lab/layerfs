//! The `workspace` family: each call goes to its Sandbox's machine.
use crate::common::context::Context;
use crate::common::home;
use crate::common::names::{WorkspaceAt, WorkspaceName, WorkspaceRef};
use crate::common::outcome::{refused, Outcome};
use crate::machine::worker::Unreached;
use crate::sandbox::family::sandbox_of;
use crate::sandbox::route::{on_machine, reach};
use layerfs_bridge::control::SdkValue;

/// Runs one `workspace` call.
pub fn workspace(context: &mut Context<'_>, call: &str, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    if call == "mount" {
        let name = args.get_text("sandbox").unwrap_or("");
        let record = sandbox_of(&home, name)?;
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
        let mounted = on_machine(context, &record, "workspace.mount", more)?;
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
    // a path, or a name the Sandbox's side resolves to its path
    let mut more = vec![("path", SdkValue::text(ws.at.as_str()))];
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
            for flag in ["discard", "force"] {
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
        "status" => {
            reach(context, &record, op, more, false).unwrap_or_else(|Unreached(why)| Err(why))
        }
        _ => on_machine(context, &record, op, more),
    }?;
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
