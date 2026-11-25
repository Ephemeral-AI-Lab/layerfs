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

/// A reach: the last one seen, from this computer (`reach`) or from an
/// attached machine (a `machines` row's).
const REACH: Form = Form::of(
    &["at", "ok"],
    &["latency_ms", "server_version", "error"],
    &["at", "ok", "latency_ms", "server_version", "error"],
);

/// One `machines` row of a PostgreSQL Store: a machine attached to it, the
/// word of its last reach (`ok`, `failed`, null) and, in the long form, the
/// reach itself.
const MACHINE: Form = Form::of(&["machine"], &["reached"], &["machine", "reached", "reach"])
    .parts(&[("reach", Part::Rows(&REACH))]);

/// The rows inside a Store's answer.
const PARTS: &[(&str, Part)] = &[
    ("projects", Part::Scalars("project")),
    ("reach", Part::Rows(&REACH)),
    ("machines", Part::Rows(&MACHINE)),
    ("backups", Part::Rows(&BACKUP)),
];

/// The rows inside a `store.list` item: its `machines` are names.
pub(super) const ITEM_PARTS: &[(&str, Part)] = &[
    ("projects", Part::Scalars("project")),
    ("reach", Part::Rows(&REACH)),
    ("machines", Part::Scalars("machine")),
    ("backups", Part::Rows(&BACKUP)),
];

/// Its answer: `engine` and `state` always, with the state's companion.
const FORM: Form = Form::of(
    &["engine", "state"],
    &[
        "machine",
        "url",
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
        "store",
        "profile",
        "synchronous_commit",
        "sqlite_version",
        "created_at",
        "last_transfer",
    ],
)
.parts(PARTS)
.short_when(&[crate::calls::form::OBSERVED]);

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
        format: false,
    };
    let (home, events) = context.parts();
    let reads = layerfs_control::store::reads();
    layerfs_store::get::get(home, events, store.as_str(), with, reads)
}
