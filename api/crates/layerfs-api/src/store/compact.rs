//! `store.compact`: reclaim dead bytes from one sqlite Store.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::StoreName;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "compact",
    &[a("name", Name)],
    &[],
    "reclaim dead bytes from one sqlite Store",
    run,
);

/// Runs `store.compact`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let name = StoreName::parse(args.get_text("name").unwrap_or(""))?;
    let (home, events) = context.parts();
    layerfs_control::store::compact(home, events, &name)
}
