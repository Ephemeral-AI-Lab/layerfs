//! `sandbox.doctor`: one Sandbox's checks: its record, runtime and daemon.
//! Under a machine that is not ready, nothing is tried.
use crate::dispatch::table::Kind::Name;
use crate::dispatch::table::{a, call, Call, PROGRESS};
use crate::dispatch::Context;
use crate::doctor::answer::{self, FORM};
use layerfs_home::outcome::Outcome;
use layerfs_inspect::{Kind, Options};
use layerfs_protocol::sdk::SdkValue;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "sandbox",
    "doctor",
    &[a("sandbox", Name)],
    &[PROGRESS],
    "check one Sandbox: its record, runtime and daemon, then each of its Workspaces",
    run,
)
.unopened()
.form(&FORM);

fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let sandbox = args.get_text("sandbox").unwrap_or("");
    answer::run(context, Kind::Sandbox, sandbox, Options::default())
}
