//! `store.get` and each `store.list` item (call surface 2.0): a Store's
//! record and its records-only state ([`crate::state::recorded`]), with no
//! probe. `--with live` reads the Store where it lives ([`crate::live`]);
//! `--with stats` opens it for its counters ([`crate::stats`]).
//!
//! The answer is one full map, the long form's keys: the dispatcher makes
//! the short form from it. A sqlite Store answers `machine`, `profile` and
//! `sqlite_version`; a PostgreSQL Store `auth`, `connections` and
//! `synchronous_commit`; the other engine's keys are null.
use crate::backup::record::StoreLink;
use crate::intent;
use crate::record::{Placement, StoreRecord, StoreRef};
use crate::state;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::home::{observed, paths};
use layerfs_home::outcome::{failed, Outcome};
use layerfs_home::state::State;
use layerfs_home::time;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// How many project names an answer shows; `projects_count` counts them all.
pub const PROJECTS_SHOWN: usize = 20;

/// What `store.get` was asked to read beyond the records.
#[derive(Clone, Copy, Debug, Default)]
pub struct With {
    /// `--with live`: the Store where it lives.
    pub live: bool,
    /// `--with stats`: its counters, opening it.
    pub stats: bool,
}

/// `ms` since 1970 as an answer's time, null when unknown.
pub(crate) fn at(ms: Option<i64>) -> SdkValue {
    ms.map_or(SdkValue::Null, |ms| SdkValue::Text(time::iso(ms)))
}

/// A digest as answers show it: hex text, whether the record holds text
/// or bytes; null when absent.
pub(crate) fn hex_of(value: Option<&SdkValue>) -> SdkValue {
    match value {
        Some(SdkValue::Text(text)) => SdkValue::text(text),
        Some(SdkValue::Bytes(bytes)) => SdkValue::text(layerfs_bridge::sdk_values::hex(bytes)),
        _ => SdkValue::Null,
    }
}

/// One `backups` row: `{ bucket, file, sha256, heads_sha256, last, attempt }`.
/// `last` is the last transfer that worked (`{call, ended_at, size}`) and
/// `attempt` the last one that did not (`{call, ended_at, error}`), from
/// the link's `attempt` field ([`ATTEMPT`]) once one is written.
fn backup_row(bucket: &str, link: &StoreLink) -> SdkValue {
    let fields = link.to_record();
    let last = fields.get("last");
    let last = last.map_or(SdkValue::Null, |last| {
        SdkValue::map([
            ("call", last.get("op").cloned().unwrap_or_default()),
            ("ended_at", at(last.get_int("at"))),
            ("size", last.get("bytes").cloned().unwrap_or_default()),
        ])
    });
    let attempt = fields
        .get(ATTEMPT)
        .filter(|value| **value != SdkValue::Null);
    let attempt = attempt.map_or(SdkValue::Null, |tried| {
        SdkValue::map([
            ("call", tried.get("call").cloned().unwrap_or_default()),
            ("ended_at", at(tried.get_int("at"))),
            ("error", tried.get("error").cloned().unwrap_or_default()),
        ])
    });
    SdkValue::map([
        ("bucket", SdkValue::text(bucket)),
        ("file", SdkValue::opt_text(link.file())),
        ("sha256", hex_of(last_field(fields, "sha256"))),
        ("heads_sha256", SdkValue::opt_text(link.heads())),
        ("last", last),
        ("attempt", attempt),
    ])
}

fn last_field<'a>(fields: &'a SdkValue, name: &str) -> Option<&'a SdkValue> {
    fields.get("last").and_then(|last| last.get(name))
}

/// The field of a Store's link to a bucket (`backups.<bucket>.attempt`)
/// that holds the last transfer that did not work: `{ call, at, error }`.
/// The backup package writes it when a backup or restore fails; a live
/// read writes it when it settles one whose owner is gone.
pub const ATTEMPT: &str = "attempt";

/// The record's `last_transfer` as answers show it: `{ call, from, to,
/// ended_at, size, sha256, heads_sha256, retired }`.
fn last_transfer(record: &StoreRecord) -> SdkValue {
    let Some(last) = record.field("last_transfer") else {
        return SdkValue::Null;
    };
    let copied = |name: &str| last.get(name).cloned().unwrap_or_default();
    SdkValue::map([
        (
            "call",
            last.get_text("call")
                .map_or(SdkValue::Null, |call| SdkValue::text(intent::word(call))),
        ),
        ("from", copied("from")),
        ("to", copied("to")),
        ("ended_at", at(last.get_int("at"))),
        ("size", copied("bytes")),
        ("sha256", hex_of(last.get("sha256"))),
        ("heads_sha256", hex_of(last.get("heads"))),
        ("retired", copied("retired")),
    ])
}

