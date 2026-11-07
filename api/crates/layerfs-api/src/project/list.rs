//! `project.list`: every project in the home, or one Store's.
use crate::calls::table::Kind::Name;
use crate::calls::table::{call, opt, Call, PAGE};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::StoreName;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::Page;
use layerfs_store::project::family::{list, ProjectList};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "project",
    "list",
    &[opt("store", Name)],
    &PAGE,
    "every project in the home, or one Store's",
    run,
);

/// Runs `project.list`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = ProjectList {
        store: args.get_text("store").map(StoreName::parse).transpose()?,
        page: Page {
            size: args.get_int("page_size"),
            pos: args.get_int("page_pos"),
        },
    };
    list(&context.home, &request)
}
