//! `store.disconnect`: forget a PostgreSQL Store; its data stays.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::StoreName;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "disconnect",
    &[a("name", Name)],
    &[],
    "forget a PostgreSQL Store; its data stays",
    run,
);

/// Runs `store.disconnect`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let name = StoreName::parse(args.get_text("name").unwrap_or(""))?;
    layerfs_control::store::disconnect(&context.home, &name)
}