/// A PostgreSQL Store's `connections`: the Store's own address (no
/// `machine`: every machine without its own uses it), then each machine's
/// own, in record order. Never a password.
fn connections(record: &StoreRecord) -> SdkValue {
    let store_url = record.text("url");
    let mut rows = vec![SdkValue::map([
        ("machine", SdkValue::Null),
        ("url", SdkValue::opt_text(store_url)),
    ])];
    for (machine, own) in record.own_connections() {
        rows.push(SdkValue::map([
            ("machine", SdkValue::text(machine)),
            ("url", SdkValue::opt_text(own.url.as_deref().or(store_url))),
        ]));
    }
    SdkValue::List(rows)
}

/// The machine a sqlite Store's record places it on (`native` when a
/// record names none); null for a PostgreSQL Store and for one whose
/// machine a 1.1.0 home did not tell.
fn placed(record: &StoreRecord) -> SdkValue {
    match record.store_ref() {
        Ok(StoreRef::Sqlite {
            machine: Placement::Machine(machine),
            ..
        }) => SdkValue::text(machine),
        _ => SdkValue::Null,
    }
}

/// The names of Store `store`'s projects, sorted.
pub fn project_names(home: &Path, store: &str) -> Result<Vec<String>> {
    paths::names(&paths::store_dir(home, store).join("project")).map_err(failed)
}

/// Store `record` as `store.get` and a `store.list` item answer it, in
/// `state`: every long-form key. `observed_at` is the state's own when it
/// is `unknown`, else when the Store was last read live (`observed`).
pub fn summary(
    home: &Path,
    record: &StoreRecord,
    state: &State,
    observed: Option<i64>,
) -> Result<SdkValue> {
    let store = record.name();
    let projects = project_names(home, store)?;
    let shown = projects.iter().take(PROJECTS_SHOWN).map(SdkValue::text);
    let (engine, sqlite) = match record.store_ref() {
        Ok(StoreRef::Postgres { .. }) => ("postgresql", false),
        Ok(StoreRef::Sqlite { .. }) => ("sqlite", true),
        // a kind this build does not read: its own word, as recorded
        Err(_) => (record.text("kind").unwrap_or("unknown"), false),
    };
    let only = |yes: bool, value: SdkValue| if yes { value } else { SdkValue::Null };
    let postgresql = engine == "postgresql";
    let links = record.links();
    let backups = links.iter().map(|(bucket, link)| backup_row(bucket, link));
    let mut fields: Vec<(&str, SdkValue)> = vec![
        ("store", SdkValue::text(store)),
        ("engine", SdkValue::text(engine)),
    ];
    fields.extend(state.fields());
    if !matches!(state, State::Unknown { .. }) {
        fields.push(("observed_at", at(observed)));
    }
    fields.extend([
        ("machine", placed(record)),
        // a saved login is package F-pg's: null until it lands
        ("auth", SdkValue::Null),
        ("connections", only(postgresql, connections(record))),
        ("projects", SdkValue::List(shown.collect())),
        (
            "projects_count",
            SdkValue::Int(i64::try_from(projects.len()).unwrap_or(i64::MAX)),
        ),
        ("backups", SdkValue::List(backups.collect())),
        (
            "profile",
            only(sqlite, SdkValue::opt_text(record.text("profile"))),
        ),
        (
            "synchronous_commit",
            only(
                postgresql,
                SdkValue::opt_text(record.text("synchronous_commit")),
            ),
        ),
        (
            "sqlite_version",
            only(sqlite, SdkValue::opt_text(record.text("sqlite"))),
        ),
        ("created_at", at(record.to_record().get_int("created"))),
        ("last_transfer", last_transfer(record)),
    ]);
    Ok(SdkValue::Map(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    ))
}

/// `store.get(store)`: the Store's record and its records-only state. With
/// `live`, the state, its companion and `observed_at` come from a read of
/// the Store where it lives, and the `live` section holds what that read
/// found; with `stats`, the `stats` section holds its counters.
pub fn get(
    home: &Path,
    events: &mut Events<'_>,
    store: &str,
    with: With,
    machines: state::Machines,
) -> Outcome {
    let record = StoreRecord::of(home, store)?;
    let (record, state, observed, live) = match with.live {
        true => {
            let seen = crate::live::read(home, record, machines)?;
            (seen.record, seen.state, seen.observed, Some(seen.section))
        }
        false => {
            let state = state::recorded(home, &record, machines)?;
            (
                record,
                state,
                observed::read(&paths::store_record(home, store)),
                None,
            )
        }
    };
    let mut answer = summary(home, &record, &state, observed)?;
    if let SdkValue::Map(fields) = &mut answer {
        if let Some(live) = live {
            fields.push(("live".to_owned(), live));
        }
        if with.stats {
            fields.push((
                "stats".to_owned(),
                crate::stats::stats(home, events, &record)?,
            ));
        }
    }
    Ok(answer)
}
