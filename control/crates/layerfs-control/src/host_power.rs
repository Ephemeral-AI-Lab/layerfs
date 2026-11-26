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
            Ok(reply) => reply.is_some(),
            Err(error)
                if error.code == "Unavailable"
                    && error.detail.get_text("state") == Some("draining") =>
            {
                true
            }
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
    let mut blocking = Vec::new();
    for sandbox in crate::inventory::sandboxes(home)? {
        if sandbox.machine() == name.as_str() && !sandbox.machine_stopped() {
            blocking.push(format!("Sandbox {}", sandbox.name()));
        }
    }
    let stores = crate::inventory::stores_on(home, name.as_str())?;
    // this process's own idle copies close first, as a move's settle does
    // (Store I/O, so before any record lock): only other processes and a
    // call running here hold the machine; a copy in use stays a holder
    for store in &stores {
        let file = layerfs_home::home::paths::store_file(home, store);
        match layerfs_store::open::close(&file) {
            Err(error) if error.code != "InUse" => return Err(error),
            _ => {}
        }
    }
    for store in stores {
        let _guard = layerfs_store::record::lock(home, &store)?;
        let record = layerfs_store::record::StoreRecord::of(home, &store)?;
        if !matches!(record.store_ref()?,layerfs_store::record::StoreRef::Sqlite{machine,..} if machine==name.as_str())
        {
            continue;
        }
        if layerfs_store::lease::held_here(home, &store) {
            blocking.push(format!("Store {store} (pid {})", std::process::id()));
        }
        for pid in layerfs_store::lease::holders(home, &store)? {
            blocking.push(format!("Store {store} (pid {pid})"));
        }
    }
    if !blocking.is_empty() {
        let mut error = refused(
            "InUse",
            format!(
                "host machine {name} is stopped; holders: {}",
                blocking.join(", ")
            ),
        );
        error.detail = with_field(error.detail, "holders", texts(blocking));
        if let Some(outcome) = forced {
            error.detail = with_field(error.detail, "forced_outcome", outcome);
        }
        return Err(error);
    }
    if let Some(outcome) = forced {
        return Ok(outcome);
    }
    Ok(layerfs_machine::with_stopped(
        layerfs_machine::done(name, (changed || drained).then_some("stopped")),
        false,
        &[],
        &[],
    ))
}
fn texts(items: Vec<String>) -> SdkValue {
    SdkValue::List(items.into_iter().map(SdkValue::Text).collect())
}
