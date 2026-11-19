//! `store.move`: move a sqlite Store between this computer and a machine.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call, PROGRESS};
use crate::calls::{Context, Form, Policy};
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// Its answer: the bytes moved.
const FORM: Form = Form::of(
    &["moved_bytes"],
    &[],
    &[
        "moved_bytes",
        "outcome",
        "store",
        "from",
        "to",
        "heads_sha256",
    ],
);

/// Its table entry. The names are checked in its check phase.
pub(crate) const CALL: Call = call(
    "store",
    "move",
    &[a("store", Name), a("machine", Name)],
    &[PROGRESS],
    "move a sqlite Store between this computer and a machine",
    run,
)
.policy(Policy::TransferTo("store"))
.form(&FORM);

/// Runs `store.move`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).unwrap_or("").to_owned();
    let (store, machine) = (text("store"), text("machine"));
    let (home, events) = context.parts();
    layerfs_control::store::move_to(home, events, &store, &machine)
}
