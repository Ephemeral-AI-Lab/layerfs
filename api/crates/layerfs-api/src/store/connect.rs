//! `store.connect`: attach a PostgreSQL database as a Store.
use crate::calls::table::Kind::{Name, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
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
    layerfs_store::postgresql::connect::connect(&context.home, args)
}
