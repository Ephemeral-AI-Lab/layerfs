//! `backup.connect`: connect an r2 or MinIO bucket.
use crate::calls::table::Kind::{Name, Size, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::BucketName;
use layerfs_home::outcome::Outcome;
use layerfs_store::backup::family::connect;
use layerfs_store::backup::record::BucketConnect;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "backup",
    "connect",
    &[a("name", Name), a("url", Text)],
    &[
        opt("key", Text),
        opt("region", Text),
        opt("part_size", Size),
    ],
    "connect an r2 or MinIO bucket",
    run,
);

/// Runs `backup.connect`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = BucketConnect {
        name: BucketName::parse(args.get_text("name").unwrap_or(""))?,
        url: text("url").unwrap_or_default(),
        key: text("key"),
        region: text("region"),
        part_size: args.get_int("part_size"),
    };
    connect(&context.home, &request)
}
