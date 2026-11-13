//! `logs.list`: the calls made against this home, newest first.
use crate::calls::table::Kind::{Int, Name, Project, Text};
use crate::calls::table::{call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::logs::Filter;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "logs",
    "list",
    &[],
    &[
        opt("op", Text),
        opt("project", Project),
        opt("client", Name),
        opt("tag", Text),
        opt("outcome", Text),
        opt("since", Int),
        opt("cursor", Text),
        opt("limit", Int),
    ],
    "the calls made against this home, newest first",
    run,
);

/// Runs `logs.list`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let filter = Filter {
        op: text("op"),
        project: text("project"),
        client: text("client"),
        tag: text("tag"),
        outcome: text("outcome"),
        since: args.get_int("since"),
    };
    layerfs_home::logs::read(
        &context.home,
        &filter,
        args.get_text("cursor"),
        args.get_int("limit"),
    )
}
