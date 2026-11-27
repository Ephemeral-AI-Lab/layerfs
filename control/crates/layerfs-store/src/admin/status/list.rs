//! `store.list` (call surface 2.0): every Store, from the home's records.
//! Each item is [`crate::get::summary`] in its records-only state; each
//! machine's state is derived once for all its Stores. Nothing is probed
//! unless `--with stats` asks for sizes or `--with format` for formats.
use crate::admin::record::store::StoreRecord;
use crate::admin::route::executor;
use crate::admin::status::get::summary;
use crate::admin::status::state;
use crate::backend::sqlite::family::step_on;
use crate::backend::sqlite::step_request::StoreStep;
use layerfs_home::home::{observed, paths};
use layerfs_home::outcome::Outcome;
use layerfs_home::state::State;
use layerfs_protocol::sdk::SdkValue;
use std::collections::BTreeMap;
use std::path::Path;

/// The bytes of a sqlite Store's file with its write-ahead log, from one
/// `stat` on its machine, starting nothing; none for a PostgreSQL Store
/// and for a machine that does not answer (which is not asked again for
/// its other Stores).
fn size(home: &Path, record: &StoreRecord, unanswered: &mut Vec<String>) -> SdkValue {
    let Some(machine) = state::placed(record) else {
        return SdkValue::Null;
    };
    if unanswered.iter().any(|gone| gone == machine) {
        return SdkValue::Null;
    }
    let stat = (record.store_ref())
        .and_then(|store_ref| executor::plan(home, store_ref))
        .and_then(|mut on| step_on(home, &mut on, record.name(), StoreStep::Stat));
    match stat {
        Ok(stat) if stat.get("exists") == Some(&SdkValue::Bool(true)) => {
            let bytes = stat.get_int("bytes").unwrap_or(0);
            SdkValue::Int(bytes.saturating_add(stat.get_int("wal_bytes").unwrap_or(0)))
        }
        Ok(_) => SdkValue::Null,
        Err(_) => {
            unanswered.push(machine.to_owned());
            SdkValue::Null
        }
    }
}

/// `store.list`: every Store, or with `machine` only the sqlite Stores on
/// that machine, each with its records-only state; with `stats`, each
/// item's `size`.
pub fn list(home: &Path, machine: Option<&str>, stats: bool, machines: state::Machines) -> Outcome {
    list_with(home, machine, stats, false, machines)
}

/// [`list`], with `format` each item's `format` section (#804,
/// [`crate::formats`]), read without opening the Store; a machine that did
/// not answer is not asked again for its other Stores.
pub fn list_with(
    home: &Path,
    machine: Option<&str>,
    stats: bool,
    format: bool,
    machines: state::Machines,
) -> Outcome {
    let mut derived: BTreeMap<String, State> = BTreeMap::new();
    let mut unanswered = Vec::new();
    // the machines a format read did not reach: kept apart from the size
    // reads', where any failure skips a machine's later Stores
    let mut unformatted = Vec::new();
    let mut items = Vec::new();
    for record in crate::admin::record::store::stores(home)? {
        let on = state::placed(&record);
        if machine.is_some_and(|wanted| on != Some(wanted)) {
            continue;
        }
        let under = match on {
            Some(on) => {
                if !derived.contains_key(on) {
                    derived.insert(on.to_owned(), machines(home, on)?);
                }
                derived.get(on)
            }
            None => None,
        };
        let state = state::recorded_under(&record, under)?;
        let observed = observed::read(&paths::store_record(home, record.name()));
        let mut item = summary(home, &record, &state, observed, false)?;
        if let (true, SdkValue::Map(fields)) = (stats, &mut item) {
            fields.push(("size".to_owned(), size(home, &record, &mut unanswered)));
        }
        if let (true, SdkValue::Map(fields)) = (format, &mut item) {
            let section = crate::admin::status::formats::of(home, &record, &mut unformatted);
            fields.push(("format".to_owned(), section));
        }
        items.push(item);
    }
    Ok(SdkValue::map([("items", SdkValue::List(items))]))
}
