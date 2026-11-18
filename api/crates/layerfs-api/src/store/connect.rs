//! `store.connect`: connect a PostgreSQL database as a Store, or set an
//! address of a connected one (every machine's, or one machine's own).
use crate::calls::table::Kind::{Name, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::{Context, Form};
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::{MachineName, StoreName};
use layerfs_home::outcome::Outcome;
use layerfs_store::postgresql::connect::StoreConnect;

/// Its answer: the projects a first connect found, else `ok`.
const FORM: Form = Form::of(
    &[],
    &["ok", "projects", "projects_count"],
    &[
        "ok",
        "projects",
        "projects_count",
        "outcome",
        "store",
        "engine",
        "auth",
        "connections",
        "synchronous_commit",
    ],
);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "connect",
    &[a("store", Name), a("url", Text).redact()],
    &[
        opt("auth", Name),
        opt("machine", Name),
        opt("synchronous_commit", Text),
    ],
    "connect a PostgreSQL database as a Store, or set an address of a connected one",
    run,
)
.form(&FORM);

/// Runs `store.connect`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = StoreConnect {
        store: StoreName::parse(args.get_text("store").unwrap_or(""))?,
        url: text("url").unwrap_or_default(),
        auth: text("auth"),
        synchronous_commit: text("synchronous_commit"),
    };
    let machine = args
        .get_text("machine")
        .map(MachineName::parse)
        .transpose()?;
    let (home, events) = context.parts();
    layerfs_control::store::connect(home, events, &request, machine.as_ref())
}
