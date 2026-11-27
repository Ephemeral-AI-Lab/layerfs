//! `store.attach`: let a machine run Sandboxes of a PostgreSQL Store,
//! dialling the URL and login this computer saved; checked from it first.
//! `--dry-run` makes the check alone and records nothing.
use crate::calls::table::Kind::{Bool, Name};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::{Context, Form};
use layerfs_protocol::provision::install::SdkValue;
use layerfs_home::names::{MachineName, StoreName};
use layerfs_home::outcome::Outcome;

/// Its answer: `ok` and the outcome (`attached`, `checked` for an
/// attached machine checked again, or `tested` for a dry run), with what
/// the check measured.
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
    &[opt("machine", Name).required(), opt("dry_run", Bool)],
    "let a machine run Sandboxes of a PostgreSQL Store, checked from that machine first",
    run,
)
.form(&FORM);

/// Runs `store.attach`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let store = StoreName::parse(args.get_text("store").unwrap_or(""))?;
    let machine = MachineName::parse(args.get_text("machine").unwrap_or(""))?;
    let (home, events) = context.parts();
    let dry_run = args.get_bool("dry_run");
    layerfs_store::admin::with_users::attach(home, events, &store, &machine, dry_run)
}
