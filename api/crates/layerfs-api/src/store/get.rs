//! `store.get`: a Store's record.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "get",
    &[a("name", Name)],
    &[],
    "a Store's record",
    run,
);

/// Runs `store.get`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_store::sqlite::family::get(&context.home, args)
}
