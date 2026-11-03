//! `store.remove`: a sqlite Store deleted wherever it is, with its projects.
use super::family::{ended, idle, in_progress, of_store, record_of, settle, step};
use crate::common::home::{self, paths};
use crate::common::outcome::{failed, refused, Outcome};
use crate::common::place::{self, Executor};
use crate::store::record::StoreRef;
use layerfs_bridge::control::SdkValue;
use std::path::Path;

/// `store.remove`: deletes a sqlite Store wherever it is, with every project
/// in it. Its backups stay in their buckets. A stopped machine is started
/// before the home lock is taken, never under it.
pub(super) fn remove(home: &Path, name: &str) -> Outcome {
    let record = record_of(home, name)?;
    let store_ref = record.store_ref()?;
    if let StoreRef::Postgres { .. } = store_ref {
        return Err(refused(
            "InvalidInput",
            format!("Store {name} is a PostgreSQL database; `store disconnect` forgets it and keeps its data"),
        ));
    }
    let machine = store_ref.machine()?.to_owned();
    let mut executor = place::plan(home, store_ref)?;
    if let Executor::Worker(worker) = &mut executor {
        worker.ready()?;
    }
    {
        let _lock = home::lock::take(home)?;
        let record = record_of(home, name)?;
        if record.machine()? != machine {
            return Err(refused(
                "Conflict",
                format!("Store {name} moved off {machine} meanwhile; nothing was removed"),
            ));
        }
        idle(home, name, &record)?;
        let at = in_progress("remove", vec![("machine", SdkValue::text(&machine))]);
        record.with("in_progress", at).write(home)?;
    }
    // a machine whose Stores open in this process keeps the file under the
    // home: it goes with the Store's directory below
    let removed = settle(home, name).and_then(|lease| {
        drop(lease);
        match executor {
            Executor::Local => Ok(SdkValue::Null),
            Executor::Worker(_) => step(home, &mut executor, "remove", of_store(name, vec![])),
        }
    });
    if let Err(refusal) = removed {
        let uncertain = refusal.code == "Unknown";
        ended(home, "store.remove", name, "remove", &refusal, uncertain);
        return Err(refusal);
    }
    let _lock = home::lock::take(home)?;
    std::fs::remove_dir_all(paths::store_dir(home, name)).map_err(failed)?;
    Ok(SdkValue::map([("removed", SdkValue::text(name))]))
}
