//! `store.create`: an empty sqlite Store on this computer, or made on a
//! machine directly.
use crate::dispatch::table::Kind::Name;
use crate::dispatch::table::{a, call, opt, Call};
use crate::dispatch::{Context, Form};
use layerfs_home::names::{MachineName, StoreName};
use layerfs_home::reply::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;

/// Its answer: nothing to say in the short form.
const FORM: Form = Form::of(
    &["ok"],
    &[],
    &[
        "ok",
        "outcome",
        "store",
        "engine",
        "machine",
        "profile",
        "sqlite_version",
        "created_at",
    ],
);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "create",
    &[a("name", Name)],
    &[opt("machine", Name)],
    "an empty sqlite Store",
    run,
)
.form(&FORM);

/// Runs `store.create`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let name = StoreName::parse(args.get_text("name").unwrap_or(""))?;
    let machine = args
        .get_text("machine")
        .map(MachineName::parse)
        .transpose()?;
    layerfs_store::backend::sqlite::create::create(
        &context.home,
        &name,
        machine.as_ref().map(|m| m.as_str()),
    )
}
