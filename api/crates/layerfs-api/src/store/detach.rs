//! `store.detach`: stop a machine running Sandboxes of a PostgreSQL Store.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::{MachineName, StoreName};
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "detach",
    &[a("name", Name), a("machine", Name)],
    &[],
    "stop a machine running Sandboxes of a PostgreSQL Store",
    run,
);

/// Runs `store.detach`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let name = StoreName::parse(args.get_text("name").unwrap_or(""))?;
    let machine = MachineName::parse(args.get_text("machine").unwrap_or(""))?;
    layerfs_control::store::detach(&context.home, &name, &machine)
}
