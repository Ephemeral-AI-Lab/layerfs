//! `sandbox.list`: every Sandbox in the home, or one project's.
use crate::calls::table::Kind::Project;
use crate::calls::table::{call, opt, Call, PAGE};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::{Outcome, View};
use layerfs_sandbox::family::{list, ListRequest, Page};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "sandbox",
    "list",
    &[opt("project", Project)],
    &PAGE,
    "every Sandbox in the home, or one project's",
    run,
);

/// Runs `sandbox.list`: `{ items, next_page_pos }`, each item a record
/// with its state.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = ListRequest {
        project: args.get_text("project").map(str::to_owned),
        page: Page {
            size: args.get_int("page_size"),
            pos: args.get_int("page_pos"),
        },
    };
    let listed = list(&context.home, &request)?;
    let items = (listed.items.iter()).map(|record| record.to_sdk(View::List));
    Ok(SdkValue::map([
        ("items", SdkValue::List(items.collect())),
        ("next_page_pos", next_page_pos(listed.next)),
    ]))
}

/// A next page's position, or null after the last.
pub(super) fn next_page_pos(next: Option<usize>) -> SdkValue {
    next.map_or(SdkValue::Null, |pos| SdkValue::Int(pos as i64))
}
