//! `store.list` (#536 §10b): every Store with its state.
use crate::lease;
use crate::place;
use crate::record::StoreRef;
use crate::sqlite::family::{public, step_on};
use crate::sqlite::transfer::step::StoreStep;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::{Outcome, View};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// `store.list`: every Store with its state, derived as `store.status`
/// derives it (#536 §10b), starting nothing and changing nothing: the live
/// lease holders (a stale lease is ignored, not removed; no lock is taken),
/// and for a sqlite Store whether its machine answers a `stat` of its file. A machine that did not answer once is down for
/// its other Stores too, without dialing it again; a Store whose kind or
/// machine its record does not tell is `down`.
pub fn list(home: &Path) -> Outcome {
    let records = crate::record::stores(home)?;
    let holders = records
        .iter()
        .map(|r| lease::read_holders(home, r.name()).map(|(live, _)| live));
    let holders = holders.collect::<Result<Vec<_>>>()?;
    let mut unanswered = std::collections::BTreeSet::new();
    let mut items = Vec::new();
    for (record, holders) in records.iter().zip(holders) {
        let down = match record.store_ref() {
            Err(_) => true,
            Ok(StoreRef::Postgres { .. }) => false,
            Ok(store_ref) => match store_ref.machine() {
                Err(_) => true,
                Ok(machine) if unanswered.contains(machine) => true,
                Ok(machine) => {
                    let stat = place::plan(home, store_ref)
                        .and_then(|mut on| step_on(home, &mut on, record.name(), StoreStep::Stat));
                    if stat.is_err() {
                        unanswered.insert(machine.to_owned());
                    }
                    stat.is_err()
                }
            },
        };
        let state = record.state(down, &holders);
        items.push(public(home, record, View::List, Some(state))?);
    }
    Ok(SdkValue::map([("items", SdkValue::List(items))]))
}
