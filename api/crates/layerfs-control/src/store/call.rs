//! The route of one Store call, chosen from records (H §7.7): a PostgreSQL
//! Store and a sqlite Store on `native` run in this process, under its
//! lease; a sqlite Store
//! on any other machine runs in that machine's worker, started first if it
//! is not running. The caller sends the worker the Store's record with the
//! call, so a guest never reads the home; the worker finds the Store file
//! under its own root.
use crate::common::context::Context;
use crate::common::home::lease;
use crate::common::home::paths;
use crate::common::outcome::Outcome;
use crate::common::place::{plan, Executor};
use crate::store::open;
use crate::store::record::{StoreRecord, StoreRef};
use crate::store::resolve::Target;
use layerfs_bridge::control::SdkValue;
use std::path::Path;

/// Runs Store method `op` with `args` on the Store of `target`.
pub(crate) fn store_call(
    context: &mut Context<'_>,
    target: &Target,
    op: &str,
    args: Vec<(String, SdkValue)>,
) -> Outcome {
    let home = context.home.clone();
    target.record.unblocked()?;
    // the lease is taken under the Store's lock with the record read there:
    // a move or remove that started since `target` was read refuses it
    let (lease, record) = lease::claim(&home, &target.store)?;
    if let StoreRef::Postgres { .. } = target.store_ref {
        let fields = with_target(args, target, &record);
        return postgresql(&home, target, op, fields, lease);
    }
    let mut fields = with_target(args, target, &record);
    // the record read under the lock, parsed once (R4)
    match plan(&home, record.store_ref()?)? {
        Executor::Local => {
            let file = paths::store_file(&home, &target.store);
            fields.push((
                "storePath".to_owned(),
                SdkValue::text(file.display().to_string()),
            ));
            open::run(op, SdkValue::Map(fields), Some(lease))
        }
        Executor::Worker(mut worker) => {
            // the host process holds the lease while it sends the call
            let _lease = lease;
            let request =
                SdkValue::map([("op", SdkValue::text(op)), ("args", SdkValue::Map(fields))]);
            worker.call("store.call", request, &mut |event| context.event(event))
        }
    }
}

/// A call's arguments with its project, Store and Store record.
fn with_target(
    args: Vec<(String, SdkValue)>,
    target: &Target,
    record: &StoreRecord,
) -> Vec<(String, SdkValue)> {
    let mut fields = args;
    fields.extend([
        ("project".to_owned(), SdkValue::text(&target.project)),
        ("store".to_owned(), SdkValue::text(&target.store)),
        ("store_record".to_owned(), record.to_record().clone()),
    ]);
    fields
}

/// A PostgreSQL Store's call in this process: its URL from the record, the
/// password reference resolved now (H §7.4). Its lease is held while this
/// process keeps the Store open, as a sqlite Store's is: `store.disconnect`
/// refuses `InUse` meanwhile.
fn postgresql(
    home: &Path,
    target: &Target,
    op: &str,
    mut fields: Vec<(String, SdkValue)>,
    lease: lease::Lease,
) -> Outcome {
    let url = crate::common::secrets::url(&target.record)?;
    fields.push((
        "postgresql".to_owned(),
        SdkValue::map([("url", SdkValue::text(url))]),
    ));
    // one open service per PostgreSQL Store, keyed by a path no file has
    let key = paths::store_dir(home, &target.store).join("postgresql");
    fields.push((
        "storePath".to_owned(),
        SdkValue::text(key.display().to_string()),
    ));
    open::run(op, SdkValue::Map(fields), Some(lease))
}
