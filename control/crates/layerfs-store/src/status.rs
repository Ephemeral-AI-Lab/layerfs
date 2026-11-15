//! `store.status`: who holds a Store, the operation on it, its size where it lives.
use crate::lease;
use crate::place;
use crate::record::StoreRef;
use crate::sqlite::family::{fields_map, record_of, step_on};
use crate::sqlite::transfer::step::StoreStep;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use std::path::Path;

/// `store.status`: who holds it, the operation on it (`op`, as H6 shows it),
/// its size. It starts no machine: a Store whose machine is not running is
/// `down`, with the reason (O34). It changes nothing (#536 §12 item 3): no
/// lock, and a stale lease is ignored, not removed.
///
/// While a compaction runs, or after a dead owner left one, `op` is that run's
/// projection and `state` is `busy`/`unresolved` (§11); `phases` names the
/// backend's ordered phases. The compaction record is read from
/// `<store>.compact-op` alone: the Store database and its artifacts are never
/// opened for it.
pub fn status(home: &Path, name: &str) -> Outcome {
    let record = record_of(home, name)?;
    let (holders, _) = lease::read_holders(home, name)?;
    let mut down = None;
    let mut more = Vec::new();
    let store_ref = record.store_ref()?;
    let mut compaction = SdkValue::Null;
    if let StoreRef::Postgres { .. } = store_ref {
        more.push(("server", crate::postgresql::connect::status(&record)));
        // A PostgreSQL compaction's op lives in the Store's `gc_op`; T9 reads
        // it here with its rebuild. This path reads no file.
    } else {
        more.push(("machine", SdkValue::text(store_ref.machine()?)));
        // the file's size where it lives, starting nothing (P4), and the
        // compaction record beside it; a machine that is down says why
        let mut placed = place::plan(home, store_ref);
        let (file, op) = match placed.as_mut() {
            Ok(executor) => (
                step_on(home, executor, name, StoreStep::Stat),
                step_on(home, executor, name, StoreStep::CompactOp).unwrap_or_default(),
            ),
            Err(why) => (Err(why.clone()), SdkValue::Null),
        };
        compaction = op;
        let file = file.unwrap_or_else(|why| {
            down = Some(why.message.clone());
            SdkValue::map([("unavailable", SdkValue::Text(why.message))])
        });
        more.push(("file", file));
    }
    // A compaction's record is the operation the Store is running, even when
    // some other process holds its lease; otherwise the record's own `op`
    // (a push, pull or remove) is what `store.status` shows.
    let op = if compaction.get("id").is_some() {
        let field = |field: &str| compaction.get(field).cloned().unwrap_or_default();
        SdkValue::map([
            ("id", field("id")),
            ("name", field("name")),
            ("phase", field("phase")),
            ("started", field("started")),
            ("uncertain", field("uncertain")),
        ])
    } else {
        record.op().map(|op| op.to_sdk()).unwrap_or_default()
    };
    let phases = compaction.get("phases").cloned().unwrap_or_default();
    let state = compaction
        .get_text("state")
        .unwrap_or_else(|| record.state(down.is_some(), &holders).as_str());
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
        ("op", op),
        ("phases", phases),
        ("down_reason", down.map(SdkValue::Text).unwrap_or_default()),
    ];
    fields.extend(more);
    Ok(fields_map(fields))
}
