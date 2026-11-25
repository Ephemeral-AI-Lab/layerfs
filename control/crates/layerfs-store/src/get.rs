//! `store.get` and each `store.list` item (call surface 2.0): a Store's
//! record and its records-only state ([`crate::state::recorded`]), with no
//! probe. `--with live` reads the Store where it lives ([`crate::live`]);
//! `--with stats` opens it for its counters ([`crate::stats`]).
//!
//! The answer is one full map, the long form's keys: the dispatcher makes
//! the short form from it. A sqlite Store answers `machine`, `profile` and
//! `sqlite_version`; a PostgreSQL Store `url`, `user`, `password_saved`,
//! `password_changed_at`, `binding`, `synchronous_commit`, `reach` (this
//! computer's last) and `machines` (the attached machines: rows with
//! their reach in `store.get`, names in a `store.list` item); the other
//! engine's keys are null. The password is never answered.
use crate::backup::record::StoreLink;
use crate::intent;
use crate::record::{StoreRecord, StoreRef};
use crate::state;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::home::{observed, paths};
use layerfs_home::outcome::{failed, Outcome};
use layerfs_home::state::State;
use layerfs_home::time;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// What `store.get` was asked to read beyond the records.
#[derive(Clone, Copy, Debug, Default)]
pub struct With {
    /// `--with live`: the Store where it lives.
    pub live: bool,
    /// `--with stats`: its counters, opening it.
    pub stats: bool,
    /// `--with format`: its format, read without opening it (#804).
    pub format: bool,
}

/// `ms` since 1970 as an answer's time, null when unknown.
pub(crate) fn at(ms: Option<i64>) -> SdkValue {
    ms.map_or(SdkValue::Null, |ms| SdkValue::Text(time::iso(ms)))
}

/// Bytes as answers show them: lowercase hex text.
pub(crate) fn hex_text(bytes: &[u8]) -> SdkValue {
    SdkValue::text(layerfs_bridge::sdk_values::hex(bytes))
}

/// A digest as answers show it: hex text, whether the record holds text
/// or bytes; null when absent.
pub(crate) fn hex_of(value: Option<&SdkValue>) -> SdkValue {
    match value {
        Some(SdkValue::Text(text)) => SdkValue::text(text),
        Some(SdkValue::Bytes(bytes)) => hex_text(bytes),
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

/// The names of Store `store`'s projects, sorted.
pub fn project_names(home: &Path, store: &str) -> Result<Vec<String>> {
    paths::names(&paths::store_dir(home, store).join("project")).map_err(failed)
}

/// Store `record` as `store.get` and a `store.list` item answer it, in
/// `state`: every long-form key. `observed_at` is the state's own when it
/// is `unknown`, else when the Store was last read live (`observed`).
/// `rows` answers a PostgreSQL Store's `machines` as rows (`store.get`),
/// else as names (a `store.list` item). A PostgreSQL Store's `binding` is
/// the first 8 bytes of its id as 16 hex, the open-Store cache's key (spec
/// v23 V23-5).
pub fn summary(
    home: &Path,
    record: &StoreRecord,
    state: &State,
    observed: Option<i64>,
    rows: bool,
) -> Result<SdkValue> {
    let store = record.name();
    let projects = project_names(home, store)?;
    let (shown, shown_count) = layerfs_home::helpers::shown_names(&projects);
    let (engine, sqlite) = match record.store_ref() {
        Ok(StoreRef::Postgres { .. }) => ("postgresql", false),
        Ok(StoreRef::Sqlite { .. }) => ("sqlite", true),
        // a kind this build does not read: its own word, as recorded
        Err(_) => (record.text("kind").unwrap_or("unknown"), false),
    };
    let only = |yes: bool, value: SdkValue| if yes { value } else { SdkValue::Null };
    let postgresql = engine == "postgresql";
    let pg_text = |key: &str| only(postgresql, SdkValue::opt_text(record.text(key)));
    // a URL that may hold a login is never shown
    let url = record.text("url").map(crate::postgresql::connect::shown);
    let (saved, changed) = match postgresql {
        true => crate::postgresql::connect::password_changed(record),
        false => (false, SdkValue::Null),
    };
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
        // a sqlite Store's machine; null for a PostgreSQL Store
        ("machine", SdkValue::opt_text(state::placed(record))),
        ("url", only(postgresql, SdkValue::opt_text(url))),
        ("user", pg_text("user")),
        ("password_saved", only(postgresql, SdkValue::Bool(saved))),
        ("password_changed_at", changed),
        (
            "binding",
            only(
                postgresql,
                record
                    .binding()
                    .map_or(SdkValue::Null, |id| hex_text(&id[..id.len().min(8)])),
            ),
        ),
        (
            "reach",
            only(postgresql, crate::postgresql::connect::reach(record)),
        ),
        (
            "machines",
            only(
                postgresql,
                crate::postgresql::connect::machines(record, rows),
            ),
        ),
        ("projects", shown),
        ("projects_count", shown_count),
        ("backups", SdkValue::List(backups.collect())),
        (
            "profile",
            only(sqlite, SdkValue::opt_text(crate::record::profile(record))),
        ),
        ("synchronous_commit", pg_text("synchronous_commit")),
        (
            "sqlite_version",
            only(sqlite, SdkValue::opt_text(record.text("sqlite"))),
        ),
        ("created_at", at(record.to_record().get_int("created"))),
        ("last_transfer", last_transfer(record)),
    ]);
    Ok(layerfs_home::helpers::map_of(fields))
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
    let mut answer = summary(home, &record, &state, observed, true)?;
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
