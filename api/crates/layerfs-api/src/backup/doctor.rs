//! `backup.doctor`: one bucket's checks (its record, key, address and one
//! listing page); `--full` also writes one small object, reads it back and
//! deletes it.
use crate::calls::table::Kind::{Bool, Name};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use crate::doctor::answer::{self, FORM};
use layerfs_bridge::control::SdkValue;
use layerfs_control::doctor::{Kind, Options};
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "backup",
    "doctor",
    &[a("bucket", Name)],
    &[opt("full", Bool)],
    "check a bucket; --full also tests a write",
    run,
)
.unopened()
.form(&FORM);

fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let bucket = args.get_text("bucket").unwrap_or("");
    let options = Options {
        full: args.get("full") == Some(&SdkValue::Bool(true)),
        ..Options::default()
    };
    answer::run(context, Kind::Bucket, bucket, options)
}
