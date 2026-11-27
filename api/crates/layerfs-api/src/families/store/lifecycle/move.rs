//! `store.move`: move a sqlite Store between this computer and a machine.
use crate::dispatch::table::Kind::Name;
use crate::dispatch::table::{a, call, Call, PROGRESS};
use crate::dispatch::{Context, Form, Policy};
use layerfs_home::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;

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
    layerfs_store::admin::with_users::move_to(
        home,
        events,
        &store,
        &machine,
        &layerfs_sandbox::users::STORE,
    )
}
