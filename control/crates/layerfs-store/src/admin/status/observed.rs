//! What was seen when a PostgreSQL Store's database was dialled (spec v23
//! V23-2): `store/<s>/observed.json`, beside the `at` a live read writes
//! there. `here` is this computer's dial of the Store's address (the
//! control process that holds the connection), never a machine's;
//! `reach.<machine>` a machine's, as its Sandboxes dial it. Each entry is
//! `{ at, ok, latency_ms, server_version, error }`, the last one seen.
//!
//! It is not a record: a write never moves the home's generation and never
//! fails the call that made it. Writers run after their Store I/O, under
//! the Store's lock, and write nothing once the Store is gone, so a late
//! write never leaves a folder behind.
use crate::admin::record::store::{lock, StoreKind, StoreRecord};
use layerfs_home::disk::{now_ms, paths, with_field};
use layerfs_home::record::observed;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The key of this computer's reach.
pub const HERE: &str = "here";
/// The key of the machines' reaches, by machine.
pub const REACH: &str = "reach";

/// One reach result.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reach {
    /// Whether the database answered as this Store.
    pub ok: bool,
    /// How long the check took, when it measured it.
    pub latency_ms: Option<i64>,
    /// The server's version, when the check read it.
    pub server_version: Option<String>,
    /// Why it failed, as the refusal said it (never a URL).
    pub error: Option<String>,
}

impl Reach {
    /// A check's answer (`{ at, server_version, latency_ms }`) or refusal.
    pub fn of(checked: &Result<SdkValue>) -> Self {
        match checked {
            Ok(answer) => Self {
                ok: true,
                latency_ms: answer.get_int("latency_ms"),
                server_version: answer.get_text("server_version").map(str::to_owned),
                error: None,
            },
            Err(refusal) => Self::failed(&refusal.message),
        }
    }

    /// Reached, with nothing measured (a Sandbox create's reply).
    pub fn reached() -> Self {
        Self {
            ok: true,
            ..Self::default()
        }
    }

    /// Not reached, for `error`.
    pub fn failed(error: &str) -> Self {
        Self {
            error: Some(error.to_owned()),
            ..Self::default()
        }
    }

    fn to_record(&self, at: i64) -> SdkValue {
        SdkValue::map([
            ("at", SdkValue::Int(at)),
            ("ok", SdkValue::Bool(self.ok)),
            (
                "latency_ms",
                self.latency_ms.map_or(SdkValue::Null, SdkValue::Int),
            ),
            (
                "server_version",
                SdkValue::opt_text(self.server_version.clone()),
            ),
            ("error", SdkValue::opt_text(self.error.clone())),
        ])
    }
}

/// Runs `write` on Store `store`'s record path under its lock, while its
/// record exists; a Store that is gone (or whose lock is not taken) is
/// left alone.
fn locked(home: &Path, store: &str, write: impl FnOnce(&StoreRecord, &Path) -> Result<()>) {
    let Ok(_lock) = lock(home, store) else {
        return;
    };
    if let Ok(Some(record)) = StoreRecord::read(home, store) {
        let _ = write(&record, &paths::store_record(home, store));
    }
}

/// Notes that Store `store` was read live at `ms` (`store get --with
/// live`), keeping its reach observations. A read takes no Store lock, so
/// this merge does not either: it writes only while the Store's record is
/// there, and a reach recorded in the same instant may be lost to it (the
/// next check records it again).
pub fn seen_at(home: &Path, store: &str, ms: i64) {
    let file = paths::store_record(home, store);
    if file.is_file() {
        let _ = observed::write(&file, ms);
    }
}

/// Records what `machine` (none: this computer) saw dialling PostgreSQL
/// Store `store`, best effort.
pub fn record(home: &Path, store: &str, machine: Option<&str>, reach: &Reach) {
    locked(home, store, |record, file| {
        if record.kind()? != StoreKind::Postgresql {
            return Ok(());
        }
        let entry = reach.to_record(now_ms());
        match machine {
            None => observed::merge(file, HERE, entry),
            Some(machine) => {
                let machines = reaches(&observed::fields(file));
                observed::merge(file, REACH, with_field(machines, machine, entry))
            }
        }
    });
}

fn reaches(fields: &SdkValue) -> SdkValue {
    match fields.get(REACH) {
        Some(map @ SdkValue::Map(_)) => map.clone(),
        _ => SdkValue::Map(Vec::new()),
    }
}

/// Forgets `machine`'s reach of Store `store`, the caller holding the
/// Store's lock; whether there was one.
pub fn forget_locked(home: &Path, store: &str, machine: &str) -> Result<bool> {
    let file = paths::store_record(home, store);
    let SdkValue::Map(machines) = reaches(&observed::fields(&file)) else {
        return Ok(false);
    };
    let before = machines.len();
    let kept: Vec<_> = machines.into_iter().filter(|(m, _)| m != machine).collect();
    if kept.len() == before {
        return Ok(false);
    }
    observed::merge(&file, REACH, SdkValue::Map(kept))?;
    Ok(true)
}

/// What was seen of one Store: this computer's reach and each machine's,
/// as answers show them (`at` as a time).
#[derive(Clone, Debug, Default)]
pub struct Seen {
    /// This computer's.
    pub here: Option<SdkValue>,
    /// Each machine's, by name.
    pub machines: Vec<(String, SdkValue)>,
}

impl Seen {
    /// `machine`'s reach, if one was seen.
    pub fn of(&self, machine: &str) -> Option<SdkValue> {
        let mut found = self.machines.iter();
        found.find(|(m, _)| m == machine).map(|(_, r)| r.clone())
    }
}

/// An entry as answers show it: `{ at, ok, latency_ms, server_version,
/// error }`, `at` as a time.
fn shown(entry: &SdkValue) -> SdkValue {
    let copied = |key: &str| entry.get(key).cloned().unwrap_or_default();
    SdkValue::map([
        ("at", crate::admin::status::get::at(entry.get_int("at"))),
        (
            "ok",
            SdkValue::Bool(entry.get("ok") == Some(&SdkValue::Bool(true))),
        ),
        ("latency_ms", copied("latency_ms")),
        ("server_version", copied("server_version")),
        ("error", copied("error")),
    ])
}

/// What was seen of Store `store` in `home`; nothing when its file is
/// absent or cannot be read.
pub fn read(home: &Path, store: &str) -> Seen {
    let fields = observed::fields(&paths::store_record(home, store));
    let machines = match reaches(&fields) {
        SdkValue::Map(machines) => machines,
        _ => Vec::new(),
    };
    Seen {
        here: fields.get(HERE).map(shown),
        machines: (machines.iter())
            .map(|(machine, entry)| (machine.clone(), shown(entry)))
            .collect(),
    }
}

/// The word a reach shows as `reached`: `ok`, `failed`, or null when none
/// was seen.
pub fn reached(reach: Option<&SdkValue>) -> SdkValue {
    match reach.map(|r| r.get("ok") == Some(&SdkValue::Bool(true))) {
        Some(true) => SdkValue::text("ok"),
        Some(false) => SdkValue::text("failed"),
        None => SdkValue::Null,
    }
}
