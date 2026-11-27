//! `store.disconnect`: forget a PostgreSQL Store, its saved login and its
//! attachments; its data stays.
use crate::dispatch::table::Kind::Name;
use crate::dispatch::table::{a, call, Call};
use crate::dispatch::{Context, Form};
use layerfs_home::names::StoreName;
use layerfs_home::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;

/// Its answer: `ok`.
const FORM: Form = Form::of(&["ok"], &[], &["ok", "outcome", "store"]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "disconnect",
    &[a("store", Name)],
    &[],
    "forget a PostgreSQL Store, its saved login and its attachments; its data stays",
    run,
)
.form(&FORM);

/// Runs `store.disconnect`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let store = StoreName::parse(args.get_text("store").unwrap_or(""))?;
    layerfs_store::admin::with_users::disconnect(
        &context.home,
        &store,
        &layerfs_sandbox::users::STORE,
    )
}
