//! `store.connect`: connect a PostgreSQL database as a Store, or change a
//! connected one's address, login or `synchronous_commit`. This computer
//! is the one place a Store's URL and login are saved; machines attach to
//! the Store (`store.attach`) and dial both. `--dry-run` makes the checks
//! alone, from this computer and each attached machine, and saves nothing.
use crate::calls::table::Kind::{Bool, Content, Name, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::{Context, Form};
use layerfs_home::names::StoreName;
use layerfs_home::outcome::{refused, Outcome};
use layerfs_protocol::sdk::SdkValue;
use layerfs_store::postgresql::connect::StoreConnect;

/// Its answer: the projects a first connect found, else `ok`; never the
/// password. A dry run's (`outcome: tested`) shows the checks in the
/// short form too: `here`, this computer's, and `machines`, one row each.
const FORM: Form = Form::of(
    &[],
    &[
        "ok",
        "outcome",
        "projects",
        "projects_count",
        "here",
        "machines",
    ],
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
        "here",
    ],
)
.parts(&[
    ("machines", crate::calls::Part::Scalars("machine")),
    ("here", crate::calls::Part::Rows(&TESTED)),
])
.short_when(&[("outcome", tested), ("here", tested), ("machines", tested)]);

/// One check of a dry run: `ok` (false kept in the short form), what it
/// measured, or the step it stopped at, the refusal's code and why.
const TESTED: Form = Form::of(
    &["ok"],
    &[
        "latency_ms",
        "server_version",
        "empty",
        "step",
        "code",
        "message",
    ],
    &[
        "ok",
        "latency_ms",
        "server_version",
        "empty",
        "step",
        "code",
        "message",
    ],
);

/// Whether the answer is a dry run's.
fn tested(fields: &[(String, SdkValue)], _asked: &[&str]) -> bool {
    (fields.iter()).any(|(key, value)| key == "outcome" && *value == SdkValue::text("tested"))
}

/// Its table entry: the password is a secret the shell reads from stdin.
pub(crate) const CALL: Call = call(
    "store",
    "connect",
    &[a("store", Name), a("url", Text).redact()],
    &[
        opt("user", Text),
        opt("password", Content).from_stdin("--password-stdin"),
        opt("synchronous_commit", Text),
        opt("dry_run", Bool),
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
        dry_run: args.get_bool("dry_run"),
    };
    let (home, events) = context.parts();
    layerfs_store::admin::with_users::connect(
        home,
        events,
        &request,
        &layerfs_sandbox::users::STORE,
    )
}
