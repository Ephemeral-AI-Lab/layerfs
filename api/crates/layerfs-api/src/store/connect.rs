//! `store.connect`: attach a PostgreSQL database as a Store.
use layerfs_bridge::control::SdkValue;
use layerfs_control::calls::table::Kind::{Name, Text};
use layerfs_control::calls::table::{a, call, opt, Call};
use layerfs_control::calls::Context;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "connect",
    &[a("name", Name), a("url", Text)],
    &[opt("password", Text), opt("synchronous_commit", Text)],
    "attach a PostgreSQL database as a Store",
    run,
);

/// Runs `store.connect`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_control::store::connect::run(context, "connect", args)
}
