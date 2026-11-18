//! `store.remove`: delete a sqlite Store with every project in it.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call};
use crate::calls::{Context, Form};
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::StoreName;
use layerfs_home::outcome::Outcome;

/// Its answer: nothing to say in the short form.
const FORM: Form = Form::of(&["ok"], &[], &["ok", "outcome", "store"]);

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
    layerfs_control::store::remove(&context.home, &store)
}
