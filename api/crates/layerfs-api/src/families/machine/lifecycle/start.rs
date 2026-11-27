//! `machine.start`: start a stopped machine and its worker; a running one answers at once.
use crate::dispatch::table::Kind::Name;
use crate::dispatch::table::{a, call, Call, PROGRESS};
#[cfg(feature = "machine")]
use crate::dispatch::Context;
use crate::dispatch::{Form, Policy};
#[cfg(feature = "machine")]
use layerfs_home::names::MachineName;
#[cfg(feature = "machine")]
use layerfs_home::outcome::Outcome;
#[cfg(feature = "machine")]
use layerfs_protocol::sdk::SdkValue;

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
    layerfs_machine::with_users::start(home, events, &machine, &layerfs_sandbox::users::MACHINE)
}

// without the `machine` feature (the guest worker's build) it is
// refused before any effect
crate::dispatch::guard::host_only::host_only!("start");
