//! `store.list`: every Store, from the home's records.
use super::get::PARTS;
use crate::calls::table::Kind::Name;
use crate::calls::table::{call, opt, Call};
use crate::calls::{Context, Form, Part, Section};
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::MachineName;
use layerfs_home::outcome::Outcome;

/// One item: a Store as `store.get` answers it, its name first.
const ITEM: Form = Form::of(
    &["store", "engine"],
    &[
        "state",
        "machine",
        "auth",
        "connections",
        "projects",
        "projects_count",
        "backups",
        "op",
        "blocked_by",
        "reason",
        "fix",
        "observed_at",
    ],
    &[
        "store",
        "engine",
        "state",
        "machine",
        "auth",
        "connections",
        "projects",
        "projects_count",
        "backups",
        "op",
        "blocked_by",
        "reason",
        "fix",
        "observed_at",
        "profile",
        "synchronous_commit",
        "sqlite_version",
        "created_at",
        "last_transfer",
    ],
)
.parts(PARTS)
.short_when(&[crate::calls::form::OBSERVED]);

/// Its answer: the items, unpaged (the list is the home's Store records).
const FORM: Form = Form::of(&["items"], &[], &["items"]).parts(&[("items", Part::Rows(&ITEM))]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "list",
    &[],
    &[opt("machine", Name)],
    "every Store, from the home's records",
    run,
)
.form(&FORM)
.with(&[Section::item("stats", &["size"])]);

/// Runs `store.list`: sizes are read only when `stats` was asked for.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let machine = args
        .get_text("machine")
        .map(MachineName::parse)
        .transpose()?;
    let machine = machine.as_ref().map(MachineName::as_str);
    let machines = &layerfs_control::store::machine_state;
    layerfs_store::list::list(&context.home, machine, context.with("stats"), machines)
}
