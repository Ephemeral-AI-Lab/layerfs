//! `store.disconnect`: forget a PostgreSQL Store, or one machine's own
//! address; its data stays.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, opt, Call};
use crate::calls::{Context, Form};
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::{MachineName, StoreName};
use layerfs_home::outcome::Outcome;

/// Its answer: `ok`.
const FORM: Form = Form::of(&["ok"], &[], &["ok", "outcome", "store", "machine"]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "disconnect",
    &[a("store", Name)],
    &[opt("machine", Name)],
    "forget a PostgreSQL Store, or one machine's own address and reach; its data stays",
    run,
)
.form(&FORM);

/// Runs `store.disconnect`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let store = StoreName::parse(args.get_text("store").unwrap_or(""))?;
    let machine = args
        .get_text("machine")
        .map(MachineName::parse)
        .transpose()?;
    layerfs_control::store::disconnect(&context.home, &store, machine.as_ref())
}
