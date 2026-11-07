//! `sandbox.get`: a Sandbox and its Workspaces.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call, PAGE};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::with_field;
use layerfs_home::outcome::{Outcome, View};
use layerfs_sandbox::family::{get, GetRequest, Page};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "sandbox",
    "get",
    &[a("name", Name)],
    &PAGE,
    "a Sandbox and its Workspaces",
    run,
);

/// Runs `sandbox.get`: its record with its derived `state`, one page of
/// its `workspaces` (each with its `path`, `name` and `ws`), why its
/// machine did not answer (`down_reason`) and `next_page_pos`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = GetRequest {
        name: args.get_text("name").unwrap_or("").to_owned(),
        page: Page {
            size: args.get_int("page_size"),
            pos: args.get_int("page_pos"),
        },
    };
    let (home, events) = context.parts();
    let got = get(home, events, &request)?;
    let workspaces = got.workspaces.into_iter().map(|listed| {
        let item = with_field(listed.item, "path", SdkValue::Text(listed.path));
        let item = with_field(item, "name", SdkValue::opt_text(listed.name));
        with_field(item, "ws", SdkValue::Text(listed.ws))
    });
    let shown = got.record.to_sdk(View::Full);
    let shown = with_field(shown, "state", SdkValue::text(got.state));
    let mut shown = with_field(shown, "workspaces", SdkValue::List(workspaces.collect()));
    if let Some(why) = got.down_reason {
        shown = with_field(shown, "down_reason", SdkValue::Text(why));
    }
    let next = super::list::next_page_pos(got.next);
    Ok(with_field(shown, "next_page_pos", next))
}
