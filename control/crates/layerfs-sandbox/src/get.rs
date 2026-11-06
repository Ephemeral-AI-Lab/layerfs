//! `sandbox.get`: a Sandbox's record with one page of its Workspaces.
use super::direct;
use super::family::{page, sandbox_of};
use crate::route::reach;
use layerfs_bridge::control::SdkValue;
use layerfs_home::events::Events;
use layerfs_home::home;
use layerfs_home::outcome::{Outcome, View};
use layerfs_machine::worker::Unreached;
use std::path::Path;

/// `sandbox.get(name, { page_size, page_pos })`: its record with its
/// derived `state`, its `op` when it has one (H6), and one page of its
/// Workspaces. Reading never clears an intent.
pub fn get(home: &Path, events: &mut Events<'_>, args: &SdkValue) -> Outcome {
    let home = home.to_path_buf();
    let name = args.get_text("name").unwrap_or("");
    let record = sandbox_of(&home, name)?;
    // a Sandbox with no id has no daemon to ask; a get starts no machine
    // (P4): a machine that is down answers the record with no Workspaces
    // and why
    let listed = match record.id() {
        // one reached directly lists its daemon's Workspaces from here (#535 G4)
        Some(_) if direct::forwarded(&record) => direct::list(&home, &record),
        Some(_) => reach(
            &home,
            events,
            &record,
            "sandbox.list_workspaces",
            vec![],
            false,
            crate::dispatch::call,
        ),
        None => Ok(Ok(SdkValue::Null)),
    };
    let (all, down, gone) = match listed {
        Ok(Err(refusal)) if refusal.code == "NoSuchSandbox" => (Vec::new(), None, true),
        Ok(workspaces) => match workspaces?.get("workspaces") {
            Some(SdkValue::List(items)) => (
                items.iter().map(|i| with_ws(name, i)).collect(),
                None,
                false,
            ),
            _ => (Vec::new(), None, false),
        },
        Err(Unreached(why)) => (Vec::new(), Some(why.message), false),
    };
    let (items, next) = page(all, args);
    let shown = record.to_sdk(View::Full);
    let shown = home::with_field(shown, "state", SdkValue::text(record.state(gone)));
    let mut record = home::with_field(shown, "workspaces", SdkValue::List(items));
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
