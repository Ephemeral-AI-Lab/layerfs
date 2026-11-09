//! `project.stats`: counts of one pinned saved tree.
use crate::calls::{
    table::{
        a, call, opt, Call,
        Kind::{Name, Project},
    },
    Context,
};
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::project::{stats, ProjectStats};

pub(crate) const CALL: Call = call(
    "project",
    "stats",
    &[
        a("project", Project),
        opt("branch", Name),
        opt("commit", Name),
    ],
    &[],
    "entry counts of one pinned saved tree",
    run,
);

fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = ProjectStats {
        project: args.get_text("project").unwrap_or("").to_owned(),
        branch: args.get_text("branch").unwrap_or("main").to_owned(),
        commit: args.get_text("commit").map(str::to_owned),
    };
    let (home, events) = context.parts();
    stats(home, events, &request)
}
