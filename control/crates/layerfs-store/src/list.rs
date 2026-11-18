//! `store.list` (call surface 2.0): every Store, from the home's records.
//! Each item is [`crate::get::summary`] in its records-only state; each
//! machine's state is derived once for all its Stores. Nothing is probed
//! unless `--with stats` asks for sizes.
use crate::get::summary;
use crate::observed;
use crate::place;
use crate::record::{Placement, StoreRecord, StoreRef};
use crate::sqlite::family::step_on;
use crate::sqlite::transfer::step::StoreStep;
use crate::state;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_home::state::State;
use std::collections::BTreeMap;
use std::path::Path;

/// The machine a sqlite Store's record places it on, if it tells one.
fn machine_of(record: &StoreRecord) -> Option<&str> {
    match record.store_ref() {
        Ok(StoreRef::Sqlite {
            machine: Placement::Machine(machine),
            ..
        }) => Some(machine),
        _ => None,
    }
}

/// The bytes of a sqlite Store's file with its write-ahead log, from one
/// `stat` on its machine, starting nothing; none for a PostgreSQL Store
/// and for a machine that does not answer (which is not asked again for
/// its other Stores).
fn size(home: &Path, record: &StoreRecord, unanswered: &mut Vec<String>) -> SdkValue {
    let Some(machine) = machine_of(record) else {
        return SdkValue::Null;
    };
    if unanswered.iter().any(|gone| gone == machine) {
        return SdkValue::Null;
    }
    let stat = (record.store_ref())
        .and_then(|store_ref| place::plan(home, store_ref))
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
pub fn list(home: &Path, machine: Option<&str>, stats: bool) -> Outcome {
    let mut machines: BTreeMap<String, State> = BTreeMap::new();
    let mut unanswered = Vec::new();
    let mut items = Vec::new();
    for record in crate::record::stores(home)? {
        let on = machine_of(&record);
        if machine.is_some_and(|wanted| on != Some(wanted)) {
            continue;
        }
        let under = match on {
            Some(on) => {
                if !machines.contains_key(on) {
                    machines.insert(on.to_owned(), state::machine_state(home, on)?);
                }
                machines.get(on)
            }
            None => None,
        };
        let state = state::recorded_under(home, &record, under)?;
        let observed = observed::read(home, record.name());
        let mut item = summary(home, &record, &state, observed)?;
        if let (true, SdkValue::Map(fields)) = (stats, &mut item) {
            fields.push(("size".to_owned(), size(home, &record, &mut unanswered)));
        }
        items.push(item);
    }
    Ok(SdkValue::map([("items", SdkValue::List(items))]))
}
