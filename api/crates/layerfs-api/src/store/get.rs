//! `store.get`: a Store's record and state, from the home's records; with
//! `--with live`, read where the Store lives; with `--with stats`, its
//! counters.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call};
use crate::calls::{Context, Form, Part, Section};
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::StoreName;
use layerfs_home::outcome::Outcome;
use layerfs_store::get::With;

/// One `backups` row: `{bucket}`, and in the long form what the link holds.
const BACKUP: Form = Form::of(
    &["bucket"],
    &[],
    &[
        "bucket",
        "file",
        "sha256",
        "heads_sha256",
        "last",
        "attempt",
    ],
);

/// One `connections` row of a PostgreSQL Store: its address, and the
/// machine whose own it is (none for the Store's own).
const CONNECTION: Form = Form::of(&["url"], &["machine"], &["machine", "url"]);

/// The rows inside a Store's answer; `store.list` items share them.
pub(super) const PARTS: &[(&str, Part)] = &[
    ("projects", Part::Scalars("project")),
    ("connections", Part::Rows(&CONNECTION)),
    ("backups", Part::Rows(&BACKUP)),
];

/// Its answer: `engine` and `state` always, with the state's companion.
const FORM: Form = Form::of(
    &["engine", "state"],
    &[
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
        "store",
        "profile",
        "synchronous_commit",
        "sqlite_version",
        "created_at",
        "last_transfer",
    ],
)
.parts(PARTS);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "store",
    "get",
    &[a("store", Name)],
    &[],
    "a Store's record and state",
    run,
)
.form(&FORM)
.with(&[
    Section::own(
        "live",
        &[
            "holders",
            "phases",
            "behind",
            "server_version",
            "latency_ms",
        ],
    ),
    Section::own(
        "stats",
        &[
            "size",
            "logical_bytes",
            "canonical_bytes",
            "stored_bytes",
            "metadata_bytes",
            "file_bytes",
            "reusable_bytes",
            "dedupe",
            "compression",
            "init",
        ],
    ),
]);

/// Runs `store.get`: the Store is read only for the sections asked for.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let store = StoreName::parse(args.get_text("store").unwrap_or(""))?;
    let with = With {
        live: context.with("live"),
        stats: context.with("stats"),
    };
    let (home, events) = context.parts();
    let machines = &layerfs_control::store::machine_state;
    layerfs_store::get::get(home, events, store.as_str(), with, machines)
}
