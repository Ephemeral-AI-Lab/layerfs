//! `store.upgrade`: bring an older-format Store to the latest format, in
//! place and in one transaction (#804). It runs where the Store lives, as
//! every Store call does; the `layerfs` command turns its first SIGINT or
//! SIGTERM into the process-wide cancel request, read until `commit`.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call, PROGRESS};
use crate::calls::{Context, Form};
use layerfs_protocol::provision::install::SdkValue;
use layerfs_home::names::StoreName;
use layerfs_home::outcome::Outcome;

/// Its answer: the outcome (`upgraded` or `current`), and in the long
/// form the formats before and after, the steps applied and what went
/// wrong after COMMIT.
const FORM: Form = Form::of(
    &["outcome"],
    &[],
    &[
        "outcome",
        "store",
        "engine",
        "format_before",
        "format_after",
        "steps",
        "warnings",
    ],
);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "upgrade",
    &[a("store", Name)],
    &[PROGRESS],
    "bring an older Store to the latest format, in place",
    run,
)
.form(&FORM);

/// Runs `store.upgrade`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let store = StoreName::parse(args.get_text("store").unwrap_or(""))?;
    let (home, events) = context.parts();
    layerfs_store::admin::with_users::upgrade(home, events, &store)
}
