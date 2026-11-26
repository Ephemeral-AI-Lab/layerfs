//! `store.remove`: a sqlite Store deleted wherever it is, with its projects.
use crate::place::{self, Executor};
use crate::record::{self, StoreRef};
use crate::sqlite::family::{advance, begin_maintenance, ended, settle, step_on};
use crate::sqlite::transfer::step::StoreStep;
use crate::Related;
use crate::{host_maintenance, intent};
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::{self, paths};
use layerfs_home::op::{Op, Standing};
use layerfs_home::outcome::{failed, refused, Outcome};
use std::path::Path;

/// `store.remove`: deletes a sqlite Store wherever it is, with every project
/// in it. Its backups stay in their buckets. A stopped machine is started
/// before the locks are taken, never under them. It is also the call that
/// clears an unresolved operation on the Store: a gone process's intent
/// does not refuse it; a live one's is `Busy`. A Store whose record this
/// build does not read, or on a machine whose record has no 2.0 name, goes
/// by its records alone, and `left` says what may remain.
pub fn remove(home: &Path, name: &str, related: Related) -> Outcome {
    host_maintenance::recover(home, name, intent::REMOVE, None)?;
    let record = record::StoreRecord::of(home, name)?;
    let Ok(store_ref) = record.store_ref() else {
        let left = record::forget_unread(home, name, related)?;
        return Ok(answer(name, left));
    };
    if let StoreRef::Postgres { .. } = store_ref {
        return Err(refused(
            "InvalidInput",
            format!("Store {name} is a PostgreSQL database; `layerfs store disconnect {name}` forgets it and keeps its data"),
        ));
    }
    let machine = store_ref.machine()?.to_owned();
    if nameless(home, &machine)? {
        let left = forget_on_nameless(home, name, &machine, related)?;
        return Ok(answer(name, left));
    }
    let mut executor = place::plan(home, store_ref)?;
    if let Executor::Worker(worker) = &mut executor {
        worker.ready()?;
    }
    // the copy an ended move left on the other machine, if the
    // record names one: it goes too, and `op.args.leftover` records it so a
    // second `store.remove` finishes an interrupted one
    let left = crate::sqlite::transfer::resolve::leftover(&record);
    let mut leftover_executor = match &left {
        Some(left) if left.on != machine => {
            let mut ex = place::on_machine(home, &left.on)?;
            if let Executor::Worker(worker) = &mut ex {
                worker.ready()?;
            }
            Some((left.clone(), ex))
        }
        _ => None,
    };
    let mut fields = vec![("machine", SdkValue::text(&machine))];
    if let Some((left, _)) = &leftover_executor {
        fields.push(("leftover", SdkValue::text(&left.on)));
    }
    let args = layerfs_home::helpers::map_of(fields);
    let mut op = Op::begin(intent::REMOVE, "check", args)?;
    let worker = host_maintenance::observe(home, &record);
    let (previous, worker) = begin_maintenance(home, name, &op, related, worker, |record| {
        if record.machine()? != machine {
            return Err(refused(
                "Conflict",
                format!("Store {name} moved off {machine} meanwhile; nothing was removed"),
            ));
        }
        let now_left =
            crate::sqlite::transfer::resolve::leftover(record).filter(|left| left.on != machine);
        let want_left = leftover_executor.as_ref().map(|(left, _)| left);
        if now_left.as_ref() != want_left {
            return Err(refused(
                "Conflict",
                format!("Store {name} changed meanwhile; nothing was removed"),
            ));
        }
        let previous = record.op();
        if previous.as_ref().map(Op::standing) == Some(Standing::Running) {
            record.unheld()?;
        }
        Ok(previous.filter(|held| held.standing() == Standing::Unresolved))
    })?;
    host_maintenance::finish(
        home,
        name,
        &op,
        worker.as_ref(),
        Some(related),
        previous.as_ref(),
    )?;
    // a machine whose Stores open in this process keeps the file under the
    // home: it goes with the Store's directory below
    let mut phase = "check";
    let removed = settle(home, name, related, &op).and_then(|lease| {
        drop(lease);
        // a crash from here on leaves the file's fate unknown
        phase = "remove";
        let unknown = op.clone().as_uncertain();
        op = unknown;
        advance(home, name, &mut op, phase)?;
        if let Some((_, ex)) = &mut leftover_executor {
            if let Executor::Worker(_) = ex {
                step_on(home, ex, name, StoreStep::Remove)?;
            }
        }
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
    layerfs_home::home::generation::moved(home);
    Ok(answer(name, Vec::new()))
}

/// What a remove answers: `left` (each sentence about what may remain)
/// only when it took records alone.
fn answer(name: &str, left: Vec<String>) -> SdkValue {
    let mut fields = vec![
        ("outcome", SdkValue::text("removed")),
        ("store", SdkValue::text(name)),
    ];
    if !left.is_empty() {
        let left = left.into_iter().map(SdkValue::Text).collect();
        fields.push(("left", SdkValue::List(left)));
    }
    layerfs_home::helpers::map_of(fields)
}

/// Whether machine `machine`'s record has no 2.0 name (RV3 FAIL 1), or a
/// live read found its VM gone (area 10): this build reaches nothing on
/// it, so a Store on it goes by its records alone.
fn nameless(home: &Path, machine: &str) -> Result<bool, layerfs_bridge::control::SdkRefusal> {
    let record = layerfs_machine::record::MachineRecord::read(home, machine)?;
    Ok(record.is_some_and(|record| record.gone()))
}

/// Forgets sqlite Store `name` on machine `machine`, whose record has no
/// 2.0 name (RV3 FAIL 1): under the home lock and the Store's, once
/// nothing uses it ([`Related::store_idle`]) and no live operation holds
/// it, its folder in the home goes (its record, its projects' records and
/// leases). Nothing on the machine is reached or touched; the answer says
/// what may remain there, and on the machine an ended move left a copy on.
fn forget_on_nameless(
    home: &Path,
    name: &str,
    machine: &str,
    related: Related,
) -> Result<Vec<String>, layerfs_bridge::control::SdkRefusal> {
    let _lock = home::lock::take(home)?;
    let _store = record::lock(home, name)?;
    let now = record::StoreRecord::of(home, name)?;
    if now.machine().ok().as_deref() != Some(machine) || !nameless(home, machine)? {
        return Err(refused(
            "Conflict",
            format!("Store {name} changed meanwhile; nothing was removed"),
        ));
    }
    if now.op().as_ref().map(Op::standing) == Some(Standing::Running) {
        now.unheld()?;
    }
    (related.store_idle)(home, name)?;
    let leftover = crate::sqlite::transfer::resolve::leftover(&now).filter(|l| l.on != machine);
    std::fs::remove_dir_all(paths::store_dir(home, name)).map_err(failed)?;
    layerfs_home::home::generation::moved(home);
    let on = layerfs_machine::record::MachineRecord::read(home, machine)?;
    let mut left = vec![match on.is_some_and(|record| record.nameless()) {
        true => format!(
            "machine {machine}'s record has no 2.0 name, so only Store {name}'s records were \
             removed: its file stays on that machine's disk, in the VM or container an earlier \
             build made, which no call reaches"
        ),
        false => format!(
            "machine {machine}'s VM is gone, so only Store {name}'s records were removed: its \
             file was on that VM's disk, which its provider no longer holds"
        ),
    }];
    if let Some(leftover) = leftover {
        left.push(format!(
            "the copy an ended move left on machine {} was not touched either",
            leftover.on
        ));
    }
    Ok(left)
}
