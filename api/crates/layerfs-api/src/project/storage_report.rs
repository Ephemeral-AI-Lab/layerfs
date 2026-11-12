//! `project.storage_report`: bounded canonical reuse among one project's
//! explicit immutable saved snapshots.
use crate::calls::{
    table::{
        a, call, opt, Call,
        Kind::{Bool, Int, Json, Project, Size},
    },
    Context,
};
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::project::{storage_report, ProjectStorageReport};

pub(crate) const CALL: Call = call(
    "project",
    "storage_report",
    &[a("project", Project), a("snapshots", Json)],
    &[
        opt("max_objects", Int),
        opt("max_canonical_bytes", Size),
        opt("include_encoding", Bool),
    ],
    "bounded canonical reuse among one project's explicit immutable saved snapshots",
    run,
);

fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = ProjectStorageReport {
        project: args.get_text("project").unwrap_or_default().to_owned(),
        snapshots: args.get("snapshots").cloned().unwrap_or_default(),
        max_objects: args.get_int("max_objects"),
        max_canonical_bytes: args.get_int("max_canonical_bytes"),
        include_encoding: args
            .get("include_encoding")
            .map(|_| args.get_bool("include_encoding")),
    };
    let (home, events) = context.parts();
    storage_report(home, events, &request)
}
