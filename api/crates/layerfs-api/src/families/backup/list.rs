//! `backup.list`: the Store files in the connected buckets, with the Store
//! each is linked to.
use crate::dispatch::table::Kind::Name;
use crate::dispatch::table::{call, opt, Call};
use crate::dispatch::{Context, Form, Part};
use layerfs_home::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;

/// One Store file: its bucket and name, and what is news about it.
const ITEM: Form = Form::of(
    &["bucket", "file"],
    &["size", "store", "behind", "changed", "missing"],
    &[
        "bucket",
        "file",
        "size",
        "store",
        "behind",
        "changed",
        "missing",
        "message",
        "modified_at",
        "sha256",
        "heads_sha256",
    ],
);
/// A bucket that could not be listed, with the call that connects it
/// again when its key was missing or refused.
const NOT_LISTED: Form = Form::of(
    &["bucket"],
    &["blocked_by", "reason", "message", "fix"],
    &["bucket", "blocked_by", "reason", "message", "fix"],
);
const FORM: Form = Form::of(&["items"], &["not_listed"], &["items", "not_listed"]).parts(&[
    ("items", Part::Rows(&ITEM)),
    ("not_listed", Part::Rows(&NOT_LISTED)),
]);

/// Its table entry: unpaged, a row per Store file under each bucket's
/// prefix.
pub(crate) const CALL: Call = call(
    "backup",
    "list",
    &[],
    &[opt("bucket", Name)],
    "the Store files in the connected buckets, with the Store each is linked to",
    run,
)
.form(&FORM);

/// Runs `backup.list`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_store::backup::family::list(&context.home, args.get_text("bucket"))
}
