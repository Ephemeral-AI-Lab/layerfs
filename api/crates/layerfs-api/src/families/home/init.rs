//! `home.init`: creates an empty home and leaves a current home as it is;
//! a folder that is not a LayerFS 2.0 home is refused with the fix that
//! moves it aside.
use crate::dispatch::table::{call, Call};
use crate::dispatch::{Context, Form};
use layerfs_home::reply::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;

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
    layerfs_inspect::home::init::init(&context.home)
}
