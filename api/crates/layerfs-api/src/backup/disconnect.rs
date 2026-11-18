//! `backup.disconnect`: forget a bucket and its key; its files stay.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call};
use crate::calls::{Context, Form, Part};
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::BucketName;
use layerfs_home::outcome::Outcome;

/// Its answer: `ok`, or the Stores whose link to it was removed.
const FORM: Form = Form::of(
    &[],
    &["ok", "unlinked", "unlinked_count"],
    &["ok", "unlinked", "unlinked_count", "outcome", "bucket"],
)
.parts(&[("unlinked", Part::Scalars("store"))]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "backup",
    "disconnect",
    &[a("bucket", Name)],
    &[],
    "forget a bucket; its files stay",
    run,
)
.form(&FORM);

/// Runs `backup.disconnect`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let bucket = BucketName::parse(args.get_text("bucket").unwrap_or(""))?;
    layerfs_store::backup::family::disconnect(&context.home, &bucket)
}
