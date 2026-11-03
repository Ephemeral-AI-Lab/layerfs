//! `store.status`: who holds a Store, the operation on it, its size where it lives.
use super::family::{fields_map, of_store, record_of, step};
use crate::common::home::lease;
use crate::common::outcome::Outcome;
use crate::common::place;
use crate::store::record;
use crate::store::record::StoreRef;
use layerfs_bridge::control::SdkValue;
use std::path::Path;

/// `store.status`: who holds it, the operation its record holds (`op`, as
/// H6 shows it), its size. It starts no machine: a Store whose machine is
/// not running is `down`, with the reason (O34).
pub(super) fn status(home: &Path, name: &str) -> Outcome {
    let record = record_of(home, name)?;
    let holders = {
        let _store = record::lock(home, name)?;
        lease::holders(home, name)?
    };
    let mut down = None;
    let mut more = Vec::new();
    let store_ref = record.store_ref()?;
    if let StoreRef::Postgres { .. } = store_ref {
        more.push(("server", crate::store::connect::status(&record)));
    } else {
        more.push(("machine", SdkValue::text(store_ref.machine()?)));
        // the file's size where it lives, starting nothing (P4); a machine
        // that is down says why
        let file = place::plan(home, store_ref)
            .and_then(|mut executor| step(home, &mut executor, "stat", of_store(name, vec![])));
        let file = file.unwrap_or_else(|why| {
            down = Some(why.message.clone());
            SdkValue::map([("unavailable", SdkValue::Text(why.message))])
        });
        more.push(("file", file));
    }
    let state = record.state(down.is_some(), &holders).as_str();
    let mut fields = vec![
        ("name", SdkValue::text(name)),
        ("kind", SdkValue::text(store_ref.kind().as_str())),
        ("state", SdkValue::text(state)),
        (
            "holders",
            SdkValue::List(
                holders
                    .iter()
                    .map(|p| SdkValue::Int(i64::from(*p)))
                    .collect(),
            ),
        ),
        ("op", record.op().map(|op| op.to_sdk()).unwrap_or_default()),
        ("down_reason", down.map(SdkValue::Text).unwrap_or_default()),
    ];
    fields.extend(more);
    Ok(fields_map(fields))
}
