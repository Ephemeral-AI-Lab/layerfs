//! `store.detach`: a machine no longer runs Sandboxes of a PostgreSQL
//! Store; its reach is forgotten.
use crate::dispatch::table::Kind::Name;
use crate::dispatch::table::{a, call, opt, Call};
use crate::dispatch::{Context, Form};
use layerfs_home::names::{MachineName, StoreName};
use layerfs_home::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;

/// Its answer: `ok`.
const FORM: Form = Form::of(&["ok"], &[], &["ok", "outcome", "store", "machine"]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "detach",
    &[a("store", Name)],
    &[opt("machine", Name).required()],
    "stop a machine running Sandboxes of a PostgreSQL Store; refused while one runs there",
    run,
)
.form(&FORM);

/// Runs `store.detach`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let store = StoreName::parse(args.get_text("store").unwrap_or(""))?;
    let machine = MachineName::parse(args.get_text("machine").unwrap_or(""))?;
    layerfs_store::admin::with_users::detach(
        &context.home,
        &store,
        &machine,
        &layerfs_sandbox::users::STORE,
    )
}
