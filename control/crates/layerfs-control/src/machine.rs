//! The machine calls whose answers or checks read a machine's Stores and
//! Sandboxes (layerfs-project#104): each runs `layerfs-machine`'s call with
//! those reads from [`crate::inventory`]. The other machine calls are
//! `layerfs-machine`'s own.
use crate::inventory;
use layerfs_bridge::control::SdkRefusal;
use layerfs_home::events::Events;
use layerfs_home::names::MachineName;
use layerfs_home::outcome::Outcome;
use layerfs_machine::worker::Endpoint;
use layerfs_machine::{MachineCreate, MachineUpdate, Related};
use std::path::Path;

/// What a machine holds, read from the Store and Sandbox records.
pub const RELATED: Related = Related {
    stores_on: inventory::stores_on,
    sandboxes_on: inventory::sandboxes_on,
    hosted_on: inventory::hosted_on,
    can_remove: inventory::can_remove_machine,
    can_stop: inventory::can_stop_machine,
    can_change: inventory::can_change_machine,
};

/// `machine.list`: from the records; with `live`, every machine probed.
pub fn list(home: &Path, live: bool) -> Outcome {
    layerfs_machine::list(home, live, RELATED)
}

/// `machine.get`: from its record; with `live`, one probe and the `live`
/// section.
pub fn get(home: &Path, name: &MachineName, live: bool) -> Outcome {
    layerfs_machine::get(home, name, live, RELATED)
}

/// `machine.start`, with its phases on `events`.
pub fn start(home: &Path, events: &mut Events<'_>, name: &MachineName) -> Outcome {
    layerfs_machine::start(home, name, RELATED, events)
}

/// The worker of machine `name`, started if it does not answer; `None`
/// when its calls run in this process.
pub fn running(home: &Path, name: &str) -> Result<Option<Endpoint>, SdkRefusal> {
    layerfs_machine::running(home, name, RELATED)
}

/// `machine.stop`, with its phases on `events`; `force` stops it and this
/// home's Sandboxes on it without asking its worker.
pub fn stop(home: &Path, events: &mut Events<'_>, name: &MachineName, force: bool) -> Outcome {
    match force {
        true => crate::machine_force::stop(home, events, name),
        false => layerfs_machine::stop(home, name, RELATED, events),
    }
}

/// `machine.create`.
pub fn create(home: &Path, events: &mut Events<'_>, request: &MachineCreate) -> Outcome {
    layerfs_machine::create(home, events, request, RELATED)
}

/// `machine.remove`. A removed machine's own connection to each
/// PostgreSQL Store is forgotten with it, so a later machine of its name
/// dials the Store's, and so are the home's image copy locks of it
/// (`run/image-lock/<machine>/`).
pub fn remove(home: &Path, events: &mut Events<'_>, name: &MachineName) -> Outcome {
    let removed = layerfs_machine::remove(home, name, RELATED, events)?;
    layerfs_store::postgresql::check::forget_machine(home, name).map_err(|refusal| {
        layerfs_home::outcome::refused(
            &refusal.code,
            format!(
                "machine {name} was removed, but its own PostgreSQL connection was not forgotten: {}",
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
