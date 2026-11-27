//! `sandbox.doctor`: one Sandbox's checks: its record, runtime and daemon.
//! Under a machine that is not ready, nothing is tried.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call, PROGRESS};
use crate::calls::Context;
use crate::doctor::answer::{self, FORM};
use layerfs_protocol::provision::install::SdkValue;
use layerfs_inspect::{Kind, Options};
use layerfs_home::outcome::Outcome;

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
