//! `store get --with stats` (call surface 2.0, was `store.stats`): the
//! Store's counters, read by opening it on its existing in-process or
//! worker route, and its size where it lives.
use crate::calls::StoreOp;
use crate::place;
use crate::record::{StoreRecord, StoreRef};
use crate::resolve::Target;
use crate::sqlite::family::step_on;
use crate::sqlite::transfer::step::StoreStep;
use crate::{call, postgresql};
use layerfs_protocol::provision::install::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// `numerator / denominator` as text with six decimals; null for none.
fn ratio(numerator: Option<i64>, denominator: Option<i64>) -> SdkValue {
    match (numerator, denominator) {
        (Some(n), Some(d)) if d > 0 => SdkValue::text(format!("{:.6}", n as f64 / d as f64)),
        _ => SdkValue::Null,
    }
}

/// The size of Store `record` where it lives: a sqlite file with its
/// write-ahead log from one `stat` on its machine; a PostgreSQL Store's
/// schema as its server counts it. Null when it cannot be told.
fn size(home: &Path, record: &StoreRecord) -> SdkValue {
    match record.store_ref() {
        Ok(StoreRef::Postgres { .. }) => {
            let status = postgresql::connect::status(record);
            match status.get("schemas") {
                Some(SdkValue::Map(schemas)) => schemas
                    .first()
                    .map(|(_, bytes)| bytes.clone())
                    .unwrap_or_default(),
                _ => SdkValue::Null,
            }
        }
        Ok(store_ref) => {
            let stat = place::plan(home, store_ref)
                .and_then(|mut on| step_on(home, &mut on, record.name(), StoreStep::Stat));
            match stat {
                Ok(stat) => {
                    let bytes = stat.get_int("bytes").unwrap_or(0);
                    SdkValue::Int(bytes.saturating_add(stat.get_int("wal_bytes").unwrap_or(0)))
                }
                Err(_) => SdkValue::Null,
            }
        }
        Err(_) => SdkValue::Null,
    }
}

/// The `stats` section of Store `record`: `{ size, logical_bytes,
/// canonical_bytes, stored_bytes, metadata_bytes, file_bytes,
/// reusable_bytes, dedupe, compression, init }`. It opens the Store, as a
/// Store call does: refused while a move or a remove holds it.
pub fn stats(home: &Path, events: &mut Events<'_>, record: &StoreRecord) -> Result<SdkValue> {
    let target = Target {
        store: record.name().to_owned(),
        // Store-wide methods do not resolve or require any project.
        project: String::new(),
        store_ref: record.store_ref()?.clone(),
        record: record.clone(),
    };
    let postgresql = matches!(target.store_ref, StoreRef::Postgres { .. });
    let counters = call::store_call(home, events, &target, &StoreOp::Statistics)?;
    // the engine names its counters in camelCase; the call boundary
    // renames them, so either spelling is read
    let int = |value: &SdkValue, names: &[&str]| names.iter().find_map(|name| value.get_int(name));
    let logical = int(&counters, &["historyLogical", "history_logical", "logical"]);
    let canonical = int(&counters, &["canonical"]);
    let stored = int(&counters, &["stored"]);
    let of = |name: &str| counters.get(name).cloned().unwrap_or_default();
    let init = match counters.get("init") {
        Some(init @ SdkValue::Map(_)) => {
            let copied = |name: &str| init.get(name).cloned().unwrap_or_default();
            SdkValue::map([
                ("entries", copied("entries")),
                ("files", copied("files")),
                ("size", copied("bytes")),
                ("canonical_bytes", copied("canonical")),
                ("stored_bytes", copied("stored")),
                ("metadata_bytes", copied("metadata")),
                ("duration_ms", copied("millis")),
            ])
        }
        _ => SdkValue::Null,
    };
    Ok(SdkValue::map([
        ("size", size(home, record)),
        (
            "logical_bytes",
            logical.map_or(SdkValue::Null, SdkValue::Int),
        ),
        ("canonical_bytes", of("canonical")),
        ("stored_bytes", of("stored")),
        ("metadata_bytes", of("metadata")),
        ("file_bytes", of("file")),
        (
            "reusable_bytes",
            if postgresql {
                SdkValue::Null
            } else {
                of("free")
            },
        ),
        ("dedupe", ratio(logical, canonical)),
        ("compression", ratio(canonical, stored)),
        ("init", init),
    ]))
}
