//! `branch.list`: a project's Branches.
use crate::calls::scope::NAMESPACE;
use crate::calls::table::Kind::Project;
use crate::calls::table::{a, call, opt, Call, Kind::Int};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::branch::{list, BranchList};
use layerfs_store::calls::Page;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "branch",
    "list",
    &[a("project", Project)],
    &[
        opt("page_size", Int),
        opt("page_pos", Int),
        opt("page", Int),
    ],
    "a project's Branches",
    run,
);

/// Runs `branch.list`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = BranchList {
        project: text("project").unwrap_or_default(),
        namespace: text(NAMESPACE),
        page: Page {
            size: args.get_int("page_size"),
            pos: args.get_int("page_pos"),
            number: args.get_int("page"),
        },
    };
    let (home, events) = context.parts();
    list(home, events, &request)
}
