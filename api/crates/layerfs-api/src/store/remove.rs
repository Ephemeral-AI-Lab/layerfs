//! `store.remove`: delete a sqlite Store with every project in it.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call};
use crate::calls::{Context, Form};
use layerfs_protocol::provision::install::SdkValue;
use layerfs_home::names::StoreName;
use layerfs_home::outcome::Outcome;

/// Its answer: `ok` alone, or `left` when only its records went (its
/// record was not read, or its machine's record has no 2.0 name): each
/// sentence about what may remain.
const FORM: Form = Form::of(&[], &["ok", "left"], &["ok", "outcome", "store", "left"]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "remove",
    &[a("store", Name)],
    &[],
    "delete a sqlite Store with every project in it",
    run,
)
.form(&FORM);

/// Runs `store.remove`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let store = StoreName::parse(args.get_text("store").unwrap_or(""))?;
    layerfs_store::admin::with_users::remove(&context.home, &store)
}
