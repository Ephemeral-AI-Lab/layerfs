//! `home.move`: moves this home into another folder on the same
//! filesystem, renaming its own entries and rewriting the paths its
//! records hold; client folders beside them stay.
use crate::dispatch::table::{a, call, Call, Kind};
use crate::dispatch::{Context, Form};
use layerfs_home::reply::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use std::path::Path;

/// Its answer: the new home, and what stayed behind when anything did.
const FORM: Form = Form::of(
    &["home"],
    &["left"],
    &["home", "from", "outcome", "moved", "left", "rewritten"],
);

/// Its table entry: it checks the home itself, and writes no call-log
/// line, since the home it names is gone from that path when it ends.
pub(crate) const CALL: Call = call(
    "home",
    "move",
    &[a("to", Kind::Text)],
    &[],
    "move this home into another folder on the same filesystem; client folders stay",
    run,
)
.unopened()
.quiet()
.form(&FORM);

/// Runs `home.move`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let to = args.get_text("to").unwrap_or_default();
    layerfs_inspect::home::relocate::relocate(&context.home, Path::new(to))
}
