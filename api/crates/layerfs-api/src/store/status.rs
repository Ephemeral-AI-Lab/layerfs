//! `store.status`: a Store's live state, and on request the check of one
//! machine's connection to it.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::{MachineName, StoreName};
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "status",
    &[a("name", Name)],
    &[opt("machine", Name)],
    "a Store's live state; with a machine, whether it reaches a PostgreSQL Store",
    run,
);

/// Runs `store.status`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let name = StoreName::parse(args.get_text("name").unwrap_or(""))?;
    let machine = args
        .get_text("machine")
        .map(MachineName::parse)
        .transpose()?;
    let (home, events) = context.parts();
    layerfs_control::store::status(home, events, &name, machine.as_ref())
}
