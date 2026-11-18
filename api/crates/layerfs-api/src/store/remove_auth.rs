//! `store.remove_auth`: remove a saved PostgreSQL login no Store uses.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call};
use crate::calls::{Context, Form};
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its answer: `ok`.
const FORM: Form = Form::of(&["ok"], &[], &["ok", "outcome", "auth"]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "remove_auth",
    &[a("auth", Name)],
    &[],
    "remove a saved PostgreSQL login no Store uses",
    run,
)
.form(&FORM);

/// Runs `store.remove_auth`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    layerfs_store::postgresql::login::remove(&context.home, args.get_text("auth").unwrap_or(""))
}
