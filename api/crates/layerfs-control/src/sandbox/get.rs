//! `sandbox.get`: a Sandbox's record with one page of its Workspaces.
use super::direct;
use super::family::{page, sandbox_of};
use crate::common::context::Context;
use crate::common::home;
use crate::common::outcome::{Outcome, View};
use crate::machine::worker::Unreached;
use crate::sandbox::route::reach;
use layerfs_bridge::control::SdkValue;

/// `sandbox.get(name, { page_size, page_pos })`: its record with its
/// derived `state`, its `op` when it has one (H6), and one page of its
/// Workspaces. Reading never clears an intent.
pub(super) fn get(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    let name = args.get_text("name").unwrap_or("");
    let record = sandbox_of(&home, name)?;
    // a Sandbox with no id has no daemon to ask; a get starts no machine
    // (P4): a machine that is down answers the record with no Workspaces
    // and why
    let listed = match record.id() {
        // one reached directly lists its daemon's Workspaces from here (#535 G4)
        Some(_) if direct::forwarded(&record) => direct::list(context, &record),
        Some(_) => reach(context, &record, "sandbox.list_workspaces", vec![], false),
        None => Ok(Ok(SdkValue::Null)),
    };
    let (all, down, gone, running, recently_closed) = match listed {
        Ok(Err(refusal)) if refusal.code == "NoSuchSandbox" => {
            (Vec::new(), None, true, SdkValue::List(Vec::new()), SdkValue::List(Vec::new()))
        }
        Ok(workspaces) => {
            let workspaces = workspaces?;
            let running = workspaces
                .get("running")
                .cloned()
                .unwrap_or_else(|| SdkValue::List(Vec::new()));
            let recently_closed = workspaces
                .get("recently_closed")
                .cloned()
                .unwrap_or_else(|| SdkValue::List(Vec::new()));
            match workspaces.get("workspaces") {
                Some(SdkValue::List(items)) => (
                    items.iter().map(|i| with_ws(name, i)).collect(),
                    None,
                    false,
                    running,
                    recently_closed,
                ),
                _ => (Vec::new(), None, false, running, recently_closed),
            }
        }
        Err(Unreached(why)) => (
            Vec::new(),
            Some(why.message),
            false,
            SdkValue::List(Vec::new()),
            SdkValue::List(Vec::new()),
        ),
    };
    let (items, next) = page(all, args);
    let shown = record.to_sdk(View::Full);
    // what this process measures for the Sandbox, when it owns it (#551
    // B8-157); otherwise the record's bounds only
    let directory = crate::common::home::paths::sandbox_dir(&home, name);
    let measured = crate::sandbox::registry::live_at(&directory)
        .map(|state| state.controls.measured());
    let shown = home::with_field(shown, "capacity", record.capacity_sdk(measured));
    let shown = home::with_field(shown, "state", SdkValue::text(record.state(gone)));
    let mut record = home::with_field(shown, "workspaces", SdkValue::List(items));
    // the command running now (#551 B8-158): the first recorded entry, and
    // the full bounded list beside it
    let first = match &running {
        SdkValue::List(items) => items.first().cloned().unwrap_or(SdkValue::Null),
        _ => SdkValue::Null,
    };
    record = home::with_field(record, "running_command", first);
    record = home::with_field(record, "running", running);
    record = home::with_field(record, "recently_closed", recently_closed);
    if let Some(why) = down {
        record = home::with_field(record, "down_reason", SdkValue::Text(why));
    }
    Ok(home::with_field(record, "next_page_pos", next))
}

/// Workspace `item` of Sandbox `sandbox` with its `ws` (#536 C3):
/// `<sandbox>:<name>` when it has a name, else `<sandbox>:<path>`; and its
/// `name` and `path` (null and the daemon's `directory` from a worker that
/// does not give them).
fn with_ws(sandbox: &str, item: &SdkValue) -> SdkValue {
    let path = (item.get_text("path"))
        .or_else(|| item.get_text("directory"))
        .unwrap_or("");
    let name = item.get_text("name").filter(|name| !name.is_empty());
    let ws = format!("{sandbox}:{}", name.unwrap_or(path));
    let item = home::with_field(item.clone(), "path", SdkValue::text(path));
    let item = home::with_field(item, "name", SdkValue::opt_text(name.map(str::to_owned)));
    home::with_field(item, "ws", SdkValue::Text(ws))
}
