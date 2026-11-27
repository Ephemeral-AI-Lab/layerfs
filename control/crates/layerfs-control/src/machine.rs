//! The machine calls whose answers or checks read a machine's Stores and
//! Sandboxes (layerfs-project#104): each runs `layerfs-machine`'s call with
//! those reads from [`crate::inventory`]. The other machine calls are
//! `layerfs-machine`'s own.
use crate::inventory;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::names::MachineName;
use layerfs_home::outcome::Outcome;
use layerfs_machine::worker::Endpoint;
use layerfs_machine::{MachineCreate, MachineUpdate, Related};
use std::path::Path;
use std::time::Duration;

/// What a machine holds, read from the Store and Sandbox records.
pub const RELATED: Related = Related {
    stores_on: inventory::stores_on,
    sandboxes_on: inventory::sandboxes_on,
    hosted_on: inventory::hosted_on,
    can_remove: inventory::can_remove_machine,
    can_stop: inventory::can_stop_machine,
    can_change: inventory::can_change_machine,
};

/// `refusal` with the chain of what is on its machine as its fix, when its
/// fix is the bare `layerfs machine remove <m>` of a machine whose record
/// has no `vm_name` (UI2-8; RC-B F3): a Sandbox or machine call refused
/// `Failed`, or a Store call refused `Unavailable` (LF10), before a
/// nameless machine's worker is dialled. The holders are read here, on the
/// refusal path alone; any other refusal is kept.
pub fn nameless_chain(home: &Path, refusal: SdkRefusal) -> SdkRefusal {
    let named = (refusal.detail.get_text(layerfs_home::outcome::FIX))
        .and_then(|fix| fix.strip_prefix("layerfs machine remove "))
        .filter(|name| layerfs_home::names::MachineName::parse(name).is_ok())
        .map(str::to_owned);
    match named {
        Some(name) if matches!(refusal.code.as_str(), "Failed" | "Unavailable") => {
            RELATED.nameless_fix(home, &name, refusal)
        }
        _ => refusal,
    }
}

/// `machine.list`: from the records; with `live`, every machine probed.
pub fn list(home: &Path, live: bool) -> Outcome {
    layerfs_machine::list(home, live, RELATED)
}

/// `machine.get`: from its record; with `live`, one probe and the `live`
/// section.
pub fn get(home: &Path, name: &MachineName, live: bool) -> Outcome {
    let answer = layerfs_machine::get(home, name, live, RELATED)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if live
        && name.as_str() == layerfs_home::names::HOST_MACHINE
        && answer.get_text("platform") == Some("host")
    {
        return Ok(crate::doctor::host_worker::live(home, answer));
    }
    Ok(answer)
}

/// `machine.start`, with its phases on `events`. The start stops a previous
/// worker and confirms it gone (the machine crate does), persists the ordered
/// incarnation in the home `MachineRecord` before the worker serves, and then
/// settles the intents a previous incarnation left in each Store on that
/// machine (spec §3.1). A start that cannot confirm the old worker or record
/// the token fails before any settle; a Store the reconcile cannot reach is
/// left for the next start.
pub fn start(home: &Path, events: &mut Events<'_>, name: &MachineName) -> Outcome {
    #[cfg(unix)]
    if name.as_str() == layerfs_home::names::HOST_MACHINE {
        return crate::host_power::start(home, name);
    }
    let started = layerfs_machine::start(home, name, RELATED, events)?;
    let incarnation = record_start(home, name)?;
    let _ = reconcile_stores(home, name, incarnation.as_deref());
    Ok(started)
}

/// The ordered incarnation of the start that just happened: the kernel boot
/// on `native` (stable across starts on one boot, changed by a reboot), or the
/// value the machine start persisted in the home `MachineRecord` before it
/// launched the worker. Native records it here; a worker's start records it
/// before serving and fails if it cannot, so this only reads it back. No
/// record means no token and no reconcile: an older worker's intents stay
/// conservative live rather than being settled by a guessed value.
fn record_start(home: &Path, machine: &MachineName) -> Result<Option<String>, SdkRefusal> {
    if layerfs_machine::runs_here(home, machine.as_str())? {
        let Some(incarnation) =
            layerfs_home::process::me().map(|(_, boot)| format!("boot:{boot:032x}"))
        else {
            return Ok(None);
        };
        let _lock = layerfs_home::home::lock::take(home)?;
        let record = layerfs_machine::record::MachineRecord::of(home, machine.as_str())?;
        record.with_incarnation(&incarnation).write(home)?;
        return Ok(Some(incarnation));
    }
    let record = layerfs_machine::record::MachineRecord::of(home, machine.as_str())?;
    let Some(recorded) = record.incarnation().map(str::to_owned) else {
        return Ok(None);
    };
    // Reconcile only with the token the worker is actually serving: a home
    // restore can bring back an older record than the running worker, and a
    // reconcile over that older value must not settle a live owner's intents.
    match worker_incarnation(home, machine.as_str())? {
        Some(reported) if reported == recorded => Ok(Some(recorded)),
        _ => Ok(None),
    }
}

