//! `sandbox.list`: every Sandbox in the home, or one project's.
use crate::calls::table::Kind::{Bool, Int, Project};
use crate::calls::table::{call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::with_field;
use layerfs_home::outcome::{Outcome, View};
use layerfs_sandbox::family::{list, ListRequest, Page};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "sandbox",
    "list",
    &[opt("project", Project)],
    &[
        opt("page_size", Int),
        opt("page_pos", Int),
        opt("live", Bool),
    ],
    "every Sandbox in the home, or one project's",
    run,
);

/// Runs `sandbox.list`: `{ items, next_page_pos }`, each item a record
/// with its state. With `live`, each item also says whether its daemon
/// process runs on its machine now (`live`: true, false, or null with
/// `live_reason` when its machine did not say; layerfs-project#164);
/// `state` is the record's either way. Without it the answer is the
/// records alone.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = ListRequest {
        project: args.get_text("project").map(str::to_owned),
        page: Page {
            size: args.get_int("page_size"),
            pos: args.get_int("page_pos"),
        },
        live: args.get("live") == Some(&SdkValue::Bool(true)),
    };
    let listed = list(&context.home, &request)?;
    #[cfg(unix)]
    let mut live = listed.live.map(Vec::into_iter);
    // Windows: no live face (the monitoring port is the #179 follow-up).
    #[cfg(windows)]
    let mut live: Option<std::vec::IntoIter<layerfs_sandbox::inspect::Live>> = None;
    let items = (listed.items.iter()).map(|record| {
        let item = record.to_sdk(View::List);
        let Some(found) = live.as_mut().and_then(Iterator::next) else {
            return item;
        };
        let known = found.live.map_or(SdkValue::Null, SdkValue::Bool);
        let item = with_field(item, "live", known);
        match found.reason {
            Some(reason) => with_field(item, "live_reason", SdkValue::Text(reason)),
            None => item,
        }
    });
    Ok(SdkValue::map([
        ("items", SdkValue::List(items.collect())),
        ("next_page_pos", next_page_pos(listed.next)),
    ]))
}

/// A next page's position, or null after the last.
pub(super) fn next_page_pos(next: Option<usize>) -> SdkValue {
    next.map_or(SdkValue::Null, |pos| SdkValue::Int(pos as i64))
}
