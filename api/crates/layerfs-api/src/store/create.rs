//! `store.create`: an empty sqlite Store on native.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::StoreName;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "create",
    &[a("name", Name)],
    &[],
    "an empty sqlite Store on native",
    run,
);

/// Runs `store.create`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let name = StoreName::parse(args.get_text("name").unwrap_or(""))?;
    layerfs_store::sqlite::family::create(&context.home, &name)
}
