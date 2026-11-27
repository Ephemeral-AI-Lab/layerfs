//! `backup.create`: back up a sqlite Store to a bucket.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, opt, Call, PROGRESS};
use crate::calls::{Context, Form, Policy};
use layerfs_protocol::provision::install::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::backup::family::{create, BackupCreate};

/// Its answer: the file and its size.
const FORM: Form = Form::of(
    &["file", "size"],
    &[],
    &[
        "file",
        "size",
        "outcome",
        "store",
        "bucket",
        "object",
        "sha256",
        "heads_sha256",
    ],
);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "backup",
    "create",
    &[a("store", Name), a("bucket", Name)],
    &[opt("file", Name), PROGRESS],
    "back up a sqlite Store to a bucket",
    run,
)
.policy(Policy::Store("store"))
.form(&FORM);

/// Runs `backup.create`: `{ phase, done, total }` events, phases `check`,
/// `snapshot`, `send`, `hash` and `upload`.
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
