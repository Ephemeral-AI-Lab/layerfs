//! Hand off one native worker's Store lease before a local maintenance step.
use crate::admin::open::lease::Owner;
use crate::admin::record::intent::{self, WORKER_CLOSE};
use crate::admin::record::store::{self, StoreRecord, StoreRef};
use crate::Related;
use layerfs_home::home::{self, with_field};
use layerfs_home::op::{self, Op, Standing};
use layerfs_home::outcome::refused;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

const WORKER: &str = "close_worker";
/// The authenticated native worker management operation.
pub const CLOSE: &str = "store.close";

/// The compatible worker observed before taking Store or home locks.
pub(crate) struct Worker {
    owner: Owner,
    generation: String,
}

pub(crate) fn local(home: &Path, record: &StoreRecord) -> Result<bool> {
    match record.store_ref()? {
        StoreRef::Sqlite { machine, .. } => layerfs_machine::runs_here(home, machine),
        StoreRef::Postgres { .. } => Ok(false),
    }
}

/// Looks for an existing worker without starting one. An unreachable
/// worker is never exempt from the ordinary holder checks.
pub(crate) fn observe(home: &Path, record: &StoreRecord) -> Option<Worker> {
    if !local(home, record).ok()? {
        return None;
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let paths = layerfs_machine::worker::HostPaths::new(home).ok()?;
        let answer = paths
            .call_existing(true, "ping", SdkValue::map([]))
            .ok()??;
        let generation = answer.get_text("generation")?;
        if generation.is_empty() {
            return None;
        }
        Some(Worker {
            owner: Owner::decode(&answer)?,
            generation: generation.to_owned(),
        })
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

impl Worker {
    /// Under the Store lock, identify the sole lease eligible for exemption.
    pub(crate) fn holder<'a>(
        worker: Option<&'a Self>,
        home: &Path,
        name: &str,
        record: &StoreRecord,
    ) -> Result<Option<&'a Owner>> {
        match worker {
            Some(worker) if local(home, record)? && worker.owner.holds(home, name)? => {
                Ok(Some(&worker.owner))
            }
            _ => Ok(None),
        }
    }

    pub(crate) fn fence(&self, op: &Op) -> Op {
        let worker = with_field(
            self.owner.to_value(),
            "generation",
            SdkValue::text(&self.generation),
        );
        op.clone()
            .at(WORKER_CLOSE)
            .with_args(with_field(op.args().clone(), WORKER, worker))
            .as_uncertain()
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn close(&self, home: &Path, name: &str, op: &Op) -> Result<()> {
        let paths = layerfs_machine::worker::HostPaths::new(home)?;
        let args = SdkValue::map([
            ("store", SdkValue::text(name)),
            ("op", SdkValue::text(op.id())),
        ]);
        paths.call_existing_generation(CLOSE, args, &self.generation)?;
        Ok(())
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    fn close(&self, _home: &Path, _name: &str, _op: &Op) -> Result<()> {
        unreachable!("host worker observation is unavailable")
    }
}

/// A new explicit invocation can settle its earlier close without sending
/// anything. Keep the recorded fence while that identity still holds the
/// Store, even if the process has no accepting endpoint. A record this
/// build does not read names no machine a worker could hold: the call
/// itself refuses it, or `store remove` forgets it by its records.
pub(crate) fn recover(home: &Path, name: &str, call: &str, to: Option<&str>) -> Result<()> {
    let initial = StoreRecord::of(home, name)?;
    if initial.store_ref().is_err() {
        return Ok(());
    }
    initial.admit_machine(home)?;
    if initial.op().is_none_or(|op| op.phase() != WORKER_CLOSE) {
        return Ok(());
    }
    let _store = store::lock(home, name)?;
    let record = StoreRecord::of(home, name)?;
    record.admit_machine(home)?;
    let Some(held) = record.op().filter(|op| op.phase() == WORKER_CLOSE) else {
        return Ok(());
    };
    if held.name() != call
        || held.args().get_text("to") != to
        || held.standing() == Standing::Running
    {
        return record.unheld();
    }
    let owner = held.args().get(WORKER).and_then(Owner::decode);
    if let Some(owner) = owner {
        if owner.released(home, name)? {
            return record.without(op::FIELD).write(home);
        }
    }
    record.unheld()
}

/// Send one close after the record has been fenced and the locks released.
/// Return with the original operation restored only after proving release.
pub(crate) fn finish(
    home: &Path,
    name: &str,
    op: &Op,
    worker: Option<&Worker>,
    related: Option<Related>,
    previous: Option<&Op>,
) -> Result<()> {
    let Some(worker) = worker else { return Ok(()) };
    let closed = worker.close(home, name, op).map_err(|error| {
        if error.code == "InUse" {
            worker_in_use(error, &worker.owner)
        } else {
            error
        }
    });
    if let Err(refusal) = &closed {
        if refusal.code == "Unknown" {
            intent::leave(home, name, &worker.fence(op), &refusal.message);
            return closed;
        }
    }
    let settled = closed.and_then(|()| {
        let _home = home::lock::take(home)?;
        let _store = store::lock(home, name)?;
        let record = StoreRecord::of(home, name)?;
        let held = ensures_fence(&record, op.id())?;
        if !held.mine() {
            return Err(refused("Conflict", "the Store's maintenance owner changed"));
        }
        if !local(home, &record)? {
            return Err(refused("Conflict", format!("Store {name} moved meanwhile")));
        }
        if !worker.owner.released(home, name)? {
            let error = refused("InUse", format!("Store {name} is still open"));
            return Err(worker_in_use(error, &worker.owner));
        }
        if let Some(related) = related {
            related.check_idle(home, name, None)?;
        }
        record.with(op::FIELD, op.to_record().clone()).write(home)
    });
    if settled.is_err() {
        let _ = intent::replace(home, name, op.id(), previous);
    }
    settled
}

fn worker_in_use(mut error: SdkRefusal, owner: &Owner) -> SdkRefusal {
    error.message = format!("{} (host worker process {})", error.message, owner.pid());
    error.detail = with_field(error.detail, "worker", owner.to_value());
    error
}

fn ensures_fence(record: &StoreRecord, id: &str) -> Result<Op> {
    record
        .op()
        .filter(|op| op.id() == id && op.phase() == WORKER_CLOSE && op.owner_alive())
        .ok_or_else(|| refused("Conflict", "the Store's worker-close intent changed"))
}

/// Validates a close against the real home and record, inside the same
/// Store lock that admits new calls. Cache I/O happens once past that lock.
pub fn close_at(
    home: &Path,
    generation: &str,
    args: &SdkValue,
) -> Result<crate::admin::open::cache::CloseReport> {
    let name = args.get_text("store").unwrap_or_default();
    layerfs_home::names::StoreName::parse(name)?;
    let id = args
        .get_text("op")
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            refused(
                "InvalidInput",
                "store.close requires its maintenance intent",
            )
        })?;
    crate::admin::open::cache::close_host(home, name, |record| {
        let held = ensures_fence(record, id)?;
        let worker = held.args().get(WORKER).unwrap_or(&SdkValue::Null);
        if !local(home, record)?
            || !matches!(
                held.name(),
                intent::MOVE | intent::REMOVE | intent::COMPACT | intent::UPGRADE
            )
            || worker.get_text("generation") != Some(generation)
            || !Owner::decode(worker).is_some_and(|owner| owner.mine())
        {
            return Err(refused(
                "Conflict",
                "the Store is not fenced for this host worker",
            ));
        }
        Ok(())
    })
}
