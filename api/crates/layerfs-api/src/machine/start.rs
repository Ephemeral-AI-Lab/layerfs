//! `machine.start`: start a stopped machine and its worker; a running one answers at once.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call, PROGRESS};
#[cfg(feature = "machine")]
use crate::calls::Context;
use crate::calls::{Form, Policy};
#[cfg(feature = "machine")]
use layerfs_protocol::provision::install::SdkValue;
#[cfg(feature = "machine")]
use layerfs_home::names::MachineName;
#[cfg(feature = "machine")]
use layerfs_home::outcome::Outcome;

/// Its answer: `ok`, or `unchanged` when it was running already.
const FORM: Form = Form::of(
    &[],
    &["ok", "unchanged"],
    &["ok", "unchanged", "outcome", "machine"],
);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "machine",
    "start",
    &[a("machine", Name)],
    &[PROGRESS],
    "start a stopped machine and its worker; a running one answers at once",
    run,
)
.policy(Policy::Machine("machine"))
.form(&FORM);

/// Runs `machine.start`: `{ phase, done, total }` events, phases `bundle`,
/// `image` (docker), `boot` and `worker`.
#[cfg(feature = "machine")]
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let machine = MachineName::parse(args.get_text("machine").unwrap_or(""))?;
    let (home, events) = context.parts();
    layerfs_machine::with_users::start(home, events, &machine)
}

// without the `machine` feature (the guest worker's build) it is
// refused before any effect
crate::calls::host_only::host_only!("start");
