//! `home.init`: creates an empty home and leaves a current home as it is; a
//! folder that is not a LayerFS 2.0 home is refused with the fix that
//! moves it aside, and a home whose data is there but whose `layerfs.json`
//! is missing is refused until `--adopt` writes the marker without touching
//! the data (layerfs-project#752, c6).
use crate::calls::table::Kind::Bool;
use crate::calls::table::{call, opt, Call};
use crate::calls::{Context, Form};
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its answer: the home; an adopted home also says the id it wrote, where
/// the id came from and what the read only audit found.
const FORM: Form = Form::of(
    &["home"],
    &["unchanged", "id", "id_source", "warnings"],
    &[
        "home",
        "unchanged",
        "outcome",
        "layout",
        "version",
        "id",
        "id_source",
        "warnings",
    ],
);

/// Its table entry: it opens the home itself, to say what it did.
pub(crate) const CALL: Call = call(
    "home",
    "init",
    &[],
    &[opt("adopt", Bool)],
    "create this home, or adopt one whose data is there but layerfs.json is missing; a current home is left as it is",
    run,
)
.unopened()
.form(&FORM);

/// Runs `home.init`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let adopt = args.get("adopt") == Some(&SdkValue::Bool(true));
    layerfs_control::home::init(&context.home, adopt)
}
