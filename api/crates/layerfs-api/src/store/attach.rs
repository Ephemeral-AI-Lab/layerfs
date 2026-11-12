//! `store.attach`: let a machine run Sandboxes of a PostgreSQL Store, after
//! checking from that machine that it reaches the Store's database.
use crate::calls::table::Kind::{Name, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_control::store::StoreAttach;
use layerfs_home::names::{MachineName, StoreName};
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "attach",
    &[a("name", Name), a("machine", Name)],
    &[opt("url", Text), opt("password", Text)],
    "let a machine run Sandboxes of a PostgreSQL Store, after checking it reaches the server",
    run,
);

/// Runs `store.attach`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = StoreAttach {
        name: StoreName::parse(args.get_text("name").unwrap_or(""))?,
        machine: MachineName::parse(args.get_text("machine").unwrap_or(""))?,
        url: text("url"),
        password: text("password"),
    };
    let (home, events) = context.parts();
    layerfs_control::store::attach(home, events, &request)
}
