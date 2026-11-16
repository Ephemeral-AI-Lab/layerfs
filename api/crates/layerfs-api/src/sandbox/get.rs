//! `sandbox.get`: a Sandbox and its Workspaces.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call, PAGE};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::with_field;
use layerfs_home::outcome::{Outcome, View};
use layerfs_sandbox::family::{get, GetRequest, Got, Page};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "sandbox",
    "get",
    &[a("name", Name)],
    &PAGE,
    "a Sandbox and its Workspaces",
    run,
);

/// Runs `sandbox.get`: its record with its derived `state`, one page of its
/// `workspaces` (each with its `path`, `name` and `ws`, and, from the side
/// that keeps its record, the commands `running` in it and its
/// `overlay_bytes`, layerfs-project#178), why its machine did
/// not answer (`down_reason`, with the partial `down` counts when the
/// daemon's capacity notice carried them, #649), the daemon's
/// `daemon_capacity` read model when the Sandbox answered it (#540), its
/// `workspace_capacity` (how many Workspaces a Sandbox holds, always), its
/// daemon process as its machine saw it (`daemon`: `{ pid, started_at }`,
/// `usage`: its cgroup's `{ cpu_usec, memory_current, memory_peak, pids }`
/// or null with `usage_reason`; layerfs-project#164) and
/// `next_page_pos`. The library reads it best effort and bounded, so this
/// layer only shows what it got.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = GetRequest {
        name: args.get_text("name").unwrap_or("").to_owned(),
        page: Page {
            size: args.get_int("page_size"),
            pos: args.get_int("page_pos"),
        },
    };
    let (home, events) = context.parts();
    let Got {
        record,
        state,
        workspaces,
        down_reason,
        down,
        next,
        daemon_capacity,
        workspace_capacity,
        process,
    } = get(home, events, &request)?;
    let workspaces = workspaces.into_iter().map(|listed| {
        let item = with_field(listed.item, "path", SdkValue::Text(listed.path));
        let item = with_field(item, "name", SdkValue::opt_text(listed.name));
        with_field(item, "ws", SdkValue::Text(listed.ws))
    });
    let shown = record.to_sdk(View::Full);
    let shown = with_field(shown, "state", SdkValue::text(state));
    let mut shown = with_field(shown, "workspaces", SdkValue::List(workspaces.collect()));
    if let Some(why) = down_reason {
        shown = with_field(shown, "down_reason", SdkValue::Text(why));
    }
    if let Some(down) = down {
        shown = with_field(shown, "down", down.to_sdk());
    }
    if let Some(model) = daemon_capacity {
        shown = with_field(shown, "daemon_capacity", model.to_sdk());
    }
    let capacity = SdkValue::Int(i64::from(workspace_capacity));
    let shown = with_field(shown, "workspace_capacity", capacity);
    let shown = (process.into_iter()).fold(shown, |shown, (field, value)| {
        with_field(shown, &field, value)
    });
    let next = super::list::next_page_pos(next);
    Ok(with_field(shown, "next_page_pos", next))
}
