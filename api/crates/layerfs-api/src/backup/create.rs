//! `backup.create`: upload a sqlite Store to a bucket.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, opt, Call, PROGRESS};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::backup::family::{create, BackupCreate};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "backup",
    "create",
    &[a("store", Name), a("bucket", Name), opt("file", Name)],
    &[PROGRESS],
    "upload a sqlite Store to a bucket",
    run,
);

/// Runs `backup.create`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = BackupCreate {
        store: text("store").unwrap_or_default(),
        bucket: text("bucket").unwrap_or_default(),
        file: text("file"),
    };
    let (home, events) = context.parts();
    create(home, events, &request)
}
