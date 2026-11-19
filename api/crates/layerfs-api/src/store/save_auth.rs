//! `store.save_auth`: save a PostgreSQL login in the home's secret folder;
//! saving again rotates it.
use crate::calls::table::Kind::{Content, Name, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::{Context, Form};
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::{refused, Outcome};

/// Its answer: `ok`; never the password.
const FORM: Form = Form::of(
    &["ok"],
    &[],
    &["ok", "outcome", "auth", "user", "stores", "stores_count"],
);

/// Its table entry: the password is a secret the shell reads from stdin.
pub(crate) const CALL: Call = call(
    "store",
    "save_auth",
    &[a("auth", Name)],
    &[
        opt("user", Text).required(),
        opt("password", Content)
            .from_stdin("--password-stdin")
            .required(),
    ],
    "save a PostgreSQL login in the home; saving again rotates it",
    run,
)
.form(&FORM);

/// Runs `store.save_auth`; the table requires `user` and `password`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let auth = args.get_text("auth").unwrap_or("");
    let user = args.get_text("user").unwrap_or_default();
    let password = match args.get("password") {
        Some(SdkValue::Bytes(bytes)) => String::from_utf8(bytes.clone())
            .map_err(|_| refused("InvalidInput", "the password is not UTF-8 text"))?,
        _ => args.get_text("password").unwrap_or_default().to_owned(),
    };
    layerfs_store::postgresql::login::save(&context.home, auth, user, &password)
}
