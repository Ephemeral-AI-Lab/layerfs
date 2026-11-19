//! `store.auths`: the saved PostgreSQL logins and the Stores that use each.
use crate::calls::table::{call, Call};
use crate::calls::{Context, Form, Part};
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;

/// One login: never its password. A login file that cannot be read is
/// `broken` with its fix (spec v23).
const ITEM: Form = Form::of(
    &["auth", "user"],
    &["state", "reason", "fix", "stores", "stores_count"],
    &[
        "auth",
        "user",
        "state",
        "reason",
        "fix",
        "stores",
        "stores_count",
        "created_at",
        "updated_at",
    ],
);

/// Its answer: every login; unpaged, a home holds a handful.
const FORM: Form = Form::of(&["items"], &[], &["items"]).parts(&[("items", Part::Rows(&ITEM))]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "auths",
    &[],
    &[],
    "the saved PostgreSQL logins and the Stores that use each",
    run,
)
.form(&FORM);

/// Runs `store.auths`.
fn run(context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    layerfs_store::postgresql::login::list(&context.home)
}
