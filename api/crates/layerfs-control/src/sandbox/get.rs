//! `sandbox.get`: a Sandbox's record with one page of its Workspaces.
use super::family::{page, sandbox_of};
use crate::common::context::Context;
use crate::common::home;
use crate::common::outcome::{Outcome, View};
use crate::common::place::Start;
use crate::machine::worker::Unreached;
use crate::sandbox::route::reach;
use layerfs_bridge::control::SdkValue;

/// `sandbox.get(name, { page_size, page_pos })`: its record with one page
/// of its Workspaces.
pub(super) fn get(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    let name = args.get_text("name").unwrap_or("");
    let record = sandbox_of(&home, name)?;
    // a get starts no machine (P4): a machine that is down answers
    // the record with no Workspaces and why
    let listed = reach(
        context,
        &record,
        "sandbox.list_workspaces",
        vec![],
        Start::Never,
    );
    let (all, down) = match listed {
        Ok(workspaces) => match workspaces?.get("workspaces") {
            Some(SdkValue::List(items)) => (items.clone(), None),
            _ => (Vec::new(), None),
        },
        Err(Unreached(why)) => (Vec::new(), Some(why.message)),
    };
    let (items, next) = page(all, args);
    let shown = record.to_sdk(View::Full);
    let mut record = home::with_field(shown, "workspaces", SdkValue::List(items));
    if let Some(why) = down {
        record = home::with_field(record, "down_reason", SdkValue::Text(why));
    }
    Ok(home::with_field(record, "next_page_pos", next))
}
