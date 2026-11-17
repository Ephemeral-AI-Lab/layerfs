//! `home.init`: creates an empty home, converts an older one, and leaves a
//! current home as it is.
use crate::calls::table::{call, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "home",
    "init",
    &[],
    &[],
    "create this home, or convert an older one; a current home is left as it is",
    run,
)
.unopened();

/// Runs `home.init`.
fn run(context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    layerfs_control::home::init(&context.home)
}
