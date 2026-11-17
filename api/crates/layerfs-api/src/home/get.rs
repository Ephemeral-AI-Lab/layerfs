//! `home.get`: the home's state, and the Linux bundle and native daemon this
//! build would use, changing nothing.
use crate::calls::table::{call, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_control::home::Build;
use layerfs_home::outcome::Outcome;

/// Its table entry: never opens the home and writes no log line.
pub(crate) const CALL: Call = call(
    "home",
    "get",
    &[],
    &[],
    "the home's state, and the Linux bundle and daemon this build would use; changes nothing",
    run,
)
.unopened()
.quiet();

/// The commit this library was built from (`LAYERFS_BUILD_COMMIT`), empty
/// when the build named none.
const COMMIT: &str = env!("LAYERFS_API_BUILD_COMMIT");

/// Runs `home.get`.
fn run(context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    let build = Build {
        commit: (!COMMIT.is_empty()).then_some(COMMIT),
        dirty: env!("LAYERFS_API_BUILD_DIRTY") == "true",
    };
    layerfs_control::home::get(&context.home, build)
}
