//! `store.storage_report`: bounded canonical sharing among explicit immutable
//! saved snapshots of one Store's projects.
use crate::calls::{
    table::{
        a, call, opt, Call,
        Kind::{Bool, Int, Json, Name, Size},
    },
    Context,
};
use layerfs_bridge::control::SdkValue;
use layerfs_home::{names::StoreName, outcome::Outcome};
use layerfs_store::storage_report::{storage_report, StorageReport};

pub(crate) const CALL: Call = call(
    "store",
    "storage_report",
    &[a("name", Name), a("snapshots", Json)],
    &[
        opt("max_objects", Int),
        opt("max_canonical_bytes", Size),
        opt("include_encoding", Bool),
    ],
    "bounded canonical sharing among explicit immutable saved snapshots",
    run,
);

fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let name = StoreName::parse(args.get_text("name").unwrap_or(""))?;
    let request = StorageReport {
        name: name.to_string(),
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
