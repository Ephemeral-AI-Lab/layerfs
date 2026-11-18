//! `home.leftovers`: what earlier runs left behind on this computer and on
//! each running machine, one bounded walk per side; a machine that is
//! stopped or does not answer is named in `not_listed`.
use crate::calls::table::Kind::Name;
use crate::calls::table::{call, opt, Call};
use crate::calls::{Context, Form, Part};
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// One leftover; `leftover` is the id `home prune` takes.
const ITEM: Form = Form::of(
    &["leftover", "modified_at"],
    &["size", "report_only"],
    &[
        "leftover",
        "modified_at",
        "size",
        "report_only",
        "removable_at",
        "kind",
        "machine",
        "path",
    ],
);
/// A machine that was not walked, or not to the end.
const NOT_LISTED: Form = Form::of(
    &["machine", "message"],
    &["blocked_by", "reason"],
    &["machine", "blocked_by", "reason", "message"],
);
const FORM: Form = Form::of(
    &["items"],
    &["not_listed"],
    &["items", "not_listed", "observed_at"],
)
.parts(&[
    ("items", Part::Rows(&ITEM)),
    ("not_listed", Part::Rows(&NOT_LISTED)),
]);

/// Its table entry: a read that never opens (creates or converts) the
/// home; one that is not current has no leftovers.
pub(crate) const CALL: Call = call(
    "home",
    "leftovers",
    &[],
    &[opt("machine", Name)],
    "what earlier runs left behind on this computer and on each running machine",
    run,
)
.unopened()
.form(&FORM);

/// Runs `home.leftovers`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let machine = args.get_text("machine");
    layerfs_control::doctor::leftover_list::list(&context.home, machine)
}
