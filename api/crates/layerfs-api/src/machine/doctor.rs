//! `machine.doctor`: one machine's checks, then a shallow check of each
//! sqlite Store and each Sandbox on it. Under a stopped machine, or one
//! whose provider or worker failed its check, nothing on it is tried.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call, PROGRESS};
use crate::calls::Context;
use crate::doctor::answer::{self, FORM};
use layerfs_home::outcome::Outcome;
use layerfs_inspect::{Kind, Options};
use layerfs_protocol::sdk::SdkValue;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "machine",
    "doctor",
    &[a("machine", Name)],
    &[PROGRESS],
    "check a machine, then each Store and Sandbox on it",
    run,
)
.unopened()
.form(&FORM);

fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let machine = args.get_text("machine").unwrap_or("");
    answer::run(context, Kind::Machine, machine, Options::default())
}
