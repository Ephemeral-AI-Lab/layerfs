//! `store.connect`: attach a PostgreSQL database as a Store.
use crate::calls::table::Kind::{Name, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::StoreName;
use layerfs_home::outcome::Outcome;
use layerfs_store::postgresql::connect::{connect, StoreConnect};

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
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = StoreConnect {
        name: StoreName::parse(args.get_text("name").unwrap_or(""))?,
        url: text("url").unwrap_or_default(),
        password: text("password"),
        synchronous_commit: text("synchronous_commit"),
    };
    connect(&context.home, &request)
}
