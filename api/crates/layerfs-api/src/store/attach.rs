//! `store.attach`: let a machine run Sandboxes of a PostgreSQL Store,
//! dialling the URL and login this computer saved; checked from it first.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, opt, Call};
use crate::calls::{Context, Form};
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::{MachineName, StoreName};
use layerfs_home::outcome::Outcome;

/// Its answer: `ok` and the outcome (`attached`, or `checked` for an
/// attached machine checked again), with what the check measured.
const FORM: Form = Form::of(
    &["ok", "outcome"],
    &["latency_ms"],
    &[
        "ok",
        "outcome",
        "store",
        "machine",
        "latency_ms",
        "server_version",
    ],
);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "attach",
    &[a("store", Name)],
    &[opt("machine", Name).required()],
    "let a machine run Sandboxes of a PostgreSQL Store, checked from that machine first",
    run,
)
.form(&FORM);

/// Runs `store.attach`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let store = StoreName::parse(args.get_text("store").unwrap_or(""))?;
    let machine = MachineName::parse(args.get_text("machine").unwrap_or(""))?;
    let (home, events) = context.parts();
    layerfs_control::store::attach(home, events, &store, &machine)
}
