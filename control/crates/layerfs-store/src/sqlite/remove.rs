//! `store.remove`: a sqlite Store deleted wherever it is, with its projects.
use crate::place::{self, Executor};
use crate::record::{self, StoreRef};
use crate::sqlite::family::{advance, begin, ended, record_of, settle, step_on};
use crate::sqlite::transfer::step::StoreStep;
use crate::Related;
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::{self, paths};
use layerfs_home::op::{Op, Standing};
use layerfs_home::outcome::{failed, refused, Outcome};
use std::path::Path;

/// `store.remove`: deletes a sqlite Store wherever it is, with every project
/// in it. Its backups stay in their buckets. A stopped machine is started
/// before the locks are taken, never under them. It is also the call that
/// clears an unresolved operation on the Store: a gone process's intent
/// does not refuse it; a live one's is `Busy`.
pub fn remove(home: &Path, name: &str, related: Related) -> Outcome {
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
    let args = SdkValue::map([("machine", SdkValue::text(&machine))]);
    let mut op = Op::begin("store.remove", "check", args)?;
    let previous = begin(home, name, &op, related, |record| {
        if record.machine()? != machine {
            return Err(refused(
                "Conflict",
                format!("Store {name} moved off {machine} meanwhile; nothing was removed"),
            ));
        }
        let previous = record.op();
        if previous.as_ref().map(Op::standing) == Some(Standing::Running) {
            record.unblocked()?;
        }
        Ok(previous.filter(|held| held.standing() == Standing::Unresolved))
    })?;
    // a machine whose Stores open in this process keeps the file under the
    // home: it goes with the Store's directory below
    let mut phase = "check";
    let removed = settle(home, name, related).and_then(|lease| {
        drop(lease);
        // a crash from here on leaves the file's fate unknown
        phase = "remove";
        let unknown = op.clone().as_uncertain();
        op = unknown;
        advance(home, name, &mut op, phase)?;
        match executor {
            Executor::Local => Ok(SdkValue::Null),
            Executor::Worker(_) => step_on(home, &mut executor, name, StoreStep::Remove),
        }
    });
    if let Err(refusal) = removed {
        let uncertain = refusal.code == "Unknown";
        ended(
            home,
            name,
            &op,
            phase,
            &refusal,
            uncertain,
            previous.as_ref(),
        );
        return Err(refusal);
    }
    let _lock = home::lock::take(home)?;
    let _store = record::lock(home, name)?;
    std::fs::remove_dir_all(paths::store_dir(home, name)).map_err(failed)?;
    Ok(SdkValue::map([("removed", SdkValue::text(name))]))
}
