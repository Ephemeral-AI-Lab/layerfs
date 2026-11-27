//! `home.doctor`: the home's own checks (this build, its prerequisites, the
//! Linux bundle, the home folder and its layout, free disk, every record in
//! the home), then a shallow check of each machine, PostgreSQL Store and
//! bucket. It changes nothing, not even a home that does not exist yet.
use crate::dispatch::table::{call, Call, PROGRESS};
use crate::dispatch::Context;
use crate::doctor::{self, FORM};
use layerfs_home::reply::outcome::Outcome;
use layerfs_inspect::{Kind, Options};
use layerfs_protocol::sdk::SdkValue;

/// Its table entry: it never opens (creates or converts) the home.
pub(crate) const CALL: Call = call(
    "home",
    "doctor",
    &[],
    &[PROGRESS],
    "check the home, then each machine, PostgreSQL Store and bucket, shallowly",
    run,
)
.unopened()
.form(&FORM);

fn run(context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    let home = context.home.display().to_string();
    doctor::run(context, Kind::Home, &home, Options::default())
}
