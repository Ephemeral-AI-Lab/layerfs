//! `store.update`: change a PostgreSQL Store's connection, or the one a
//! machine alone dials.
use crate::calls::table::Kind::{Bool, Name, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_control::store::StoreUpdate;
use layerfs_home::names::{MachineName, StoreName};
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "update",
    &[a("name", Name)],
    &[
        opt("machine", Name),
        opt("url", Text).redact(),
        opt("password", Text).redact(),
        opt("synchronous_commit", Text),
        opt("clear", Bool),
    ],
    "change a PostgreSQL Store's connection, or the one a machine alone dials",
    run,
);

/// Runs `store.update`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = StoreUpdate {
        name: StoreName::parse(args.get_text("name").unwrap_or(""))?,
        machine: args
            .get_text("machine")
            .map(MachineName::parse)
            .transpose()?,
        url: text("url"),
        password: text("password"),
        synchronous_commit: text("synchronous_commit"),
        clear: args.get_bool("clear"),
    };
    let (home, events) = context.parts();
    layerfs_control::store::update(home, events, &request)
}
