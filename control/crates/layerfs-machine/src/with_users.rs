//! The machine calls whose answers or checks read a machine's Stores and
//! Sandboxes (layerfs-project#104): each runs this crate's call with the
//! reads and steps its caller passes in ([`MachineUsers`], which the Sandbox
//! crate implements). The other machine calls are this crate's own.
use crate::route::Endpoint;
use crate::{MachineCreate, MachineUpdate, MachineUsers};
use layerfs_home::names::MachineName;
use layerfs_home::reply::events::Events;
use layerfs_home::reply::outcome::Outcome;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::Path;
use std::time::Duration;

/// `refusal` with the chain of what is on its machine as its fix, when its
/// fix is the bare `layerfs machine remove <m>` of a machine whose record
/// has no `vm_name` (UI2-8; RC-B F3): a Sandbox or machine call refused
/// `Failed`, or a Store call refused `Unavailable` (LF10), before a
/// nameless machine's worker is dialled. The holders are read here, on the
/// refusal path alone; any other refusal is kept.
pub fn nameless_chain(home: &Path, refusal: SdkRefusal, users: &MachineUsers) -> SdkRefusal {
    let named = (refusal.detail.get_text(layerfs_home::reply::outcome::FIX))
        .and_then(|fix| fix.strip_prefix("layerfs machine remove "))
        .filter(|name| layerfs_home::names::MachineName::parse(name).is_ok())
        .map(str::to_owned);
    match named {
        Some(name) if matches!(refusal.code.as_str(), "Failed" | "Unavailable") => {
            users.related.nameless_fix(home, &name, refusal)
        }
        _ => refusal,
    }
}

/// `machine.list`: from the records; with `live`, every machine probed.
pub fn list(home: &Path, live: bool, users: &MachineUsers) -> Outcome {
    crate::status::list(home, live, users.related)
}

/// `machine.get`: from its record; with `live`, one probe and the `live`
/// section.
pub fn get(home: &Path, name: &MachineName, live: bool, users: &MachineUsers) -> Outcome {
    let answer = crate::get(home, name, live, users.related)?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if live
        && name.as_str() == layerfs_home::names::HOST_MACHINE
        && answer.get_text("platform") == Some("host")
    {
        return Ok(crate::status::worker::live(home, answer));
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
pub fn start(
    home: &Path,
    events: &mut Events<'_>,
    name: &MachineName,
    users: &MachineUsers,
) -> Outcome {
    #[cfg(unix)]
    if name.as_str() == layerfs_home::names::HOST_MACHINE {
        return crate::lifecycle::host::start(home, name);
    }
    let started = crate::start(home, name, users.related, events)?;
    let incarnation = record_start(home, name)?;
    // A Store the reconcile cannot reach is left for the next start.
    if let Some(incarnation) = incarnation {
        let _ = (users.reconcile)(home, name.as_str(), &incarnation);
    }
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
    if crate::runs_here(home, machine.as_str())? {
        let Some(incarnation) =
            layerfs_home::record::process::me().map(|(_, boot)| format!("boot:{boot:032x}"))
        else {
            return Ok(None);
        };
        let _lock = layerfs_home::lock::home::take(home)?;
        let record = crate::record::machine::MachineRecord::of(home, machine.as_str())?;
        record.with_incarnation(&incarnation).write(home)?;
        return Ok(Some(incarnation));
    }
    let record = crate::record::machine::MachineRecord::of(home, machine.as_str())?;
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
    if crate::runs_here(home, machine)? {
        return Ok(None);
    }
    let Some(endpoint) = crate::worker_endpoint(home, machine)? else {
        return Ok(None);
    };
    // A bounded read, not a retry: the machine is already up, so a worker
    // that does not answer now leaves the reconcile for the next start.
    let budget = crate::route::Budget {
        connect: Duration::from_millis(500),
        hello: Duration::from_millis(500),
        answer: Duration::from_secs(1),
    };
    let request = crate::route::request("worker.ping", SdkValue::Null)?;
    let answer = match crate::route::send_within(&endpoint, "worker.ping", &request, budget) {
        Ok(Ok(answer)) => answer,
        _ => return Ok(None),
    };
    Ok(answer.get_text("incarnation").map(str::to_owned))
}

/// The worker of machine `name`, started if it does not answer; `None`
/// when its calls run in this process.
pub fn running(
    home: &Path,
    name: &str,
    users: &MachineUsers,
) -> Result<Option<Endpoint>, SdkRefusal> {
    crate::running(home, name, users.related)
}

/// `machine.stop`, with its phases on `events`; `force` stops it and this
/// home's Sandboxes on it without asking its worker.
pub fn stop(
    home: &Path,
    events: &mut Events<'_>,
    name: &MachineName,
    force: bool,
    users: &MachineUsers,
) -> Outcome {
    #[cfg(unix)]
    if name.as_str() == layerfs_home::names::HOST_MACHINE {
        return crate::lifecycle::host::stop(home, events, name, force, users);
    }
    match force {
        true => crate::lifecycle::force_stop::stop(home, events, name, users),
        false => crate::stop(home, name, users.related, events),
    }
}

/// `machine.create`.
pub fn create(
    home: &Path,
    events: &mut Events<'_>,
    request: &MachineCreate,
    users: &MachineUsers,
) -> Outcome {
    crate::lifecycle::create(home, events, request, users.related)
}

/// `machine.remove`. A removed machine's attachment to each PostgreSQL
/// Store is forgotten with it, so a later machine of its name is attached
/// to none, and so are the home's image copy locks of it
/// (`run/image-lock/<machine>/`).
pub fn remove(
    home: &Path,
    events: &mut Events<'_>,
    name: &MachineName,
    users: &MachineUsers,
) -> Outcome {
    let removed = crate::lifecycle::remove(home, name, users.related, events)?;
    (users.forget)(home, name.as_str())?;
    Ok(removed)
}

/// `machine.update`.
pub fn update(home: &Path, request: &MachineUpdate, users: &MachineUsers) -> Outcome {
    crate::update(home, request, users.related)
}
