//! `store.update`: change a PostgreSQL Store's saved login or its
//! `synchronous_commit`.
use crate::calls::table::Kind::{Name, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::{Context, Form};
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::StoreName;
use layerfs_home::outcome::Outcome;
use layerfs_store::postgresql::update::{update, StoreUpdate};

/// Its answer: `ok`.
const FORM: Form = Form::of(
    &["ok"],
    &[],
    &["ok", "outcome", "store", "auth", "synchronous_commit"],
);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "update",
    &[a("store", Name)],
    &[opt("auth", Name), opt("synchronous_commit", Text)],
    "change a PostgreSQL Store's saved login or its synchronous_commit",
    run,
)
.form(&FORM);

/// Runs `store.update`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = StoreUpdate {
        store: StoreName::parse(args.get_text("store").unwrap_or(""))?,
        auth: text("auth"),
        synchronous_commit: text("synchronous_commit"),
    };
    update(&context.home, &request)
}
