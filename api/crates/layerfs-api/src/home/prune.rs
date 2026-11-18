//! `home.prune`: removes one leftover `home leftovers` listed, under its
//! guard.
use crate::calls::table::Kind::Text;
use crate::calls::table::{a, call, Call};
use crate::calls::{Context, Form};
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its answer: the bytes it freed, when they were measured.
const FORM: Form = Form::of(
    &[],
    &["ok", "freed_bytes"],
    &[
        "ok",
        "freed_bytes",
        "outcome",
        "leftover",
        "kind",
        "machine",
        "path",
    ],
);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "home",
    "prune",
    &[a("leftover", Text)],
    &[],
    "remove one leftover that home leftovers listed",
    run,
)
.form(&FORM);

/// Runs `home.prune`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let leftover = args.get_text("leftover").unwrap_or("");
    layerfs_control::doctor::leftover_prune::prune(&context.home, leftover)
}
