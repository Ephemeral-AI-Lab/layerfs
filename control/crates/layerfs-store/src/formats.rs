//! The `format` section of `store get --with format` and each `store list
//! --with format` item (#804 §2): `{ version, latest, status, fix }`, read
//! from a sqlite Store's header on a raw read-only connection (through its
//! machine's worker, [`crate::upgrade::FORMAT_OP`], when its calls run
//! there) or with one `SELECT` on a PostgreSQL Store. It starts nothing (no
//! machine start, no Store open), so it answers for a Store this build
//! refuses to open.
//!
//! A Store whose format could not be read (its machine is stopped or does
//! not answer, its worker is of an earlier build, its server does not
//! answer, its file is not there) answers status `unknown` with a null
//! version and fix, as `--with live` answers a Store it could not read
//! `unknown`.
use crate::place::{self, Executor};
use crate::record::{StoreRecord, StoreRef};
use crate::upgrade::FORMAT_OP;
use layerfs_protocol::provision::install::SdkValue;
use layerfs_home::home::paths;
use layerfs_home::outcome::shell_word;
use layerfs_persistence::upgrade::{
    postgresql_format, sqlite_format, Format, FormatStatus, POSTGRESQL_LATEST, SQLITE_LATEST,
};
use std::path::Path;

/// The status word of a format that could not be read.
pub const UNKNOWN: &str = "unknown";

/// The section of `format` of Store `store`, with its fix: `layerfs store
/// upgrade <store>` when outdated, `store remove` (sqlite) or `store
/// disconnect` (PostgreSQL) when unsupported, none when current or newer.
pub fn section(format: &Format, store: &str, postgresql: bool) -> SdkValue {
    let fix = |call: &str| SdkValue::text(format!("layerfs store {call} {}", shell_word(store)));
    let fix = match (format.status, postgresql) {
        (FormatStatus::Outdated, _) => fix("upgrade"),
        (FormatStatus::Unsupported, true) => fix("disconnect"),
        (FormatStatus::Unsupported, false) => fix("remove"),
        (FormatStatus::Current | FormatStatus::Newer, _) => SdkValue::Null,
    };
    SdkValue::map([
        (
            "version",
            format.version.map_or(SdkValue::Null, SdkValue::Int),
        ),
        ("latest", SdkValue::Int(format.latest)),
        ("status", SdkValue::text(format.status.word())),
        ("fix", fix),
    ])
}

/// The section of a format that could not be read, `latest` being the
/// engine's.
pub fn unknown(latest: i64) -> SdkValue {
    SdkValue::map([
        ("version", SdkValue::Null),
        ("latest", SdkValue::Int(latest)),
        ("status", SdkValue::text(UNKNOWN)),
        ("fix", SdkValue::Null),
    ])
}

/// The section of the sqlite Store file at `path`, Store `store`'s.
pub fn sqlite_file(path: &Path, store: &str) -> SdkValue {
    if !path.is_file() {
        return unknown(SQLITE_LATEST);
    }
    match sqlite_format(path) {
        Ok(format) => section(&format, store, false),
        Err(_) => unknown(SQLITE_LATEST),
    }
}

/// The section of Store `record` where it lives. `unanswered` holds the
/// machines that did not answer already (a `store list` asks each once).
pub fn of(home: &Path, record: &StoreRecord, unanswered: &mut Vec<String>) -> SdkValue {
    let store = record.name();
    match record.store_ref() {
        Ok(StoreRef::Postgres { .. }) => {
            let read = crate::record::url(record).and_then(|url| {
                let schema = crate::upgrade::postgresql_schema(record)?;
                Ok((url, schema))
            });
            match read.map(|(url, schema)| postgresql_format(&url, &schema)) {
                Ok(Ok(format)) => section(&format, store, true),
                _ => unknown(POSTGRESQL_LATEST),
            }
        }
        Ok(store_ref @ StoreRef::Sqlite { machine, .. }) => {
            if unanswered.iter().any(|gone| gone == machine) {
                return unknown(SQLITE_LATEST);
            }
            let read = match place::plan(home, store_ref) {
                Ok(Executor::Local) => return sqlite_file(&paths::store_file(home, store), store),
                Ok(Executor::Worker(mut worker)) => {
                    let request = SdkValue::map([("store", SdkValue::text(store))]);
                    worker.read(FORMAT_OP, request, place::READ)
                }
                Err(refusal) => Err(refusal),
            };
            match read {
                Ok(section) if section.get("status").is_some() => section,
                Ok(_) => unknown(SQLITE_LATEST),
                Err(refusal) => {
                    // a worker of an earlier build answers the op unknown;
                    // only a machine that did not answer is not asked again
                    if refusal.code == "Unavailable" {
                        unanswered.push(machine.clone());
                    }
                    unknown(SQLITE_LATEST)
                }
            }
        }
        // a kind this build does not read has no format it can name
        Err(_) => SdkValue::Null,
    }
}
