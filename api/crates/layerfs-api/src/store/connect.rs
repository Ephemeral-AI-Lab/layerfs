//! `store.connect`: connect a PostgreSQL database as a Store, or change a
//! connected one's address, login or `synchronous_commit`. This computer
//! is the one place a Store's URL and login are saved; machines attach to
//! the Store (`store.attach`) and dial both.
use crate::calls::table::Kind::{Content, Name, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::{Context, Form};
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::StoreName;
use layerfs_home::outcome::{refused, Outcome};
use layerfs_store::postgresql::connect::StoreConnect;

/// Its answer: the projects a first connect found, else `ok`; never the
/// password.
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
        "url",
        "user",
        "synchronous_commit",
        "machines",
    ],
)
.parts(&[("machines", crate::calls::Part::Scalars("machine"))]);

/// Its table entry: the password is a secret the shell reads from stdin.
pub(crate) const CALL: Call = call(
    "store",
    "connect",
    &[a("store", Name), a("url", Text).redact()],
    &[
        opt("user", Text),
        opt("password", Content).from_stdin("--password-stdin"),
        opt("synchronous_commit", Text),
    ],
    "connect a PostgreSQL database as a Store, or change a connected one's address, login or setting",
    run,
)
.form(&FORM);

/// Runs `store.connect`; a new Store needs `user` and `password`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let password = match args.get("password") {
        Some(SdkValue::Bytes(bytes)) => Some(
            String::from_utf8(bytes.clone())
                .map_err(|_| refused("InvalidInput", "the password is not UTF-8 text"))?,
        ),
        _ => text("password"),
    };
    let request = StoreConnect {
        store: StoreName::parse(args.get_text("store").unwrap_or(""))?,
        url: text("url").unwrap_or_default(),
        user: text("user"),
        password,
        synchronous_commit: text("synchronous_commit"),
    };
    layerfs_control::store::connect(&context.home, &request)
}