/// The incarnation the machine's running worker reports, or `None` when its
/// calls run in this process (the host machine has no machine worker). Reads the
/// endpoint the start recorded and asks it once; it never starts a machine, so
/// a failed read cannot restart one.
fn worker_incarnation(home: &Path, machine: &str) -> Result<Option<String>, SdkRefusal> {
    if layerfs_machine::runs_here(home, machine)? {
        return Ok(None);
    }
    let Some(endpoint) = layerfs_machine::worker_endpoint(home, machine)? else {
        return Ok(None);
    };
    // A bounded read, not a retry: the machine is already up, so a worker
    // that does not answer now leaves the reconcile for the next start.
    let budget = layerfs_machine::worker::Budget {
        connect: Duration::from_millis(500),
        hello: Duration::from_millis(500),
        answer: Duration::from_secs(1),
    };
    let request = layerfs_machine::worker::request("worker.ping", SdkValue::Null)?;
    let answer =
        match layerfs_machine::worker::send_within(&endpoint, "worker.ping", &request, budget) {
            Ok(Ok(answer)) => answer,
            _ => return Ok(None),
        };
    Ok(answer.get_text("incarnation").map(str::to_owned))
}

/// Settles the mount intents a previous worker incarnation left in each Store
/// on `machine`, with the incarnation this start recorded. Never a live or
/// uncertain owner: the Store-side reconcile applies the same rule admission
/// does. A start that could not read or verify the incarnation skips the
/// reconcile.
fn reconcile_stores(
    home: &Path,
    machine: &MachineName,
    incarnation: Option<&str>,
) -> Result<(), SdkRefusal> {
    let Some(incarnation) = incarnation else {
        return Ok(());
    };
    let stores = inventory::stores_on(home, machine.as_str())?;
    for store in stores {
        let _ =
            layerfs_store::service::reconcile_machine(home, &store, machine.as_str(), incarnation);
    }
    Ok(())
}

/// The worker of machine `name`, started if it does not answer; `None`
/// when its calls run in this process.
pub fn running(home: &Path, name: &str) -> Result<Option<Endpoint>, SdkRefusal> {
    layerfs_machine::running(home, name, RELATED)
}

/// `machine.stop`, with its phases on `events`; `force` stops it and this
/// home's Sandboxes on it without asking its worker.
pub fn stop(home: &Path, events: &mut Events<'_>, name: &MachineName, force: bool) -> Outcome {
    #[cfg(unix)]
    if name.as_str() == layerfs_home::names::HOST_MACHINE {
        return crate::host_power::stop(home, events, name, force);
    }
    match force {
        true => crate::machine_force::stop(home, events, name),
        false => layerfs_machine::stop(home, name, RELATED, events),
    }
}

/// `machine.create`.
pub fn create(home: &Path, events: &mut Events<'_>, request: &MachineCreate) -> Outcome {
    layerfs_machine::create(home, events, request, RELATED)
}

/// `machine.remove`. A removed machine's attachment to each PostgreSQL
/// Store is forgotten with it, so a later machine of its name is attached
/// to none, and so are the home's image copy locks of it
/// (`run/image-lock/<machine>/`).
pub fn remove(home: &Path, events: &mut Events<'_>, name: &MachineName) -> Outcome {
    let removed = layerfs_machine::remove(home, name, RELATED, events)?;
    layerfs_store::postgresql::check::forget_machine(home, name).map_err(|refusal| {
        layerfs_home::outcome::refused(
            &refusal.code,
            format!(
                "machine {name} was removed, but its PostgreSQL Store attachments were not forgotten: {}",
                refusal.message
            ),
        )
    })?;
    // the home's image copy locks of the machine go with it
    let locks = layerfs_sandbox::image_lock::ImageLock::dir(home, name.as_str());
    match std::fs::remove_dir_all(&locks) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(layerfs_home::outcome::refused(
                "Failed",
                format!(
                    "machine {name} was removed, but its image copy locks in {} were not: {error}",
                    locks.display()
                ),
            ))
        }
        _ => {}
    }
    Ok(removed)
}

/// `machine.update`.
pub fn update(home: &Path, request: &MachineUpdate) -> Outcome {
    layerfs_machine::update(home, request, RELATED)
}
