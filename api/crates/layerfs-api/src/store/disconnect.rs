//! `store.disconnect`: forget a PostgreSQL Store; its data stays.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::Name;
use layerfs_control::calls::table::{a, call, Call};
use layerfs_control::calls::Context;
use layerfs_control::common::outcome::Outcome;

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
    layerfs_control::store::connect::run(context, "disconnect", args)
}
