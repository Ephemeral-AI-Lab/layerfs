//! `store.list`: every Store, from the home's records.
use super::get::ITEM_PARTS;
use crate::dispatch::table::Kind::Name;
use crate::dispatch::table::{call, opt, Call};
use crate::dispatch::{Context, Form, Part, Section};
use layerfs_home::names::MachineName;
use layerfs_home::reply::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;

/// One item: a Store as `store.get` answers it, its name first.
const ITEM: Form = Form::of(
    &["store", "engine"],
    &[
        "state",
        "machine",
        "user",
        "machines",
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
        "url",
        "user",
        "password_saved",
        "password_changed_at",
        "binding",
        "reach",
        "machines",
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
.parts(ITEM_PARTS)
.short_when(&[crate::dispatch::answer::form::OBSERVED]);

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
.with(&[
    Section::item("stats", &["size"]),
    Section::item("format", &["format"]),
]);

/// Runs `store.list`: sizes are read only when `stats` was asked for, and
/// formats only when `format` was.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let machine = args
        .get_text("machine")
        .map(MachineName::parse)
        .transpose()?;
    let machine = machine.as_ref().map(MachineName::as_str);
    let machines = layerfs_sandbox::users::reads();
    let (stats, format) = (context.with("stats"), context.with("format"));
    layerfs_store::admin::status::list::list_with(&context.home, machine, stats, format, machines)
}
