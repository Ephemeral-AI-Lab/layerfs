//! `home.init`: creates an empty home and leaves a current home as it is;
//! a folder that is not a LayerFS 2.0 home is refused with the fix that
//! moves it aside.
use crate::calls::table::{call, Call};
use crate::calls::{Context, Form};
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its answer: the home, and whether it was there already.
const FORM: Form = Form::of(
    &["home"],
    &["unchanged"],
    &["home", "unchanged", "outcome", "layout", "version"],
);

/// Its table entry: it opens the home itself, to say what it did.
pub(crate) const CALL: Call = call(
    "home",
    "init",
    &[],
    &[],
    "create this home; a current home is left as it is",
    run,
)
.unopened()
.form(&FORM);

/// Runs `home.init`.
fn run(context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    layerfs_control::home::init(&context.home)
}
