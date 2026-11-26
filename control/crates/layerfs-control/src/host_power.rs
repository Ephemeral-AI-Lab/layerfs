//! Native host power fences and holder reporting.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::{
    events::Events,
    home::{lock, with_field},
    names::MachineName,
    outcome::{refused, Outcome},
};
use layerfs_machine::{record::MachineRecord, worker::HostPaths};

use std::path::Path;

fn set(home: &Path, name: &MachineName, stopped: bool) -> Result<bool, SdkRefusal> {
    let _guard = lock::take(home)?;
    let record = MachineRecord::of(home, name.as_str())?;
    let previous = record.recorded_state() == Some("stopped");
    if previous != stopped {
        record
            .with(
                "state",
                SdkValue::text(if stopped { "stopped" } else { "enabled" }),
            )
            .write(home)?;
    }
    Ok(previous != stopped)
}

pub(crate) fn start(home: &Path, name: &MachineName) -> Outcome {
    let _operation = layerfs_machine::force_stop_lock(home, name.as_str())?;
    let record = layerfs_machine::machine_of(home, name.as_str())?;
    let paths = HostPaths::new(home)?;
    if record.recorded_state() == Some("stopped") && paths.fits() {
        // A stop that reported holders may still have a draining worker.
        // Wait for its lifetime lock before the new start sends anything.
        let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while paths.try_hold()?.is_none() {
            if std::time::Instant::now() >= until {
                return Err(refused(
                    "Unavailable",
                    "the previous host worker is still draining",
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    let enabled = set(home, name, false)?;
    let was_ready = paths.control.exists();
    paths.for_management()?.start()?;
    Ok(layerfs_machine::done(
        name,
        (enabled || !was_ready).then_some("started"),
    ))
}

pub(crate) fn stop(
    home: &Path,
    events: &mut Events<'_>,
    name: &MachineName,
    force: bool,
) -> Outcome {
    let _operation = layerfs_machine::force_stop_lock(home, name.as_str())?;
    if force {
        crate::machine_force::force_record(home, name.as_str())?;
    } else {
        layerfs_machine::machine_of(home, name.as_str())?;
    }
    let changed = set(home, name, true)?;
    let paths = HostPaths::new(home)?;
    // what the drain answered (also when it had begun already): uncertain
    // write outcomes the worker could not record up to then (§8.5), and the
    // admitted writes still running, which the drain lets finish
    let mut drain = Drained::default();
    let drained = if paths.fits()
        && paths
            .control
            .try_exists()
            .map_err(layerfs_home::outcome::failed)?
    {
        match paths
            .for_management()?
            .call_existing(true, "drain", SdkValue::map([]))
        {
            Ok(reply) => {
                if let Some(reply) = &reply {
                    drain = Drained::of(reply);
                }
                reply.is_some()
            }
            Err(error)
                if error.code == "Unavailable"
                    && error.detail.get_text("state") == Some("draining") =>
            {
                true
            }
            // a forced stop goes on without its worker's answer
            Err(_) if force => false,
            Err(error) => return Err(error),
        }
    } else {
        false
    };
    let forced = if force {
        Some(crate::machine_force::host_locked(home, events, name)?)
    } else {
        None
    };
    // from here the stop is recorded and a forced one has torn down: a
    // failure keeps the forced report, and a Store that cannot be closed or
    // read is a holder naming why, not a bare error
    let with_forced = |mut error: SdkRefusal| {
        if let Some(outcome) = &forced {
            error.detail = with_field(error.detail, "forced_outcome", outcome.clone());
        }
        error
    };
    let mut blocking = Vec::new();
    for sandbox in crate::inventory::sandboxes(home).map_err(with_forced)? {
        if sandbox.machine() == name.as_str() && !sandbox.machine_stopped() {
            blocking.push(format!("Sandbox {}", sandbox.name()));
        }
    }
    let stores = crate::inventory::stores_on(home, name.as_str()).map_err(with_forced)?;
    // this process's own idle copies close first, as a move's settle does
    // (Store I/O, so before any record lock): only other processes and a
    // call running here hold the machine; a copy in use stays a holder
    for store in &stores {
        let file = layerfs_home::home::paths::store_file(home, store);
        match layerfs_store::open::close(&file) {
            Err(error) if error.code != "InUse" => {
                blocking.push(format!("Store {store} (close failed: {})", error.message));
            }
            _ => {}
        }
    }
    for store in stores {
        if let Err(error) = store_holders(home, name, &store, &mut blocking) {
            blocking.push(format!(
                "Store {store} (holders not read: {})",
                error.message
            ));
        }
    }
    if !blocking.is_empty() {
        let mut error = refused(
            "InUse",
            format!(
                "host machine {name} is stopped; holders: {}{}",
                blocking.join(", "),
                match drain.running {
                    0 => String::new(),
                    _ => format!("; {}", drain.note()),
                }
            ),
        );
        error.detail = with_field(error.detail, "holders", texts(blocking));
        if let Some(outcome) = forced {
            error.detail = with_field(error.detail, "forced_outcome", outcome);
        }
        error.detail = drain.add(error.detail);
        return Err(error);
    }
    if let Some(outcome) = forced {
        return Ok(drain.add(outcome));
    }
    Ok(drain.add(layerfs_machine::with_stopped(
        layerfs_machine::done(name, (changed || drained).then_some("stopped")),
        false,
        &[],
        &[],
    )))
}
/// The holders of host Store `store` on machine `name`: this process's
/// lease and every other live one.
fn store_holders(
    home: &Path,
    name: &MachineName,
    store: &str,
    blocking: &mut Vec<String>,
) -> Result<(), SdkRefusal> {
    let _guard = layerfs_store::record::lock(home, store)?;
    let record = layerfs_store::record::StoreRecord::of(home, store)?;
    if !matches!(record.store_ref()?,layerfs_store::record::StoreRef::Sqlite{machine,..} if machine==name.as_str())
    {
        return Ok(());
    }
    if layerfs_store::lease::held_here(home, store) {
        blocking.push(format!("Store {store} (pid {})", std::process::id()));
    }
    for pid in layerfs_store::lease::holders(home, store)? {
        blocking.push(format!("Store {store} (pid {pid})"));
    }
    Ok(())
}

/// What the host worker's drain answered.
#[derive(Default)]
struct Drained {
    /// Uncertain write outcomes it could not record, up to the drain:
    /// listed (the first 32) and counted, when there are any.
    unrecorded: Option<(SdkValue, i64)>,
    /// Admitted writes still running, which finish after the drain.
    running: i64,
}

impl Drained {
    fn of(reply: &SdkValue) -> Self {
        let count = reply.get_int("unrecorded_count").filter(|count| *count > 0);
        Self {
            unrecorded: count.map(|count| {
                let listed = reply.get("unrecorded").cloned().unwrap_or_default();
                (listed, count)
            }),
            running: reply.get_int("writes_running").unwrap_or(0),
        }
    }

    /// The running writes in words.
    fn note(&self) -> String {
        match self.running {
            1 => "1 write still running; see worker.log".to_owned(),
            n => format!("{n} writes still running; see worker.log"),
        }
    }

    /// `value` with that account, when there is one. A running write may
    /// still produce an outcome no record holds: it goes to `worker.log`.
    fn add(&self, mut value: SdkValue) -> SdkValue {
        if let Some((listed, count)) = &self.unrecorded {
            value = with_field(value, "unrecorded_writes", listed.clone());
            value = with_field(value, "unrecorded_writes_count", SdkValue::Int(*count));
        }
        if self.running > 0 {
            value = with_field(value, "running_writes", SdkValue::Int(self.running));
            value = with_field(value, "running_writes_note", SdkValue::Text(self.note()));
        }
        value
    }
}
fn texts(items: Vec<String>) -> SdkValue {
    SdkValue::List(items.into_iter().map(SdkValue::Text).collect())
}
